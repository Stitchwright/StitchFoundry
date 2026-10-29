//! Magnetic Lasso (PC-05): a shape drawn round part of a photo whose border snaps to the edges in
//! it, then stitched like Click to Stitch. Between two fastening points the border is the
//! cheapest path along strong edges near the pointer's path, the "live wire" of Mortensen &
//! Barrett, *Intelligent Scissors for Image Composition* (SIGGRAPH 1995), as `sf_photo::magnetic`
//! (adapted from PhotoCraft) traces it.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::model::{FillParams, RunParams, SatinParams};
use sf_photo::geom::Rect;
use sf_photo::magnetic::{Settings, Tracer, WIDTH_RANGE, simplify};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, NodeKind};
use vectorcraft_trace::Raster;

use super::embroidery_photo::Picture;
use super::{CommandSpec, Result, Session, bad, cmd, f64_or, has_doc, str_param};

/// The most fastening points, and pointer-path points per stretch, a call takes.
const MAX_POINTS: usize = 2000;
const MAX_GUIDE: usize = 20_000;
/// The kinds of stitching the shape can get (`none`: a plain path).
const LASSO_AS: [&str; 4] = ["fill", "satin", "outline", "none"];

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.magneticLasso",
        "Magnetic Lasso",
        [],
        None,
        "{points: [[x, y]] the fastening points (document points, 3 or more; the shape closes back to the first), guide?: [[[x, y]]] the pointer's path after each point (one list per point; the border keeps near it), width?: px 1–256 (10: how far from the pointer's path an edge is looked for, in the photo's pixels), contrast?: 0–100 (10: how strong a step in colour has to be to count as an edge), as?: fill|satin|outline|none (fill; none: a plain path), id?: an image (the topmost under the first point, else the selected one)} a closed shape round part of a photo whose border snaps to the edges in it (live wire), stitched in the photo's colour inside it; the shape is selected → {id, points: the border's points, color}",
        has_doc,
        magnetic_lasso
    )]
}

/// `[x, y]` points under `key`, at most `max`.
fn points(v: &Value, key: &str, max: usize) -> Result<Vec<[f64; 2]>> {
    const CMD: &str = "embroidery.magneticLasso";
    let list = v.as_array().ok_or_else(|| bad(CMD, format!("`{key}` is a list of [x, y] points")))?;
    if list.len() > max {
        return Err(bad(CMD, format!("`{key}` has more than {max} points")));
    }
    list.iter()
        .map(|q| match q.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()).as_deref() {
            Some([x, y]) if x.is_finite() && y.is_finite() => Ok([*x, *y]),
            _ => Err(bad(CMD, format!("each of `{key}` is [x, y]"))),
        })
        .collect()
}

/// The image `id`, else the topmost visible one under `at`, else the selected one.
fn picture_at(s: &Session, p: &Value, at: kurbo::Point, cmd: &str) -> Result<Picture> {
    if p.get("id").is_some_and(|v| !v.is_null()) {
        return Picture::get(s, p, cmd);
    }
    let mut top = None;
    s.doc()?.doc.walk(|n| {
        if matches!(n.kind, NodeKind::Image(_)) && n.visible && n.geometric_bounds().is_some_and(|b| b.contains(at)) {
            top = Some(n.id);
        }
    });
    match top {
        Some(id) => Picture::get(s, &json!({ "id": id.0 }), cmd),
        None => Picture::get(s, p, cmd),
    }
}

/// The mean colour of the opaque pixels of `raster` inside the closed polygon `poly` (pixels);
/// `None` when it covers none.
fn mean_inside(raster: &Raster, poly: &[[f64; 2]]) -> Option<[u8; 3]> {
    let (w, h) = (raster.width, raster.height);
    let fold = |f: fn(f64, f64) -> f64, i: usize, init: f64| poly.iter().map(|q| q[i]).fold(init, f);
    let (x0, x1) = (fold(f64::min, 0, f64::INFINITY).max(0.0) as u32, (fold(f64::max, 0, 0.0).ceil().max(0.0) as u32).min(w));
    let (y0, y1) = (fold(f64::min, 1, f64::INFINITY).max(0.0) as u32, (fold(f64::max, 1, 0.0).ceil().max(0.0) as u32).min(h));
    // a sample grid of at most about 256 × 256 points over the polygon's box
    let step = ((x1.saturating_sub(x0).max(y1.saturating_sub(y0)) / 256).max(1)) as usize;
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for y in (y0..y1).step_by(step) {
        let fy = f64::from(y) + 0.5;
        for x in (x0..x1).step_by(step) {
            let fx = f64::from(x) + 0.5;
            // even-odd crossing test
            let mut inside = false;
            let mut j = poly.len().wrapping_sub(1);
            for (i, a) in poly.iter().enumerate() {
                let Some(b) = poly.get(j) else { break };
                if (a[1] > fy) != (b[1] > fy) && fx < (b[0] - a[0]) * (fy - a[1]) / (b[1] - a[1]) + a[0] {
                    inside = !inside;
                }
                j = i;
            }
            let [r, g, b, a] = raster.pixel(x, y);
            if inside && a >= 128 {
                for (s, v) in sum.iter_mut().zip([r, g, b]) {
                    *s += u64::from(v);
                }
                n += 1;
            }
        }
    }
    (n > 0).then(|| sum.map(|s| u8::try_from(s / n).unwrap_or(u8::MAX)))
}

fn magnetic_lasso(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.magneticLasso";
    let anchors = points(p.get("points").unwrap_or(&Value::Null), "points", MAX_POINTS)?;
    if anchors.len() < 3 {
        return Err(bad(CMD, "`points` needs 3 or more fastening points"));
    }
    let guides: Vec<Vec<[f64; 2]>> = match p.get("guide").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => {
            let list = v.as_array().ok_or_else(|| bad(CMD, "`guide` is a list of point lists, one per fastening point"))?;
            if list.len() > anchors.len() {
                return Err(bad(CMD, "`guide` has more lists than there are points"));
            }
            list.iter().map(|l| points(l, "guide", MAX_GUIDE)).collect::<Result<_>>()?
        }
    };
    let width = f64_or(p, "width", 10.0);
    if !(width.is_finite() && (WIDTH_RANGE.0..=WIDTH_RANGE.1).contains(&width)) {
        return Err(bad(CMD, "`width` is 1 to 256 px"));
    }
    let contrast = f64_or(p, "contrast", 10.0);
    if !(contrast.is_finite() && (0.0..=100.0).contains(&contrast)) {
        return Err(bad(CMD, "`contrast` is 0 to 100"));
    }
    let kind = str_param(p, "as").unwrap_or("fill");
    let kind = LASSO_AS.into_iter().find(|k| k.eq_ignore_ascii_case(kind)).ok_or_else(|| bad(CMD, "`as` is fill, satin, outline or none"))?;
    let first = anchors.first().map(|q| kurbo::Point::new(q[0], q[1])).unwrap_or_default();
    let pic = picture_at(s, p, first, CMD)?;
    let (w, h) = (pic.raster.width, pic.raster.height);
    if u64::from(w) * u64::from(h) > 64 << 20 {
        return Err(bad(CMD, "the image is larger than 64 megapixels: scale it down first"));
    }
    let (Ok(wi), Ok(hi)) = (i32::try_from(w), i32::try_from(h)) else { return Err(bad(CMD, "the image is too large")) };
    // everything in the photo's pixels
    let inv = pic.to_doc.inverse();
    let px = |q: &[f64; 2]| {
        let r = inv * kurbo::Point::new(q[0], q[1]);
        [r.x, r.y]
    };
    let anchors: Vec<[f64; 2]> = anchors.iter().map(px).collect();
    let guides: Vec<Vec<[f64; 2]>> = guides.iter().map(|g| g.iter().map(px).collect()).collect();
    let raster = &pic.raster;
    let mut fetch = |r: Rect| -> Vec<[u8; 4]> {
        let mut out = Vec::with_capacity(r.width() as usize * r.height() as usize);
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let inside = x >= 0 && y >= 0 && x < wi && y < hi;
                out.push(if inside { raster.pixel(x as u32, y as u32) } else { [0; 4] });
            }
        }
        out
    };
    let settings = Settings::new(width, (contrast / 100.0) as f32);
    let mut tracer = Tracer::new(Rect::new(0, 0, wi, hi));
    // each fastening point onto the edge near it, then the border from one to the next
    let snapped: Vec<[f64; 2]> = anchors.iter().map(|q| tracer.snap(&mut fetch, *q, settings).unwrap_or(*q)).collect();
    let mut border: Vec<[f64; 2]> = Vec::new();
    for (i, from) in snapped.iter().enumerate() {
        let to = snapped.get(i + 1).or(snapped.first()).copied().unwrap_or(*from);
        let guide = guides.get(i).map(Vec::as_slice).unwrap_or(&[]);
        let piece = tracer.trace(&mut fetch, *from, to, guide, settings);
        // the pieces meet at the fastening points: drop each one's last point
        let n = piece.len().saturating_sub(1);
        border.extend(piece.into_iter().take(n));
    }
    let border = simplify(&border, 0.5);
    if border.len() < 3 {
        return Err(bad(CMD, "the shape is too small to stitch"));
    }
    let [r, g, b] = mean_inside(raster, &border).unwrap_or([128, 128, 128]);
    let mut bez = kurbo::BezPath::new();
    for (i, q) in border.iter().enumerate() {
        let pt = pic.to_doc * kurbo::Point::new(q[0], q[1]);
        if i == 0 {
            bez.move_to(pt);
        } else {
            bez.line_to(pt);
        }
    }
    bez.close_path();
    let path = vectorcraft_geom::path::PathData::from_bezpath(&bez);
    let paint = Paint::solid(Color::rgb8(r, g, b));
    let look = match kind {
        "outline" => Appearance::basic(Paint::None, paint, 1.0),
        "none" => Appearance::basic(Paint::None, Paint::solid(Color::rgb8(0, 0, 0)), 1.0),
        _ => Appearance::basic(paint, Paint::None, 0.0),
    };
    let stitch = match kind {
        "satin" => Some(StitchType::Satin(SatinParams::default())),
        "outline" => Some(StitchType::Run(RunParams::default())),
        "none" => None,
        _ => Some(StitchType::Fill(FillParams::default())),
    };
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Magnetic Lasso", |d, sel| {
        let mut n = vectorcraft_doc::Node::path(d.alloc_id(), path, look);
        n.embroidery = stitch.map(|t| Box::new(Embroidery::new(t)));
        let id = n.id;
        d.insert(parent, usize::MAX, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0, "points": border.len(), "color": format!("#{r:02x}{g:02x}{b:02x}") }))
}
