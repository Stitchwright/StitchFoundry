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

/// Fetch the design at `url` and put it in `inbox`, which opens it; `note` gets the message the
/// status bar shows when it can't be opened.
#[cfg(target_arch = "wasm32")]
pub fn start(url: String, inbox: vectorcraft_ui_egui::Inbox, note: std::rc::Rc<std::cell::RefCell<Option<String>>>, ctx: egui::Context) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    let js = |e: wasm_bindgen::JsValue| e.as_string().unwrap_or_else(|| format!("{e:?}"));
    wasm_bindgen_futures::spawn_local(async move {
        let got: Result<(String, Vec<u8>), String> = async {
            let window = web_sys::window().ok_or("no window")?;
            // a cross-site request sends no cookies (credentials: same-origin, the default)
            let init = web_sys::RequestInit::new();
            init.set_method("GET");
            let response: web_sys::Response =
                JsFuture::from(window.fetch_with_str_and_init(&url, &init)).await.map_err(js)?.dyn_into().map_err(js)?;
            if !response.ok() {
                return Err(format!("MakerFoundry answered {}", response.status()));
            }
            let long = response.headers().get("content-length").ok().flatten().and_then(|n| n.parse::<usize>().ok()).is_some_and(|n| !fits(n));
            if long {
                return Err("the design is too big to open".into());
            }
            let disposition = response.headers().get("content-disposition").ok().flatten();
            let buffer = JsFuture::from(response.array_buffer().map_err(js)?).await.map_err(js)?;
            let bytes = js_sys::Uint8Array::new(&buffer);
            if !fits(bytes.length() as usize) {
                return Err("the design is too big to open".into());
            }
            Ok((file_name(&url, disposition.as_deref()), bytes.to_vec()))
        }
        .await;
        match got {
            Ok(file) => inbox.lock().unwrap_or_else(|e| e.into_inner()).push(file),
            Err(e) => {
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

    #[test]
    fn the_file_is_named_safely() {
        assert_eq!(file_name("https://makerfoundry.app/api/v1/designs/42/file/rose.pes", None), "rose.pes");
        assert_eq!(file_name("https://makerfoundry.app/d/42/file/Big%20Rose.dst?x=1", None), "Big Rose.dst");
        assert_eq!(file_name("https://makerfoundry.app/f", Some(r#"attachment; filename="tulip.jef""#)), "tulip.jef");
        assert_eq!(file_name("https://makerfoundry.app/f", Some("attachment; filename=../../etc/passwd")), "passwd");
        assert_eq!(file_name("https://makerfoundry.app/", None), "design");
        assert_eq!(file_name("https://makerfoundry.app/.hidden", None), "hidden");
    }
}
