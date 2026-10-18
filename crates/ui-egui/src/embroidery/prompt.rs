//! Design from Prompt and Redraw Image (Embroidery menu): describe a design, or select a logo you
//! may use, and Claude draws flat-colour artwork for it, which `embroidery.placeArt` places and
//! digitizes (a redrawn image's art lands on the image). The request goes out with the user's own
//! Anthropic API key through the host's `http_post` service, on a thread of its own, while the
//! dialog shows it drawing; [`poll`] picks the answer up on a later frame.

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use serde_json::{Value, json};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_embroidery::sf_core::ai::{self, Art, ArtError, ArtImage, ArtRequest, cloud};
use vectorcraft_embroidery::{MM_PER_PT, PT_PER_MM};
use vectorcraft_geom::Rect;

use crate::VectorcraftApp;
use crate::state::Dialog;

/// The dialog kind.
pub const KIND: &str = "designFromPrompt";
/// The secret the API key is kept as.
pub const KEY_NAME: &str = "anthropic-api-key";
/// Where to get a key.
pub const KEY_URL: &str = "https://console.anthropic.com/settings/keys";

/// Design from Prompt between frames. Not saved: the key is kept by the host's secret store.
#[derive(Default)]
pub struct PromptState {
    /// The API key as typed or loaded. Never in the dialog's fields, which agents can read.
    pub key: String,
    /// Whether the secret store was read for the key.
    loaded: bool,
    /// The request on its way.
    pending: Option<Pending>,
    /// Why the last request drew nothing, shown in the dialog.
    pub error: Option<String>,
    /// The time of the last frame drawn (egui time, seconds).
    pub now: f64,
}

struct Pending {
    rx: Receiver<Result<Art, ArtError>>,
    /// The key sent, kept once Anthropic takes it (Remember the key).
    keep: Option<String>,
    /// The placed art's name (the description) and size (mm).
    name: String,
    size: f64,
    /// A redrawn image's place, `[x, y, width, height]`: the art lands there.
    frame: Option<[f64; 4]>,
    /// When it was sent (egui time, seconds).
    since: f64,
}

impl PromptState {
    /// Seconds since the request on its way was sent.
    pub fn drawing_for(&self, now: f64) -> Option<f64> {
        self.pending.as_ref().map(|p| (now - p.since).max(0.0))
    }
}

/// Whether this build can send the request (the web can't yet).
pub fn available(app: &VectorcraftApp) -> bool {
    app.services.http_post.is_some() || app.services.http_post_async.is_some()
}

/// Sends AI request `http` (see [`post_ai`]) to the AI service's messages endpoint.
pub(crate) fn post<T: Send + 'static>(
    app: &VectorcraftApp,
    http: crate::HttpRequest,
    thread: &str,
    parse: fn(u16, &str) -> Result<T, ArtError>,
) -> Result<Receiver<Result<T, ArtError>>, String> {
    post_ai(app, http, thread, cloud::MESSAGES, Vec::new(), parse)
}

/// Sends AI request `http` (built with [`super::account::key_to_send`]): to Anthropic with the
/// user's own key, or, when paying with StitchFoundry credits, to `path` on the StitchFoundry
/// service with `extra` headers, its refusals read as the service's (sign in, credits).
pub(crate) fn post_ai<T: Send + 'static>(
    app: &VectorcraftApp,
    http: crate::HttpRequest,
    thread: &str,
    path: &str,
    extra: Vec<(&'static str, String)>,
    parse: fn(u16, &str) -> Result<T, ArtError>,
) -> Result<Receiver<Result<T, ArtError>>, String> {
    let credits = super::account::paying_with_credits(app);
    let http = match super::account::server(app).filter(|_| credits) {
        Some(server) => cloud::through_cloud(http, &server, path, extra).map_err(|e| e.to_string())?,
        None => http,
    };
    post_plain(app, http, thread, move |status, body| {
        if credits && matches!(status, 401 | 402 | 403 | 503) { Err(cloud::status_error(status, body)) } else { parse(status, body) }
    })
}

/// Sends `http` with the host's service and reads the answer with `parse`: on the desktop on a
/// thread named `thread`, on the web through the browser. The result arrives on the receiver;
/// nothing waits for it once the receiver is dropped.
pub(crate) fn post_plain<T: Send + 'static, F: Fn(u16, &str) -> Result<T, ArtError> + Send + 'static>(
    app: &VectorcraftApp,
    http: crate::HttpRequest,
    thread: &str,
    parse: F,
) -> Result<Receiver<Result<T, ArtError>>, String> {
    let (tx, rx) = channel();
    if let Some(post) = app.services.http_post.clone() {
        std::thread::Builder::new()
            .name(thread.into())
            .spawn(move || {
                let r = vectorcraft_engine::guard::catch_panic(|| post(&http))
                    .unwrap_or_else(|msg| Err(format!("internal error: {msg} (please report this bug)")))
                    .map_err(ArtError::Network)
                    .and_then(|(status, body)| parse(status, &body));
                let _ = tx.send(r); // (nothing may wait any more)
            })
            .map_err(|e| format!("can't send the request: {e}"))?;
    } else if let Some(post) = app.services.http_post_async.clone() {
        post(
            http,
            Box::new(move |r| {
                let _ = tx.send(r.map_err(ArtError::Network).and_then(|(status, body)| parse(status, &body))); // (nothing may wait any more)
            }),
        );
    } else {
        return Err("this needs a connection to the AI service, which this build doesn't have".into());
    }
    Ok(rx)
}

/// The size new art is drawn at by default (mm): 90% of the hoop's shorter side, else 100.
fn default_size(app: &VectorcraftApp) -> f64 {
    let hoop = app.session.active().and_then(|st| st.doc.embroidery.as_deref().and_then(|e| e.hoop.clone()));
    hoop.map_or(100.0, |h| (0.9 * h.width.min(h.height)).round())
}

/// The first selected raster image: what Redraw Image redraws → (its id, bounds in points, longest
/// side in pixels).
pub fn selected_image(app: &VectorcraftApp) -> Option<(NodeId, Rect, u32)> {
    let st = app.session.active()?;
    st.selection.objects.iter().find_map(|id| {
        let n = st.doc.node(*id)?;
        let NodeKind::Image(im) = &n.kind else { return None };
        Some((*id, n.geometric_bounds().filter(|b| b.width() > 0.0 && b.height() > 0.0)?, im.width.max(im.height)))
    })
}

/// The dialog can open: the desktop app, a document, and the API key read from the store once.
fn ready(app: &mut VectorcraftApp) -> Result<(), String> {
    if !available(app) {
        return Err("Design from Prompt needs the desktop app for now".into());
    }
    if app.session.active().is_none() {
        return Err("no document".into());
    }
    load_key(app);
    Ok(())
}

/// Read the API key from the secret store once (Design from Prompt and Ask share it).
pub(crate) fn load_key(app: &mut VectorcraftApp) {
    let st = &mut app.embroidery.prompt;
    if !st.loaded {
        st.loaded = true;
        if let Some(k) = app.services.secrets.as_ref().and_then(|s| s.load(KEY_NAME)) {
            st.key = k;
        }
    }
}

/// Open the dialog to redraw the selected image, its fields from `p` (`{prompt?: notes on what to
/// change, size?, colors?}`). Its size is the image's, brought into the hoop (else to 100 mm when
/// it is larger than 200 mm), as Auto-Digitize sizes a traced image.
pub fn open_redraw(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    ready(app)?;
    let (id, bounds, _) = selected_image(app).ok_or("select an image to redraw")?;
    let image_mm = bounds.width().max(bounds.height()) * MM_PER_PT;
    let hoop = app.session.active().and_then(|st| st.doc.embroidery.as_deref().and_then(|e| e.hoop.clone()));
    let fitted = match hoop {
        Some(h) => image_mm.min(0.9 * h.width.min(h.height)),
        None if image_mm > 200.0 => 100.0,
        None => image_mm,
    };
    let size = p.get("size").and_then(Value::as_f64).unwrap_or(fitted.round()).clamp(ai::SIZE_RANGE.0, ai::SIZE_RANGE.1);
    let fields = json!({
        "image": id.0,
        "prompt": p.get("prompt").and_then(Value::as_str).unwrap_or(""),
        "size": size,
        "colors": p.get("colors").and_then(Value::as_u64).unwrap_or(6),
        "remember": true,
    });
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(json!({ "open": true }))
}

/// Image `id` as Claude is sent it (PNG, or JPEG when that is too large; no larger than the model
/// looks at, never enlarged), and the frame its art lands in: the image's place, scaled about its
/// centre so its longest side is `size_mm`.
fn image_to_send(app: &mut VectorcraftApp, id: NodeId, size_mm: f64) -> Result<(ArtImage, [f64; 4]), String> {
    let (_, bounds, px) = selected_image(app).filter(|s| s.0 == id).ok_or("select an image to redraw")?;
    let longest = bounds.width().max(bounds.height());
    let scale = (f64::from(px.clamp(1, ai::IMAGE_SIDE)) / longest).clamp(0.01, 20.0);
    // Straight to the engine: through the menus, Export Selection asks where to save.
    let export = |app: &mut VectorcraftApp, format: &str| -> Result<String, String> {
        let p = json!({ "format": format, "scale": scale, "background": "white", "quality": 85 });
        let r = app.session.execute("document.exportSelection", &p).map_err(|e| e.to_string())?;
        Ok(r.get("dataBase64").and_then(Value::as_str).unwrap_or_default().to_string())
    };
    // Only the image is exported: it alone stays selected.
    app.run("select.set", json!({ "ids": [id.0] }))?;
    let mut image = ArtImage { media_type: "image/png", data_base64: export(app, "png")? };
    if image.data_base64.len() > ai::MAX_IMAGE_BASE64 {
        image = ArtImage { media_type: "image/jpeg", data_base64: export(app, "jpg")? };
    }
    if image.data_base64.is_empty() {
        return Err("the image couldn't be read".into());
    }
    let k = size_mm * PT_PER_MM / longest;
    let (w, h) = (bounds.width() * k, bounds.height() * k);
    let c = bounds.center();
    Ok((image, [c.x - w / 2.0, c.y - h / 2.0, w, h]))
}

/// Open the dialog, its fields from `p` (`{prompt?, size?, colors?}`).
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    ready(app)?;
    let size = p.get("size").and_then(Value::as_f64).unwrap_or_else(|| default_size(app));
    let fields = json!({
        "prompt": p.get("prompt").and_then(Value::as_str).unwrap_or(""),
        "size": size,
        "colors": p.get("colors").and_then(Value::as_u64).unwrap_or(6),
        "remember": true,
    });
    app.ui.dialog = Some(Dialog::new(KIND, fields));
    Ok(json!({ "open": true }))
}

/// Send the dialog's request (Generate). The dialog stays open, showing that it is drawing.
pub fn start(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    if app.embroidery.prompt.pending.is_some() {
        return Err("already drawing: wait for it, or Cancel".into());
    }
    let now = app.embroidery.prompt.now;
    let size = d.f64("size", 100.0);
    let mut request = ArtRequest::new(&d.str("prompt"), size, d.f64("colors", 6.0).round().clamp(0.0, 99.0) as u32);
    // Why nothing was sent, also shown in the dialog.
    let fail = |app: &mut VectorcraftApp, m: String| {
        app.embroidery.prompt.error = Some(m.clone());
        Err(m)
    };
    // Redraw Image: the image goes along.
    let mut frame = None;
    if let Some(id) = d.fields.get("image").and_then(Value::as_u64) {
        let (image, at) = match image_to_send(app, NodeId(id), request.size_mm) {
            Ok(v) => v,
            Err(e) => return fail(app, e),
        };
        request = request.with_image(image);
        frame = Some(at);
    }
    let key = super::account::key_to_send(app);
    let http = match request.http(&key) {
        Ok(h) => h,
        Err(e) => return fail(app, e.to_string()),
    };
    // Remember off forgets a kept key now; on, the key is kept once Anthropic takes it ([`poll`]).
    // Paid with credits, no key goes and none is kept or forgotten.
    let credits = super::account::paying_with_credits(app);
    let keep = (d.bool("remember") && !credits).then(|| app.embroidery.prompt.key.trim().to_string());
    if keep.is_none() && !credits {
        keep_key(app.services.secrets.as_deref(), None);
    }
    let rx = post(app, http, "design-from-prompt", ai::parse_reply)?;
    let name: String = if frame.is_some() { "Redrawn image".to_string() } else { request.prompt.chars().take(60).collect() };
    let st = &mut app.embroidery.prompt;
    st.error = None;
    st.pending = Some(Pending { rx, keep, name, size: request.size_mm, frame, since: now });
    Ok(json!({ "drawing": true }))
}

/// Keep `key` as the API key (`None`: forget it). A store that can't only costs the convenience.
pub(crate) fn keep_key(store: Option<&dyn crate::SecretStore>, key: Option<&str>) {
    if let Some(Err(e)) = store.map(|s| s.save(KEY_NAME, key)) {
        log::warn!("Design from Prompt: the API key: {e}");
    }
}

/// Stop waiting for the request on its way (Cancel). The service may still bill it.
pub fn cancel(app: &mut VectorcraftApp) {
    app.embroidery.prompt.pending = None;
}

/// Each frame: place the art that arrived, or show why none did.
pub fn poll(app: &mut VectorcraftApp, ctx: &egui::Context) {
    app.embroidery.prompt.now = ctx.input(|i| i.time);
    let Some(p) = app.embroidery.prompt.pending.as_ref() else { return };
    let r = match p.rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => {
            // The thread doesn't wake the window: look again shortly.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(ArtError::Network("the request stopped".into())),
    };
    let Some(p) = app.embroidery.prompt.pending.take() else { return };
    // the credits it cost show next time the account is read
    app.embroidery.account.stale |= super::account::paying_with_credits(app);
    // Anthropic took the key unless it said otherwise.
    if let Some(key) = p.keep.as_deref().filter(|_| !matches!(r, Err(ArtError::Key(_)))) {
        keep_key(app.services.secrets.as_deref(), Some(key));
    }
    let open = app.ui.dialog.as_ref().is_some_and(|d| d.kind == KIND);
    let placed = r.map_err(|e| e.to_string()).and_then(|art| {
        if app.session.active().is_none() {
            return Err("no document".to_string());
        }
        let params = match p.frame {
            Some(frame) => json!({ "svg": art.svg, "frame": frame, "name": p.name }),
            None => json!({ "svg": art.svg, "size": p.size, "name": p.name }),
        };
        app.run("embroidery.placeArt", params)
    });
    match placed {
        Ok(v) => {
            if open {
                app.ui.dialog = None;
            }
            let n = v.get("objects").and_then(Value::as_u64).unwrap_or(0);
            app.status(crate::i18n::tn(n, "Design placed: {n} stitched object", "Design placed: {n} stitched objects"));
        }
        Err(e) => {
            if !open {
                app.status(crate::i18n::msg(&e));
            }
            app.embroidery.prompt.error = Some(e);
        }
    }
}
