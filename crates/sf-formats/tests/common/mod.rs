//! Shared helpers for the integration tests: a sample design per format and a
//! small deterministic random generator.
#![allow(dead_code)]

use sf_core::threads::{hus_chart, jef_chart, pec_chart};
use sf_core::{Cmd, Pattern, Point, Rgb, Thread};
use sf_formats::{Format, Timestamp, WriteOptions};

/// Four threads the format can store exactly: palette colours for PES/PEC/JEF/HUS,
/// arbitrary RGB with catalogue data elsewhere.
pub fn threads_for(format: Format) -> Vec<Thread> {
    match format {
        Format::Pes | Format::Pec => {
            let c = pec_chart();
            [5, 13, 2, 37].iter().map(|&i| c.threads[i].clone()).collect()
        }
        Format::Jef => {
            let c = jef_chart();
            [10, 3, 12, 6].iter().map(|&i| c.threads[i].clone()).collect()
        }
        Format::Hus => {
            let c = hus_chart();
            [3, 1, 12, 9].iter().map(|&i| c.threads[i].clone()).collect()
        }
        _ => vec![
            Thread::with_code(Rgb::new(0x12, 0x34, 0x56), "Midnight", "1234", "Madeira"),
            Thread::with_code(Rgb::new(0xFA, 0x80, 0x07), "Tangerine", "1078", "Isacord"),
            Thread::new(Rgb::new(0x3C, 0xB3, 0x71), "Sea Green"),
            Thread::new(Rgb::new(200, 10, 90), ""),
        ],
    }
}

/// A design with four colour blocks, stitches, short and long jumps, trims, a
/// stitch longer than most formats allow, negative coordinates and (where the
/// format has one) a stop.
pub fn sample(format: Format) -> Pattern {
    let pt = Point::new;
    let mut p = Pattern::new();
    p.name = "Sample 1".into();
    p.threads = threads_for(format);
    // Block 0: a zigzag, a trim and a short jump.
    p.jump(pt(-15.0, -10.0));
    for i in 0..12 {
        p.stitch(pt(-15.0 + f64::from(i) * 0.8, if i % 2 == 0 { -10.0 } else { -6.5 }));
    }
    p.trim();
    p.jump(pt(-12.0, -2.0));
    p.stitch(pt(-12.0, -2.0));
    p.stitch(pt(-8.0, -1.5));
    p.stitch(pt(-4.0, -2.5));
    p.color_change();
    // Block 1: an 18 mm stitch, a stop, a trim and a 35 mm jump.
    p.stitch(pt(-4.0, 0.0));
    p.stitch(pt(14.0, 3.0));
    p.stitch(pt(14.5, 3.6));
    if format.supports_stop() {
        p.stop();
        p.stitch(pt(15.0, 4.0));
    }
    p.trim();
    p.jump(pt(-16.0, 22.0));
    p.stitch(pt(-16.0, 22.0));
    p.stitch(pt(-13.3, 21.1));
    p.stitch(pt(-10.7, 24.9));
    p.color_change();
    // Block 2: a satin-like zigzag and a jump without a trim.
    for i in 0..30 {
        p.stitch(pt(-10.0 + f64::from(i) * 0.45, if i % 2 == 0 { 18.0 } else { 23.5 }));
    }
    p.jump(pt(5.0, 10.0));
    p.stitch(pt(5.0, 10.0));
    p.stitch(pt(9.0, 12.0));
    p.color_change();
    // Block 3: up into negative y, ending with a trim.
    p.jump(pt(12.0, -5.0));
    p.stitch(pt(12.0, -5.0));
    p.stitch(pt(18.0, -9.0));
    p.stitch(pt(12.0, -13.0));
    p.trim();
    p.ensure_end();
    p
}

/// Options with a fixed date so files are reproducible.
pub fn options(center: bool) -> WriteOptions {
    WriteOptions {
        name: "Sample 1".into(),
        center,
        timestamp: Some(Timestamp { year: 2026, month: 10, day: 10, hour: 12, minute: 0, second: 0 }),
        every_stop: false,
    }
}

/// The non-move commands in order.
pub fn commands(p: &Pattern) -> Vec<Cmd> {
    p.stitches.iter().map(|s| s.cmd).filter(|c| !matches!(c, Cmd::Stitch | Cmd::Jump)).collect()
}

fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

/// `back` sews the stitches of `orig` (moved by `shift`) up to `tol`: every
/// stitch is matched in order, and the extra stitches a writer adds to split a
/// long one lie on the needle's way to it (from the previous stitch or jump).
/// With `by_block`, each stitch must also be in the same colour block.
pub fn assert_same_stitches(orig: &Pattern, shift: Point, back: &Pattern, tol: f64, by_block: bool, what: &str) {
    let mut segments = Vec::new();
    let mut prev: Option<Point> = None;
    for (s, block) in orig.stitches.iter().zip(orig.thread_indices()) {
        let p = s.pos() + shift;
        match s.cmd {
            Cmd::Stitch => {
                segments.push((block, prev.unwrap_or(p), p));
                prev = Some(p);
            }
            Cmd::Jump => prev = Some(p),
            _ => {}
        }
    }
    let got: Vec<(usize, Point)> =
        back.stitches.iter().zip(back.thread_indices()).filter(|(s, _)| s.cmd == Cmd::Stitch).map(|(s, b)| (b, s.pos())).collect();
    let mut j = 0;
    for (k, &(block, from, to)) in segments.iter().enumerate() {
        loop {
            let Some(&(got_block, b)) = got.get(j) else { panic!("{what}: stitch {k} {to:?} missing") };
            j += 1;
            assert!(!by_block || got_block == block, "{what}: stitch {k} {to:?} in block {got_block}, not {block}");
            if b.distance(to) <= tol {
                break;
            }
            assert!(dist_to_segment(b, from, to) <= tol, "{what}: {b:?} is not on the way to stitch {k} {to:?} from {from:?}");
        }
    }
    assert_eq!(j, got.len(), "{what}: extra stitches after the end");
}

/// Same records: commands equal, positions within `tol`.
pub fn assert_same_records(expected: &Pattern, back: &Pattern, tol: f64, what: &str) {
    assert_eq!(expected.stitches.len(), back.stitches.len(), "{what}: record count");
    for (i, (e, b)) in expected.stitches.iter().zip(&back.stitches).enumerate() {
        assert_eq!(e.cmd, b.cmd, "{what}: command of record {i}");
        assert!((e.x - b.x).abs() <= tol && (e.y - b.y).abs() <= tol, "{what}: record {i} {e:?} vs {b:?}");
    }
}

/// xorshift64: deterministic pseudo-random numbers for the robustness tests.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }

    /// A float in `lo..hi`.
    pub fn float(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A random design: stitches (short and long), jumps, trims, stops and colour
/// changes in any order, random thread colours, coordinates within ±90 mm.
pub fn random_pattern(rng: &mut Rng) -> Pattern {
    let mut p = Pattern::new();
    let mut pos = Point::new(rng.float(-60.0, 60.0), rng.float(-60.0, 60.0));
    let n = 1 + rng.below(160);
    for _ in 0..n {
        match rng.below(24) {
            0 => p.trim(),
            1 => p.color_change(),
            2 => p.stop(),
            3..=5 => {
                pos = Point::new(rng.float(-90.0, 90.0), rng.float(-90.0, 90.0));
                p.jump(pos);
            }
            6 => {
                // A long stitch.
                pos = Point::new((pos.x + rng.float(-40.0, 40.0)).clamp(-90.0, 90.0), (pos.y + rng.float(-40.0, 40.0)).clamp(-90.0, 90.0));
                p.stitch(pos);
            }
            7 => p.stitch(pos),
            _ => {
                pos = Point::new((pos.x + rng.float(-6.0, 6.0)).clamp(-90.0, 90.0), (pos.y + rng.float(-6.0, 6.0)).clamp(-90.0, 90.0));
                p.stitch(pos);
            }
        }
    }
    let colours = rng.below(p.count(Cmd::ColorChange) + 2);
    p.threads = (0..colours).map(|i| Thread::new(Rgb::new(rng.next() as u8, rng.next() as u8, rng.next() as u8), format!("T{i}"))).collect();
    if rng.below(2) == 0 {
        p.ensure_end();
    }
    p
}
