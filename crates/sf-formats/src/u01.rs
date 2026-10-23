//! Barudan U01.
//!
//! A 0x100-byte header (written as zeros; the reader skips it), then 3-byte
//! records: a control byte, then the y and x move magnitudes (0.1 mm, +y up,
//! at most 127). Control byte: bit 6 makes y negative, bit 5 makes x negative,
//! bits 0–4 hold the function and bit 7 is set on every record we write. A move of zero has
//! its sign bit set too, as in Barudan's files (pyembroidery's notes: a stop is `E8`), so
//! trims, stops and needle changes are `E7`, `E8` and `E9`–`F7`:
//!
//! | function | meaning |
//! |---|---|
//! | 0x00 | stitch |
//! | 0x01 | jump |
//! | 0x07 | trim |
//! | 0x08 | stop |
//! | 0x09–0x17 | change to needle 1–15 (a colour change) |
//! | 0x18 | end (written as `F8 00 00`) |
//!
//! Colour changes go to the next needle, cycling through 1–15 (needle 1 sews
//! the first block). U01 stores no colours: the reader colours the blocks from
//! a generic palette.
//!
//! Uncertain: the header contents and the meaning of bit 7; readers ignore
//! both, and unknown functions are read as jumps so positions stay right.

use crate::dst::Extents;
use crate::plan::{Decoder, Plan, PlanCfg, build_plan};
use crate::{FormatError, WriteOptions};
use sf_core::{Cmd, Pattern};

const HEADER: usize = 0x100;
const MAX_STEP: i64 = 127;
const STITCH: u8 = 0x00;
const JUMP: u8 = 0x01;
const TRIM: u8 = 0x07;
const STOP: u8 = 0x08;
const NEEDLE_1: u8 = 0x09;
const NEEDLE_15: u8 = 0x17;
const END: u8 = 0x18;
const Y_NEG: u8 = 0x40;
const X_NEG: u8 = 0x20;
const VALID: u8 = 0x80;

pub(crate) fn plan(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg), FormatError> {
    let cfg = PlanCfg { native_stop: true, ..PlanCfg::tenth_mm(true, MAX_STEP, MAX_STEP, opts.center) };
    let mut plan = build_plan(pattern, &cfg.with(opts))?;
    plan.use_generic_threads();
    Ok((plan, cfg))
}

fn record(func: u8, dx: i64, dy: i64) -> [u8; 3] {
    // a move of zero carries the sign bit, as in Barudan's own files: a trim, stop or needle
    // change is `E7`, `E8`, `E9`…, which readers of DSB look for exactly
    let mut ctrl = VALID | func;
    if dx <= 0 {
        ctrl |= X_NEG;
    }
    if dy <= 0 {
        ctrl |= Y_NEG;
    }
    [ctrl, dy.unsigned_abs().min(255) as u8, dx.unsigned_abs().min(255) as u8]
}

pub(crate) fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, FormatError> {
    let (plan, _) = plan(pattern, opts)?;
    let mut out = vec![0u8; HEADER];
    out.extend_from_slice(&records(&plan).0);
    Ok(out)
}

/// The records of a plan (ending with its end record) and what a DST-style header counts of
/// them (Barudan DSB files hold these records after a DST header).
pub(crate) fn records(plan: &Plan) -> (Vec<u8>, Extents) {
    let mut out = Vec::with_capacity(plan.recs.len() * 3);
    let mut e = Extents { records: 0, changes: 0, min: (0, 0), max: (0, 0), end: (0, 0) };
    let mut cur = (0i64, 0i64);
    let mut needle = 1u8;
    for r in &plan.recs {
        let (dx, dy) = (r.x - cur.0, r.y - cur.1);
        let rec = match r.cmd {
            Cmd::Stitch => record(STITCH, dx, dy),
            Cmd::Jump => record(JUMP, dx, dy),
            Cmd::Trim => record(TRIM, 0, 0),
            Cmd::Stop => record(STOP, 0, 0),
            Cmd::ColorChange => {
                needle = needle % 15 + 1;
                e.changes += 1;
                record(NEEDLE_1 + needle - 1, 0, 0)
            }
            Cmd::End => [0xF8, 0, 0],
        };
        out.extend_from_slice(&rec);
        cur = (r.x, r.y);
        e.records += 1;
        e.min = (e.min.0.min(cur.0), e.min.1.min(cur.1));
        e.max = (e.max.0.max(cur.0), e.max.1.max(cur.1));
    }
    e.end = cur;
    (out, e)
}

/// The move of a record.
fn delta(b: &[u8; 3]) -> (i64, i64) {
    let (dy, dx) = (i64::from(b[1]), i64::from(b[2]));
    (if b[0] & X_NEG != 0 { -dx } else { dx }, if b[0] & Y_NEG != 0 { -dy } else { dy })
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    read_records(bytes.get(HEADER..).ok_or(FormatError::Truncated)?, String::new())
}

/// Decodes U01 records (up to the end record) into a pattern named `name`.
pub(crate) fn read_records(body: &[u8], name: String) -> Result<Pattern, FormatError> {
    let mut dec = Decoder::new(10.0, true);
    for chunk in body.chunks_exact(3) {
        let Ok(b) = <[u8; 3]>::try_from(chunk) else { break };
        let (dx, dy) = delta(&b);
        match b[0] & 0x1F {
            STITCH => dec.stitch(dx, dy)?,
            JUMP => dec.jump(dx, dy)?,
            END => break,
            TRIM => dec.command(Cmd::Trim)?,
            STOP => dec.command(Cmd::Stop)?,
            NEEDLE_1..=NEEDLE_15 => dec.command(Cmd::ColorChange)?,
            _ => {
                if (dx, dy) != (0, 0) {
                    dec.jump(dx, dy)?;
                }
            }
        }
    }
    dec.finish(Vec::new(), name)
}

/// Whole records after the header, known functions only, ending with the end record.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    bytes.get(HEADER..).is_some_and(sniff_records)
}

/// Whole records, known functions only, ending with the end record.
pub(crate) fn sniff_records(body: &[u8]) -> bool {
    if body.len() < 6 || !body.len().is_multiple_of(3) {
        return false;
    }
    let mut records = body.chunks_exact(3).map(|c| c.first().copied().unwrap_or(0xFF) & 0x1F);
    let last = records.next_back();
    last == Some(END) && records.all(|f| matches!(f, STITCH | JUMP | TRIM | STOP | NEEDLE_1..=NEEDLE_15))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_round_trip() {
        for (dx, dy) in [(0, 0), (127, -127), (-5, 9), (-127, 0)] {
            assert_eq!(delta(&record(STITCH, dx, dy)), (dx, dy));
        }
        assert_eq!(record(JUMP, 3, -4), [0x80 | 0x40 | 0x01, 4, 3]);
        // zero moves carry the sign bits, as Barudan's files do: E7 trim, E8 stop, E9 needle 1
        assert_eq!((record(TRIM, 0, 0), record(STOP, 0, 0), record(NEEDLE_1, 0, 0)), ([0xE7, 0, 0], [0xE8, 0, 0], [0xE9, 0, 0]));
    }
}
