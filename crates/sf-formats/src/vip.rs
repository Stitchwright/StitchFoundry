//! Pfaff `.vip` (read only), the format of older Pfaff machines that replaced HUS and came before
//! VP3. HUS's layout with another signature and a scrambled RGB colour table: the layout follows
//! the KDE Liberty project's notes (<https://community.kde.org/Projects/Liberty/File_Formats/Husqvarna_HUS>)
//! and the zlib-licensed libembroidery's reader (`readVip`), whose key table descrambles the
//! colours. The stitch streams are compressed as HUS's ([`crate::hus`]).
//!
//! Little-endian header:
//!
//! | offset | type | field |
//! |---|---|---|
//! | 0x00 | 2 bytes | signature `5D FC` (or `5B AF`) |
//! | 0x02 | u16 | 400 (HUS: 200) |
//! | 0x04 | u32 | record count |
//! | 0x08 | u32 | colour count |
//! | 0x0C | 4 × i16 | extents |
//! | 0x14 | 3 × u32 | offsets of the compressed command, x and y streams |
//! | 0x20 | 10 bytes | a name and an unknown word |
//! | 0x2A | u32 | the colour table's length |
//! | 0x2E | colours × 4 bytes | RGB and one more byte each, scrambled: each byte XOR the key's byte at its place, XOR the file byte before it |
//!
//! The streams: one byte a record each, the command (`0x80` stitch, `0x81` jump, `0x84` colour
//! change, `0x88` trim, `0x90` end), dx and dy (signed, 0.1 mm, +y up).
//!
//! Not written: nobody has published a writer that machines are known to accept (libembroidery's
//! is unfinished); machines that read VIP read HUS or VP3 too.

use crate::FormatError;
use crate::bytes::u32_le_at;
use crate::exp::signed;
use crate::hus::expand;
use crate::plan::Decoder;
use sf_core::{Cmd, Pattern, Rgb, Thread};

/// Where the scrambled colours start.
const COLORS_AT: usize = 0x2E;
const MAX_RECORDS: usize = 2_000_000;
const MAX_COLORS: usize = 10_000;

/// libembroidery's key for the colour table (zlib licence): 100 colours' worth.
const KEY: [u8; 400] = [
    0x2E, 0x82, 0xE4, 0x6F, 0x38, 0xA9, 0xDC, 0xC6, 0x7B, 0xB6, 0x28, 0xAC, 0xFD, 0xAA, 0x8A, 0x4E, 0x76, 0x2E, 0xF0, 0xE4, 0x25, 0x1B, 0x8A, 0x68,
    0x4E, 0x92, 0xB9, 0xB4, 0x95, 0xF0, 0x3E, 0xEF, 0xF7, 0x40, 0x24, 0x18, 0x39, 0x31, 0xBB, 0xE1, 0x53, 0xA8, 0x1F, 0xB1, 0x3A, 0x07, 0xFB, 0xCB,
    0xE6, 0x00, 0x81, 0x50, 0x0E, 0x40, 0xE1, 0x2C, 0x73, 0x50, 0x0D, 0x91, 0xD6, 0x0A, 0x5D, 0xD6, 0x8B, 0xB8, 0x62, 0xAE, 0x47, 0x00, 0x53, 0x5A,
    0xB7, 0x80, 0xAA, 0x28, 0xF7, 0x5D, 0x70, 0x5E, 0x2C, 0x0B, 0x98, 0xE3, 0xA0, 0x98, 0x60, 0x47, 0x89, 0x9B, 0x82, 0xFB, 0x40, 0xC9, 0xB4, 0x00,
    0x0E, 0x68, 0x6A, 0x1E, 0x09, 0x85, 0xC0, 0x53, 0x81, 0xD1, 0x98, 0x89, 0xAF, 0xE8, 0x85, 0x4F, 0xE3, 0x69, 0x89, 0x03, 0xA1, 0x2E, 0x8F, 0xCF,
    0xED, 0x91, 0x9F, 0x58, 0x1E, 0xD6, 0x84, 0x3C, 0x09, 0x27, 0xBD, 0xF4, 0xC3, 0x90, 0xC0, 0x51, 0x1B, 0x2B, 0x63, 0xBC, 0xB9, 0x3D, 0x40, 0x4D,
    0x62, 0x6F, 0xE0, 0x8C, 0xF5, 0x5D, 0x08, 0xFD, 0x3D, 0x50, 0x36, 0xD7, 0xC9, 0xC9, 0x43, 0xE4, 0x2D, 0xCB, 0x95, 0xB6, 0xF4, 0x0D, 0xEA, 0xC2,
    0xFD, 0x66, 0x3F, 0x5E, 0xBD, 0x69, 0x06, 0x2A, 0x03, 0x19, 0x47, 0x2B, 0xDF, 0x38, 0xEA, 0x4F, 0x80, 0x49, 0x95, 0xB2, 0xD6, 0xF9, 0x9A, 0x75,
    0xF4, 0xD8, 0x9B, 0x1D, 0xB0, 0xA4, 0x69, 0xDB, 0xA9, 0x21, 0x79, 0x6F, 0xD8, 0xDE, 0x33, 0xFE, 0x9F, 0x04, 0xE5, 0x9A, 0x6B, 0x9B, 0x73, 0x83,
    0x62, 0x7C, 0xB9, 0x66, 0x76, 0xF2, 0x5B, 0xC9, 0x5E, 0xFC, 0x74, 0xAA, 0x6C, 0xF1, 0xCD, 0x93, 0xCE, 0xE9, 0x80, 0x53, 0x03, 0x3B, 0x97, 0x4B,
    0x39, 0x76, 0xC2, 0xC1, 0x56, 0xCB, 0x70, 0xFD, 0x3B, 0x3E, 0x52, 0x57, 0x81, 0x5D, 0x56, 0x8D, 0x51, 0x90, 0xD4, 0x76, 0xD7, 0xD5, 0x16, 0x02,
    0x6D, 0xF2, 0x4D, 0xE1, 0x0E, 0x96, 0x4F, 0xA1, 0x3A, 0xA0, 0x60, 0x59, 0x64, 0x04, 0x1A, 0xE4, 0x67, 0xB6, 0xED, 0x3F, 0x74, 0x20, 0x55, 0x1F,
    0xFB, 0x23, 0x92, 0x91, 0x53, 0xC8, 0x65, 0xAB, 0x9D, 0x51, 0xD6, 0x73, 0xDE, 0x01, 0xB1, 0x80, 0xB7, 0xC0, 0xD6, 0x80, 0x1C, 0x2E, 0x3C, 0x83,
    0x63, 0xEE, 0xBC, 0x33, 0x25, 0xE2, 0x0E, 0x7A, 0x67, 0xDE, 0x3F, 0x71, 0x14, 0x49, 0x9C, 0x92, 0x93, 0x0D, 0x26, 0x9A, 0x0E, 0xDA, 0xED, 0x6F,
    0xA4, 0x89, 0x0C, 0x1B, 0xF0, 0xA1, 0xDF, 0xE1, 0x9E, 0x3C, 0x04, 0x78, 0xE4, 0xAB, 0x6D, 0xFF, 0x9C, 0xAF, 0xCA, 0xC7, 0x88, 0x17, 0x9C, 0xE5,
    0xB7, 0x33, 0x6D, 0xDC, 0xED, 0x8F, 0x6C, 0x18, 0x1D, 0x71, 0x06, 0xB1, 0xC5, 0xE2, 0xCF, 0x13, 0x77, 0x81, 0xC5, 0xB7, 0x0A, 0x14, 0x0A, 0x6B,
    0x40, 0x26, 0xA0, 0x88, 0xD1, 0x62, 0x6A, 0xB3, 0x50, 0x12, 0xB9, 0x9B, 0xB5, 0x83, 0x9B, 0x37,
];

/// The signature: `5B`/`5D`, then `AF`/`FF`/`FC`, then 400 little-endian.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    matches!(bytes, [0x5B | 0x5D, 0xAF | 0xFF | 0xFC, 0x90, 0x01, ..]) && bytes.len() >= COLORS_AT
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    if !sniff(bytes) {
        return Err(FormatError::Invalid("not a VIP file".into()));
    }
    let num = |at: usize| u32_le_at(bytes, at).map(|v| v as usize).ok_or(FormatError::Truncated);
    let (records, colors) = (num(0x04)?, num(0x08)?);
    let (cmd_at, x_at, y_at) = (num(0x14)?, num(0x18)?, num(0x1C)?);
    if records > MAX_RECORDS {
        return Err(FormatError::TooLarge);
    }
    if colors > MAX_COLORS {
        return Err(FormatError::Invalid("VIP colour count out of range".into()));
    }
    if !(COLORS_AT <= cmd_at && cmd_at <= x_at && x_at <= y_at && y_at <= bytes.len()) {
        return Err(FormatError::Invalid("VIP stream offsets out of order".into()));
    }
    // the colours the key covers, from the bytes before the streams
    let table = bytes.get(COLORS_AT..cmd_at).unwrap_or_default();
    let mut plain = Vec::with_capacity(colors.min(KEY.len() / 4) * 4);
    let mut prev = 0u8;
    for (&b, &k) in table.iter().zip(KEY.iter()).take(colors.saturating_mul(4)) {
        plain.push(b ^ k ^ prev);
        prev = b;
    }
    let threads: Vec<Thread> = (0..colors)
        .map(|i| match plain.get(4 * i..4 * i + 3) {
            Some([r, g, b]) => Thread::new(Rgb::new(*r, *g, *b), ""),
            _ => Pattern::new().thread(i),
        })
        .collect();
    let stream = |a: usize, b: usize| bytes.get(a..b).ok_or(FormatError::Truncated).and_then(|s| expand(s, records));
    let (cmds, xs, ys) = (stream(cmd_at, x_at)?, stream(x_at, y_at)?, stream(y_at, bytes.len())?);
    let mut dec = Decoder::new(10.0, true);
    for ((&c, &x), &y) in cmds.iter().zip(&xs).zip(&ys) {
        let (dx, dy) = (signed(x), signed(y));
        match c {
            0x80 => dec.stitch(dx, dy)?,
            0x81 => dec.jump(dx, dy)?,
            0x84 => {
                dec.command(Cmd::ColorChange)?;
                if (dx, dy) != (0, 0) {
                    dec.jump(dx, dy)?;
                }
            }
            0x88 => {
                if (dx, dy) != (0, 0) {
                    dec.jump(dx, dy)?;
                }
                dec.command(Cmd::Trim)?;
            }
            // the end, or a command nobody knows
            _ => break,
        }
    }
    dec.finish(threads, String::new())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A VIP file as the layout above describes it: a HUS file's streams (its writer's), the VIP
    /// signature and a scrambled colour table in front of them.
    pub(crate) fn vip_from(pattern: &Pattern) -> Vec<u8> {
        let hus = crate::hus::write(pattern, &crate::WriteOptions { center: false, ..Default::default() }).unwrap();
        let colors = u32_le_at(&hus, 0x08).unwrap() as usize;
        let (cmd_at, x_at, y_at) =
            (u32_le_at(&hus, 0x14).unwrap() as usize, u32_le_at(&hus, 0x18).unwrap() as usize, u32_le_at(&hus, 0x1C).unwrap() as usize);
        let streams = &hus[cmd_at..];
        let mut table = Vec::new();
        let mut prev = 0u8;
        for (i, t) in pattern.threads.iter().take(colors).enumerate() {
            for (j, v) in [t.color.r, t.color.g, t.color.b, 1].into_iter().enumerate() {
                let b = v ^ KEY[4 * i + j] ^ prev;
                table.push(b);
                prev = b;
            }
        }
        let start = COLORS_AT + table.len();
        let mut out = hus[..0x2A].to_vec();
        out[0..4].copy_from_slice(&[0x5D, 0xFC, 0x90, 0x01]);
        let shift = |v: usize| ((v - cmd_at + start) as u32).to_le_bytes();
        out[0x14..0x18].copy_from_slice(&shift(cmd_at));
        out[0x18..0x1C].copy_from_slice(&shift(x_at));
        out[0x1C..0x20].copy_from_slice(&shift(y_at));
        out.extend_from_slice(&(table.len() as u32).to_le_bytes());
        out.extend_from_slice(&table);
        out.extend_from_slice(streams);
        out
    }

    #[test]
    fn a_vip_file_reads_with_its_colours() {
        let mut p = Pattern::new();
        p.threads = vec![Thread::new(Rgb::new(200, 30, 40), ""), Thread::new(Rgb::new(10, 60, 150), "")];
        let pt = sf_core::Point::new;
        p.jump(pt(0.0, 0.0));
        p.stitch(pt(5.0, 0.0));
        p.stitch(pt(5.0, 5.0));
        p.color_change();
        p.stitch(pt(10.0, 5.0));
        p.ensure_end();
        let vip = vip_from(&p);
        assert!(sniff(&vip) && crate::detect(&vip) == Some(crate::Format::Vip));
        let back = read(&vip).unwrap();
        let colors: Vec<Rgb> = back.threads.iter().map(|t| t.color).collect();
        assert_eq!(colors, [Rgb::new(200, 30, 40), Rgb::new(10, 60, 150)], "the colours descrambled");
        let stitches = |q: &Pattern| q.stitches.iter().filter(|s| s.cmd == Cmd::Stitch).count();
        assert_eq!(stitches(&back), stitches(&p));
        let last = back.stitches.iter().rev().find(|s| s.cmd == Cmd::Stitch).unwrap();
        assert!((last.x - 10.0).abs() < 0.11 && (last.y - 5.0).abs() < 0.11, "{last:?}");
        // a HUS file isn't a VIP file, and the other way round
        let hus = crate::hus::write(&p, &crate::WriteOptions::default()).unwrap();
        assert!(!sniff(&hus) && !crate::hus::sniff(&vip));
        assert!(read(&vip[..20]).is_err());
    }

    /// Untrusted files: random bytes behind a real header, cut-off files, wild counts and offsets.
    #[test]
    fn broken_vip_files_are_refused_not_crashed_on() {
        let mut p = Pattern::new();
        p.threads = vec![Thread::new(Rgb::new(1, 2, 3), "")];
        p.jump(sf_core::Point::new(0.0, 0.0));
        for i in 0..40 {
            p.stitch(sf_core::Point::new(f64::from(i % 7), f64::from(i % 5)));
        }
        p.ensure_end();
        let vip = vip_from(&p);
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..3000 {
            let keep = (next() as usize) % vip.len();
            let mut bytes = vip[..keep].to_vec();
            for _ in 0..(next() % 600) {
                bytes.push(next() as u8);
            }
            let _ = read(&bytes);
            let mut flipped = vip.clone();
            let at = (next() as usize) % flipped.len();
            flipped[at] ^= next() as u8;
            let _ = read(&flipped);
        }
        let mut wild = vip.clone();
        wild[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(&wild).is_err(), "a record count no file holds");
        let mut wild = vip.clone();
        wild[0x14..0x18].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read(&wild).is_err(), "a stream past the file's end");
    }
}
