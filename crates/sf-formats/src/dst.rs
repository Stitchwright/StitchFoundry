//! Tajima DST.
//!
//! * A 512-byte ASCII header. Each field ends with CR (0x0D): `LA:` label
//!   (16 characters, space padded), `ST:` record count (7 digits), `CO:` colour
//!   changes (3 digits), `+X:` `-X:` `+Y:` `-Y:` extents from the start point
//!   (5 digits, 0.1 mm), `AX:` `AY:` end point relative to the start (sign and
//!   5 characters), `MX:` `MY:` (end of a previous volume, always `+    0`) and
//!   `PD:******`; then 0x1A and spaces up to byte 512.
//! * 3-byte records holding the move as balanced ternary digits in 0.1 mm,
//!   +y up, at most ±121 per axis:
//!
//!   | byte | bit 7 | bit 6 | bit 5 | bit 4 | bit 3 | bit 2 | bit 1 | bit 0 |
//!   |---|---|---|---|---|---|---|---|---|
//!   | 1 | y+1 | y−1 | y+9 | y−9 | x−9 | x+9 | x−1 | x+1 |
//!   | 2 | y+3 | y−3 | y+27 | y−27 | x−27 | x+27 | x−3 | x+3 |
//!   | 3 | jump | colour change | y+81 | y−81 | x−81 | x+81 | 1 | 1 |
//!
//!   A stitch has `0x03` in byte 3, a jump `0x83`, a colour change or stop
//!   `0xC3` (the same code: a DST [`Cmd::Stop`] reads back as a colour change)
//!   and the end is `00 00 F3`.
//!
//! **Trims.** DST has no trim command: machines cut when they meet a run of
//! jumps. A trim is written as three small jumps that cancel out (+0.2/+0.2,
//! −0.4/−0.4, +0.2/+0.2 mm) and the reader turns every run of three or more
//! jumps into a trim followed by the run's jumps, dropping the first three when
//! they cancel out (the marker). So that this reading gives back what was
//! written, the writer puts a trim before every run of three or more jumps (a
//! move longer than 24.2 mm) that has none: the machine cuts there anyway.
//!
//! Uncertain: whether `ST:` counts the end record (we count every record,
//! end included); readers ignore the header numbers.

use crate::bytes::{ascii_label, read_label};
use crate::plan::{Decoder, Plan, PlanCfg, Rec, build_plan};
use crate::{Format, FormatError, WriteOptions};
use sf_core::{Cmd, Pattern};

const HEADER: usize = 512;
/// Largest move per axis per record (0.1 mm).
const MAX_STEP: i64 = 121;
/// The trim marker: three jumps that cancel out.
const TRIM_JUMPS: [(i64, i64); 3] = [(2, 2), (-4, -4), (2, 2)];
/// (weight, byte, bit for +weight, bit for −weight), lowest weight first.
const X_BITS: [(i64, usize, u8, u8); 5] = [(1, 0, 0x01, 0x02), (3, 1, 0x01, 0x02), (9, 0, 0x04, 0x08), (27, 1, 0x04, 0x08), (81, 2, 0x04, 0x08)];
const Y_BITS: [(i64, usize, u8, u8); 5] = [(1, 0, 0x80, 0x40), (3, 1, 0x80, 0x40), (9, 0, 0x20, 0x10), (27, 1, 0x20, 0x10), (81, 2, 0x20, 0x10)];

const STITCH: u8 = 0x03;
const JUMP: u8 = 0x83;
const COLOR_CHANGE: u8 = 0xC3;

pub(crate) fn plan(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg), FormatError> {
    // The header holds 5-digit extents; keep a margin for the trim marker.
    let cfg = PlanCfg { max_coord: 99_000, ..PlanCfg::tenth_mm(true, MAX_STEP, MAX_STEP, opts.center) };
    let mut plan = build_plan(pattern, &cfg.with(opts))?;
    plan.recs = place_trims(&plan.recs);
    plan.use_generic_threads();
    Ok((plan, cfg))
}

/// Puts the trims where DST can express them: a trim is three jumps and the
/// reader looks for it at the start of a run of jumps. So a trim right after
/// jumps moves before them (cutting before the move instead of after it), and
/// every run of three or more jumps, on which machines cut, gets a trim.
fn place_trims(recs: &[Rec]) -> Vec<Rec> {
    let mut moved: Vec<Rec> = Vec::with_capacity(recs.len() + 4);
    for r in recs {
        let start = moved.iter().rposition(|p| p.cmd != Cmd::Jump).map_or(0, |i| i + 1);
        if r.cmd == Cmd::Trim && start < moved.len() {
            let before = start.checked_sub(1).and_then(|i| moved.get(i)).copied();
            if before.map(|b| b.cmd) != Some(Cmd::Trim) {
                let (x, y) = before.map_or((0, 0), |b| (b.x, b.y));
                moved.insert(start, Rec { cmd: Cmd::Trim, x, y });
            }
            continue;
        }
        moved.push(*r);
    }
    let mut out: Vec<Rec> = Vec::with_capacity(moved.len() + 4);
    for group in moved.chunk_by(|a, b| (a.cmd == Cmd::Jump) == (b.cmd == Cmd::Jump)) {
        let is_jump_run = group.len() >= 3 && group.first().is_some_and(|r| r.cmd == Cmd::Jump);
        if is_jump_run && out.last().map(|r| r.cmd) != Some(Cmd::Trim) {
            let (x, y) = out.last().map_or((0, 0), |r| (r.x, r.y));
            out.push(Rec { cmd: Cmd::Trim, x, y });
        }
        out.extend_from_slice(group);
    }
    out
}

/// Balanced ternary digits of `v` (|v| ≤ 121), lowest weight first.
fn ternary(mut v: i64) -> [i64; 5] {
    let mut d = [0; 5];
    for digit in &mut d {
        let r = v.rem_euclid(3);
        *digit = if r == 2 { -1 } else { r };
        v = (v - *digit) / 3;
    }
    d
}

fn encode(dx: i64, dy: i64, flags: u8) -> [u8; 3] {
    let mut b = [0u8, 0, flags];
    for (bits, v) in [(&X_BITS, dx), (&Y_BITS, dy)] {
        for (&(_, byte, plus, minus), digit) in bits.iter().zip(ternary(v)) {
            if let Some(slot) = b.get_mut(byte) {
                *slot |= match digit {
                    1 => plus,
                    -1 => minus,
                    _ => 0,
                };
            }
        }
    }
    b
}

fn decode(b: &[u8; 3]) -> (i64, i64) {
    let axis = |bits: &[(i64, usize, u8, u8); 5]| {
        bits.iter().fold(0, |acc, &(w, byte, plus, minus)| {
            let v = b.get(byte).copied().unwrap_or(0);
            acc + if v & plus != 0 { w } else { 0 } - if v & minus != 0 { w } else { 0 }
        })
    };
    (axis(&X_BITS), axis(&Y_BITS))
}

/// Accumulates records and the extents they visit.
struct Body {
    bytes: Vec<u8>,
    x: i64,
    y: i64,
    min: (i64, i64),
    max: (i64, i64),
    records: usize,
}

impl Body {
    fn record(&mut self, dx: i64, dy: i64, flags: u8) {
        self.bytes.extend_from_slice(&encode(dx, dy, flags));
        self.x += dx;
        self.y += dy;
        self.min = (self.min.0.min(self.x), self.min.1.min(self.y));
        self.max = (self.max.0.max(self.x), self.max.1.max(self.y));
        self.records += 1;
    }
}

pub(crate) fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, FormatError> {
    let (plan, _) = plan(pattern, opts)?;
    let mut body = Body { bytes: Vec::with_capacity(plan.recs.len() * 3 + 16), x: 0, y: 0, min: (0, 0), max: (0, 0), records: 0 };
    let mut changes = 0usize;
    for r in &plan.recs {
        let (dx, dy) = (r.x - body.x, r.y - body.y);
        match r.cmd {
            Cmd::Stitch => body.record(dx, dy, STITCH),
            Cmd::Jump => body.record(dx, dy, JUMP),
            Cmd::Trim => {
                for (tx, ty) in TRIM_JUMPS {
                    body.record(tx, ty, JUMP);
                }
            }
            Cmd::ColorChange | Cmd::Stop => {
                body.record(dx, dy, COLOR_CHANGE);
                changes += 1;
            }
            Cmd::End => {}
        }
    }
    body.bytes.extend_from_slice(&[0x00, 0x00, 0xF3]);
    body.records += 1;
    let mut out = header(opts.label(pattern), &Extents { records: body.records, changes, min: body.min, max: body.max, end: (body.x, body.y) })?;
    out.extend_from_slice(&body.bytes);
    Ok(out)
}

/// What a DST-style header counts (0.1 mm, +y up, from the start point).
pub(crate) struct Extents {
    pub records: usize,
    pub changes: usize,
    pub min: (i64, i64),
    pub max: (i64, i64),
    pub end: (i64, i64),
}

/// The 512-byte DST header (also the header of Barudan DSB files).
pub(crate) fn header(label: &str, e: &Extents) -> Result<Vec<u8>, FormatError> {
    if e.records > 9_999_999 || e.changes > 999 {
        return Err(FormatError::TooLarge);
    }
    let sign = |v: i64| if v < 0 { '-' } else { '+' };
    let label = ascii_label(label, 16);
    let header = format!(
        "LA:{label:<16}\rST:{:>7}\rCO:{:>3}\r+X:{:>5}\r-X:{:>5}\r+Y:{:>5}\r-Y:{:>5}\rAX:{}{:>5}\rAY:{}{:>5}\rMX:+    0\rMY:+    0\rPD:******\r\u{1a}",
        e.records,
        e.changes,
        e.max.0.max(0),
        (-e.min.0).max(0),
        e.max.1.max(0),
        (-e.min.1).max(0),
        sign(e.end.0),
        e.end.0.abs(),
        sign(e.end.1),
        e.end.1.abs(),
    );
    let mut out = header.into_bytes();
    if out.len() > HEADER {
        return Err(FormatError::TooLarge);
    }
    out.resize(HEADER, b' ');
    Ok(out)
}

/// The label of a DST-style header.
pub(crate) fn header_label(bytes: &[u8]) -> String {
    read_label(bytes.get(3..19).unwrap_or_default())
}

/// Emits a run of jumps, as a trim first when it is three or more long.
fn flush_jumps(dec: &mut Decoder, run: &mut Vec<(i64, i64)>) -> Result<(), FormatError> {
    let mut skip = 0;
    if run.len() >= 3 {
        dec.command(Cmd::Trim)?;
        let first: (i64, i64) = run.iter().take(3).fold((0, 0), |a, d| (a.0 + d.0, a.1 + d.1));
        if first == (0, 0) {
            skip = 3;
        }
    }
    for &(dx, dy) in run.iter().skip(skip) {
        dec.jump(dx, dy)?;
    }
    run.clear();
    Ok(())
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    if !bytes.starts_with(b"LA:") {
        return Err(if bytes.len() < 3 { FormatError::Truncated } else { FormatError::BadMagic(Format::Dst) });
    }
    let body = bytes.get(HEADER..).ok_or(FormatError::Truncated)?;
    read_records(body, header_label(bytes))
}

/// Decodes DST stitch records (up to the end record) into a pattern named `name`.
pub(crate) fn read_records(body: &[u8], name: String) -> Result<Pattern, FormatError> {
    let mut dec = Decoder::new(10.0, true);
    let mut run: Vec<(i64, i64)> = Vec::new();
    for b in body.as_chunks::<3>().0.iter().copied() {
        if b[2] & 0xF0 == 0xF0 {
            break;
        }
        let (dx, dy) = decode(&b);
        if b[2] & 0xC0 == 0x80 {
            run.push((dx, dy));
            continue;
        }
        flush_jumps(&mut dec, &mut run)?;
        if b[2] & 0xC0 == 0xC0 {
            dec.command(Cmd::ColorChange)?;
            if (dx, dy) != (0, 0) {
                dec.jump(dx, dy)?;
            }
        } else {
            // Byte 3 = 0x43 marks sequin stitches on some machines: sewn like stitches.
            dec.stitch(dx, dy)?;
        }
    }
    flush_jumps(&mut dec, &mut run)?;
    dec.finish(Vec::new(), name)
}

/// A DST header: `LA:` and an `ST:` field.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    let head = bytes.get(..HEADER).unwrap_or(bytes);
    bytes.starts_with(b"LA:") && head.windows(3).any(|w| w == b"ST:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ternary_round_trips_every_step() {
        for dx in -MAX_STEP..=MAX_STEP {
            for dy in [-121, -64, -1, 0, 1, 40, 121] {
                let b = encode(dx, dy, STITCH);
                assert_eq!(decode(&b), (dx, dy));
                assert_eq!(b[2] & 0x03, 0x03);
                // Never mistaken for the end record.
                assert_ne!(b[2] & 0xF0, 0xF0);
            }
        }
        assert_eq!(encode(0, 0, JUMP), [0, 0, 0x83]);
        assert_eq!(encode(1, 1, STITCH), [0x81, 0, 0x03]);
        assert_eq!(encode(-121, 0, STITCH), [0x0A, 0x0A, 0x0B]);
    }

    #[test]
    fn trims_go_before_jump_runs() {
        use Cmd::*;
        let r = |cmd, x| Rec { cmd, x, y: 0 };
        let cmds = |recs: &[Rec]| place_trims(recs).iter().map(|r| (r.cmd, r.x)).collect::<Vec<_>>();
        // Long runs get a trim; an existing one is kept.
        let recs = [
            r(Stitch, 0),
            r(Jump, 100),
            r(Jump, 200),
            r(Jump, 300),
            r(Stitch, 300),
            r(Trim, 300),
            r(Jump, 400),
            r(Jump, 500),
            r(Jump, 600),
            r(Stitch, 600),
            r(Jump, 700),
        ];
        assert_eq!(
            cmds(&recs),
            vec![
                (Stitch, 0),
                (Trim, 0),
                (Jump, 100),
                (Jump, 200),
                (Jump, 300),
                (Stitch, 300),
                (Trim, 300),
                (Jump, 400),
                (Jump, 500),
                (Jump, 600),
                (Stitch, 600),
                (Jump, 700)
            ]
        );
        // A trim after jumps moves before them.
        let recs = [r(Stitch, 0), r(Jump, 100), r(Trim, 100), r(Stitch, 100), r(Jump, 200), r(Jump, 300), r(Jump, 400), r(Trim, 400), r(End, 400)];
        assert_eq!(
            cmds(&recs),
            vec![(Stitch, 0), (Trim, 0), (Jump, 100), (Stitch, 100), (Trim, 100), (Jump, 200), (Jump, 300), (Jump, 400), (End, 400)]
        );
        // At the very start the trim sits at the origin.
        assert_eq!(cmds(&[r(Jump, 50), r(Trim, 50), r(Stitch, 50)]), vec![(Trim, 0), (Jump, 50), (Stitch, 50)]);
    }
}
