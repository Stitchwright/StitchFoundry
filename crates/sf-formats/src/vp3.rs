//! Pfaff / Husqvarna Viking VP3.
//!
//! Big-endian, nested blocks each introduced by a 3-byte tag and a u32 length
//! of the rest of the block. `str8` is a u16 byte length and UTF-8 bytes,
//! `str16` a u16 byte length and UTF-16BE. Header positions are in **µm with
//! +y up**; stitch data is in **0.1 mm with +y down**.
//!
//! ```text
//! "%vsm%" 00, str16 producer
//! 00 02 00, u32 length                      file block (to the end of the file)
//!   str16 settings (empty), i32 right, top, left, bottom (design extents, µm)
//!   u32 stitch data records but the end, u16 colour blocks, 0C 00, 01 (one design)
//!   00 03 00, u32 length                    design block (to the end of the file)
//!     i32 centre x, centre y (µm), 00 00 00
//!     i32 left, right, bottom, top (extents from the centre), i32 width, height (µm)
//!     str16 settings (empty), 64 64, u32 4096, u32 0, u32 0, u32 4096
//!     "xxPP" 01 00, str16 producer, u16 colour blocks
//!     per colour block:
//!       00 05 00, u32 length
//!       i32 x, y: where the block starts, from the centre (µm)
//!       01 00 RR GG BB 00 00 00 05 28        a solid thread (one colour, weight 40)
//!       str8 catalogue number, str8 name, str8 brand
//!       i32 x, y: from the block's start to its end (µm)
//!       00 01 00, u32 length, 0A F6 00, stitch data
//!       00
//! ```
//!
//! An independent open-source writer, whose files load on Husqvarna Viking machines,
//! and a published description of the format agree on this layout, field by field.
//! The reader also takes what early versions of this crate wrote: colour blocks
//! without the block shift or the `00 01 00` stitch block, and the design name where
//! the design block's settings go (its centred extents, then in another order, are
//! skipped).
//!
//! Stitch data: signed 8-bit dx dy pairs (±127, the x byte never 0x80);
//! `80 01` + i16 dx + i16 dy + `80 02` is a jump (every jump uses this long
//! form), `80 03` a trim. Readers differ on that long form: some take it for a
//! stitch (the format has no jump, a long stitch moves the frame), so each run
//! of jumps is written as one record ending where the next stitch starts, and
//! that stitch stays: the needle goes down only there, however it is read.
//! Every colour change starts a new colour block, which
//! holds its own position and thread (exact RGB, name, catalogue number and
//! brand round-trip); stops become colour blocks with the same thread. The end
//! of the last block ends the design.
//!
//! Uncertain: the constant bytes (`0C 00 01 00 03 00`, `64 64`, 4096, the
//! thread's `00 00 00 05 28`, `0A F6 00`), the exact contents of the design
//! block, and which strings real files store as UTF-8 or UTF-16; the reader
//! finds the colour blocks after the `xxPP` marker, skips what it does not need
//! and decodes strings as UTF-16BE when they look like it.

use crate::bytes::{Cursor, Put, sat_i32};
use crate::exp::{byte, signed};
use crate::plan::{Decoder, Plan, PlanCfg, Rec, build_plan};
use crate::{Format, FormatError, WriteOptions};
use sf_core::{Cmd, Pattern, Rgb, Thread};

const MAX_STITCH: i64 = 127;
const MAX_JUMP: i64 = i16::MAX as i64;
const PRODUCER: &str = "Produced by StitchFoundry";
/// µm per file unit (0.1 mm).
const UM: i64 = 100;
/// Readers refuse more colour blocks than this.
const MAX_BLOCKS: usize = 10_000;

pub(crate) fn plan(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg), FormatError> {
    let cfg = PlanCfg::tenth_mm(false, MAX_STITCH, MAX_JUMP, opts.center);
    let mut plan = build_plan(pattern, &cfg.with(opts))?;
    if plan.threads.len() > usize::from(u16::MAX) {
        return Err(FormatError::TooLarge);
    }
    plan.recs = merge_jumps(plan.recs);
    Ok((plan, cfg))
}

/// Each run of jumps as one jump to where the next stitch starts, or to where the run
/// ends when no stitch follows (more records only when that is further than
/// [`MAX_JUMP`] on an axis). The stitch stays, in place, so the needle goes down there
/// however a reader takes the long form.
fn merge_jumps(recs: Vec<Rec>) -> Vec<Rec> {
    let fits = |from: (i64, i64), to: &Rec| (to.x - from.0).abs() <= MAX_JUMP && (to.y - from.1).abs() <= MAX_JUMP;
    let mut out: Vec<Rec> = Vec::with_capacity(recs.len());
    // where the needle was before the last record of `out` (it starts at the origin)
    let mut before = (0i64, 0i64);
    for (i, r) in recs.iter().enumerate() {
        if let Some(last) = out.last_mut()
            && last.cmd == Cmd::Jump
            && r.cmd == Cmd::Jump
            && fits(before, r)
        {
            *last = *r;
        } else {
            before = out.last().map_or((0, 0), |l| (l.x, l.y));
            out.push(*r);
        }
        if let (Some(last), Some(next)) = (out.last_mut(), recs.get(i + 1))
            && last.cmd == Cmd::Jump
            && next.cmd == Cmd::Stitch
            && fits(before, next)
        {
            (last.x, last.y) = (next.x, next.y);
        }
    }
    out
}

/// The longest prefix of `s` that fits `max` bytes, on a character boundary.
fn clip(s: &str, max: usize) -> &str {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s.get(..end).unwrap_or_default()
}

fn put_str8(out: &mut Vec<u8>, s: &str) {
    let s = clip(s, usize::from(u16::MAX));
    out.put_u16_be(s.len() as u16);
    out.put(s.as_bytes());
}

/// UTF-16 code units of `s` that fit a u16 byte length, not ending on half a surrogate pair.
fn utf16_units(s: &str) -> Vec<u16> {
    let mut units: Vec<u16> = s.encode_utf16().take(usize::from(u16::MAX) / 2).collect();
    if units.last().is_some_and(|u| (0xD800..0xDC00).contains(u)) {
        units.pop();
    }
    units
}

fn put_str16(out: &mut Vec<u8>, s: &str) {
    let units = utf16_units(s);
    out.put_u16_be((units.len() * 2) as u16);
    for u in units {
        out.put_u16_be(u);
    }
}

/// A tagged block: tag, u32 length of the content, content.
fn put_block(out: &mut Vec<u8>, tag: [u8; 3], content: &[u8]) -> Result<(), FormatError> {
    out.put(&tag);
    out.put_u32_be(u32::try_from(content.len()).map_err(|_| FormatError::TooLarge)?);
    out.put(content);
    Ok(())
}

/// The stitch data of one colour block.
fn encode_block(recs: &[Rec], cur: &mut (i64, i64)) -> Vec<u8> {
    let mut out = Vec::with_capacity(recs.len() * 2);
    for r in recs {
        let (dx, dy) = (r.x - cur.0, r.y - cur.1);
        match r.cmd {
            Cmd::Stitch => out.extend_from_slice(&[byte(dx), byte(dy)]),
            Cmd::Jump => {
                out.extend_from_slice(&[0x80, 0x01]);
                out.put_i16_be(dx as i16);
                out.put_i16_be(dy as i16);
                out.extend_from_slice(&[0x80, 0x02]);
            }
            Cmd::Trim => out.extend_from_slice(&[0x80, 0x03]),
            Cmd::ColorChange | Cmd::Stop | Cmd::End => {}
        }
        *cur = (r.x, r.y);
    }
    out
}

pub(crate) fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, FormatError> {
    let (plan, _) = plan(pattern, opts)?;
    let (min_x, min_y, max_x, max_y) = plan.bounds(&[Cmd::Stitch, Cmd::Jump]).unwrap_or((0, 0, 0, 0));
    // Header positions: µm, +y up.
    let (right, top, left, bottom) = (max_x * UM, -min_y * UM, min_x * UM, -max_y * UM);
    let (cx, cy) = ((min_x + max_x) * UM / 2, -(min_y + max_y) * UM / 2);
    let blocks = plan.threads.len();

    let mut colour_blocks = Vec::new();
    let mut cur = (0i64, 0i64);
    for (k, recs) in plan.recs.split(|r| r.cmd == Cmd::ColorChange).enumerate() {
        let thread = plan.threads.get(k).cloned().unwrap_or_default();
        let mut b = Vec::new();
        b.put_i32_be(sat_i32(cur.0 * UM - cx));
        b.put_i32_be(sat_i32(-cur.1 * UM - cy));
        b.put(&[0x01, 0x00, thread.color.r, thread.color.g, thread.color.b, 0x00, 0x00, 0x00, 0x05, 0x28]);
        put_str8(&mut b, &thread.code);
        put_str8(&mut b, &thread.name);
        put_str8(&mut b, &thread.chart);
        let start = cur;
        let data = encode_block(recs, &mut cur);
        b.put_i32_be(sat_i32((cur.0 - start.0) * UM));
        b.put_i32_be(sat_i32(-(cur.1 - start.1) * UM));
        let mut st = vec![0x0A, 0xF6, 0x00];
        st.extend_from_slice(&data);
        put_block(&mut b, [0x00, 0x01, 0x00], &st)?;
        b.put(&[0x00]);
        put_block(&mut colour_blocks, [0x00, 0x05, 0x00], &b)?;
    }

    let mut design = Vec::new();
    design.put_i32_be(sat_i32(cx));
    design.put_i32_be(sat_i32(cy));
    design.put(&[0, 0, 0]);
    for v in [left - cx, right - cx, bottom - cy, top - cy, right - left, top - bottom] {
        design.put_i32_be(sat_i32(v));
    }
    put_str16(&mut design, "");
    design.put(&[0x64, 0x64]);
    for v in [4096, 0, 0, 4096] {
        design.put_u32_be(v);
    }
    design.put(b"xxPP\x01\x00");
    put_str16(&mut design, PRODUCER);
    design.put_u16_be(blocks as u16);
    design.put(&colour_blocks);

    let mut file = Vec::new();
    put_str16(&mut file, "");
    for v in [right, top, left, bottom] {
        file.put_i32_be(sat_i32(v));
    }
    let records = plan.recs.iter().filter(|r| r.cmd != Cmd::End).count();
    file.put_u32_be(u32::try_from(records).map_err(|_| FormatError::TooLarge)?);
    file.put_u16_be(blocks as u16);
    file.put(&[0x0C, 0x00, 0x01]);
    put_block(&mut file, [0x00, 0x03, 0x00], &design)?;

    let mut out = b"%vsm%\x00".to_vec();
    put_str16(&mut out, PRODUCER);
    put_block(&mut out, [0x00, 0x02, 0x00], &file)?;
    Ok(out)
}

/// A UTF-16BE string.
fn decode_utf16(b: &[u8]) -> String {
    let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
    String::from_utf16_lossy(&units)
}

/// A `str8`: UTF-8, unless it looks like UTF-16BE (as some files store it).
fn read_str8(c: &mut Cursor) -> Result<String, FormatError> {
    let n = usize::from(c.u16_be()?);
    let b = c.take(n)?;
    let looks_utf16 = !b.is_empty() && b.len().is_multiple_of(2) && b.iter().step_by(2).all(|&v| v == 0);
    Ok(if looks_utf16 { decode_utf16(b) } else { String::from_utf8_lossy(b).into_owned() })
}

/// A `str16`: UTF-16BE.
fn read_str16(c: &mut Cursor) -> Result<String, FormatError> {
    let n = usize::from(c.u16_be()?);
    Ok(decode_utf16(c.take(n)?))
}

fn expect(c: &mut Cursor, tag: &[u8]) -> Result<(), FormatError> {
    if c.take(tag.len())? == tag { Ok(()) } else { Err(FormatError::Invalid("VP3: unexpected block".into())) }
}

/// `a / b` rounded half away from zero (`b > 0`).
fn div_round(a: i64, b: i64) -> i64 {
    if a >= 0 { (a + b / 2) / b } else { -((-a + b / 2) / b) }
}

/// The design name, if the design block has the layout we write.
fn design_name(mut c: Cursor) -> Option<String> {
    c.skip(3 + 6 * 4).ok()?;
    let name = read_str16(&mut c).ok()?;
    (c.take(2).ok()? == [0x64, 0x64]).then_some(name)
}

/// Decodes one block's stitch data.
fn decode_stitches(dec: &mut Decoder, data: &[u8]) -> Result<(), FormatError> {
    let mut c = Cursor::new(data);
    // A record cut off by the end of the block ends it.
    while let Ok([a, b]) = c.array::<2>() {
        if a != 0x80 {
            dec.stitch(signed(a), signed(b))?;
            continue;
        }
        match b {
            0x01 => {
                let (Ok(dx), Ok(dy)) = (c.i16_be(), c.i16_be()) else { break };
                dec.jump(i64::from(dx), i64::from(dy))?;
                if c.peek(2) == Some(&[0x80, 0x02][..]) {
                    c.skip(2)?;
                }
            }
            0x03 => dec.command(Cmd::Trim)?,
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    if !bytes.starts_with(b"%vsm%") {
        return Err(if bytes.len() < 5 { FormatError::Truncated } else { FormatError::BadMagic(Format::Vp3) });
    }
    let mut c = Cursor::at(bytes, 6);
    read_str8(&mut c)?;
    expect(&mut c, &[0x00, 0x02, 0x00])?;
    c.skip(4)?;
    read_str16(&mut c)?;
    c.skip(4 * 4 + 4 + 5)?;
    // Early versions of this crate wrote the design tag twice.
    if c.peek(6) == Some(&[0x00, 0x03, 0x00, 0x00, 0x03, 0x00][..]) {
        c.skip(3)?;
    }
    expect(&mut c, &[0x00, 0x03, 0x00])?;
    c.skip(4)?;
    let (cx, cy) = (i64::from(c.i32_be()?), i64::from(c.i32_be()?));
    let name = design_name(c.clone()).unwrap_or_default();
    let rest = bytes.get(c.pos()..).ok_or(FormatError::Truncated)?;
    let marker = rest.windows(4).position(|w| w == b"xxPP").ok_or_else(|| FormatError::Invalid("VP3: no colour section".into()))?;
    let mut c = Cursor::at(bytes, c.pos() + marker + 6);
    read_str16(&mut c)?;
    let count = usize::from(c.u16_be()?);
    if count > MAX_BLOCKS {
        return Err(FormatError::TooLarge);
    }

    let mut dec = Decoder::new(10.0, false);
    let mut threads = Vec::with_capacity(count);
    for k in 0..count {
        expect(&mut c, &[0x00, 0x05, 0x00])?;
        let len = c.u32_be()? as usize;
        let end = c.pos().checked_add(len).filter(|&e| e <= bytes.len()).ok_or(FormatError::Truncated)?;
        let mut b = Cursor::new(bytes.get(..end).ok_or(FormatError::Truncated)?);
        b.skip(c.pos())?;
        let (sx, sy) = (i64::from(b.i32_be()?), i64::from(b.i32_be()?));
        b.skip(2)?;
        let [r, g, bl] = b.array::<3>()?;
        b.skip(5)?;
        let code = read_str8(&mut b)?;
        let thread_name = read_str8(&mut b)?;
        let brand = read_str8(&mut b)?;
        // The block shift, then the stitch block (`00 01 00`, its length, `0A F6 00`).
        let tagged = b.peek(11).is_some_and(|p| p.get(8..11) == Some(&[0x00, 0x01, 0x00][..]));
        let stitches_end = if tagged {
            b.skip(8 + 3)?;
            let n = b.u32_be()? as usize;
            b.pos().checked_add(n).map_or(end, |e| e.min(end))
        } else {
            b.skip(4)?;
            end
        };
        b.skip(3)?;
        threads.push(Thread::with_code(Rgb::new(r, g, bl), thread_name, code, brand));
        if k > 0 {
            dec.command(Cmd::ColorChange)?;
        }
        // Header positions are µm with +y up; the decoder works in 0.1 mm with +y down.
        let (ax, ay) = (cx.saturating_add(sx), cy.saturating_add(sy));
        dec.jump_to(div_round(ax, UM), -div_round(ay, UM))?;
        decode_stitches(&mut dec, bytes.get(b.pos()..stitches_end).unwrap_or_default())?;
        c = Cursor::at(bytes, end);
    }
    dec.finish(threads, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::Point;

    #[test]
    fn header_fields() {
        let mut p = Pattern::new();
        for (x, y) in [(-20.0, -10.0), (30.0, -10.0), (30.0, 15.0), (-20.0, 15.0)] {
            p.stitch(Point::new(x, y));
        }
        p.trim();
        p.ensure_end();
        let opts = WriteOptions { name: "Name".into(), center: false, timestamp: None, every_stop: false };
        let bytes = write(&p, &opts).unwrap();
        let mut c = Cursor::new(&bytes);
        c.skip(6).unwrap();
        read_str16(&mut c).unwrap();
        assert_eq!(c.take(3).unwrap(), [0x00, 0x02, 0x00]);
        c.skip(4).unwrap();
        assert_eq!(read_str16(&mut c).unwrap(), "", "settings");
        let ints = |c: &mut Cursor, n: usize| (0..n).map(|_| c.i32_be().unwrap()).collect::<Vec<_>>();
        // right, top, left, bottom: µm from the origin, +y up
        assert_eq!(ints(&mut c, 4), [30_000, 10_000, -20_000, -15_000]);
        let records = plan(&p, &opts).unwrap().0.recs.len() - 1;
        assert_eq!(c.u32_be().unwrap() as usize, records, "every record but the end");
        assert_eq!(c.u16_be().unwrap(), 1, "colour blocks");
        assert_eq!(c.take(6).unwrap(), [0x0C, 0x00, 0x01, 0x00, 0x03, 0x00]);
        c.skip(4).unwrap();
        assert_eq!(ints(&mut c, 2), [5_000, -2_500], "centre");
        assert_eq!(c.take(3).unwrap(), [0, 0, 0]);
        // left, right, bottom, top from the centre, then width and height
        assert_eq!(ints(&mut c, 6), [-25_000, 25_000, -12_500, 12_500, 50_000, 25_000]);
        assert_eq!(read_str16(&mut c).unwrap(), "", "settings");
        assert_eq!(c.take(2).unwrap(), [0x64, 0x64]);
    }

    #[test]
    fn strings() {
        let mut out = Vec::new();
        put_str16(&mut out, "Rosé 日本 🧵");
        let mut c = Cursor::new(&out);
        assert_eq!(read_str16(&mut c).unwrap(), "Rosé 日本 🧵");
        let mut out = Vec::new();
        put_str8(&mut out, "Madeira Ñ 日本");
        let mut c = Cursor::new(&out);
        assert_eq!(read_str8(&mut c).unwrap(), "Madeira Ñ 日本");
        // Some files hold UTF-16 where UTF-8 is expected.
        let mut c = Cursor::new(&[0, 4, 0, b'O', 0, b'K']);
        assert_eq!(read_str8(&mut c).unwrap(), "OK");
        assert_eq!(clip("aé", 2), "a");
    }

    #[test]
    fn jump_runs_become_one_record_to_the_next_stitch() {
        let rec = |cmd, x, y| Rec { cmd, x, y };
        let recs = vec![
            rec(Cmd::Jump, 100, 0),
            rec(Cmd::Jump, 200, 50),
            rec(Cmd::Stitch, 200, 50),
            rec(Cmd::Trim, 200, 50),
            rec(Cmd::Jump, 300, 50),
            rec(Cmd::Jump, 400, 50),
            rec(Cmd::Jump, 500, 60),
            rec(Cmd::Stitch, 500, 60),
            // a long stitch kept whole: the frame moves, the needle drops at its end
            rec(Cmd::Jump, 600, 60),
            rec(Cmd::Jump, 700, 60),
            rec(Cmd::Stitch, 790, 60),
            // a move with nothing sewn after it
            rec(Cmd::Jump, 900, 60),
            rec(Cmd::Jump, 1000, 60),
            rec(Cmd::ColorChange, 1000, 60),
            // further than one record reaches: as few as fit
            rec(Cmd::Jump, 20_000, 60),
            rec(Cmd::Jump, 40_000, 60),
            rec(Cmd::Jump, 60_000, 60),
            rec(Cmd::Stitch, 60_000, 60),
        ];
        let merged = merge_jumps(recs);
        let jumps: Vec<(i64, i64)> = merged.iter().filter(|r| r.cmd == Cmd::Jump).map(|r| (r.x, r.y)).collect();
        assert_eq!(jumps, [(200, 50), (500, 60), (790, 60), (1000, 60), (20_000, 60), (40_000, 60), (60_000, 60)]);
        let others: Vec<(Cmd, i64, i64)> = merged.iter().filter(|r| r.cmd != Cmd::Jump).map(|r| (r.cmd, r.x, r.y)).collect();
        let expected = [
            (Cmd::Stitch, 200, 50),
            (Cmd::Trim, 200, 50),
            (Cmd::Stitch, 500, 60),
            (Cmd::Stitch, 790, 60),
            (Cmd::ColorChange, 1000, 60),
            (Cmd::Stitch, 60_000, 60),
        ];
        assert_eq!(others, expected);
    }

    #[test]
    fn rounding() {
        assert_eq!(div_round(150, 100), 2);
        assert_eq!(div_round(-150, 100), -2);
        assert_eq!(div_round(-149, 100), -1);
    }
}
