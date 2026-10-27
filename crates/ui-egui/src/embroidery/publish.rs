//! Publish to MakerFoundry (PUB-01): the open design, written in the machine format the maker
//! picks, sent with the sew-out photos to MakerFoundry as a draft, signed in with the Foundry
//! Account; the draft's page opens in the browser, where the maker adds its title, price and
//! licence and publishes it (MakerFoundry's `docs/api.md`; the requests are
//! `sf_core::ai::makerfoundry`).

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use serde_json::{Value, json};
use vectorcraft_embroidery::sf_core::ai::ArtError;
use vectorcraft_embroidery::sf_core::ai::makerfoundry::{self, Draft, Upload};

use super::{account, prompt};
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The command (File › Publish to MakerFoundry…).
pub const COMMAND: &str = "embroidery.publishToMakerFoundry";
/// The formats a design can be published in, PES first (the default).
pub const FORMATS: [&str; 8] = ["pes", "dst", "jef", "exp", "vp3", "xxx", "pec", "u01"];
/// The secret MakerFoundry's address (the setting) is kept as, when one is typed.
const SERVER_NAME: &str = "makerfoundry-server";

/// The window between frames.
#[derive(Default)]
pub struct PublishState {
    pub open: bool,
    loaded: bool,
    /// MakerFoundry's address as typed (empty: makerfoundry.app, or `MAKERFOUNDRY_URL`).
    pub server: String,
    /// The machine format, one of [`FORMATS`].
    pub format: String,
    /// Send Check My Sew-out's photos along.
    pub photos: bool,
    pending: Option<(Receiver<Result<Draft, ArtError>>, String)>,
    /// The draft MakerFoundry kept last.
    pub sent: Option<Draft>,
    pub error: Option<String>,
}

/// Whether the command can run: a design with stitches, and signed in to Foundry Account.
pub fn enabled(app: &VectorcraftApp) -> bool {
    prompt::available(app) && signed_in(app) && app.session.embroidery_plan().is_some_and(|p| !p.is_empty())
}

fn signed_in(app: &VectorcraftApp) -> bool {
    !app.embroidery.account.token.is_empty()
}

/// `embroidery.publishToMakerFoundry {format?, photos?: bool, server?: text, send?: bool}`: opens
/// the window with what is given chosen; `send` sends the design at once.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    load(app);
    let photos_taken = !app.embroidery.sewout.photos.is_empty();
    let st = &mut app.embroidery.publish;
    if !st.open {
        st.photos = photos_taken;
        st.error = None;
    }
    st.open = true;
    if let Some(f) = p.get("format").and_then(Value::as_str) {
        let f = f.trim().trim_start_matches('.').to_ascii_lowercase();
        if !FORMATS.contains(&f.as_str()) {
            return Err(format!("`format` is one of {}", FORMATS.join(", ")));
        }
        st.format = f;
    }
    if let Some(b) = p.get("photos").and_then(Value::as_bool) {
        st.photos = b;
    }
    if let Some(s) = p.get("server").and_then(Value::as_str) {
        if !s.trim().is_empty() {
            makerfoundry::server(s).map_err(|e| e.to_string())?;
        }
        st.server = s.trim().to_string();
        let kept = st.server.clone();
        account::keep(app, SERVER_NAME, (!kept.is_empty()).then_some(kept.as_str()));
    }
    if p.get("send").and_then(Value::as_bool) == Some(true) {
        send(app)?;
    }
    let st = &app.embroidery.publish;
    Ok(json!({
        "open": true,
        "signedIn": signed_in(app),
        "format": st.format,
        "photos": st.photos,
        "server": server(app).ok(),
        "sending": st.pending.is_some(),
        "url": st.sent.as_ref().map(|d| d.url.clone()),
        "error": st.error,
    }))
}

/// The kept sign-in and address, read once.
fn load(app: &mut VectorcraftApp) {
    account::load(app);
    if app.embroidery.publish.format.is_empty() {
        app.embroidery.publish.format = FORMATS[0].to_string();
    }
    if app.embroidery.publish.loaded {
        return;
    }
    app.embroidery.publish.loaded = true;
    if let Some(s) = app.services.secrets.as_ref().and_then(|s| s.load(SERVER_NAME)) {
        app.embroidery.publish.server = s;
    }
}

/// The address the design goes to: the one typed, else `MAKERFOUNDRY_URL`, else makerfoundry.app.
fn server(app: &VectorcraftApp) -> Result<String, ArtError> {
    let typed = app.embroidery.publish.server.trim();
    #[cfg(not(target_arch = "wasm32"))]
    if typed.is_empty()
        && let Ok(s) = std::env::var("MAKERFOUNDRY_URL")
    {
        return makerfoundry::server(&s);
    }
    makerfoundry::server(typed)
}

/// Publish: the design in the chosen format, and the photos if chosen, sent to MakerFoundry.
fn send(app: &mut VectorcraftApp) -> Result<(), String> {
    let fail = |app: &mut VectorcraftApp, m: String| {
        app.embroidery.publish.error = Some(m.clone());
        Err(m)
    };
    if app.embroidery.publish.pending.is_some() {
        return Ok(());
    }
    if !signed_in(app) {
        return fail(app, ArtError::SignIn.to_string());
    }
    let server = match server(app) {
        Ok(s) => s,
        Err(e) => return fail(app, e.to_string()),
    };
    let format = app.embroidery.publish.format.clone();
    let title = app.session.active().map(|d| d.doc.title.clone()).unwrap_or_default();
    let stem = vectorcraft_engine::cmd::fileio::file_stem(&title);
    let stem = if stem.trim().is_empty() { "Design".to_string() } else { stem };
    let file = match app.session.execute("embroidery.export", &json!({ "format": format, "center": true, "name": stem })) {
        Ok(r) => r["dataBase64"].as_str().unwrap_or_default().to_string(),
        Err(e) => return fail(app, e.to_string()),
    };
    let photos =
        if app.embroidery.publish.photos { app.embroidery.sewout.photos.iter().map(|p| p.image.data_base64.clone()).collect() } else { Vec::new() };
    let upload =
        Upload { file_name: format!("{stem}.{format}"), file_base64: file, photos, app: format!("StitchFoundry {}", env!("CARGO_PKG_VERSION")) };
    let token = app.embroidery.account.token.clone();
    let http = match makerfoundry::publish(&server, &token, &upload) {
        Ok(h) => h,
        Err(e) => return fail(app, e.to_string()),
    };
    let from = server.clone();
    match prompt::post_plain(app, http, "makerfoundry-publish", move |status, body| makerfoundry::answer(&from, status, body)) {
        Ok(rx) => {
            let st = &mut app.embroidery.publish;
            st.pending = Some((rx, server));
            st.error = None;
            st.sent = None;
            Ok(())
        }
        Err(e) => fail(app, e),
    }
}

/// Each frame: MakerFoundry's answer, and the draft's page opened in the browser.
pub fn poll(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let Some((rx, _)) = app.embroidery.publish.pending.as_ref() else { return };
    let r = match rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(ArtError::Network("the request stopped".into())),
    };
    answered(app, r);
}

fn answered(app: &mut VectorcraftApp, r: Result<Draft, ArtError>) {
    app.embroidery.publish.pending = None;
    match r {
        Ok(draft) => {
            let url = draft.url.clone();
            app.embroidery.publish.sent = Some(draft);
            app.embroidery.publish.error = None;
            app.open_url(&url);
            app.status(tl!("Your draft is open on MakerFoundry: add its title, price and licence there, and publish it."));
        }
        Err(e) => {
            // the sign-in ended: Foundry Account opens to sign in again
            if matches!(e, ArtError::MakerFoundrySignIn(_) | ArtError::SignIn) {
                app.embroidery.account.open = true;
                app.embroidery.account.sign_in_shown = true;
            }
            let m = match e {
                ArtError::MakerFoundrySignIn(m) if !m.is_empty() => m,
                ArtError::MakerFoundrySignIn(_) => ArtError::SignIn.to_string(),
                e => e.to_string(),
            };
            app.embroidery.publish.error = Some(m);
        }
    }
}

/// The Publish to MakerFoundry window, while open.
pub fn window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.publish.open {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut publish = false;
    let mut sign_in = false;
    let mut reopen = None;
    let mut keep_server = None;
    let signed = signed_in(app);
    let ready = app.session.embroidery_plan().is_some_and(|p| !p.is_empty());
    let photos = app.embroidery.sewout.photos.len();
    let default = server(app).unwrap_or_else(|_| makerfoundry::DEFAULT_SERVER.to_string());
    egui::Window::new(tl!("Publish to MakerFoundry"))
        .id(egui::Id::unique("publish-makerfoundry"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(220.0, 160.0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            let st = &mut app.embroidery.publish;
            ui.label(
                RichText::new(tl!("Send this design to MakerFoundry as a draft. Its page opens in your browser: add the title, price and licence there, and publish it."))
                    .size(12.0)
                    .color(t.text_dim),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(tl!("Format"));
                craft_ui::ComboBox::from_id_salt("publish-format").selected_text(st.format.to_ascii_uppercase()).show_ui(ui, |ui| {
                    for f in FORMATS {
                        ui.selectable_value(&mut st.format, f.to_string(), f.to_ascii_uppercase());
                    }
                });
            });
            if photos > 0 {
                ui.checkbox(&mut st.photos, crate::i18n::fmt(tl!("Include the photos ({n})"), &[("n", &photos.to_string())]));
            }
            ui.add_space(6.0);
            if !signed {
                ui.label(RichText::new(tl!("Sign in to your Foundry Account to publish.")).color(t.text_dim));
                sign_in = ui.button(tl!("Sign In")).clicked();
            } else if st.pending.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tl!("Sending to MakerFoundry…"));
                });
            } else {
                publish = ui.add_enabled(ready, egui::Button::new(tl!("Publish to MakerFoundry"))).clicked();
            }
            if let Some(d) = &st.sent {
                ui.label(RichText::new(tl!("Your draft is open on MakerFoundry: add its title, price and licence there, and publish it.")).strong());
                if ui.link(&d.url).clicked() {
                    reopen = Some(d.url.clone());
                }
            }
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            egui::CollapsingHeader::new(tl!("Server")).id_salt("publish-server").show(ui, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut st.server).desired_width(300.0).hint_text(default.as_str()));
                if r.lost_focus() {
                    keep_server = Some(st.server.trim().to_string());
                }
            });
        });
    if let Some(s) = keep_server {
        account::keep(app, SERVER_NAME, (!s.is_empty()).then_some(s.as_str()));
    }
    if publish {
        let _ = send(app); // (the error is shown in the window)
    }
    if sign_in {
        let _ = account::open(app, &json!({ "create": false })); // (only fails without a connection, which the window needs anyway)
    }
    if let Some(url) = reopen {
        app.open_url(&url);
    }
    if !open {
        app.embroidery.publish.open = false;
    }
}
