//! Readers parse untrusted bytes: random data, every truncation of a valid
//! file and valid files with random byte changes must give `Ok` or `Err`,
//! never a panic.
// Integration tests: unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Rng, options, sample};
use sf_formats::{Format, detect, read, read_any, write};

/// Reads `bytes` every way there is; only a panic can fail.
fn read_all_ways(format: Format, bytes: &[u8]) {
    let _ = read(format, bytes);
    let _ = detect(bytes);
    let _ = read_any(bytes, Some(format.extension()));
}

#[test]
fn random_bytes() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for format in Format::ALL.into_iter().chain(Format::READ_ONLY) {
        for _ in 0..2000 {
            let len = rng.below(2049);
            let bytes = rng.bytes(len);
            read_all_ways(format, &bytes);
        }
    }
}

#[test]
fn random_bytes_behind_a_valid_header() {
    // Random data behind each format's real header reaches the deeper parsing code.
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for format in Format::ALL {
        let valid = write(format, &sample(format), &options(true)).unwrap();
        let keep = match format {
            Format::Dst | Format::U01 | Format::Xxx | Format::Dsb => 0x200.min(valid.len()),
            Format::Pes | Format::Pec => 0x220,
            Format::Jef => 0x94,
            Format::Vp3 => 120,
            Format::Exp | Format::Tap => 0,
            // the hoop, the colours and the record count
            Format::Pcs => 22,
            // the header and the colour table
            Format::Hus => 0x2A + 2 * 4,
            Format::Dsz => 0,
        };
        for _ in 0..2000 {
            let mut bytes = valid[..keep.min(valid.len())].to_vec();
            let len = rng.below(2049);
            bytes.extend(rng.bytes(len));
            read_all_ways(format, &bytes);
        }
    }
}

/// D1: ZSK DSZ is read only, so its fuzzing starts from a DSB file's DST header.
#[test]
fn random_dsz_records_behind_a_dst_header() {
    let mut rng = Rng(0x6C07_8965_D1B5_4A32);
    let header = write(Format::Dsb, &sample(Format::Dsb), &options(true)).unwrap()[..0x200].to_vec();
    for _ in 0..4000 {
        let mut bytes = header.clone();
        let len = rng.below(2049);
        bytes.extend(rng.bytes(len));
        read_all_ways(Format::Dsz, &bytes);
    }
}

#[test]
fn every_truncation_of_a_valid_file() {
    for format in Format::ALL {
        let valid = write(format, &sample(format), &options(true)).unwrap();
        for n in 0..valid.len() {
            read_all_ways(format, &valid[..n]);
        }
    }
}

#[test]
fn valid_files_with_random_byte_changes() {
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    for format in Format::ALL {
        let valid = write(format, &sample(format), &options(true)).unwrap();
        for _ in 0..2000 {
            let mut bytes = valid.clone();
            for _ in 0..=rng.below(8) {
                let i = rng.below(bytes.len());
                bytes[i] = if rng.below(4) == 0 { 0xFF } else { rng.next() as u8 };
            }
            read_all_ways(format, &bytes);
        }
    }
}

#[test]
fn extreme_header_values() {
    // Offsets and counts at their limits must be refused or bounded, not trusted.
    for format in Format::ALL {
        let valid = write(format, &sample(format), &options(true)).unwrap();
        for value in [0u8, 0x7F, 0x80, 0xFF] {
            for start in (0..valid.len().min(0x220)).step_by(2) {
                let mut bytes = valid.clone();
                for b in bytes.iter_mut().skip(start).take(4) {
                    *b = value;
                }
                read_all_ways(format, &bytes);
            }
        }
    }
}
