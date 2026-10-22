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
/// Input tokens the search results of a researched drawing typically add (the AI test jobs of
/// 2026-10-22: 10 000 to 25 000), and what each search costs on top (dollars: $10 a thousand).
const RESEARCH_TOKENS: u64 = 15_000;
pub const SEARCH_USD: f64 = 0.01;
/// Searches a researched drawing typically makes.
const TYPICAL_SEARCHES: u64 = 1;
/// The most input tokens one search's results are taken to add. Not a limit the API keeps: a
/// generous figure, so that what is held before a request (the hosted service's credits) covers it.
pub const SEARCH_RESULT_TOKENS: u64 = 40_000;

/// The most input tokens a request of `base` input tokens is billed when it may search `searches`
/// times and write `max_output` tokens. Each search is a round on Anthropic's side that reads the
/// whole conversation again: the request, the results so far and what was written so far.
pub fn research_input_most(base: u64, searches: u64, max_output: u64) -> u64 {
    let rounds = searches.saturating_add(1);
    let results = SEARCH_RESULT_TOKENS.saturating_mul(searches).saturating_mul(rounds) / 2;
    base.saturating_mul(rounds).saturating_add(results).saturating_add(max_output.saturating_mul(searches))
}

/// How hard the AI works on a request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Effort {
    /// Thinks briefly: cheapest, for simple designs.
    Quick,
    /// The model's own default.
    #[default]
    Normal,
    /// Thinks longest, looks up what it draws ([`Effort::researches`]) and checks its drawing
    /// once more as it stitches ([`Effort::checks`]): for real things and detailed designs.
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

    /// Whether the model may search the web for what it draws before drawing (AI-04).
    pub fn researches(self) -> bool {
        self == Effort::Thorough
    }

    /// Whether the drawing goes back to the model as it stitches, to be checked and corrected
    /// once (AI-04).
    pub fn checks(self) -> bool {
        self == Effort::Thorough
    }

    /// The output tokens (thinking included) a drawing typically takes at this effort, rounded up
    /// from what the AI test jobs measured on 2026-10-22 (AI-EVAL-01): Quick 977 on average,
    /// Normal 2 818, Thorough 6 300 to 17 800 for the drawing and its check together (the check's
    /// share is counted in [`estimate`]).
    pub fn typical_output(self) -> u64 {
        match self {
            Effort::Quick => 1_200,
            Effort::Normal => 3_200,
            Effort::Thorough => 8_000,
        }
    }
}

/// Tokens a reply used, and the web searches it made, from its `usage`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub searches: u64,
}

impl std::ops::Add for Usage {
    type Output = Usage;
    fn add(self, o: Usage) -> Usage {
        Usage {
            input: self.input.saturating_add(o.input),
            output: self.output.saturating_add(o.output),
            searches: self.searches.saturating_add(o.searches),
        }
    }
}

impl Usage {
    /// The `usage` of reply `v` (cached input counted as input); zero when it has none.
    pub fn of_reply(v: &Value) -> Usage {
        let n = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_u64).unwrap_or(0);
        Usage {
            input: n("input_tokens").saturating_add(n("cache_read_input_tokens")).saturating_add(n("cache_creation_input_tokens")),
            output: n("output_tokens"),
            searches: v.pointer("/usage/server_tool_use/web_search_requests").and_then(Value::as_u64).unwrap_or(0),
        }
    }

    pub fn total(self) -> u64 {
        self.input.saturating_add(self.output)
    }

    /// What it cost at Anthropic's prices (dollars), searches included.
    pub fn usd(self) -> f64 {
        (self.input as f64 * INPUT_PRICE + self.output as f64 * OUTPUT_PRICE) / 1e6 + self.searches as f64 * SEARCH_USD
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
    let searches = body.pointer("/tools/0/max_uses").and_then(Value::as_u64).unwrap_or(0);
    let output = effort.typical_output().min(max);
    let typical_searches = TYPICAL_SEARCHES.min(searches);
    let mut typical = Usage { input: input + if searches > 0 { RESEARCH_TOKENS } else { 0 }, output, searches: typical_searches }.usd();
    let mut most = Usage { input: research_input_most(input, searches, max), output: max, searches }.usd();
    if effort.checks() {
        // the check, a conversation of its own: the request and the drawing again with the
        // stitched picture, and a reply that is a word or a corrected drawing
        let svg = max.min(output);
        typical += Usage { input: input + svg + IMAGE_TOKENS, output: output / 2, searches: 0 }.usd();
        most += Usage { input: input + max + 2 * IMAGE_TOKENS, output: max, searches: 0 }.usd();
    }
    Estimate { typical, most }
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
        assert_eq!(u, Usage { input: 1500, output: 6000, searches: 0 });
        assert!((u.usd() - 0.126).abs() < 1e-9, "{}", u.usd());
        assert_eq!(Usage::of_reply(&json!({})), Usage::default());
        // the searches a reply made are paid for too, and two replies add up
        let searched = Usage::of_reply(&json!({"usage": {"input_tokens": 1000, "output_tokens": 0, "server_tool_use": {"web_search_requests": 2}}}));
        assert_eq!(searched.searches, 2);
        assert!((searched.usd() - (0.004 + 2.0 * SEARCH_USD)).abs() < 1e-9, "{}", searched.usd());
        assert_eq!((u + searched).total(), u.total() + searched.total());
        assert_eq!((u + searched).searches, 2);
        let body = json!({"max_tokens": 16000, "system": "x".repeat(3500), "messages": [{"role": "user", "content": "a fox"}]});
        let (quick, thorough) = (estimate(&body, Effort::Quick), estimate(&body, Effort::Thorough));
        assert!(quick.typical < estimate(&body, Effort::Normal).typical && estimate(&body, Effort::Normal).typical < thorough.typical);
        assert!(thorough.most > quick.most, "Thorough's check is a second reply");
        // 16 000 output tokens at $20 a million, plus the input
        assert!(quick.most > 0.32 && quick.most < 0.34, "{}", quick.most);
        // searches add their results and their fee
        let mut searched = body.clone();
        searched["tools"] = json!([{"type": "web_search_20250305", "name": "web_search", "max_uses": 2}]);
        let researched = estimate(&searched, Effort::Thorough);
        assert!(researched.typical > thorough.typical + 0.05);
        // at most: every round reads everything again (the 2026-10-22 jobs' worst ran to $4.30
        // with ten rounds; two searches are three rounds)
        assert!(researched.most > thorough.most + 0.4 && researched.most < 1.5, "{}", researched.most);
        assert_eq!(research_input_most(1000, 0, 16_000), 1000);
        assert_eq!(research_input_most(1000, 2, 16_000), 3000 + 120_000 + 32_000);
        // an image counts at its most
        let image = json!({"messages": [{"content": [{"type": "image", "source": {"type": "base64", "data": "AAAA"}}]}]});
        assert!(input_tokens(&image) >= IMAGE_TOKENS);
    }
}
