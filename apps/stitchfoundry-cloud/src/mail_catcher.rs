//! A staging service's test mailbox (CLOUD-11). With `SF_CLOUD_MAIL_CATCHER_URL` set, every email
//! the service would send through Resend (password resets, sew-out reports, and any later account
//! email) goes instead to a Mailpit test mailbox over its send API
//! (<https://mailpit.axllent.org/docs/api-v1/view.html#post-/api/v1/send>), so no real person gets
//! mail from a staging service and the end-to-end tests can read it. Resend is never called while
//! it is set, and its key is never sent anywhere.
//!
//! It sits between the service and the network ([`Upstream`]): only posts to [`RESEND_URL`] are
//! turned into Mailpit's shape; Stripe and Anthropic requests pass as they are. So the senders in
//! `api.rs` need no change, and a new one (email confirmation, say) is caught the same way.

use serde_json::{Value, json};

use crate::api::{RESEND_URL, Upstream};

/// Mailpit's send API, under the catcher's address.
const SEND_PATH: &str = "/api/v1/send";

/// Sends Resend-shaped emails to a Mailpit test mailbox; everything else to `inner`.
pub struct MailCatcher {
    inner: Box<dyn Upstream>,
    send_url: String,
    /// `Basic …`, when the catcher's address carries a user name and password (Mailpit's send API
    /// shares the web UI's password unless told otherwise).
    auth: Option<String>,
}

impl MailCatcher {
    /// A catcher at `address` (`http://mailpit.railway.internal:8025`, optionally
    /// `http://user:password@host:port`), passing everything but email on to `inner`.
    pub fn new(address: &str, inner: Box<dyn Upstream>) -> Result<MailCatcher, String> {
        let (send_url, auth) = parse(address)?;
        Ok(MailCatcher { inner, send_url, auth })
    }

    /// Where the emails go (without any password), for the log.
    pub fn send_url(&self) -> &str {
        &self.send_url
    }
}

impl Upstream for MailCatcher {
    fn post(&self, url: &str, headers: &[(&str, String)], body: &str) -> Result<(u16, String), String> {
        if url != RESEND_URL {
            return self.inner.post(url, headers, body);
        }
        // (Resend's key, in `headers`, is left behind)
        let mail: Value = serde_json::from_str(body).map_err(|e| format!("mail catcher: the email isn't JSON: {e}"))?;
        let mut out = vec![("content-type", "application/json".to_string())];
        if let Some(a) = &self.auth {
            out.push(("authorization", a.clone()));
        }
        self.inner.post(&self.send_url, &out, &to_mailpit(&mail).to_string())
    }
}

/// The send address and the `Authorization` value from the catcher's address.
fn parse(address: &str) -> Result<(String, Option<String>), String> {
    const WHAT: &str = "SF_CLOUD_MAIL_CATCHER_URL is the test mailbox's address, like http://mailpit.railway.internal:8025";
    let a = address.trim().trim_end_matches('/');
    let (scheme, rest) = a.split_once("://").ok_or(WHAT)?;
    if !matches!(scheme, "http" | "https")
        || rest.is_empty()
        || rest.contains(['?', '#'])
        || rest.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(WHAT.into());
    }
    let (authority, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let (userinfo, host) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    if host.is_empty() {
        return Err(WHAT.into());
    }
    let auth = userinfo.map(|u| format!("Basic {}", crate::sewout::base64_encode(percent_decode(u).as_bytes())));
    Ok((format!("{scheme}://{host}{path}{SEND_PATH}"), auth))
}

/// `%XX` escapes decoded (a password may hold `@` or `:` written so); anything else as it is.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (c, hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `Name <user@host>` (or a bare address) as Mailpit's `{Email, Name}`.
fn address(text: &str) -> Value {
    let t = text.trim();
    match (t.rfind('<'), t.ends_with('>')) {
        (Some(i), true) => {
            let name = t.get(..i).unwrap_or_default().trim().trim_matches('"');
            let email = t.get(i + 1..t.len() - 1).unwrap_or_default().trim();
            json!({ "Email": email, "Name": name })
        }
        _ => json!({ "Email": t, "Name": "" }),
    }
}

/// A list of addresses: Resend takes a string or an array of them.
fn addresses(v: Option<&Value>) -> Vec<Value> {
    match v {
        Some(Value::String(s)) => vec![address(s)],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(address).collect(),
        _ => Vec::new(),
    }
}

/// A Resend email (<https://resend.com/docs/api-reference/emails/send-email>) in Mailpit's shape.
pub fn to_mailpit(mail: &Value) -> Value {
    let text = |k: &str| mail.get(k).and_then(Value::as_str).unwrap_or_default();
    let attachments: Vec<Value> = mail
        .get("attachments")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|f| json!({ "Filename": f.get("filename").and_then(Value::as_str).unwrap_or("file"), "Content": f.get("content").and_then(Value::as_str).unwrap_or_default() }))
                .collect()
        })
        .unwrap_or_default();
    let mut out = json!({
        "From": address(text("from")),
        "To": addresses(mail.get("to")),
        "Subject": text("subject"),
        "Text": text("text"),
    });
    let reply_to = addresses(mail.get("reply_to"));
    if !reply_to.is_empty() {
        out["ReplyTo"] = Value::from(reply_to);
    }
    if !text("html").is_empty() {
        out["HTML"] = Value::from(text("html"));
    }
    if !attachments.is_empty() {
        out["Attachments"] = Value::from(attachments);
    }
    if let Some(h) = mail.get("headers").and_then(Value::as_object) {
        out["Headers"] = Value::Object(h.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    type Calls = Arc<Mutex<Vec<(String, Vec<(String, String)>, String)>>>;

    struct Fake(Calls);

    impl Upstream for Fake {
        fn post(&self, url: &str, headers: &[(&str, String)], body: &str) -> Result<(u16, String), String> {
            self.0.lock().unwrap().push((url.to_string(), headers.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(), body.to_string()));
            Ok((200, r#"{"ID":"abc"}"#.into()))
        }
    }

    fn catcher(address: &str) -> (MailCatcher, Calls) {
        let calls = Calls::default();
        (MailCatcher::new(address, Box::new(Fake(calls.clone()))).unwrap(), calls)
    }

    #[test]
    fn emails_go_to_mailpit_without_resends_key() {
        let (c, calls) = catcher("http://mailpit.railway.internal:8025/");
        assert_eq!(c.send_url(), "http://mailpit.railway.internal:8025/api/v1/send");
        let mail = json!({ "from": "StitchFoundry <accounts@stitchfoundry.app>", "to": ["a@test.example"], "reply_to": "support@stitchfoundry.app", "subject": "Reset", "text": "open https://x/reset?token=t" });
        let (status, _) = c.post(RESEND_URL, &[("authorization", "Bearer re_live_secret".into())], &mail.to_string()).unwrap();
        assert_eq!(status, 200);
        let calls = calls.lock().unwrap();
        let (url, headers, body) = &calls[0];
        assert_eq!(url, "http://mailpit.railway.internal:8025/api/v1/send");
        assert!(headers.iter().all(|(_, v)| !v.contains("re_live_secret")), "Resend's key never leaves");
        assert!(!headers.iter().any(|(k, _)| k == "authorization"));
        let sent: Value = serde_json::from_str(body).unwrap();
        assert_eq!(sent["From"], json!({ "Email": "accounts@stitchfoundry.app", "Name": "StitchFoundry" }));
        assert_eq!(sent["To"], json!([{ "Email": "a@test.example", "Name": "" }]));
        assert_eq!(sent["ReplyTo"], json!([{ "Email": "support@stitchfoundry.app", "Name": "" }]));
        assert_eq!(sent["Subject"], "Reset");
        assert_eq!(sent["Text"], "open https://x/reset?token=t");
    }

    #[test]
    fn other_requests_pass_as_they_are() {
        let (c, calls) = catcher("http://mailpit:8025");
        c.post("https://api.stripe.com/v1/checkout/sessions", &[("authorization", "Bearer sk_test".into())], "a=1").unwrap();
        let calls = calls.lock().unwrap();
        assert_eq!(calls[0].0, "https://api.stripe.com/v1/checkout/sessions");
        assert_eq!(calls[0].1, [("authorization".to_string(), "Bearer sk_test".to_string())]);
        assert_eq!(calls[0].2, "a=1");
    }

    #[test]
    fn a_password_in_the_address_signs_in_and_attachments_travel() {
        let (c, calls) = catcher("https://qa:p%40ss@mail.example.com");
        assert_eq!(c.send_url(), "https://mail.example.com/api/v1/send");
        let mail =
            json!({ "from": "r@x.app", "to": "o@x.app", "subject": "S", "text": "T", "attachments": [{ "filename": "a.jpg", "content": "AAAA" }] });
        c.post(RESEND_URL, &[], &mail.to_string()).unwrap();
        let calls = calls.lock().unwrap();
        // base64("qa:p@ss")
        assert!(calls[0].1.contains(&("authorization".to_string(), "Basic cWE6cEBzcw==".to_string())));
        let sent: Value = serde_json::from_str(&calls[0].2).unwrap();
        assert_eq!(sent["To"], json!([{ "Email": "o@x.app", "Name": "" }]));
        assert_eq!(sent["Attachments"], json!([{ "Filename": "a.jpg", "Content": "AAAA" }]));
        assert!(sent.get("ReplyTo").is_none());
    }

    #[test]
    fn bad_addresses_are_refused_and_bad_mail_is_an_error() {
        for bad in ["", "mailpit:8025", "ftp://mailpit", "http://", "http://user@", "http://mailpit?x=1", "http://mail pit"] {
            assert!(MailCatcher::new(bad, Box::new(Fake(Calls::default()))).is_err(), "{bad}");
        }
        let (c, _) = catcher("http://mailpit:8025");
        assert!(c.post(RESEND_URL, &[], "not json").is_err());
        assert_eq!(percent_decode("a%zz%4"), "a%zz%4");
    }
}
