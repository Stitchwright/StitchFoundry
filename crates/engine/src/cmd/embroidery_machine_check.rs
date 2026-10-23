//! What to know before a design goes to a machine in a given file format: what the machine may
//! refuse or show differently from the design. Formats' hard limits (PCS's 16 colours, DST's 999
//! colour changes) refuse the file instead; these are the soft ones, given as warnings with the
//! file: a design larger than its hoop or than the format's largest hoop, threads a palette
//! format can only show as its nearest colour, colours a format doesn't store, and stops a format
//! writes as colour changes.

use sf_core::hoops::Hoop;
use sf_core::threads::{Rgb, ThreadChart, hus_chart, jef_chart, pec_chart, sew_chart};
use sf_core::{BBox, Cmd, Pattern, Point};
use sf_formats::Format;

/// A thread further than this (CIEDE2000) from every palette colour is named in a warning.
pub(crate) const PALETTE_MISS: f64 = 12.0;
/// The largest Janome hoop a JEF file can name (mm).
const JEF_LARGEST: (f64, f64) = (200.0, 200.0);

/// The palette a format stores thread colours as, and whether its first entry is a placeholder.
fn palette(f: Format) -> Option<(ThreadChart, bool)> {
    match f {
        Format::Pec | Format::Pes => Some((pec_chart(), false)),
        Format::Jef => Some((jef_chart(), true)),
        Format::Sew => Some((sew_chart(), true)),
        Format::Hus => Some((hus_chart(), false)),
        _ => None,
    }
}

/// Warnings for `pattern` written as `f`, sewn in `hoop` with its middle at `center` (mm, the
/// hoop's centre when `None`).
pub(crate) fn warnings(f: Format, pattern: &Pattern, hoop: Option<&Hoop>, center: Option<[f64; 2]>) -> Vec<String> {
    let mut out = Vec::new();
    let b = pattern.bounds();
    let (w, h) = (b.width(), b.height());
    if let Some(hoop) = hoop {
        let [cx, cy] = center.unwrap_or([0.0, 0.0]);
        let placed = BBox::new(Point::new(cx - w / 2.0, cy - h / 2.0), Point::new(cx + w / 2.0, cy + h / 2.0));
        if !hoop.fits(&placed) {
            out.push(format!(
                "the design is {w:.0} × {h:.0} mm, larger than the {} hoop's {:.0} × {:.0} mm sewing field: the machine may refuse it",
                hoop.name, hoop.width, hoop.height
            ));
        }
    }
    if f == Format::Jef && (w > JEF_LARGEST.0 || h > JEF_LARGEST.1) {
        out.push(format!("JEF names Janome hoops up to 200 × 200 mm: a machine may refuse this {w:.0} × {h:.0} mm design"));
    }
    let blocks = pattern.color_blocks().len();
    if !f.has_colors() && blocks > 1 {
        out.push(format!("{} stores no thread colours: the machine shows its own, so sew the {blocks} colours by the colour list", f.name()));
    }
    if let Some((chart, placeholder)) = palette(f) {
        let colours: Vec<(Rgb, &str)> = chart.threads.iter().skip(usize::from(placeholder)).map(|t| (t.color, t.name.as_str())).collect();
        let mut missed: Vec<String> = Vec::new();
        for i in 0..blocks {
            let want = pattern.thread(i).color;
            let nearest = colours.iter().min_by(|a, b| want.delta_e2000(a.0).total_cmp(&want.delta_e2000(b.0)));
            if let Some((c, name)) = nearest
                && want.delta_e2000(*c) > PALETTE_MISS
            {
                let m = format!("{} as {name}", want.to_hex());
                if !missed.contains(&m) {
                    missed.push(m);
                }
            }
        }
        if !missed.is_empty() {
            let shown: Vec<&str> = missed.iter().take(4).map(String::as_str).collect();
            let more = if missed.len() > 4 { format!(" and {} more", missed.len() - 4) } else { String::new() };
            out.push(format!(
                "{} stores colours from its machines' palette: {} thread colour(s) show as the nearest one ({}{more}); the threads to use are on the colour list",
                f.name(),
                missed.len(),
                shown.join(", ")
            ));
        }
    }
    let stops = pattern.stitches.iter().filter(|s| s.cmd == Cmd::Stop).count();
    if stops > 0 && !f.supports_stop() {
        out.push(format!(
            "{} has no stop: the {stops} stop(s) are written as colour changes, so the machine stops there and asks for a thread",
            f.name()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::hoops::HoopShape;
    use sf_core::{Stitch, Thread};

    fn square(size: f64, colours: &[Rgb], stop: bool) -> Pattern {
        let mut p = Pattern::new();
        for (i, c) in colours.iter().enumerate() {
            if i > 0 {
                p.stitches.push(Stitch::new(0.0, 0.0, Cmd::ColorChange));
            }
            for (x, y) in [(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)] {
                p.stitches.push(Stitch::new(x, y, Cmd::Stitch));
            }
            p.threads.push(Thread::new(*c, c.to_hex()));
        }
        if stop {
            p.stitches.push(Stitch::new(0.0, 0.0, Cmd::Stop));
            p.stitches.push(Stitch::new(1.0, 1.0, Cmd::Stitch));
        }
        p
    }

    #[test]
    fn what_a_machine_may_refuse_or_show_otherwise() {
        let red = Rgb::new(237, 23, 31);
        let hoop = Hoop::new("4×4", 100.0, 100.0, HoopShape::Rect);
        // fits, Brother's own red, no stops: nothing to say
        assert!(warnings(Format::Pes, &square(50.0, &[red], false), Some(&hoop), None).is_empty());
        // larger than the hoop, or placed off its edge
        let w = warnings(Format::Pes, &square(120.0, &[red], false), Some(&hoop), None);
        assert!(w.len() == 1 && w[0].contains("larger than the 4×4 hoop"), "{w:?}");
        assert!(!warnings(Format::Pes, &square(50.0, &[red], false), Some(&hoop), Some([40.0, 0.0])).is_empty());
        // JEF's largest hoop
        assert!(warnings(Format::Jef, &square(210.0, &[red], false), None, None).iter().any(|m| m.contains("200 × 200")));
        // no colours stored
        let two = square(20.0, &[red, Rgb::new(0, 0, 0)], false);
        assert!(warnings(Format::Dst, &two, None, None).iter().any(|m| m.contains("stores no thread colours")));
        assert!(warnings(Format::Dst, &square(20.0, &[red], false), None, None).is_empty(), "one colour: nothing to sort out");
        // a fuchsia no Brother thread is near; the same in VP3 (RGB) is fine
        let fuchsia = square(20.0, &[Rgb::new(240, 50, 230)], false);
        let w = warnings(Format::Pes, &fuchsia, None, None);
        assert!(w.iter().any(|m| m.contains("#f032e6 as")), "{w:?}");
        assert!(warnings(Format::Vp3, &fuchsia, None, None).is_empty());
        // stops: written as colour changes where the format has none
        let stopped = square(20.0, &[red], true);
        assert!(warnings(Format::Pes, &stopped, None, None).iter().any(|m| m.contains("1 stop(s)")));
        assert!(warnings(Format::U01, &stopped, None, None).is_empty(), "U01 has a stop");
    }
}
