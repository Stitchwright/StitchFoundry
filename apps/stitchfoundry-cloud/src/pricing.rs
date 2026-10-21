//! What an AI request costs in credits: what Anthropic charges for its tokens, times the markup,
//! in credits of `cents_per_credit` cents, rounded up. Before a request goes out the most it could
//! cost is held ([`estimate`]); once the reply is in, only what it used is taken ([`charge`]).

use serde_json::Value;

use crate::config::{Config, ModelPrice};

/// Tokens an image costs at most (the longest side sent is 1568 px).
const IMAGE_TOKENS: u64 = 3_300;
/// Tokens added for the request's framing, tools and the like.
const OVERHEAD_TOKENS: u64 = 2_000;
/// What one web search costs (dollars: Anthropic's $10 a thousand), on top of its tokens.
const SEARCH_USD: f64 = 0.01;

/// Credits for `usd` dollars of AI use.
fn credits(cfg: &Config, usd: f64) -> u64 {
    let cents = usd * 100.0 * cfg.markup;
    let c = (cents / cfg.cents_per_credit).ceil();
    if c.is_finite() && c > 0.0 { (c as u64).max(1) } else { 0 }
}

/// A generous count of the input tokens of request body `v`: text at three bytes a token (real
/// text runs nearer four), each image at its most.
pub fn input_tokens(v: &Value) -> u64 {
    match v {
        Value::Object(o) => {
            if o.get("type").and_then(Value::as_str) == Some("base64") {
                return IMAGE_TOKENS;
            }
            o.values().map(input_tokens).sum()
        }
        Value::Array(a) => a.iter().map(input_tokens).sum(),
        Value::String(s) => (s.len() as u64).div_ceil(3),
        _ => 1,
    }
}

/// The most request `body` can cost (credits): all its input, and all the reply it may write.
pub fn estimate(cfg: &Config, price: &ModelPrice, body: &Value, max_tokens: u64) -> u64 {
    let input = input_tokens(body.get("messages").unwrap_or(&Value::Null))
        + input_tokens(body.get("system").unwrap_or(&Value::Null))
        + input_tokens(body.get("tools").unwrap_or(&Value::Null))
        + OVERHEAD_TOKENS;
    // the dearest way the input could be billed
    let per_input = price.input.max(price.cache_write);
    // every search a web search tool may make
    let searches: u64 = body.get("tools").and_then(Value::as_array).map_or(0, |t| {
        t.iter().filter(|t| t["type"].as_str().is_some_and(|k| k.starts_with("web_search"))).map(|t| t["max_uses"].as_u64().unwrap_or(10)).sum()
    });
    credits(cfg, (input as f64 * per_input + max_tokens as f64 * price.output) / 1e6 + searches as f64 * SEARCH_USD)
}

/// What a reply's `usage` cost the service (cents, rounded up, without the markup).
pub fn cost_cents(price: &ModelPrice, usage: &Value) -> u64 {
    let c = (usd(price, usage) * 100.0).ceil();
    if c.is_finite() && c > 0.0 { c as u64 } else { 0 }
}

/// The most `credits` given away can cost the service in AI (cents, rounded up).
pub fn grant_cost_cents(cfg: &Config, credits: u64) -> u64 {
    let c = (credits as f64 * cfg.cents_per_credit / cfg.markup).ceil();
    if c.is_finite() && c > 0.0 { c as u64 } else { 0 }
}

/// What a reply's `usage` cost Anthropic's way (dollars).
fn usd(price: &ModelPrice, usage: &Value) -> f64 {
    let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0) as f64;
    (n("input_tokens") * price.input
        + n("output_tokens") * price.output
        + n("cache_read_input_tokens") * price.cache_read
        + n("cache_creation_input_tokens") * price.cache_write)
        / 1e6
        + usage.pointer("/server_tool_use/web_search_requests").and_then(Value::as_u64).unwrap_or(0) as f64 * SEARCH_USD
}

/// What a reply's `usage` cost (credits).
pub fn charge(cfg: &Config, price: &ModelPrice, usage: &Value) -> u64 {
    credits(cfg, usd(price, usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cfg() -> Config {
        Config::from_env(|_| None).unwrap()
    }

    #[test]
    fn a_reply_costs_what_it_used_with_the_markup() {
        let c = cfg();
        let p = c.model("claude-opus-5-5").unwrap().clone();
        // 10,000 in at $4/M, 2,000 out at $20/M = $0.08; × 3 = 24 cents = 24 credits
        assert_eq!(charge(&c, &p, &json!({"input_tokens": 10_000, "output_tokens": 2_000})), 24);
        // a tiny reply still costs a credit; nothing used costs nothing
        assert_eq!(charge(&c, &p, &json!({"input_tokens": 10, "output_tokens": 1})), 1);
        assert_eq!(charge(&c, &p, &json!({})), 0);
        // web searches cost on top of their tokens, and are held for before the request goes
        let searched = json!({"input_tokens": 10_000, "output_tokens": 2_000, "server_tool_use": {"web_search_requests": 3}});
        assert!(charge(&c, &p, &searched) > charge(&c, &p, &json!({"input_tokens": 10_000, "output_tokens": 2_000})));
        let body = json!({"messages": [{"role": "user", "content": "a truck"}]});
        let mut with_search = body.clone();
        with_search["tools"] = json!([{"type": "web_search_20260209", "name": "web_search", "max_uses": 3}]);
        assert!(estimate(&c, &p, &with_search, 16_000) > estimate(&c, &p, &body, 16_000));
    }

    #[test]
    fn what_free_offers_cost_the_service() {
        let c = cfg();
        let p = c.model("claude-opus-5-5").unwrap().clone();
        // $0.08 of AI is 8 cents, without the markup
        assert_eq!(cost_cents(&p, &json!({"input_tokens": 10_000, "output_tokens": 2_000})), 8);
        assert_eq!(cost_cents(&p, &json!({})), 0);
        // 100 credits at 1 cent and a markup of 3 buy at most 34 cents of AI
        assert_eq!(grant_cost_cents(&c, 100), 34);
        assert_eq!(grant_cost_cents(&c, 0), 0);
    }

    #[test]
    fn the_hold_covers_the_most_a_request_could_cost() {
        let c = cfg();
        let p = c.model("claude-opus-5-5").unwrap().clone();
        let body = json!({"messages": [{"role": "user", "content": "x".repeat(3000)}]});
        let held = estimate(&c, &p, &body, 16_000);
        let used = charge(&c, &p, &json!({"input_tokens": 1000 + 2000, "output_tokens": 16_000}));
        assert!(held >= used, "{held} >= {used}");
        // an image is counted as one image, not as its base64 bytes
        let img = json!({"messages": [{"role": "user", "content": [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "A".repeat(4_000_000)}}]}]});
        assert!(estimate(&c, &p, &img, 16_000) < 130, "{}", estimate(&c, &p, &img, 16_000));
    }
}
