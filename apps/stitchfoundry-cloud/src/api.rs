//! The service's endpoints, independent of the HTTP server that carries them ([`Service::handle`]
//! takes a request and gives a response), so they are tested without a network. Errors come back
//! in the Anthropic API's shape (`{"type": "error", "error": {"type", "message"}}`), so the app
//! reads them as it reads Anthropic's own.
//!
//! - `GET  /v1/health`: whether AI requests and payments are set up.
//! - `POST /v1/signup {email, password}`, `POST /v1/signin {email, password}` → `{token, account}`.
//! - `POST /v1/signout` (with the token).
//! - `POST /v1/password/forgot {email}`: a link to reset the password, emailed if the account
//!   exists (the answer is the same either way); `GET /reset?token=…` is the page it opens, and
//!   `POST /reset` (its form) or `POST /v1/password/reset {token, password}` sets the new one.
//! - `GET  /verify?token=…`: the page the email-confirmation link (sent at sign-up) opens, which
//!   confirms the account's email; `POST /v1/verify/resend` (signed in) sends the link again.
//!   Until the email is confirmed, the account can sign in and read itself but spends nothing:
//!   AI requests, sew-out checks (free ones too) and buying credits answer 403.
//! - `GET  /v1/account` → `{id, email, verified, credits, packs, history}`.
//! - `POST /v1/checkout {pack}` → `{url}`: Stripe's checkout page for the pack.
//! - `POST /v1/stripe/webhook`: Stripe's signed word that a checkout was paid.
//! - `POST /v1/messages`: an Anthropic Messages API request, sent with the service's key and paid
//!   for in credits (`x-stitchfoundry-credits` on the reply: what is left).
//!
//! The token comes as `Authorization: Bearer sfs_…`.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::{Value, json};

use crate::auth::{self, Throttle};
use crate::config::Config;
use crate::store::{Check, Store, StoreError};
use crate::{pricing, sewout, stripe};

/// The largest request body accepted (an image to redraw is the largest thing sent).
pub const MAX_BODY: usize = 8 * 1024 * 1024;
/// AI requests one account may have on their way at once.
pub const MAX_IN_FLIGHT: u32 = 3;
/// The fewest credits a paid-as-used request starts with. The most a request could cost (a long
/// reply allowed) can be more than a new account's credits; it holds what there is, and is
/// charged what it used, at most that.
pub const MIN_TO_START: u64 = 25;
/// StitchFoundry's site, linked from the pages Stripe returns to.
const SITE_URL: &str = "https://stitchfoundry.app";
/// The Anthropic Messages API.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
/// Where sew-out reports are emailed through (Resend's API).
pub const RESEND_URL: &str = "https://api.resend.com/emails";
/// The keys of a Messages request passed on; any others are left out.
const PASSED: &[&str] = &["model", "max_tokens", "system", "messages", "tools", "tool_choice", "output_config", "fallbacks", "thinking"];

/// The `anthropic-beta` features a request may turn on: the ones the apps send. Any other (a
/// longer context window, say) could be billed above what [`pricing`] charges, so it is dropped.
const BETAS: &[&str] = &[sf_core::ai::FALLBACK_BETA];

/// The allowed betas of an `anthropic-beta` header (a comma-separated list), the rest left out.
fn allowed_betas(header: &str) -> String {
    header.split(',').map(str::trim).filter(|b| BETAS.contains(b)).collect::<Vec<_>>().join(",")
}

/// Why `tools` can't be passed on, if it can't. The app's own tools (`type` absent: they come back
/// to the app to run) pass, and the basic web search, at most [`sf_core::ai::MAX_SEARCHES`]
/// searches, so that what is held before a request covers it ([`pricing::estimate`]). Every other
/// tool that runs on Anthropic's side (code, search that filters with code, fetching pages) can
/// run up to ten rounds in one request, each reading everything again, past any hold.
fn tools_refused(tools: &Value) -> Option<String> {
    let Some(list) = tools.as_array() else { return Some("tools is a list".into()) };
    for t in list {
        match t.get("type").and_then(Value::as_str) {
            None | Some("custom") => {}
            Some(k) if k == sf_core::ai::WEB_SEARCH => {
                let uses = t.get("max_uses").and_then(Value::as_u64).unwrap_or(0);
                if uses == 0 || uses > u64::from(sf_core::ai::MAX_SEARCHES) {
                    return Some(format!("web search needs max_uses of 1 to {}", sf_core::ai::MAX_SEARCHES));
                }
            }
            Some(k) => return Some(format!("the tool {k:?} isn't offered")),
        }
    }
    None
}

/// The headers that let the web app call from a browser.
const CORS: &[(&str, &str)] = &[
    ("Access-Control-Allow-Origin", "*"),
    ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
    ("Access-Control-Allow-Headers", "authorization, content-type, anthropic-version, anthropic-beta"),
    ("Access-Control-Expose-Headers", "x-stitchfoundry-credits, x-stitchfoundry-spent"),
    ("Access-Control-Max-Age", "86400"),
];

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug, PartialEq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl Response {
    fn json(status: u16, v: &Value) -> Response {
        Response { status, content_type: "application/json", body: v.to_string(), headers: Vec::new() }
    }

    fn error(status: u16, kind: &str, message: impl Into<String>) -> Response {
        Response::json(status, &json!({ "type": "error", "error": { "type": kind, "message": message.into() } }))
    }

    /// A page for Stripe's return addresses: `text`, what to do next, and a way to the site (the
    /// app has no address a page could go back to, so it says where to look instead).
    fn page(text: &str, next: &str) -> Response {
        let body = format!(
            "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>StitchFoundry</title><body style=\"font-family:sans-serif;max-width:32em;margin:4em auto;padding:0 1em;line-height:1.5\"><h1>StitchFoundry</h1><p>{text}</p><p>{next}</p><p><a href=\"{SITE_URL}\" style=\"display:inline-block;padding:.6em 1.2em;border-radius:.4em;background:#2a5bd7;color:#fff;text-decoration:none\">Back to stitchfoundry.app</a></p></body>"
        );
        Response { status: 200, content_type: "text/html; charset=utf-8", body, headers: Vec::new() }
    }
}

/// HTTPS POSTs to Anthropic and Stripe: (url, headers, body) → (status, body).
pub trait Upstream: Send + Sync {
    fn post(&self, url: &str, headers: &[(&str, String)], body: &str) -> Result<(u16, String), String>;
}

/// The real one, with ureq.
pub struct Https {
    agent: ureq::Agent,
}

impl Default for Https {
    fn default() -> Self {
        let agent =
            ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(std::time::Duration::from_secs(600))).build().new_agent();
        Https { agent }
    }
}

impl Upstream for Https {
    fn post(&self, url: &str, headers: &[(&str, String)], body: &str) -> Result<(u16, String), String> {
        let mut req = self.agent.post(url);
        for (k, v) in headers {
            req = req.header(*k, v.as_str());
        }
        let mut resp = req.send(body).map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let text = resp.body_mut().with_config().limit(32 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
        Ok((status, text))
    }
}

pub struct Service {
    cfg: Config,
    store: Mutex<Store>,
    throttle: Mutex<Throttle>,
    /// Reset emails lately, by address (so nobody's inbox can be flooded).
    resets: Mutex<Throttle>,
    /// Confirmation emails lately, by address (sign-up's and the ones sent again).
    verifies: Mutex<Throttle>,
    in_flight: Mutex<HashMap<u64, u32>>,
    upstream: Box<dyn Upstream>,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
}

/// Seconds since 1970.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn body_json(req: &Request) -> Result<Value, Response> {
    serde_json::from_slice(&req.body).map_err(|_| Response::error(400, "invalid_request_error", "the body isn't JSON"))
}

/// `n` with commas between thousands (1000 → "1,000").
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Why a reset link doesn't work.
const EXPIRED: &str = "This link has expired or was used already: ask for a new one where you sign in";
/// Why a confirmation link doesn't work.
const VERIFY_EXPIRED: &str = "This link has expired or was used already. If your Foundry Account still asks you to confirm your email, send a new link from its window in the app.";
/// What an account whose email isn't confirmed is told when it tries to spend (the address added
/// where it is known).
const CONFIRM_FIRST: &str = "Confirm your email first: open the link we sent you (send it again from your Foundry Account window)";

/// `text` with a capital first letter.
fn capitalized(text: &str) -> String {
    let mut c = text.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// A `%`-encoded form or query value decoded (`+` is a space); `None` if it isn't UTF-8.
fn url_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            _ => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// `name`'s value in a form body (`a=1&b=2`).
fn form_value(body: &str, name: &str) -> Option<String> {
    body.split('&').filter_map(|pair| pair.split_once('=')).find(|(k, _)| url_decode(k).as_deref() == Some(name)).and_then(|(_, v)| url_decode(v))
}

/// `name`'s value in a path's query (`/reset?token=…`).
fn query_value(path: &str, name: &str) -> Option<String> {
    form_value(path.split_once('?')?.1, name)
}

/// The reset form for `token` (checked to be a reset token's shape, so it goes in as it is), with
/// `problem` above it if there was one.
fn reset_form_html(token: &str, problem: &str) -> String {
    let problem = if problem.is_empty() { String::new() } else { format!("<p style=\"color:#b00020\">{problem}</p>") };
    format!(
        "<p>Choose a new password for your Foundry account: at least {min} characters.</p>{problem}\
         <form method=\"post\" action=\"/reset\"><input type=\"hidden\" name=\"token\" value=\"{token}\">\
         <p><label>New password<br><input type=\"password\" name=\"password\" autocomplete=\"new-password\" minlength=\"{min}\" maxlength=\"{max}\" required style=\"box-sizing:border-box;width:100%;padding:.5em;font-size:1em\"></label></p>\
         <p><label>The same again<br><input type=\"password\" name=\"again\" autocomplete=\"new-password\" minlength=\"{min}\" maxlength=\"{max}\" required style=\"box-sizing:border-box;width:100%;padding:.5em;font-size:1em\"></label></p>\
         <p><button type=\"submit\" style=\"padding:.6em 1.2em;border:0;border-radius:.4em;background:#2a5bd7;color:#fff;font-size:1em\">Set the new password</button></p></form>",
        min = auth::PASSWORD_LEN.0,
        max = auth::PASSWORD_LEN.1,
    )
}

/// A reset page around `body`.
fn reset_html(body: &str) -> Response {
    link_html("Reset your password", body)
}

/// A page an emailed link opens, titled `title`, around `body`. The link's token is in its
/// address, so the page is never cached, framed, or named to another site it links to.
fn link_html(title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>{title} · StitchFoundry</title><body style=\"font-family:sans-serif;max-width:32em;margin:4em auto;padding:0 1em;line-height:1.5\"><h1>StitchFoundry</h1><h2>{title}</h2>{body}</body>"
    );
    let headers = [
        ("Cache-Control", "no-store"),
        ("Referrer-Policy", "no-referrer"),
        ("X-Frame-Options", "DENY"),
        ("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'"),
    ];
    Response {
        status: 200,
        content_type: "text/html; charset=utf-8",
        body: html,
        headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    }
}

fn store_error(e: StoreError) -> Response {
    match e {
        StoreError::Taken => Response::error(409, "invalid_request_error", "an account with that email already exists: sign in instead"),
        StoreError::NoAccount => Response::error(401, "authentication_error", "sign in again"),
        StoreError::NotEnough => Response::error(402, "billing_error", "not enough Foundry credits: buy more in Account"),
        StoreError::Unconfirmed => Response::error(403, "permission_error", format!("{CONFIRM_FIRST}.")),
        StoreError::Io(e) => {
            log::error!("store: {e}");
            Response::error(500, "api_error", "the service couldn't save: try again later")
        }
    }
}

impl Service {
    pub fn new(cfg: Config, store: Store, upstream: Box<dyn Upstream>) -> Service {
        Service::with_clock(cfg, store, upstream, Box::new(now))
    }

    pub fn with_clock(cfg: Config, store: Store, upstream: Box<dyn Upstream>, clock: Box<dyn Fn() -> u64 + Send + Sync>) -> Service {
        Service {
            cfg,
            store: Mutex::new(store),
            throttle: Mutex::default(),
            resets: Mutex::default(),
            verifies: Mutex::default(),
            in_flight: Mutex::default(),
            upstream,
            clock,
        }
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        // A panic while holding the lock can't leave the store half changed (each change is
        // whole), so a poisoned lock is taken over.
        self.store.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The answer to `req`, readable by the web app in a browser (CORS: sessions are bearer tokens,
    /// never cookies, so any page may call; Stripe's own pages don't need it).
    pub fn handle(&self, req: &Request) -> Response {
        let mut r = self.route(req);
        r.headers.extend(CORS.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        r
    }

    fn route(&self, req: &Request) -> Response {
        if req.body.len() > MAX_BODY {
            return Response::error(413, "request_too_large", "the request is too large");
        }
        let path = req.path.split('?').next().unwrap_or_default();
        match (req.method.as_str(), path) {
            // a browser asking whether it may call
            ("OPTIONS", _) => Response { status: 204, content_type: "text/plain", body: String::new(), headers: Vec::new() },
            ("GET", "/v1/health") => Response::json(
                200,
                &json!({ "ok": true, "ai": self.cfg.anthropic_key.is_some(), "payments": self.cfg.stripe_key.is_some() && self.cfg.stripe_webhook_secret.is_some(), "email": self.cfg.resend_key.is_some() }),
            ),
            ("POST", "/v1/signup") => self.signup(req),
            ("POST", "/v1/signin") => self.signin(req),
            ("POST", "/v1/signout") => self.signout(req),
            ("POST", "/v1/password/forgot") => self.forgot(req),
            ("POST", "/v1/password/reset") => self.reset_json(req),
            ("GET", "/reset") => self.reset_page(req),
            ("POST", "/reset") => self.reset_form(req),
            ("GET", "/verify") => self.verify_page(req),
            ("POST", "/v1/verify/resend") => self.resend_verify(req),
            // (POST too: the app's client only posts)
            ("GET" | "POST", "/v1/account") => self.account(req),
            ("POST", "/v1/checkout") => self.checkout(req),
            ("POST", "/v1/stripe/webhook") => self.webhook(req),
            ("POST", "/v1/messages") => self.messages(req),
            ("POST", "/v1/sewout") => self.sewout(req),
            ("POST", "/v1/sewout/delete") => self.delete_shared(req),
            ("POST", "/v1/report") => self.report(req),
            ("GET", "/paid") => self.paid(req),
            ("GET", "/cancelled") => {
                Response::page("The payment was cancelled; nothing was charged.", "You can close this tab and go back to StitchFoundry.")
            }
            _ => Response::error(404, "not_found_error", "no such endpoint"),
        }
    }

    /// The account the request's token stands for.
    fn signed_in(&self, req: &Request) -> Result<u64, Response> {
        let token = req.header("authorization").and_then(|h| h.strip_prefix("Bearer ")).map(str::trim).filter(|t| t.starts_with("sfs_"));
        let Some(token) = token else { return Err(Response::error(401, "authentication_error", "sign in to your Foundry account")) };
        let now = (self.clock)();
        self.store()
            .session(&auth::token_hash(token), now)
            .ok_or_else(|| Response::error(401, "authentication_error", "your sign-in has ended: sign in again"))
    }

    fn account_json(&self, store: &Store, id: u64) -> Value {
        let a = store.account(id);
        let month = sewout::month((self.clock)());
        let used = store.checks_in(id, &month);
        // none while the month's free offers are used up
        let free_checks = if self.free_left(store, &month) { self.cfg.free_checks.saturating_sub(used as u64) } else { 0 };
        json!({
            // the account's lasting number: other SiteSentrix services (MakerFoundry) keep their
            // own records under it, since it never changes
            "id": id,
            "email": a.map(|a| a.email.as_str()),
            // whether the email is confirmed; until it is, nothing can be spent, and the link
            // went to `email`
            "verified": a.is_some_and(|a| self.verified(a)),
            "credits": a.map_or(0, |a| a.credits.saturating_sub(a.reserved)),
            "freeChecks": free_checks,
            "checkCredits": self.cfg.check_credits,
            // what a dollar of AI use costs in credits, for the app's estimates
            "creditsPerDollar": 100.0 * self.cfg.markup / self.cfg.cents_per_credit,
            "sharedChecks": store.shared_checks(id).len(),
            "reports": store.reports(id).len(),
        })
    }

    /// Whether `a`'s email counts as confirmed: confirmed, or the service sends no email (then
    /// nobody could confirm, so nobody is kept waiting).
    fn verified(&self, a: &crate::store::Account) -> bool {
        a.verified || self.cfg.resend_key.is_none()
    }

    /// Refuses (403) an account whose email isn't confirmed, saying where the link went. With no
    /// email set up, an account still waiting (made while it was) is confirmed now.
    fn confirmed(&self, id: u64) -> Result<(), Response> {
        let mut store = self.store();
        let Some(a) = store.account(id) else { return Err(store_error(StoreError::NoAccount)) };
        if a.verified {
            return Ok(());
        }
        if self.cfg.resend_key.is_none() {
            return store.confirm(id).map_err(store_error);
        }
        Err(Response::error(
            403,
            "permission_error",
            format!("Confirm your email first: we sent a link to {} (send it again from your Foundry Account window).", a.email),
        ))
    }

    /// Emails `email` (account `id`'s) a link that confirms it, replacing any earlier one.
    fn send_verify(&self, id: u64, email: &str, now: u64) -> Result<(), String> {
        let Some(key) = &self.cfg.resend_key else { return Err("email isn't set up".into()) };
        let (token, hash) = auth::new_verify_token()?;
        self.store().add_verify(hash, id, now).map_err(|e| e.to_string())?;
        let link = format!("{}/verify?token={token}", self.cfg.public_url);
        let text = format!(
            "Welcome to your Foundry Account, for StitchFoundry and its sister apps.\n\n\
             To confirm that {email} is your address, open this link within 48 hours:\n{link}\n\n\
             Until you do, you can sign in, but the AI features, the free sew-out checks and buying \
             credits wait. Your free starting credits are kept for you.\n\n\
             If the link has expired, send a new one from the Foundry Account window in the app.\n\n\
             If you didn't make this account, ignore this email.\n\n\
             StitchFoundry · support@stitchfoundry.app\n"
        );
        let mail = json!({ "from": self.cfg.mail_from, "to": [email], "reply_to": "support@stitchfoundry.app", "subject": "Confirm your email for your Foundry Account", "text": text });
        match self.upstream.post(
            RESEND_URL,
            &[("authorization", format!("Bearer {key}")), ("content-type", "application/json".into())],
            &mail.to_string(),
        ) {
            Ok((status, _)) if (200..300).contains(&status) => Ok(()),
            // (logged without the link, which would confirm the address)
            Ok((status, _)) => {
                log::error!("confirmation email for account {id} refused ({status})");
                Err(format!("refused ({status})"))
            }
            Err(e) => {
                log::error!("confirmation email for account {id} didn't go: {e}");
                Err(e)
            }
        }
    }

    /// Whether another confirmation email may go to `email` now (counting this one if so).
    fn verify_email_allowed(&self, email: &str, now: u64) -> bool {
        let mut throttle = self.verifies.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !throttle.allowed(email, now) {
            return false;
        }
        throttle.failed(email, now);
        true
    }

    /// `POST /v1/verify/resend`: the confirmation link again, for a signed-in account still
    /// waiting → `{sent, verified, message}`; at most five emails an address in 15 minutes.
    fn resend_verify(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        let Some((email, verified)) = self.store().account(id).map(|a| (a.email.clone(), self.verified(a))) else {
            return store_error(StoreError::NoAccount);
        };
        if verified {
            return Response::json(200, &json!({ "sent": false, "verified": true, "message": "Your email is confirmed already." }));
        }
        let now = (self.clock)();
        if !self.verify_email_allowed(&email, now) {
            return Response::error(429, "rate_limit_error", "a few links went to that address lately: wait 15 minutes and try again");
        }
        match self.send_verify(id, &email, now) {
            Ok(()) => Response::json(
                200,
                &json!({ "sent": true, "verified": false, "message": format!("A new link is on its way to {email}. It works for 48 hours.") }),
            ),
            Err(_) => Response::error(502, "api_error", "the email couldn't be sent: try again later"),
        }
    }

    /// The page a confirmation link opens: it confirms the email, and says so.
    fn verify_page(&self, req: &Request) -> Response {
        let token = query_value(&req.path, "token").unwrap_or_default();
        let title = "Confirm your email";
        if !auth::is_verify_token(&token) {
            return link_html(title, &format!("<p>{VERIFY_EXPIRED}</p>"));
        }
        let now = (self.clock)();
        let used = self.store().use_verify(&auth::token_hash(&token), now);
        match used {
            Ok(Some(_)) => link_html(
                title,
                "<p>Your email is confirmed: your Foundry Account is ready to use.</p><p>You can close this page. In the app, open the Foundry Account window and press Refresh.</p>",
            ),
            Ok(None) => link_html(title, &format!("<p>{VERIFY_EXPIRED}</p>")),
            Err(e) => {
                log::error!("verify: {e}");
                link_html(title, "<p>Your email couldn't be confirmed just now: open the link again in a few minutes.</p>")
            }
        }
    }

    /// Whether the free offers have budget left in `month` (`SF_CLOUD_FREE_BUDGET_CENTS`).
    fn free_left(&self, store: &Store, month: &str) -> bool {
        store.free_spent(month) < self.cfg.free_budget_cents
    }

    fn new_session(&self, id: u64) -> Response {
        let (token, hash) = match auth::new_token() {
            Ok(t) => t,
            Err(e) => return Response::error(500, "api_error", e),
        };
        let now = (self.clock)();
        let mut store = self.store();
        if let Err(e) = store.add_session(hash, id, now) {
            return store_error(e);
        }
        Response::json(200, &json!({ "token": token, "account": self.account_json(&store, id) }))
    }

    fn credentials(req: &Request) -> Result<(String, String), Response> {
        let v = body_json(req)?;
        let email = v
            .get("email")
            .and_then(Value::as_str)
            .and_then(auth::clean_email)
            .ok_or_else(|| Response::error(400, "invalid_request_error", "give a real email address"))?;
        let password = v.get("password").and_then(Value::as_str).unwrap_or_default().to_string();
        Ok((email, password))
    }

    fn signup(&self, req: &Request) -> Response {
        let (email, password) = match Self::credentials(req) {
            Ok(c) => c,
            Err(r) => return r,
        };
        if let Err(e) = auth::check_password(&password) {
            return Response::error(400, "invalid_request_error", e);
        }
        let hash = match auth::hash_password(&password) {
            Ok(h) => h,
            Err(e) => return Response::error(500, "api_error", e),
        };
        let now = (self.clock)();
        let month = sewout::month(now);
        // with email set up, the account waits for its address to be confirmed; without, nobody
        // could confirm it, so it starts confirmed
        let verified = self.cfg.resend_key.is_none();
        let id = {
            let mut store = self.store();
            // the starting credits, while the month's free offers can afford them
            let cost = pricing::grant_cost_cents(&self.cfg, self.cfg.signup_credits);
            let credits = if store.free_spent(&month).saturating_add(cost) <= self.cfg.free_budget_cents { self.cfg.signup_credits } else { 0 };
            let id = match store.create(&email, hash, credits, verified, now) {
                Ok(id) => id,
                Err(e) => return store_error(e),
            };
            if credits > 0
                && let Err(e) = store.add_free_spent(&month, cost)
            {
                log::error!("free budget: {e}");
            }
            id
        };
        // (an email that didn't go is sent again from the account window)
        if !verified && self.verify_email_allowed(&email, now) {
            let _ = self.send_verify(id, &email, now);
        }
        self.new_session(id)
    }

    fn signin(&self, req: &Request) -> Response {
        let (email, password) = match Self::credentials(req) {
            Ok(c) => c,
            Err(r) => return r,
        };
        let now = (self.clock)();
        let mut throttle = self.throttle.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if !throttle.allowed(&email, now) {
            return Response::error(429, "rate_limit_error", "too many wrong passwords: wait 15 minutes and try again");
        }
        let account = self.store().by_email(&email).map(|a| (a.id, a.password.clone()));
        // a missing account is checked against a hash too, so its answer takes as long
        let ok = match &account {
            Some((_, hash)) => auth::verify_password(&password, hash),
            None => {
                let _ = auth::hash_password(&password); // (only the time it takes matters)
                false
            }
        };
        match account {
            Some((id, _)) if ok => {
                throttle.succeeded(&email);
                drop(throttle);
                self.new_session(id)
            }
            _ => {
                throttle.failed(&email, now);
                Response::error(401, "authentication_error", "the email or the password is wrong")
            }
        }
    }

    /// A password-reset link, emailed to the account's address. The answer is the same whether
    /// or not there is an account with that email, so it can't be used to find out who has one.
    fn forgot(&self, req: &Request) -> Response {
        let v = match body_json(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let Some(email) = v.get("email").and_then(Value::as_str).and_then(auth::clean_email) else {
            return Response::error(400, "invalid_request_error", "give a real email address");
        };
        let Some(key) = &self.cfg.resend_key else {
            return Response::error(503, "api_error", "password reset by email isn't set up yet: write to support@stitchfoundry.app");
        };
        let now = (self.clock)();
        {
            let mut resets = self.resets.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if !resets.allowed(&email, now) {
                return Response::error(429, "rate_limit_error", "a few reset emails went to that address lately: wait 15 minutes");
            }
            resets.failed(&email, now);
        }
        let sent = json!({ "sent": true, "message": "If there's a Foundry account with that email, a link to reset its password is on its way. It works for an hour." });
        let Some(id) = self.store().by_email(&email).map(|a| a.id) else { return Response::json(200, &sent) };
        let (token, hash) = match auth::new_reset_token() {
            Ok(t) => t,
            Err(e) => return Response::error(500, "api_error", e),
        };
        if let Err(e) = self.store().add_reset(hash, id, now) {
            return store_error(e);
        }
        let link = format!("{}/reset?token={token}", self.cfg.public_url);
        let text = format!(
            "Someone, hopefully you, asked to reset the password of the Foundry account for {email}.\n\n\
             To choose a new password, open this link within an hour:\n{link}\n\n\
             It works once. Setting a new password signs the account out everywhere.\n\n\
             If you didn't ask, ignore this email: your password stays as it is.\n\n\
             StitchFoundry · support@stitchfoundry.app\n"
        );
        let mail = json!({ "from": self.cfg.mail_from, "to": [email], "reply_to": "support@stitchfoundry.app", "subject": "Reset your StitchFoundry password", "text": text });
        match self.upstream.post(
            RESEND_URL,
            &[("authorization", format!("Bearer {key}")), ("content-type", "application/json".into())],
            &mail.to_string(),
        ) {
            Ok((status, _)) if (200..300).contains(&status) => {}
            // (logged without the link, which would let whoever reads the log in)
            Ok((status, _)) => log::error!("reset email for account {id} refused ({status})"),
            Err(e) => log::error!("reset email for account {id} didn't go: {e}"),
        }
        Response::json(200, &sent)
    }

    /// Sets the password a reset link is for: `Ok` with the account, or the reason it can't.
    fn use_reset(&self, token: &str, password: &str) -> Result<u64, String> {
        if !auth::is_reset_token(token) {
            return Err(EXPIRED.into());
        }
        auth::check_password(password)?;
        let hash = auth::hash_password(password)?;
        let now = (self.clock)();
        let mut store = self.store();
        match store.use_reset(&auth::token_hash(token), hash, now) {
            Ok(Some(id)) => {
                // wrong guesses before don't keep the owner out now
                if let Some(email) = store.account(id).map(|a| a.email.clone()) {
                    drop(store);
                    self.throttle.lock().unwrap_or_else(std::sync::PoisonError::into_inner).succeeded(&email);
                }
                Ok(id)
            }
            Ok(None) => Err(EXPIRED.into()),
            Err(e) => {
                log::error!("reset: {e}");
                Err("the new password couldn't be saved: try again later".into())
            }
        }
    }

    /// `POST /v1/password/reset {token, password}`, for apps → `{ok: true}`.
    fn reset_json(&self, req: &Request) -> Response {
        let v = match body_json(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let token = v.get("token").and_then(Value::as_str).unwrap_or_default();
        let password = v.get("password").and_then(Value::as_str).unwrap_or_default();
        match self.use_reset(token, password) {
            Ok(_) => Response::json(200, &json!({ "ok": true })),
            Err(why) => Response::error(400, "invalid_request_error", why),
        }
    }

    /// The page a reset link opens: a form for the new password, or why the link no longer works.
    fn reset_page(&self, req: &Request) -> Response {
        let token = query_value(&req.path, "token").unwrap_or_default();
        let now = (self.clock)();
        if !auth::is_reset_token(&token) || self.store().reset_for(&auth::token_hash(&token), now).is_none() {
            return reset_html(&format!("<p>{EXPIRED}.</p>"));
        }
        reset_html(&reset_form_html(&token, ""))
    }

    /// The reset page's form, posted.
    fn reset_form(&self, req: &Request) -> Response {
        let body = String::from_utf8_lossy(&req.body);
        let field = |name: &str| form_value(&body, name).unwrap_or_default();
        let (token, password) = (field("token"), field("password"));
        if !auth::is_reset_token(&token) {
            return reset_html(&format!("<p>{EXPIRED}.</p>"));
        }
        if password != field("again") {
            return reset_html(&reset_form_html(&token, "The two passwords aren't the same."));
        }
        match self.use_reset(&token, &password) {
            Ok(_) => {
                reset_html("<p>Your password is changed, and the account is signed out everywhere.</p><p>Sign in again with your new password.</p>")
            }
            Err(why) if why == EXPIRED => reset_html(&format!("<p>{EXPIRED}.</p>")),
            Err(why) => reset_html(&reset_form_html(&token, &format!("{}.", capitalized(&why)))),
        }
    }

    fn signout(&self, req: &Request) -> Response {
        if let Some(token) = req.header("authorization").and_then(|h| h.strip_prefix("Bearer "))
            && let Err(e) = self.store().end_session(&auth::token_hash(token.trim()))
        {
            return store_error(e);
        }
        Response::json(200, &json!({}))
    }

    fn account(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        let store = self.store();
        let mut v = self.account_json(&store, id);
        v["packs"] = json!(self.cfg.packs);
        v["payments"] = json!(self.cfg.stripe_key.is_some());
        let history: Vec<Value> = store
            .recent(id)
            .iter()
            .rev()
            .take(20)
            .map(|e| json!({ "at": e.at, "kind": e.kind, "credits": e.credits, "balance": e.balance, "note": e.note }))
            .collect();
        v["history"] = json!(history);
        Response::json(200, &v)
    }

    /// Stripe's return page after a payment (`/paid?pack=…`): the pack's credits when the pack is
    /// one on sale (only the service's own numbers are shown, nothing from the address).
    fn paid(&self, req: &Request) -> Response {
        let query = req.path.split_once('?').map(|(_, q)| q).unwrap_or_default();
        let pack_id = query.split('&').find_map(|kv| kv.strip_prefix("pack="));
        let next = "You can close this tab: in StitchFoundry, press Refresh in Account to see them.";
        match pack_id.and_then(|id| self.cfg.packs.iter().find(|p| p.id == id)) {
            Some(p) => Response::page(
                &format!(
                    "Thank you. Your {} credits (${}.{:02}) are added as soon as the payment is through, usually within a minute.",
                    thousands(p.credits),
                    p.cents / 100,
                    p.cents % 100
                ),
                next,
            ),
            None => Response::page("Thank you. Your credits are added as soon as the payment is through, usually within a minute.", next),
        }
    }

    fn checkout(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        // payments go only to confirmed addresses (receipts, refunds)
        if let Err(r) = self.confirmed(id) {
            return r;
        }
        let Some(key) = &self.cfg.stripe_key else {
            return Response::error(503, "api_error", "buying credits isn't open yet");
        };
        let v = match body_json(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let pack_id = v.get("pack").and_then(Value::as_str).unwrap_or_default();
        let Some(pack) = self.cfg.packs.iter().find(|p| p.id == pack_id) else {
            return Response::error(400, "invalid_request_error", "no such pack of credits");
        };
        let email = self.store().account(id).map(|a| a.email.clone()).unwrap_or_default();
        let form = stripe::checkout_form(&self.cfg, pack, id, &email);
        let headers = [("authorization", format!("Bearer {key}")), ("content-type", "application/x-www-form-urlencoded".to_string())];
        match self.upstream.post(stripe::CHECKOUT_URL, &headers, &form) {
            Ok((200..=299, body)) => {
                match serde_json::from_str::<Value>(&body).ok().and_then(|v| v.get("url").and_then(Value::as_str).map(str::to_string)) {
                    Some(url) => Response::json(200, &json!({ "url": url })),
                    None => Response::error(502, "api_error", "the payment service gave no checkout page"),
                }
            }
            Ok((status, body)) => {
                log::error!("stripe checkout: {status}: {}", body.chars().take(300).collect::<String>());
                Response::error(502, "api_error", "the payment service refused: try again later")
            }
            Err(e) => {
                log::error!("stripe checkout: {e}");
                Response::error(502, "api_error", "couldn't reach the payment service")
            }
        }
    }

    fn webhook(&self, req: &Request) -> Response {
        let Some(secret) = &self.cfg.stripe_webhook_secret else {
            return Response::error(503, "api_error", "payments aren't set up");
        };
        let now = (self.clock)();
        if let Err(e) = stripe::verify(&req.body, req.header("stripe-signature").unwrap_or_default(), secret, now) {
            log::warn!("stripe webhook refused: {e}");
            return Response::error(400, "invalid_request_error", e);
        }
        let event = match body_json(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let Some(p) = stripe::paid(&event) else {
            // other events are acknowledged and ignored
            return Response::json(200, &json!({ "received": true }));
        };
        // credited once a checkout, whichever of its events comes first
        let key = format!("checkout:{}", p.session);
        match self.store().add_credits(p.account, p.credits, "purchase", &key, &format!("{} pack ({})", p.pack, p.event), now) {
            Ok(added) => Response::json(200, &json!({ "received": true, "credited": added })),
            // Stripe sends the event again later on an error: an unknown account is reported, not retried
            Err(StoreError::NoAccount) => {
                log::error!("stripe: paid checkout {} for unknown account {}", p.session, p.account);
                Response::json(200, &json!({ "received": true, "credited": false }))
            }
            Err(e) => store_error(e),
        }
    }

    /// A Messages request checked and cut down to what is passed on → (body, max_tokens).
    fn checked_body(&self, req: &Request) -> Result<(Value, u64), Response> {
        let v = body_json(req)?;
        let Some(o) = v.as_object() else { return Err(Response::error(400, "invalid_request_error", "the body is a JSON object")) };
        if o.get("stream").and_then(Value::as_bool) == Some(true) {
            return Err(Response::error(400, "invalid_request_error", "streaming isn't offered"));
        }
        let model = o.get("model").and_then(Value::as_str).unwrap_or_default();
        if !self.cfg.allowed.iter().any(|m| m == model) {
            return Err(Response::error(400, "invalid_request_error", format!("the model {model:?} isn't offered")));
        }
        let max_tokens = o.get("max_tokens").and_then(Value::as_u64).unwrap_or(0);
        if max_tokens == 0 || max_tokens > self.cfg.max_tokens {
            return Err(Response::error(400, "invalid_request_error", format!("max_tokens is 1 to {}", self.cfg.max_tokens)));
        }
        if let Some(e) = o.get("tools").and_then(tools_refused) {
            return Err(Response::error(400, "invalid_request_error", e));
        }
        let body: serde_json::Map<String, Value> =
            o.iter().filter(|(k, _)| PASSED.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        Ok((Value::Object(body), max_tokens))
    }

    fn messages(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        if let Err(r) = self.confirmed(id) {
            return r;
        }
        let Some(key) = self.cfg.anthropic_key.clone() else {
            return Response::error(503, "api_error", "StitchFoundry's AI isn't open yet: use your own Anthropic API key for now");
        };
        let (body, max_tokens) = match self.checked_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        self.forward(req, id, key, &body, max_tokens, Charge::Metered, "AI request")
    }

    /// Check My Sew-out: a check's requests, with the service's own instructions; the month's
    /// first checks free, then a flat price a check (see `sewout.rs`).
    fn sewout(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        if let Err(r) = self.confirmed(id) {
            return r;
        }
        let Some(key) = self.cfg.anthropic_key.clone() else {
            return Response::error(503, "api_error", "StitchFoundry's AI isn't open yet: use your own Anthropic API key for now");
        };
        let (body, max_tokens) = match self.checked_body(req) {
            Ok(b) => b,
            Err(r) => return r,
        };
        if let Some(why) = sewout::not_a_check(&body) {
            return Response::error(400, "invalid_request_error", why);
        }
        let Some(check) = req.header("x-stitchfoundry-check").filter(|c| sewout::good_id(c)).map(|c| format!("{id}-{c}")) else {
            return Response::error(400, "invalid_request_error", "a check needs its id (x-stitchfoundry-check)");
        };
        let body = sewout::with_own_instructions(body);
        let now = (self.clock)();
        let month = sewout::month(now);
        let share = req.header("x-stitchfoundry-share") == Some("1");
        let follow_up = req.header("x-stitchfoundry-follow-up").filter(|c| sewout::good_id(c)).map(|c| format!("{id}-{c}"));
        let (charge, free, started) = {
            let mut store = self.store();
            match store.check(&check).filter(|c| c.account == id) {
                // its rounds are covered by what the check cost
                Some(c) if now.saturating_sub(c.created) < sewout::CHECK_LIFE && c.requests < sewout::MAX_ROUNDS => (Charge::Flat(0), c.free, false),
                // past the check: as any request
                Some(c) => (Charge::Metered, c.free, false),
                None => {
                    let free = (store.checks_in(id, &month) as u64) < self.cfg.free_checks && self.free_left(&store, &month);
                    // a free check takes its slot now, under the lock, so that checks started at
                    // the same time can't all take the month's last one (given back on failure)
                    if free {
                        let slot = Check {
                            account: id,
                            month: month.clone(),
                            created: now,
                            requests: 0,
                            free,
                            shared: share,
                            follow_up: follow_up.clone(),
                            spent: 0,
                        };
                        if let Err(e) = store.start_check(&check, slot) {
                            return store_error(e);
                        }
                    }
                    (Charge::Flat(if free { 0 } else { self.cfg.check_credits }), free, free)
                }
            }
        };
        // only a round charged nothing is paid by the free offers; a free check's rounds past its
        // life are paid in credits, so they aren't counted against the month's free budget too
        let on_the_house = free && charge == Charge::Flat(0);
        let mut resp = self.forward(req, id, key, &body, max_tokens, charge, if on_the_house { "sew-out check (free)" } else { "sew-out check" });
        if started && !(200..300).contains(&resp.status) {
            // the free slot wasn't used: give it back
            if let Err(e) = self.store().drop_unstarted_check(&check) {
                log::error!("sew-out check {check}: {e}");
            }
        }
        if (200..300).contains(&resp.status) {
            let mut store = self.store();
            if on_the_house {
                // a free check's rounds count against the month's free offers at what they cost
                let cost = serde_json::from_str::<Value>(&resp.body).ok().map_or(0, |r| {
                    let price = r.get("model").and_then(Value::as_str).and_then(|m| self.cfg.model(m)).cloned().unwrap_or_else(|| self.cfg.dearest());
                    pricing::cost_cents(&price, r.get("usage").unwrap_or(&Value::Null))
                });
                if let Err(e) = store.add_free_spent(&month, cost) {
                    log::error!("free budget: {e}");
                }
            }
            let spent = resp.headers.iter().find(|(k, _)| k == "x-stitchfoundry-spent").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
            let noted =
                Check { account: id, month: month.clone(), created: now, requests: 0, free, shared: share, follow_up: follow_up.clone(), spent };
            if let Err(e) = store.note_check(&check, noted) {
                log::error!("sew-out check {check}: {e}");
            }
            if share {
                let (files, line) = sewout::shared_round(&body, &resp.body, id, follow_up.as_deref(), now);
                // (the check went through; keeping it is a bonus)
                if let Err(e) = store.keep_shared(&check, &files, &line) {
                    log::error!("sew-out check {check}: {e}");
                }
            }
            let left = if self.free_left(&store, &month) { self.cfg.free_checks.saturating_sub(store.checks_in(id, &month) as u64) } else { 0 };
            resp.headers.push(("x-stitchfoundry-free-checks".into(), left.to_string()));
        }
        resp
    }

    /// Delete a shared check (`{check}`), or all of the account's (`{}`) → `{deleted}`.
    fn delete_shared(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        let v = match body_json(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let mut store = self.store();
        let ids: Vec<String> = match v.get("check").and_then(Value::as_str) {
            Some(c) if sewout::good_id(c) => vec![format!("{id}-{c}")],
            Some(_) => return Response::error(400, "invalid_request_error", "no such check"),
            None => store.shared_checks(id),
        };
        let all = v.get("check").is_none();
        let deleted = store.delete_shared(&ids).and_then(|n| if all { store.delete_reports(id).map(|r| (n, r)) } else { Ok((n, 0)) });
        match deleted {
            Ok((n, r)) => Response::json(200, &json!({ "deleted": n, "reports": r })),
            Err(e) => store_error(e),
        }
    }

    /// Report to StitchFoundry: a sew-out and what StitchFoundry could do better, sent with the
    /// user's leave; kept privately (deleted with the shared checks) and emailed to the owner
    /// when Resend is set up. The check it reports is on the house: the credits it cost come back,
    /// once → `{report, emailed, refunded}`.
    fn report(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
        let v = match body_json(req) {
            Ok(v) => v,
            Err(r) => return r,
        };
        let report = match sewout::read_report(&v) {
            Ok(r) => r,
            Err(why) => return Response::error(400, "invalid_request_error", why),
        };
        let now = (self.clock)();
        let (rid, email, refunded) = {
            let mut store = self.store();
            if store.reports_since(id, now.saturating_sub(24 * 3600)) >= sewout::MAX_REPORTS_A_DAY {
                return Response::error(
                    429,
                    "rate_limit_error",
                    "you've sent a few reports today: send more tomorrow, or write to support@stitchfoundry.app",
                );
            }
            let email = store.account(id).map(|a| a.email.clone()).unwrap_or_default();
            let rid = format!("report-{id}-{now}-{}", store.reports(id).len() + 1);
            let kept = report.kept(&rid, id, &email, now);
            if let Err(e) = store.keep_report(&rid, crate::store::Report { account: id, created: now }, &report.photos, &kept) {
                return store_error(e);
            }
            // the check it reports is on us: what it cost comes back, once
            let check = report.check.as_ref().map(|c| format!("{id}-{c}"));
            let spent = check.as_ref().and_then(|c| store.check(c)).filter(|c| c.account == id).map_or(0, |c| c.spent);
            let refunded = match check {
                Some(c) if spent > 0 => {
                    match store.add_credits(id, spent, "refund", &format!("refund-{c}"), "sew-out report: the check is on us", now) {
                        Ok(true) => spent,
                        Ok(false) => 0,
                        Err(e) => {
                            log::error!("report {rid}: the refund didn't go: {e}");
                            0
                        }
                    }
                }
                _ => 0,
            };
            (rid, email, refunded)
        };
        // the report is kept; the email is how the owner hears of it
        let emailed = match (&self.cfg.resend_key, &self.cfg.reports_to) {
            (Some(key), Some(to)) => {
                let mail = report.email(&self.cfg.reports_from, to, &rid, &email);
                match self.upstream.post(
                    RESEND_URL,
                    &[("authorization", format!("Bearer {key}")), ("content-type", "application/json".into())],
                    &mail.to_string(),
                ) {
                    Ok((status, _)) if (200..300).contains(&status) => true,
                    Ok((status, body)) => {
                        log::error!("report {rid}: the email was refused ({status}): {body}");
                        false
                    }
                    Err(e) => {
                        log::error!("report {rid}: the email didn't go: {e}");
                        false
                    }
                }
            }
            _ => false,
        };
        Response::json(200, &json!({ "report": rid, "emailed": emailed, "refunded": refunded }))
    }

    /// Send a checked request to Anthropic with the service's key, holding then taking its price:
    /// what it used ([`Charge::Metered`]) or a flat price. Only a reply that came through is paid
    /// for; a refused or failed one costs nothing.
    #[allow(clippy::too_many_arguments)]
    fn forward(&self, req: &Request, id: u64, key: String, body: &Value, max_tokens: u64, charge: Charge, note: &str) -> Response {
        let most = pricing::estimate(&self.cfg, &self.cfg.dearest(), body, max_tokens);
        let hold = {
            let mut flying = self.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let n = flying.entry(id).or_default();
            if *n >= MAX_IN_FLIGHT {
                return Response::error(429, "rate_limit_error", "a few requests are on their way already: wait for them");
            }
            let mut store = self.store();
            let hold = match charge {
                Charge::Metered => {
                    let free = store.account(id).map_or(0, |a| a.credits.saturating_sub(a.reserved));
                    if free < MIN_TO_START.min(most) {
                        return store_error(StoreError::NotEnough);
                    }
                    most.min(free)
                }
                Charge::Flat(c) => c,
            };
            if let Err(e) = store.reserve(id, hold) {
                return store_error(e);
            }
            *n += 1;
            hold
        };
        let headers = [
            ("content-type", "application/json".to_string()),
            ("x-api-key", key),
            ("anthropic-version", req.header("anthropic-version").unwrap_or("2023-06-01").to_string()),
            ("anthropic-beta", allowed_betas(req.header("anthropic-beta").unwrap_or_default())),
        ];
        let headers: Vec<(&str, String)> = headers.into_iter().filter(|(_, v)| !v.is_empty()).collect();
        let answer = self.upstream.post(ANTHROPIC_URL, &headers, &body.to_string());
        let spent = match (&answer, charge) {
            (Ok((200..=299, text)), Charge::Metered) => serde_json::from_str::<Value>(text).ok().map_or(0, |r| {
                let price = r.get("model").and_then(Value::as_str).and_then(|m| self.cfg.model(m)).cloned().unwrap_or_else(|| self.cfg.dearest());
                pricing::charge(&self.cfg, &price, r.get("usage").unwrap_or(&Value::Null))
            }),
            (Ok((200..=299, _)), Charge::Flat(c)) => c,
            _ => 0,
        };
        let now = (self.clock)();
        let left = {
            let mut store = self.store();
            store.settle(id, hold, spent.min(hold), note, now).map(|l| l.saturating_sub(store.account(id).map_or(0, |a| a.reserved)))
        };
        // (the locks are always taken in-flight first, then the store: never the other way round)
        if let Some(n) = self.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get_mut(&id) {
            *n = n.saturating_sub(1);
        }
        let left = match left {
            Ok(l) => l,
            Err(e) => return store_error(e),
        };
        match answer {
            Ok((status, text)) => Response {
                status,
                content_type: "application/json",
                body: text,
                headers: vec![("x-stitchfoundry-credits".into(), left.to_string()), ("x-stitchfoundry-spent".into(), spent.to_string())],
            },
            Err(e) => {
                log::error!("anthropic: {e}");
                Response::error(502, "api_error", "couldn't reach the AI service: nothing was charged")
            }
        }
    }
}

/// What a request costs: what it used, or a flat price (a sew-out check; 0 when free or covered).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Charge {
    Metered,
    Flat(u64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    type Calls = Arc<StdMutex<Vec<(String, Vec<(String, String)>, String)>>>;

    struct Fake {
        calls: Calls,
        reply: (u16, String),
    }

    impl Upstream for Fake {
        fn post(&self, url: &str, headers: &[(&str, String)], body: &str) -> Result<(u16, String), String> {
            self.calls.lock().unwrap().push((url.into(), headers.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(), body.into()));
            Ok(self.reply.clone())
        }
    }

    fn service(reply: (u16, String), keys: bool) -> (Service, Calls) {
        service_with(reply, keys, None)
    }

    /// A service with a free budget of `budget` cents (else the default).
    fn service_with(reply: (u16, String), keys: bool, budget: Option<u64>) -> (Service, Calls) {
        let calls: Calls = Arc::default();
        let cfg = Config::from_env(|k| match k {
            "ANTHROPIC_API_KEY" if keys => Some("sk-ant-service".into()),
            "STRIPE_SECRET_KEY" if keys => Some("sk_test_x".into()),
            "STRIPE_WEBHOOK_SECRET" if keys => Some("whsec_x".into()),
            "SF_CLOUD_FREE_BUDGET_CENTS" => budget.map(|b| b.to_string()),
            _ => None,
        })
        .unwrap();
        let s = Service::with_clock(cfg, Store::in_memory(), Box::new(Fake { calls: calls.clone(), reply }), Box::new(|| 1_000_000));
        (s, calls)
    }

    fn req(method: &str, path: &str, token: Option<&str>, body: Value) -> Request {
        let mut headers = vec![];
        if let Some(t) = token {
            headers.push(("Authorization".to_string(), format!("Bearer {t}")));
        }
        Request { method: method.into(), path: path.into(), headers, body: body.to_string().into_bytes() }
    }

    fn json_of(r: &Response) -> Value {
        serde_json::from_str(&r.body).unwrap()
    }

    fn sign_up(s: &Service) -> String {
        let r = s.handle(&req("POST", "/v1/signup", None, json!({"email": "Denise@Example.com", "password": "fire truck 1"})));
        assert_eq!(r.status, 200, "{}", r.body);
        json_of(&r)["token"].as_str().unwrap().to_string()
    }

    #[test]
    fn sign_up_sign_in_and_out() {
        let (s, _) = service((200, String::new()), false);
        let t = sign_up(&s);
        let a = s.handle(&req("GET", "/v1/account", Some(&t), json!({})));
        assert_eq!(json_of(&a)["email"], "denise@example.com");
        assert_eq!(json_of(&a)["id"], 1, "the account's lasting number");
        assert_eq!(json_of(&a)["credits"], 100, "the starting credits");
        assert_eq!(json_of(&a)["history"][0]["kind"], "signup");
        // the same email again is refused; a short password too
        assert_eq!(s.handle(&req("POST", "/v1/signup", None, json!({"email": "denise@example.com", "password": "another one"}))).status, 409);
        assert_eq!(s.handle(&req("POST", "/v1/signup", None, json!({"email": "x@y.z", "password": "short"}))).status, 400);
        // signing in gives a new token; a wrong password doesn't
        let r = s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": "fire truck 1"})));
        assert_eq!(r.status, 200);
        assert_eq!(json_of(&r)["account"]["id"], 1, "sign-in names the account too");
        assert_eq!(s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": "wrong"}))).status, 401);
        assert_eq!(s.handle(&req("POST", "/v1/signin", None, json!({"email": "nobody@example.com", "password": "fire truck 1"}))).status, 401);
        // signing out ends the token
        s.handle(&req("POST", "/v1/signout", Some(&t), json!({})));
        assert_eq!(s.handle(&req("GET", "/v1/account", Some(&t), json!({}))).status, 401);
        assert_eq!(s.handle(&req("GET", "/v1/account", Some("sfs_made_up"), json!({}))).status, 401);
    }

    /// A service that emails through a fake Resend.
    fn service_with_email() -> (Service, Calls) {
        let calls: Calls = Arc::default();
        let cfg = Config::from_env(|k| match k {
            "RESEND_API_KEY" => Some("re_test".into()),
            "SF_CLOUD_PUBLIC_URL" => Some("https://cloud.example.com/".into()),
            _ => None,
        })
        .unwrap();
        let s =
            Service::with_clock(cfg, Store::in_memory(), Box::new(Fake { calls: calls.clone(), reply: (200, "{}".into()) }), Box::new(|| 1_000_000));
        (s, calls)
    }

    /// A form post, as a browser sends it.
    fn form(path: &str, body: &str) -> Request {
        Request {
            method: "POST".into(),
            path: path.into(),
            headers: vec![("Content-Type".into(), "application/x-www-form-urlencoded".into())],
            body: body.as_bytes().to_vec(),
        }
    }

    /// The reset link in the last email sent.
    fn emailed_link(calls: &Calls) -> String {
        let mail: Value = serde_json::from_str(&calls.lock().unwrap().last().unwrap().2).unwrap();
        let text = mail["text"].as_str().unwrap();
        text.split_whitespace().find(|w| w.starts_with("https://")).unwrap().to_string()
    }

    #[test]
    fn a_forgotten_password_is_reset_through_an_emailed_link() {
        let (s, calls) = service_with_email();
        let t = sign_up(&s);
        calls.lock().unwrap().clear(); // (the sign-up's confirmation email)
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/health", None, json!({}))))["email"], true);
        // an unknown address gets the same answer, and no email
        let unknown = s.handle(&req("POST", "/v1/password/forgot", None, json!({"email": "nobody@example.com"})));
        let known = s.handle(&req("POST", "/v1/password/forgot", None, json!({"email": " Denise@Example.com"})));
        assert_eq!((unknown.status, &unknown.body), (known.status, &known.body));
        assert_eq!(s.handle(&req("POST", "/v1/password/forgot", None, json!({"email": "not an address"}))).status, 400);
        let sent = calls.lock().unwrap().clone();
        assert_eq!(sent.len(), 1, "one email, to the account");
        let mail: Value = serde_json::from_str(&sent[0].2).unwrap();
        assert_eq!(mail["to"][0], "denise@example.com");
        assert_eq!(mail["from"], "StitchFoundry <accounts@stitchfoundry.app>");
        let link = emailed_link(&calls);
        let token = link.strip_prefix("https://cloud.example.com/reset?token=").unwrap();
        assert!(auth::is_reset_token(token), "{link}");
        // the link opens a form that keeps the token to itself
        let page = s.handle(&req("GET", &format!("/reset?token={token}"), None, json!({})));
        assert!(page.content_type.starts_with("text/html") && page.body.contains(token) && page.body.contains("new-password"));
        for (k, v) in [("Referrer-Policy", "no-referrer"), ("Cache-Control", "no-store"), ("X-Frame-Options", "DENY")] {
            assert!(page.headers.iter().any(|h| h.0 == k && h.1 == v), "{k}");
        }
        // two different passwords, or one too short, are refused and leave the link working
        let mismatch = s.handle(&form("/reset", &format!("token={token}&password=fire+truck+2&again=fire+truck+3")));
        assert!(mismatch.body.contains("aren't the same"));
        let short = s.handle(&form("/reset", &format!("token={token}&password=short&again=short")));
        assert!(short.body.contains("at least 8"), "{}", short.body);
        // the new password works, the old one and the old sign-in don't
        let done = s.handle(&form("/reset", &format!("token={token}&password=fire+truck+%232&again=fire+truck+%232")));
        assert!(done.body.contains("Your password is changed"), "{}", done.body);
        assert_eq!(s.handle(&req("GET", "/v1/account", Some(&t), json!({}))).status, 401, "signed out everywhere");
        let signin = |p: &str| s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": p}))).status;
        assert_eq!(signin("fire truck 1"), 401);
        assert_eq!(signin("fire truck #2"), 200);
        // a link works once
        let again = s.handle(&form("/reset", &format!("token={token}&password=fire+truck+4&again=fire+truck+4")));
        assert!(again.body.contains("expired or was used"));
        assert!(s.handle(&req("GET", &format!("/reset?token={token}"), None, json!({}))).body.contains("expired or was used"));
        assert!(s.handle(&req("GET", "/reset?token=%3Cscript%3E", None, json!({}))).body.contains("expired or was used"));
        // apps reset through the JSON endpoint
        s.handle(&req("POST", "/v1/password/forgot", None, json!({"email": "denise@example.com"})));
        let token = emailed_link(&calls).split_once("token=").unwrap().1.to_string();
        let r = s.handle(&req("POST", "/v1/password/reset", None, json!({"token": token, "password": "short"})));
        assert_eq!(r.status, 400);
        assert_eq!(s.handle(&req("POST", "/v1/password/reset", None, json!({"token": token, "password": "fire truck 5"}))).status, 200);
        assert_eq!(signin("fire truck 5"), 200);
        assert_eq!(s.handle(&req("POST", "/v1/password/reset", None, json!({"token": token, "password": "fire truck 6"}))).status, 400);
    }

    #[test]
    fn reset_emails_are_limited_and_need_resend() {
        let (s, calls) = service_with_email();
        sign_up(&s);
        calls.lock().unwrap().clear(); // (the sign-up's confirmation email)
        let ask = || s.handle(&req("POST", "/v1/password/forgot", None, json!({"email": "denise@example.com"}))).status;
        for _ in 0..auth::MAX_FAILURES {
            assert_eq!(ask(), 200);
        }
        assert_eq!(ask(), 429, "an inbox can't be flooded");
        assert_eq!(calls.lock().unwrap().len(), auth::MAX_FAILURES);
        // without Resend there's no way to send the link
        let (s, _) = service((200, String::new()), false);
        sign_up(&s);
        assert_eq!(s.handle(&req("POST", "/v1/password/forgot", None, json!({"email": "denise@example.com"}))).status, 503);
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/health", None, json!({}))))["email"], false);
    }

    /// A service with email, AI and payments set up, on a clock the test moves. Its fake answers
    /// every call with one reply that serves as an email's, an AI answer and a checkout page.
    fn service_confirming() -> (Service, Calls, Arc<std::sync::atomic::AtomicU64>) {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "ok"}], "usage": {"input_tokens": 10, "output_tokens": 10}, "url": "https://checkout.stripe.com/c/pay/cs_1"});
        let calls: Calls = Arc::default();
        let cfg = Config::from_env(|k| match k {
            "RESEND_API_KEY" => Some("re_test".into()),
            "ANTHROPIC_API_KEY" => Some("sk-ant-service".into()),
            "STRIPE_SECRET_KEY" => Some("sk_test_x".into()),
            "STRIPE_WEBHOOK_SECRET" => Some("whsec_x".into()),
            "SF_CLOUD_PUBLIC_URL" => Some("https://cloud.example.com".into()),
            _ => None,
        })
        .unwrap();
        let clock = Arc::new(std::sync::atomic::AtomicU64::new(1_000_000));
        let c = clock.clone();
        let fake = Fake { calls: calls.clone(), reply: (200, reply.to_string()) };
        let s = Service::with_clock(cfg, Store::in_memory(), Box::new(fake), Box::new(move || c.load(std::sync::atomic::Ordering::SeqCst)));
        (s, calls, clock)
    }

    /// The emails sent so far: (to, subject, text).
    fn emails(calls: &Calls) -> Vec<(String, String, String)> {
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.0 == RESEND_URL)
            .map(|c| {
                let m: Value = serde_json::from_str(&c.2).unwrap();
                (m["to"][0].as_str().unwrap().into(), m["subject"].as_str().unwrap().into(), m["text"].as_str().unwrap().into())
            })
            .collect()
    }

    /// The token in the last confirmation link emailed.
    fn verify_token(calls: &Calls) -> String {
        let (_, _, text) = emails(calls).pop().unwrap();
        let link = text.split_whitespace().find(|w| w.starts_with("https://")).unwrap();
        link.strip_prefix("https://cloud.example.com/verify?token=").unwrap().to_string()
    }

    fn ai(t: &str) -> Request {
        req("POST", "/v1/messages", Some(t), json!({"model": "claude-opus-5-5", "max_tokens": 100, "messages": [{"role": "user", "content": "hi"}]}))
    }

    /// CLOUD-10: a new account can sign in and read itself, but spends nothing (AI, a free sew-out
    /// check, buying credits) until the link emailed at sign-up confirms its address.
    #[test]
    fn a_new_account_confirms_its_email_before_it_spends() {
        let (s, calls, _) = service_confirming();
        let r = s.handle(&req("POST", "/v1/signup", None, json!({"email": "Denise@Example.com", "password": "fire truck 1"})));
        assert_eq!(r.status, 200, "{}", r.body);
        assert_eq!(json_of(&r)["account"]["verified"], false);
        let t = json_of(&r)["token"].as_str().unwrap().to_string();
        let account = || json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))));
        let a = account();
        assert_eq!((a["verified"].clone(), a["email"].clone(), a["credits"].clone()), (json!(false), json!("denise@example.com"), json!(100)));
        // the link went to the address, through Resend, for 48 hours
        let sent = emails(&calls);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, "denise@example.com");
        assert!(sent[0].1.contains("Confirm your email") && sent[0].2.contains("within 48 hours"), "{sent:?}");
        let token = verify_token(&calls);
        assert!(auth::is_verify_token(&token), "{token}");
        // spending is refused, saying where the link went; nothing reaches Anthropic or Stripe
        for r in [
            s.handle(&ai(&t)),
            s.handle(&check(&t, "aaaaaaaa", "aGVsbG8=", false)),
            s.handle(&req("POST", "/v1/checkout", Some(&t), json!({"pack": "small"}))),
        ] {
            assert_eq!(r.status, 403, "{}", r.body);
            let m = json_of(&r)["error"]["message"].as_str().unwrap().to_string();
            assert!(m.starts_with("Confirm your email first") && m.contains("denise@example.com") && m.contains("Foundry Account"), "{m}");
        }
        assert_eq!(calls.lock().unwrap().len(), 1, "only the email went out");
        let a = account();
        assert_eq!((a["credits"].clone(), a["freeChecks"].clone()), (json!(100), json!(3)), "the starting credits and free checks wait");
        // a wrong or made-up link confirms nothing, and isn't echoed
        for bad in [format!("sfv_{}", "0".repeat(64)), "%3Cscript%3E".into(), format!("sfr_{}", &token[4..])] {
            let page = s.handle(&req("GET", &format!("/verify?token={bad}"), None, json!({})));
            assert!(page.body.contains("expired or was used") && !page.body.contains("<script"), "{}", page.body);
        }
        assert_eq!(account()["verified"], false);
        // the link confirms it, on a page that says so
        let page = s.handle(&req("GET", &format!("/verify?token={token}"), None, json!({})));
        assert!(page.content_type.starts_with("text/html") && page.body.contains("Your email is confirmed"), "{}", page.body);
        assert!(page.headers.iter().any(|h| h.0 == "Referrer-Policy" && h.1 == "no-referrer"));
        assert_eq!(account()["verified"], true);
        // ... once
        assert!(s.handle(&req("GET", &format!("/verify?token={token}"), None, json!({}))).body.contains("expired or was used"));
        // and now it spends
        assert_eq!(s.handle(&ai(&t)).status, 200);
        assert_eq!(s.handle(&check(&t, "aaaaaaaa", "aGVsbG8=", false)).status, 200);
        assert_eq!(
            json_of(&s.handle(&req("POST", "/v1/checkout", Some(&t), json!({"pack": "small"}))))["url"],
            "https://checkout.stripe.com/c/pay/cs_1"
        );
        assert_eq!(account()["freeChecks"], 2);
    }

    /// A confirmation link works for 48 hours; after that a new one is sent from the account.
    #[test]
    fn an_expired_confirmation_link_confirms_nothing() {
        let (s, calls, clock) = service_confirming();
        let t = sign_up(&s);
        let old = verify_token(&calls);
        clock.store(1_000_000 + auth::VERIFY_TTL, std::sync::atomic::Ordering::SeqCst);
        assert!(s.handle(&req("GET", &format!("/verify?token={old}"), None, json!({}))).body.contains("expired or was used"));
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))))["verified"], false);
        assert_eq!(s.handle(&ai(&t)).status, 403);
        let r = s.handle(&req("POST", "/v1/verify/resend", Some(&t), json!({})));
        assert_eq!(r.status, 200, "{}", r.body);
        assert_eq!(json_of(&r)["sent"], true);
        let new = verify_token(&calls);
        assert_ne!(new, old);
        assert!(s.handle(&req("GET", &format!("/verify?token={new}"), None, json!({}))).body.contains("Your email is confirmed"));
        assert_eq!(s.handle(&ai(&t)).status, 200);
    }

    /// The link is sent again on asking, signed in, at most five emails an address in 15 minutes
    /// (sign-up's counted); a new link replaces the earlier one; a confirmed account gets none.
    #[test]
    fn the_confirmation_link_is_sent_again_a_few_times_at_most() {
        let (s, calls, clock) = service_confirming();
        let t = sign_up(&s);
        let first = verify_token(&calls);
        let resend = |t: Option<&str>| s.handle(&req("POST", "/v1/verify/resend", t, json!({})));
        assert_eq!(resend(None).status, 401);
        for _ in 1..auth::MAX_FAILURES {
            let r = resend(Some(&t));
            assert_eq!(r.status, 200, "{}", r.body);
            assert!(json_of(&r)["message"].as_str().unwrap().contains("denise@example.com"));
        }
        let r = resend(Some(&t));
        assert_eq!(r.status, 429, "an inbox can't be flooded");
        assert_eq!(emails(&calls).len(), auth::MAX_FAILURES);
        clock.store(1_000_000 + auth::WINDOW, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(resend(Some(&t)).status, 200, "after 15 minutes");
        // only the latest link works
        assert!(s.handle(&req("GET", &format!("/verify?token={first}"), None, json!({}))).body.contains("expired or was used"));
        let last = verify_token(&calls);
        assert!(s.handle(&req("GET", &format!("/verify?token={last}"), None, json!({}))).body.contains("Your email is confirmed"));
        let n = emails(&calls).len();
        let r = json_of(&resend(Some(&t)));
        assert_eq!((r["sent"].clone(), r["verified"].clone()), (json!(false), json!(true)));
        assert_eq!(emails(&calls).len(), n, "nothing to confirm, nothing sent");
    }

    /// Without email (no Resend key) nobody could confirm, so accounts start confirmed, and one
    /// still waiting from when email was set up is confirmed when it next spends.
    #[test]
    fn without_email_accounts_start_confirmed() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [], "usage": {"input_tokens": 1, "output_tokens": 1}});
        let (s, calls) = service((200, reply.to_string()), true);
        let t = sign_up(&s);
        assert!(s.store().account(1).unwrap().verified, "made confirmed");
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/health", None, json!({}))))["email"], false);
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))))["verified"], true);
        assert_eq!(s.handle(&ai(&t)).status, 200);
        assert!(calls.lock().unwrap().iter().all(|c| c.0 != RESEND_URL));
        // an account made while email was on, the key since removed
        let mut store = Store::in_memory();
        let id = store.create("old@example.com", auth::hash_password("fire truck 1").unwrap(), 100, false, 0).unwrap();
        let (token, hash) = auth::new_token().unwrap();
        store.add_session(hash, id, 0).unwrap();
        let cfg = Config::from_env(|k| (k == "ANTHROPIC_API_KEY").then(|| "sk-ant-service".to_string())).unwrap();
        let s = Service::with_clock(cfg, store, Box::new(Fake { calls: Arc::default(), reply: (200, reply.to_string()) }), Box::new(|| 1_000));
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/account", Some(&token), json!({}))))["verified"], true);
        assert_eq!(s.handle(&ai(&token)).status, 200, "{}", s.handle(&ai(&token)).body);
        assert!(s.store().account(id).unwrap().verified);
    }

    #[test]
    fn form_values_are_decoded() {
        assert_eq!(form_value("a=1&token=sfr_ab&password=a+b%2Bc%25", "password").as_deref(), Some("a b+c%"));
        assert_eq!(form_value("a=1", "token"), None);
        assert_eq!(form_value("p=%", "p").as_deref(), Some("%"), "a stray % stays");
        assert_eq!(form_value("p=%e2%9c%93", "p").as_deref(), Some("\u{2713}"));
        assert_eq!(form_value("p=%ff", "p"), None, "not UTF-8");
        assert_eq!(query_value("/reset?token=sfr_1", "token").as_deref(), Some("sfr_1"));
        assert_eq!(query_value("/reset", "token"), None);
    }

    #[test]
    fn the_web_app_may_call_and_the_account_answers_a_post() {
        let (s, _) = service((200, String::new()), true);
        let pre = s.handle(&req("OPTIONS", "/v1/messages", None, json!({})));
        assert_eq!(pre.status, 204);
        let allow = |r: &Response, k: &str| r.headers.iter().find(|h| h.0 == k).map(|h| h.1.clone());
        assert_eq!(allow(&pre, "Access-Control-Allow-Origin").as_deref(), Some("*"));
        assert!(allow(&pre, "Access-Control-Allow-Headers").unwrap().contains("authorization"));
        let t = sign_up(&s);
        let a = s.handle(&req("POST", "/v1/account", Some(&t), json!({})));
        assert_eq!(a.status, 200);
        assert_eq!(json_of(&a)["credits"], 100);
        assert_eq!(allow(&a, "Access-Control-Allow-Origin").as_deref(), Some("*"), "every answer");
    }

    #[test]
    fn wrong_passwords_are_limited() {
        let (s, _) = service((200, String::new()), false);
        sign_up(&s);
        for _ in 0..auth::MAX_FAILURES {
            assert_eq!(s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": "guess guess"}))).status, 401);
        }
        let r = s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": "fire truck 1"})));
        assert_eq!(r.status, 429, "even the right one waits now");
    }

    #[test]
    fn an_ai_request_goes_out_with_the_services_key_and_is_paid_in_credits() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "Use cut-away."}], "stop_reason": "end_turn", "usage": {"input_tokens": 10_000, "output_tokens": 2_000}});
        let (s, calls) = service((200, reply.to_string()), true);
        let t = sign_up(&s);
        let body = json!({"model": "claude-opus-5-5", "max_tokens": 16000, "messages": [{"role": "user", "content": "Which stabilizer?"}], "metadata": {"x": 1}});
        let mut r = req("POST", "/v1/messages", Some(&t), body);
        // the app's own beta goes on; one it doesn't send (billed differently) is left out
        r.headers.push(("anthropic-beta".into(), "context-1m-2025-08-07, server-side-fallback-2026-07-01".into()));
        let out = s.handle(&r);
        assert_eq!(out.status, 200, "{}", out.body);
        assert_eq!(json_of(&out)["content"][0]["text"], "Use cut-away.", "Anthropic's reply as it came");
        // 24 credits ($0.08 × 3): 100 − 24 = 76 left
        assert!(out.headers.contains(&("x-stitchfoundry-credits".to_string(), "76".to_string())), "{:?}", out.headers);
        let calls = calls.lock().unwrap();
        let (url, headers, sent) = &calls[0];
        assert_eq!(url, ANTHROPIC_URL);
        assert!(headers.contains(&("x-api-key".to_string(), "sk-ant-service".to_string())));
        assert!(headers.contains(&("anthropic-beta".to_string(), "server-side-fallback-2026-07-01".to_string())), "{headers:?}");
        let sent: Value = serde_json::from_str(sent).unwrap();
        assert!(sent.get("metadata").is_none(), "only the request's own keys go on");
        drop(calls);
        let a = json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))));
        assert_eq!(a["credits"], 76);
        assert_eq!(a["history"][0]["credits"], -24);
    }

    /// The app's own tools and the basic web search (two searches at most) are passed on; tools
    /// that run rounds of their own on Anthropic's side are refused, since no hold covers them.
    #[test]
    fn only_tools_a_hold_covers_are_passed_on() {
        let search = |kind: &str, uses: u64| json!([{"type": kind, "name": "web_search", "max_uses": uses}]);
        assert_eq!(tools_refused(&search(sf_core::ai::WEB_SEARCH, 2)), None);
        assert_eq!(tools_refused(&sf_core::ai::ask::tools()), None);
        for bad in [
            search(sf_core::ai::WEB_SEARCH, 3),
            search(sf_core::ai::WEB_SEARCH, 0),
            search("web_search_20260209", 1),
            json!([{"type": "code_execution_20260521", "name": "code_execution"}]),
            json!([{"type": "web_fetch_20260209", "name": "web_fetch"}]),
            json!({"type": "custom"}),
        ] {
            assert!(tools_refused(&bad).is_some(), "{bad}");
        }
    }

    #[test]
    fn requests_are_refused_without_credits_a_session_or_an_allowed_shape() {
        let (s, calls) = service((529, r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#.into()), true);
        let t = sign_up(&s);
        let ok = json!({"model": "claude-opus-5-5", "max_tokens": 16000, "messages": [{"role": "user", "content": "hi"}]});
        assert_eq!(s.handle(&req("POST", "/v1/messages", None, ok.clone())).status, 401);
        let mut wrong = ok.clone();
        wrong["model"] = json!("claude-fable-5-1");
        assert_eq!(s.handle(&req("POST", "/v1/messages", Some(&t), wrong)).status, 400, "a model not offered");
        let mut big = ok.clone();
        big["max_tokens"] = json!(100_000);
        assert_eq!(s.handle(&req("POST", "/v1/messages", Some(&t), big)).status, 400);
        let mut stream = ok.clone();
        stream["stream"] = json!(true);
        assert_eq!(s.handle(&req("POST", "/v1/messages", Some(&t), stream)).status, 400);
        assert!(calls.lock().unwrap().is_empty());
        // a failed request is passed on and costs nothing
        let r = s.handle(&req("POST", "/v1/messages", Some(&t), ok.clone()));
        assert_eq!(r.status, 529);
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))))["credits"], 100);
        // under MIN_TO_START credits left: buy more first
        s.store().settle(1, 0, 90, "test", 0).unwrap();
        let r = s.handle(&req("POST", "/v1/messages", Some(&t), ok.clone()));
        assert_eq!(r.status, 402);
        assert!(json_of(&r)["error"]["message"].as_str().unwrap().contains("buy more"));
    }

    fn check(t: &str, id: &str, photo: &str, share: bool) -> Request {
        let img = json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": photo}});
        let body = json!({"model": "claude-opus-5-5", "max_tokens": 16000, "system": "draw a cat", "messages": [{"role": "user", "content": [img, {"type": "text", "text": "Fabric: Jersey knit"}]}]});
        let mut r = req("POST", "/v1/sewout", Some(t), body);
        r.headers.push(("x-stitchfoundry-check".into(), id.into()));
        if share {
            r.headers.push(("x-stitchfoundry-share".into(), "1".into()));
        }
        r
    }

    /// SO-01: three checks a month free, then a flat price a check; a check's rounds are covered;
    /// only photos make a check, under the service's own instructions; shared checks are kept until
    /// deleted.
    #[test]
    fn sew_out_checks_are_free_three_times_a_month_then_flat() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "Overlap the shapes 1 mm."}], "stop_reason": "end_turn", "usage": {"input_tokens": 10_000, "output_tokens": 2_000}});
        let (s, calls) = service((200, reply.to_string()), true);
        let t = sign_up(&s);
        let credits = |s: &Service| json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))))["credits"].as_u64().unwrap();
        for (i, id) in ["aaaaaaaa", "bbbbbbbb", "cccccccc"].iter().enumerate() {
            let r = s.handle(&check(&t, id, "aGVsbG8=", i == 0));
            assert_eq!(r.status, 200, "{}", r.body);
            assert!(r.headers.contains(&("x-stitchfoundry-free-checks".to_string(), (2 - i).to_string())), "{:?}", r.headers);
        }
        assert_eq!(credits(&s), 100, "the month's first three are free");
        // the instructions are the service's, whatever the app sent
        let sent: Value = serde_json::from_str(&calls.lock().unwrap()[0].2).unwrap();
        assert!(sent["system"].as_str().unwrap().contains("sew-out") && sent["tools"].as_array().unwrap().len() == 2);
        // the fourth costs the flat price, its next rounds nothing more
        assert_eq!(s.handle(&check(&t, "dddddddd", "aGVsbG8=", false)).status, 200);
        assert_eq!(credits(&s), 90);
        assert_eq!(s.handle(&check(&t, "dddddddd", "aGVsbG8=", false)).status, 200);
        assert_eq!(credits(&s), 90, "a check's follow-up is covered");
        let a = json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))));
        assert_eq!(a["freeChecks"], 0);
        assert_eq!(a["sharedChecks"], 1);
        // no photo, no check; no id, no check
        let mut text = check(&t, "eeeeeeee", "aGVsbG8=", false);
        text.body =
            json!({"model": "claude-opus-5-5", "max_tokens": 100, "messages": [{"role": "user", "content": "draw a cat"}]}).to_string().into_bytes();
        assert_eq!(s.handle(&text).status, 400);
        let mut no_id = check(&t, "ffffffff", "aGVsbG8=", false);
        no_id.headers.retain(|h| h.0 != "x-stitchfoundry-check");
        assert_eq!(s.handle(&no_id).status, 400);
        // the shared one kept its photo and answer, until deleted
        let (files, rounds) = s.store().shared("1-aaaaaaaa");
        assert_eq!(files, ["image-1.jpg"]);
        assert!(rounds[0].contains("Overlap the shapes"), "{rounds:?}");
        let d = s.handle(&req("POST", "/v1/sewout/delete", Some(&t), json!({})));
        assert_eq!(json_of(&d)["deleted"], 1);
        assert!(s.store().shared("1-aaaaaaaa").0.is_empty());
    }

    /// Holds each request until `n` are on their way at once (or a few seconds pass), so requests
    /// made at the same time really overlap.
    struct Together {
        n: usize,
        waiting: Arc<(StdMutex<usize>, std::sync::Condvar)>,
        reply: (u16, String),
    }

    impl Upstream for Together {
        fn post(&self, _url: &str, _headers: &[(&str, String)], _body: &str) -> Result<(u16, String), String> {
            let (count, cv) = &*self.waiting;
            let mut c = count.lock().unwrap();
            *c += 1;
            cv.notify_all();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while *c < self.n && std::time::Instant::now() < deadline {
                c = cv.wait_timeout(c, std::time::Duration::from_millis(50)).unwrap().0;
            }
            Ok(self.reply.clone())
        }
    }

    /// Two checks started at the same time when one free check is left: one is free, the other
    /// pays, as when they come one after the other.
    #[test]
    fn checks_at_the_same_time_cant_both_take_the_last_free_one() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "ok"}], "usage": {"input_tokens": 10, "output_tokens": 10}});
        let cfg = Config::from_env(|k| match k {
            "ANTHROPIC_API_KEY" => Some("sk-ant-service".into()),
            "SF_CLOUD_FREE_CHECKS" => Some("1".into()),
            _ => None,
        })
        .unwrap();
        let up = Together { n: 2, waiting: Arc::default(), reply: (200, reply.to_string()) };
        let s = Arc::new(Service::with_clock(cfg, Store::in_memory(), Box::new(up), Box::new(|| 1_000_000)));
        let t = sign_up(&s);
        let runs: Vec<_> = ["aaaaaaaa", "bbbbbbbb"]
            .iter()
            .map(|id| {
                let (s, r) = (s.clone(), check(&t, id, "aGVsbG8=", false));
                std::thread::spawn(move || s.handle(&r).status)
            })
            .collect();
        for r in runs {
            assert_eq!(r.join().unwrap(), 200);
        }
        let a = json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))));
        assert_eq!(a["credits"], 90, "one free check, one paid: {a}");
        assert_eq!(a["freeChecks"], 0);
    }

    /// A free check whose slot was taken but whose request failed gives the slot back.
    #[test]
    fn a_failed_free_check_gives_its_slot_back() {
        let (s, _) = service((529, r#"{"type":"error","error":{"type":"overloaded_error","message":"busy"}}"#.into()), true);
        let t = sign_up(&s);
        assert_eq!(s.handle(&check(&t, "aaaaaaaa", "aGVsbG8=", false)).status, 529);
        assert!(s.store().check("1-aaaaaaaa").is_none());
        assert_eq!(json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))))["freeChecks"], 3);
    }

    /// A free check's rounds after its day are paid in credits, so they don't count against the
    /// month's free budget as well.
    #[test]
    fn a_free_checks_paid_rounds_dont_use_the_free_budget() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "ok"}], "usage": {"input_tokens": 10_000, "output_tokens": 2_000}});
        let cfg = Config::from_env(|k| match k {
            "ANTHROPIC_API_KEY" => Some("sk-ant-service".into()),
            _ => None,
        })
        .unwrap();
        let clock = Arc::new(std::sync::atomic::AtomicU64::new(1_000_000));
        let c = clock.clone();
        let fake = Fake { calls: Arc::default(), reply: (200, reply.to_string()) };
        let s = Service::with_clock(cfg, Store::in_memory(), Box::new(fake), Box::new(move || c.load(std::sync::atomic::Ordering::SeqCst)));
        let t = sign_up(&s);
        let month = sewout::month(1_000_000);
        let before = s.store().free_spent(&month);
        assert_eq!(s.handle(&check(&t, "aaaaaaaa", "aGVsbG8=", false)).status, 200);
        let free_check = s.store().free_spent(&month) - before;
        assert!(free_check > 0);
        // the next day, in the same month: a round of the same check is paid as any request
        clock.store(1_000_000 + sewout::CHECK_LIFE, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(sewout::month(1_000_000 + sewout::CHECK_LIFE), month);
        assert_eq!(s.handle(&check(&t, "aaaaaaaa", "aGVsbG8=", false)).status, 200);
        assert_eq!(json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))))["credits"], 76, "metered: 24 credits");
        assert_eq!(s.store().free_spent(&month) - before, free_check, "not counted as free as well");
    }

    /// Only the betas the apps send are passed to Anthropic.
    #[test]
    fn only_allowed_betas_are_passed_on() {
        assert_eq!(allowed_betas(sf_core::ai::FALLBACK_BETA), sf_core::ai::FALLBACK_BETA);
        assert_eq!(allowed_betas("context-1m-2025-08-07"), "");
        assert_eq!(allowed_betas(" context-1m-2025-08-07 ,server-side-fallback-2026-07-01"), "server-side-fallback-2026-07-01");
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [], "usage": {"input_tokens": 1, "output_tokens": 1}});
        let (s, calls) = service((200, reply.to_string()), true);
        let t = sign_up(&s);
        let mut r = req(
            "POST",
            "/v1/messages",
            Some(&t),
            json!({"model": "claude-opus-5-5", "max_tokens": 100, "messages": [{"role": "user", "content": "hi"}]}),
        );
        r.headers.push(("anthropic-beta".into(), "context-1m-2025-08-07".into()));
        assert_eq!(s.handle(&r).status, 200);
        assert!(!calls.lock().unwrap()[0].1.iter().any(|(k, _)| k == "anthropic-beta"), "no beta header at all");
    }

    #[test]
    fn a_report_is_kept_privately_emailed_to_the_owner_and_deleted_on_request() {
        let calls: Calls = Arc::default();
        let cfg = Config::from_env(|k| match k {
            "RESEND_API_KEY" => Some("re_test".into()),
            "SF_CLOUD_REPORTS_TO" => Some("support@stitchfoundry.app".into()),
            _ => None,
        })
        .unwrap();
        let s =
            Service::with_clock(cfg, Store::in_memory(), Box::new(Fake { calls: calls.clone(), reply: (200, "{}".into()) }), Box::new(|| 1_000_000));
        let t = sign_up(&s);
        let body = json!({"consent": true, "title": "Gaps on knit", "report": "The fill pulled in; StitchFoundry could add more pull compensation on knits.",
            "photos": [{"mediaType": "image/jpeg", "data": "aGVsbG8="}], "app": "0.13.1"});
        // signed in, with leave, and something to say
        assert_eq!(s.handle(&req("POST", "/v1/report", None, body.clone())).status, 401);
        let mut no_leave = body.clone();
        no_leave["consent"] = json!(false);
        assert_eq!(s.handle(&req("POST", "/v1/report", Some(&t), no_leave)).status, 400);
        let r = s.handle(&req("POST", "/v1/report", Some(&t), body.clone()));
        assert_eq!(r.status, 200, "{}", r.body);
        let v = json_of(&r);
        assert_eq!(v["emailed"], true);
        let rid = v["report"].as_str().unwrap().to_string();
        // kept with its photo, and mailed to the owner with it, replies to the sender
        let (files, text) = s.store().shared(&rid);
        assert_eq!(files, ["photo-1.jpg"]);
        assert!(text[0].contains("pull compensation") && text[0].contains("denise@example.com"), "{text:?}");
        let sent = calls.lock().unwrap().clone();
        let (url, headers, mail) = sent.last().unwrap();
        assert_eq!(url, RESEND_URL);
        assert!(headers.iter().any(|(k, v)| k == "authorization" && v == "Bearer re_test"));
        let mail: Value = serde_json::from_str(mail).unwrap();
        assert_eq!(mail["to"][0], "support@stitchfoundry.app");
        assert_eq!(mail["reply_to"], "denise@example.com");
        assert_eq!(mail["attachments"][0]["filename"], "photo-1.jpg");
        assert_eq!(json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))))["reports"], 1);
        // a few a day
        for _ in 1..sewout::MAX_REPORTS_A_DAY {
            assert_eq!(s.handle(&req("POST", "/v1/report", Some(&t), body.clone())).status, 200);
        }
        assert_eq!(s.handle(&req("POST", "/v1/report", Some(&t), body.clone())).status, 429);
        // deleting the shared checks deletes the reports too
        let d = json_of(&s.handle(&req("POST", "/v1/sewout/delete", Some(&t), json!({}))));
        assert_eq!(d["reports"], sewout::MAX_REPORTS_A_DAY);
        assert!(s.store().shared(&rid).0.is_empty());
        assert_eq!(json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))))["reports"], 0);
    }

    #[test]
    fn reporting_a_check_gives_back_what_it_cost_once() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "Overlap the shapes."}], "usage": {"input_tokens": 1000, "output_tokens": 100}});
        let (s, _) = service((200, reply.to_string()), true);
        let t = sign_up(&s);
        let credits = |s: &Service| json_of(&s.handle(&req("POST", "/v1/account", Some(&t), json!({}))))["credits"].as_u64().unwrap();
        // the free checks used up, a check costs the flat price
        for c in ["aaaaaaaa", "bbbbbbbb", "cccccccc"] {
            assert_eq!(s.handle(&check(&t, c, "aGVsbG8=", false)).status, 200);
        }
        assert_eq!(s.handle(&check(&t, "dddddddd", "aGVsbG8=", false)).status, 200);
        assert_eq!(credits(&s), 90);
        let body = json!({"consent": true, "check": "dddddddd", "report": "StitchFoundry could overlap shapes on knit."});
        let r = json_of(&s.handle(&req("POST", "/v1/report", Some(&t), body.clone())));
        assert_eq!(r["refunded"], 10);
        assert_eq!(credits(&s), 100, "the check is on us");
        assert_eq!(json_of(&s.handle(&req("POST", "/v1/report", Some(&t), body)))["refunded"], 0, "once a check");
        assert_eq!(credits(&s), 100);
        // a free check had nothing to give back; someone else's check gives nothing
        let free = json!({"consent": true, "check": "aaaaaaaa", "report": "x"});
        assert_eq!(json_of(&s.handle(&req("POST", "/v1/report", Some(&t), free)))["refunded"], 0);
    }

    #[test]
    fn without_resend_a_report_is_only_kept() {
        let (s, calls) = service((200, "{}".into()), false);
        let t = sign_up(&s);
        let r = s.handle(&req("POST", "/v1/report", Some(&t), json!({"consent": true, "note": "the letters puckered"})));
        assert_eq!(r.status, 200, "{}", r.body);
        assert_eq!(json_of(&r)["emailed"], false);
        assert!(calls.lock().unwrap().is_empty());
    }

    /// The month's free offers stop at the budget: new accounts start without credits, and checks
    /// are paid, until the next month.
    #[test]
    fn free_offers_stop_at_the_months_budget() {
        let reply = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "ok"}], "usage": {"input_tokens": 10_000, "output_tokens": 2_000}});
        // 100 starting credits can buy 34 cents of AI; a check here costs 8
        let (s, _) = service_with((200, reply.to_string()), true, Some(40));
        let t = sign_up(&s);
        let account = |t: &str| json_of(&s.handle(&req("POST", "/v1/account", Some(t), json!({}))));
        assert_eq!(account(&t)["credits"], 100);
        assert_eq!(account(&t)["freeChecks"], 3);
        // the first check is free, and uses up the budget (34 + 8 = 42 cents)
        assert_eq!(s.handle(&check(&t, "aaaaaaaa", "aGVsbG8=", false)).status, 200);
        assert_eq!(account(&t)["credits"], 100);
        assert_eq!(s.store().free_spent(&sewout::month(1_000_000)), 42);
        assert_eq!(account(&t)["freeChecks"], 0, "no more free checks this month");
        let r = s.handle(&check(&t, "bbbbbbbb", "aGVsbG8=", false));
        assert!(r.headers.contains(&("x-stitchfoundry-free-checks".to_string(), "0".to_string())));
        assert_eq!(account(&t)["credits"], 90, "the next check is paid");
        // a new account starts without credits
        let r = s.handle(&req("POST", "/v1/signup", None, json!({"email": "second@example.com", "password": "fire truck 2"})));
        let t2 = json_of(&r)["token"].as_str().unwrap().to_string();
        assert_eq!(account(&t2)["credits"], 0);
    }

    #[test]
    fn a_paid_checkout_adds_its_credits_once() {
        let (s, calls) = service((200, json!({"id": "cs_1", "url": "https://checkout.stripe.com/c/pay/cs_1"}).to_string()), true);
        let t = sign_up(&s);
        let r = s.handle(&req("POST", "/v1/checkout", Some(&t), json!({"pack": "small"})));
        assert_eq!(json_of(&r)["url"], "https://checkout.stripe.com/c/pay/cs_1");
        assert!(calls.lock().unwrap()[0].1.contains(&("authorization".to_string(), "Bearer sk_test_x".to_string())));
        assert_eq!(s.handle(&req("POST", "/v1/checkout", Some(&t), json!({"pack": "huge"}))).status, 400);
        // Stripe's webhook, signed; sent twice
        let event = json!({"id": "evt_1", "type": "checkout.session.completed", "data": {"object": {"id": "cs_1", "payment_status": "paid", "metadata": {"account": "1", "credits": "500", "pack": "small"}}}}).to_string();
        let hook = |sig: String| Request {
            method: "POST".into(),
            path: "/v1/stripe/webhook".into(),
            headers: vec![("Stripe-Signature".into(), sig)],
            body: event.clone().into_bytes(),
        };
        assert_eq!(s.handle(&hook(stripe::sign(event.as_bytes(), "whsec_x", 1_000_000))).status, 200);
        let again = s.handle(&hook(stripe::sign(event.as_bytes(), "whsec_x", 1_000_000)));
        assert_eq!(json_of(&again)["credited"], false);
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))))["credits"], 600);
        // a forged one is refused
        assert_eq!(s.handle(&hook(stripe::sign(event.as_bytes(), "whsec_wrong", 1_000_000))).status, 400);
    }

    #[test]
    fn without_keys_the_service_says_what_isnt_open() {
        let (s, _) = service((200, String::new()), false);
        let t = sign_up(&s);
        assert_eq!(
            json_of(&s.handle(&req("GET", "/v1/health", None, json!({})))),
            json!({"ok": true, "ai": false, "payments": false, "email": false})
        );
        assert_eq!(s.handle(&req("POST", "/v1/messages", Some(&t), json!({}))).status, 503);
        assert_eq!(s.handle(&req("POST", "/v1/checkout", Some(&t), json!({"pack": "small"}))).status, 503);
        assert_eq!(s.handle(&req("GET", "/nope", None, json!({}))).status, 404);
        assert_eq!(s.handle(&req("GET", "/paid", None, json!({}))).content_type, "text/html; charset=utf-8");
    }

    #[test]
    fn the_paid_page_shows_the_packs_credits() {
        let (s, _) = service((200, String::new()), true);
        let page = |path: &str| s.handle(&req("GET", path, None, json!({}))).body;
        assert!(page("/paid?pack=small").contains("Your 1,000 credits ($10.00)"));
        assert!(page("/paid?pack=large&x=1").contains("Your 6,000 credits ($50.00)"));
        // an unknown pack, or none: the general thanks, nothing from the address
        let unknown = page("/paid?pack=%3Cscript%3E");
        assert!(unknown.contains("Your credits are added") && !unknown.contains("script"));
        assert!(page("/paid").contains("Your credits are added"));
        // both return pages say what to do next and link to the site
        for p in [page("/paid?pack=small"), page("/cancelled")] {
            assert!(p.contains("close this tab") && p.contains("href=\"https://stitchfoundry.app\""));
        }
        assert!(page("/paid?pack=small").contains("press Refresh in Account"));
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }
}
