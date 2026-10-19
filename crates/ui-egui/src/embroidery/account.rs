//! Foundry Account (Embroidery and Help menus): sign in to the StitchFoundry cloud service
//! or make an account, see the credits, buy more (Stripe's checkout page opens in the browser) and
//! choose to pay for the AI features with them instead of one's own Anthropic API key. With that
//! on, Design from Prompt, Redraw Image, Ask About Embroidery and Check My Sew-out send their
//! requests through the service ([`prompt::post`] reroutes them: [`cloud::through_cloud`]).
//!
//! The session token is kept by the host's secret store, never in the settings; the password is
//! sent once and forgotten. The service's address comes from the build (`SF_CLOUD_URL`), the
//! `STITCHFOUNDRY_CLOUD_URL` environment variable, or the window's Server row.

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use serde_json::{Value, json};
use vectorcraft_embroidery::sf_core::ai::{ArtError, HttpRequest, cloud};

use super::prompt;
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The secrets the choices are kept as.
const SERVER_NAME: &str = "stitchfoundry-server";
const CREDITS_NAME: &str = "stitchfoundry-pay-with-credits";

/// What a request on its way was for.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    SignIn,
    SignUp,
    Refresh,
    Buy,
    SignOut,
    DeleteShared,
}

/// The window and the account between frames.
#[derive(Default)]
pub struct AccountState {
    pub open: bool,
    loaded: bool,
    /// The Server row as typed (empty: the build's or the environment's).
    pub server: String,
    /// The session token; empty while signed out.
    pub token: String,
    /// Pay for the AI features with Foundry credits.
    pub use_credits: bool,
    pub email: String,
    pub password: String,
    /// The account as last read (`{email, credits, packs, payments, history}`).
    pub info: Option<Value>,
    pending: Option<(Receiver<Result<Value, ArtError>>, Action)>,
    /// Read the account again when nothing is on its way (after an AI request paid in credits).
    pub stale: bool,
    pub error: Option<String>,
    /// What just happened, shown under the buttons.
    pub note: Option<String>,
    /// Signed out: show Sign In (else Create Account, the first choice).
    pub sign_in_shown: bool,
}

/// The service's address from the build or the environment.
fn default_server() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(s) = std::env::var("STITCHFOUNDRY_CLOUD_URL")
        && !s.trim().is_empty()
    {
        return s;
    }
    option_env!("SF_CLOUD_URL").unwrap_or_default().to_string()
}

/// Read the kept token and choices once.
pub(crate) fn load(app: &mut VectorcraftApp) {
    let st = &mut app.embroidery.account;
    if st.loaded {
        return;
    }
    st.loaded = true;
    if let Some(s) = app.services.secrets.as_ref() {
        st.token = s.load(cloud::TOKEN_NAME).unwrap_or_default();
        if let Some(server) = s.load(SERVER_NAME) {
            st.server = server;
        }
        // signed in and never chosen: paying with credits, as signing in chooses
        st.use_credits = s.load(CREDITS_NAME).map_or(!st.token.is_empty(), |v| v == "1");
    }
    st.stale = !st.token.is_empty();
}

fn keep(app: &VectorcraftApp, name: &str, value: Option<&str>) {
    if let Some(s) = app.services.secrets.as_ref() {
        // (not kept: asked again next time)
        let _ = s.save(name, value);
    }
}

/// The service's address in use (`None`: accounts aren't open).
pub fn server(app: &VectorcraftApp) -> Option<String> {
    let typed = app.embroidery.account.server.trim();
    if typed.is_empty() { cloud::clean_server(&default_server()) } else { cloud::clean_server(typed) }
}

/// Whether AI requests go through the service, paid in credits.
pub fn paying_with_credits(app: &VectorcraftApp) -> bool {
    let st = &app.embroidery.account;
    st.use_credits && !st.token.is_empty() && server(app).is_some()
}

/// The credits left, as last read.
pub fn credits(app: &VectorcraftApp) -> Option<u64> {
    app.embroidery.account.info.as_ref().and_then(|i| i["credits"].as_u64())
}

/// What the AI features send in the API key's place: the session token when paying with
/// credits, else the user's own Anthropic key.
pub fn key_to_send(app: &mut VectorcraftApp) -> String {
    load(app);
    if paying_with_credits(app) { app.embroidery.account.token.clone() } else { app.embroidery.prompt.key.clone() }
}

/// `ui.account`: opens the window and reads the account.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    if !prompt::available(app) {
        return Err("Foundry Account needs the desktop app for now".into());
    }
    load(app);
    app.embroidery.account.open = true;
    if let Some(create) = p.get("create").and_then(Value::as_bool) {
        app.embroidery.account.sign_in_shown = !create;
    }
    if !app.embroidery.account.token.is_empty() {
        app.embroidery.account.stale = true;
    }
    let st = &app.embroidery.account;
    Ok(json!({ "open": true, "signedIn": !st.token.is_empty(), "server": server(app), "credits": credits(app), "payWithCredits": st.use_credits }))
}

fn send(app: &mut VectorcraftApp, http: Result<HttpRequest, ArtError>, action: Action) {
    let st = &mut app.embroidery.account;
    st.error = None;
    st.note = None;
    let http = match http {
        Ok(h) => h,
        Err(e) => {
            st.error = Some(e.to_string());
            return;
        }
    };
    match prompt::post_plain(app, http, "foundry-account", cloud::parse) {
        Ok(rx) => app.embroidery.account.pending = Some((rx, action)),
        Err(e) => app.embroidery.account.error = Some(e),
    }
}

/// Sign In / Create Account with the email and password typed.
fn sign_in(app: &mut VectorcraftApp, create: bool) {
    let Some(server) = server(app) else {
        app.embroidery.account.error = Some(ArtError::NoServer.to_string());
        return;
    };
    let st = &app.embroidery.account;
    let email = st.email.trim().to_string();
    if !email.contains('@') {
        app.embroidery.account.error = Some("Type your email address.".into());
        return;
    }
    if st.password.chars().count() < 8 {
        app.embroidery.account.error = Some("A password needs at least 8 characters.".into());
        return;
    }
    let http = if create { cloud::sign_up(&server, &email, &st.password) } else { cloud::sign_in(&server, &email, &st.password) };
    send(app, http, if create { Action::SignUp } else { Action::SignIn });
}

fn refresh(app: &mut VectorcraftApp) {
    let st = &mut app.embroidery.account;
    st.stale = false;
    if st.token.is_empty() || st.pending.is_some() {
        return;
    }
    let Some(server) = server(app) else { return };
    let token = app.embroidery.account.token.clone();
    send(app, cloud::account(&server, &token), Action::Refresh);
}

fn buy(app: &mut VectorcraftApp, pack: &str) {
    let Some(server) = server(app) else { return };
    let token = app.embroidery.account.token.clone();
    send(app, cloud::checkout(&server, &token, pack), Action::Buy);
}

fn delete_shared(app: &mut VectorcraftApp) {
    let Some(server) = server(app) else { return };
    let token = app.embroidery.account.token.clone();
    send(app, cloud::delete_shared(&server, &token), Action::DeleteShared);
}

fn sign_out(app: &mut VectorcraftApp) {
    let token = std::mem::take(&mut app.embroidery.account.token);
    keep(app, cloud::TOKEN_NAME, None);
    app.embroidery.account.info = None;
    if let Some(server) = server(app) {
        // the session ends at the service too (signed out here whatever it answers)
        send(app, cloud::sign_out(&server, &token), Action::SignOut);
    }
}

/// Each frame: take in the answer that arrived, and read the account again when it is stale.
pub fn poll(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if app.embroidery.account.stale && app.embroidery.account.pending.is_none() {
        refresh(app);
    }
    let Some((rx, _)) = app.embroidery.account.pending.as_ref() else { return };
    let r = match rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(ArtError::Network("the request stopped".into())),
    };
    let Some((_, action)) = app.embroidery.account.pending.take() else { return };
    answered(app, action, r);
}

fn answered(app: &mut VectorcraftApp, action: Action, r: Result<Value, ArtError>) {
    let v = match r {
        Ok(v) => v,
        Err(e) => {
            if e == ArtError::SignIn && action != Action::SignIn {
                // the session ended: signed out here too
                app.embroidery.account.token.clear();
                app.embroidery.account.info = None;
                keep(app, cloud::TOKEN_NAME, None);
            }
            if action != Action::SignOut {
                app.embroidery.account.error = Some(e.to_string());
            }
            return;
        }
    };
    match action {
        Action::SignIn | Action::SignUp => {
            let token = v["token"].as_str().unwrap_or_default().to_string();
            if !token.starts_with(cloud::TOKEN_PREFIX) {
                app.embroidery.account.error = Some(ArtError::NoAnswer.to_string());
                return;
            }
            keep(app, cloud::TOKEN_NAME, Some(&token));
            let st = &mut app.embroidery.account;
            st.token = token;
            st.password.clear();
            // signing in is choosing to pay with credits
            st.use_credits = true;
            st.info = Some(v["account"].clone());
            st.stale = true;
            st.note = Some(if action == Action::SignUp { "Account made: welcome to StitchFoundry.".into() } else { "Signed in.".into() });
            keep(app, CREDITS_NAME, Some("1"));
        }
        Action::Refresh => app.embroidery.account.info = Some(v),
        Action::Buy => match v["url"].as_str().filter(|u| u.starts_with("https://")) {
            Some(url) => {
                app.open_url(url);
                app.embroidery.account.note = Some("Pay on the page that opened in your browser, then press Refresh.".into());
            }
            None => app.embroidery.account.error = Some(ArtError::NoAnswer.to_string()),
        },
        Action::SignOut => app.embroidery.account.note = Some("Signed out.".into()),
        Action::DeleteShared => {
            app.embroidery.account.note = Some("Your shared sew-out checks are deleted.".into());
            app.embroidery.account.stale = true;
        }
    }
}

/// What the title bar's account button says: Sign In, or the credits left; `None` while
/// accounts aren't open.
pub fn chip_label(app: &mut VectorcraftApp) -> Option<String> {
    load(app);
    server(app)?;
    let st = &app.embroidery.account;
    Some(if st.token.is_empty() {
        tl!("Sign In").to_string()
    } else {
        match credits(app) {
            Some(n) => crate::i18n::fmt(tl!("{n} credits"), &[("n", &n.to_string())]),
            None => tl!("Account").to_string(),
        }
    })
}

/// Opens the Account window on Create Account (`create`) or Sign In.
pub fn show(app: &mut VectorcraftApp, create: bool) {
    load(app);
    let st = &mut app.embroidery.account;
    st.open = true;
    st.sign_in_shown = !create;
    if !st.token.is_empty() {
        st.stale = true;
    }
}

/// The Home screen's account card: signed out, what a free account gives and the button to
/// make one; signed in, the credits and a way to buy more. Nothing while accounts aren't open.
pub fn home_card(app: &mut VectorcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    load(app);
    if server(app).is_none() || !prompt::available(app) {
        return;
    }
    let signed_in = !app.embroidery.account.token.is_empty();
    let mut go = None;
    egui::Frame::group(ui.style()).fill(t.panel).corner_radius(8.0).inner_margin(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width().min(560.0));
        if signed_in {
            let email = app.embroidery.account.info.as_ref().and_then(|i| i["email"].as_str().map(str::to_string)).unwrap_or_default();
            ui.label(RichText::new(tl!("Foundry Account")).strong().size(15.0));
            let line = match credits(app) {
                Some(n) => crate::i18n::fmt(tl!("{email}: {n} credits"), &[("email", &email), ("n", &n.to_string())]),
                None => email,
            };
            ui.label(RichText::new(line).color(t.text_dim));
            ui.horizontal(|ui| {
                if crate::widgets::secondary_button(ui, tl!("Buy Credits…")).clicked() {
                    go = Some(false);
                }
            });
        } else {
            ui.label(RichText::new(tl!("Create a free Foundry Account")).strong().size(15.0));
            for line in [
                tl!("100 free credits to try AI design: Design from Prompt, Redraw Image, Ask About Embroidery"),
                tl!("3 free Check My Sew-out photo checks every month"),
                tl!("No card needed, and no API key to set up"),
            ] {
                ui.label(RichText::new(format!("•  {line}")).color(t.text_dim));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, tl!("Create Free Account")).clicked() {
                    go = Some(true);
                }
                if ui.link(tl!("I have an account: Sign In")).clicked() {
                    go = Some(false);
                }
            });
        }
    });
    if let Some(create) = go {
        show(app, create);
    }
}

/// A line for the AI windows saying how they are paid for, with the Account window a click away;
/// `true` when paying with credits (then no API key is asked for).
pub fn paid_line(app: &mut VectorcraftApp, ui: &mut egui::Ui, t: &Tokens) -> bool {
    load(app);
    let credits_on = paying_with_credits(app);
    ui.horizontal_wrapped(|ui| {
        if credits_on {
            let line = match credits(app) {
                Some(n) => crate::i18n::fmt(tl!("Paid with Foundry credits ({n} left)."), &[("n", &n.to_string())]),
                None => tl!("Paid with Foundry credits.").to_string(),
            };
            ui.label(RichText::new(line).size(11.0).color(t.text_dim));
            if ui.link(RichText::new(tl!("Account")).size(11.0)).clicked() {
                app.embroidery.account.open = true;
                app.embroidery.account.stale = true;
            }
        } else if server(app).is_some() && app.embroidery.account.token.is_empty() {
            if crate::widgets::primary_button(ui, tl!("Create Free Account"))
                .on_hover_text(tl!("100 free credits for the AI features, no API key needed"))
                .clicked()
            {
                show(app, true);
            }
            if ui.link(RichText::new(tl!("Sign In")).size(11.0)).clicked() {
                show(app, false);
            }
        } else if server(app).is_some() && ui.link(RichText::new(tl!("Pay with Foundry credits instead")).size(11.0)).clicked() {
            app.embroidery.account.open = true;
        }
    });
    credits_on
}

/// What a click does once the window is drawn.
type Then = Box<dyn FnOnce(&mut VectorcraftApp)>;

/// The window, while open.
pub fn window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.account.open {
        return;
    }
    load(app);
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut todo: Option<Then> = None;
    let srv = server(app);
    egui::Window::new(tl!("Foundry Account"))
        .id(egui::Id::new("foundry-account"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(220.0, 220.0))
        .show(ctx, |ui| {
            ui.set_width(440.0);
            let busy = app.embroidery.account.pending.is_some();
            let st = &mut app.embroidery.account;
            if srv.is_none() {
                ui.label(tl!("Foundry accounts aren't open yet. Until then, the AI features work with your own Anthropic API key."));
            } else if st.token.is_empty() {
                let create = !st.sign_in_shown;
                if create {
                    ui.label(RichText::new(tl!("Create a free Foundry Account")).strong().size(16.0));
                    for line in [
                        tl!("100 free credits to try AI design: Design from Prompt, Redraw Image, Ask About Embroidery"),
                        tl!("3 free Check My Sew-out photo checks every month"),
                        tl!("No card needed, and no API key to set up"),
                    ] {
                        ui.label(RichText::new(format!("•  {line}")).color(t.text_dim));
                    }
                } else {
                    ui.label(RichText::new(tl!("Sign in to your Foundry Account")).strong().size(16.0));
                }
                ui.add_space(6.0);
                egui::Grid::new("account-sign-in").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    ui.label(tl!("Email"));
                    ui.add(egui::TextEdit::singleline(&mut st.email).desired_width(260.0));
                    ui.end_row();
                    ui.label(tl!("Password"));
                    let r = ui.add(egui::TextEdit::singleline(&mut st.password).password(true).desired_width(260.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        todo = Some(Box::new(move |app| sign_in(app, create)));
                    }
                    ui.end_row();
                });
                ui.horizontal(|ui| {
                    let label = if create { tl!("Create Free Account") } else { tl!("Sign In") };
                    if ui.add_enabled_ui(!busy, |ui| crate::widgets::primary_button(ui, label)).inner.clicked() {
                        todo = Some(Box::new(move |app| sign_in(app, create)));
                    }
                    let other = if create { tl!("I have an account: Sign In") } else { tl!("New here? Create a free account") };
                    if ui.link(other).clicked() {
                        todo = Some(Box::new(move |app| app.embroidery.account.sign_in_shown = create));
                    }
                });
                if create {
                    ui.label(RichText::new(tl!("A password needs at least 8 characters.")).size(11.0).color(t.text_dim));
                }
            } else {
                let info = st.info.clone().unwrap_or(Value::Null);
                if let Some(email) = info["email"].as_str() {
                    ui.label(crate::i18n::fmt(tl!("Signed in as {email}"), &[("email", email)]));
                }
                let n = info["credits"].as_u64();
                ui.label(
                    RichText::new(match n {
                        Some(n) => crate::i18n::fmt(tl!("{n} credits"), &[("n", &n.to_string())]),
                        None => tl!("Reading your credits…").to_string(),
                    })
                    .size(20.0)
                    .strong(),
                );
                let mut on = st.use_credits;
                if ui.checkbox(&mut on, tl!("Pay for the AI features with my Foundry credits")).changed() {
                    todo = Some(Box::new(move |app| {
                        app.embroidery.account.use_credits = on;
                        keep(app, CREDITS_NAME, Some(if on { "1" } else { "0" }));
                    }));
                }
                if let Some(f) = info["freeChecks"].as_u64() {
                    ui.label(
                        RichText::new(crate::i18n::fmt(tl!("Free sew-out checks left this month: {n}"), &[("n", &f.to_string())])).color(t.text_dim),
                    );
                }
                if info["sharedChecks"].as_u64().is_some_and(|n| n > 0)
                    && ui.add_enabled(!busy, egui::Button::new(tl!("Delete My Shared Sew-out Checks"))).clicked()
                {
                    todo = Some(Box::new(delete_shared));
                }
                ui.add_space(6.0);
                let packs = info["packs"].as_array().cloned().unwrap_or_default();
                if info["payments"].as_bool() == Some(true) && !packs.is_empty() {
                    ui.label(RichText::new(tl!("Buy credits")).strong());
                    for p in packs {
                        let (Some(id), Some(credits), Some(cents)) =
                            (p["id"].as_str().map(str::to_string), p["credits"].as_u64(), p["cents"].as_u64())
                        else {
                            continue;
                        };
                        ui.horizontal(|ui| {
                            let price = format!("${}.{:02}", cents / 100, cents % 100);
                            ui.label(crate::i18n::fmt(tl!("{n} credits for {price}"), &[("n", &credits.to_string()), ("price", &price)]));
                            if ui.add_enabled(!busy, egui::Button::new(tl!("Buy…"))).clicked() {
                                todo = Some(Box::new(move |app| buy(app, &id)));
                            }
                        });
                    }
                    ui.label(
                        RichText::new(tl!("Payment is on Stripe's secure page, in your browser; your card never reaches StitchFoundry."))
                            .size(11.0)
                            .color(t.text_dim),
                    );
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy, egui::Button::new(tl!("Refresh"))).clicked() {
                        todo = Some(Box::new(refresh));
                    }
                    if ui.add_enabled(!busy, egui::Button::new(tl!("Sign Out"))).clicked() {
                        todo = Some(Box::new(sign_out));
                    }
                });
            }
            if busy {
                ui.add(egui::Spinner::new());
            }
            let st = &mut app.embroidery.account;
            if let Some(n) = &st.note {
                ui.label(RichText::new(crate::i18n::msg(n)).color(t.text_dim));
            }
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            egui::CollapsingHeader::new(tl!("Server")).id_salt("account-server").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let r = ui.add(egui::TextEdit::singleline(&mut st.server).desired_width(300.0).hint_text(default_server()));
                    if r.lost_focus() {
                        let s = st.server.trim().to_string();
                        todo = Some(Box::new(move |app| keep(app, SERVER_NAME, (!s.is_empty()).then_some(s.as_str()))));
                    }
                });
                ui.label(RichText::new(tl!("Leave it empty for StitchFoundry's own service.")).size(11.0).color(t.text_dim));
            });
        });
    if let Some(f) = todo {
        f(app);
    }
    if !open {
        app.embroidery.account.open = false;
    }
}

/// Sign In / Create Account as the buttons do (tests).
#[cfg(test)]
pub(crate) fn test_sign_in(app: &mut VectorcraftApp, create: bool) {
    sign_in(app, create);
}
