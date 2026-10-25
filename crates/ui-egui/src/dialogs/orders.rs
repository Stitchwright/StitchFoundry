//! Online Orders (ORD-01): the shop's orders fetched (Shopify's GraphQL Admin API with the store's
//! own app's client ID and secret; WooCommerce's REST API with a read-only key) or read from an
//! orders CSV export, and made into designs, job orders, machine files and worksheets by the
//! rules (`embroidery.orders.import`). The keys are kept by the host's secret store, never in the
//! settings. Desktop only: a browser can't call a shop's admin API.
//!
//! Shopify: https://shopify.dev/docs/apps/build/authentication-authorization/access-tokens/client-credentials-grant
//! (a 24-hour token from the client ID and secret) and
//! https://shopify.dev/docs/api/admin-graphql/latest/queries/orders. WooCommerce:
//! https://woocommerce.github.io/woocommerce-rest-api-docs/#list-all-orders (HTTP Basic auth over
//! HTTPS with the consumer key and secret).

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};

/// Shopify's GraphQL Admin API version asked for (its quarterly releases; the newest at writing).
const SHOPIFY_API: &str = "2026-10";
/// How the setup guide and each shop's own instructions are reached.
pub const GUIDE_URL: &str = "https://github.com/Stitchwright/StitchFoundry/blob/main/docs/online-orders.md";
const SHOPIFY_DASHBOARD: &str = "https://dev.shopify.com/dashboard/";
const SHOPIFY_HELP: &str = "https://shopify.dev/docs/apps/build/authentication-authorization/access-tokens/client-credentials-grant";
const WOO_HELP: &str = "https://woocommerce.com/document/woocommerce-rest-api/";
/// The secrets the keys are kept as.
const SECRET_ID: &str = "orders-client-id";
const SECRET_KEY: &str = "orders-client-secret";
/// Orders asked for at once (the shops' own page sizes allow it).
const PAGE: u32 = 50;
/// Checking by itself: how often (seconds) while the app is open.
const EVERY: f64 = 15.0 * 60.0;

/// A rule as typed in the window.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuleDraft {
    pub product: String,
    pub template: String,
    pub font: String,
    pub color: String,
    pub size: String,
}

/// The Online Orders window.
#[derive(Default)]
pub struct OrdersState {
    pub open: bool,
    /// The setup as typed: shopify, woocommerce or csv; the store's address; the folder; the
    /// machine formats ("dst, pes").
    pub shop: String,
    pub store: String,
    pub folder: String,
    pub formats: String,
    pub rules: Vec<RuleDraft>,
    /// The keys as typed or read from the secret store: Shopify's client ID and secret, or
    /// WooCommerce's consumer key and secret.
    pub id: String,
    pub secret: String,
    /// The setup and keys were read in.
    pub loaded: bool,
    /// A fetch on its way: what it fetched ((source, answer)), or why it couldn't.
    pub fetching: Option<Receiver<Result<(String, String), String>>>,
    /// The last import's answer, or what went wrong.
    pub result: Option<Result<Value, String>>,
    /// Check by itself while open, and when it last did (seconds since the app started).
    pub auto: bool,
    pub last: f64,
    /// A job or order number typed or scanned, and the job it found.
    pub scan: String,
    pub found: Option<Result<Value, String>>,
}

/// `ui.onlineOrders {check?: bool}`: the window opens; with `check`, the shop is checked at once.
pub fn run(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    load(app);
    app.embroidery.orders.open = true;
    if p.get("check").and_then(Value::as_bool) == Some(true) {
        check(app)?;
        return Ok(json!({ "open": true, "checking": true }));
    }
    Ok(json!({ "open": true }))
}

/// The setup from the preferences and the keys from the secret store, once.
fn load(app: &mut VectorcraftApp) {
    if app.embroidery.orders.loaded {
        return;
    }
    let set = app.session.execute("embroidery.orders.setup", &json!({})).unwrap_or_default();
    let id = app.services.secrets.as_ref().and_then(|s| s.load(SECRET_ID)).unwrap_or_default();
    let secret = app.services.secrets.as_ref().and_then(|s| s.load(SECRET_KEY)).unwrap_or_default();
    let st = &mut app.embroidery.orders;
    st.loaded = true;
    st.shop = set["shop"].as_str().filter(|s| !s.is_empty()).unwrap_or("shopify").to_string();
    st.store = set["store"].as_str().unwrap_or_default().to_string();
    st.folder = set["folder"].as_str().unwrap_or_default().to_string();
    st.formats = set["formats"].as_array().map(|f| f.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    st.rules = set["rules"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|r| RuleDraft {
            product: r["product"].as_str().unwrap_or_default().into(),
            template: r["template"].as_str().unwrap_or_default().into(),
            font: r["font"].as_str().unwrap_or_default().into(),
            color: r["color"].as_str().unwrap_or_default().into(),
            size: r["size"].as_f64().map(|v| format!("{v}")).unwrap_or_default(),
        })
        .collect();
    if st.rules.is_empty() {
        st.rules.push(RuleDraft { template: "Name tape 100 × 30".into(), ..RuleDraft::default() });
    }
    (st.id, st.secret) = (id, secret);
}

/// The setup as typed, kept (the keys in the secret store).
fn save(app: &mut VectorcraftApp) -> Result<(), String> {
    let st = &app.embroidery.orders;
    let formats: Vec<String> = st.formats.split([',', ' ']).map(str::trim).filter(|f| !f.is_empty()).map(str::to_lowercase).collect();
    let rules: Vec<Value> = st
        .rules
        .iter()
        .map(|r| {
            let mut v = json!({ "product": r.product.trim(), "template": r.template.trim(), "font": r.font.trim(), "color": r.color.trim() });
            if let Ok(size) = r.size.trim().replace(',', ".").parse::<f64>() {
                v["size"] = json!(size);
            }
            v
        })
        .collect();
    let q = json!({ "shop": st.shop, "store": st.store.trim(), "folder": st.folder.trim(), "formats": formats, "rules": rules });
    let (id, secret) = (st.id.trim().to_string(), st.secret.trim().to_string());
    app.session.execute("embroidery.orders.setup", &q).map_err(|e| e.to_string())?;
    if let Some(s) = app.services.secrets.as_ref() {
        // an empty field forgets its key
        s.save(SECRET_ID, Some(id.as_str()).filter(|v| !v.is_empty())).map_err(|e| format!("couldn't keep the keys: {e}"))?;
        s.save(SECRET_KEY, Some(secret.as_str()).filter(|v| !v.is_empty())).map_err(|e| format!("couldn't keep the keys: {e}"))?;
    }
    Ok(())
}

/// A Shopify store's host: `name`, `name.myshopify.com` or its address → `name.myshopify.com`;
/// only letters, digits, dots and hyphens.
pub fn shopify_host(store: &str) -> Option<String> {
    let s = store.trim().trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_lowercase();
    let s = s.split('/').next().unwrap_or_default().to_string();
    let host = if s.contains('.') { s } else { format!("{s}.myshopify.com") };
    let ok =
        !host.is_empty() && host.len() <= 255 && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') && !host.starts_with('.');
    ok.then_some(host)
}

/// A WooCommerce shop's address, HTTPS only (its keys go along): `https://example.com`.
pub fn woocommerce_base(store: &str) -> Option<String> {
    let s = store.trim().trim_end_matches('/');
    let rest = s.strip_prefix("https://")?;
    let ok = !rest.is_empty() && rest.len() <= 300 && !rest.contains(['?', '#', ' ', '@']);
    ok.then(|| s.to_string())
}

/// `text` in base 64 (for HTTP Basic auth).
fn base64(text: &str) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let b = text.as_bytes();
    let mut out = String::new();
    for chunk in b.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, v)| n | (u32::from(*v) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `text` for a form field (application/x-www-form-urlencoded).
fn form(text: &str) -> String {
    text.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { char::from(b).to_string() } else { format!("%{b:02X}") }).collect()
}

/// What a shop's refusal means, in plain words.
fn refused(shop: &str, status: u16, body: &str) -> String {
    match status {
        401 | 403 => format!(
            "{shop} refused the keys ({status}). Check them, and that the app may read orders{}.",
            if shop == "Shopify" { " (read_orders, approved on the store)" } else { " (a Read key)" }
        ),
        404 => format!("{shop} has no such store or page (404): check the store's address."),
        429 => format!("{shop} asks to wait a little (429): try again in a minute."),
        _ => format!("{shop} answered {status}: {}", body.chars().take(200).collect::<String>()),
    }
}

/// The shop's orders fetched on a thread of their own (desktop): (source, the shop's answer).
fn fetch(post: crate::HttpPostFn, shop: String, store: String, id: String, secret: String) -> Result<(String, String), String> {
    use crate::{HttpRequest, Method};
    match shop.as_str() {
        "shopify" => {
            let host = shopify_host(&store).ok_or("the store's address is name.myshopify.com")?;
            // a token for 24 hours from the app's client ID and secret
            let body = format!("grant_type=client_credentials&client_id={}&client_secret={}", form(&id), form(&secret));
            let token_req = HttpRequest {
                method: Method::Post,
                url: format!("https://{host}/admin/oauth/access_token"),
                headers: vec![("Content-Type", "application/x-www-form-urlencoded".into()), ("Accept", "application/json".into())],
                body,
                bytes: None,
            };
            let (status, answer) = post(&token_req)?;
            if !(200..300).contains(&status) {
                return Err(refused("Shopify", status, &answer));
            }
            let token = serde_json::from_str::<Value>(&answer).ok().and_then(|v| v["access_token"].as_str().map(str::to_string));
            let token = token.ok_or("Shopify gave no access token: check the client ID and secret")?;
            // the open orders not yet fulfilled, newest first, each item's personalization
            let query = format!(
                "{{ orders(first: {PAGE}, query: \"fulfillment_status:unfulfilled status:open\", sortKey: CREATED_AT, reverse: true) {{ edges {{ node {{ id name email customer {{ displayName }} shippingAddress {{ name }} lineItems(first: 50) {{ edges {{ node {{ id title name sku quantity unfulfilledQuantity customAttributes {{ key value }} }} }} }} }} }} }} }}"
            );
            let req = HttpRequest {
                method: Method::Post,
                url: format!("https://{host}/admin/api/{SHOPIFY_API}/graphql.json"),
                headers: vec![("Content-Type", "application/json".into()), ("X-Shopify-Access-Token", token)],
                body: json!({ "query": query }).to_string(),
                bytes: None,
            };
            let (status, answer) = post(&req)?;
            if !(200..300).contains(&status) {
                return Err(refused("Shopify", status, &answer));
            }
            Ok(("shopify".into(), answer))
        }
        "woocommerce" => {
            let base = woocommerce_base(&store).ok_or("the shop's address starts https:// (its keys only go over HTTPS)")?;
            let req = HttpRequest {
                method: Method::Get,
                url: format!("{base}/wp-json/wc/v3/orders?status=processing&per_page={PAGE}"),
                headers: vec![("Authorization", format!("Basic {}", base64(&format!("{id}:{secret}")))), ("Accept", "application/json".into())],
                body: String::new(),
                bytes: None,
            };
            let (status, answer) = post(&req)?;
            if !(200..300).contains(&status) {
                return Err(refused("WooCommerce", status, &answer));
            }
            Ok(("woocommerce".into(), answer))
        }
        _ => Err("this shop's orders come from a CSV file: Make Designs from a CSV File…".into()),
    }
}

/// Check the shop for new orders: the setup kept, the fetch started.
pub fn check(app: &mut VectorcraftApp) -> Result<(), String> {
    load(app);
    save(app)?;
    let post = app.services.http_post.clone().ok_or("online orders need the desktop app (a browser can't reach a shop's admin API)")?;
    let st = &mut app.embroidery.orders;
    if st.fetching.is_some() {
        return Ok(());
    }
    if st.id.trim().is_empty() || st.secret.trim().is_empty() {
        return Err("enter the shop's keys first (How to Connect shows where to get them)".into());
    }
    let (shop, store, id, secret) = (st.shop.clone(), st.store.clone(), st.id.trim().to_string(), st.secret.trim().to_string());
    let (tx, rx) = channel();
    std::thread::Builder::new()
        .name("online-orders".into())
        .spawn(move || {
            let r = vectorcraft_engine::guard::catch_panic(|| fetch(post, shop, store, id, secret))
                .unwrap_or_else(|msg| Err(format!("internal error: {msg} (please report this bug)")));
            let _ = tx.send(r); // (nothing may wait any more)
        })
        .map_err(|e| format!("can't reach the shop: {e}"))?;
    st.fetching = Some(rx);
    st.result = None;
    Ok(())
}

/// The orders fetched made into designs.
fn import(app: &mut VectorcraftApp, source: &str, data: String) {
    let r = app.run("embroidery.orders.import", json!({ "source": source, "data": data }));
    if let Ok(v) = &r {
        let made = v["made"].as_array().map_or(0, Vec::len);
        app.status(crate::i18n::fmt(tl!("Online orders: {n} designs made"), &[("n", &made.to_string())]));
    }
    app.embroidery.orders.result = Some(r);
}

/// Each frame: a fetch that has come back made into designs, and the check by itself when due.
pub fn tick(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let back = match app.embroidery.orders.fetching.as_ref().map(Receiver::try_recv) {
        Some(Ok(r)) => Some(r),
        Some(Err(TryRecvError::Disconnected)) => Some(Err("the shop didn't answer".to_string())),
        _ => None,
    };
    if let Some(r) = back {
        app.embroidery.orders.fetching = None;
        app.embroidery.orders.last = now;
        match r {
            Ok((source, data)) => import(app, &source, data),
            Err(e) => app.embroidery.orders.result = Some(Err(e)),
        }
    } else if app.embroidery.orders.fetching.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(300));
    }
    let st = &app.embroidery.orders;
    if st.auto && st.fetching.is_none() && st.shop != "csv" && now - st.last > EVERY {
        app.embroidery.orders.last = now;
        if let Err(e) = check(app) {
            app.embroidery.orders.result = Some(Err(e));
        }
    }
}

/// The window, while open.
pub fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    tick(app, ctx);
    if !app.embroidery.orders.open {
        return;
    }
    load(app);
    let mut open = true;
    egui::Window::new(tl!("Online Orders"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(280.0, 300.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.set_width(560.0);
            let body = (ui.ctx().content_rect().height() - 120.0).max(200.0);
            egui::ScrollArea::vertical().max_height(body).auto_shrink([false, true]).show(ui, |ui| body_ui(app, ui, &t));
        });
    if !open {
        if let Err(e) = save(app) {
            app.status(e);
        }
        app.embroidery.orders.open = false;
    }
}

fn body_ui(app: &mut VectorcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    use egui::RichText;
    ui.label(
        RichText::new(tl!("New orders from your online shop made into designs: the name or text the customer typed, in the font and thread colour the rules give, with a job, machine files and a worksheet for each."))
            .size(13.0)
            .color(t.text_dim),
    );
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("Your shop")).font(theme::semibold(14.0)).color(t.text));
    ui.horizontal(|ui| {
        for (id, label) in [("shopify", "Shopify"), ("woocommerce", "WooCommerce"), ("csv", tl!("Another shop (CSV file)"))] {
            if ui.selectable_label(app.embroidery.orders.shop == id, label).clicked() {
                app.embroidery.orders.shop = id.to_string();
            }
        }
    });
    let shop = app.embroidery.orders.shop.clone();
    if shop != "csv" {
        let (hint, id_label, secret_label) = match shop.as_str() {
            "shopify" => ("your-store.myshopify.com", tl!("Client ID"), tl!("Client secret")),
            _ => ("https://your-shop.com", tl!("Consumer key"), tl!("Consumer secret")),
        };
        egui::Grid::new("orders-connect").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(tl!("Store address"));
            ui.add(egui::TextEdit::singleline(&mut app.embroidery.orders.store).hint_text(hint).desired_width(300.0));
            ui.end_row();
            ui.label(id_label);
            ui.add(egui::TextEdit::singleline(&mut app.embroidery.orders.id).password(true).desired_width(300.0));
            ui.end_row();
            ui.label(secret_label);
            ui.add(egui::TextEdit::singleline(&mut app.embroidery.orders.secret).password(true).desired_width(300.0));
            ui.end_row();
        });
    }
    ui.horizontal_wrapped(|ui| {
        if ui.link(tl!("How to Connect")).clicked() {
            app.open_url(GUIDE_URL);
        }
        let links: &[(&str, &str)] = match shop.as_str() {
            "shopify" => &[("Shopify Dev Dashboard", SHOPIFY_DASHBOARD), ("Shopify: client credentials", SHOPIFY_HELP)],
            "woocommerce" => &[("WooCommerce: REST API keys", WOO_HELP)],
            _ => &[],
        };
        for (label, url) in links {
            if ui.link(*label).clicked() {
                app.open_url(url);
            }
        }
    });
    ui.add_space(6.0);
    ui.label(RichText::new(tl!("Rules")).font(theme::semibold(14.0)).color(t.text));
    ui.label(
        RichText::new(tl!("The first rule whose product matches makes the item. Leave Product empty to take any.")).size(12.0).color(t.text_dim),
    );
    let templates: Vec<String> = app
        .session
        .execute("embroidery.templates", &json!({}))
        .ok()
        .and_then(|v| v["templates"].as_array().map(|a| a.iter().filter_map(|t| t["name"].as_str().map(str::to_string)).collect()))
        .unwrap_or_default();
    let fonts: Vec<String> = app.session.embroidery_fonts.iter().map(|f| f.info.name.clone()).collect();
    let mut remove = None;
    egui::Grid::new("orders-rules").num_columns(6).spacing([6.0, 4.0]).show(ui, |ui| {
        for h in [tl!("Product"), tl!("Template"), tl!("Font"), tl!("Thread"), tl!("Size (mm)"), ""] {
            ui.label(RichText::new(h).size(12.0).color(t.text_dim));
        }
        ui.end_row();
        for (i, r) in app.embroidery.orders.rules.iter_mut().enumerate() {
            ui.add(egui::TextEdit::singleline(&mut r.product).hint_text(tl!("any")).desired_width(90.0));
            choose(ui, egui::Id::new(("orders-template", i)), &mut r.template, &templates, 120.0);
            choose(ui, egui::Id::new(("orders-font", i)), &mut r.font, &fonts, 110.0);
            ui.add(egui::TextEdit::singleline(&mut r.color).hint_text("Navy").desired_width(60.0));
            ui.add(egui::TextEdit::singleline(&mut r.size).hint_text(tl!("fit")).desired_width(40.0));
            if ui.small_button("✕").on_hover_text(tl!("Remove this rule")).clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        app.embroidery.orders.rules.remove(i);
    }
    if ui.button(tl!("Add a Rule")).clicked() && app.embroidery.orders.rules.len() < 100 {
        app.embroidery.orders.rules.push(RuleDraft::default());
    }
    ui.add_space(6.0);
    egui::Grid::new("orders-out").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label(tl!("Save designs in"));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.embroidery.orders.folder).hint_text(tl!("My Designs/Orders")).desired_width(240.0));
            if ui.button(tl!("Choose Folder…")).clicked() {
                crate::picks::button(app, |app| {
                    if let Some(f) = crate::picks::folder(app) {
                        app.embroidery.orders.folder = f;
                    }
                });
            }
        });
        ui.end_row();
        ui.label(tl!("Machine files"));
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.orders.formats).hint_text("dst, pes").desired_width(120.0));
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let busy = app.embroidery.orders.fetching.is_some();
        if shop == "csv" {
            if crate::widgets::primary_button(ui, tl!("Make Designs from a CSV File…")).clicked() {
                if let Err(e) = save(app) {
                    app.embroidery.orders.result = Some(Err(e));
                } else {
                    crate::picks::button(app, |app| {
                        let pick = crate::FilePick { filters: vec![("CSV", &["csv", "txt"])], ..Default::default() };
                        if let Some(path) = crate::picks::open(app, &pick) {
                            match std::fs::read_to_string(&path) {
                                Ok(text) => import(app, "csv", text),
                                Err(e) => app.embroidery.orders.result = Some(Err(format!("can't read {path}: {e}"))),
                            }
                        }
                    });
                }
            }
        } else {
            ui.add_enabled_ui(!busy, |ui| {
                if crate::widgets::primary_button(ui, tl!("Check for New Orders")).clicked()
                    && let Err(e) = check(app)
                {
                    app.embroidery.orders.result = Some(Err(e));
                }
            });
            if busy {
                ui.add(egui::Spinner::new());
            }
            ui.checkbox(&mut app.embroidery.orders.auto, tl!("Check every 15 minutes while open"));
        }
    });
    match &app.embroidery.orders.result {
        Some(Ok(v)) => {
            let made = v["made"].as_array().map(Vec::as_slice).unwrap_or_default();
            let skipped = v["skipped"].as_array().map(Vec::as_slice).unwrap_or_default();
            ui.label(
                RichText::new(crate::i18n::fmt(
                    tl!("{n} designs made, {m} to look at"),
                    &[("n", &made.len().to_string()), ("m", &skipped.len().to_string())],
                ))
                .font(theme::semibold(13.0))
                .color(t.text),
            );
            for m in made.iter().take(50) {
                let line = format!(
                    "{} {} · {} · {}",
                    m["job"].as_str().unwrap_or(""),
                    m["order"].as_str().unwrap_or(""),
                    m["customer"].as_str().unwrap_or(""),
                    m["text"].as_str().unwrap_or("").replace('\n', " / ")
                );
                ui.label(RichText::new(line).size(12.0));
            }
            for s in skipped.iter().take(50) {
                let line = format!(
                    "{} {}: {}",
                    s["order"].as_str().unwrap_or(""),
                    s["product"].as_str().unwrap_or(""),
                    crate::i18n::msg(s["why"].as_str().unwrap_or(""))
                );
                ui.label(RichText::new(line).size(12.0).color(t.text_dim));
            }
        }
        Some(Err(e)) => {
            ui.label(RichText::new(crate::i18n::msg(e)).size(13.0).color(t.error));
        }
        None => {}
    }
    ui.add_space(8.0);
    ui.separator();
    ui.label(RichText::new(tl!("Find a job")).font(theme::semibold(14.0)).color(t.text));
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.embroidery.orders.scan)
                .hint_text(tl!("Scan a worksheet's barcode, or type J-0001 or #1001"))
                .desired_width(300.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button(tl!("Find")).clicked() || enter) && !app.embroidery.orders.scan.trim().is_empty() {
            let code = std::mem::take(&mut app.embroidery.orders.scan);
            app.embroidery.orders.found =
                Some(app.session.execute("embroidery.job.find", &json!({ "code": code.trim() })).map_err(|e| e.to_string()));
        }
    });
    let mut open_job = None;
    match &app.embroidery.orders.found {
        Some(Ok(v)) => {
            let j = &v["job"];
            ui.label(format!(
                "{} · {} · {} × {}",
                j["number"].as_str().unwrap_or(""),
                j["customer"].as_str().unwrap_or(""),
                j["quantity"].as_u64().unwrap_or(1),
                j["garment"].as_str().unwrap_or("")
            ));
            ui.label(RichText::new(j["notes"].as_str().unwrap_or("")).size(12.0).color(t.text_dim));
            if ui.button(tl!("Open the Design")).clicked() {
                open_job = j["design"].as_str().map(str::to_string);
            }
        }
        Some(Err(e)) => {
            ui.label(RichText::new(crate::i18n::msg(e)).size(13.0).color(t.error));
        }
        None => {}
    }
    if let Some(path) = open_job.filter(|p| !p.is_empty())
        && let Err(e) = app.run("file.open", json!({ "path": path }))
    {
        app.status(e);
    }
}

/// A text field with a list of choices beside it.
fn choose(ui: &mut egui::Ui, id: egui::Id, value: &mut String, choices: &[String], width: f32) {
    let shown = if value.is_empty() { tl!("first").to_string() } else { value.clone() };
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(width).show_ui(ui, |ui| {
        for c in choices {
            if ui.selectable_label(value == c, c).clicked() {
                *value = c.clone();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VectorcraftApp;

    #[test]
    fn store_addresses_and_encodings() {
        assert_eq!(shopify_host("rose-towels").as_deref(), Some("rose-towels.myshopify.com"));
        assert_eq!(shopify_host("https://Rose-Towels.myshopify.com/admin").as_deref(), Some("rose-towels.myshopify.com"));
        assert_eq!(shopify_host("evil.com/x?y"), Some("evil.com".into()), "only the host is kept");
        assert_eq!(shopify_host("a b"), None);
        assert_eq!(woocommerce_base("https://shop.example.com/").as_deref(), Some("https://shop.example.com"));
        assert_eq!(woocommerce_base("http://shop.example.com"), None, "keys only go over HTTPS");
        assert_eq!(woocommerce_base("https://user@evil.com"), None);
        assert_eq!(base64("ck_1:cs_2"), "Y2tfMTpjc18y");
        assert_eq!(base64("ab"), "YWI=");
        assert_eq!(form("a b&c=d"), "a%20b%26c%3Dd");
        assert!(refused("Shopify", 403, "").contains("read_orders"));
    }

    /// A secret store in memory.
    struct Secrets(std::sync::Mutex<std::collections::HashMap<String, String>>);
    impl crate::SecretStore for Secrets {
        fn load(&self, name: &str) -> Option<String> {
            self.0.lock().unwrap().get(name).cloned()
        }
        fn save(&self, name: &str, value: Option<&str>) -> Result<(), String> {
            let mut m = self.0.lock().unwrap();
            match value {
                Some(v) => m.insert(name.into(), v.into()),
                None => m.remove(name),
            };
            Ok(())
        }
    }

    /// ORD-01: Check for New Orders against a shop that answers as Shopify's documentation says:
    /// a token from the client ID and secret, then the orders; the design, job and files made.
    #[test]
    fn shopify_orders_are_fetched_and_made() {
        use std::sync::{Arc, Mutex};
        let asked: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
        let log = asked.clone();
        let post: crate::HttpPostFn = Arc::new(move |r: &crate::HttpRequest| {
            log.lock().unwrap().push((r.url.clone(), r.body.clone()));
            if r.url.ends_with("/admin/oauth/access_token") {
                return Ok((200, r#"{"access_token":"shpat_test","scope":"read_orders","expires_in":86399}"#.into()));
            }
            assert!(r.headers.iter().any(|(k, v)| *k == "X-Shopify-Access-Token" && v == "shpat_test"));
            let answer = json!({"data": {"orders": {"edges": [{"node": {"id": "gid://shopify/Order/9", "name": "#1009", "customer": {"displayName": "Dot Ng"},
                "lineItems": {"edges": [{"node": {"id": "gid://shopify/LineItem/91", "title": "Name Tape", "sku": "NT", "quantity": 1, "unfulfilledQuantity": 1,
                    "customAttributes": [{"key": "Name", "value": "Rose"}]}}]}}}]}}});
            Ok((200, answer.to_string()))
        });
        let secrets = Secrets(Mutex::default());
        let services = crate::Services { http_post: Some(post), secrets: Some(Box::new(secrets)), ..Default::default() };
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), services);
        app.session.execute("embroidery.fontDoc.fromType", &json!({"family": "Source Sans 3", "chars": "Rose", "size": 20})).unwrap();
        app.session.execute("embroidery.fontDoc.save", &json!({})).unwrap();
        app.session.execute("file.close", &json!({})).ok();
        let dir = std::env::temp_dir().join(format!("sf-ui-orders-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        app.run("ui.onlineOrders", json!({})).unwrap();
        let st = &mut app.embroidery.orders;
        (st.shop, st.store, st.id, st.secret, st.folder) =
            ("shopify".into(), "dots-tapes".into(), "id 1".into(), "s&2".into(), dir.to_string_lossy().into());
        app.run("ui.onlineOrders", json!({"check": true})).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..200 {
            ctx.run_ui(egui::RawInput::default(), |ui| super::tick(&mut app, ui.ctx())).textures_delta.clear();
            if app.embroidery.orders.result.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let r = app.embroidery.orders.result.clone().expect("an answer").expect("made");
        assert_eq!(r["made"][0]["customer"], "Dot Ng", "{r}");
        let asked = asked.lock().unwrap().clone();
        assert_eq!(asked[0].0, "https://dots-tapes.myshopify.com/admin/oauth/access_token");
        assert_eq!(asked[0].1, "grant_type=client_credentials&client_id=id%201&client_secret=s%262", "form-encoded");
        assert!(asked[1].0.ends_with("/admin/api/2026-10/graphql.json") && asked[1].1.contains("customAttributes"), "{asked:?}");
        // the keys in the secret store, not the settings
        assert_eq!(app.services.secrets.as_ref().unwrap().load(SECRET_KEY).as_deref(), Some("s&2"));
        let set = app.session.execute("embroidery.orders.setup", &json!({})).unwrap().to_string();
        assert!(!set.contains("s&2") && set.contains("dots-tapes"), "{set}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
