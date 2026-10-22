//! Every format: write → read gives the design back (positions within the
//! file resolution, colour blocks, colours, commands), `detect` recognises
//! every written file, and the shared preparation behaves as documented.
// Integration tests: unwrapping and panicking on failure is fine here, unlike in shipped code (AGENTS.md › Robustness).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{Rng, assert_same_records, assert_same_stitches, commands, options, random_pattern, sample};
use sf_core::{Cmd, Pattern, Point};
use sf_formats::{Format, FormatError, WriteOptions, detect, normalize, prepare, read, read_any, write};

/// 0.1 mm resolution: positions within half a unit on each axis (0.071 mm), well inside 0.11 mm.
const TOL: f64 = 0.11;

/// The tolerance for `format`: PCS stores 1/6 mm, and a long move split between rounded ends
/// rounds its steps again (about one unit across).
fn tol(format: Format) -> f64 {
    if format == Format::Pcs { 0.18 } else { TOL }
}

#[test]
fn uncentred_files_give_the_original_design_back() {
    for format in Format::ALL {
        let original = sample(format);
        let bytes = write(format, &original, &options(false)).unwrap();
        let back = read(format, &bytes).unwrap_or_else(|e| panic!("{format}: {e}"));
        let what = format.name();

        // Same colour blocks, same stitches in each (long stitches may be split).
        assert_eq!(back.color_blocks().len(), original.color_blocks().len(), "{what}: colour blocks");
        assert_same_stitches(&original, Point::ZERO, &back, tol(format), true, what);
        // Commands in order: trims, stops (where supported), colour changes, end.
        assert_eq!(commands(&back), commands(&original), "{what}: commands");
        // Every jump of the original lands where it did.
        for s in original.stitches.iter().filter(|s| s.cmd == Cmd::Jump) {
            assert!(
                back.stitches.iter().any(|b| b.cmd == Cmd::Jump && b.pos().distance(s.pos()) <= tol(format)),
                "{what}: jump to {:?} lost",
                s.pos()
            );
        }
        // Colours.
        assert_eq!(back.threads.len(), original.threads.len(), "{what}: one thread per block");
        if format.has_colors() {
            for (i, (o, b)) in original.threads.iter().zip(&back.threads).enumerate() {
                assert_eq!(o.color, b.color, "{what}: colour of block {i}");
            }
        }
        match format {
            // Palette formats give back the palette thread (the sample uses palette colours).
            Format::Pes | Format::Pec | Format::Jef | Format::Hus | Format::Sew | Format::Vp3 => {
                assert_eq!(back.threads, original.threads, "{what}: threads")
            }
            _ => {}
        }
        if matches!(format, Format::Dst | Format::Pes | Format::Pec) {
            assert_eq!(back.name, "Sample 1", "{what}: name");
        }
    }
}

#[test]
fn files_hold_exactly_the_prepared_pattern() {
    for format in Format::ALL {
        for center in [true, false] {
            let original = sample(format);
            let opts = options(center);
            let expected = prepare(format, &original, &opts).unwrap();
            let back = read(format, &write(format, &original, &opts).unwrap()).unwrap();
            let what = format!("{} centred={center}", format.name());
            assert_same_records(&expected, &back, 1e-9, &what);
            assert_eq!(back.threads, expected.threads, "{what}: threads");
            assert_eq!(back.name, expected.name, "{what}: name");
            if center {
                let c = back.bounds().center();
                assert!(c.x.abs() <= 0.051 && c.y.abs() <= 0.051, "{what}: centre {c:?}");
            }
        }
    }
}

#[test]
fn detect_recognises_every_written_file() {
    for format in Format::ALL {
        for center in [true, false] {
            let bytes = write(format, &sample(format), &options(center)).unwrap();
            assert_eq!(detect(&bytes), Some(format), "{}", format.name());
            let (found, pattern) = read_any(&bytes, None).unwrap();
            assert_eq!(found, format);
            assert_eq!(pattern, read(format, &bytes).unwrap());
            // A wrong extension falls back to detection.
            let wrong = if format == Format::Dst { "exp" } else { "dst" };
            assert_eq!(read_any(&bytes, Some(wrong)).unwrap().0, format, "{} named .{wrong}", format.name());
            assert_eq!(read_any(&bytes, Some(&format!("design.{}", format.extension()))).unwrap().0, format);
        }
    }
    assert_eq!(read_any(b"", None), Err(FormatError::Unknown));
}

#[test]
fn the_default_options_centre_the_design() {
    let original = sample(Format::Dst);
    let back = read(Format::Dst, &write(Format::Dst, &original, &WriteOptions::default()).unwrap()).unwrap();
    let c = back.bounds().center();
    assert!(c.x.abs() <= 0.051 && c.y.abs() <= 0.051, "{c:?}");
    // The stitches keep their shape, moved by minus the original centre.
    assert_same_stitches(&original, Point::ZERO - original.bounds().center(), &back, TOL, true, "centred DST");
}

#[test]
fn long_moves_are_split_within_each_formats_limit() {
    let mut p = Pattern::new();
    p.stitch(Point::new(0.0, 0.0));
    p.stitch(Point::new(40.0, -3.0));
    p.trim();
    p.jump(Point::new(-150.0, 90.0));
    p.stitch(Point::new(-150.0, 90.0));
    let limits = [
        (Format::Dst, 12.1, 12.1),
        (Format::Exp, 12.7, 12.7),
        (Format::Jef, 12.7, 12.7),
        (Format::Pes, 204.7, 204.7),
        (Format::Vp3, 12.7, 3276.7),
        (Format::Xxx, 12.4, 12.4),
        (Format::U01, 12.7, 12.7),
        (Format::Pcs, 12.0, 12.0),
        (Format::Tap, 12.1, 12.1),
        (Format::Dsb, 12.7, 12.7),
        (Format::Hus, 12.7, 12.7),
        (Format::Sew, 12.7, 12.7),
    ];
    for (format, max_stitch, max_jump) in limits {
        let back = read(format, &write(format, &p, &options(false)).unwrap()).unwrap();
        let mut prev = Point::ZERO;
        for s in &back.stitches {
            let d = s.pos() - prev;
            let max = if s.cmd == Cmd::Jump { max_jump } else { max_stitch };
            assert!(d.x.abs() <= max + 1e-9 && d.y.abs() <= max + 1e-9, "{format}: {d:?} > {max}");
            prev = s.pos();
        }
        assert_same_stitches(&p, Point::ZERO, &back, tol(format), true, format.name());
    }
}

#[test]
fn dst_trims_long_jump_runs() {
    let mut p = Pattern::new();
    p.stitch(Point::new(0.0, 0.0));
    p.stitch(Point::new(1.0, 0.0));
    p.jump(Point::new(50.0, 0.0)); // five records: the machine trims, so the file says so
    p.stitch(Point::new(50.0, 0.0));
    p.jump(Point::new(60.0, 0.0)); // one record: no trim
    p.stitch(Point::new(60.0, 0.0));
    let prepared = prepare(Format::Dst, &p, &options(false)).unwrap();
    assert_eq!(commands(&prepared), vec![Cmd::Trim, Cmd::End]);
    let back = read(Format::Dst, &write(Format::Dst, &p, &options(false)).unwrap()).unwrap();
    assert_same_records(&prepared, &back, 1e-9, "DST");
}

#[test]
fn stops_without_a_stop_command_become_colour_changes() {
    let mut p = Pattern::new();
    p.stitch(Point::new(0.0, 0.0));
    p.stitch(Point::new(1.0, 0.0));
    p.stop();
    p.stitch(Point::new(2.0, 0.0));
    p.threads = common::threads_for(Format::Pes)[..1].to_vec();
    for format in Format::ALL {
        let back = read(format, &write(format, &p, &options(false)).unwrap()).unwrap();
        if format.supports_stop() {
            assert_eq!(commands(&back), vec![Cmd::Stop, Cmd::End]);
        } else {
            assert_eq!(commands(&back), vec![Cmd::ColorChange, Cmd::End], "{format}");
            if format.has_colors() {
                assert_eq!(back.threads[0].color, back.threads[1].color, "{format}: the stop keeps the thread");
            }
        }
    }
}

#[test]
fn writers_refuse_what_they_cannot_store() {
    for format in Format::ALL {
        assert_eq!(write(format, &Pattern::new(), &WriteOptions::default()), Err(FormatError::Empty));
        let mut p = Pattern::new();
        p.stitch(Point::new(0.0, 0.0));
        p.stitch(Point::new(f64::NAN, 1.0));
        assert!(matches!(write(format, &p, &WriteOptions::default()), Err(FormatError::Invalid(_))), "{format}");
        let mut far = Pattern::new();
        far.stitch(Point::new(0.0, 0.0));
        far.stitch(Point::new(30_000.0, 0.0));
        assert_eq!(write(format, &far, &WriteOptions::default()), Err(FormatError::TooLarge), "{format}");
    }
}

#[test]
fn normalize_splits_moves_and_cleans_commands() {
    let mut p = Pattern::new();
    p.trim();
    p.stitch(Point::new(0.0, 0.0));
    p.stitch(Point::new(25.0, 0.0));
    p.jump(Point::new(25.0, 30.0));
    p.stitch(Point::new(25.0, 30.0));
    p.add(Point::new(25.0, 30.0), Cmd::End);
    p.add(Point::new(25.0, 30.0), Cmd::End);
    let n = normalize(&p, 10.0);
    let cmds: Vec<Cmd> = n.stitches.iter().map(|s| s.cmd).collect();
    use Cmd::*;
    assert_eq!(cmds, vec![Stitch, Stitch, Stitch, Stitch, Jump, Jump, Jump, Stitch, End]);
    assert_eq!(n.stitches[1].pos(), Point::new(8.333, 0.0));
}

#[test]
fn vp3_writes_each_run_of_jumps_as_one_record() {
    // a long move split the way stitch engines split it: jumps in a row
    let mut p = Pattern::new();
    p.stitch(Point::new(0.0, 0.0));
    p.stitch(Point::new(2.0, 0.0));
    p.trim();
    for i in 1..=4 {
        p.jump(Point::new(2.0 + 10.0 * f64::from(i), 5.0 * f64::from(i)));
    }
    p.stitch(Point::new(42.0, 20.0));
    p.stitch(Point::new(44.0, 20.0));
    p.ensure_end();
    let back = read(Format::Vp3, &write(Format::Vp3, &p, &options(false)).unwrap()).unwrap();
    // readers that take the long form for a stitch drop the needle only where the next stitch starts
    let jumps: Vec<Point> = back.stitches.iter().filter(|s| s.cmd == Cmd::Jump).map(|s| s.pos()).collect();
    assert_eq!(jumps.len(), 1, "{jumps:?}");
    assert!(jumps[0].distance(Point::new(42.0, 20.0)) <= TOL, "{jumps:?}");
    assert_same_stitches(&p, Point::ZERO, &back, TOL, true, "VP3");
    assert_eq!(commands(&back), commands(&p));
}

#[test]
fn vp3_keeps_unicode_thread_details() {
    let mut p = sample(Format::Vp3);
    p.threads[1] = sf_core::Thread::with_code(sf_core::Rgb::new(1, 2, 3), "Rosé 日本", "№ 7", "Gütermann");
    let opts = WriteOptions { name: "Ünïcode 🧵 design".into(), ..options(true) };
    let back = read(Format::Vp3, &write(Format::Vp3, &p, &opts).unwrap()).unwrap();
    assert_eq!(back.threads[1], p.threads[1]);
    // Labels limited to ASCII are made printable.
    let back = read(Format::Dst, &write(Format::Dst, &p, &opts).unwrap()).unwrap();
    assert_eq!(back.name, "_n_code _ design");
}

#[test]
fn random_designs_round_trip_through_every_format() {
    let mut rng = Rng(0x5DEE_CE66_D1CE_4E5B);
    let mut written = 0;
    for case in 0..300 {
        let original = random_pattern(&mut rng);
        let center = rng.below(2) == 0;
        for format in Format::ALL {
            let opts = options(center);
            let what = format!("case {case} {} centred={center}", format.name());
            let bytes = match write(format, &original, &opts) {
                Ok(b) => b,
                Err(FormatError::Empty) => {
                    assert_eq!(original.stitch_count(), 0, "{what}");
                    continue;
                }
                // PCS holds 16 colours at most
                Err(FormatError::Invalid(m)) if format == Format::Pcs && m.contains("16 colours") => continue,
                Err(e) => panic!("{what}: {e}"),
            };
            written += 1;
            assert_eq!(detect(&bytes), Some(format), "{what}: detect");
            let expected = prepare(format, &original, &opts).unwrap();
            let back = read(format, &bytes).unwrap_or_else(|e| panic!("{what}: {e}"));
            assert_same_records(&expected, &back, 1e-9, &what);
            assert_eq!(back.threads, expected.threads, "{what}: threads");
            assert_eq!(back.name, expected.name, "{what}: name");
            // The stitches are the original ones (moved by the centring, split where too long).
            let shift = if center { Point::ZERO - original.bounds().center() } else { Point::ZERO };
            assert_same_stitches(&original, shift, &back, tol(format), false, &what);
        }
    }
    assert!(written > 2000, "{written}");
}
