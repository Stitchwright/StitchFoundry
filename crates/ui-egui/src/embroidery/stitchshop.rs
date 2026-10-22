//! Send to StitchShop (SHOP-01): the open design, written in the machine format chosen (DST unless
//! another is picked), sent with a picture of its stitches and what StitchFoundry knows about it
//! (and the job order, when sent from one) to the shop that will sew it, through the StitchHub
//! inbox, signed in with the Foundry Account. The steps (ask for the addresses, upload the file
//! and the preview, say it is done) run one after another between frames; Take It Back retracts
//! it while no shop has taken it (`docs/stitchshop.md`; the requests are
//! `sf_core::ai::stitchshop`).

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use serde_json::{Value, json};
use vectorcraft_embroidery::sf_core::ai::stitchshop::{self, Offer, Shop, Slots};
use vectorcraft_embroidery::sf_core::ai::{ArtError, HttpRequest};

use super::{account, prompt};
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The command (File › Send to StitchShop…, and a job order's Send to StitchShop).
pub const COMMAND: &str = "embroidery.sendToStitchShop";
/// The formats a design can be sent in, DST first (the default: StitchShop reads it itself).
pub const FORMATS: [&str; 8] = ["dst", "pes", "jef", "exp", "vp3", "xxx", "pec", "u01"];
/// The secret the hub's address (the setting) is kept as, when one is typed.
const HUB_NAME: &str = "stitchshop-hub";

/// What a request in the sequence brought back.
enum Step {
    Shops(Vec<Shop>),
    Slots(Slots),
    /// The file (false) or the preview (true) is uploaded.
    Uploaded(bool),
    Done,
    Retracted(bool),
}

/// A design on its way: the bytes to upload once the hub has given the addresses.
struct Sending {
    hub: String,
    file_name: String,
    file: Vec<u8>,
    preview: Option<Vec<u8>>,
    slots: Option<Slots>,
}

/// What was sent last, to take back.
#[derive(Clone, Debug, PartialEq)]
pub struct Sent {
    pub id: String,
    pub file_name: String,
    /// The shop it went to (none: any of the account's).
    pub shop: Option<String>,
    hub: String,
}

/// The window between frames.
#[derive(Default)]
pub struct StitchShopState {
    pub open: bool,
    loaded: bool,
    /// The hub's address as typed (empty: hub.stitchbench.app, or `STITCHSHOP_HUB_URL`).
    pub hub: String,
    /// The machine format, one of [`FORMATS`].
    pub format: String,
    /// The job order sent from (its id).
    pub job: Option<u64>,
    /// Send a picture of the stitches along.
    pub preview: bool,
    /// The account's shops, once asked.
    pub shops: Option<Vec<Shop>>,
    /// The shop chosen (its id); none: any of the account's.
    pub shop: Option<String>,
    pending: Option<Receiver<Result<Step, ArtError>>>,
    sending: Option<Sending>,
    pub sent: Option<Sent>,
    /// Taken back: the file's name.
    pub retracted: Option<String>,
    pub error: Option<String>,
}

/// Whether the command can run: a design with stitches, and signed in to Foundry Account.
pub fn enabled(app: &VectorcraftApp) -> bool {
    prompt::available(app) && signed_in(app) && app.session.embroidery_plan().is_some_and(|p| !p.is_empty())
}

fn signed_in(app: &VectorcraftApp) -> bool {
    !app.embroidery.account.token.is_empty()
}

/// `embroidery.sendToStitchShop {format?, job?: id, shop?: id, preview?: bool, hub?: text, send?:
/// bool, retract?: bool}`: opens the window with what is given chosen; `send` sends the design at
/// once, `retract` takes back the one sent last.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    load(app);
    let st = &mut app.embroidery.stitchshop;
    if !st.open {
        st.error = None;
        st.preview = true;
    }
    st.open = true;
    if let Some(f) = p.get("format").and_then(Value::as_str) {
        let f = f.trim().trim_start_matches('.').to_ascii_lowercase();
        if !FORMATS.contains(&f.as_str()) {
            return Err(format!("`format` is one of {}", FORMATS.join(", ")));
        }
        st.format = f;
    }
    if let Some(b) = p.get("preview").and_then(Value::as_bool) {
        st.preview = b;
    }
    if let Some(s) = p.get("shop") {
        st.shop = s.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    }
    if let Some(s) = p.get("hub").and_then(Value::as_str) {
        if !s.trim().is_empty() {
            stitchshop::hub(s).map_err(|e| e.to_string())?;
        }
        if st.hub != s.trim() {
            st.shops = None;
        }
        st.hub = s.trim().to_string();
        let kept = st.hub.clone();
        account::keep(app, HUB_NAME, (!kept.is_empty()).then_some(kept.as_str()));
    }
    if let Some(j) = p.get("job") {
        let id = if j.is_null() { None } else { Some(j.as_u64().ok_or("`job` is a job order's id")?) };
        if let Some(id) = id {
            job(app, id)?;
        }
        app.embroidery.stitchshop.job = id;
    }
    if p.get("retract").and_then(Value::as_bool) == Some(true) {
        retract(app)?;
    } else if p.get("send").and_then(Value::as_bool) == Some(true) {
        send(app)?;
    } else if app.embroidery.stitchshop.shops.is_none() && signed_in(app) && app.embroidery.stitchshop.pending.is_none() {
        // the shops for the picker (no matter if this fails: the design can go to any of them)
        let _ = ask_shops(app);
    }
    Ok(state(app))
}

fn state(app: &VectorcraftApp) -> Value {
    let st = &app.embroidery.stitchshop;
    json!({
        "open": st.open,
        "signedIn": signed_in(app),
        "format": st.format,
        "job": st.job,
        "preview": st.preview,
        "hub": hub(app).ok(),
        "shops": st.shops.as_ref().map(|l| l.iter().map(|s| json!({"id": s.id, "name": s.name, "program": s.program})).collect::<Vec<_>>()),
        "shop": st.shop,
        "sending": st.pending.is_some(),
        "sent": st.sent.as_ref().map(|s| json!({"id": s.id, "fileName": s.file_name, "shop": s.shop})),
        "retracted": st.retracted,
        "error": st.error,
    })
}

/// The kept sign-in and address, read once.
fn load(app: &mut VectorcraftApp) {
    account::load(app);
    if app.embroidery.stitchshop.format.is_empty() {
        app.embroidery.stitchshop.format = FORMATS[0].to_string();
    }
    if app.embroidery.stitchshop.loaded {
        return;
    }
    app.embroidery.stitchshop.loaded = true;
    if let Some(s) = app.services.secrets.as_ref().and_then(|s| s.load(HUB_NAME)) {
        app.embroidery.stitchshop.hub = s;
    }
}

/// The address the design goes to: the one typed, else `STITCHSHOP_HUB_URL`, else the hub's own.
fn hub(app: &VectorcraftApp) -> Result<String, ArtError> {
    let typed = app.embroidery.stitchshop.hub.trim();
    #[cfg(not(target_arch = "wasm32"))]
    if typed.is_empty()
        && let Ok(s) = std::env::var("STITCHSHOP_HUB_URL")
    {
        return stitchshop::hub(&s);
    }
    stitchshop::hub(typed)
}

/// The job order `id` (`embroidery.jobs`' entry), its design made the active document.
fn job(app: &mut VectorcraftApp, id: u64) -> Result<Value, String> {
    let jobs = app.session.execute("embroidery.jobs", &json!({})).map_err(|e| e.to_string())?;
    let job = jobs["jobs"].as_array().and_then(|l| l.iter().find(|j| j["id"].as_u64() == Some(id))).cloned().ok_or("no job order with that id")?;
    let design = job["design"].as_str().unwrap_or_default();
    if !design.is_empty() && app.session.active().and_then(|d| d.path.as_deref()) != Some(design) {
        match app.session.documents().iter().position(|d| d.path.as_deref() == Some(design)) {
            Some(i) => {
                app.session.set_active(i);
            }
            None => {
                app.session.execute("file.open", &json!({ "path": design })).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(job)
}

/// The request `http` sent, its answer read by `parse` into the next step.
fn post(app: &mut VectorcraftApp, http: HttpRequest, parse: impl Fn(u16, &str) -> Result<Step, ArtError> + Send + 'static) -> Result<(), String> {
    let rx = prompt::post_plain(app, http, "stitchshop", parse)?;
    app.embroidery.stitchshop.pending = Some(rx);
    Ok(())
}

fn fail(app: &mut VectorcraftApp, m: String) -> Result<(), String> {
    app.embroidery.stitchshop.error = Some(m.clone());
    app.embroidery.stitchshop.sending = None;
    Err(m)
}

fn ask_shops(app: &mut VectorcraftApp) -> Result<(), String> {
    let hub = hub(app).map_err(|e| e.to_string())?;
    let http = stitchshop::shops(&hub, &app.embroidery.account.token).map_err(|e| e.to_string())?;
    post(app, http, |status, body| stitchshop::shops_answer(status, body).map(Step::Shops))
}

/// Send: the design in the chosen format, its preview and `meta`, offered to the hub.
fn send(app: &mut VectorcraftApp) -> Result<(), String> {
    if app.embroidery.stitchshop.pending.is_some() {
        return Ok(());
    }
    if !signed_in(app) {
        return fail(app, ArtError::SignIn.to_string());
    }
    let hub = match hub(app) {
        Ok(h) => h,
        Err(e) => return fail(app, e.to_string()),
    };
    let job = match app.embroidery.stitchshop.job {
        Some(id) => match job(app, id) {
            Ok(j) => Some(j),
            Err(e) => return fail(app, e),
        },
        None => None,
    };
    let format = app.embroidery.stitchshop.format.clone();
    let title = app.session.active().map(|d| d.doc.title.clone()).unwrap_or_default();
    let stem = vectorcraft_engine::cmd::fileio::file_stem(&title);
    let design = if stem.trim().is_empty() { "Design".to_string() } else { stem };
    let name = match job.as_ref().and_then(|j| j["number"].as_str()) {
        Some(n) => format!("{n} {design}"),
        None => design.clone(),
    };
    let file = match app.session.execute("embroidery.export", &json!({ "format": format, "center": true, "name": design })) {
        Ok(r) => match vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap_or_default()) {
            Some(b) => b,
            None => return fail(app, "the design couldn't be written".into()),
        },
        Err(e) => return fail(app, e.to_string()),
    };
    let info = app.session.execute("embroidery.info", &json!({})).unwrap_or_default();
    let preview = if app.embroidery.stitchshop.preview { preview(app) } else { None };
    let offer = Offer {
        file_name: stitchshop::file_name(&format!("{name}.{format}")),
        meta: stitchshop::meta(&info, job.as_ref(), &design, &format, &file, env!("CARGO_PKG_VERSION")),
        file,
        preview,
        to_shop: app.embroidery.stitchshop.shop.clone(),
    };
    let http = match stitchshop::put(&hub, &app.embroidery.account.token, &offer) {
        Ok(h) => h,
        Err(e) => return fail(app, e.to_string()),
    };
    let st = &mut app.embroidery.stitchshop;
    st.error = None;
    st.sent = None;
    st.retracted = None;
    st.sending = Some(Sending { hub, file_name: offer.file_name, file: offer.file, preview: offer.preview, slots: None });
    let with_preview = st.sending.as_ref().is_some_and(|s| s.preview.is_some());
    post(app, http, move |status, body| stitchshop::put_answer(status, body, with_preview).map(Step::Slots)).or_else(|e| fail(app, e))
}

/// A PNG of the stitches on the fabric's colour, within the hub's 2 MB (smaller until it fits;
/// none if it never does).
fn preview(app: &mut VectorcraftApp) -> Option<Vec<u8>> {
    for ppmm in [8, 5, 3, 2] {
        let r = app.session.execute("embroidery.exportImage", &json!({ "ppmm": ppmm, "margin": 3 })).ok()?;
        let png = vectorcraft_format::base64_decode(r["dataBase64"].as_str()?)?;
        if png.len() <= stitchshop::MAX_PREVIEW_BYTES {
            return Some(png);
        }
    }
    None
}

/// Take It Back: the design sent last retracted from the inbox.
fn retract(app: &mut VectorcraftApp) -> Result<(), String> {
    if app.embroidery.stitchshop.pending.is_some() {
        return Ok(());
    }
    let Some(sent) = app.embroidery.stitchshop.sent.clone() else { return fail(app, "nothing has been sent to take back".into()) };
    let http = match stitchshop::retract(&sent.hub, &app.embroidery.account.token, &sent.id) {
        Ok(h) => h,
        Err(e) => return fail(app, e.to_string()),
    };
    app.embroidery.stitchshop.error = None;
    post(app, http, |status, body| stitchshop::retract_answer(status, body).map(Step::Retracted)).or_else(|e| fail(app, e))
}

/// Each frame: the answer to the request on its way, and the next request.
pub fn poll(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let Some(rx) = app.embroidery.stitchshop.pending.as_ref() else { return };
    let r = match rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(ArtError::Network("the request stopped".into())),
    };
    app.embroidery.stitchshop.pending = None;
    if let Err(e) = answered(app, r) {
        // the sign-in ended: Foundry Account opens to sign in again
        if matches!(e, ArtError::StitchHubSignIn(_) | ArtError::SignIn) {
            app.embroidery.account.open = true;
            app.embroidery.account.sign_in_shown = true;
        }
        let m = match e {
            ArtError::StitchHubSignIn(m) if !m.is_empty() => m,
            ArtError::StitchHubSignIn(_) => ArtError::SignIn.to_string(),
            e => e.to_string(),
        };
        let _ = fail(app, m); // (shown in the window)
    }
    if app.embroidery.stitchshop.pending.is_some() {
        ctx.request_repaint();
    }
}

fn answered(app: &mut VectorcraftApp, r: Result<Step, ArtError>) -> Result<(), ArtError> {
    let token = app.embroidery.account.token.clone();
    let next = |app: &mut VectorcraftApp, http: HttpRequest, parse: fn(u16, &str) -> Result<Step, ArtError>| {
        post(app, http, parse).map_err(ArtError::Network)
    };
    match r? {
        Step::Shops(shops) => {
            let st = &mut app.embroidery.stitchshop;
            if st.shop.as_ref().is_some_and(|id| !shops.iter().any(|s| &s.id == id)) {
                st.shop = None;
            }
            st.shops = Some(shops);
            Ok(())
        }
        Step::Slots(slots) => {
            let Some(s) = app.embroidery.stitchshop.sending.as_mut() else { return Ok(()) };
            let http = stitchshop::upload(&slots.file, &s.file);
            s.slots = Some(slots);
            next(app, http, |status, body| stitchshop::upload_answer(status, body).map(|()| Step::Uploaded(false)))
        }
        Step::Uploaded(was_preview) => {
            let Some(s) = app.embroidery.stitchshop.sending.as_ref() else { return Ok(()) };
            let Some(slots) = s.slots.as_ref() else { return Err(ArtError::NoAnswer) };
            if let (Some(slot), Some(png), false) = (&slots.preview, &s.preview, was_preview) {
                let http = stitchshop::upload(slot, png);
                return next(app, http, |status, body| stitchshop::upload_answer(status, body).map(|()| Step::Uploaded(true)));
            }
            let http = stitchshop::done(&s.hub, &token, &slots.id)?;
            next(app, http, |status, body| stitchshop::done_answer(status, body).map(|()| Step::Done))
        }
        Step::Done => {
            let st = &mut app.embroidery.stitchshop;
            let Some(s) = st.sending.take() else { return Ok(()) };
            let Some(slots) = s.slots else { return Err(ArtError::NoAnswer) };
            let shop = st.shop.as_ref().and_then(|id| st.shops.as_ref()?.iter().find(|s| &s.id == id)).map(|s| s.name.clone());
            st.sent = Some(Sent { id: slots.id, file_name: s.file_name, shop, hub: s.hub });
            app.status(tl!("The design is in the shop's StitchShop inbox."));
            Ok(())
        }
        Step::Retracted(was_there) => {
            let st = &mut app.embroidery.stitchshop;
            st.retracted = st.sent.take().map(|s| s.file_name);
            // (gone already: it waited its 30 days, or the hub swept it)
            app.status(if was_there {
                tl!("The design was taken back from the shop's inbox.")
            } else {
                tl!("The design was no longer in the shop's inbox.")
            });
            Ok(())
        }
    }
}

/// The Send to StitchShop window, while open.
pub fn window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.stitchshop.open {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut send_now = false;
    let mut take_back = false;
    let mut sign_in = false;
    let mut keep_hub = None;
    let signed = signed_in(app);
    let ready = app.session.embroidery_plan().is_some_and(|p| !p.is_empty());
    let default = hub(app).unwrap_or_else(|_| stitchshop::DEFAULT_HUB.to_string());
    let job = app.embroidery.stitchshop.job.and_then(|id| {
        let jobs = app.session.execute("embroidery.jobs", &json!({})).ok()?;
        jobs["jobs"]
            .as_array()?
            .iter()
            .find(|j| j["id"].as_u64() == Some(id))
            .map(|j| format!("{}  {}", j["number"].as_str().unwrap_or_default(), j["customer"].as_str().unwrap_or_default()))
    });
    egui::Window::new(tl!("Send to StitchShop"))
        .id(egui::Id::new("send-stitchshop"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(220.0, 160.0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            let st = &mut app.embroidery.stitchshop;
            ui.label(
                RichText::new(tl!("Send this design to the shop that will sew it. It waits in the shop's StitchShop inbox, with a picture of the stitches and its details, until the shop takes it into an order."))
                    .size(12.0)
                    .color(t.text_dim),
            );
            ui.add_space(6.0);
            if let Some(j) = &job {
                ui.label(crate::i18n::fmt(tl!("Job order: {job}"), &[("job", j)]));
            }
            if let Some(shops) = st.shops.as_ref().filter(|s| s.len() > 1) {
                ui.horizontal(|ui| {
                    ui.label(tl!("Shop"));
                    let any = tl!("Any of my shops").to_string();
                    let shown = st.shop.as_ref().and_then(|id| shops.iter().find(|s| &s.id == id)).map_or(any.clone(), |s| s.name.clone());
                    egui::ComboBox::from_id_salt("stitchshop-shop").selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut st.shop, None, any);
                        for s in shops {
                            ui.selectable_value(&mut st.shop, Some(s.id.clone()), &s.name);
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                ui.label(tl!("Format"));
                egui::ComboBox::from_id_salt("stitchshop-format").selected_text(st.format.to_ascii_uppercase()).show_ui(ui, |ui| {
                    for f in FORMATS {
                        ui.selectable_value(&mut st.format, f.to_string(), f.to_ascii_uppercase());
                    }
                });
            });
            ui.checkbox(&mut st.preview, tl!("Include a picture of the stitches"));
            ui.add_space(6.0);
            if !signed {
                ui.label(RichText::new(tl!("Sign in to your Foundry Account to send designs to your shop.")).color(t.text_dim));
                sign_in = ui.button(tl!("Sign In")).clicked();
            } else if st.pending.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(if st.sending.is_some() { tl!("Sending to StitchShop…") } else { tl!("Asking StitchHub…") });
                });
            } else {
                send_now = ui.add_enabled(ready, egui::Button::new(tl!("Send to StitchShop"))).clicked();
            }
            if let Some(s) = &st.sent {
                let what = match &s.shop {
                    Some(shop) => crate::i18n::fmt(tl!("{file} is in {shop}'s inbox."), &[("file", &s.file_name), ("shop", shop)]),
                    None => crate::i18n::fmt(tl!("{file} is in your shops' inbox."), &[("file", &s.file_name)]),
                };
                ui.label(RichText::new(what).strong());
                take_back = st.pending.is_none() && ui.button(tl!("Take It Back")).clicked();
            }
            if let Some(f) = &st.retracted {
                ui.label(crate::i18n::fmt(tl!("{file} was taken back."), &[("file", f)]));
            }
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            egui::CollapsingHeader::new(tl!("Server")).id_salt("stitchshop-hub").show(ui, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut st.hub).desired_width(300.0).hint_text(default.as_str()));
                if r.lost_focus() {
                    keep_hub = Some(st.hub.trim().to_string());
                }
            });
        });
    if let Some(h) = keep_hub {
        let _ = open_with(app, json!({ "hub": h })); // (an address that isn't valid is said when sending)
    }
    if send_now {
        let _ = send(app); // (the error is shown in the window)
    }
    if take_back {
        let _ = retract(app); // (the error is shown in the window)
    }
    if sign_in {
        let _ = account::open(app, &json!({ "create": false })); // (only fails without a connection, which the window needs anyway)
    }
    if !open {
        app.embroidery.stitchshop.open = false;
    }
}

fn open_with(app: &mut VectorcraftApp, p: Value) -> Result<Value, String> {
    open(app, &p)
}
