//! Images for embroidery (phase 5): an image's main colours matched to threads (BM-03), an image
//! prepared for digitizing: smoothed, its colours reduced and its edges cleaned (BM-02), and photo
//! stitch (PH-01…03): a photo sewn as lines whose zigzag follows its darkness, as one meandering
//! run that is denser where it is darker, in a few thread colours, in sepia or grey tones, or as
//! colour tiles (squares, diamonds or hexagons). Everything made is ordinary objects, editable as
//! any other.

use std::sync::Arc;

use serde_json::{Value, json};
use sf_core::Rgb;
use sf_core::attr::{Embroidery, StitchType};
use sf_core::model::FillParams;
use sf_core::threads::ThreadChart;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, NodeId, NodeKind};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_trace::Raster;

use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// The most points a photo stitch makes, and the longest side (cells) its colour grid has.
const MAX_POINTS: usize = 200_000;
const MAX_GRID: usize = 600;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.image.colors",
            "Image Colours",
            [],
            None,
            "{id?: an image (the selected one), colors?: 2–48 (8), chart?: a thread chart (the design's)} the image's main colours, most used first, each with how much of the image it covers and the nearest thread in the chart → {colors: [{color, share, thread: {color, name, code}}], chart}",
            has_doc,
            image_colors
        ),
        cmd!(
            query "embroidery.image.preview",
            "Colour Preview",
            [],
            None,
            "{id?: an image (the selected one), colors?: 2–32 (6), removeBackground?: bool (true), size?: px 64–512 (240), chart?: (the design's)} the image reduced to that many thread colours as Auto-Digitize sees them (the same colour reduction and speck removal), small, to compare counts before digitizing: the background (white, or the colour round its edges) left out when removed → {dataBase64: PNG, width, height, colors: [{color, share, background, thread: {color, name, code}}], sewn, threadChanges}",
            has_doc,
            preview
        ),
        cmd!(
            "embroidery.image.prepare",
            "Prepare Image",
            ["Embroidery"],
            None,
            "{id?: an image (the selected one), colors?: 0 or 2–48 (8; 0: keep them all), smooth?: px 0–5 (1: a median filter that keeps edges), clean?: passes 0–5 (1: specks and ragged edges given their neighbours' colour), replace?: bool (false: the prepared copy goes over the original, which stays)} an image made ready to digitize: smoothed, its colours reduced, its edges cleaned; the prepared copy is selected → {id, colors: [\"#rrggbb\"], width, height}",
            has_doc,
            prepare
        ),
        cmd!(
            "embroidery.photoStitch",
            "Photo Stitch",
            ["Embroidery"],
            None,
            "{id?: an image (the selected one), mode?: lines|coral|color|sepia|mono|tile (lines), color?: \"#rrggbb\" the thread (lines, coral; black), invert?: bool (light thread on a dark fabric: the light parts sewn), contrast?: 0.3–3 (1), spacing?: mm between lines 0.6–5 (1.6; lines), angle?: degrees (0; lines), density?: 0.5–8 points a mm² where darkest (3; coral), colors?: 2–32 (color 6; sepia 7; mono 11; tile 8), chart?: (the design's; color sepia mono tile), cell?: mm 0.5–5 (1; color sepia mono), shape?: square|diamond|hexagon (square; tile), size?: mm 2–12 (4; tile), gap?: mm 0–1 (0.3; tile), background?: bool (false: the colour covering most of the image's edges is left unsewn; color sepia mono tile)} the image sewn as photo stitch over it: lines across it whose zigzag widens where it is darker (lines), one run meandering through points that crowd where it is darker (coral), or its main colours as short-stitch fills, the lightest first, matched to the chart's threads, its background left out (color), the same in shades of brown (sepia) or grey (mono) by how light it is, or small tiles each one thread colour (tile) → {ids, mode, points?, colors?, tiles?}",
            has_doc,
            photo_stitch
        ),
    ]
}

/// Whether `raster` looks like a photo rather than art in flat colours: more than
/// [`PHOTO_COLORS`] colours at 4 bits a channel among its opaque pixels (a sample of at most
/// 65 536), which a drawing, even saved as a JPEG, doesn't reach.
pub(super) fn looks_like_photo(raster: &Raster) -> bool {
    // measured: a photo (the macaw) has about 1 000, a flat cartoon saved as a JPEG under 150
    const PHOTO_COLORS: usize = 300;
    let step = (raster.rgba.len() / 4 / 65_536).max(1);
    let mut seen = std::collections::HashSet::new();
    for px in raster.rgba.chunks_exact(4).step_by(step) {
        if let [r, g, b, a] = *px
            && a >= 128
        {
            seen.insert((r >> 4, g >> 4, b >> 4));
        }
    }
    seen.len() > PHOTO_COLORS
}

/// An image, its pixels and the map from pixel space to the document.
pub(super) struct Picture {
    pub(super) id: NodeId,
    pub(super) raster: Raster,
    pub(super) to_doc: kurbo::Affine,
}

impl Picture {
    /// The image `id` (else the selected one) decoded.
    pub(super) fn get(s: &Session, p: &Value, cmd: &str) -> Result<Self> {
        let d = &s.doc()?.doc;
        let node = match p.get("id").and_then(Value::as_u64) {
            Some(id) => d.node(NodeId(id)).cloned(),
            None => {
                let sel = &s.doc()?.selection.objects;
                sel.iter().find_map(|id| d.node(*id).filter(|n| matches!(n.kind, NodeKind::Image(_))).cloned())
            }
        };
        let Some(node) = node else { return Err(bad(cmd, "select an image (or give its `id`)")) };
        let NodeKind::Image(im) = &node.kind else { return Err(bad(cmd, "that isn't an image")) };
        let blob = d.images.get(&im.key).ok_or_else(|| bad(cmd, format!("image data `{}` is missing", im.key)))?;
        let raster = Raster::decode(&blob.bytes).map_err(|e| bad(cmd, e.to_string()))?;
        let to_doc =
            im.xf * kurbo::Affine::scale_non_uniform(f64::from(im.width) / f64::from(raster.width), f64::from(im.height) / f64::from(raster.height));
        if !(to_doc.determinant().abs() > 1e-12 && to_doc.as_coeffs().iter().all(|c| c.is_finite())) {
            return Err(bad(cmd, "the image has no size"));
        }
        Ok(Self { id: node.id, raster, to_doc })
    }

    /// The image's box in the document (points).
    pub(super) fn bounds(&self) -> kurbo::Rect {
        self.to_doc.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, f64::from(self.raster.width), f64::from(self.raster.height)))
    }

    /// How dark the image is at document point `at` (0 light or off the image or transparent, 1 black).
    fn darkness(&self, inv: kurbo::Affine, at: kurbo::Point, invert: bool, contrast: f64) -> f64 {
        let q = inv * at;
        if !(q.x >= 0.0 && q.y >= 0.0 && q.x < f64::from(self.raster.width) && q.y < f64::from(self.raster.height)) {
            return 0.0;
        }
        let [r, g, b, a] = self.raster.pixel(q.x as u32, q.y as u32);
        let luma = (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) / 255.0;
        let dark = if invert { luma } else { 1.0 - luma };
        (dark * f64::from(a) / 255.0).clamp(0.0, 1.0).powf(contrast)
    }
}

/// `n` main colours of `px` (k-means in Lab from spread-out starting colours), most used first,
/// each with how many of the pixels it stands for.
pub(super) fn main_colors(px: &[Rgb], n: usize) -> Vec<(Rgb, usize)> {
    if px.is_empty() || n == 0 {
        return Vec::new();
    }
    let labs: Vec<[f64; 3]> = px.iter().map(|c| c.to_lab()).collect();
    let dist = |a: &[f64; 3], b: &[f64; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    // starting colours: the first pixel, then each time the one farthest from those chosen
    let mut centres: Vec<[f64; 3]> = labs.first().copied().into_iter().collect();
    while centres.len() < n {
        let far = labs
            .iter()
            .map(|l| (l, centres.iter().map(|c| dist(l, c)).fold(f64::INFINITY, f64::min)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|(_, d)| *d > 1.0);
        match far {
            Some((l, _)) => centres.push(*l),
            None => break,
        }
    }
    let mut owner = vec![0usize; labs.len()];
    for _ in 0..10 {
        for (k, l) in labs.iter().enumerate() {
            let best = centres.iter().enumerate().min_by(|a, b| dist(l, a.1).total_cmp(&dist(l, b.1))).map_or(0, |(i, _)| i);
            if let Some(o) = owner.get_mut(k) {
                *o = best;
            }
        }
        let mut sums = vec![([0.0f64; 3], 0usize); centres.len()];
        for (l, o) in labs.iter().zip(&owner) {
            if let Some((s, c)) = sums.get_mut(*o) {
                s[0] += l[0];
                s[1] += l[1];
                s[2] += l[2];
                *c += 1;
            }
        }
        for (c, (s, n)) in centres.iter_mut().zip(&sums) {
            if *n > 0 {
                let k = *n as f64;
                *c = [s[0] / k, s[1] / k, s[2] / k];
            }
        }
    }
    // each centre shown as the average of its pixels in sRGB
    let mut rgb = vec![([0u64; 3], 0usize); centres.len()];
    for (c, o) in px.iter().zip(&owner) {
        if let Some((s, n)) = rgb.get_mut(*o) {
            s[0] += u64::from(c.r);
            s[1] += u64::from(c.g);
            s[2] += u64::from(c.b);
            *n += 1;
        }
    }
    let mut out: Vec<(Rgb, usize)> = rgb
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(s, n)| {
            let k = n as u64;
            (Rgb::new((s[0] / k) as u8, (s[1] / k) as u8, (s[2] / k) as u8), n)
        })
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.1));
    out
}

/// The opaque pixels of `r`, sampled down to about `max` of them.
pub(super) fn sample(r: &Raster, max: usize) -> Vec<Rgb> {
    let total = r.width as usize * r.height as usize;
    let step = ((total as f64 / max.max(1) as f64).sqrt().ceil() as u32).max(1);
    let mut out = Vec::new();
    for y in (0..r.height).step_by(step as usize) {
        for x in (0..r.width).step_by(step as usize) {
            let [cr, cg, cb, a] = r.pixel(x, y);
            if a >= 128 {
                out.push(Rgb::new(cr, cg, cb));
            }
        }
    }
    out
}

/// The design's thread chart, or the one named.
pub(super) fn chart(s: &Session, p: &Value, cmd: &str) -> Result<ThreadChart> {
    let name = match str_param(p, "chart") {
        Some(n) => n.to_string(),
        None => s.doc()?.doc.embroidery.as_deref().map(|c| c.chart.clone()).filter(|c| !c.is_empty()).unwrap_or_else(|| "Generic".into()),
    };
    super::embroidery_charts::find_chart(s, &name)
        .or_else(|| sf_core::threads::charts().into_iter().next())
        .ok_or_else(|| bad(cmd, format!("unknown thread chart `{name}`")))
}

fn whole(p: &Value, key: &str, lo: u64, hi: u64, default: u64, cmd: &str) -> Result<u64> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_u64().filter(|v| (lo..=hi).contains(v)).ok_or_else(|| bad(cmd, format!("`{key}` is {lo} to {hi}"))),
    }
}

fn number(p: &Value, key: &str, lo: f64, hi: f64, default: f64, cmd: &str) -> Result<f64> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().filter(|v| (lo..=hi).contains(v)).ok_or_else(|| bad(cmd, format!("`{key}` is {lo} to {hi}"))),
    }
}

fn image_colors(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.image.colors";
    let n = whole(p, "colors", 2, 48, 8, CMD)? as usize;
    let pic = Picture::get(s, p, CMD)?;
    let chart = chart(s, p, CMD)?;
    let px = sample(&pic.raster, 40_000);
    if px.is_empty() {
        return Err(bad(CMD, "the image is transparent"));
    }
    let total = px.len() as f64;
    let colors: Vec<Value> = main_colors(&px, n)
        .into_iter()
        .map(|(c, k)| {
            let t = chart.threads.get(chart.nearest(c));
            json!({
                "color": c.to_hex(),
                "share": ((k as f64 / total) * 1000.0).round() / 1000.0,
                "thread": t.map(|t| json!({ "color": t.color.to_hex(), "name": t.name, "code": t.code })),
            })
        })
        .collect();
    Ok(json!({ "colors": colors, "chart": chart.name }))
}

fn preview(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.image.preview";
    let n = whole(p, "colors", 2, 32, 6, CMD)? as u32;
    let size = whole(p, "size", 64, 512, 240, CMD)? as u32;
    let remove_bg = bool_or(p, "removeBackground", true);
    let pic = Picture::get(s, p, CMD)?;
    let chart = chart(s, p, CMD)?;
    // the image at most `size` pixels across (each pixel the one under its middle)
    let (rw, rh) = (pic.raster.width, pic.raster.height);
    let k = (f64::from(size) / f64::from(rw.max(rh))).min(1.0);
    let (w, h) = (((f64::from(rw) * k).round() as u32).max(1), ((f64::from(rh) * k).round() as u32).max(1));
    let small = Raster::from_fn(w, h, |x, y| {
        let sx = ((f64::from(x) + 0.5) / k) as u32;
        let sy = ((f64::from(y) + 0.5) / k) as u32;
        pic.raster.pixel(sx.min(rw - 1), sy.min(rh - 1))
    });
    // as Auto-Digitize traces it: its colour reduction, then specks merged (its noise, scaled)
    let params = vectorcraft_trace::TraceParams {
        mode: vectorcraft_trace::Mode::Color,
        palette: vectorcraft_trace::Palette::Limited,
        colors: n,
        ignore_white: remove_bg,
        ..Default::default()
    };
    let mut q = vectorcraft_trace::quantize(&small, &params);
    let noise = ((16.0 * k * k).round() as usize).max(1);
    vectorcraft_trace::denoise(&mut q.labels, w as usize, h as usize, noise);
    let total = q.labels.iter().filter(|l| **l != vectorcraft_trace::TRANSPARENT).count().max(1);
    let mut counts = vec![0usize; q.palette.len()];
    let mut edge = vec![0usize; q.palette.len()];
    let mut border = 0usize;
    for (i, l) in q.labels.iter().enumerate() {
        let (x, y) = (i % w as usize, i / w as usize);
        let on_edge = x == 0 || y == 0 || x + 1 == w as usize || y + 1 == h as usize;
        border += usize::from(on_edge);
        if let Some(c) = counts.get_mut(*l as usize) {
            *c += 1;
            if on_edge && let Some(e) = edge.get_mut(*l as usize) {
                *e += 1;
            }
        }
    }
    // left out as background: white, and the colour round most of the edges
    let backdrop = edge.iter().enumerate().max_by_key(|(_, e)| **e).filter(|(_, e)| remove_bg && **e * 10 > border * 6).map(|(i, _)| i);
    let is_bg = |i: usize| remove_bg && (backdrop == Some(i) || q.palette.get(i).is_some_and(|c| c.iter().all(|v| *v >= 245)));
    let out = Raster::from_fn(w, h, |x, y| {
        let l = q.labels.get((y * w + x) as usize).copied().unwrap_or(vectorcraft_trace::TRANSPARENT) as usize;
        match q.palette.get(l) {
            Some(c) if !is_bg(l) => [c[0], c[1], c[2], 255],
            // left out: a light checker shows through
            _ => {
                let v = if (x / 6 + y / 6) % 2 == 0 { 236 } else { 250 };
                [v, v, v, 255]
            }
        }
    });
    let mut order: Vec<usize> = (0..q.palette.len()).filter(|i| counts.get(*i).is_some_and(|c| *c > 0)).collect();
    order.sort_by_key(|i| std::cmp::Reverse(counts.get(*i).copied().unwrap_or(0)));
    let colors: Vec<Value> = order
        .iter()
        .filter_map(|i| {
            let c = q.palette.get(*i)?;
            let rgb = Rgb::new(c[0], c[1], c[2]);
            let t = chart.threads.get(chart.nearest(rgb));
            Some(json!({
                "color": rgb.to_hex(),
                "share": ((counts.get(*i).copied().unwrap_or(0) as f64 / total as f64) * 1000.0).round() / 1000.0,
                "background": is_bg(*i),
                "thread": t.map(|t| json!({ "color": t.color.to_hex(), "name": t.name, "code": t.code })),
            }))
        })
        .collect();
    let sewn = order.iter().filter(|i| !is_bg(**i)).count();
    Ok(json!({
        "dataBase64": vectorcraft_format::base64_encode(&out.encode_png()),
        "width": w, "height": h, "colors": colors, "sewn": sewn, "threadChanges": sewn.saturating_sub(1),
    }))
}

/// `r` with each channel the median of its `radius` neighbourhood (edges kept, noise gone).
fn median(r: &Raster, radius: u32) -> Raster {
    if radius == 0 {
        return r.clone();
    }
    let (w, h) = (r.width as i64, r.height as i64);
    let rad = i64::from(radius);
    Raster::from_fn(r.width, r.height, |x, y| {
        let mut ch: [Vec<u8>; 4] = Default::default();
        for dy in -rad..=rad {
            for dx in -rad..=rad {
                let (sx, sy) = ((i64::from(x) + dx).clamp(0, w - 1), (i64::from(y) + dy).clamp(0, h - 1));
                let px = r.pixel(sx as u32, sy as u32);
                for (c, v) in ch.iter_mut().zip(px) {
                    c.push(v);
                }
            }
        }
        let mut out = [0u8; 4];
        for (o, c) in out.iter_mut().zip(ch.iter_mut()) {
            c.sort_unstable();
            *o = c.get(c.len() / 2).copied().unwrap_or(0);
        }
        out
    })
}

/// Each pixel the palette colour it is nearest (index; `None` transparent).
pub(super) fn quantise(r: &Raster, palette: &[Rgb]) -> Vec<Option<usize>> {
    let labs: Vec<[f64; 3]> = palette.iter().map(|c| c.to_lab()).collect();
    let mut cache: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    r.rgba
        .chunks_exact(4)
        .map(|c| {
            let (cr, cg, cb, a) = (c.first().copied()?, c.get(1).copied()?, c.get(2).copied()?, c.get(3).copied()?);
            if a < 128 {
                return None;
            }
            let rgb = Rgb::new(cr, cg, cb);
            let key = rgb.to_u32();
            if let Some(i) = cache.get(&key) {
                return Some(*i);
            }
            let l = rgb.to_lab();
            let i = labs
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let d = |m: &[f64; 3]| (m[0] - l[0]).powi(2) + (m[1] - l[1]).powi(2) + (m[2] - l[2]).powi(2);
                    d(a.1).total_cmp(&d(b.1))
                })
                .map_or(0, |(i, _)| i);
            cache.insert(key, i);
            Some(i)
        })
        .collect()
}

/// Specks and ragged edges given their neighbours' colour: each cell the most common of its 3 × 3
/// neighbourhood when that is most of it, `passes` times.
pub(super) fn majority(cells: &mut [Option<usize>], w: usize, h: usize, passes: u32) {
    for _ in 0..passes {
        let before = cells.to_vec();
        for y in 0..h {
            for x in 0..w {
                let mut counts: Vec<(Option<usize>, u32)> = Vec::new();
                for dy in [-1i64, 0, 1] {
                    for dx in [-1i64, 0, 1] {
                        let (sx, sy) = (x as i64 + dx, y as i64 + dy);
                        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                            continue;
                        }
                        let v = before.get(sy as usize * w + sx as usize).copied().flatten();
                        match counts.iter_mut().find(|(c, _)| *c == v) {
                            Some((_, n)) => *n += 1,
                            None => counts.push((v, 1)),
                        }
                    }
                }
                if let Some((v, n)) = counts.iter().max_by_key(|(_, n)| *n)
                    && *n >= 5
                    && let Some(c) = cells.get_mut(y * w + x)
                {
                    *c = *v;
                }
            }
        }
    }
}

fn prepare(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.image.prepare";
    let n = whole(p, "colors", 0, 48, 8, CMD)? as usize;
    if n == 1 {
        return Err(bad(CMD, "`colors` is 0 (keep them all) or 2 to 48"));
    }
    let smooth = whole(p, "smooth", 0, 5, 1, CMD)? as u32;
    let passes = whole(p, "clean", 0, 5, 1, CMD)? as u32;
    let replace = bool_or(p, "replace", false);
    let pic = Picture::get(s, p, CMD)?;
    if u64::from(pic.raster.width) * u64::from(pic.raster.height) > 16 << 20 {
        return Err(bad(CMD, "the image is larger than 16 megapixels: scale it down first"));
    }
    let smoothed = median(&pic.raster, smooth);
    let (w, h) = (smoothed.width as usize, smoothed.height as usize);
    let (raster, palette) = if n == 0 {
        (smoothed, Vec::new())
    } else {
        let palette: Vec<Rgb> = main_colors(&sample(&smoothed, 40_000), n).into_iter().map(|(c, _)| c).collect();
        let mut cells = quantise(&smoothed, &palette);
        majority(&mut cells, w, h, passes);
        let out = Raster::from_fn(smoothed.width, smoothed.height, |x, y| match cells.get(y as usize * w + x as usize).copied().flatten() {
            Some(i) => palette.get(i).map_or([0, 0, 0, 0], |c| [c.r, c.g, c.b, 255]),
            None => [0, 0, 0, 0],
        });
        (out, palette)
    };
    let png = raster.encode_png();
    if png.is_empty() {
        return Err(bad(CMD, "the prepared image couldn't be written"));
    }
    let id = add_image_copy(s, pic.id, png, "sf-prepared", "Prepared Image", "Prepare Image", replace)?;
    let colors: Vec<String> = palette.iter().map(|c| c.to_hex()).collect();
    Ok(json!({ "id": id.0, "colors": colors, "width": w, "height": h }))
}

/// A copy of the image `source` showing the PNG `png` (kept under a new key starting `key`), named
/// `name`, put over it (or in its place with `replace`) and selected, as the undo step `label`.
pub(super) fn add_image_copy(s: &mut Session, source: NodeId, png: Vec<u8>, key: &str, name: &str, label: &str, replace: bool) -> Result<NodeId> {
    s.edit(label, |d, sel| {
        let node = d.node(source).cloned().ok_or(EngineError::NoNode(source))?;
        // (the caller read it as an image already)
        let NodeKind::Image(mut im) = node.kind.clone() else { return Err(EngineError::NoNode(source)) };
        let mut k = 1;
        let key = loop {
            let key = format!("{key}-{k}");
            if !d.images.contains_key(&key) {
                break key;
            }
            k += 1;
        };
        d.images.insert(key.clone(), vectorcraft_doc::ImageBlob { mime: "image/png".into(), bytes: Arc::new(png), proxy: None });
        im.key = key;
        im.link = None;
        let mut copy = node.clone();
        copy.id = d.alloc_id();
        copy.kind = NodeKind::Image(im);
        copy.name = Some(name.into());
        let id = copy.id;
        let (parent, idx, _) = d.position(source).ok_or(EngineError::NoNode(source))?;
        d.insert(parent, idx + 1, copy)?;
        if replace {
            d.remove(source)?;
        }
        sel.set([id]);
        Ok(id)
    })
}

/// A path of needle points (document points), sewn as manual stitches in `color`, named `name`.
fn manual(points: Vec<Vec<kurbo::Point>>, color: Rgb, name: &str) -> Option<vectorcraft_doc::Node> {
    use vectorcraft_geom::path::{PathData, SubPath};
    let subs: Vec<SubPath> = points.iter().filter(|l| l.len() >= 2).map(|l| SubPath::polyline(l, false)).collect();
    if subs.is_empty() {
        return None;
    }
    let c = Color::rgb8(color.r, color.g, color.b);
    let mut n = vectorcraft_doc::Node::path(NodeId(0), PathData::new(subs), Appearance::basic(Paint::None, Paint::solid(c), 0.35 * PT_PER_MM));
    let mut e = Embroidery::new(StitchType::Manual);
    e.thread = Some(sf_core::threads::Thread::new(color, ""));
    n.embroidery = Some(Box::new(e));
    n.name = Some(name.to_string());
    Some(n)
}

/// Photo lines: parallel lines `spacing` apart at `angle`, each zigzagging across itself as wide
/// as the image is dark there, joined end to end into one line of needle points.
fn lines(pic: &Picture, spacing: f64, angle: f64, invert: bool, contrast: f64) -> Result<Vec<Vec<kurbo::Point>>> {
    let inv = pic.to_doc.inverse();
    let b = pic.bounds();
    let mm = PT_PER_MM;
    let (dir, across) = {
        let a = angle.to_radians();
        (kurbo::Vec2::new(a.cos(), a.sin()), kurbo::Vec2::new(-a.sin(), a.cos()))
    };
    // the image's box measured along and across the lines
    let corners = [kurbo::Point::new(b.x0, b.y0), kurbo::Point::new(b.x1, b.y0), kurbo::Point::new(b.x0, b.y1), kurbo::Point::new(b.x1, b.y1)];
    let span = |v: kurbo::Vec2| {
        let ds: Vec<f64> = corners.iter().map(|c| c.to_vec2().dot(v)).collect();
        (ds.iter().copied().fold(f64::INFINITY, f64::min), ds.iter().copied().fold(f64::NEG_INFINITY, f64::max))
    };
    let ((a0, a1), (c0, c1)) = (span(dir), span(across));
    let (gap, step) = (spacing * mm, 0.6 * mm);
    let rows = ((c1 - c0) / gap).floor() as usize + 1;
    let cols = ((a1 - a0) / step).floor() as usize + 1;
    if rows.saturating_mul(cols) > MAX_POINTS {
        return Err(bad("embroidery.photoStitch", "the photo would take too many stitches: widen the spacing or make the image smaller"));
    }
    let mut out = Vec::new();
    let mut line: Vec<kurbo::Point> = Vec::new();
    for r in 0..rows {
        let c = c0 + gap * (r as f64 + 0.5);
        let mut row: Vec<kurbo::Point> = Vec::new();
        let mut since = 0.0;
        for k in 0..cols {
            let t = a0 + step * k as f64;
            let base = (dir * t + across * c).to_point();
            let d = pic.darkness(inv, base, invert, contrast);
            since += step;
            if d < 0.04 {
                // light: a straight run, a needle point every 2.4 mm
                if since >= 2.4 * mm || k + 1 == cols {
                    row.push(base);
                    since = 0.0;
                }
                continue;
            }
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            row.push(base + across * (side * d * gap * 0.45));
            since = 0.0;
        }
        if r % 2 == 1 {
            row.reverse();
        }
        line.extend(row);
    }
    if line.len() >= 2 {
        out.push(line);
    }
    Ok(out)
}

/// Coral: points scattered as densely as the image is dark (up to `density` a mm²), sewn as one run
/// from each to the nearest one not yet sewn; a gap longer than 5 mm starts a new run (a trim).
fn coral(pic: &Picture, density: f64, invert: bool, contrast: f64) -> Result<Vec<Vec<kurbo::Point>>> {
    let inv = pic.to_doc.inverse();
    let b = pic.bounds();
    let mm = PT_PER_MM;
    let cell = 1.0 / density.sqrt() * mm;
    let (nx, ny) = ((b.width() / cell).ceil() as usize, (b.height() / cell).ceil() as usize);
    if nx.saturating_mul(ny) > MAX_POINTS * 4 {
        return Err(bad("embroidery.photoStitch", "the photo would take too many stitches: lower the density or make the image smaller"));
    }
    // a cell holds a point as often as it is dark (a fixed hash decides, so it is the same each time)
    let hash = |x: usize, y: usize, k: u64| {
        let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ k;
        h ^= h >> 29;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 32;
        (h % 10_000) as f64 / 10_000.0
    };
    let mut pts: Vec<kurbo::Point> = Vec::new();
    for y in 0..ny {
        for x in 0..nx {
            let at = kurbo::Point::new(b.x0 + (x as f64 + hash(x, y, 1)) * cell, b.y0 + (y as f64 + hash(x, y, 2)) * cell);
            if hash(x, y, 3) < pic.darkness(inv, at, invert, contrast) {
                pts.push(at);
            }
        }
    }
    if pts.len() > MAX_POINTS {
        return Err(bad("embroidery.photoStitch", "the photo would take too many stitches: lower the density or make the image smaller"));
    }
    // nearest unsewn point next, found through a grid of buckets
    let bucket = 4.0 * mm;
    let (bw, bh) = ((b.width() / bucket).ceil() as usize + 1, (b.height() / bucket).ceil() as usize + 1);
    let key =
        |p: &kurbo::Point| (((p.x - b.x0) / bucket).max(0.0) as usize).min(bw - 1) + (((p.y - b.y0) / bucket).max(0.0) as usize).min(bh - 1) * bw;
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); bw * bh];
    for (i, p) in pts.iter().enumerate() {
        if let Some(g) = grid.get_mut(key(p)) {
            g.push(i);
        }
    }
    let mut used = vec![false; pts.len()];
    let mut runs: Vec<Vec<kurbo::Point>> = Vec::new();
    let mut run: Vec<kurbo::Point> = Vec::new();
    let mut here = match pts.first() {
        Some(p) => *p,
        None => return Ok(runs),
    };
    for _ in 0..pts.len() {
        let (cx, cy) = (key(&here) % bw, key(&here) / bw);
        let mut best: Option<(usize, f64)> = None;
        let mut ring = 0usize;
        while ring <= bw.max(bh) {
            for gy in cy.saturating_sub(ring)..=(cy + ring).min(bh - 1) {
                for gx in cx.saturating_sub(ring)..=(cx + ring).min(bw - 1) {
                    if gx.abs_diff(cx) != ring && gy.abs_diff(cy) != ring {
                        continue;
                    }
                    for &i in grid.get(gx + gy * bw).map(Vec::as_slice).unwrap_or_default() {
                        if !used.get(i).copied().unwrap_or(true)
                            && let Some(p) = pts.get(i)
                        {
                            let d = p.distance(here);
                            if best.is_none_or(|(_, bd)| d < bd) {
                                best = Some((i, d));
                            }
                        }
                    }
                }
            }
            // a point found in this ring may be beaten by one in the next only within its reach
            if let Some((_, d)) = best
                && d <= ring as f64 * bucket
            {
                break;
            }
            ring += 1;
        }
        let Some((i, d)) = best else { break };
        if let Some(u) = used.get_mut(i) {
            *u = true;
        }
        let Some(p) = pts.get(i).copied() else { break };
        if d > 5.0 * mm && run.len() >= 2 {
            runs.push(std::mem::take(&mut run));
        } else if d > 5.0 * mm {
            run.clear();
        }
        run.push(p);
        here = p;
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    Ok(runs)
}

/// Colour photo: the image on a grid of `cell` mm cells, each the nearest of its `n` main colours
/// matched to the chart's threads, specks cleaned; each colour's region one short-stitch tatami
/// fill, the lightest first, their angles turned colour to colour.
///
/// With `tones` (a ramp from light to dark) the photo is sewn by lightness alone: each cell takes
/// the ramp's colour at its lightness, and the palette is `n` steps along the ramp.
fn colour_fills(
    s: &Session,
    pic: &Picture,
    n: usize,
    cell: f64,
    chart: &ThreadChart,
    background: bool,
    tones: Option<[Rgb; 2]>,
) -> Result<Vec<vectorcraft_doc::Node>> {
    use kurbo::Shape;
    let b = pic.bounds();
    let mm = PT_PER_MM;
    let c = cell * mm;
    let (w, h) = ((b.width() / c).ceil() as usize, (b.height() / c).ceil() as usize);
    if w == 0 || h == 0 || w > MAX_GRID || h > MAX_GRID {
        return Err(bad("embroidery.photoStitch", "the photo is too large for that cell size: make the cells larger or the image smaller"));
    }
    let inv = pic.to_doc.inverse();
    // the grid's cells as a small raster
    let grid = Raster::from_fn(w as u32, h as u32, |x, y| {
        let q = inv * kurbo::Point::new(b.x0 + (f64::from(x) + 0.5) * c, b.y0 + (f64::from(y) + 0.5) * c);
        if q.x >= 0.0 && q.y >= 0.0 && q.x < f64::from(pic.raster.width) && q.y < f64::from(pic.raster.height) {
            let px = pic.raster.pixel(q.x as u32, q.y as u32);
            match tones {
                Some(ramp) => {
                    let [r, g, b, a] = px;
                    let luma = (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)) / 255.0;
                    let t = along(ramp, 1.0 - luma);
                    [t.r, t.g, t.b, a]
                }
                None => px,
            }
        } else {
            [0, 0, 0, 0]
        }
    });
    let found: Vec<Rgb> = match tones {
        Some(ramp) => (0..n).map(|i| along(ramp, i as f64 / (n - 1).max(1) as f64)).collect(),
        None => main_colors(&sample(&grid, 40_000), n).into_iter().map(|(c, _)| c).collect(),
    };
    // a tone keeps its own colour unless the chart has a thread that really is that tone (a
    // blue-grey is no grey): within 6 in Lab
    let palette: Vec<Rgb> = found
        .into_iter()
        .map(|c| match chart.threads.get(chart.nearest(c)) {
            Some(t) if tones.is_none() || lab_distance(t.color, c) <= 6.0 => t.color,
            _ => c,
        })
        .fold(Vec::new(), |mut v, c| {
            if !v.contains(&c) {
                v.push(c);
            }
            v
        });
    let mut cells = quantise(&grid, &palette);
    majority(&mut cells, w, h, 2);
    // the background: the colour covering most of the edges, left unsewn unless asked for
    let skip = if background {
        None
    } else {
        let mut edge = vec![0usize; palette.len()];
        let mut total = 0usize;
        for y in 0..h {
            for x in 0..w {
                if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                    total += 1;
                    if let Some(Some(i)) = cells.get(y * w + x)
                        && let Some(e) = edge.get_mut(*i)
                    {
                        *e += 1;
                    }
                }
            }
        }
        edge.iter().enumerate().max_by_key(|(_, n)| **n).filter(|(_, n)| **n * 2 > total).map(|(i, _)| i)
    };
    // the lightest colour sewn first
    let mut order: Vec<usize> = (0..palette.len()).collect();
    order.sort_by(|a, b| {
        let l = |i: &usize| palette.get(*i).map_or(0.0, |c| c.to_lab()[0]);
        l(b).total_cmp(&l(a))
    });
    let to_doc = kurbo::Affine::translate((b.x0, b.y0)) * kurbo::Affine::scale(c);
    let new = super::embroidery::new_stitching(s);
    let mut out = Vec::new();
    for (k, i) in order.iter().enumerate() {
        if skip == Some(*i) {
            continue;
        }
        let mask: Vec<bool> = cells.iter().map(|v| *v == Some(*i)).collect();
        let Some(path) = vectorcraft_trace::region::outline(&mask, w, h, true, 4) else { continue };
        let path = path.transformed(to_doc);
        if path.to_bezpath().area().abs() < (2.0 * mm) * (2.0 * mm) {
            continue;
        }
        let Some(color) = palette.get(*i).copied() else { continue };
        let paint = Paint::solid(Color::rgb8(color.r, color.g, color.b));
        let mut node = vectorcraft_doc::Node::path(NodeId(0), path, Appearance::basic(paint, Paint::None, 0.0));
        let mut fill = match &new[0].stitch {
            StitchType::Fill(f) => f.clone(),
            _ => FillParams::default(),
        };
        fill.angle = (k as f64 * 37.0) % 180.0;
        fill.stitch_length = 2.0;
        let mut e = Embroidery::new(StitchType::Fill(fill));
        let thread = chart.threads.iter().find(|t| t.color == color).cloned().unwrap_or_else(|| sf_core::threads::Thread::new(color, ""));
        e.thread = Some(thread);
        node.embroidery = Some(Box::new(e));
        node.name = Some(format!("Photo {}", color.to_hex()));
        out.push(node);
    }
    Ok(out)
}

/// How far apart two colours are in Lab.
fn lab_distance(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (a.to_lab(), b.to_lab());
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The colour `t` of the way (0–1) from a ramp's light end to its dark end.
fn along(ramp: [Rgb; 2], t: f64) -> Rgb {
    let t = if t.is_finite() { t.clamp(0.0, 1.0) } else { 0.0 };
    let mix = |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * t).round().clamp(0.0, 255.0) as u8;
    let [l, d] = ramp;
    Rgb::new(mix(l.r, d.r), mix(l.g, d.g), mix(l.b, d.b))
}

/// Sepia's and black and white's ramps, light to dark.
const SEPIA: [Rgb; 2] = [Rgb::new(0xf2, 0xe4, 0xc9), Rgb::new(0x3a, 0x23, 0x12)];
const MONO: [Rgb; 2] = [Rgb::new(0xf5, 0xf5, 0xf5), Rgb::new(0x14, 0x14, 0x14)];

/// The most tiles a colour tile photo stitch makes.
const MAX_TILES: usize = 60_000;

/// A tile's shape, round its centre, `size` across, shrunk by `gap` on each side.
fn tile_shape(shape: &str, at: kurbo::Point, size: f64, gap: f64) -> Vec<kurbo::Point> {
    let r = (size / 2.0 - gap).max(size * 0.1);
    let (corners, turn) = match shape {
        "diamond" => (4, 0.0),
        "hexagon" => (6, 30.0),
        _ => (4, 45.0),
    };
    // a square's corners sit r·√2 out, so that it is `size` across
    let reach = if shape == "square" {
        r * std::f64::consts::SQRT_2
    } else if shape == "hexagon" {
        r / (30f64.to_radians().cos())
    } else {
        r
    };
    (0..corners)
        .map(|k| {
            let a = (turn + 360.0 * f64::from(k) / f64::from(corners)).to_radians();
            kurbo::Point::new(at.x + reach * a.cos(), at.y + reach * a.sin())
        })
        .collect()
}

/// A colour tile's shape (square, diamond or hexagon), its size across and the gap between
/// tiles, in mm.
#[derive(Clone, Copy)]
struct Tiles<'a> {
    shape: &'a str,
    size: f64,
    gap: f64,
}

/// Colour tile: the photo as small tiles (squares, diamonds or hexagons `size` mm across), each
/// one of the photo's `n` main colours matched to the chart, every colour's tiles one short-stitch
/// fill, the lightest first.
fn colour_tiles(
    s: &Session,
    pic: &Picture,
    n: usize,
    tiles: Tiles<'_>,
    chart: &ThreadChart,
    background: bool,
) -> Result<(Vec<vectorcraft_doc::Node>, usize)> {
    const CMD: &str = "embroidery.photoStitch";
    let Tiles { shape, size, gap } = tiles;
    let b = pic.bounds();
    let mm = PT_PER_MM;
    let (sz, gp) = (size * mm, gap * mm);
    // the tiles' centres: a square grid, diamonds in rows half a tile apart, hexagons (flat sides
    // left and right) in rows 3/4 of a tile apart, every other row moved half a tile
    let (dx, dy, shift) = match shape {
        "diamond" => (sz, sz / 2.0, sz / 2.0),
        "hexagon" => (sz, sz * 3f64.sqrt() / 2.0, sz / 2.0),
        _ => (sz, sz, 0.0),
    };
    let (cols, rows) = ((b.width() / dx).ceil() as usize + 1, (b.height() / dy).ceil() as usize + 1);
    if cols.saturating_mul(rows) > MAX_TILES {
        return Err(bad(CMD, "too many tiles: make them larger or the image smaller"));
    }
    let inv = pic.to_doc.inverse();
    let colour_at = |at: kurbo::Point| -> Option<Rgb> {
        // the tile's colour: the mean of nine samples in it, if most are on the image and opaque
        let (mut sum, mut k) = ([0.0f64; 3], 0u32);
        for i in [-1.0, 0.0, 1.0] {
            for j in [-1.0, 0.0, 1.0] {
                let q = inv * kurbo::Point::new(at.x + i * sz / 4.0, at.y + j * sz / 4.0);
                if q.x >= 0.0 && q.y >= 0.0 && q.x < f64::from(pic.raster.width) && q.y < f64::from(pic.raster.height) {
                    let [r, g, bl, a] = pic.raster.pixel(q.x as u32, q.y as u32);
                    if a >= 128 {
                        sum[0] += f64::from(r);
                        sum[1] += f64::from(g);
                        sum[2] += f64::from(bl);
                        k += 1;
                    }
                }
            }
        }
        (k >= 5).then(|| {
            let m = |v: f64| (v / f64::from(k)).round().clamp(0.0, 255.0) as u8;
            Rgb::new(m(sum[0]), m(sum[1]), m(sum[2]))
        })
    };
    let mut tiles: Vec<(kurbo::Point, Rgb, bool)> = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let off = if r % 2 == 1 { shift } else { 0.0 };
            let at = kurbo::Point::new(b.x0 + c as f64 * dx + off, b.y0 + r as f64 * dy);
            if !b.contains(at) {
                continue;
            }
            if let Some(col) = colour_at(at) {
                let edge = at.x - b.x0 < sz || b.x1 - at.x < sz || at.y - b.y0 < sz || b.y1 - at.y < sz;
                tiles.push((at, col, edge));
            }
        }
    }
    let colours: Vec<Rgb> = tiles.iter().map(|t| t.1).collect();
    let palette: Vec<Rgb> =
        main_colors(&colours, n).into_iter().map(|(c, _)| chart.threads.get(chart.nearest(c)).map_or(c, |t| t.color)).fold(Vec::new(), |mut v, c| {
            if !v.contains(&c) {
                v.push(c);
            }
            v
        });
    let labs: Vec<[f64; 3]> = palette.iter().map(|c| c.to_lab()).collect();
    let nearest = |c: Rgb| {
        let l = c.to_lab();
        let d = |m: &[f64; 3]| (m[0] - l[0]).powi(2) + (m[1] - l[1]).powi(2) + (m[2] - l[2]).powi(2);
        labs.iter().enumerate().min_by(|a, b| d(a.1).total_cmp(&d(b.1))).map_or(0, |(i, _)| i)
    };
    let picked: Vec<usize> = tiles.iter().map(|t| nearest(t.1)).collect();
    let skip = if background {
        None
    } else {
        let mut edge = vec![0usize; palette.len()];
        let mut total = 0usize;
        for (t, i) in tiles.iter().zip(&picked) {
            if t.2 {
                total += 1;
                if let Some(e) = edge.get_mut(*i) {
                    *e += 1;
                }
            }
        }
        edge.iter().enumerate().max_by_key(|(_, n)| **n).filter(|(_, n)| **n * 2 > total).map(|(i, _)| i)
    };
    let mut order: Vec<usize> = (0..palette.len()).collect();
    order.sort_by(|a, b| {
        let l = |i: &usize| palette.get(*i).map_or(0.0, |c| c.to_lab()[0]);
        l(b).total_cmp(&l(a))
    });
    let new = super::embroidery::new_stitching(s);
    let mut out = Vec::new();
    let mut count = 0;
    for (k, i) in order.iter().enumerate() {
        if skip == Some(*i) {
            continue;
        }
        let mut bp = kurbo::BezPath::new();
        for (t, _) in tiles.iter().zip(&picked).filter(|(_, p)| **p == *i) {
            let pts = tile_shape(shape, t.0, sz, gp);
            let Some((first, rest)) = pts.split_first() else { continue };
            bp.move_to(*first);
            for q in rest {
                bp.line_to(*q);
            }
            bp.close_path();
            count += 1;
        }
        if bp.elements().is_empty() {
            continue;
        }
        let Some(color) = palette.get(*i).copied() else { continue };
        let paint = Paint::solid(Color::rgb8(color.r, color.g, color.b));
        let mut node =
            vectorcraft_doc::Node::path(NodeId(0), vectorcraft_geom::PathData::from_bezpath(&bp), Appearance::basic(paint, Paint::None, 0.0));
        let mut fill = match new.first().map(|n| &n.stitch) {
            Some(StitchType::Fill(f)) => f.clone(),
            _ => FillParams::default(),
        };
        fill.angle = (k as f64 * 37.0) % 180.0;
        fill.stitch_length = (size * 0.5).clamp(1.5, 3.0);
        let mut e = Embroidery::new(StitchType::Fill(fill));
        let thread = chart.threads.iter().find(|t| t.color == color).cloned().unwrap_or_else(|| sf_core::threads::Thread::new(color, ""));
        e.thread = Some(thread);
        node.embroidery = Some(Box::new(e));
        node.name = Some(format!("Tiles {}", color.to_hex()));
        out.push(node);
    }
    Ok((out, count))
}

fn photo_stitch(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.photoStitch";
    let mode = str_param(p, "mode").unwrap_or("lines");
    if !["lines", "coral", "color", "sepia", "mono", "tile"].contains(&mode) {
        return Err(bad(CMD, "`mode` is lines, coral, color, sepia, mono or tile"));
    }
    let invert = bool_or(p, "invert", false);
    let contrast = number(p, "contrast", 0.3, 3.0, 1.0, CMD)?;
    let thread = match str_param(p, "color") {
        Some(c) => Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?,
        None => Rgb::new(0, 0, 0),
    };
    let pic = Picture::get(s, p, CMD)?;
    let b = pic.bounds();
    if b.width() > 600.0 * PT_PER_MM || b.height() > 600.0 * PT_PER_MM {
        return Err(bad(CMD, "the image is larger than 600 mm: scale it down first"));
    }
    let mut tiles = None;
    let (nodes, points, colors) = match mode {
        "lines" => {
            let spacing = number(p, "spacing", 0.6, 5.0, 1.6, CMD)?;
            let angle = number(p, "angle", -360.0, 360.0, 0.0, CMD)?;
            let l = lines(&pic, spacing, angle, invert, contrast)?;
            let n: usize = l.iter().map(Vec::len).sum();
            (manual(l, thread, "Photo Lines").into_iter().collect::<Vec<_>>(), Some(n), None)
        }
        "coral" => {
            let density = number(p, "density", 0.5, 8.0, 3.0, CMD)?;
            let l = coral(&pic, density, invert, contrast)?;
            let n: usize = l.iter().map(Vec::len).sum();
            (manual(l, thread, "Photo Coral").into_iter().collect(), Some(n), None)
        }
        "tile" => {
            let n = whole(p, "colors", 2, 32, 8, CMD)? as usize;
            let shape = str_param(p, "shape").unwrap_or("square");
            if !["square", "diamond", "hexagon"].contains(&shape) {
                return Err(bad(CMD, "`shape` is square, diamond or hexagon"));
            }
            let size = number(p, "size", 2.0, 12.0, 4.0, CMD)?;
            let gap = number(p, "gap", 0.0, 1.0, 0.3, CMD)?;
            let chart = chart(s, p, CMD)?;
            let (fills, count) = colour_tiles(s, &pic, n, Tiles { shape, size, gap }, &chart, bool_or(p, "background", false))?;
            tiles = Some(count);
            let k = fills.len();
            (fills, None, Some(k))
        }
        _ => {
            let (tones, default) = match mode {
                "sepia" => (Some(SEPIA), 7),
                "mono" => (Some(MONO), 11),
                _ => (None, 6),
            };
            let n = whole(p, "colors", 2, 32, default, CMD)? as usize;
            let cell = number(p, "cell", 0.5, 5.0, 1.0, CMD)?;
            let chart = chart(s, p, CMD)?;
            let fills = colour_fills(s, &pic, n, cell, &chart, bool_or(p, "background", false), tones)?;
            let k = fills.len();
            (fills, None, Some(k))
        }
    };
    if nodes.is_empty() {
        return Err(bad(CMD, "nothing to stitch: the image is too light (try `invert` or more `contrast`)"));
    }
    let source = pic.id;
    let ids = s.edit("Photo Stitch", |d, sel| {
        let (parent, idx, _) = d.position(source).ok_or(EngineError::NoNode(source))?;
        let mut ids = Vec::new();
        for (k, mut n) in nodes.into_iter().enumerate() {
            n.id = d.alloc_id();
            ids.push(n.id);
            d.insert(parent, idx + 1 + k, n)?;
        }
        sel.set(ids.iter().copied());
        Ok(ids)
    })?;
    let ids: Vec<u64> = ids.iter().map(|i| i.0).collect();
    Ok(json!({ "ids": ids, "mode": mode, "points": points, "colors": colors, "tiles": tiles }))
}
