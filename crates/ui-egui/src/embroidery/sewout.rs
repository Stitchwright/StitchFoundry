//! Check My Sew-out (Embroidery and Help menus): photos of a finished sew-out, the design that was
//! sewn when it is open, and a few answers about the sewing (machine, fabric, stabilizer, needle,
//! what happened) go to Claude, which says what went wrong and why and offers to fix the design.
//! The check runs as an Ask About Embroidery conversation in its own mode ([`ask::begin_check`]):
//! the answer, the changes to allow and the questions after it are in that window.
//!
//! Photos are turned upright (a phone's orientation tag), made no larger than Claude looks at and
//! sent as JPEG ([`prepare`]). On the desktop they are picked in an open dialog; on the web the
//! browser's picker hands them in through the place inbox ([`take_arrivals`]).

use egui::RichText;
use serde_json::{Value, json};
use vectorcraft_embroidery::sf_core::ai::sewout::{self, Answers};
use vectorcraft_embroidery::sf_core::ai::{self, ArtImage};

use super::{ask, prompt};
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The thumbnails' longest side (px).
const THUMB: u32 = 96;
/// The largest photo file read (bytes).
const MAX_FILE: usize = 60_000_000;
/// The largest photo decoded (pixels a side).
const MAX_SIDE: u32 = 20_000;

/// A photo ready to send.
pub struct Photo {
    pub name: String,
    pub image: ArtImage,
    /// Its size as sent (px).
    pub size: (u32, u32),
    thumb: egui::ColorImage,
    texture: Option<egui::TextureHandle>,
}

/// The window between frames.
#[derive(Default)]
pub struct SewOutState {
    pub open: bool,
    pub photos: Vec<Photo>,
    /// The stabilizer chosen ([`STABILIZERS`]), the fabric (`None`: as the design has it).
    pub stabilizer: usize,
    pub fabric: Option<String>,
    pub needle: String,
    /// What happened while it sewed ([`HAPPENED`]).
    pub happened: Vec<bool>,
    pub notes: String,
    /// Send the design open now with the photos.
    pub include_design: bool,
    /// The web's picker is open for photos: the files it hands in are photos.
    pub waiting: bool,
    pub error: Option<String>,
}

/// The stabilizers to choose from: (sent to Claude, shown).
fn stabilizers() -> [(&'static str, &'static str); 6] {
    [
        ("not sure", tl!("Not sure")),
        ("tear-away", tl!("Tear-away")),
        ("cut-away", tl!("Cut-away")),
        ("no-show mesh", tl!("No-show mesh")),
        ("wash-away", tl!("Wash-away")),
        ("none", tl!("None")),
    ]
}

/// What can happen while a design sews: (sent to Claude, shown).
fn happened() -> [(&'static str, &'static str); 6] {
    [
        ("the top thread broke", tl!("The top thread broke")),
        ("the bobbin ran out", tl!("The bobbin ran out")),
        ("the bobbin thread didn't cut, or caught", tl!("The bobbin thread didn't cut, or caught")),
        ("the hoop was knocked or the fabric slipped", tl!("The hoop was knocked or the fabric slipped")),
        ("the needle broke", tl!("The needle broke")),
        ("the machine stopped with a message", tl!("The machine stopped with a message")),
    ]
}

/// `ui.checkSewout {photos?: [path]}`: opens the window, with the photos at those paths added.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    if !prompt::available(app) {
        return Err("Check My Sew-out needs the desktop app for now".into());
    }
    prompt::load_key(app);
    let st = &mut app.embroidery.sewout;
    if !st.open && st.photos.is_empty() {
        st.include_design = true;
    }
    st.open = true;
    st.happened.resize(happened().len(), false);
    let paths: Vec<String> = p.get("photos").and_then(Value::as_array).into_iter().flatten().filter_map(|v| v.as_str().map(str::to_string)).collect();
    read_photos(app, paths);
    Ok(json!({ "open": true, "photos": app.embroidery.sewout.photos.len(), "error": app.embroidery.sewout.error }))
}

/// A photo's bytes as Claude is sent them: upright, no larger than it looks at, as JPEG; with its
/// thumbnail. An error says why it can't be read.
pub fn prepare(bytes: &[u8]) -> Result<(ArtImage, (u32, u32), egui::ColorImage), String> {
    use image::ImageDecoder;
    if bytes.len() > MAX_FILE {
        return Err("This photo is too large: send a smaller copy.".into());
    }
    let unreadable =
        || "This photo can't be read: save or send it as a JPEG (on an iPhone: Settings › Camera › Formats › Most Compatible).".to_string();
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|_| unreadable())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| unreadable())?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(|_| unreadable())?;
    img.apply_orientation(orientation);
    let side = ai::IMAGE_SIDE;
    if img.width() > side || img.height() > side {
        img = img.resize(side, side, image::imageops::FilterType::Triangle);
    }
    let rgb = img.to_rgb8();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&rgb).map_err(|e| e.to_string())?;
    let thumb = img.thumbnail(THUMB, THUMB).to_rgba8();
    let thumb = egui::ColorImage::from_rgba_unmultiplied([thumb.width() as usize, thumb.height() as usize], thumb.as_raw());
    let image = ArtImage { media_type: "image/jpeg", data_base64: vectorcraft_format::base64_encode(&jpeg) };
    Ok((image, (rgb.width(), rgb.height()), thumb))
}

/// Add the photo `bytes` (file `name`); why not, in the window.
pub fn add_photo(app: &mut VectorcraftApp, name: &str, bytes: &[u8]) {
    let st = &mut app.embroidery.sewout;
    if st.photos.len() >= sewout::MAX_PHOTOS {
        st.error = Some("At most 6 photos: remove one first.".into());
        return;
    }
    match prepare(bytes) {
        Ok((image, size, thumb)) => {
            st.photos.push(Photo { name: name.to_string(), image, size, thumb, texture: None });
            st.error = None;
        }
        // (the message alone: it is shown translated)
        Err(e) => {
            log::info!("{name}: {e}");
            st.error = Some(e);
        }
    }
}

/// Add Photos…: picked in an open dialog (desktop), or in the browser's picker (web; they arrive
/// through [`take_arrivals`]).
fn add_photos(app: &mut VectorcraftApp) {
    if let Some(start) = app.services.place_async.as_mut() {
        start();
        app.embroidery.sewout.waiting = true;
        return;
    }
    let paths = crate::picks::open_many(app);
    read_photos(app, paths);
}

/// The photos at `paths` added (desktop).
fn read_photos(app: &mut VectorcraftApp, paths: Vec<String>) {
    for path in paths {
        let read = match app.services.read.as_ref() {
            Some(read) => read(&path),
            None => Err("no files here".into()),
        };
        let name = std::path::Path::new(&path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
        match read {
            Ok(bytes) => add_photo(app, &name, &bytes),
            Err(e) => {
                log::info!("{name}: {e}");
                app.embroidery.sewout.error = Some(e);
            }
        }
    }
}

/// Web: the files the browser's picker handed in while it was open for photos → the ones taken
/// (the rest go on to Place).
pub fn take_arrivals(app: &mut VectorcraftApp, arrived: Vec<crate::place::PlaceArrival>) -> Vec<crate::place::PlaceArrival> {
    if !app.embroidery.sewout.waiting {
        return arrived;
    }
    let mut rest = Vec::new();
    let mut took = false;
    for a in arrived {
        if a.drop.is_none() {
            add_photo(app, &a.name, &a.bytes);
            took = true;
        } else {
            rest.push(a);
        }
    }
    if took {
        app.embroidery.sewout.waiting = false;
    }
    rest
}

/// How the design open now should look, drawn from its stitches (as JPEG, like the photos: lit
/// thread makes a large PNG; no larger than Claude looks at); `None` without stitches.
fn expected_look(app: &mut VectorcraftApp) -> Option<ArtImage> {
    let info = app.session.execute("embroidery.info", &json!({})).ok()?;
    if info["stitches"].as_u64().unwrap_or(0) == 0 {
        return None;
    }
    let longest = info["widthMm"].as_f64().unwrap_or(100.0).max(info["heightMm"].as_f64().unwrap_or(100.0)).max(1.0) + 6.0;
    let ppmm = (f64::from(ai::IMAGE_SIDE) / longest).clamp(1.0, 10.0);
    let r = app.session.execute("embroidery.exportImage", &json!({ "ppmm": ppmm, "background": "#ffffff", "margin": 3 })).ok()?;
    let png = vectorcraft_format::base64_decode(r["dataBase64"].as_str()?)?;
    prepare(&png).ok().map(|(image, _, _)| image)
}

/// Check (the window's button, `ui.checkSewout {check: true}` in tests): the photos and the
/// answers go to Claude, and the Ask window shows the answer.
pub fn check(app: &mut VectorcraftApp) -> Result<(), String> {
    let has_doc = app.session.active().is_some();
    let design = app.embroidery.sewout.include_design && has_doc;
    let machine =
        app.session.execute("embroidery.myMachine", &json!({})).ok().and_then(|m| m["machine"].as_str().map(str::to_string)).unwrap_or_default();
    let fabric = match app.embroidery.sewout.fabric.clone() {
        Some(f) => f,
        None if design => {
            app.session.execute("embroidery.info", &json!({})).ok().and_then(|i| i["fabric"].as_str().map(str::to_string)).unwrap_or_default()
        }
        None => String::new(),
    };
    let st = &app.embroidery.sewout;
    let stabilizer = stabilizers().get(st.stabilizer).map(|s| s.0).unwrap_or("not sure").to_string();
    let happened: Vec<String> = happened().iter().zip(&st.happened).filter(|(_, on)| **on).map(|((en, _), _)| en.to_string()).collect();
    let answers = Answers { machine, fabric, stabilizer, needle: st.needle.clone(), happened, notes: st.notes.clone() };
    let photos: Vec<ArtImage> = st.photos.iter().map(|p| p.image.clone()).collect();
    let n = photos.len();
    let (expected, summary) = if design { (expected_look(app), ask::design_summary(app)) } else { (None, None) };
    let message = match sewout::first_message(&photos, expected.as_ref(), summary.as_deref(), &answers) {
        Ok(m) => m,
        Err(e) => {
            let m = e.to_string();
            app.embroidery.sewout.error = Some(m.clone());
            return Err(m);
        }
    };
    let shown = crate::i18n::fmt(tl!("My sew-out ({n} photos)"), &[("n", &n.to_string())]);
    app.embroidery.sewout.open = false;
    ask::begin_check(app, message, shown)
}

/// The window, while open.
pub fn window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.sewout.open {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut add = false;
    let mut remove = None;
    let mut go = false;
    let fabrics: Vec<String> = app
        .session
        .execute("embroidery.info", &json!({}))
        .ok()
        .and_then(|i| i["fabrics"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()))
        .unwrap_or_default();
    let has_doc = app.session.active().is_some();
    egui::Window::new(tl!("Check My Sew-out"))
        .id(egui::Id::new("check-my-sewout"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(250.0, 300.0))
        .show(ctx, |ui| {
            ui.set_width(500.0);
            let st = &mut app.embroidery.sewout;
            ui.label(RichText::new(tl!("Photos of your finished sew-out")).strong().size(15.0));
            ui.label(
                RichText::new(tl!("The front, the back, and close-ups of anything that looks wrong. A ruler or a coin in the photo shows the size."))
                    .size(12.0)
                    .color(t.text_dim),
            );
            ui.horizontal_wrapped(|ui| {
                for (i, p) in st.photos.iter_mut().enumerate() {
                    ui.vertical(|ui| {
                        let tex = p.texture.get_or_insert_with(|| ui.ctx().load_texture(format!("sewout-{i}-{}", p.name), p.thumb.clone(), Default::default()));
                        ui.image((tex.id(), tex.size_vec2()));
                        if ui.small_button(tl!("Remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
            ui.horizontal(|ui| {
                let full = st.photos.len() >= sewout::MAX_PHOTOS;
                if ui.add_enabled(!full, egui::Button::new(tl!("Add Photos…"))).clicked() {
                    add = true;
                }
                if st.waiting {
                    ui.label(RichText::new(tl!("Choose the photos in the browser's window…")).size(12.0).color(t.text_dim));
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(tl!("About the sewing")).strong());
            egui::Grid::new("sewout-answers").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label(tl!("Fabric"));
                let mut options: Vec<&str> = vec![tl!("As the design has it")];
                options.extend(fabrics.iter().map(|f| crate::guide::fabric_name(f)));
                let cur = st.fabric.as_ref().and_then(|f| fabrics.iter().position(|x| x == f)).map_or(0, |i| i + 1);
                if let Some(i) = crate::widgets::dropdown_names(ui, "sewout-fabric", options.get(cur).copied().unwrap_or_default(), &options, 240.0) {
                    st.fabric = i.checked_sub(1).and_then(|k| fabrics.get(k).cloned());
                }
                ui.end_row();
                ui.label(tl!("Stabilizer"));
                let stab = stabilizers();
                let shown: Vec<&str> = stab.iter().map(|s| s.1).collect();
                if let Some(i) = crate::widgets::dropdown_names(ui, "sewout-stabilizer", shown.get(st.stabilizer).copied().unwrap_or_default(), &shown, 240.0) {
                    st.stabilizer = i;
                }
                ui.end_row();
                ui.label(tl!("Needle"));
                ui.add(egui::TextEdit::singleline(&mut st.needle).desired_width(240.0).hint_text(tl!("e.g. 75/11 ballpoint")));
                ui.end_row();
            });
            ui.label(tl!("While it sewed:"));
            st.happened.resize(happened().len(), false);
            for ((_, label), on) in happened().iter().zip(st.happened.iter_mut()) {
                ui.checkbox(on, *label);
            }
            ui.add(egui::TextEdit::multiline(&mut st.notes).desired_rows(2).desired_width(f32::INFINITY).hint_text(tl!("Anything else? For example: a gap above the bumper")));
            ui.add_enabled_ui(has_doc, |ui| {
                ui.checkbox(&mut st.include_design, tl!("Include the design that was sewn (open now), so it can be fixed"));
            });
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            ui.add_space(4.0);
            let ready = !st.photos.is_empty() && !app.embroidery.ask.asking();
            if ui.add_enabled_ui(ready, |ui| crate::widgets::primary_button(ui, tl!("Check My Sew-out"))).inner.clicked() {
                go = true;
            }
            ui.label(
                RichText::new(tl!("Claude looks at the photos with your own Anthropic API key, billed to your Anthropic account. It can be wrong: try a fix on a scrap of the same fabric first."))
                    .size(11.0)
                    .color(t.text_dim),
            );
        });
    if let Some(i) = remove
        && i < app.embroidery.sewout.photos.len()
    {
        app.embroidery.sewout.photos.remove(i);
    }
    if add {
        crate::picks::button(app, add_photos);
    }
    if go {
        // (a failure is shown in the window it happened in)
        let _ = check(app);
    }
    if !open {
        let st = &mut app.embroidery.sewout;
        st.open = false;
        st.waiting = false;
    }
}
