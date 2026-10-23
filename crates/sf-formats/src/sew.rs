//! Janome `.sew` (experimental), the format of older Janome, Elna and Kenmore machines. The layout
//! follows the MIT-licensed pyembroidery project's reader (`SewReader.py`) and the zlib-licensed
//! libembroidery's (`readSew`, `writeSew`); no machine has sewn a SEW file we wrote yet.
//!
//! | offset | type | field |
//! |---|---|---|
//! | 0x0000 | u16 | colour (block) count |
//! | 0x0002 | colours × u16 | palette index of each block ([`sew_chart`]: SEW's own palette, not JEF's) |
//! | … | | up to 0x1D78: a preview picture in Janome's files; we write zeros |
//! | 0x1D78 | | stitch data, as JEF's |
//!
//! Stitch data: signed 8-bit dx dy pairs in 0.1 mm, +y up, at most ±127 per axis; `0x80`
//! escapes: `80 01 dx dy` colour change, `80 02 dx dy` move (jump; `80 04` reads as one too),
//! `80 10 00 00` end. A trim is written as a move of zero length, as in JEF.
//!
//! Uncertain: what machines make of a preview of zeros, and `80 10`, which libembroidery reads as
//! the end and pyembroidery as a stitch (we write it at the end only, with no move, so both read
//! the same design).

use crate::bytes::{Cursor, Put};
use crate::exp::{byte, signed};
use crate::plan::{Decoder, Plan, PlanCfg, build_plan};
use crate::{FormatError, WriteOptions};
use sf_core::threads::sew_chart;
use sf_core::{Cmd, Pattern, Thread};

const MAX_STEP: i64 = 127;
/// Where the stitch data starts.
const STITCHES: usize = 0x1D78;
/// The colour table must end before the stitch data.
const MAX_COLORS: usize = (STITCHES - 2) / 2;

pub(crate) fn plan(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg), FormatError> {
    plan_with_indices(pattern, opts).map(|(plan, cfg, _)| (plan, cfg))
}

fn plan_with_indices(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg, Vec<usize>), FormatError> {
    let cfg = PlanCfg::tenth_mm(true, MAX_STEP, MAX_STEP, opts.center);
    let mut plan = build_plan(pattern, &cfg.with(opts))?;
    let indices = plan.map_to_chart(&sew_chart());
    Ok((plan, cfg, indices))
}

pub(crate) fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, FormatError> {
    let (plan, _, indices) = plan_with_indices(pattern, opts)?;
    if indices.len() > MAX_COLORS {
        return Err(FormatError::TooLarge);
    }
    let mut out = Vec::with_capacity(STITCHES + plan.recs.len() * 2 + 4);
    out.put(&u16::try_from(indices.len()).map_err(|_| FormatError::TooLarge)?.to_le_bytes());
    for &i in &indices {
        out.put(&u16::try_from(i).unwrap_or(0).to_le_bytes());
    }
    out.resize(STITCHES, 0);
    let mut cur = (0i64, 0i64);
    for r in &plan.recs {
        let (dx, dy) = (r.x - cur.0, r.y - cur.1);
        match r.cmd {
            Cmd::Stitch => out.extend_from_slice(&[byte(dx), byte(dy)]),
            Cmd::Jump => out.extend_from_slice(&[0x80, 0x02, byte(dx), byte(dy)]),
            Cmd::Trim => out.extend_from_slice(&[0x80, 0x02, 0x00, 0x00]),
            Cmd::ColorChange | Cmd::Stop => out.extend_from_slice(&[0x80, 0x01, 0x00, 0x00]),
            Cmd::End => out.extend_from_slice(&[0x80, 0x10, 0x00, 0x00]),
        }
        cur = (r.x, r.y);
    }
    Ok(out)
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    let colors = bytes.get(0..2).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]]))).ok_or(FormatError::Truncated)?;
    if colors > MAX_COLORS {
        return Err(FormatError::Invalid("SEW colour table does not fit before the stitches".into()));
    }
    if bytes.len() < STITCHES {
        return Err(FormatError::Truncated);
    }
    let chart = sew_chart();
    let threads: Vec<Thread> = (0..colors)
        .map(|i| {
            let at = 2 + 2 * i;
            let idx = bytes.get(at..at + 2).map_or(0, |b| usize::from(u16::from_le_bytes([b[0], b[1]])));
            // index 0 is "unknown"; past the palette it wraps, as the readers do
            let idx = idx % chart.threads.len().max(1);
            chart.threads.get(idx).filter(|_| idx > 0).cloned().unwrap_or_else(|| Pattern::new().thread(i))
        })
        .collect();
    let mut dec = Decoder::new(10.0, true);
    let mut c = Cursor::at(bytes, STITCHES);
    // a file cut off before its end record ends where it stops
    while let Ok([a, b]) = c.array::<2>() {
        if a != 0x80 {
            dec.stitch(signed(a), signed(b))?;
            continue;
        }
        let Ok([x, y]) = c.array::<2>() else { break };
        match b {
            _ if b & 0x01 != 0 => {
                dec.command(Cmd::ColorChange)?;
                if (x, y) != (0, 0) {
                    dec.jump(signed(x), signed(y))?;
                }
            }
            0x02 | 0x04 => {
                if (x, y) == (0, 0) {
                    dec.command(Cmd::Trim)?;
                } else {
                    dec.jump(signed(x), signed(y))?;
                }
            }
            0x10 => {
                // the end; a move given with it is a last stitch
                if (x, y) != (0, 0) {
                    dec.stitch(signed(x), signed(y))?;
                }
                break;
            }
            _ => break,
        }
    }
    dec.finish(threads, String::new())
}

/// No signature: a colour count of 1 to 100, every block's palette index within the Janome
/// palette, and room for the stitch data. Tried after every format with a signature.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    let Some(colors) = bytes.get(0..2).map(|b| usize::from(u16::from_le_bytes([b[0], b[1]]))) else { return false };
    let palette = sew_chart().threads.len();
    (1..=100).contains(&colors)
        && bytes.len() >= STITCHES + 2
        && (0..colors).all(|i| bytes.get(2 + 2 * i..4 + 2 * i).is_some_and(|b| usize::from(u16::from_le_bytes([b[0], b[1]])) < palette))
}
