//! Design from Prompt and Redraw Image: what StitchFoundry asks an AI model for, and how its reply
//! becomes artwork.
//!
//! The model is Claude, reached through the Messages API with the user's own API key. No network
//! code lives here: the app sends the [`HttpRequest`] that [`ArtRequest::http`] builds with its
//! own client and hands the answer to [`parse_reply`].
//!
//! The art asked for is what digitizes well: filled shapes in a few flat colours, no part narrower
//! than a satin column, stacked back to front (the order they are sewn). It is drawn from a
//! description, or redrawn from an image ([`ArtRequest::image`]: a logo the user may use). The art
//! is placed and given stitches by the engine's `embroidery.placeArt`.

use serde_json::{Value, json};

pub use effort::{Effort, Usage};

pub mod ask;
pub mod cloud;
pub mod effort;
pub mod makerfoundry;
pub mod picture;
pub mod sewout;

/// The Messages API endpoint.
pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
/// The API version the request is written against.
pub const API_VERSION: &str = "2023-06-01";
/// The model that draws.
pub const MODEL: &str = "claude-opus-5-5";
/// The beta that lets a request the model declines run again on the model Anthropic recommends
/// (`"fallbacks": "default"`).
pub const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// The web search tool a Thorough drawing may use to look up what it draws, and how many
/// searches it may make.
pub const WEB_SEARCH: &str = "web_search_20260209";
pub const MAX_SEARCHES: u32 = 3;
/// The most tokens the reply (thinking included) may use.
pub const MAX_TOKENS: u32 = 16_000;
/// The longest description sent, in characters.
pub const MAX_PROMPT: usize = 2_000;
/// The largest artwork accepted, in bytes.
pub const MAX_SVG: usize = 1_000_000;
/// The narrowest part a stitched shape should have, in millimetres.
pub const MIN_FEATURE_MM: f64 = 1.5;
/// Sizes (mm, longest side) and thread colours a request may ask for.
pub const SIZE_RANGE: (f64, f64) = (20.0, 400.0);
pub const COLOR_RANGE: (u32, u32) = (1, 12);
/// The longest side, in pixels, an image to redraw is sent at (the model sees no more detail).
pub const IMAGE_SIDE: u32 = 1568;
/// The largest image sent, as base64 (the API takes up to 5 MB an image).
pub const MAX_IMAGE_BASE64: usize = 5_000_000;

/// Artwork to ask for.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtRequest {
    /// What to draw, in the user's words.
    pub prompt: String,
    /// The stitched size of the longest side, in millimetres.
    pub size_mm: f64,
    /// The most thread colours the art may use.
    pub colors: u32,
    /// An image to redraw (then `prompt` holds optional notes on what to change).
    pub image: Option<ArtImage>,
    /// How hard the model works on it.
    pub effort: Effort,
}

/// An image to redraw: PNG or JPEG bytes in base64.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtImage {
    /// `image/png` or `image/jpeg`.
    pub media_type: &'static str,
    pub data_base64: String,
}

/// An HTTP POST for the app's client to send.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// Artwork the model drew.
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    /// The SVG document.
    pub svg: String,
    /// The model that drew it (another one when the request fell back).
    pub model: String,
    /// The tokens it took.
    pub usage: Usage,
    /// The reply's content blocks as they came (thinking and searches included), sent back
    /// unchanged with the check ([`ArtRequest::check_http`]).
    pub content: Value,
}

/// What the check of a drawing said: it is right, or here is a better one.
#[derive(Clone, Debug, PartialEq)]
pub enum Checked {
    Good,
    Redrawn(Art),
}

/// Why no artwork came back. The messages are for the user; the UI shows them translated.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ArtError {
    #[error("Enter your Anthropic API key first.")]
    NoKey,
    #[error("The API key isn't valid: paste it again from the Anthropic Console.")]
    BadKey,
    #[error("Describe the design first.")]
    NoPrompt,
    #[error("Type a question first.")]
    NoQuestion,
    #[error("The question is too long: keep it under 2,000 characters.")]
    LongQuestion,
    #[error("The description is too long: keep it under 2,000 characters.")]
    LongPrompt,
    #[error("The image is too large to send: crop it, or place a smaller copy.")]
    LargeImage,
    #[error("Write what went wrong first, or have Claude write it.")]
    EmptyReport,
    /// The key was refused (401) or may not use the model (403).
    #[error("Anthropic refused the API key: {0}")]
    Key(String),
    /// The account can't pay for the request (402).
    #[error("Your Anthropic account can't pay for this: {0}")]
    Billing(String),
    /// Too many requests (429) or the service is overloaded (529, 503).
    #[error("The AI service is busy: try again in a minute.")]
    Busy,
    /// The service failed (5xx).
    #[error("The AI service failed (error {0}): try again later.")]
    Service(u16),
    /// Any other refusal of the request, with the API's message.
    #[error("The request failed (error {0}): {1}")]
    Request(u16, String),
    /// The model declined to draw it.
    #[error("The AI declined to draw this: try another description.")]
    Refused,
    /// The description asks for someone else's logo or character, with the model's reason.
    #[error("The AI won't copy someone else's logo or character: {0}")]
    NotOriginal(String),
    /// The reply ran out of room before the artwork was done.
    #[error("The drawing grew too detailed to finish: ask for something simpler.")]
    Truncated,
    /// The reply holds no usable artwork.
    #[error("The reply had no artwork: try again.")]
    NoArt,
    /// The model declined to answer.
    #[error("The AI declined to answer this: ask another way.")]
    RefusedAnswer,
    /// The reply holds no answer, or ran out of room before one.
    #[error("The reply had no answer: try again, or ask something narrower.")]
    NoAnswer,
    /// A picture service answered without a picture.
    #[error("The AI service sent no picture: try again, or choose another service.")]
    NoPicture,
    /// Check My Sew-out was asked with no photo.
    #[error("Add a photo of the sew-out first.")]
    NoPhoto,
    /// No StitchFoundry service is set up in this build.
    #[error("Foundry accounts aren't open yet.")]
    NoServer,
    /// The StitchFoundry session ended, or there is none.
    #[error("Sign in to your Foundry account (Embroidery › Foundry Account…).")]
    SignIn,
    /// The Foundry account hasn't the credits.
    #[error("Not enough Foundry credits: buy more in Foundry Account.")]
    NoCredits,
    /// An account with the email exists already.
    #[error("An account with that email already exists: sign in instead.")]
    Taken,
    /// The StitchFoundry service can't do it (its AI or payments aren't set up).
    #[error("The StitchFoundry service can't do this yet: try again later.")]
    CloudClosed,
    /// The request never got an answer (from the app's client).
    #[error("Couldn't reach the AI service: {0}")]
    Network(String),
    /// MakerFoundry's address (the setting) isn't one the app may send to.
    #[error("MakerFoundry's address isn't valid: use an https:// address.")]
    NoMakerFoundry,
    /// The design and its photos are over MakerFoundry's limits (or it answered 413).
    #[error("The design is too big for MakerFoundry.")]
    TooBigForMakerFoundry,
    /// MakerFoundry found the sign-in ended (401), with its message.
    #[error("{0}")]
    MakerFoundrySignIn(String),
    /// MakerFoundry refused the design, with its message to show as it is.
    #[error("{0}")]
    MakerFoundry(String),
    /// MakerFoundry failed without saying why.
    #[error("MakerFoundry couldn't take the design (error {0}): try again later.")]
    MakerFoundryFailed(u16),
}

impl ArtError {
    /// Whether the same request may succeed if sent again shortly.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Busy | Self::Service(_) | Self::Network(_) | Self::NoArt | Self::NoAnswer | Self::NoPicture)
    }
}

/// `key` checked for use in a header: trimmed, non-empty, printable ASCII without spaces.
pub fn clean_key(key: &str) -> Result<&str, ArtError> {
    let key = key.trim();
    if key.is_empty() {
        return Err(ArtError::NoKey);
    }
    if key.len() > 400 || !key.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(ArtError::BadKey);
    }
    Ok(key)
}

impl ArtRequest {
    /// The request with its size and colours brought into range and the description trimmed.
    pub fn new(prompt: &str, size_mm: f64, colors: u32) -> Self {
        let size_mm = if size_mm.is_finite() { size_mm.clamp(SIZE_RANGE.0, SIZE_RANGE.1) } else { 100.0 };
        Self { prompt: prompt.trim().to_string(), size_mm, colors: colors.clamp(COLOR_RANGE.0, COLOR_RANGE.1), image: None, effort: Effort::Normal }
    }

    /// The request made with `effort`.
    pub fn with_effort(mut self, effort: Effort) -> Self {
        self.effort = effort;
        self
    }

    /// What it should cost (dollars, at Anthropic's prices).
    pub fn estimate(&self) -> effort::Estimate {
        effort::estimate(&self.body(), self.effort)
    }

    /// The request redrawing `image` (its prompt: notes on what to change, if any).
    pub fn with_image(mut self, image: ArtImage) -> Self {
        self.image = Some(image);
        self
    }

    /// The instructions that make the model draw stitchable art.
    pub fn system_prompt(&self) -> String {
        let unit = 1000.0 / self.size_mm; // viewBox units per millimetre
        let min = (MIN_FEATURE_MM * unit).ceil();
        let gap = unit.ceil();
        let colors = self.colors;
        let size = self.size_mm.round();
        let shapes = "- Only filled shapes (path, circle, ellipse, rect, polygon), each with a solid fill=\"#rrggbb\". \
No strokes, gradients, patterns, filters, masks, clip paths, opacity, text, images or fonts. Draw \
outlines and lines as filled shapes too.";
        let order = "- Shapes go back to front: the first one is sewn first and later ones lie on top of it. Where \
the stacking allows, draw the shapes of one colour one after another (each colour change stops the \
machine).";
        let reply = "Reply with one SVG document and nothing else: no explanation and no Markdown code fence.";
        if self.image.is_some() {
            return format!(
                "You redraw images as artwork that will be stitched by an embroidery machine. {reply}\n\
\n\
The attached image is the design. It will be stitched about {size} mm across its longest side. Keep \
its composition, shapes, proportions, lettering and colours as faithfully as embroidery allows, and \
simplify only what can't be stitched, the way a skilled embroidery digitizer would:\n\
{shapes}\n\
- At most {colors} different fill colours: the image's own, with gradients, shading and similar \
shades merged into the nearest of them.\n\
- Leave out the image's own background (paper, a photo backdrop, a plain colour around the design) \
unless it is part of the design, like the shape of a badge or a patch.\n\
- No detail too small to stitch: every part at least {min} units wide (about {MIN_FEATURE_MM} mm \
stitched) and gaps between parts at least {gap} units; thicken or leave out anything finer, and no \
shading, hatching or texture.\n\
- Lettering as filled shapes that follow its letterforms closely. Letters shorter than about 5 mm \
stitched can't be read: make them larger where there is room, else leave them out.\n\
{order} Keep to about 100 shapes or fewer.\n\
- viewBox with the image's aspect ratio and 1000 on its longest side, the drawing in the image's own \
place on it, absolute coordinates and no transforms on the root element."
            );
        }
        let research = if self.effort.researches() {
            "\n\nWhen the description names a real thing (an animal breed, a vehicle or building model, a \
plant, a place) and you aren't sure what sets it apart, search the web first, so that its shape, \
proportions, colours and markings are right. Then reply with the SVG alone."
        } else {
            ""
        };
        format!(
            "You draw artwork that will be stitched by an embroidery machine. {reply}\n\
\n\
The artwork is stitched about {size} mm across its longest side, so draw it the way a skilled \
embroidery digitizer would:\n\
{shapes}\n\
- At most {colors} different fill colours, chosen to read well as thread on fabric. No \
background rectangle unless the design is a patch that needs one.\n\
- Bold, flat, simple shapes with clear silhouettes, like a sticker or a patch: no shading, \
hatching, texture or tiny details. Every part is at least {min} units wide (about \
{MIN_FEATURE_MM} mm stitched) and gaps between parts at least {gap} units.\n\
{order} Keep to about 60 shapes or fewer.\n\
- viewBox=\"0 0 1000 1000\" (another aspect is fine with 1000 on its longest side), absolute \
coordinates, no transforms on the root element.\n\
\n\
Never reproduce a logo, trademark, mascot or character that belongs to someone else (a team, \
company, brand, film, show or game). Real things are fine (an animal breed, a car or truck model, a \
building): draw them without their makers' badges or logos. If the description asks for a logo or \
a character, reply with a single line that starts with \"DECLINED:\" followed by a short reason, \
instead of the SVG.{research}"
        )
    }

    /// The Messages API request body.
    pub fn body(&self) -> Value {
        let content = match &self.image {
            Some(img) => {
                let mut text = "Redraw this image as embroidery artwork.".to_string();
                if !self.prompt.is_empty() {
                    text.push_str("\n\nWhat to change: ");
                    text.push_str(&self.prompt);
                }
                json!([
                    { "type": "image", "source": { "type": "base64", "media_type": img.media_type, "data": img.data_base64 } },
                    { "type": "text", "text": text },
                ])
            }
            None => json!(format!("Draw this as embroidery artwork: {}", self.prompt)),
        };
        let mut body = json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "fallbacks": "default",
            "output_config": { "effort": self.effort.api() },
            "system": self.system_prompt(),
            "messages": [{ "role": "user", "content": content }],
        });
        // a drawing from a description may look up what it draws
        if self.effort.researches() && self.image.is_none() {
            body["tools"] = json!([{ "type": WEB_SEARCH, "name": "web_search", "max_uses": MAX_SEARCHES }]);
        }
        body
    }

    /// The check's request body: the conversation so far (the request and `first`, the drawing
    /// as it came), and `stitched`, a picture of it as the machine will sew it.
    pub fn check_body(&self, first: &Art, stitched: &ArtImage) -> Value {
        let size = self.size_mm.round();
        let text = format!(
            "This picture shows your artwork as the machine will stitch it, about {size} mm across. Check it \
against the request: is the subject right and recognisable (what sets it apart, every detail asked for), \
are the colours right, has any part gone missing, merged or come out too thin to stitch? If it is good, \
reply with the single word OK. Otherwise reply with a corrected SVG document alone, keeping to the same rules."
        );
        let mut body = self.body();
        if let Some(m) = body.get_mut("messages").and_then(Value::as_array_mut) {
            m.push(json!({ "role": "assistant", "content": first.content }));
            m.push(json!({ "role": "user", "content": [
                { "type": "image", "source": { "type": "base64", "media_type": stitched.media_type, "data": stitched.data_base64 } },
                { "type": "text", "text": text },
            ] }));
        }
        body
    }

    /// The check's HTTP request with API key `key`.
    pub fn check_http(&self, key: &str, first: &Art, stitched: &ArtImage) -> Result<HttpRequest, ArtError> {
        if stitched.data_base64.len() > MAX_IMAGE_BASE64 {
            return Err(ArtError::LargeImage);
        }
        let mut http = self.http(key)?;
        http.body = self.check_body(first, stitched).to_string();
        Ok(http)
    }

    /// The HTTP request to send with API key `key`.
    pub fn http(&self, key: &str) -> Result<HttpRequest, ArtError> {
        let key = clean_key(key)?;
        // A redrawing needs no description: its notes are optional.
        if self.prompt.is_empty() && self.image.is_none() {
            return Err(ArtError::NoPrompt);
        }
        if self.image.as_ref().is_some_and(|i| i.data_base64.len() > MAX_IMAGE_BASE64) {
            return Err(ArtError::LargeImage);
        }
        if self.prompt.chars().count() > MAX_PROMPT {
            return Err(ArtError::LongPrompt);
        }
        Ok(HttpRequest {
            url: API_URL.to_string(),
            headers: vec![
                ("content-type", "application/json".to_string()),
                ("x-api-key", key.to_string()),
                ("anthropic-version", API_VERSION.to_string()),
                ("anthropic-beta", FALLBACK_BETA.to_string()),
            ],
            body: self.body().to_string(),
        })
    }
}

/// The API's own message in an error body (`{"type": "error", "error": {"message": …}}`), else
/// the start of the body.
fn api_message(body: &str) -> String {
    let v: Option<Value> = serde_json::from_str(body).ok();
    if let Some(m) = v.as_ref().and_then(|v| v.pointer("/error/message")).and_then(Value::as_str) {
        return m.to_string();
    }
    let text: String = body.trim().chars().take(200).collect();
    if text.is_empty() { "no details".to_string() } else { text }
}

/// The error an HTTP status other than success stands for.
fn check_status(status: u16, body: &str) -> Result<(), ArtError> {
    match status {
        200..=299 => Ok(()),
        401 | 403 => Err(ArtError::Key(api_message(body))),
        402 => Err(ArtError::Billing(api_message(body))),
        429 | 503 | 529 => Err(ArtError::Busy),
        500..=599 => Err(ArtError::Service(status)),
        _ => Err(ArtError::Request(status, api_message(body))),
    }
}

/// A reply's text blocks joined (thinking left out).
fn reply_text(v: &Value) -> String {
    v.get("content")
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default()
}

/// The artwork in a reply with HTTP status `status` and body `body`.
pub fn parse_reply(status: u16, body: &str) -> Result<Art, ArtError> {
    check_status(status, body)?;
    let v: Value = serde_json::from_str(body).map_err(|_| ArtError::NoArt)?;
    // The stop reason comes first: a declined or cut-off reply has no finished artwork.
    match v.get("stop_reason").and_then(Value::as_str) {
        Some("refusal") => return Err(ArtError::Refused),
        Some("max_tokens") => return Err(ArtError::Truncated),
        _ => {}
    }
    let text = reply_text(&v);
    if let Some(reason) = text.trim().strip_prefix("DECLINED:") {
        let reason: String = reason.trim().chars().take(300).collect();
        return Err(ArtError::NotOriginal(if reason.is_empty() { "describe an original design".into() } else { reason }));
    }
    let svg = extract_svg(&text).ok_or(ArtError::NoArt)?;
    let model = v.get("model").and_then(Value::as_str).unwrap_or(MODEL).to_string();
    Ok(Art { svg: svg.to_string(), model, usage: Usage::of_reply(&v), content: v.get("content").cloned().unwrap_or(Value::Null) })
}

/// What the check said ([`ArtRequest::check_http`]), and the tokens it took: a reply with an SVG
/// in it is a better drawing; one without (\"OK\") passes the first.
pub fn parse_check(status: u16, body: &str) -> Result<(Checked, Usage), ArtError> {
    check_status(status, body)?;
    let v: Value = serde_json::from_str(body).map_err(|_| ArtError::NoArt)?;
    let usage = Usage::of_reply(&v);
    // a check that was declined or cut off leaves the first drawing as it is
    if let Some("refusal" | "max_tokens") = v.get("stop_reason").and_then(Value::as_str) {
        return Ok((Checked::Good, usage));
    }
    let text = reply_text(&v);
    let checked = match extract_svg(&text) {
        Some(svg) => {
            let model = v.get("model").and_then(Value::as_str).unwrap_or(MODEL).to_string();
            Checked::Redrawn(Art { svg: svg.to_string(), model, usage, content: v.get("content").cloned().unwrap_or(Value::Null) })
        }
        None => Checked::Good,
    };
    Ok((checked, usage))
}

/// The SVG document in `text`: from its first `<svg` to its last `</svg>` (prose or a code fence
/// around it is left out).
pub fn extract_svg(text: &str) -> Option<&str> {
    let start = text.find("<svg")?;
    let end = text.rfind("</svg>")? + "</svg>".len();
    let svg = text.get(start..end)?;
    (svg.len() <= MAX_SVG).then_some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AI-04: the check sends the conversation back with the stitched picture, and reads OK or a
    /// better drawing.
    #[test]
    fn the_check_goes_back_with_the_stitches_and_reads_the_answer() {
        let r = ArtRequest::new("a fox", 80.0, 4).with_effort(Effort::Thorough);
        let first_reply = json!({"model": MODEL, "stop_reason": "end_turn", "content": [
            {"type": "thinking", "thinking": "", "signature": "sig"},
            {"type": "text", "text": "<svg viewBox=\"0 0 10 10\"><rect width=\"10\" height=\"10\" fill=\"#ff8800\"/></svg>"}
        ], "usage": {"input_tokens": 1200, "output_tokens": 5000}});
        let first = parse_reply(200, &first_reply.to_string()).unwrap();
        assert_eq!(first.usage, Usage { input: 1200, output: 5000, searches: 0 });
        let png = ArtImage { media_type: "image/png", data_base64: "iVBORw0KGgo=".into() };
        let http = r.check_http("sk-ant-test", &first, &png).unwrap();
        let body: Value = serde_json::from_str(&http.body).unwrap();
        let m = body["messages"].as_array().unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(m[1]["content"], first_reply["content"], "the reply goes back unchanged, thinking included");
        assert_eq!(m[2]["content"][0]["type"], "image");
        assert!(m[2]["content"][1]["text"].as_str().unwrap().contains("single word OK"));
        let ok =
            json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": "OK"}], "usage": {"input_tokens": 9000, "output_tokens": 40}});
        assert_eq!(parse_check(200, &ok.to_string()).unwrap(), (Checked::Good, Usage { input: 9000, output: 40, searches: 0 }));
        let better = json!({"stop_reason": "end_turn", "content": [{"type": "text", "text": "<svg viewBox=\"0 0 10 10\"></svg>"}]});
        assert!(matches!(parse_check(200, &better.to_string()).unwrap().0, Checked::Redrawn(a) if a.svg.starts_with("<svg")));
        let cut = json!({"stop_reason": "max_tokens", "content": [{"type": "text", "text": "<svg"}]});
        assert_eq!(parse_check(200, &cut.to_string()).unwrap().0, Checked::Good, "a cut-off check keeps the first drawing");
        assert!(parse_check(529, "{}").is_err());
    }

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000"><circle cx="500" cy="500" r="400" fill="#e07020"/></svg>"##;

    fn reply(stop: &str, blocks: Value) -> String {
        json!({"id": "msg_1", "type": "message", "role": "assistant", "model": MODEL, "content": blocks, "stop_reason": stop}).to_string()
    }

    #[test]
    fn request_names_the_model_effort_fallback_and_art_rules() {
        let r = ArtRequest::new("  a cartoon fox head  ", 80.0, 5);
        assert_eq!(r.prompt, "a cartoon fox head");
        let http = r.http(" sk-ant-test ").unwrap();
        assert_eq!(http.url, API_URL);
        let header = |k: &str| http.headers.iter().find(|h| h.0 == k).map(|h| h.1.as_str());
        assert_eq!(header("x-api-key"), Some("sk-ant-test"));
        assert_eq!(header("anthropic-version"), Some("2023-06-01"));
        assert_eq!(header("anthropic-beta"), Some(FALLBACK_BETA));
        assert_eq!(header("content-type"), Some("application/json"));
        let body: Value = serde_json::from_str(&http.body).unwrap();
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["max_tokens"], MAX_TOKENS);
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["output_config"]["effort"], "medium");
        // AI-05: the effort chosen is sent, and the estimate grows with it
        let quick = ArtRequest::new("a fox", 80.0, 5).with_effort(Effort::Quick);
        let quick_body: Value = serde_json::from_str(&quick.http("sk-ant-test").unwrap().body).unwrap();
        assert_eq!(quick_body["output_config"]["effort"], "low");
        assert!(quick.estimate().typical < quick.clone().with_effort(Effort::Thorough).estimate().typical);
        assert!(body.get("tools").is_none(), "only Thorough looks things up");
        // AI-04: Thorough may search the web, a little, and is told when to
        let thorough = ArtRequest::new("a 2023 Ford F-350 crew cab", 100.0, 6).with_effort(Effort::Thorough);
        let thorough_body = thorough.body();
        assert_eq!(thorough_body["tools"][0]["type"], WEB_SEARCH);
        assert_eq!(thorough_body["tools"][0]["max_uses"], MAX_SEARCHES);
        assert!(thorough_body["system"].as_str().unwrap().contains("search the web first"));
        assert!(thorough_body["system"].as_str().unwrap().contains("without their makers' badges"));
        assert!(body.get("thinking").is_none(), "thinking is always on: not sent");
        assert!(body["messages"][0]["content"].as_str().unwrap().ends_with("a cartoon fox head"));
        let system = body["system"].as_str().unwrap();
        // 80 mm: 12.5 units a millimetre, so 1.5 mm is 19 units and a 1 mm gap 13
        assert!(system.contains("At most 5 different fill colours"), "{system}");
        assert!(system.contains("about 80 mm") && system.contains("at least 19 units") && system.contains("at least 13 units"), "{system}");
        assert!(system.contains("starts with \"DECLINED:\""), "{system}");
    }

    #[test]
    fn a_redrawing_sends_the_image_first_and_needs_no_description() {
        let img = ArtImage { media_type: "image/png", data_base64: "iVBORw0KGgo=".into() };
        let r = ArtRequest::new("", 70.0, 4).with_image(img.clone());
        let body: Value = serde_json::from_str(&r.http("sk-ant-test").unwrap().body).unwrap();
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "image");
        assert_eq!(content[0]["source"], json!({"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}));
        assert_eq!(content[1]["text"], "Redraw this image as embroidery artwork.");
        let system = body["system"].as_str().unwrap();
        assert!(system.contains("The attached image is the design") && system.contains("At most 4 different fill colours"), "{system}");
        assert!(system.contains("image's aspect ratio") && !system.contains("DECLINED"), "{system}");
        assert!(system.contains("the shapes of one colour one after another"), "{system}");
        // notes on what to change follow the instruction
        let r = ArtRequest::new("drop the tagline", 70.0, 4).with_image(img);
        let body: Value = serde_json::from_str(&r.http("sk-ant-test").unwrap().body).unwrap();
        assert!(body["messages"][0]["content"][1]["text"].as_str().unwrap().ends_with("What to change: drop the tagline"));
        // too large to send
        let big = ArtImage { media_type: "image/jpeg", data_base64: "A".repeat(MAX_IMAGE_BASE64 + 1) };
        assert_eq!(ArtRequest::new("", 70.0, 4).with_image(big).http("k"), Err(ArtError::LargeImage));
        // a description is drawn with the same colour order rule
        assert!(ArtRequest::new("a star", 70.0, 4).system_prompt().contains("the shapes of one colour one after another"));
    }

    #[test]
    fn requests_are_checked_before_sending() {
        let r = ArtRequest::new("a star", 100.0, 3);
        assert_eq!(r.http("  "), Err(ArtError::NoKey));
        assert_eq!(r.http("sk-ant bad"), Err(ArtError::BadKey));
        assert_eq!(r.http("sk-ant-\r\nx"), Err(ArtError::BadKey));
        assert_eq!(ArtRequest::new(" ", 100.0, 3).http("k"), Err(ArtError::NoPrompt));
        assert_eq!(ArtRequest::new(&"x".repeat(MAX_PROMPT + 1), 100.0, 3).http("k"), Err(ArtError::LongPrompt));
        // out-of-range sizes and colour counts come into range
        let r = ArtRequest::new("a", f64::NAN, 0);
        assert_eq!((r.size_mm, r.colors), (100.0, 1));
        let r = ArtRequest::new("a", 1e9, 99);
        assert_eq!((r.size_mm, r.colors), (SIZE_RANGE.1, COLOR_RANGE.1));
    }

    #[test]
    fn the_svg_is_read_from_text_blocks_around_thinking_and_fences() {
        let body = reply(
            "end_turn",
            json!([
                {"type": "thinking", "thinking": "", "signature": "x"},
                {"type": "text", "text": format!("Here it is:\n```svg\n<?xml version=\"1.0\"?>\n{SVG}\n```")}
            ]),
        );
        let art = parse_reply(200, &body).unwrap();
        assert_eq!(art.svg, SVG);
        assert_eq!(art.model, MODEL);
        // a fallback model that drew it is reported
        let v = json!({"model": "claude-opus-5", "content": [{"type": "text", "text": SVG}], "stop_reason": "end_turn"});
        assert_eq!(parse_reply(200, &v.to_string()).unwrap().model, "claude-opus-5");
    }

    #[test]
    fn stop_reasons_and_missing_art_are_errors() {
        assert_eq!(parse_reply(200, &reply("refusal", json!([]))), Err(ArtError::Refused));
        let cut = reply("max_tokens", json!([{"type": "text", "text": "<svg viewBox=\"0 0 10 10\"><path d=\"M0 0"}]));
        assert_eq!(parse_reply(200, &cut), Err(ArtError::Truncated));
        assert_eq!(parse_reply(200, &reply("end_turn", json!([{"type": "text", "text": "I can't draw that."}]))), Err(ArtError::NoArt));
        assert_eq!(parse_reply(200, "not json"), Err(ArtError::NoArt));
        // someone else's logo or character: the model's reason is shown
        let declined = reply("end_turn", json!([{"type": "text", "text": "DECLINED: that is a sports team's logo"}]));
        assert_eq!(parse_reply(200, &declined), Err(ArtError::NotOriginal("that is a sports team's logo".into())));
        let huge = format!("<svg>{}</svg>", "x".repeat(MAX_SVG));
        assert_eq!(parse_reply(200, &reply("end_turn", json!([{"type": "text", "text": huge}]))), Err(ArtError::NoArt));
    }

    #[test]
    fn http_errors_carry_the_api_message() {
        let err = |kind: &str, msg: &str| json!({"type": "error", "error": {"type": kind, "message": msg}}).to_string();
        assert_eq!(parse_reply(401, &err("authentication_error", "invalid x-api-key")), Err(ArtError::Key("invalid x-api-key".into())));
        assert_eq!(parse_reply(402, &err("billing_error", "credit balance too low")), Err(ArtError::Billing("credit balance too low".into())));
        assert_eq!(parse_reply(429, &err("rate_limit_error", "slow down")), Err(ArtError::Busy));
        assert_eq!(parse_reply(529, &err("overloaded_error", "overloaded")), Err(ArtError::Busy));
        assert_eq!(parse_reply(500, &err("api_error", "oops")), Err(ArtError::Service(500)));
        assert_eq!(parse_reply(400, &err("invalid_request_error", "bad field")), Err(ArtError::Request(400, "bad field".into())));
        assert_eq!(parse_reply(404, "<html>gone</html>"), Err(ArtError::Request(404, "<html>gone</html>".into())));
        assert_eq!(parse_reply(413, ""), Err(ArtError::Request(413, "no details".into())));
        assert!(ArtError::Busy.retryable() && ArtError::Network("x".into()).retryable() && !ArtError::Refused.retryable());
        assert_eq!(ArtError::Key("invalid x-api-key".into()).to_string(), "Anthropic refused the API key: invalid x-api-key");
    }
}
