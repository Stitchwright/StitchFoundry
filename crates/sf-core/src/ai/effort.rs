//! How hard the AI works on a request (AI-05), and what that costs.
//!
//! Effort is the Messages API's `output_config.effort`: how long the model thinks before it
//! answers, which is most of what a request costs. Quick asks for `low`, Normal for `medium` (the
//! model's own default) and Thorough for `high`. What a request costs is estimated before it is
//! sent from its size and the effort's typical reply, and read afterwards from the reply's
//! `usage`, at Anthropic's published prices for [`super::MODEL`]
//! (<https://www.anthropic.com/pricing>).

use serde_json::Value;

/// Dollars per million input and output tokens for [`super::MODEL`].
pub const INPUT_PRICE: f64 = 4.0;
pub const OUTPUT_PRICE: f64 = 20.0;
/// Tokens an image sent costs at most (its longest side at [`super::IMAGE_SIDE`] pixels).
const IMAGE_TOKENS: u64 = 3_300;
/// Tokens added for the request's framing.
const OVERHEAD_TOKENS: u64 = 500;

/// How hard the AI works on a request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Effort {
    /// Thinks briefly: cheapest, for simple designs.
    Quick,
    /// The model's own default.
    #[default]
    Normal,
    /// Thinks longest: for designs with much detail, at about twice Normal's cost.
    Thorough,
}

impl Effort {
    pub const ALL: [Effort; 3] = [Effort::Quick, Effort::Normal, Effort::Thorough];

    /// The id kept in preferences and sent in params: `quick`, `normal` or `thorough`.
    pub fn id(self) -> &'static str {
        match self {
            Effort::Quick => "quick",
            Effort::Normal => "normal",
            Effort::Thorough => "thorough",
        }
    }

    pub fn from_id(id: &str) -> Option<Effort> {
        Effort::ALL.into_iter().find(|e| e.id().eq_ignore_ascii_case(id.trim()))
    }

    /// The Messages API's `output_config.effort`.
    pub fn api(self) -> &'static str {
        match self {
            Effort::Quick => "low",
            Effort::Normal => "medium",
            Effort::Thorough => "high",
        }
    }

    /// The output tokens (thinking included) a drawing typically takes at this effort: an estimate,
    /// to be replaced by what the AI test jobs measure (AI-EVAL-01).
    pub fn typical_output(self) -> u64 {
        match self {
            Effort::Quick => 3_500,
            Effort::Normal => 6_000,
            Effort::Thorough => 11_000,
        }
    }
}

/// Tokens a reply used, from its `usage`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

impl Usage {
    /// The `usage` of reply `v` (cached input counted as input); zero when it has none.
    pub fn of_reply(v: &Value) -> Usage {
        let n = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_u64).unwrap_or(0);
        Usage {
            input: n("input_tokens").saturating_add(n("cache_read_input_tokens")).saturating_add(n("cache_creation_input_tokens")),
            output: n("output_tokens"),
        }
    }

    pub fn total(self) -> u64 {
        self.input.saturating_add(self.output)
    }

    /// What it cost at Anthropic's prices (dollars).
    pub fn usd(self) -> f64 {
        (self.input as f64 * INPUT_PRICE + self.output as f64 * OUTPUT_PRICE) / 1e6
    }
}

/// A generous count of the input tokens of request `body`: text at three and a half bytes a
/// token, each image at its most.
pub fn input_tokens(body: &Value) -> u64 {
    fn count(v: &Value) -> u64 {
        match v {
            Value::Object(o) if o.get("type").and_then(Value::as_str) == Some("base64") => IMAGE_TOKENS,
            Value::Object(o) => o.values().map(count).sum(),
            Value::Array(a) => a.iter().map(count).sum(),
            Value::String(s) => (s.len() as u64 * 2).div_ceil(7),
            _ => 0,
        }
    }
    count(body.get("system").unwrap_or(&Value::Null)) + count(body.get("messages").unwrap_or(&Value::Null)) + OVERHEAD_TOKENS
}

/// What request `body` should cost before it is sent (dollars): `typical` with the effort's usual
/// reply, `most` if the reply runs to `max_tokens`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub typical: f64,
    pub most: f64,
}

pub fn estimate(body: &Value, effort: Effort) -> Estimate {
    let input = input_tokens(body);
    let max = body.get("max_tokens").and_then(Value::as_u64).unwrap_or(u64::from(super::MAX_TOKENS));
    let at = |output: u64| Usage { input, output }.usd();
    Estimate { typical: at(effort.typical_output().min(max)), most: at(max) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn effort_ids_and_api_levels() {
        for e in Effort::ALL {
            assert_eq!(Effort::from_id(e.id()), Some(e));
        }
        assert_eq!(Effort::from_id(" Thorough "), Some(Effort::Thorough));
        assert_eq!(Effort::from_id("max"), None);
        assert_eq!(Effort::ALL.map(Effort::api), ["low", "medium", "high"]);
        assert_eq!(Effort::default(), Effort::Normal);
    }

    #[test]
    fn usage_and_estimates_are_in_dollars() {
        let u = Usage::of_reply(&json!({"usage": {"input_tokens": 1000, "cache_read_input_tokens": 500, "output_tokens": 6000}}));
        assert_eq!(u, Usage { input: 1500, output: 6000 });
        assert!((u.usd() - 0.126).abs() < 1e-9, "{}", u.usd());
        assert_eq!(Usage::of_reply(&json!({})), Usage::default());
        let body = json!({"max_tokens": 16000, "system": "x".repeat(3500), "messages": [{"role": "user", "content": "a fox"}]});
        let (quick, thorough) = (estimate(&body, Effort::Quick), estimate(&body, Effort::Thorough));
        assert!(quick.typical < estimate(&body, Effort::Normal).typical && estimate(&body, Effort::Normal).typical < thorough.typical);
        assert_eq!(quick.most, thorough.most, "the most a reply can cost doesn't depend on effort");
        // 16 000 output tokens at $20 a million, plus the input
        assert!(thorough.most > 0.32 && thorough.most < 0.34, "{}", thorough.most);
        // an image counts at its most
        let image = json!({"messages": [{"content": [{"type": "image", "source": {"type": "base64", "data": "AAAA"}}]}]});
        assert!(input_tokens(&image) >= IMAGE_TOKENS);
    }
}
