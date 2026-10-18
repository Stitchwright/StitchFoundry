//! The service's endpoints, independent of the HTTP server that carries them ([`Service::handle`]
//! takes a request and gives a response), so they are tested without a network. Errors come back
//! in the Anthropic API's shape (`{"type": "error", "error": {"type", "message"}}`), so the app
//! reads them as it reads Anthropic's own.
//!
//! - `GET  /v1/health`: whether AI requests and payments are set up.
//! - `POST /v1/signup {email, password}`, `POST /v1/signin {email, password}` → `{token, account}`.
//! - `POST /v1/signout` (with the token).
//! - `GET  /v1/account` → `{email, credits, packs, history}`.
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
/// The Anthropic Messages API.
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
/// The keys of a Messages request passed on; any others are left out.
const PASSED: &[&str] = &["model", "max_tokens", "system", "messages", "tools", "tool_choice", "output_config", "fallbacks", "thinking"];

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

    fn page(text: &str) -> Response {
        let body = format!(
            "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>StitchFoundry</title><body style=\"font-family:sans-serif;max-width:32em;margin:4em auto;padding:0 1em\"><h1>StitchFoundry</h1><p>{text}</p></body>"
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

fn store_error(e: StoreError) -> Response {
    match e {
        StoreError::Taken => Response::error(409, "invalid_request_error", "an account with that email already exists: sign in instead"),
        StoreError::NoAccount => Response::error(401, "authentication_error", "sign in again"),
        StoreError::NotEnough => Response::error(402, "billing_error", "not enough StitchFoundry credits: buy more in Account"),
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
        Service { cfg, store: Mutex::new(store), throttle: Mutex::default(), in_flight: Mutex::default(), upstream, clock }
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
                &json!({ "ok": true, "ai": self.cfg.anthropic_key.is_some(), "payments": self.cfg.stripe_key.is_some() && self.cfg.stripe_webhook_secret.is_some() }),
            ),
            ("POST", "/v1/signup") => self.signup(req),
            ("POST", "/v1/signin") => self.signin(req),
            ("POST", "/v1/signout") => self.signout(req),
            // (POST too: the app's client only posts)
            ("GET" | "POST", "/v1/account") => self.account(req),
            ("POST", "/v1/checkout") => self.checkout(req),
            ("POST", "/v1/stripe/webhook") => self.webhook(req),
            ("POST", "/v1/messages") => self.messages(req),
            ("POST", "/v1/sewout") => self.sewout(req),
            ("POST", "/v1/sewout/delete") => self.delete_shared(req),
            ("GET", "/paid") => Response::page(
                "Thank you. Your credits are added as soon as the payment is through, usually within a minute: go back to StitchFoundry.",
            ),
            ("GET", "/cancelled") => Response::page("The payment was cancelled; nothing was charged. You can close this page."),
            _ => Response::error(404, "not_found_error", "no such endpoint"),
        }
    }

    /// The account the request's token stands for.
    fn signed_in(&self, req: &Request) -> Result<u64, Response> {
        let token = req.header("authorization").and_then(|h| h.strip_prefix("Bearer ")).map(str::trim).filter(|t| t.starts_with("sfs_"));
        let Some(token) = token else { return Err(Response::error(401, "authentication_error", "sign in to your StitchFoundry account")) };
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
            "email": a.map(|a| a.email.as_str()),
            "credits": a.map_or(0, |a| a.credits.saturating_sub(a.reserved)),
            "freeChecks": free_checks,
            "checkCredits": self.cfg.check_credits,
            "sharedChecks": store.shared_checks(id).len(),
        })
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
        let id = {
            let mut store = self.store();
            // the starting credits, while the month's free offers can afford them
            let cost = pricing::grant_cost_cents(&self.cfg, self.cfg.signup_credits);
            let credits = if store.free_spent(&month).saturating_add(cost) <= self.cfg.free_budget_cents { self.cfg.signup_credits } else { 0 };
            let id = match store.create(&email, hash, credits, now) {
                Ok(id) => id,
                Err(e) => return store_error(e),
            };
            if credits > 0 {
                if let Err(e) = store.add_free_spent(&month, cost) {
                    log::error!("free budget: {e}");
                }
            }
            id
        };
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

    fn checkout(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
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
        let body: serde_json::Map<String, Value> =
            o.iter().filter(|(k, _)| PASSED.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        Ok((Value::Object(body), max_tokens))
    }

    fn messages(&self, req: &Request) -> Response {
        let id = match self.signed_in(req) {
            Ok(id) => id,
            Err(r) => return r,
        };
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
        let (charge, free) = {
            let store = self.store();
            match store.check(&check).filter(|c| c.account == id) {
                // its rounds are covered by what the check cost
                Some(c) if now.saturating_sub(c.created) < sewout::CHECK_LIFE && c.requests < sewout::MAX_ROUNDS => (Charge::Flat(0), c.free),
                // past the check: as any request
                Some(c) => (Charge::Metered, c.free),
                None => {
                    let free = (store.checks_in(id, &month) as u64) < self.cfg.free_checks && self.free_left(&store, &month);
                    (Charge::Flat(if free { 0 } else { self.cfg.check_credits }), free)
                }
            }
        };
        let mut resp = self.forward(req, id, key, &body, max_tokens, charge, if free { "sew-out check (free)" } else { "sew-out check" });
        if (200..300).contains(&resp.status) {
            let mut store = self.store();
            if free {
                // a free check's rounds count against the month's free offers at what they cost
                let cost = serde_json::from_str::<Value>(&resp.body).ok().map_or(0, |r| {
                    let price = r.get("model").and_then(Value::as_str).and_then(|m| self.cfg.model(m)).cloned().unwrap_or_else(|| self.cfg.dearest());
                    pricing::cost_cents(&price, r.get("usage").unwrap_or(&Value::Null))
                });
                if let Err(e) = store.add_free_spent(&month, cost) {
                    log::error!("free budget: {e}");
                }
            }
            let noted = Check { account: id, month: month.clone(), created: now, requests: 0, free, shared: share, follow_up: follow_up.clone() };
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
        match store.delete_shared(&ids) {
            Ok(n) => Response::json(200, &json!({ "deleted": n })),
            Err(e) => store_error(e),
        }
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
            ("anthropic-beta", req.header("anthropic-beta").unwrap_or_default().to_string()),
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
        assert_eq!(json_of(&a)["credits"], 100, "the starting credits");
        assert_eq!(json_of(&a)["history"][0]["kind"], "signup");
        // the same email again is refused; a short password too
        assert_eq!(s.handle(&req("POST", "/v1/signup", None, json!({"email": "denise@example.com", "password": "another one"}))).status, 409);
        assert_eq!(s.handle(&req("POST", "/v1/signup", None, json!({"email": "x@y.z", "password": "short"}))).status, 400);
        // signing in gives a new token; a wrong password doesn't
        let r = s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": "fire truck 1"})));
        assert_eq!(r.status, 200);
        assert_eq!(s.handle(&req("POST", "/v1/signin", None, json!({"email": "denise@example.com", "password": "wrong"}))).status, 401);
        assert_eq!(s.handle(&req("POST", "/v1/signin", None, json!({"email": "nobody@example.com", "password": "fire truck 1"}))).status, 401);
        // signing out ends the token
        s.handle(&req("POST", "/v1/signout", Some(&t), json!({})));
        assert_eq!(s.handle(&req("GET", "/v1/account", Some(&t), json!({}))).status, 401);
        assert_eq!(s.handle(&req("GET", "/v1/account", Some("sfs_made_up"), json!({}))).status, 401);
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
        r.headers.push(("anthropic-beta".into(), "server-side-fallback-2026-07-01".into()));
        let out = s.handle(&r);
        assert_eq!(out.status, 200, "{}", out.body);
        assert_eq!(json_of(&out)["content"][0]["text"], "Use cut-away.", "Anthropic's reply as it came");
        // 24 credits ($0.08 × 3): 100 − 24 = 76 left
        assert!(out.headers.contains(&("x-stitchfoundry-credits".to_string(), "76".to_string())), "{:?}", out.headers);
        let calls = calls.lock().unwrap();
        let (url, headers, sent) = &calls[0];
        assert_eq!(url, ANTHROPIC_URL);
        assert!(headers.contains(&("x-api-key".to_string(), "sk-ant-service".to_string())));
        assert!(headers.contains(&("anthropic-beta".to_string(), "server-side-fallback-2026-07-01".to_string())));
        let sent: Value = serde_json::from_str(sent).unwrap();
        assert!(sent.get("metadata").is_none(), "only the request's own keys go on");
        drop(calls);
        let a = json_of(&s.handle(&req("GET", "/v1/account", Some(&t), json!({}))));
        assert_eq!(a["credits"], 76);
        assert_eq!(a["history"][0]["credits"], -24);
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
        assert_eq!(json_of(&s.handle(&req("GET", "/v1/health", None, json!({})))), json!({"ok": true, "ai": false, "payments": false}));
        assert_eq!(s.handle(&req("POST", "/v1/messages", Some(&t), json!({}))).status, 503);
        assert_eq!(s.handle(&req("POST", "/v1/checkout", Some(&t), json!({"pack": "small"}))).status, 503);
        assert_eq!(s.handle(&req("GET", "/nope", None, json!({}))).status, 404);
        assert_eq!(s.handle(&req("GET", "/paid", None, json!({}))).content_type, "text/html; charset=utf-8");
    }
}
