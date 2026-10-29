//! The design as an image (RP-04): its stitches drawn as lit thread, as the TrueView display shows
//! them (or flat), on white, a colour or transparent, written as PNG for a website, a customer or a
//! catalogue.

use serde_json::{Value, json};

use vectorcraft_doc::Document;
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, Result, Session, bad, cmd, has_doc, str_param};

/// Thread thickness of 40 wt polyester lying on fabric (mm), as the display draws it.
const THREAD_WIDTH_MM: f64 = 0.36;
/// The largest image (pixels a side, and in all).
const MAX_SIDE: u32 = 16_000;
const MAX_PIXELS: u64 = 120_000_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.exportImage",
            "Export Stitch Image",
            [],
            None,
            "{path?: a .png file to write (without it: the bytes), ppmm?: pixels a millimetre (10; 1–40), background?: \"#rrggbb\"|transparent (white), margin?: mm round the design (3; 0–100), view?: trueview|flat (trueview: lit thread as the TrueView display shows it)} the design's stitches as a PNG image → {path, format, bytes, width, height} (no path: {dataBase64, format, bytes, width, height})",
            has_doc,
            export_image
        ),
        cmd!(
            "embroidery.exportAnimation",
            "Export Sew-out Animation",
            [],
            None,
            "{path?: a .gif or .mp4 file to write (without it: the bytes), format?: gif|mp4 (the path's extension, else gif; mp4: H.264, which phones and social sites play), frames?: (60; 10–240), seconds?: how long it takes to sew (6; 2–60), hold?: seconds the finished design stays before it starts again (2; 0–10), ppmm?: pixels a millimetre (4; 1–12), background?: \"#rrggbb\" (white), margin?: mm (3; 0–50), view?: trueview|flat} the design sewing itself, stitch by stitch in sewing order as the stitch player shows it, as an animated GIF that loops or an MP4 video (for a website, a listing, a message or a social post) → {path?, dataBase64?, format, bytes, width, height, frames}",
            has_doc,
            export_animation
        ),
        cmd!(
            "embroidery.printView",
            "Print Stitch View",
            ["Embroidery"],
            None,
            "{scale?: life|fit (life: the stitches at their real size, over as many pages as it takes; fit: on one page), page?: a4|letter (a4), view?: trueview|flat} the stitch view as a document of pages to print (File › Print) or export, the stitches drawn as embroidery.exportImage draws them, 12 pixels a mm → {pages, scale}",
            has_doc,
            print_view
        ),
    ]
}

/// Light from the top left, a little towards the viewer (as the display's).
const LIGHT: [f64; 3] = [-0.45, -0.55, 0.70];

fn mix(dst: &mut [u8; 4], src: [f64; 3], a: f64) {
    // source over destination, straight alpha
    let da = f64::from(dst[3]) / 255.0;
    let oa = a + da * (1.0 - a);
    if oa <= 0.0 {
        return;
    }
    for (i, s) in src.iter().enumerate() {
        let d = f64::from(dst[i]);
        dst[i] = ((s * a + d * da * (1.0 - a)) / oa).round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (oa * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// One stitch from `a` to `b` (pixels) as thread `half` px wide each side: shaded as a lit
/// cylinder (or flat), its ends tucked in, edges anti-aliased.
fn stitch(img: &mut image::RgbaImage, a: (f64, f64), b: (f64, f64), half: f64, color: sf_core::Rgb, flat: bool) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx.hypot(dy);
    if !len.is_finite() || len < 0.01 {
        return;
    }
    let (tx, ty) = (dx / len, dy / len);
    let (nx, ny) = (-ty, tx);
    let inset = (half * 0.35).min(len * 0.2);
    let (w, h) = (img.width(), img.height());
    let lo_x = (a.0.min(b.0) - half - 1.0).floor().max(0.0) as u32;
    let lo_y = (a.1.min(b.1) - half - 1.0).floor().max(0.0) as u32;
    let hi_x = ((a.0.max(b.0) + half + 1.0).ceil().max(0.0) as u32).min(w);
    let hi_y = ((a.1.max(b.1) + half + 1.0).ceil().max(0.0) as u32).min(h);
    let base = [f64::from(color.r), f64::from(color.g), f64::from(color.b)];
    for y in lo_y..hi_y {
        for x in lo_x..hi_x {
            let (vx, vy) = (f64::from(x) + 0.5 - a.0, f64::from(y) + 0.5 - a.1);
            let along = vx * tx + vy * ty;
            let across = vx * nx + vy * ny;
            let cover_along = (along - inset + 0.5).min(len - inset - along + 0.5).clamp(0.0, 1.0);
            let cover_across = (half - across.abs() + 0.5).clamp(0.0, 1.0);
            let cover = cover_along * cover_across;
            if cover <= 0.0 {
                continue;
            }
            let rgb = if flat {
                base
            } else {
                let u = (across / half).clamp(-1.0, 1.0);
                let z = (1.0 - u * u).max(0.0).sqrt();
                let ndl = nx * u * LIGHT[0] + ny * u * LIGHT[1] + z * LIGHT[2];
                let k = 0.38 + 0.75 * ndl.max(0.0);
                let spec = (2.0 * ndl * z - LIGHT[2]).max(0.0).powf(12.0);
                let t = (spec * 0.55).min(0.6);
                base.map(|c| {
                    let lit = (c * k).clamp(0.0, 255.0);
                    if spec > 0.01 { lit + (255.0 - lit) * t } else { lit }
                })
            };
            let px = img.get_pixel_mut(x, y);
            let mut d = px.0;
            mix(&mut d, rgb, cover);
            px.0 = d;
        }
    }
}

/// The pattern drawn at `ppmm` pixels a millimetre with `margin` mm round it.
fn render(pattern: &sf_core::Pattern, ppmm: f64, margin: f64, background: [u8; 4], flat: bool) -> std::result::Result<image::RgbaImage, String> {
    render_frames(pattern, ppmm, margin, background, flat, usize::MAX, &mut |_| Ok(()))
}

/// [`render`], handing `frame` the picture so far after every `every` needle points (the
/// stitch player's view, for an animation); the finished picture is returned.
fn render_frames(
    pattern: &sf_core::Pattern,
    ppmm: f64,
    margin: f64,
    background: [u8; 4],
    flat: bool,
    every: usize,
    frame: &mut dyn FnMut(&image::RgbaImage) -> std::result::Result<(), String>,
) -> std::result::Result<image::RgbaImage, String> {
    let every = every.max(1);
    let b = pattern.bounds();
    if b.is_empty() {
        return Err("the design has no stitches".into());
    }
    let (w, h) = ((b.width() + 2.0 * margin) * ppmm, (b.height() + 2.0 * margin) * ppmm);
    if !(w.is_finite() && h.is_finite())
        || w.ceil() > f64::from(MAX_SIDE)
        || h.ceil() > f64::from(MAX_SIDE)
        || (w.ceil() * h.ceil()) as u64 > MAX_PIXELS
    {
        return Err(format!("the image would be {w:.0} × {h:.0} pixels: lower ppmm"));
    }
    let (w, h) = ((w.ceil() as u32).max(1), (h.ceil() as u32).max(1));
    let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba(background));
    let to_px = |p: sf_core::Point| ((p.x - b.min.x + margin) * ppmm, (p.y - b.min.y + margin) * ppmm);
    let half = (THREAD_WIDTH_MM * ppmm * 0.5).max(0.5);
    let (mut thread, mut prev, mut sewing) = (0usize, None, false);
    let mut color = pattern.thread(0).color;
    for (i, st) in pattern.stitches.iter().enumerate() {
        if i > 0 && i % every == 0 {
            frame(&img)?;
        }
        let p = to_px(st.pos());
        match st.cmd {
            sf_core::Cmd::Stitch => {
                if let Some(a) = prev
                    && sewing
                {
                    stitch(&mut img, a, p, half, color, flat);
                }
                prev = Some(p);
                sewing = true;
            }
            sf_core::Cmd::Jump => {
                prev = Some(p);
                sewing = false;
            }
            sf_core::Cmd::Trim => sewing = false,
            sf_core::Cmd::ColorChange => {
                thread += 1;
                color = pattern.thread(thread).color;
                sewing = false;
            }
            sf_core::Cmd::Stop | sf_core::Cmd::End => {}
        }
    }
    Ok(img)
}

/// The most pixels an animation's frames hold together.
const MAX_ANIMATION_PIXELS: u64 = 400_000_000;

fn export_animation(s: &mut Session, p: &Value) -> Result<Value> {
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Frame};
    const CMD: &str = "embroidery.exportAnimation";
    let num = |key: &str, default: f64, range: std::ops::RangeInclusive<f64>, what: &str| -> Result<f64> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_f64().filter(|v| range.contains(v)).ok_or_else(|| bad(CMD, what.to_string())),
        }
    };
    let frames = num("frames", 60.0, 10.0..=240.0, "`frames` is 10 to 240")?.round() as usize;
    let seconds = num("seconds", 6.0, 2.0..=60.0, "`seconds` is 2 to 60")?;
    let hold = num("hold", 2.0, 0.0..=10.0, "`hold` is 0 to 10 seconds")?;
    let ppmm = num("ppmm", 4.0, 1.0..=12.0, "`ppmm` is 1 to 12")?;
    let margin = num("margin", 3.0, 0.0..=50.0, "`margin` is 0 to 50 mm")?;
    let background = match str_param(p, "background") {
        None => [255, 255, 255, 255],
        Some(c) => {
            let c = sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`background` is \"#rrggbb\""))?;
            [c.r, c.g, c.b, 255]
        }
    };
    let flat = match str_param(p, "view").unwrap_or("trueview") {
        "trueview" => false,
        "flat" => true,
        _ => return Err(bad(CMD, "`view` is trueview or flat")),
    };
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    if plan.is_empty() {
        return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type first"));
    }
    let pattern = plan.pattern();
    let b = pattern.bounds();
    let px = ((b.width() + 2.0 * margin) * ppmm).ceil() * ((b.height() + 2.0 * margin) * ppmm).ceil();
    if !px.is_finite() || px * frames as f64 > MAX_ANIMATION_PIXELS as f64 {
        return Err(bad(CMD, "the animation would be too large: lower ppmm or frames"));
    }
    // gif, or mp4: from `format`, else the path's extension
    let path = str_param(p, "path").map(str::trim).filter(|p| !p.is_empty());
    let format = match str_param(p, "format") {
        Some(f) => f.to_ascii_lowercase(),
        None => path.and_then(|p| p.rsplit_once('.')).map(|(_, e)| e.to_ascii_lowercase()).filter(|e| e == "mp4").unwrap_or_else(|| "gif".into()),
    };
    if format != "gif" && format != "mp4" {
        return Err(bad(CMD, "`format` is gif or mp4"));
    }
    // every frame the same share of the needle points, the last one the finished design
    let every = pattern.stitches.len().div_ceil(frames).max(1);
    let frame_ms = (seconds * 1000.0 / frames as f64).round().max(20.0) as u32;
    let hold_ms = (hold * 1000.0).round().max(20.0) as u32;
    let mut bytes = Vec::new();
    let (width, height, count) = if format == "mp4" {
        let mut video = super::embroidery_mp4::Mp4::new(1000.0 / f64::from(frame_ms));
        let mut count = 0usize;
        // the picture in whole 16-pixel macroblocks, the design in the middle
        let mut add = |img: &image::RgbaImage, ms: u32| -> std::result::Result<(), String> {
            count += 1;
            let (w, h) = (img.width().div_ceil(16) * 16, img.height().div_ceil(16) * 16);
            let mut padded = image::RgbaImage::from_pixel(w, h, image::Rgba(background));
            image::imageops::overlay(&mut padded, img, i64::from((w - img.width()) / 2), i64::from((h - img.height()) / 2));
            video.frame(&padded, ms)
        };
        let done = render_frames(pattern, ppmm, margin, background, flat, every, &mut |img| add(img, frame_ms)).map_err(|e| bad(CMD, e))?;
        add(&done, hold_ms).map_err(EngineError::Other)?;
        bytes = video.finish().map_err(EngineError::Other)?;
        (done.width().div_ceil(16) * 16, done.height().div_ceil(16) * 16, count)
    } else {
        let delay = Delay::from_numer_denom_ms(frame_ms, 1);
        let mut enc = GifEncoder::new_with_speed(&mut bytes, 10);
        enc.set_repeat(Repeat::Infinite).map_err(|e| EngineError::Other(format!("GIF encoding failed: {e}")))?;
        let mut count = 0usize;
        let mut add = |img: &image::RgbaImage, delay: Delay| -> std::result::Result<(), String> {
            count += 1;
            enc.encode_frame(Frame::from_parts(img.clone(), 0, 0, delay)).map_err(|e| format!("GIF encoding failed: {e}"))
        };
        let done = render_frames(pattern, ppmm, margin, background, flat, every, &mut |img| add(img, delay)).map_err(|e| bad(CMD, e))?;
        add(&done, Delay::from_numer_denom_ms(hold_ms, 1)).map_err(EngineError::Other)?;
        (done.width(), done.height(), count)
    };
    let mut r = json!({ "format": format, "bytes": bytes.len(), "width": width, "height": height, "frames": count });
    match path {
        Some(path) => {
            super::fileio::write_file(path, &bytes)?;
            r["path"] = json!(path);
        }
        None => r["dataBase64"] = json!(vectorcraft_format::base64_encode(&bytes)),
    }
    Ok(r)
}

fn export_image(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.exportImage";
    let num = |key: &str, default: f64, range: std::ops::RangeInclusive<f64>, what: &str| -> Result<f64> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_f64().filter(|v| range.contains(v)).ok_or_else(|| bad(CMD, what.to_string())),
        }
    };
    let ppmm = num("ppmm", 10.0, 1.0..=40.0, "`ppmm` is 1 to 40")?;
    let margin = num("margin", 3.0, 0.0..=100.0, "`margin` is 0 to 100 mm")?;
    let background = match str_param(p, "background") {
        None => [255, 255, 255, 255],
        Some("transparent") => [255, 255, 255, 0],
        Some(c) => {
            let c = sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`background` is \"#rrggbb\" or transparent"))?;
            [c.r, c.g, c.b, 255]
        }
    };
    let flat = match str_param(p, "view").unwrap_or("trueview") {
        "trueview" => false,
        "flat" => true,
        _ => return Err(bad(CMD, "`view` is trueview or flat")),
    };
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    if plan.is_empty() {
        return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type first"));
    }
    let img = render(plan.pattern(), ppmm, margin, background, flat).map_err(|e| bad(CMD, e))?;
    let (width, height) = (img.width(), img.height());
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| EngineError::Other(format!("PNG encoding failed: {e}")))?;
    match str_param(p, "path").map(str::trim).filter(|p| !p.is_empty()) {
        Some(path) => {
            super::fileio::write_file(path, &bytes)?;
            Ok(json!({ "path": path, "format": "png", "bytes": bytes.len(), "width": width, "height": height }))
        }
        None => Ok(json!({
            "dataBase64": vectorcraft_format::base64_encode(&bytes),
            "format": "png",
            "bytes": bytes.len(),
            "width": width,
            "height": height,
        })),
    }
}

/// The stitch view as pages to print: the design's stitches as an image, life size (tiled over
/// as many pages as it takes) or fitted to one page.
fn print_view(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.printView";
    let (pw, ph) = match str_param(p, "page").unwrap_or("a4") {
        "a4" | "A4" => (595.0, 842.0),
        "letter" | "Letter" => (612.0, 792.0),
        _ => return Err(bad(CMD, "`page` is a4 or letter")),
    };
    let fit = match str_param(p, "scale").unwrap_or("life") {
        "life" => false,
        "fit" => true,
        _ => return Err(bad(CMD, "`scale` is life or fit")),
    };
    let view = str_param(p, "view").unwrap_or("trueview");
    let margin = 36.0;
    let img = s.execute("embroidery.exportImage", &json!({ "ppmm": 12, "margin": 2, "view": view }))?;
    let (iw, ih) = (img["width"].as_f64().unwrap_or(0.0), img["height"].as_f64().unwrap_or(0.0));
    if iw <= 0.0 || ih <= 0.0 {
        return Err(bad(CMD, "nothing to print"));
    }
    // the image's size on paper (pt): 12 pixels a mm
    let (aw, ah) = (pw - 2.0 * margin, ph - 2.0 * margin);
    let life = (iw / 12.0 * PT_PER_MM, ih / 12.0 * PT_PER_MM);
    let (w, h) = if fit || (life.0 <= aw && life.1 <= ah) {
        let k = (aw / life.0).min(ah / life.1).min(if fit { f64::INFINITY } else { 1.0 });
        (life.0 * k, life.1 * k)
    } else {
        life
    };
    let (cols, rows) = ((w / aw).ceil().max(1.0) as usize, (h / ah).ceil().max(1.0) as usize);
    if cols * rows > 64 {
        return Err(bad(CMD, "more than 64 pages: use scale fit"));
    }
    let title = super::fileio::file_stem(&s.doc()?.doc.title);
    let mut doc = Document::new(pw, ph);
    doc.title = format!("{title} print");
    // the pages in a grid, touching, so the image runs across them
    for r in 0..rows {
        for c in 0..cols {
            if r == 0 && c == 0 {
                continue;
            }
            let id = doc.next_artboard_id();
            let (x, y) = (c as f64 * aw, r as f64 * ah);
            doc.artboards.push(vectorcraft_doc::Artboard {
                id,
                name: format!("Page {}", r * cols + c + 1),
                rect: vectorcraft_geom::Rect::new(x - margin, y - margin, x - margin + pw, y - margin + ph),
                show_center_mark: false,
                show_cross_hairs: false,
                locked: false,
                show_video_safe_areas: false,
                video_ruler_pixel_aspect_ratio: 1.0,
                color: None,
            });
        }
    }
    if let Some(a) = doc.artboards.first_mut() {
        a.rect = vectorcraft_geom::Rect::new(-margin, -margin, pw - margin, ph - margin);
    }
    let pages = doc.artboards.len();
    s.add_document(doc, None);
    // the image centred on a single page, else from the first page's corner
    let (x, y) = if pages == 1 { ((aw - w) / 2.0, (ah - h) / 2.0) } else { (0.0, 0.0) };
    s.execute("file.place", &json!({ "name": format!("{title}.png"), "dataBase64": img["dataBase64"], "rect": [x, y, w, h] }))?;
    s.execute("select.none", &json!({}))?;
    Ok(json!({ "pages": pages, "scale": if (w - life.0).abs() < 0.5 { 1.0 } else { (w / life.0 * 1000.0).round() / 1000.0 } }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_animation_sews_the_design_frame_by_frame_and_loops() {
        let mut s = Session::new();
        s.execute("embroidery.sample", &json!({"id": "heart-patch"})).unwrap();
        let r = s.execute("embroidery.exportAnimation", &json!({"frames": 12, "ppmm": 2})).unwrap();
        assert_eq!(r["format"], "gif");
        let bytes = vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap();
        assert!(bytes.starts_with(b"GIF89a") && bytes.windows(11).any(|w| w == b"NETSCAPE2.0"), "a looping GIF");
        let frames = r["frames"].as_u64().unwrap();
        assert!((12..=14).contains(&frames), "{frames}");
        // the frames decode, the first nearly empty and the last the finished design
        let dec = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
        let all: Vec<_> = image::AnimationDecoder::into_frames(dec).collect_frames().unwrap();
        assert_eq!(all.len() as u64, frames);
        let ink = |f: &image::Frame| f.buffer().pixels().filter(|p| p.0[0] < 200 || p.0[1] < 200 || p.0[2] < 200).count();
        assert!(ink(&all[0]) < ink(all.last().unwrap()) / 3, "it fills in as it sews");
        // as an MP4: H.264 frames in one video track, whole macroblocks
        let r = s.execute("embroidery.exportAnimation", &json!({"frames": 12, "ppmm": 2, "format": "mp4"})).unwrap();
        assert_eq!(r["format"], "mp4");
        assert!(r["width"].as_u64().unwrap().is_multiple_of(16) && r["height"].as_u64().unwrap().is_multiple_of(16), "{r}");
        let mp4 = vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap();
        assert_eq!(&mp4[4..12], b"ftypisom");
        assert!(mp4.windows(4).any(|w| w == b"avcC"));
        assert!(s.execute("embroidery.exportAnimation", &json!({"format": "avi"})).is_err());
        for bad in [json!({"frames": 2}), json!({"ppmm": 50}), json!({"view": "x"}), json!({"seconds": 1})] {
            assert!(s.execute("embroidery.exportAnimation", &bad).is_err(), "{bad}");
        }
    }
}
