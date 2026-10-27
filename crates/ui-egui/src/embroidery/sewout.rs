//! Check My Sew-out (Embroidery and Help menus): photos of a finished sew-out, the design that was
//! sewn when it is open, and a few answers about the sewing (machine, fabric, stabilizer, needle,
//! what happened) go to Claude, which says what went wrong and why and offers to fix the design.
//! The check runs as an Ask About Embroidery conversation in its own mode ([`ask::begin_check`]):
//! the answer, the changes to allow and the questions after it are in that window.
//!
//! Photos are turned upright (a phone's orientation tag), made no larger than Claude looks at and
//! sent as JPEG ([`prepare`]). On the desktop they are picked in an open dialog; on the web the
//! browser's picker hands them in through the place inbox ([`take_arrivals`]).

use std::sync::mpsc::TryRecvError;

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
    /// Keep the check to improve StitchFoundry (paid with credits only).
    pub share: bool,
    /// The last check's id, and whether this one is the sew-out after its fix.
    pub last_check: Option<String>,
    pub follow_up: bool,
    /// The web's picker is open for photos: the files it hands in are photos.
    pub waiting: bool,
    pub error: Option<String>,
    /// Report to StitchFoundry.
    pub report: ReportForm,
}

/// Report to StitchFoundry between frames: the report as typed (or as Claude drafted it), what
/// goes with it, and the user's leave to send it.
#[derive(Default)]
pub struct ReportForm {
    pub open: bool,
    /// Whether the choices below were set for this check yet.
    started: Option<String>,
    pub title: String,
    pub text: String,
    pub note: String,
    pub photos: bool,
    pub conversation: bool,
    pub design: bool,
    /// The user agrees to send it.
    pub consent: bool,
    /// Claude is writing the draft.
    pub drafting: bool,
    pending: Option<std::sync::mpsc::Receiver<Result<Value, ai::ArtError>>>,
    /// Sent: the report's id.
    pub sent: Option<String>,
    /// The credits the check cost, given back for the report.
    pub refunded: u64,
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
    let st = &mut app.embroidery.sewout;
    st.open = false;
    let share = st.share && super::account::paying_with_credits(app);
    let follow_up = app.embroidery.sewout.last_check.clone().filter(|_| app.embroidery.sewout.follow_up);
    let id = ask::begin_check(app, message, shown, share, follow_up)?;
    let st = &mut app.embroidery.sewout;
    st.last_check = Some(id);
    st.follow_up = false;
    Ok(())
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
    super::account::load(app);
    let credits = super::account::paying_with_credits(app);
    egui::Window::new(tl!("Check My Sew-out"))
        .id(egui::Id::unique("check-my-sewout"))
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
            if credits {
                ui.checkbox(&mut st.share, tl!("Share this check to help improve StitchFoundry (kept private; delete it any time in Foundry Account)"));
            }
            if st.last_check.is_some() {
                ui.checkbox(&mut st.follow_up, tl!("This is the sew-out after the fix from my last check"));
            }
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            ui.add_space(4.0);
            let ready = !st.photos.is_empty() && !app.embroidery.ask.asking();
            if ui.add_enabled_ui(ready, |ui| crate::widgets::primary_button(ui, tl!("Check My Sew-out"))).inner.clicked() {
                go = true;
            }
            let paid_by_credits = super::account::paid_line(app, ui, &t);
            if !paid_by_credits {
                ui.label(
                    RichText::new(tl!("Claude looks at the photos with your own Anthropic API key, billed to your Anthropic account. It can be wrong: try a fix on a scrap of the same fabric first."))
                        .size(11.0)
                        .color(t.text_dim),
                );
            } else if let Some(n) = app.embroidery.account.info.as_ref().and_then(|i| i["freeChecks"].as_u64()) {
                let line = if n > 0 {
                    crate::i18n::fmt(tl!("{n} free checks left this month."), &[("n", &n.to_string())])
                } else {
                    let c = app.embroidery.account.info.as_ref().and_then(|i| i["checkCredits"].as_u64()).unwrap_or(10);
                    crate::i18n::fmt(tl!("A check costs {n} credits; free checks come back next month."), &[("n", &c.to_string())])
                };
                ui.label(RichText::new(line).size(11.0).color(t.text_dim));
            }
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

/// `ui.reportSewout {title?, report?, note?, consent?: bool, draft?: bool, send?: bool}`: opens
/// Report to StitchFoundry for the check in the Ask window, with what is given filled in; `draft`
/// has Claude write it, `send` sends it.
pub fn open_report(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let ask = &app.embroidery.ask;
    if ask.mode != ask::Mode::SewOut || ask.last_answer().is_none() {
        return Err("Check a sew-out first: the report goes with its photos and Claude's answer".into());
    }
    let check = ask.check.as_ref().map(|c| c.id.clone());
    super::account::load(app);
    let f = &mut app.embroidery.sewout.report;
    if f.started != check || f.started.is_none() {
        *f = ReportForm { started: check.clone().or(Some(String::new())), photos: true, conversation: true, design: true, ..Default::default() };
    }
    f.open = true;
    for (key, field) in [("title", &mut f.title), ("report", &mut f.text), ("note", &mut f.note)] {
        if let Some(v) = p.get(key).and_then(Value::as_str) {
            *field = v.to_string();
        }
    }
    if let Some(c) = p.get("consent").and_then(Value::as_bool) {
        f.consent = c;
    }
    if p.get("draft").and_then(Value::as_bool) == Some(true) {
        draft(app)?;
    }
    if p.get("send").and_then(Value::as_bool) == Some(true) {
        send_report(app)?;
    }
    let f = &app.embroidery.sewout.report;
    Ok(json!({ "open": true, "signedIn": signed_in(app), "drafting": f.drafting, "sending": f.pending.is_some(), "sent": f.sent }))
}

/// Whether a report can be sent: signed in to Foundry Account.
fn signed_in(app: &VectorcraftApp) -> bool {
    !app.embroidery.account.token.is_empty() && super::account::server(app).is_some()
}

/// Have Claude Write It: the report question asked in the check's conversation; its answer fills
/// the report ([`poll_report`]).
fn draft(app: &mut VectorcraftApp) -> Result<(), String> {
    ask::ask_as(app, sewout::REPORT_QUESTION, tl!("Write a report about this sew-out for the people who make StitchFoundry."))?;
    app.embroidery.sewout.report.drafting = true;
    app.embroidery.sewout.report.error = None;
    Ok(())
}

/// Send Report: the report and what the user chose to include go to the StitchFoundry service.
fn send_report(app: &mut VectorcraftApp) -> Result<(), String> {
    let fail = |app: &mut VectorcraftApp, m: String| {
        app.embroidery.sewout.report.error = Some(m.clone());
        Err(m)
    };
    if !app.embroidery.sewout.report.consent {
        return fail(app, "Tick the box to agree to send it first.".into());
    }
    let Some(server) = super::account::server(app).filter(|_| signed_in(app)) else {
        return fail(app, "Sign in to Foundry Account to send a report.".into());
    };
    let design = if app.embroidery.sewout.report.design && app.session.active().is_some() { ask::design_summary(app) } else { None };
    let st = &app.embroidery.sewout;
    let f = &st.report;
    let report = sewout::Report {
        check: app.embroidery.ask.check.as_ref().map(|c| c.id.clone()),
        title: f.title.clone(),
        report: f.text.clone(),
        note: f.note.clone(),
        photos: if f.photos { st.photos.iter().map(|p| p.image.clone()).collect() } else { Vec::new() },
        conversation: f.conversation.then(|| app.embroidery.ask.transcript()),
        design,
        app: env!("CARGO_PKG_VERSION").to_string(),
    };
    let body = match report.body() {
        Ok(b) => b,
        Err(e) => return fail(app, e.to_string()),
    };
    let token = app.embroidery.account.token.clone();
    let http = match vectorcraft_embroidery::sf_core::ai::cloud::report(&server, &token, &body) {
        Ok(h) => h,
        Err(e) => return fail(app, e.to_string()),
    };
    match prompt::post_plain(app, http, "stitchfoundry-report", vectorcraft_embroidery::sf_core::ai::cloud::parse) {
        Ok(rx) => {
            let f = &mut app.embroidery.sewout.report;
            f.pending = Some(rx);
            f.error = None;
            Ok(())
        }
        Err(e) => fail(app, e),
    }
}

/// Each frame: Claude's draft into the report once it is written, and the service's answer.
pub fn poll_report(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if app.embroidery.sewout.report.drafting && !app.embroidery.ask.asking() {
        let answer = app.embroidery.ask.last_answer().map(str::to_string);
        let failed = app.embroidery.ask.error.clone();
        let f = &mut app.embroidery.sewout.report;
        f.drafting = false;
        match (failed, answer) {
            (Some(e), _) => f.error = Some(e),
            (None, Some(a)) => {
                let (title, text) = sewout::split_report(&a);
                if f.title.trim().is_empty() {
                    f.title = title;
                }
                f.text = text;
            }
            (None, None) => {}
        }
    }
    let Some(rx) = app.embroidery.sewout.report.pending.as_ref() else { return };
    let r = match rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(ai::ArtError::Network("the request stopped".into())),
    };
    let f = &mut app.embroidery.sewout.report;
    f.pending = None;
    match r {
        Ok(v) => {
            f.sent = Some(v["report"].as_str().unwrap_or_default().to_string());
            f.refunded = v["refunded"].as_u64().unwrap_or(0);
            f.error = None;
            app.embroidery.account.stale = true;
        }
        Err(e) => f.error = Some(e.to_string()),
    }
}

/// The Report to StitchFoundry window, while open.
pub fn report_window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.sewout.report.open {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut write = false;
    let mut send = false;
    let mut account = false;
    let signed = signed_in(app);
    let asking = app.embroidery.ask.asking();
    let has_design = app.session.active().is_some();
    let photos = app.embroidery.sewout.photos.len();
    let credits = super::account::paying_with_credits(app);
    egui::Window::new(tl!("Report to StitchFoundry"))
        .id(egui::Id::unique("report-sewout"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(240.0, 280.0))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            let f = &mut app.embroidery.sewout.report;
            ui.label(RichText::new(tl!("Help make StitchFoundry better")).strong().size(15.0));
            ui.label(
                RichText::new(tl!("Send this sew-out to the people who make StitchFoundry, with Claude's idea of what the software could do better. We use it only to improve StitchFoundry: we never share, sell or publish it, and you can have it deleted at any time (Foundry Account › Delete My Shared Sew-out Checks)."))
                    .size(12.0)
                    .color(t.text_dim),
            );
            if credits {
                ui.label(RichText::new(tl!("Paid for this check with credits? Send a report and they come back to you.")).size(12.0).color(t.text_dim));
            }
            if let Some(id) = &f.sent {
                ui.add_space(8.0);
                ui.label(RichText::new(tl!("Thank you: your report was sent.")).strong());
                if f.refunded > 0 {
                    ui.label(crate::i18n::fmt(tl!("The {n} credits this check cost are back in your account."), &[("n", &f.refunded.to_string())]));
                }
                ui.label(RichText::new(id).size(11.0).color(t.text_dim));
                return;
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!asking, egui::Button::new(tl!("Have Claude Write It"))).clicked() {
                    write = true;
                }
                if f.drafting {
                    ui.add(egui::Spinner::new());
                    ui.label(RichText::new(tl!("Claude is writing the report…")).size(12.0).color(t.text_dim));
                }
            });
            ui.add(egui::TextEdit::singleline(&mut f.title).desired_width(f32::INFINITY).hint_text(tl!("Title")));
            ui.add(
                egui::TextEdit::multiline(&mut f.text)
                    .desired_rows(8)
                    .desired_width(f32::INFINITY)
                    .hint_text(tl!("What went wrong, and what StitchFoundry could do better")),
            );
            ui.add(egui::TextEdit::multiline(&mut f.note).desired_rows(2).desired_width(f32::INFINITY).hint_text(tl!("Anything you'd like to add (optional)")));
            ui.add_enabled_ui(photos > 0, |ui| {
                ui.checkbox(&mut f.photos, crate::i18n::fmt(tl!("Include the photos ({n})"), &[("n", &photos.to_string())]));
            });
            ui.checkbox(&mut f.conversation, tl!("Include the conversation with Claude"));
            ui.add_enabled_ui(has_design, |ui| {
                ui.checkbox(&mut f.design, tl!("Include a summary of the design"));
            });
            ui.checkbox(&mut f.consent, tl!("I agree to send this to SiteSentrix, the maker of StitchFoundry, to improve the software. It won't be shared with anyone else."));
            if let Some(e) = &f.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            if !signed {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(tl!("Sign in to Foundry Account to send a report.")).size(12.0).color(t.text_dim));
                    if ui.link(RichText::new(tl!("Account")).size(12.0)).clicked() {
                        account = true;
                    }
                });
            }
            let said = !f.text.trim().is_empty() || !f.note.trim().is_empty();
            let ready = signed && f.consent && said && f.pending.is_none() && !f.drafting;
            ui.horizontal(|ui| {
                if ui.add_enabled_ui(ready, |ui| crate::widgets::primary_button(ui, tl!("Send Report"))).inner.clicked() {
                    send = true;
                }
                if f.pending.is_some() {
                    ui.add(egui::Spinner::new());
                }
            });
        });
    if write {
        // (a failure is shown in the window)
        let _ = draft(app);
    }
    if send {
        let _ = send_report(app);
    }
    if account {
        app.embroidery.account.open = true;
    }
    if !open {
        app.embroidery.sewout.report.open = false;
    }
}
