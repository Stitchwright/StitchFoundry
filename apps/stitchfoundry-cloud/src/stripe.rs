//! Buying credits through Stripe Checkout: the service asks Stripe for a checkout page for a pack
//! ([`checkout_form`]), the buyer pays there, and Stripe tells the service with a signed webhook
//! ([`verify`], [`paid`]), which adds the credits once. Card details never reach the service.
//! Public descriptions: https://docs.stripe.com/api/checkout/sessions/create and
//! https://docs.stripe.com/webhooks#verify-manually

use hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::config::{Config, Pack};

/// Stripe's endpoint for checkout sessions.
pub const CHECKOUT_URL: &str = "https://api.stripe.com/v1/checkout/sessions";
/// How old a webhook's timestamp may be (seconds).
pub const TOLERANCE: u64 = 300;

/// `s` as a form value.
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The form that asks Stripe for a checkout page selling `pack` to `account` (`email`).
pub fn checkout_form(cfg: &Config, pack: &Pack, account: u64, email: &str) -> String {
    let fields = [
        ("mode", "payment".to_string()),
        ("success_url", format!("{}/paid?pack={}", cfg.public_url, pack.id)),
        ("cancel_url", format!("{}/cancelled", cfg.public_url)),
        ("client_reference_id", account.to_string()),
        ("customer_email", email.to_string()),
        ("line_items[0][quantity]", "1".to_string()),
        ("line_items[0][price_data][currency]", "usd".to_string()),
        ("line_items[0][price_data][unit_amount]", pack.cents.to_string()),
        ("line_items[0][price_data][product_data][name]", format!("Foundry credits: {}", pack.credits)),
        ("metadata[account]", account.to_string()),
        ("metadata[credits]", pack.credits.to_string()),
        ("metadata[pack]", pack.id.clone()),
    ];
    fields.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&")
}

/// Checks a webhook's `Stripe-Signature` header (`t=…,v1=…`) against its raw `payload` with the
/// webhook's signing `secret`, and that it was signed within [`TOLERANCE`] of `now`.
pub fn verify(payload: &[u8], header: &str, secret: &str, now: u64) -> Result<(), String> {
    let mut t = None;
    let mut sigs = Vec::new();
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", v)) => t = v.parse::<u64>().ok(),
            Some(("v1", v)) => sigs.push(v.to_string()),
            _ => {}
        }
    }
    let t = t.ok_or("the signature has no timestamp")?;
    if now.abs_diff(t) > TOLERANCE {
        return Err("the signature is too old".into());
    }
    for sig in sigs {
        let Some(bytes) = unhex(&sig) else { continue };
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|e| e.to_string())?;
        mac.update(t.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload);
        // (compared in constant time)
        if mac.verify_slice(&bytes).is_ok() {
            return Ok(());
        }
    }
    Err("the signature doesn't match".into())
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok())).collect()
}

/// A paid checkout: who paid, how many credits, and the keys it is credited under once.
#[derive(Clone, Debug, PartialEq)]
pub struct Paid {
    pub event: String,
    pub session: String,
    pub account: u64,
    pub credits: u64,
    pub pack: String,
}

/// The paid checkout a verified webhook `event` reports, if it reports one: a
/// `checkout.session.completed` (or its `async_payment_succeeded`) whose payment is in.
pub fn paid(event: &Value) -> Option<Paid> {
    let kind = event.get("type")?.as_str()?;
    if kind != "checkout.session.completed" && kind != "checkout.session.async_payment_succeeded" {
        return None;
    }
    let s = event.pointer("/data/object")?;
    if s.get("payment_status")?.as_str()? != "paid" {
        return None;
    }
    let meta = s.get("metadata")?;
    Some(Paid {
        event: event.get("id")?.as_str()?.to_string(),
        session: s.get("id")?.as_str()?.to_string(),
        account: meta.get("account")?.as_str()?.parse().ok()?,
        credits: meta.get("credits")?.as_str()?.parse().ok()?,
        pack: meta.get("pack").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
pub fn sign(payload: &[u8], secret: &str, t: u64) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{t}.").as_bytes());
    mac.update(payload);
    let sig: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("t={t},v1={sig}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn webhooks_are_taken_only_when_stripe_signed_them_lately() {
        let body = br#"{"id":"evt_1"}"#;
        let h = sign(body, "whsec_test", 1000);
        assert_eq!(verify(body, &h, "whsec_test", 1100), Ok(()));
        assert!(verify(body, &h, "whsec_other", 1100).is_err(), "another secret");
        assert!(verify(br#"{"id":"evt_2"}"#, &h, "whsec_test", 1100).is_err(), "another body");
        assert!(verify(body, &h, "whsec_test", 1000 + TOLERANCE + 1).is_err(), "too old");
        assert!(verify(body, "v1=00", "whsec_test", 1000).is_err() && verify(body, "", "whsec_test", 1000).is_err());
    }

    #[test]
    fn a_paid_checkout_names_its_account_and_credits() {
        let ev = json!({"id": "evt_1", "type": "checkout.session.completed", "data": {"object": {
            "id": "cs_1", "payment_status": "paid", "metadata": {"account": "7", "credits": "500", "pack": "small"}}}});
        assert_eq!(paid(&ev), Some(Paid { event: "evt_1".into(), session: "cs_1".into(), account: 7, credits: 500, pack: "small".into() }));
        let mut unpaid = ev.clone();
        unpaid["data"]["object"]["payment_status"] = json!("unpaid");
        assert_eq!(paid(&unpaid), None, "a bank payment still on its way");
        assert_eq!(paid(&json!({"id": "evt_2", "type": "charge.refunded"})), None);
    }

    #[test]
    fn the_checkout_form_sells_the_pack_to_the_account() {
        let cfg = Config::from_env(|k| (k == "SF_CLOUD_PUBLIC_URL").then(|| "https://cloud.example".to_string())).unwrap();
        let f = checkout_form(&cfg, &cfg.packs[0], 7, "denise+sew@example.com");
        assert!(f.contains("mode=payment&") && f.contains("client_reference_id=7"));
        assert!(f.contains("customer_email=denise%2Bsew%40example.com"));
        assert!(f.contains("line_items%5B0%5D%5Bprice_data%5D%5Bunit_amount%5D=1000"));
        assert!(f.contains("metadata%5Bcredits%5D=1000") && f.contains("success_url=https%3A%2F%2Fcloud.example%2Fpaid"));
    }
}
