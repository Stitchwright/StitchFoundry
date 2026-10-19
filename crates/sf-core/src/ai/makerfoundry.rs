//! Publish to MakerFoundry (PUB-01): the design sent to MakerFoundry (makerfoundry.app, the
//! makers' marketplace that shares Foundry Accounts) as a draft, through its API for the apps
//! (`docs/api.md` in SiteSentrix/makerfoundry). The request carries the Foundry Account's session
//! token, which MakerFoundry checks with the account service for this one request and never
//! keeps. The maker finishes and publishes the draft on the site, where they agree to its Seller
//! Terms. As elsewhere in [`super`], no network code lives here.

use serde_json::{Value, json};

use super::{ArtError, HttpRequest, cloud};

/// MakerFoundry's address unless another is set.
pub const DEFAULT_SERVER: &str = "https://makerfoundry.app";
/// Where designs are sent.
pub const DESIGNS: &str = "/api/v1/designs";
/// MakerFoundry's limits: photos a design may carry, an embroidery file's size and everything
/// sent at once (bytes, before base64).
pub const MAX_PHOTOS: usize = 6;
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;
/// The longest message from MakerFoundry shown (characters).
const MAX_MESSAGE: usize = 500;

/// A design to publish: its machine file and photos of the sew-out, as base64.
#[derive(Clone, Debug, Default)]
pub struct Upload {
    /// The file's name; its extension tells MakerFoundry how to read it.
    pub file_name: String,
    pub file_base64: String,
    /// JPEG, PNG or WebP photos of the stitched design.
    pub photos: Vec<String>,
    /// The app and its version, for MakerFoundry's log.
    pub app: String,
}

/// The draft MakerFoundry kept: its page, to finish in the browser.
#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    pub id: u64,
    pub url: String,
    /// The format MakerFoundry read the file as (`PES`).
    pub format: String,
}

impl Upload {
    /// The body of `POST /api/v1/designs`; refused here, without sending, when it is over
    /// MakerFoundry's limits.
    pub fn body(&self) -> Result<Value, ArtError> {
        let name = self.file_name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
        let photos: Vec<&str> = self.photos.iter().take(MAX_PHOTOS).map(String::as_str).collect();
        let file = decoded_len(&self.file_base64);
        let total = photos.iter().fold(file, |n, p| n.saturating_add(decoded_len(p)));
        if file > MAX_FILE_BYTES || total > MAX_UPLOAD_BYTES {
            return Err(ArtError::TooBigForMakerFoundry);
        }
        Ok(json!({
            "file_name": name,
            "file": self.file_base64,
            "photos": photos,
            "app": self.app.chars().take(64).collect::<String>(),
        }))
    }
}

/// The address to send to: `typed` (the setting) when given, else MakerFoundry's own; HTTPS, or
/// plain HTTP on this computer for testing ([`cloud::clean_server`]).
pub fn server(typed: &str) -> Result<String, ArtError> {
    let typed = typed.trim();
    cloud::clean_server(if typed.is_empty() { DEFAULT_SERVER } else { typed }).ok_or(ArtError::NoMakerFoundry)
}

/// The request publishing `upload` as a draft, signed in with the Foundry Account session `token`.
pub fn publish(server: &str, token: &str, upload: &Upload) -> Result<HttpRequest, ArtError> {
    let server = cloud::clean_server(server).ok_or(ArtError::NoMakerFoundry)?;
    let token = token.trim();
    if !token.starts_with(cloud::TOKEN_PREFIX) {
        return Err(ArtError::SignIn);
    }
    cloud::request(&server, DESIGNS, Some(token), &upload.body()?)
}

/// MakerFoundry's answer with HTTP status `status` and body `body`, sent by `server`: the draft,
/// whose page is on that same site, or what to tell the person (MakerFoundry's own sentence when
/// it gives one).
pub fn answer(server: &str, status: u16, body: &str) -> Result<Draft, ArtError> {
    let v: Option<Value> = serde_json::from_str(body).ok();
    if !(200..300).contains(&status) {
        let message = v.as_ref().and_then(|v| v.pointer("/error/message")).and_then(Value::as_str).map(shown).filter(|m| !m.is_empty());
        return Err(match (status, message) {
            (413, _) => ArtError::TooBigForMakerFoundry,
            (401, m) => ArtError::MakerFoundrySignIn(m.unwrap_or_default()),
            (_, Some(m)) => ArtError::MakerFoundry(m),
            (s, None) => ArtError::MakerFoundryFailed(s),
        });
    }
    let v = v.ok_or(ArtError::NoAnswer)?;
    let server = cloud::clean_server(server).ok_or(ArtError::NoMakerFoundry)?;
    // only a page of the site the design went to is opened
    let url = v["url"]
        .as_str()
        .filter(|u| u.strip_prefix(server.as_str()).is_some_and(|rest| rest.starts_with('/')))
        .filter(|u| u.len() <= 2000 && !u.chars().any(|c| c.is_whitespace() || c.is_control()));
    let (Some(id), Some(url)) = (v["id"].as_u64(), url) else { return Err(ArtError::NoAnswer) };
    Ok(Draft { id, url: url.to_string(), format: v["format"].as_str().map(shown).unwrap_or_default() })
}

/// A message from MakerFoundry as it is shown: one line, not too long.
fn shown(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_MESSAGE).collect::<String>().trim().to_string()
}

/// The bytes standard base64 `b64` (with padding) holds.
fn decoded_len(b64: &str) -> usize {
    let pad = b64.bytes().rev().take_while(|&b| b == b'=').count().min(2);
    (b64.len() / 4 * 3).saturating_sub(pad)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rose() -> Upload {
        Upload {
            file_name: "C:\\designs\\rose.pes".into(),
            file_base64: "I1BFUw==".into(),
            photos: vec!["aGk=".into()],
            app: "StitchFoundry 0.9.0".into(),
        }
    }

    #[test]
    fn the_request_carries_the_file_the_photos_and_the_session() {
        let r = publish("https://makerfoundry.app/", " sfs_abc ", &rose()).unwrap();
        assert_eq!(r.url, "https://makerfoundry.app/api/v1/designs");
        assert!(r.headers.contains(&("authorization", "Bearer sfs_abc".to_string())));
        assert!(r.headers.contains(&("content-type", "application/json".to_string())));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b, json!({"file_name": "rose.pes", "file": "I1BFUw==", "photos": ["aGk="], "app": "StitchFoundry 0.9.0"}));
        // a local MakerFoundry for testing; nothing else but HTTPS
        assert_eq!(publish("http://127.0.0.1:8080", "sfs_abc", &rose()).unwrap().url, "http://127.0.0.1:8080/api/v1/designs");
        assert_eq!(publish("http://makerfoundry.app", "sfs_abc", &rose()), Err(ArtError::NoMakerFoundry));
        // signed out, or an Anthropic key instead of a session: nothing is sent
        assert_eq!(publish("https://makerfoundry.app", "", &rose()), Err(ArtError::SignIn));
        assert_eq!(publish("https://makerfoundry.app", "sk-ant-mine", &rose()), Err(ArtError::SignIn));
        assert_eq!(server("").unwrap(), DEFAULT_SERVER);
        assert_eq!(server(" http://localhost:8080/ ").unwrap(), "http://localhost:8080");
        assert_eq!(server("makerfoundry.app"), Err(ArtError::NoMakerFoundry));
    }

    #[test]
    fn limits_are_checked_before_sending() {
        let mut u = rose();
        u.photos = vec!["aGk=".into(); 9];
        assert_eq!(u.body().unwrap()["photos"].as_array().unwrap().len(), MAX_PHOTOS);
        // 4 MB of file is fine, a byte more isn't
        u.file_base64 = "A".repeat(MAX_FILE_BYTES / 3 * 4) + "AA==";
        assert_eq!(decoded_len(&u.file_base64), MAX_FILE_BYTES);
        assert!(u.body().is_ok());
        u.file_base64 = "A".repeat(MAX_FILE_BYTES / 3 * 4) + "AAA=";
        assert_eq!(u.body(), Err(ArtError::TooBigForMakerFoundry));
        // the photos count toward the 64 MB in all
        let mut u = rose();
        u.photos = vec!["A".repeat(12 * 1024 * 1024 / 3 * 4); 6];
        assert_eq!(u.body(), Err(ArtError::TooBigForMakerFoundry));
        assert_eq!(decoded_len("aGk="), 2);
        assert_eq!(decoded_len("I1BFUw=="), 4);
        assert_eq!(decoded_len(""), 0);
    }

    #[test]
    fn answers_and_their_messages() {
        let ok = r#"{"id": 42, "url": "https://makerfoundry.app/designs/42", "kind": "embroidery", "format": "PES"}"#;
        assert_eq!(
            answer("https://makerfoundry.app", 201, ok),
            Ok(Draft { id: 42, url: "https://makerfoundry.app/designs/42".into(), format: "PES".into() })
        );
        // a page anywhere else isn't opened
        for url in
            ["https://evil.example/designs/42", "https://makerfoundry.app.evil.example/x", "javascript:alert(1)", "https://makerfoundry.app/a b"]
        {
            let body = json!({"id": 1, "url": url}).to_string();
            assert_eq!(answer("https://makerfoundry.app", 201, &body), Err(ArtError::NoAnswer), "{url}");
        }
        assert_eq!(answer("https://makerfoundry.app", 201, "not json"), Err(ArtError::NoAnswer));
        let said = |m: &str| json!({"error": {"message": m}}).to_string();
        // every message is shown as MakerFoundry wrote it
        assert_eq!(
            answer("https://makerfoundry.app", 422, &said("This PES file couldn't be read: it ends too soon.")).map_err(|e| e.to_string()),
            Err("This PES file couldn't be read: it ends too soon.".into())
        );
        assert_eq!(
            answer("https://makerfoundry.app", 429, &said("You've sent 50 designs today: try again tomorrow.")),
            Err(ArtError::MakerFoundry("You've sent 50 designs today: try again tomorrow.".into()))
        );
        assert_eq!(answer("https://makerfoundry.app", 400, &said("Bad\nrequest.")), Err(ArtError::MakerFoundry("Bad request.".into())));
        assert_eq!(answer("https://makerfoundry.app", 401, &said("Sign in again.")), Err(ArtError::MakerFoundrySignIn("Sign in again.".into())));
        assert_eq!(answer("https://makerfoundry.app", 401, ""), Err(ArtError::MakerFoundrySignIn(String::new())));
        assert_eq!(answer("https://makerfoundry.app", 413, ""), Err(ArtError::TooBigForMakerFoundry));
        assert_eq!(ArtError::TooBigForMakerFoundry.to_string(), "The design is too big for MakerFoundry.");
        assert_eq!(answer("https://makerfoundry.app", 503, "<html>"), Err(ArtError::MakerFoundryFailed(503)));
        let long = said(&"x".repeat(5000));
        let Err(ArtError::MakerFoundry(m)) = answer("https://makerfoundry.app", 422, &long) else { panic!() };
        assert_eq!(m.chars().count(), MAX_MESSAGE);
    }
}
