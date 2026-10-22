//! Husqvarna Viking `.hus` (experimental): the layout and the decompression follow the
//! MIT-licensed pyembroidery project's reader (`HusReader.py`, `EmbCompress.py`), which reads
//! real files; no machine has sewn a HUS file we wrote yet.
//!
//! Little-endian header:
//!
//! | offset | type | field |
//! |---|---|---|
//! | 0x00 | u32 | magic `5B AF C8 00` |
//! | 0x04 | u32 | record count |
//! | 0x08 | u32 | colour count |
//! | 0x0C | 4 × i16 | extents from the start: +x, +y, −x, −y (0.1 mm; the last two negative) |
//! | 0x14 | 3 × u32 | offsets of the compressed command, x and y streams |
//! | 0x20 | 8 bytes | a string (we write zeros) |
//! | 0x28 | u16 | unknown (we write 0) |
//! | 0x2A | colours × u16 | Husqvarna palette index of each block ([`hus_chart`]) |
//!
//! Three compressed streams follow, one byte a record each: the command (`0x80` stitch, `0x81`
//! jump, `0x84` colour change, `0x88` trim, `0x90` end), dx and dy (signed, 0.1 mm, +y up, at most
//! ±127). The compression is an LZ77 with Huffman-coded blocks (each block: its element count,
//! then the tables). The writer stores each stream in blocks whose character table codes every
//! byte as itself in 8 bits, which a decoder of the format reads like any other block.
//!
//! Uncertain: the string and the unknown word (readers ignore them) and whether machines accept
//! the stored blocks.

use crate::bytes::{Put, sat_i32, u32_le_at};
use crate::exp::{byte, signed};
use crate::plan::{Decoder, Plan, PlanCfg, build_plan};
use crate::{FormatError, WriteOptions};
use sf_core::threads::hus_chart;
use sf_core::{Cmd, Pattern, Thread};

const MAGIC: [u8; 4] = [0x5B, 0xAF, 0xC8, 0x00];
const HEADER: usize = 0x2A;
const MAX_STEP: i64 = 127;
/// Readers refuse more colours, or records, than these.
const MAX_COLORS: usize = 10_000;
const MAX_RECORDS: usize = 2_000_000;
/// The stored block's tables: no character-length table, 256 characters of 8 bits each, one
/// distance length (never used).
const STORED_TABLES: [u8; 4] = [0x02, 0xA0, 0x01, 0xFE];

pub(crate) fn plan(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg), FormatError> {
    plan_with_indices(pattern, opts).map(|(plan, cfg, _)| (plan, cfg))
}

fn plan_with_indices(pattern: &Pattern, opts: &WriteOptions) -> Result<(Plan, PlanCfg, Vec<usize>), FormatError> {
    let cfg = PlanCfg::tenth_mm(true, MAX_STEP, MAX_STEP, opts.center);
    let mut plan = build_plan(pattern, &cfg.with(opts))?;
    // index 0 is black, a colour like the others
    let indices = plan.map_to_chart_from(&hus_chart(), 0);
    Ok((plan, cfg, indices))
}

/// `data` in stored blocks of at most 65 535 bytes.
fn store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 6 * (data.len() / 0xFFFF + 1));
    for chunk in data.chunks(0xFFFF) {
        let n = u16::try_from(chunk.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&n.to_be_bytes());
        out.extend_from_slice(&STORED_TABLES);
        out.extend_from_slice(chunk);
    }
    out
}

pub(crate) fn write(pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, FormatError> {
    let (plan, _, indices) = plan_with_indices(pattern, opts)?;
    let (mut cmds, mut xs, mut ys) = (Vec::new(), Vec::new(), Vec::new());
    let mut cur = (0i64, 0i64);
    let (mut min, mut max) = ((0i64, 0i64), (0i64, 0i64));
    for r in &plan.recs {
        let (dx, dy) = (r.x - cur.0, r.y - cur.1);
        let code = match r.cmd {
            Cmd::Stitch => 0x80,
            Cmd::Jump => 0x81,
            Cmd::ColorChange | Cmd::Stop => 0x84,
            Cmd::Trim => 0x88,
            Cmd::End => 0x90,
        };
        cmds.push(code);
        xs.push(byte(dx));
        ys.push(byte(dy));
        cur = (r.x, r.y);
        min = (min.0.min(r.x), min.1.min(r.y));
        max = (max.0.max(r.x), max.1.max(r.y));
    }
    let records = u32::try_from(cmds.len()).map_err(|_| FormatError::TooLarge)?;
    let colors = indices.len();
    let (cmds, xs, ys) = (store(&cmds), store(&xs), store(&ys));
    let start = HEADER + 2 * colors;
    let at = |n: usize| u32::try_from(n).map_err(|_| FormatError::TooLarge);
    let mut out = Vec::with_capacity(start + cmds.len() + xs.len() + ys.len());
    out.put(&MAGIC);
    out.put_u32_le(records);
    out.put_u32_le(at(colors)?);
    for v in [max.0, max.1, min.0, min.1] {
        out.put(&(sat_i32(v).clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16).to_le_bytes());
    }
    out.put_u32_le(at(start)?);
    out.put_u32_le(at(start + cmds.len())?);
    out.put_u32_le(at(start + cmds.len() + xs.len())?);
    out.put(&[0; 8]);
    out.put(&[0, 0]);
    for &i in &indices {
        out.put(&u16::try_from(i).unwrap_or(0).to_le_bytes());
    }
    out.extend_from_slice(&cmds);
    out.extend_from_slice(&xs);
    out.extend_from_slice(&ys);
    Ok(out)
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    if !sniff(bytes) {
        return Err(FormatError::Invalid("not a HUS file".into()));
    }
    let num = |at: usize| u32_le_at(bytes, at).map(|v| v as usize).ok_or(FormatError::Truncated);
    let (records, colors) = (num(0x04)?, num(0x08)?);
    let (cmd_at, x_at, y_at) = (num(0x14)?, num(0x18)?, num(0x1C)?);
    if records > MAX_RECORDS {
        return Err(FormatError::TooLarge);
    }
    if colors > MAX_COLORS || HEADER + 2 * colors > bytes.len() {
        return Err(FormatError::Invalid("HUS colour table does not fit the file".into()));
    }
    if !(cmd_at <= x_at && x_at <= y_at && y_at <= bytes.len()) {
        return Err(FormatError::Invalid("HUS stream offsets out of order".into()));
    }
    let chart = hus_chart();
    let threads: Vec<Thread> = (0..colors)
        .map(|i| {
            let at = HEADER + 2 * i;
            let idx = bytes.get(at..at + 2).map_or(0, |b| usize::from(u16::from_le_bytes([b[0], b[1]])));
            chart.threads.get(idx).cloned().unwrap_or_else(|| Pattern::new().thread(i))
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

/// The magic number.
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER && bytes.starts_with(&MAGIC)
}

/// A Huffman table: symbols by the bits read, or one symbol for every read when it has no codes.
struct Huffman {
    lengths: Vec<u8>,
    table: Vec<u16>,
    width: u32,
    value: u16,
}

impl Huffman {
    fn single(value: u16) -> Huffman {
        Huffman { lengths: Vec::new(), table: Vec::new(), width: 0, value }
    }

    /// The table from code `lengths` (up to 16 bits): shorter codes first, and the lower symbol
    /// first among codes of a length.
    fn new(lengths: Vec<u8>) -> Result<Huffman, FormatError> {
        let width = u32::from(lengths.iter().copied().max().unwrap_or(0));
        if width == 0 || width > 16 {
            return Err(FormatError::Invalid("HUS compressed data has no codes".into()));
        }
        let full = 1usize << width;
        let mut table = Vec::with_capacity(full);
        for len in 1..=width {
            let span = full >> len;
            for (sym, &l) in lengths.iter().enumerate() {
                if u32::from(l) == len {
                    let sym = u16::try_from(sym).map_err(|_| FormatError::Invalid("HUS table too large".into()))?;
                    table.extend(std::iter::repeat_n(sym, span.min(full.saturating_sub(table.len()))));
                }
            }
        }
        Ok(Huffman { lengths, table, width, value: 0 })
    }

    /// The symbol the next bits (`peek`, 16 of them) stand for, and how many bits it takes.
    fn lookup(&self, peek: u32) -> (u16, u32) {
        if self.width == 0 {
            return (self.value, 0);
        }
        match self.table.get((peek >> (16 - self.width)) as usize) {
            Some(&sym) => (sym, u32::from(self.lengths.get(usize::from(sym)).copied().unwrap_or(0))),
            // a code the lengths don't cover
            None => (self.value, 0),
        }
    }
}

/// Bits from a byte slice, the most significant first; zeros past its end.
struct Bits<'a> {
    data: &'a [u8],
    pos: u64,
}

impl Bits<'_> {
    fn peek(&self, n: u32) -> u32 {
        let mut v: u32 = 0;
        for i in 0..u64::from(n) {
            let at = self.pos + i;
            let bit = usize::try_from(at / 8).ok().and_then(|b| self.data.get(b)).map_or(0, |b| (b >> (7 - (at % 8))) & 1);
            v = (v << 1) | u32::from(bit);
        }
        v
    }

    fn pop(&mut self, n: u32) -> u32 {
        let v = self.peek(n);
        self.pos += u64::from(n);
        v
    }

    fn left(&self) -> bool {
        self.pos < self.data.len() as u64 * 8
    }

    /// A length of 0–6 in 3 bits, or 7 and more: 7 plus each 1 bit before a 0 (at most 13).
    fn variable(&mut self) -> u8 {
        let mut m = self.pop(3) as u8;
        if m == 7 {
            for _ in 0..13 {
                if self.pop(1) == 1 {
                    m += 1;
                } else {
                    break;
                }
            }
        }
        m
    }
}

fn character_length_huffman(b: &mut Bits) -> Result<Huffman, FormatError> {
    let count = b.pop(5) as usize;
    if count == 0 {
        return Ok(Huffman::single(b.pop(5) as u16));
    }
    let mut lengths = vec![0u8; count];
    let mut i = 0;
    while i < count {
        if i == 3 {
            i += b.pop(2) as usize;
        }
        if let Some(l) = lengths.get_mut(i) {
            *l = b.variable();
        }
        i += 1;
    }
    Huffman::new(lengths).map(|mut h| {
        h.value = 8;
        h
    })
}

fn character_huffman(b: &mut Bits, lengths_of: &Huffman) -> Result<Huffman, FormatError> {
    let count = b.pop(9) as usize;
    if count == 0 {
        return Ok(Huffman::single(b.pop(9) as u16));
    }
    let mut lengths = vec![0u8; count];
    let mut i = 0;
    while i < count {
        let (c, n) = lengths_of.lookup(b.peek(16));
        b.pos += u64::from(n);
        match c {
            0 => i += 1,
            1 => i += 3 + b.pop(4) as usize,
            2 => i += 20 + b.pop(9) as usize,
            c => {
                if let Some(l) = lengths.get_mut(i) {
                    *l = u8::try_from(c - 2).unwrap_or(u8::MAX);
                }
                i += 1;
            }
        }
    }
    Huffman::new(lengths)
}

fn distance_huffman(b: &mut Bits) -> Result<Huffman, FormatError> {
    let count = b.pop(5) as usize;
    if count == 0 {
        return Ok(Huffman::single(b.pop(5) as u16));
    }
    let lengths: Vec<u8> = (0..count).map(|_| b.variable()).collect();
    Huffman::new(lengths)
}

/// One stream decompressed, to at most `size` bytes.
fn expand(data: &[u8], size: usize) -> Result<Vec<u8>, FormatError> {
    let mut b = Bits { data, pos: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(size.min(MAX_RECORDS));
    let mut tables: Option<(Huffman, Huffman)> = None;
    let mut left_in_block = 0u32;
    while b.left() && out.len() < size {
        if left_in_block == 0 {
            left_in_block = b.pop(16);
            let lengths_of = character_length_huffman(&mut b)?;
            tables = Some((character_huffman(&mut b, &lengths_of)?, distance_huffman(&mut b)?));
        }
        left_in_block = left_in_block.saturating_sub(1);
        let Some((chars, distances)) = &tables else { break };
        let (c, n) = chars.lookup(b.peek(16));
        b.pos += u64::from(n);
        match c {
            0..=255 => out.push(c as u8),
            510 => break,
            c => {
                let length = usize::from(c) - 253;
                let (d, n) = distances.lookup(b.peek(16));
                b.pos += u64::from(n);
                let back = if d == 0 { 1 } else { (1usize << (d - 1)) + b.pop(u32::from(d) - 1) as usize + 1 };
                let from = out.len().checked_sub(back).ok_or_else(|| FormatError::Invalid("HUS data refers back before its start".into()))?;
                for i in 0..length.min(size - out.len()) {
                    let v = out.get(from + i).copied().unwrap_or(0);
                    out.push(v);
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The palette's first colour, black, is one a design can use (PEC's and JEF's index 0 is a
    /// placeholder, HUS's isn't).
    #[test]
    fn black_threads_stay_black() {
        let mut p = Pattern::new();
        p.threads = vec![Thread::new(sf_core::Rgb::new(5, 5, 5), "Ink")];
        p.stitch(sf_core::Point::new(0.0, 0.0));
        p.stitch(sf_core::Point::new(5.0, 0.0));
        p.stitch(sf_core::Point::new(5.0, 5.0));
        let back = read(&write(&p, &WriteOptions::default()).unwrap()).unwrap();
        assert_eq!(back.threads[0].color, sf_core::Rgb::new(0, 0, 0));
    }

    #[test]
    fn stored_blocks_expand_to_their_bytes() {
        for len in [0usize, 1, 5, 255, 256, 300, 512, 65_535, 65_536, 70_000] {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 + i / 3) as u8).collect();
            assert_eq!(expand(&store(&data), len).unwrap(), data, "{len} bytes");
        }
    }

    #[test]
    fn a_table_with_one_symbol_repeats_it() {
        // a block of 5, no character-length table (5), the character 65 for every read (9+9),
        // no distances (5+5): AAAAA
        let mut bits = String::new();
        for (v, n) in [(5u32, 16usize), (0, 5), (0, 5), (0, 9), (65, 9), (0, 5), (0, 5)] {
            bits += &format!("{v:0n$b}");
        }
        while !bits.len().is_multiple_of(8) {
            bits.push('0');
        }
        let bytes: Vec<u8> = bits.as_bytes().chunks(8).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap()).collect();
        assert_eq!(expand(&bytes, 5).unwrap(), b"AAAAA");
    }

    #[test]
    fn a_reference_before_the_start_is_an_error() {
        // every character is a copy of 3 from one back, with nothing written yet
        let mut bits = String::new();
        for (v, n) in [(5u32, 16usize), (0, 5), (0, 5), (0, 9), (256, 9), (0, 5), (0, 5)] {
            bits += &format!("{v:0n$b}");
        }
        while !bits.len().is_multiple_of(8) {
            bits.push('0');
        }
        let bytes: Vec<u8> = bits.as_bytes().chunks(8).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap()).collect();
        assert!(expand(&bytes, 5).is_err());
    }
}
