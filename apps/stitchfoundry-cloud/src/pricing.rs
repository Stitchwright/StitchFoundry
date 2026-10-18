//! What an AI request costs in credits: what Anthropic charges for its tokens, times the markup,
//! in credits of `cents_per_credit` cents, rounded up. Before a request goes out the most it could
//! cost is held ([`estimate`]); once the reply is in, only what it used is taken ([`charge`]).

use serde_json::Value;

use crate::config::{Config, ModelPrice};

/// Tokens an image costs at most (the longest side sent is 1568 px).
const IMAGE_TOKENS: u64 = 3_300;
/// Tokens added for the request's framing, tools and the like.
const OVERHEAD_TOKENS: u64 = 2_000;

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
    credits(cfg, (input as f64 * per_input + max_tokens as f64 * price.output) / 1e6)
}

/// What a reply's `usage` cost (credits).
pub fn charge(cfg: &Config, price: &ModelPrice, usage: &Value) -> u64 {
    let n = |k: &str| usage.get(k).and_then(Value::as_u64).unwrap_or(0) as f64;
    let usd = (n("input_tokens") * price.input
        + n("output_tokens") * price.output
        + n("cache_read_input_tokens") * price.cache_read
        + n("cache_creation_input_tokens") * price.cache_write)
        / 1e6;
    credits(cfg, usd)
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
        // 10,000 in at $4/M, 2,000 out at $20/M = $0.08; × 1.5 = 12 cents = 12 credits
        assert_eq!(charge(&c, &p, &json!({"input_tokens": 10_000, "output_tokens": 2_000})), 12);
        // a tiny reply still costs a credit; nothing used costs nothing
        assert_eq!(charge(&c, &p, &json!({"input_tokens": 10, "output_tokens": 1})), 1);
        assert_eq!(charge(&c, &p, &json!({})), 0);
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
        assert!(estimate(&c, &p, &img, 16_000) < 60, "{}", estimate(&c, &p, &img, 16_000));
    }
}
