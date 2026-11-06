//! ZSK `.dsz` (read only): a 512-byte DST header (label, counts and extents, see
//! [`crate::dst`]), then 3-byte records: the y move, the x move (0.1 mm, +y up, at most 255)
//! and a control byte. Control byte: bit 6 makes x negative, bit 5 makes y negative; the low
//! five bits 0x00 are a stitch and 0x01 a jump; otherwise the whole byte is a command: 0x82
//! stop, 0x9B trim, 0x83–0x9A a change to needle 1–12 (two codes each). Anything else ends the
//! design; there is no end record. No colours: the reader colours the blocks from a generic
//! palette.
//!
//! Public description: the format notes of the MIT-licensed pyembroidery project
//! (https://github.com/EmbroidePy/pyembroidery). It reads DSZ but documents no writer, so
//! StitchFoundry only opens these files.

use crate::plan::Decoder;
use crate::{FormatError, dst};
use sf_core::{Cmd, Pattern};

const HEADER: usize = 512;
const X_NEG: u8 = 0x40;
const Y_NEG: u8 = 0x20;
const STITCH: u8 = 0x00;
const JUMP: u8 = 0x01;
const STOP: u8 = 0x82;
const TRIM: u8 = 0x9B;
const NEEDLE_FIRST: u8 = 0x83;
const NEEDLE_LAST: u8 = 0x9A;

/// What a record does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Stitch(i64, i64),
    Jump(i64, i64),
    Command(Cmd),
    End,
}

fn op(b: &[u8; 3]) -> Op {
    let (dy, dx, ctrl) = (i64::from(b[0]), i64::from(b[1]), b[2]);
    let dx = if ctrl & X_NEG != 0 { -dx } else { dx };
    let dy = if ctrl & Y_NEG != 0 { -dy } else { dy };
    match ctrl {
        _ if ctrl & 0x1F == STITCH => Op::Stitch(dx, dy),
        _ if ctrl & 0x1F == JUMP => Op::Jump(dx, dy),
        STOP => Op::Command(Cmd::Stop),
        TRIM => Op::Command(Cmd::Trim),
        NEEDLE_FIRST..=NEEDLE_LAST => Op::Command(Cmd::ColorChange),
        _ => Op::End,
    }
}

pub(crate) fn read(bytes: &[u8]) -> Result<Pattern, FormatError> {
    let body = bytes.get(HEADER..).ok_or(FormatError::Truncated)?;
    let mut dec = Decoder::new(10.0, true);
    for b in body.as_chunks::<3>().0 {
        match op(b) {
            Op::Stitch(dx, dy) => dec.stitch(dx, dy)?,
            Op::Jump(dx, dy) => dec.jump(dx, dy)?,
            Op::Command(cmd) => dec.command(cmd)?,
            Op::End => break,
        }
    }
    dec.finish(Vec::new(), dst::header_label(bytes))
}

/// A DST header followed by DSZ records, a good share of them stitches: a DST body never has a record
/// whose low five bits are 0 or 1 (DST sets bits 0 and 1 of every record's last byte).
pub(crate) fn sniff(bytes: &[u8]) -> bool {
    if !dst::sniff(bytes) {
        return false;
    }
    let Some(body) = bytes.get(HEADER..) else { return false };
    let (mut stitches, mut records) = (0usize, 0usize);
    for b in body.as_chunks::<3>().0 {
        match op(b) {
            Op::Stitch(..) => stitches += 1,
            Op::End => break,
            _ => {}
        }
        records += 1;
    }
    stitches > 0 && stitches * 4 >= records
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DSZ file from records written here from the layout above.
    fn file(records: &[[u8; 3]]) -> Vec<u8> {
        let mut out = b"LA:dsz test        \rST:      4\rCO:  1\r".to_vec();
        out.resize(HEADER, b' ');
        for r in records {
            out.extend_from_slice(r);
        }
        out
    }

    #[test]
    fn reads_moves_commands_and_the_label() {
        // stitch 1 mm right and 2 mm up, jump 0.5 mm left and down, trim, needle 2, stitch, stop
        let bytes = file(&[[20, 10, 0x80], [5, 5, 0x81 | X_NEG | Y_NEG], [0, 0, TRIM], [0, 0, 0x85], [0, 3, 0x80], [0, 0, STOP]]);
        assert!(sniff(&bytes));
        let p = read(&bytes).unwrap();
        assert_eq!(p.name, "dsz test");
        let cmds: Vec<Cmd> = p.stitches.iter().map(|s| s.cmd).collect();
        assert!(cmds.contains(&Cmd::Trim) && cmds.contains(&Cmd::ColorChange), "{cmds:?}");
        let firsts: Vec<(f64, f64)> = p.stitches.iter().filter(|s| s.cmd == Cmd::Stitch).map(|s| (s.x, s.y)).collect();
        // +y down in a pattern: 2 mm up is −2
        let (x0, y0) = firsts[0];
        let (x1, y1) = firsts[firsts.len() - 1];
        assert!(((x1 - x0) - (-0.5 + 0.3)).abs() < 1e-6 && ((y1 - y0) - 0.5).abs() < 1e-6, "{firsts:?}");
    }

    #[test]
    fn an_unknown_command_ends_the_design() {
        let p = read(&file(&[[10, 10, 0x80], [0, 0, 0xF0], [10, 10, 0x80]])).unwrap();
        assert_eq!(p.stitches.iter().filter(|s| s.cmd == Cmd::Stitch).count(), 1);
    }

    #[test]
    fn a_dst_file_is_not_taken_for_dsz() {
        let mut p = Pattern::new();
        p.threads.push(sf_core::Thread::default());
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (0.0, 0.0), (40.0, 0.0), (45.0, 5.0)] {
            p.stitches.push(sf_core::Stitch::new(x, y, Cmd::Stitch));
        }
        let dst = crate::write(crate::Format::Dst, &p, &Default::default()).unwrap();
        assert!(crate::dst::sniff(&dst) && !sniff(&dst));
        assert_eq!(crate::detect(&dst), Some(crate::Format::Dst));
    }
}
