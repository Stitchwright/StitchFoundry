//! Open in StitchFoundry (OPEN-01): `?open=<address>` opens a design file from another site, so
//! MakerFoundry's design pages can link straight into the web app. Only addresses on the sites in
//! [`ALLOWED_HOSTS`] are fetched, over HTTPS, without cookies, and at most [`MAX_BYTES`]; the file
//! then opens as a dropped one does (through the inbox). The site must allow this page's origin
//! (CORS).

/// The sites a design may be opened from.
pub const ALLOWED_HOSTS: &[&str] = &["makerfoundry.app", "www.makerfoundry.app"];
/// The largest file fetched (bytes).
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
/// The longest address accepted (characters, decoded).
const MAX_ADDRESS: usize = 2000;

/// The address in `raw` (the `open=` value, percent-encoded) when it may be fetched: HTTPS, on an
/// allowed site, with no user name, port or spaces; else why not.
pub fn allowed(raw: &str) -> Result<String, &'static str> {
    let url = percent_decode(raw).ok_or("the link to the design isn't a valid address")?;
    if url.len() > MAX_ADDRESS || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("the link to the design isn't a valid address");
    }
    let rest = url.strip_prefix("https://").ok_or("designs open only from https:// addresses")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.contains(['@', ':']) || !ALLOWED_HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
        return Err("designs open only from MakerFoundry");
    }
    Ok(url)
}

/// Whether a file of `len` bytes may be opened (at most [`MAX_BYTES`]).
pub fn fits(len: usize) -> bool {
    len <= MAX_BYTES
}

/// A design's bytes as they download, refused as soon as they would pass [`MAX_BYTES`], so a
/// reply without a `Content-Length` (or with a false one) can't fill the tab's memory.
#[derive(Default)]
pub struct Capped {
    bytes: Vec<u8>,
}

impl Capped {
    /// Whether `len` more bytes fit.
    pub fn room_for(&self, len: usize) -> bool {
        fits(self.bytes.len().saturating_add(len))
    }

    /// Adds `chunk` → false (nothing added) when it doesn't fit.
    pub fn push(&mut self, chunk: &[u8]) -> bool {
        if !self.room_for(chunk.len()) {
            return false;
        }
        self.bytes.extend_from_slice(chunk);
        true
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// The name the file opens under: the `filename` of a `Content-Disposition` header when the site
/// exposes it, else the last part of the address's path; never a path, never empty.
pub fn file_name(url: &str, disposition: Option<&str>) -> String {
    let from_header = disposition.and_then(|d| {
        let at = d.find("filename=")?;
        let v = d[at + "filename=".len()..].split(';').next()?.trim().trim_matches('"');
        Some(v.to_string())
    });
    let from_url = || {
        let path = url.split(['?', '#']).next().unwrap_or_default();
        path.rsplit('/').next().and_then(percent_decode).unwrap_or_default()
    };
    let name = from_header.filter(|n| !n.is_empty()).unwrap_or_else(from_url);
    let name: String = name.rsplit(['/', '\\']).next().unwrap_or_default().chars().filter(|c| !c.is_control()).take(120).collect();
    let name = name.trim().trim_start_matches('.').to_string();
    if name.is_empty() { "design".into() } else { name }
}

/// The longest error sentence shown from the site (characters).
const MAX_MESSAGE: usize = 300;

/// What the status bar says when the site refuses the design: its own sentence when the body is
/// MakerFoundry's error JSON (`{"error":{"message":"…"}}`), else the status code. The sentence is
/// untrusted text, so control characters are dropped and it is cut to [`MAX_MESSAGE`].
pub fn refusal(status: u16, body: &str) -> String {
    let sentence = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("error")?.get("message")?.as_str().map(str::to_string))
        .map(|m| m.chars().filter(|c| !c.is_control()).take(MAX_MESSAGE).collect::<String>())
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    sentence.unwrap_or_else(|| format!("Couldn't open the design from MakerFoundry: it answered {status}."))
}

/// The page's query string (`?a=1&open=…&b=2`, with or without its `?`) without the `name`
/// parameter, so a reload or a shared link doesn't open the design again: `?a=1&b=2`, or empty.
pub fn without_param(query: &str, name: &str) -> String {
    let kept: Vec<&str> = query.trim_start_matches('?').split('&').filter(|kv| !kv.is_empty() && kv.split('=').next() != Some(name)).collect();
    if kept.is_empty() { String::new() } else { format!("?{}", kept.join("&")) }
}

/// `s` with `%XX` escapes (and `+` as a space, as in a query) decoded, when that makes UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        match c {
            b'%' => {
                let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Why a design couldn't be opened: the site refused it (its own sentence, shown as it is), or
/// anything else (shown after "Couldn't open the design from MakerFoundry:").
#[cfg(target_arch = "wasm32")]
enum Failure {
    Refused(String),
    Other(String),
}

#[cfg(target_arch = "wasm32")]
impl From<String> for Failure {
    fn from(e: String) -> Self {
        Failure::Other(e)
    }
}

#[cfg(target_arch = "wasm32")]
impl From<&str> for Failure {
    fn from(e: &str) -> Self {
        Failure::Other(e.to_string())
    }
}

/// Fetch the design at `url` and put it in `inbox`, which opens it; `note` gets the message the
/// status bar shows when it can't be opened.
#[cfg(target_arch = "wasm32")]
pub fn start(url: String, inbox: vectorcraft_ui_egui::Inbox, note: std::rc::Rc<std::cell::RefCell<Option<String>>>, ctx: egui::Context) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let js = |e: wasm_bindgen::JsValue| e.as_string().unwrap_or_else(|| format!("{e:?}"));
    wasm_bindgen_futures::spawn_local(async move {
        let got: Result<(String, Vec<u8>), Failure> = async {
            let window = web_sys::window().ok_or("no window")?;
            // a cross-site request sends no cookies (credentials: same-origin, the default)
            let init = web_sys::RequestInit::new();
            init.set_method("GET");
            let response: web_sys::Response =
                JsFuture::from(window.fetch_with_str_and_init(&url, &init)).await.map_err(js)?.dyn_into().map_err(js)?;
            if !response.ok() {
                // MakerFoundry's own sentence ("That design isn't on MakerFoundry."), shown as it is.
                let status = response.status();
                let body = match response.text() {
                    Ok(p) => JsFuture::from(p).await.ok().and_then(|t| t.as_string()).unwrap_or_default(),
                    Err(_) => String::new(),
                };
                let body: String = body.chars().take(64 * 1024).collect();
                return Err(Failure::Refused(refusal(status, &body)));
            }
            let long = response.headers().get("content-length").ok().flatten().and_then(|n| n.parse::<usize>().ok()).is_some_and(|n| !fits(n));
            if long {
                return Err("the design is too big to open".into());
            }
            let disposition = response.headers().get("content-disposition").ok().flatten();
            // read piece by piece, stopping at the cap: the length the site gives may be missing
            let body = response.body().ok_or("the site sent no file")?;
            let reader: web_sys::ReadableStreamDefaultReader = body.get_reader().dyn_into().map_err(|o: js_sys::Object| js(o.into()))?;
            let mut got = Capped::default();
            loop {
                let step = JsFuture::from(reader.read()).await.map_err(js)?;
                let done = js_sys::Reflect::get(&step, &"done".into()).map_err(js)?.as_bool().unwrap_or(true);
                if done {
                    break;
                }
                let value = js_sys::Reflect::get(&step, &"value".into()).map_err(js)?;
                let chunk: js_sys::Uint8Array = value.dyn_into().map_err(js)?;
                if !got.room_for(chunk.length() as usize) {
                    // (the download is given up; whether the site hears of it doesn't matter)
                    let _ = reader.cancel();
                    return Err("the design is too big to open".into());
                }
                got.push(&chunk.to_vec());
            }
            Ok((file_name(&url, disposition.as_deref()), got.into_bytes()))
        }
        .await;
        match got {
            Ok(file) => inbox.lock().unwrap_or_else(|e| e.into_inner()).push(file),
            Err(Failure::Refused(sentence)) => {
                log::error!("vectorcraft-web: open {url}: {sentence}");
                *note.borrow_mut() = Some(sentence);
            }
            Err(Failure::Other(e)) => {
                log::error!("vectorcraft-web: open {url}: {e}");
                *note.borrow_mut() = Some(format!("Couldn't open the design from MakerFoundry: {e}"));
            }
        }
        ctx.request_repaint();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_makerfoundry_over_https() {
        let ok = "https%3A%2F%2Fmakerfoundry.app%2Fapi%2Fv1%2Fdesigns%2F42%2Ffile%2Frose.pes";
        assert_eq!(allowed(ok).unwrap(), "https://makerfoundry.app/api/v1/designs/42/file/rose.pes");
        assert!(allowed("https://www.makerfoundry.app/x.dst").is_ok());
        assert!(allowed("https://MakerFoundry.app/x.dst").is_ok());
        for bad in [
            "http://makerfoundry.app/x.pes",
            "https://makerfoundry.app.evil.example/x.pes",
            "https://evil.example/makerfoundry.app/x.pes",
            "https://user@makerfoundry.app/x.pes",
            "https://makerfoundry.app:8443/x.pes",
            "https://makerfoundry.app/a b.pes",
            "javascript:alert(1)",
            "https%3A%2F%2Fmakerfoundry.app%2F%ZZ",
            "",
        ] {
            assert!(allowed(bad).is_err(), "{bad}");
        }
        assert!(allowed(&format!("https://makerfoundry.app/{}", "a".repeat(3000))).is_err());
        assert!(fits(MAX_BYTES) && !fits(MAX_BYTES + 1));
    }

    /// The download stops as soon as it would pass the cap, whatever length the site gave (or
    /// didn't): nothing past [`MAX_BYTES`] is kept.
    #[test]
    fn a_download_stops_at_the_cap() {
        let mut got = Capped::default();
        let chunk = vec![0u8; 1024 * 1024];
        let mut taken = 0;
        while got.push(&chunk) {
            taken += 1;
            assert!(taken <= MAX_BYTES / chunk.len(), "kept past the cap");
        }
        assert_eq!(taken, MAX_BYTES / chunk.len());
        assert!(!got.room_for(1) && got.room_for(0));
        assert!(!got.push(&[1]), "not one byte more");
        assert_eq!(got.into_bytes().len(), MAX_BYTES);
        let mut small = Capped::default();
        assert!(small.push(b"PES") && !small.room_for(usize::MAX - 2));
        assert_eq!(small.into_bytes(), b"PES");
    }

    #[test]
    fn the_file_is_named_safely() {
        assert_eq!(file_name("https://makerfoundry.app/api/v1/designs/42/file/rose.pes", None), "rose.pes");
        assert_eq!(file_name("https://makerfoundry.app/d/42/file/Big%20Rose.dst?x=1", None), "Big Rose.dst");
        assert_eq!(file_name("https://makerfoundry.app/f", Some(r#"attachment; filename="tulip.jef""#)), "tulip.jef");
        assert_eq!(file_name("https://makerfoundry.app/f", Some("attachment; filename=../../etc/passwd")), "passwd");
        assert_eq!(file_name("https://makerfoundry.app/", None), "design");
        assert_eq!(file_name("https://makerfoundry.app/.hidden", None), "hidden");
    }

    #[test]
    fn makerfoundrys_own_sentence_is_shown_on_a_refusal() {
        let body = |m: &str| serde_json::json!({ "error": { "message": m } }).to_string();
        assert_eq!(refusal(404, &body("That design isn't on MakerFoundry.")), "That design isn't on MakerFoundry.");
        assert_eq!(
            refusal(403, &body("This design is for sale: buy it on MakerFoundry, then download it there.")),
            "This design is for sale: buy it on MakerFoundry, then download it there."
        );
        assert_eq!(
            refusal(429, &body("Too many designs opened lately. Please wait a while.")),
            "Too many designs opened lately. Please wait a while."
        );
        // Not that JSON: the status code.
        let fallback = "Couldn't open the design from MakerFoundry: it answered 502.";
        for other in ["", "<html>Bad gateway</html>", "{}", r#"{"error":"x"}"#, r#"{"error":{"message":""}}"#, r#"{"error":{"message":7}}"#] {
            assert_eq!(refusal(502, other), fallback, "{other}");
        }
        // Untrusted text: no control characters, and capped.
        assert_eq!(refusal(404, &body("Gone.\u{1b}[31m\nreally")), "Gone.[31mreally");
        assert_eq!(refusal(404, &body(&"x".repeat(5000))).chars().count(), MAX_MESSAGE);
    }

    #[test]
    fn the_open_parameter_leaves_the_address() {
        assert_eq!(without_param("?open=https%3A%2F%2Fmakerfoundry.app%2Fx.pes", "open"), "");
        assert_eq!(without_param("?lang=es&open=a&view", "open"), "?lang=es&view");
        assert_eq!(without_param("open=a&webgl", "open"), "?webgl");
        assert_eq!(without_param("?opener=1&open", "open"), "?opener=1");
        assert_eq!(without_param("", "open"), "");
        assert_eq!(without_param("?lang=es", "open"), "?lang=es");
    }
}
