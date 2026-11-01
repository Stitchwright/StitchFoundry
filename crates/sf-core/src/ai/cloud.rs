//! Foundry accounts: the requests the app sends the StitchFoundry cloud service
//! (`apps/stitchfoundry-cloud`, `docs/cloud.md`) to sign in, read the credits and buy more, and
//! how an AI request is sent through it, paid in credits, instead of to Anthropic with the user's
//! own key ([`through_cloud`]). As elsewhere in [`super`], no network code lives here.

use serde_json::{Value, json};

use super::{ArtError, HttpRequest};

/// The secret the session token is kept as.
pub const TOKEN_NAME: &str = "stitchfoundry-session";
/// Session tokens start with this.
pub const TOKEN_PREFIX: &str = "sfs_";

/// `server` tidied (no trailing slash) when it is an address the app may send to: HTTPS, or
/// plain HTTP on this computer (for running the service locally). The host is compared whole,
/// after the address is split into its parts, and an address with a user name or password in it
/// (`http://127.0.0.1:1@evil.example`, whose host is `evil.example`) is refused, so the session
/// token and password never go in clear text to another computer.
pub fn clean_server(server: &str) -> Option<String> {
    let s = server.trim().trim_end_matches('/');
    // (a backslash too: URL readers differ on it, some reading it as a slash)
    if s.chars().any(|c| c.is_whitespace() || c.is_control() || c == '\\') {
        return None;
    }
    let (https, rest) = match (s.strip_prefix("https://"), s.strip_prefix("http://")) {
        (Some(rest), _) => (true, rest),
        (None, Some(rest)) => (false, rest),
        (None, None) => return None,
    };
    // the authority ends where the path, query or fragment starts
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    // host and port: `[::1]:8080`, `localhost:8080`, `example.com`
    let (host, port) = match (authority.starts_with('['), authority.find(']'), authority.find(':')) {
        (true, Some(end), _) => (authority.get(..=end)?, authority.get(end + 1..)?),
        (true, None, _) => return None,
        (false, _, Some(i)) => (authority.get(..i)?, authority.get(i..)?),
        (false, _, None) => (authority, ""),
    };
    let port_ok = port.is_empty() || port.strip_prefix(':').is_some_and(|p| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()));
    let local = ["127.0.0.1", "localhost", "[::1]"].contains(&host.to_ascii_lowercase().as_str());
    (!host.is_empty() && port_ok && (https || local)).then(|| s.to_string())
}

/// A POST of `body` to `path` on `server`, signed in with `token` when given.
pub fn request(server: &str, path: &str, token: Option<&str>, body: &Value) -> Result<HttpRequest, ArtError> {
    let server = clean_server(server).ok_or(ArtError::NoServer)?;
    let mut headers = vec![("content-type", "application/json".to_string())];
    if let Some(t) = token {
        headers.push(("authorization", format!("Bearer {}", t.trim())));
    }
    Ok(HttpRequest { url: format!("{server}{path}"), headers, body: body.to_string(), ..Default::default() })
}

pub fn sign_in(server: &str, email: &str, password: &str) -> Result<HttpRequest, ArtError> {
    request(server, "/v1/signin", None, &json!({ "email": email.trim(), "password": password }))
}

pub fn sign_up(server: &str, email: &str, password: &str) -> Result<HttpRequest, ArtError> {
    request(server, "/v1/signup", None, &json!({ "email": email.trim(), "password": password }))
}

pub fn sign_out(server: &str, token: &str) -> Result<HttpRequest, ArtError> {
    request(server, "/v1/signout", Some(token), &json!({}))
}

/// The account: `{email, credits, packs: [{id, credits, cents}], payments, history}`.
pub fn account(server: &str, token: &str) -> Result<HttpRequest, ArtError> {
    request(server, "/v1/account", Some(token), &json!({}))
}

/// A checkout page for pack `pack`: `{url}` to open in the browser.
pub fn checkout(server: &str, token: &str, pack: &str) -> Result<HttpRequest, ArtError> {
    request(server, "/v1/checkout", Some(token), &json!({ "pack": pack }))
}

/// Delete the account's shared sew-out checks (their photos and rounds): `{deleted}`.
pub fn delete_shared(server: &str, token: &str) -> Result<HttpRequest, ArtError> {
    request(server, "/v1/sewout/delete", Some(token), &json!({}))
}

/// Report to StitchFoundry: a sew-out and what StitchFoundry could do better, sent with the
/// user's leave (`body`: [`super::sewout::Report::body`]) → `{report, emailed}`.
pub fn report(server: &str, token: &str, body: &Value) -> Result<HttpRequest, ArtError> {
    request(server, REPORT, Some(token), body)
}

/// The error an answer from the service with HTTP status `status` and body `body` means.
pub fn status_error(status: u16, body: &str) -> ArtError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.pointer("/error/message").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    match status {
        401 | 403 => ArtError::SignIn,
        402 => ArtError::NoCredits,
        409 => ArtError::Taken,
        429 | 529 => ArtError::Busy,
        503 => ArtError::CloudClosed,
        s if s >= 500 => ArtError::Service(s),
        s => ArtError::Request(s, message),
    }
}

/// An answer from the service: its JSON, or what went wrong.
pub fn parse(status: u16, body: &str) -> Result<Value, ArtError> {
    if !(200..300).contains(&status) {
        return Err(status_error(status, body));
    }
    serde_json::from_str(body).map_err(|_| ArtError::NoAnswer)
}

/// The service's endpoint for AI requests, and for sew-out checks.
pub const MESSAGES: &str = "/v1/messages";
pub const SEWOUT: &str = "/v1/sewout";
pub const REPORT: &str = "/v1/report";

/// `http`, an AI request built for Anthropic with session token `token` in place of an API key
/// (`x-api-key`), sent instead to `path` ([`MESSAGES`], [`SEWOUT`]) on the service at `server`,
/// which pays for it in credits, with `extra` headers.
pub fn through_cloud(mut http: HttpRequest, server: &str, path: &str, extra: Vec<(&'static str, String)>) -> Result<HttpRequest, ArtError> {
    let server = clean_server(server).ok_or(ArtError::NoServer)?;
    let token =
        http.headers.iter().find(|h| h.0 == "x-api-key").map(|h| h.1.clone()).filter(|t| t.starts_with(TOKEN_PREFIX)).ok_or(ArtError::SignIn)?;
    http.headers.retain(|h| h.0 != "x-api-key");
    http.headers.push(("authorization", format!("Bearer {token}")));
    http.headers.extend(extra);
    http.url = format!("{server}{path}");
    Ok(http)
}

/// A new id for a sew-out check (16 hex digits), from what makes it different: its first message
/// and the time.
pub fn check_id(first: &serde_json::Value, now: f64) -> String {
    // FNV-1a: an id, not a secret
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in first.to_string().bytes().chain(now.to_bits().to_le_bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_computer() {
        assert_eq!(clean_server(" https://cloud.example.com/ ").as_deref(), Some("https://cloud.example.com"));
        assert_eq!(clean_server("http://127.0.0.1:8080").as_deref(), Some("http://127.0.0.1:8080"));
        assert_eq!(clean_server("http://localhost").as_deref(), Some("http://localhost"));
        assert_eq!(clean_server("http://[::1]:7000/").as_deref(), Some("http://[::1]:7000"));
        assert_eq!(clean_server("http://localhost/@x").as_deref(), Some("http://localhost/@x"), "an @ in the path is the path's");
        for bad in ["", "https://", "http://cloud.example.com", "ftp://x", "http://localhost.evil.com", "https://a b.com"] {
            assert_eq!(clean_server(bad), None, "{bad}");
        }
    }

    /// The host is compared whole: an address that starts like this computer's but names another
    /// (through a user name and password, or a longer host) is refused.
    #[test]
    fn a_local_looking_address_with_another_host_is_refused() {
        for bad in [
            "http://127.0.0.1:1@evil.example",
            "http://localhost:80@evil.example",
            "http://localhost@evil.example",
            "http://[::1]@evil.example",
            "http://127.0.0.1.evil.example",
            "http://127.0.0.1:80\\@evil.example",
            "http://localhost:x",
            "http://[::1",
            "https://user:pass@cloud.example.com",
        ] {
            assert_eq!(clean_server(bad), None, "{bad}");
        }
    }

    #[test]
    fn requests_and_answers() {
        let r = sign_in("https://c.example", " denise@example.com ", "fire truck 1").unwrap();
        assert_eq!(r.url, "https://c.example/v1/signin");
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b, json!({"email": "denise@example.com", "password": "fire truck 1"}));
        assert!(r.headers.iter().all(|h| h.0 != "authorization"));
        let r = checkout("https://c.example", "sfs_abc", "small").unwrap();
        assert!(r.headers.contains(&("authorization", "Bearer sfs_abc".to_string())));
        assert_eq!(account("", "sfs_abc"), Err(ArtError::NoServer));
        assert_eq!(parse(200, r#"{"credits": 5}"#).unwrap()["credits"], 5);
        assert_eq!(parse(402, r#"{"type":"error","error":{"type":"x","message":"not enough credits"}}"#), Err(ArtError::NoCredits));
        assert_eq!(parse(401, "{}"), Err(ArtError::SignIn));
        assert_eq!(parse(409, "{}"), Err(ArtError::Taken));
        assert_eq!(parse(503, "{}"), Err(ArtError::CloudClosed));
        assert_eq!(
            parse(400, r#"{"error":{"message":"a password needs at least 8 characters"}}"#),
            Err(ArtError::Request(400, "a password needs at least 8 characters".into()))
        );
    }

    #[test]
    fn an_ai_request_goes_through_the_service_with_the_session() {
        let q = super::super::ask::question("Which needle?", None).unwrap();
        let http = super::super::ask::request("sfs_token", &[q]).unwrap();
        let body = http.body.clone();
        let c = through_cloud(http, "https://c.example/", MESSAGES, vec![("x-stitchfoundry-share", "1".into())]).unwrap();
        assert_eq!(c.url, "https://c.example/v1/messages");
        assert!(c.headers.contains(&("x-stitchfoundry-share", "1".to_string())));
        assert!(c.headers.iter().all(|h| h.0 != "x-api-key"));
        assert!(c.headers.contains(&("authorization", "Bearer sfs_token".to_string())));
        assert!(c.headers.iter().any(|h| h.0 == "anthropic-version"));
        assert_eq!(c.body, body, "the same request");
        // an Anthropic key never goes to the service
        let own = super::super::ask::request("sk-ant-mine", &[super::super::ask::question("x", None).unwrap()]).unwrap();
        assert_eq!(through_cloud(own, "https://c.example", SEWOUT, vec![]), Err(ArtError::SignIn));
        let a = check_id(&json!({"x": 1}), 1.0);
        assert_eq!(a.len(), 16);
        assert_ne!(a, check_id(&json!({"x": 1}), 2.0));
    }
}
