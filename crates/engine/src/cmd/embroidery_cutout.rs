//! Remove Background (BG-01): the subject of a photo cut out from what is behind it, so that only
//! the subject is digitized. The method is the one Rother, Kolmogorov and Blake published as
//! "GrabCut" (SIGGRAPH 2004): colour models of the subject and of the background (Gaussian
//! mixtures) and a graph cut that splits the pixels between them, both refined in turn; the
//! pixels outside a box round the subject, and any marked "remove", are background, those marked
//! "keep" are the subject. The cut is a minimum cut of the pixel grid found by maximum flow
//! (Dinic's algorithm). It works on a copy of the photo at most [`WORK_SIDE`] pixels across, and
//! the mask is scaled back up with its edge smoothed.

use serde_json::{Value, json};
use vectorcraft_trace::Raster;

use super::embroidery_photo::{Picture, add_image_copy};
use super::{CommandSpec, Result, Session, bad, bool_or, cmd, f64_or, has_doc};

/// The share of each side of the image the default box leaves out, as background.
const MARGIN: f64 = 0.02;
/// The most keep or remove marks a call takes.
const MAX_MARKS: usize = 2000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.image.removeBackground",
            "Remove Background",
            ["Embroidery"],
            None,
            "{id?: an image (the selected one), box?: [x, y, width, height] round the subject in fractions of the image (the whole image less 2 % round the edge; that edge is then only taken for background at first, as the subject may run off it), keep?: [[x, y, radius?]] parts that are the subject, remove?: [[x, y, radius?]] parts that are background (fractions of the image; the radius of the image's shorter side, 0.03), replace?: bool (false: the cut-out copy goes over the photo, which stays), sensitivity?: 0–100 (50: below keeps more of the subject, above takes more of the background away)} the subject of a photo cut out from its background (GrabCut: colour models of the subject and the background and a graph cut between them), the background made transparent so Auto-Digitize sews only the subject; the copy is selected → {id, kept: the share of the image kept, subject: whether Select Subject found it, width, height}",
            has_doc,
            remove_background
        ),
        cmd!(
            "embroidery.image.refineCutout",
            "Refine Cut-Out",
            [],
            None,
            "{id?: a cut-out image (the selected one), keep?: [[x, y]] points of a stroke over parts to keep, remove?: [[x, y]] points of a stroke over parts to take away (document points), radius?: pt (6: the brush's half width), sensitivity?: 0–100 (50)} a cut-out made again with the strokes: what they cover is the subject or the background for certain, the rest is cut again from its colours (the cut-out's background is still there, only transparent); the image is replaced, as one step → {id, kept}",
            has_doc,
            refine_cutout
        ),
        cmd!(
            query "embroidery.image.cutoutPreview",
            "Cut-Out Preview",
            [],
            None,
            "{id?: an image (the selected one), sensitivity?: 0–100 (50), size?: px 64–512 (240)} what Remove Background would keep, on a small copy of the photo, to compare before and after while choosing the sensitivity → {dataBase64: the cut-out (PNG with its background transparent), beforeBase64: the photo (PNG), width, height, kept, subject}",
            has_doc,
            cutout_preview
        ),
    ]
}

/// `raster` scaled down to at most `side` pixels across (box average; transparency kept).
fn shrunk(raster: &Raster, side: u32) -> Raster {
    let (w, h) = (raster.width.max(1), raster.height.max(1));
    let k = (f64::from(w.max(h)) / f64::from(side.max(1))).max(1.0);
    let (sw, sh) = (((f64::from(w) / k).round() as u32).max(1), ((f64::from(h) / k).round() as u32).max(1));
    Raster::from_fn(sw, sh, |x, y| {
        let (x0, y0) = ((f64::from(x) * k) as u32, (f64::from(y) * k) as u32);
        let (x1, y1) = ((((f64::from(x) + 1.0) * k) as u32).clamp(x0 + 1, w), (((f64::from(y) + 1.0) * k) as u32).clamp(y0 + 1, h));
        let (mut sum, mut n) = ([0u32; 4], 0u32);
        for yy in y0..y1 {
            for xx in x0..x1 {
                let at = ((yy * w + xx) * 4) as usize;
                if let Some(px) = raster.rgba.get(at..at + 4) {
                    for (s, v) in sum.iter_mut().zip(px) {
                        *s += u32::from(*v);
                    }
                    n += 1;
                }
            }
        }
        sum.map(|s| u8::try_from(s / n.max(1)).unwrap_or(255))
    })
}

fn cutout_preview(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.image.cutoutPreview";
    let bias = keep_bias(p, CMD)?;
    let side = p.get("size").and_then(Value::as_u64).unwrap_or(240);
    if !(64..=512).contains(&side) {
        return Err(bad(CMD, "`size` is 64 to 512 px"));
    }
    let pic = Picture::get(s, p, CMD)?;
    if u64::from(pic.raster.width) * u64::from(pic.raster.height) > 16 << 20 {
        return Err(bad(CMD, "the image is larger than 16 megapixels: scale it down first"));
    }
    let small = shrunk(&pic.raster, u32::try_from(side).unwrap_or(240));
    let (alpha, subject) = cut_out(&small, None, &[], &[], bias).ok_or_else(|| bad(CMD, "the image is empty"))?;
    let kept = alpha.iter().filter(|a| **a >= 128).count() as f64 / alpha.len().max(1) as f64;
    let mut rgba = small.rgba.clone();
    for (px, a) in rgba.chunks_exact_mut(4).zip(&alpha) {
        if let Some(v) = px.get_mut(3) {
            *v = *a;
        }
    }
    let after = Raster { width: small.width, height: small.height, rgba }.encode_png();
    Ok(json!({
        "dataBase64": vectorcraft_format::base64_encode(&after),
        "beforeBase64": vectorcraft_format::base64_encode(&small.encode_png()),
        "width": small.width, "height": small.height, "kept": kept, "subject": subject,
    }))
}

/// The most points a refining stroke takes.
const MAX_STROKE: usize = 20_000;

/// A stroke's points (document points) from `key`.
fn stroke(p: &Value, key: &str, cmd: &str) -> Result<Vec<kurbo::Point>> {
    let Some(v) = p.get(key).filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let list = v.as_array().ok_or_else(|| bad(cmd, format!("`{key}` is a list of [x, y] points")))?;
    if list.len() > MAX_STROKE {
        return Err(bad(cmd, format!("`{key}` has more than {MAX_STROKE} points")));
    }
    list.iter()
        .map(|q| match q.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()).as_deref() {
            Some([x, y]) if x.is_finite() && y.is_finite() => Ok(kurbo::Point::new(*x, *y)),
            _ => Err(bad(cmd, format!("each of `{key}` is [x, y]"))),
        })
        .collect()
}

fn refine_cutout(s: &mut Session, p: &Value) -> Result<Value> {
    use kurbo::ParamCurveNearest;
    const CMD: &str = "embroidery.image.refineCutout";
    let keep = stroke(p, "keep", CMD)?;
    let remove = stroke(p, "remove", CMD)?;
    if keep.is_empty() && remove.is_empty() {
        return Err(bad(CMD, "paint over what to keep or take away"));
    }
    let radius = f64_or(p, "radius", 6.0);
    if !(radius.is_finite() && radius > 0.0 && radius <= 1000.0) {
        return Err(bad(CMD, "`radius` is 0 to 1000 pt"));
    }
    let pic = Picture::get(s, p, CMD)?;
    let (w, h) = (pic.raster.width, pic.raster.height);
    if u64::from(w) * u64::from(h) > 16 << 20 {
        return Err(bad(CMD, "the image is larger than 16 megapixels: scale it down first"));
    }
    // the strokes in the image's pixels: the points, and the brush's radius
    let inv = pic.to_doc.inverse();
    let px = |l: &[kurbo::Point]| -> Vec<kurbo::Point> { l.iter().map(|q| inv * *q).collect() };
    let (keep, remove) = (px(&keep), px(&remove));
    let scale = pic.to_doc.determinant().abs().sqrt();
    let r = radius / scale.max(1e-9);
    // a point is under a stroke when it is within the radius of one of its segments
    let under = |l: &[kurbo::Point], q: kurbo::Point| -> bool {
        match l {
            [one] => one.distance(q) <= r,
            _ => l.windows(2).any(|w| kurbo::Line::new(w[0], w[1]).nearest(q, 1e-6).distance_sq <= r * r),
        }
    };
    let (fw, fh) = (f64::from(w), f64::from(h));
    let (wu, hu) = (w as usize, h as usize);
    let alpha_at = |x: usize, y: usize| pic.raster.rgba.get((y * wu + x) * 4 + 3).copied().unwrap_or(0);
    let hint = |fx: f64, fy: f64| {
        let q = kurbo::Point::new(fx * fw, fy * fh);
        if under(&remove, q) {
            Hint::Background
        } else if under(&keep, q) {
            Hint::Subject
        } else if alpha_at(((fx * fw) as usize).min(wu.saturating_sub(1)), ((fy * fh) as usize).min(hu.saturating_sub(1))) >= 128 {
            Hint::Unknown
        } else {
            Hint::LikelyBackground
        }
    };
    // the whole picture, its background (transparent in the cut-out) seen again
    let mut opaque = pic.raster.rgba.clone();
    for px in opaque.chunks_exact_mut(4) {
        if let Some(a) = px.get_mut(3) {
            *a = 255;
        }
    }
    let alpha = subject_alpha(&opaque, w, h, &hint, keep_bias(p, CMD)?).ok_or_else(|| bad(CMD, "the image is empty"))?;
    let kept = alpha.iter().filter(|a| **a >= 128).count() as f64 / alpha.len().max(1) as f64;
    if kept <= 0.0 {
        return Err(bad(CMD, "that would take everything away"));
    }
    let mut rgba = pic.raster.rgba.clone();
    for (px, a) in rgba.chunks_exact_mut(4).zip(&alpha) {
        if let Some(v) = px.get_mut(3) {
            *v = *a;
        }
    }
    let png = Raster { width: w, height: h, rgba }.encode_png();
    if png.is_empty() {
        return Err(bad(CMD, "the cut-out image couldn't be written"));
    }
    let id = add_image_copy(s, pic.id, png, "sf-cutout", "Cut-Out Image", "Refine Cut-Out", true)?;
    Ok(json!({ "id": id.0, "kept": kept }))
}

/// The subject's coverage per pixel (0–255, times the photo's own alpha) by Select Subject:
/// saliency seeding a GrabCut and the edge re-cut at full size (`sf_photo`, adapted from
/// PhotoCraft); `None` when it finds no subject.
fn select_subject(raster: &Raster) -> Option<Vec<u8>> {
    use sf_photo::segment::{ImageSampler, RgbImage};
    let (w, h) = (raster.width as usize, raster.height as usize);
    let rgba: Vec<[f32; 4]> = raster.rgba.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]].map(|c| f32::from(c) / 255.0)).collect();
    if rgba.len() != w * h || w == 0 || h == 0 {
        return None;
    }
    let img = RgbImage::from_rgba(&rgba, w, h);
    let canvas = sf_photo::Rect::new(0, 0, i32::try_from(w).ok()?, i32::try_from(h).ok()?);
    let region = sf_photo::segment::subject::select_subject(&ImageSampler { img: &img, origin: (0, 0) }, canvas)?;
    let mut alpha = vec![0u8; w * h];
    for (y, a_row) in alpha.chunks_exact_mut(w).enumerate() {
        for (x, a) in a_row.iter_mut().enumerate() {
            let cover = region.at(i32::try_from(x).ok()?, i32::try_from(y).ok()?);
            let opaque = raster.rgba.get((y * w + x) * 4 + 3).copied().unwrap_or(0);
            *a = (cover.clamp(0.0, 1.0) * f32::from(opaque)).round() as u8;
        }
    }
    alpha.iter().any(|a| *a >= 128).then_some(alpha)
}

/// A box as fractions of the image, `[x, y, width, height]`.
fn fraction_box(p: &Value, cmd: &str) -> Result<Option<[f64; 4]>> {
    let Some(v) = p.get("box").filter(|v| !v.is_null()) else { return Ok(None) };
    let nums: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let [x, y, w, h] = nums.as_slice() else { return Err(bad(cmd, "`box` is [x, y, width, height], fractions of the image")) };
    let ok = [*x, *y, *w, *h].iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)) && *w >= 0.02 && *h >= 0.02;
    if !ok {
        return Err(bad(cmd, "`box` is [x, y, width, height], fractions of the image from 0 to 1"));
    }
    Ok(Some([*x, *y, *w, *h]))
}

/// Marks: `[[x, y, radius?]]` as fractions of the image.
fn marks(p: &Value, key: &str, cmd: &str) -> Result<Vec<(f64, f64, f64)>> {
    let Some(v) = p.get(key).filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let list = v.as_array().ok_or_else(|| bad(cmd, format!("`{key}` is a list of [x, y, radius?]")))?;
    if list.len() > MAX_MARKS {
        return Err(bad(cmd, format!("`{key}` has more than {MAX_MARKS} marks")));
    }
    list.iter()
        .map(|m| {
            let nums: Vec<f64> = m.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let (x, y, r) = match nums.as_slice() {
                [x, y] => (*x, *y, 0.03),
                [x, y, r] => (*x, *y, *r),
                _ => return Err(bad(cmd, format!("each of `{key}` is [x, y, radius?]"))),
            };
            if ![x, y, r].iter().all(|v| v.is_finite()) || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) || !(r > 0.0 && r <= 0.5) {
                return Err(bad(cmd, format!("`{key}`: x and y are fractions of the image, the radius 0 to 0.5")));
            }
            Ok((x, y, r))
        })
        .collect()
}

/// The cut-out's alpha for `raster` and whether Select Subject found the subject: from the box
/// (`None`: the whole image, its edge only taken for background at first, and Select Subject
/// helping), the keep and remove marks, and `bias` (see [`subject_alpha`]).
fn cut_out(raster: &Raster, given: Option<[f64; 4]>, keep: &[(f64, f64, f64)], remove: &[(f64, f64, f64)], bias: f64) -> Option<(Vec<u8>, bool)> {
    let (w, h) = (raster.width, raster.height);
    let [bx, by, bw, bh] = given.unwrap_or([MARGIN, MARGIN, 1.0 - 2.0 * MARGIN, 1.0 - 2.0 * MARGIN]);
    let outside = if given.is_some() { Hint::Background } else { Hint::LikelyBackground };
    // a mark is round on the image, its radius a fraction of the image's shorter side
    let (fw, fh) = (f64::from(w), f64::from(h));
    let side = fw.min(fh);
    let near = |list: &[(f64, f64, f64)], fx: f64, fy: f64| list.iter().any(|(x, y, r)| ((fx - x) * fw).hypot((fy - y) * fh) <= r * side);
    let hint = |fx: f64, fy: f64| {
        if near(remove, fx, fy) {
            Hint::Background
        } else if near(keep, fx, fy) {
            Hint::Subject
        } else if fx >= bx && fx <= bx + bw && fy >= by && fy <= by + bh {
            Hint::Unknown
        } else {
            outside
        }
    };
    let cut = subject_alpha(&raster.rgba, w, h, &hint, bias)?;
    // given nothing to go on, Select Subject looks for the subject too, and the cut-out keeps what
    // either finds: Select Subject keeps parts that look like the backdrop (a dark beak against
    // dark leaves), the cut from the photo's edge keeps a subject running off it (a body off the
    // bottom), and each mostly errs by leaving parts out
    let found = if given.is_none() && keep.is_empty() && remove.is_empty() { select_subject(raster) } else { None };
    // a "subject" that fills nearly the whole frame, or next to none of it, is no subject found
    let share = |a: &[u8]| a.iter().filter(|v| **v >= 128).count() as f64 / a.len().max(1) as f64;
    let found = found.filter(|a| (0.005..=0.9).contains(&share(a)));
    let subject = found.is_some();
    let alpha: Vec<u8> = match found {
        // the cut from the edge that kept nearly everything found no background: the subject alone
        // (a shaded ball on a soft gradient); unless a low sensitivity asked to keep more
        Some(a) if share(&cut) > 0.9 && bias <= 0.0 => a,
        Some(a) => a.iter().zip(&cut).map(|(a, b)| (*a).max(*b)).collect(),
        None => cut,
    };
    Some((alpha, subject))
}

/// The most a sensitivity of 0 or 100 tips a pixel towards the subject or the background.
const MAX_BIAS: f64 = 4.0;

/// `sensitivity` (0–100, 50 even) as a keep bias: below 50 keeps more, above takes more away.
fn keep_bias(p: &Value, cmd: &str) -> Result<f64> {
    let s = f64_or(p, "sensitivity", 50.0);
    if !(0.0..=100.0).contains(&s) {
        return Err(bad(cmd, "`sensitivity` is 0 to 100"));
    }
    Ok((50.0 - s) / 50.0 * MAX_BIAS)
}

fn remove_background(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.image.removeBackground";
    // without a box, the edge of the photo is taken for background at first, but not for certain
    let given = fraction_box(p, CMD)?;
    let keep = marks(p, "keep", CMD)?;
    let remove = marks(p, "remove", CMD)?;
    let replace = bool_or(p, "replace", false);
    let pic = Picture::get(s, p, CMD)?;
    let (w, h) = (pic.raster.width, pic.raster.height);
    if u64::from(w) * u64::from(h) > 16 << 20 {
        return Err(bad(CMD, "the image is larger than 16 megapixels: scale it down first"));
    }
    let (alpha, subject) = cut_out(&pic.raster, given, &keep, &remove, keep_bias(p, CMD)?).ok_or_else(|| bad(CMD, "the image is empty"))?;
    let kept = alpha.iter().filter(|a| **a >= 128).count() as f64 / alpha.len().max(1) as f64;
    if kept <= 0.0 {
        return Err(bad(CMD, "nothing in the box looks different from the background: draw the box round the subject, or mark it to keep"));
    }
    let mut rgba = pic.raster.rgba.clone();
    for (px, a) in rgba.chunks_exact_mut(4).zip(&alpha) {
        if let Some(v) = px.get_mut(3) {
            *v = *a;
        }
    }
    let png = Raster { width: w, height: h, rgba }.encode_png();
    if png.is_empty() {
        return Err(bad(CMD, "the cut-out image couldn't be written"));
    }
    let id = add_image_copy(s, pic.id, png, "sf-cutout", "Cut-Out Image", "Remove Background", replace)?;
    Ok(json!({ "id": id.0, "kept": kept, "subject": subject, "width": w, "height": h }))
}

/// The longest side the cut is worked out at (pixels): finer is slower and changes little.
pub const WORK_SIDE: u32 = 240;
/// Gaussians in each colour model.
const COMPONENTS: usize = 5;
/// Rounds of fitting the models and cutting.
const ROUNDS: usize = 5;
/// How strongly neighbours of like colour stay on the same side (the paper's γ).
const GAMMA: f64 = 50.0;
/// A pixel's tie to the side it is fixed to.
const FIXED: f64 = 1e9;

/// What a pixel is known to be before the cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hint {
    /// Background for certain (outside the box, or marked remove).
    Background,
    /// The subject for certain (marked keep).
    Subject,
    /// To be decided (inside the box).
    Unknown,
    /// To be decided, first taken for background (the edge of a photo given no box: the subject
    /// may run off it, as a pet's body off the bottom).
    LikelyBackground,
}

/// One Gaussian of a colour model.
#[derive(Clone, Debug)]
struct Gaussian {
    weight: f64,
    mean: [f64; 3],
    /// The inverse covariance, and the log of the covariance's determinant.
    inv: [[f64; 3]; 3],
    log_det: f64,
}

impl Gaussian {
    /// −log of the weighted density at `c`, up to a constant.
    fn cost(&self, c: &[f64; 3]) -> f64 {
        let d = [c[0] - self.mean[0], c[1] - self.mean[1], c[2] - self.mean[2]];
        let mut q = 0.0;
        for (i, di) in d.iter().enumerate() {
            for (j, dj) in d.iter().enumerate() {
                q += di * self.inv[i][j] * dj;
            }
        }
        -self.weight.max(1e-12).ln() + 0.5 * self.log_det + 0.5 * q
    }
}

/// A colour model: a few Gaussians.
#[derive(Clone, Debug, Default)]
struct Model {
    parts: Vec<Gaussian>,
}

impl Model {
    /// The cost of colour `c` under the model: its best Gaussian's.
    fn cost(&self, c: &[f64; 3]) -> f64 {
        self.parts.iter().map(|g| g.cost(c)).fold(f64::INFINITY, f64::min).min(1e6)
    }
    /// The Gaussian `c` belongs to.
    fn nearest(&self, c: &[f64; 3]) -> usize {
        self.parts.iter().enumerate().min_by(|a, b| a.1.cost(c).total_cmp(&b.1.cost(c))).map_or(0, |(i, _)| i)
    }
    /// A model fitted to `colors`, split between the Gaussians by `labels` (one per colour).
    fn fit(colors: &[[f64; 3]], labels: &[usize]) -> Model {
        let total = colors.len().max(1) as f64;
        let mut parts = Vec::new();
        for k in 0..COMPONENTS {
            let mine: Vec<&[f64; 3]> = colors.iter().zip(labels).filter(|(_, l)| **l == k).map(|(c, _)| c).collect();
            if mine.len() < 2 {
                continue;
            }
            let n = mine.len() as f64;
            let mut mean = [0.0; 3];
            for c in &mine {
                for i in 0..3 {
                    mean[i] += c[i] / n;
                }
            }
            let mut cov = [[0.0; 3]; 3];
            for c in &mine {
                for i in 0..3 {
                    for j in 0..3 {
                        cov[i][j] += (c[i] - mean[i]) * (c[j] - mean[j]) / n;
                    }
                }
            }
            // a little added to the diagonal keeps a flat patch of colour invertible
            for (i, row) in cov.iter_mut().enumerate() {
                row[i] += 1e-2;
            }
            let Some((inv, det)) = invert(&cov) else { continue };
            parts.push(Gaussian { weight: n / total, mean, inv, log_det: det.max(1e-12).ln() });
        }
        Model { parts }
    }
    /// A first model of `colors`: split along its widest channel into the Gaussians (in order of
    /// that channel), then fitted.
    fn first(colors: &[[f64; 3]]) -> (Model, Vec<usize>) {
        if colors.is_empty() {
            return (Model::default(), Vec::new());
        }
        let spread = |i: usize| {
            let (lo, hi) = colors.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), c| (lo.min(c[i]), hi.max(c[i])));
            hi - lo
        };
        let axis = (0..3).max_by(|a, b| spread(*a).total_cmp(&spread(*b))).unwrap_or(0);
        let mut order: Vec<usize> = (0..colors.len()).collect();
        order.sort_by(|a, b| colors.get(*a).map_or(0.0, |c| c[axis]).total_cmp(&colors.get(*b).map_or(0.0, |c| c[axis])));
        let mut labels = vec![0; colors.len()];
        for (rank, i) in order.iter().enumerate() {
            if let Some(l) = labels.get_mut(*i) {
                *l = (rank * COMPONENTS / colors.len()).min(COMPONENTS - 1);
            }
        }
        // a few rounds of k-means on those groups
        let mut model = Model::fit(colors, &labels);
        for _ in 0..4 {
            for (c, l) in colors.iter().zip(labels.iter_mut()) {
                *l = model.nearest(c);
            }
            model = Model::fit(colors, &labels);
        }
        (model, labels)
    }
}

/// The inverse of a 3×3 matrix and its determinant, unless it is singular.
fn invert(m: &[[f64; 3]; 3]) -> Option<([[f64; 3]; 3], f64)> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let mut inv = [[0.0; 3]; 3];
    for (i, row) in inv.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            let (a, b) = ((j + 1) % 3, (j + 2) % 3);
            let (c, d) = ((i + 1) % 3, (i + 2) % 3);
            *v = (m[a][c] * m[b][d] - m[a][d] * m[b][c]) / det;
        }
    }
    Some((inv, det))
}

/// A flow network for the cut: node 0 is the source (the subject), node 1 the sink (the
/// background), and pixel `i` is node `i + 2`.
struct Network {
    head: Vec<usize>,
    to: Vec<usize>,
    cap: Vec<f64>,
    next: Vec<usize>,
}

const NONE: usize = usize::MAX;

impl Network {
    fn new(nodes: usize, edges: usize) -> Self {
        Network {
            head: vec![NONE; nodes],
            to: Vec::with_capacity(edges * 2),
            cap: Vec::with_capacity(edges * 2),
            next: Vec::with_capacity(edges * 2),
        }
    }
    /// An edge `a → b` of capacity `ab` and its reverse of capacity `ba`.
    fn add(&mut self, a: usize, b: usize, ab: f64, ba: f64) {
        for (from, to, c) in [(a, b, ab), (b, a, ba)] {
            let Some(h) = self.head.get_mut(from) else { return };
            self.to.push(to);
            self.cap.push(c.max(0.0));
            self.next.push(*h);
            *h = self.to.len() - 1;
        }
    }
    /// Push the maximum flow from node 0 to node 1 (Dinic's algorithm); afterwards the nodes
    /// still reachable from the source are its side of the cut.
    fn max_flow(&mut self) {
        let n = self.head.len();
        let mut level = vec![usize::MAX; n];
        let mut iter = vec![NONE; n];
        let mut queue = Vec::with_capacity(n);
        loop {
            // levels by breadth-first search over edges with capacity left
            level.iter_mut().for_each(|l| *l = usize::MAX);
            level[0] = 0;
            queue.clear();
            queue.push(0);
            let mut qi = 0;
            while let Some(&u) = queue.get(qi) {
                qi += 1;
                let mut e = self.head[u];
                while e != NONE {
                    let v = self.to[e];
                    if self.cap[e] > 1e-9 && level[v] == usize::MAX {
                        level[v] = level[u] + 1;
                        queue.push(v);
                    }
                    e = self.next[e];
                }
            }
            if level[1] == usize::MAX {
                return;
            }
            iter.copy_from_slice(&self.head);
            // blocking flow by depth-first search along rising levels (iterative)
            loop {
                let pushed = self.augment(&level, &mut iter);
                if pushed <= 1e-9 {
                    break;
                }
            }
        }
    }
    /// One augmenting path along the level graph, and the flow pushed along it.
    fn augment(&mut self, level: &[usize], iter: &mut [usize]) -> f64 {
        let mut path: Vec<usize> = Vec::new(); // edges
        let mut u = 0;
        loop {
            if u == 1 {
                let f = path.iter().map(|&e| self.cap[e]).fold(f64::INFINITY, f64::min);
                for &e in &path {
                    self.cap[e] -= f;
                    self.cap[e ^ 1] += f;
                }
                return f;
            }
            let mut advanced = false;
            while iter[u] != NONE {
                let e = iter[u];
                let v = self.to[e];
                if self.cap[e] > 1e-9 && level[v] == level[u] + 1 {
                    path.push(e);
                    u = v;
                    advanced = true;
                    break;
                }
                iter[u] = self.next[e];
            }
            if !advanced {
                // a dead end: never try it again this round, and step back
                level_dead(iter, u);
                let Some(e) = path.pop() else { return 0.0 };
                u = self.to[e ^ 1];
                iter[u] = self.next[iter[u]];
            }
        }
    }
    /// The nodes the source still reaches.
    fn source_side(&self) -> Vec<bool> {
        let mut seen = vec![false; self.head.len()];
        let mut stack = vec![0];
        seen[0] = true;
        while let Some(u) = stack.pop() {
            let mut e = self.head[u];
            while e != NONE {
                let v = self.to[e];
                if self.cap[e] > 1e-9 && !seen[v] {
                    seen[v] = true;
                    stack.push(v);
                }
                e = self.next[e];
            }
        }
        seen
    }
}

fn level_dead(iter: &mut [usize], u: usize) {
    if let Some(i) = iter.get_mut(u) {
        *i = NONE;
    }
}

/// The four neighbours a pixel is linked to ahead of it (right, down, down-right, down-left),
/// with their distance.
const AHEAD: [(i64, i64, f64); 4] = [(1, 0, 1.0), (0, 1, 1.0), (1, 1, std::f64::consts::SQRT_2), (-1, 1, std::f64::consts::SQRT_2)];

/// The subject of an RGBA image `w`×`h` cut out from its background (`keep_bias`: how much more a
/// pixel's colour must look like the background's to be taken for it, 0 even, more keeps more): an alpha value per pixel
/// (255 the subject, 0 the background, between them along the edge). `hint` tells what a point
/// is known to be, at (x, y) as fractions of the image's width and height. Pixels already
/// transparent are background. `None` when the image is empty or its size doesn't match.
pub fn subject_alpha(rgba: &[u8], w: u32, h: u32, hint: &dyn Fn(f64, f64) -> Hint, keep_bias: f64) -> Option<Vec<u8>> {
    let (wu, hu) = (w as usize, h as usize);
    if w == 0 || h == 0 || rgba.len() != wu * hu * 4 {
        return None;
    }
    // the working copy: a box average down to WORK_SIDE across
    let k = (f64::from(w.max(h)) / f64::from(WORK_SIDE)).max(1.0);
    let (sw, sh) = (((f64::from(w) / k).ceil() as usize).max(1), ((f64::from(h) / k).ceil() as usize).max(1));
    let mut colors = vec![[0.0f64; 3]; sw * sh];
    let mut hints = vec![Hint::Unknown; sw * sh];
    for sy in 0..sh {
        for sx in 0..sw {
            let (x0, x1) = (((sx as f64 * k) as usize).min(wu - 1), (((sx + 1) as f64 * k).ceil() as usize).clamp(1, wu));
            let (y0, y1) = (((sy as f64 * k) as usize).min(hu - 1), (((sy + 1) as f64 * k).ceil() as usize).clamp(1, hu));
            let (mut sum, mut n, mut clear) = ([0.0; 3], 0.0, 0usize);
            for y in y0..y1.max(y0 + 1) {
                for x in x0..x1.max(x0 + 1) {
                    let Some(px) = rgba.get((y * wu + x) * 4..(y * wu + x) * 4 + 4) else { continue };
                    if px[3] < 128 {
                        clear += 1;
                        continue;
                    }
                    for i in 0..3 {
                        sum[i] += f64::from(px[i]);
                    }
                    n += 1.0;
                }
            }
            let i = sy * sw + sx;
            if n > 0.0 {
                colors[i] = [sum[0] / n, sum[1] / n, sum[2] / n];
            }
            hints[i] =
                if n == 0.0 || clear as f64 > n { Hint::Background } else { hint((sx as f64 + 0.5) / sw as f64, (sy as f64 + 0.5) / sh as f64) };
        }
    }
    let small = cut(&colors, &hints, sw, sh, keep_bias);
    let small = tidy(small, sw, sh);
    // scaled back up, the edge smoothed over about a working pixel
    let mut alpha = vec![0u8; wu * hu];
    for y in 0..hu {
        for x in 0..wu {
            let (fx, fy) = ((x as f64 + 0.5) / k - 0.5, (y as f64 + 0.5) / k - 0.5);
            let (ix, iy) = (fx.floor(), fy.floor());
            let (tx, ty) = (fx - ix, fy - iy);
            let at = |dx: f64, dy: f64| {
                let (cx, cy) = ((ix + dx).clamp(0.0, (sw - 1) as f64) as usize, (iy + dy).clamp(0.0, (sh - 1) as f64) as usize);
                if small.get(cy * sw + cx).copied().unwrap_or(false) { 1.0 } else { 0.0 }
            };
            let v = at(0.0, 0.0) * (1.0 - tx) * (1.0 - ty) + at(1.0, 0.0) * tx * (1.0 - ty) + at(0.0, 1.0) * (1.0 - tx) * ty + at(1.0, 1.0) * tx * ty;
            // a sharper edge than plain blending: the middle of the ramp only
            let v = ((v - 0.5) * 2.0 + 0.5).clamp(0.0, 1.0);
            let opaque = rgba.get((y * wu + x) * 4 + 3).copied().unwrap_or(0);
            if let Some(a) = alpha.get_mut(y * wu + x) {
                *a = (v * f64::from(opaque)).round() as u8;
            }
        }
    }
    Some(alpha)
}

/// The cut of the working copy: which pixels are the subject.
fn cut(colors: &[[f64; 3]], hints: &[Hint], w: usize, h: usize, keep_bias: f64) -> Vec<bool> {
    let mut subject: Vec<bool> = hints.iter().map(|h| !matches!(h, Hint::Background | Hint::LikelyBackground)).collect();
    if !hints.iter().any(|h| matches!(h, Hint::Unknown | Hint::LikelyBackground)) || !subject.contains(&true) || subject.iter().all(|s| *s) {
        return subject;
    }
    // how alike neighbours are, on average (the paper's β)
    let (mut total, mut count) = (0.0, 0.0);
    let d2 = |a: &[f64; 3], b: &[f64; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    let neighbour = |i: usize, (dx, dy): (i64, i64)| -> Option<usize> {
        let (x, y) = ((i % w) as i64 + dx, (i / w) as i64 + dy);
        (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h).then(|| y as usize * w + x as usize)
    };
    for i in 0..colors.len() {
        for (dx, dy, _) in AHEAD {
            if let Some(j) = neighbour(i, (dx, dy)) {
                total += d2(&colors[i], &colors[j]);
                count += 1.0;
            }
        }
    }
    let beta = if total > 0.0 { count / (2.0 * total) } else { 0.0 };
    let mut links: Vec<(usize, usize, f64)> = Vec::with_capacity(colors.len() * 4);
    for i in 0..colors.len() {
        for (dx, dy, dist) in AHEAD {
            if let Some(j) = neighbour(i, (dx, dy)) {
                links.push((i, j, GAMMA * (-beta * d2(&colors[i], &colors[j])).exp() / dist));
            }
        }
    }
    let side = |subject: &[bool], want: bool| -> Vec<[f64; 3]> { colors.iter().zip(subject).filter(|(_, s)| **s == want).map(|(c, _)| *c).collect() };
    let (mut fg, _) = Model::first(&side(&subject, true));
    let (mut bg, _) = Model::first(&side(&subject, false));
    for round in 0..ROUNDS {
        if round > 0 {
            for (model, want) in [(&mut fg, true), (&mut bg, false)] {
                let mine = side(&subject, want);
                if mine.len() < 2 {
                    continue;
                }
                let labels: Vec<usize> = mine.iter().map(|c| model.nearest(c)).collect();
                let fitted = Model::fit(&mine, &labels);
                if !fitted.parts.is_empty() {
                    *model = fitted;
                }
            }
        }
        if fg.parts.is_empty() || bg.parts.is_empty() {
            break;
        }
        let mut net = Network::new(colors.len() + 2, colors.len() * 2 + links.len());
        for (i, (c, hnt)) in colors.iter().zip(hints).enumerate() {
            // the source side is the subject: a cut source edge means "background", so it costs
            // the background model's fit, and the sink edge the subject's
            let (to_bg, to_fg) = match hnt {
                Hint::Subject => (FIXED, 0.0),
                Hint::Background => (0.0, FIXED),
                Hint::Unknown | Hint::LikelyBackground => (bg.cost(c) + keep_bias, fg.cost(c)),
            };
            let lo = to_bg.min(to_fg);
            net.add(0, i + 2, to_bg - lo, 0.0);
            net.add(i + 2, 1, to_fg - lo, 0.0);
        }
        for &(i, j, wgt) in &links {
            net.add(i + 2, j + 2, wgt, wgt);
        }
        net.max_flow();
        let reached = net.source_side();
        let next: Vec<bool> = (0..colors.len()).map(|i| reached.get(i + 2).copied().unwrap_or(false)).collect();
        if next == subject {
            break;
        }
        subject = next;
    }
    subject
}

/// The cut tidied: only the subject's large pieces (at least a twentieth of the largest), and
/// the small holes in it (under a hundredth of the image) filled.
fn tidy(mut subject: Vec<bool>, w: usize, h: usize) -> Vec<bool> {
    let pieces = |mask: &[bool], want: bool| -> (Vec<usize>, Vec<(usize, bool)>) {
        // a piece number per pixel (usize::MAX when not `want`), and each piece's size and
        // whether it touches the image's edge
        let mut id = vec![usize::MAX; mask.len()];
        let mut info = Vec::new();
        for start in 0..mask.len() {
            if mask[start] != want || id[start] != usize::MAX {
                continue;
            }
            let n = info.len();
            let (mut size, mut edge) = (0usize, false);
            let mut stack = vec![start];
            id[start] = n;
            while let Some(i) = stack.pop() {
                size += 1;
                let (x, y) = (i % w, i / w);
                edge |= x == 0 || y == 0 || x + 1 == w || y + 1 == h;
                let mut near = Vec::with_capacity(4);
                if x > 0 {
                    near.push(i - 1);
                }
                if x + 1 < w {
                    near.push(i + 1);
                }
                if y > 0 {
                    near.push(i - w);
                }
                if y + 1 < h {
                    near.push(i + w);
                }
                for j in near {
                    if mask[j] == want && id[j] == usize::MAX {
                        id[j] = n;
                        stack.push(j);
                    }
                }
            }
            info.push((size, edge));
        }
        (id, info)
    };
    let (id, info) = pieces(&subject, true);
    let largest = info.iter().map(|p| p.0).max().unwrap_or(0);
    for (i, s) in subject.iter_mut().enumerate() {
        if *s && id.get(i).and_then(|p| info.get(*p)).is_some_and(|p| p.0 * 20 < largest) {
            *s = false;
        }
    }
    let (id, info) = pieces(&subject, false);
    let small = subject.len() / 100;
    for (i, s) in subject.iter_mut().enumerate() {
        if !*s && id.get(i).and_then(|p| info.get(*p)).is_some_and(|p| !p.1 && p.0 < small) {
            *s = true;
        }
    }
    subject
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A red disc on a noisy green and blue background is cut out from a box round it, and a
    /// part marked "keep" that looks like the background stays.
    #[test]
    fn the_subject_is_cut_from_its_background() {
        let (w, h) = (120u32, 90u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (f64::from(x) - 60.0, f64::from(y) - 45.0);
                let n = ((x * 7 + y * 13) % 23) as u8;
                let px = if dx * dx + dy * dy <= 25.0 * 25.0 {
                    if (55..65).contains(&x) && (25..32).contains(&y) { [40 + n, 150 + n, 60] } else { [210 + n / 2, 40 + n, 30] }
                } else if x < 60 {
                    [40 + n, 150 + n, 60]
                } else {
                    [30, 60 + n, 170 + n]
                };
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        let boxed = |fx: f64, fy: f64| if (0.15..0.85).contains(&fx) && (0.1..0.9).contains(&fy) { Hint::Unknown } else { Hint::Background };
        let a = subject_alpha(&rgba, w, h, &boxed, 0.0).unwrap();
        let at = |x: u32, y: u32| a[(y * w + x) as usize];
        assert_eq!(at(60, 50), 255, "the disc's middle is kept");
        assert_eq!(at(70, 60), 255);
        assert_eq!(at(5, 5), 0, "outside the box");
        assert_eq!(at(25, 45), 0, "inside the box, but background");
        assert_eq!(at(95, 45), 0);
        let cut: usize = a.iter().filter(|v| **v >= 128).count();
        let disc = (std::f64::consts::PI * 25.0 * 25.0) as usize;
        assert!(cut.abs_diff(disc) < disc / 8, "{cut} pixels kept, the disc has {disc}");
        // the green patch on the disc is a hole in it, filled (it is small and inside)
        assert_eq!(at(60, 28), 255, "the patch on the disc is the disc's");
        // empty and mismatched images are refused
        assert!(subject_alpha(&[], 0, 0, &boxed, 0.0).is_none());
        assert!(subject_alpha(&rgba[4..], w, h, &boxed, 0.0).is_none());
        // a transparent image is all background
        let clear = vec![0u8; (w * h * 4) as usize];
        assert!(subject_alpha(&clear, w, h, &boxed, 0.0).unwrap().iter().all(|v| *v == 0));
    }

    #[test]
    fn a_marked_part_stays_and_a_removed_part_goes() {
        // a blue square left and a red square right, both in the box: marks decide
        let (w, h) = (80u32, 40u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let px = if !(5..35).contains(&y) || !(5..75).contains(&x) {
                    [240, 240, 235]
                } else if x < 40 {
                    [30, 50, 200]
                } else {
                    [200, 40, 40]
                };
                rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
            }
        }
        let hint = |fx: f64, fy: f64| {
            if !(0.03..0.97).contains(&fx) || !(0.05..0.95).contains(&fy) || ((0.6..0.8).contains(&fx) && (0.4..0.6).contains(&fy)) {
                Hint::Background
            } else if (0.2..0.3).contains(&fx) && (0.4..0.6).contains(&fy) {
                Hint::Subject
            } else {
                Hint::Unknown
            }
        };
        let a = subject_alpha(&rgba, w, h, &hint, 0.0).unwrap();
        let at = |x: u32, y: u32| a[(y * w + x) as usize];
        assert_eq!(at(20, 20), 255, "the kept square");
        assert_eq!(at(55, 20), 0, "the square marked remove goes, all of it");
    }
}
