//! Pictures from other AI services for Design from Prompt: Google's Nano Banana, OpenAI's GPT
//! Image, Flux through fal, xAI's Grok Imagine, or any image model through OpenRouter, each with
//! the user's own key
//! for that service. A picture is never stitched as it is: the app redraws it with Claude as flat
//! shapes in the thread colours asked for ([`super::ArtRequest::with_image`]), then digitizes it,
//! so a beautiful picture with shading and fine detail still sews with few thread changes. As in
//! [`super`], no network code lives here.
//!
//! Each service answers in its own shape, and their shapes change; [`parse_reply`] finds the
//! picture wherever it is (base64 data, or a `data:image/…` address), so a model of the user's
//! choosing (its id is editable) works as long as it answers with a picture.

use serde_json::{Value, json};

use super::{ArtError, ArtImage, HttpRequest, MAX_IMAGE_BASE64, MAX_PROMPT, check_status, clean_key};

/// A service that makes pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    OpenAi,
    Fal,
    OpenRouter,
    /// xAI's Grok Imagine (<https://docs.x.ai/developers/model-capabilities/images/generation>).
    Grok,
}

impl Provider {
    pub const ALL: [Provider; 5] = [Provider::Google, Provider::OpenAi, Provider::Fal, Provider::Grok, Provider::OpenRouter];

    pub fn id(self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::OpenAi => "openai",
            Provider::Fal => "fal",
            Provider::OpenRouter => "openrouter",
            Provider::Grok => "grok",
        }
    }

    pub fn from_id(id: &str) -> Option<Provider> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The service's name, as it calls itself (not translated).
    pub fn name(self) -> &'static str {
        match self {
            Provider::Google => "Google Nano Banana",
            Provider::OpenAi => "OpenAI GPT Image",
            Provider::Fal => "Flux (fal)",
            Provider::OpenRouter => "OpenRouter",
            Provider::Grok => "Grok Imagine",
        }
    }

    /// The secret its API key is kept as.
    pub fn key_name(self) -> &'static str {
        match self {
            Provider::Google => "google-api-key",
            Provider::OpenAi => "openai-api-key",
            Provider::Fal => "fal-api-key",
            Provider::OpenRouter => "openrouter-api-key",
            Provider::Grok => "xai-api-key",
        }
    }

    /// Where to get a key.
    pub fn key_url(self) -> &'static str {
        match self {
            Provider::Google => "https://aistudio.google.com/apikey",
            Provider::OpenAi => "https://platform.openai.com/api-keys",
            Provider::Fal => "https://fal.ai/dashboard/keys",
            Provider::OpenRouter => "https://openrouter.ai/settings/keys",
            Provider::Grok => "https://console.x.ai",
        }
    }

    /// The model used unless another is named (the services' own ids, as of 2026-10).
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Google => "gemini-nano-banana-2.1",
            Provider::OpenAi => "gpt-image-2",
            Provider::Fal => "fal-ai/flux/schnell",
            Provider::OpenRouter => "openai/gpt-image-2",
            Provider::Grok => "grok-imagine-image-quality",
        }
    }
}

/// The longest model id accepted.
const MAX_MODEL: usize = 120;

/// What the picture is asked to be: the description, drawn the way that redraws and sews well.
pub fn picture_prompt(description: &str, colors: u32) -> String {
    format!(
        "{description}\n\nDraw it as a flat vector-style illustration for machine embroidery: bold, simple shapes in at \
most {colors} solid flat colours, no gradients, shading, texture or photo detail, no text or letters, no thin lines, \
the subject centred whole on a plain white background with space round it."
    )
}

/// The request for a picture of `description` (in at most `colors` colours) from `provider`'s
/// `model` (empty: its default), with that service's API key `key`.
pub fn request(provider: Provider, key: &str, model: &str, description: &str, colors: u32) -> Result<HttpRequest, ArtError> {
    let key = clean_key(key)?;
    let description = description.trim();
    if description.is_empty() {
        return Err(ArtError::NoPrompt);
    }
    if description.chars().count() > MAX_PROMPT {
        return Err(ArtError::LongPrompt);
    }
    let model = model.trim();
    let model = if model.is_empty() { provider.default_model() } else { model };
    if model.len() > MAX_MODEL || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./:@".contains(&b)) || model.contains("..") {
        return Err(ArtError::Request(400, format!("the model name {model:?} isn't one")));
    }
    let prompt = picture_prompt(description, colors.clamp(super::COLOR_RANGE.0, super::COLOR_RANGE.1));
    let json = ("content-type", "application/json".to_string());
    let (url, headers, body) = match provider {
        Provider::Google => (
            "https://generativelanguage.googleapis.com/v1beta/interactions".to_string(),
            vec![json, ("x-goog-api-key", key.to_string())],
            json!({ "model": model, "input": [{ "type": "text", "text": prompt }], "response_format": { "type": "image", "aspect_ratio": "1:1" } }),
        ),
        Provider::OpenAi => (
            "https://api.openai.com/v1/images/generations".to_string(),
            vec![json, ("authorization", format!("Bearer {key}"))],
            json!({ "model": model, "prompt": prompt, "n": 1, "size": "1024x1024" }),
        ),
        Provider::Fal => (
            format!("https://fal.run/{model}"),
            vec![json, ("authorization", format!("Key {key}"))],
            json!({ "prompt": prompt, "image_size": "square_hd", "num_images": 1, "output_format": "png", "sync_mode": true }),
        ),
        Provider::Grok => (
            "https://api.x.ai/v1/images/generations".to_string(),
            vec![json, ("authorization", format!("Bearer {key}"))],
            json!({ "model": model, "prompt": prompt, "n": 1, "response_format": "b64_json", "aspect_ratio": "1:1" }),
        ),
        Provider::OpenRouter => (
            "https://openrouter.ai/api/v1/images".to_string(),
            vec![json, ("authorization", format!("Bearer {key}"))],
            json!({ "model": model, "prompt": prompt }),
        ),
    };
    Ok(HttpRequest { url, headers, body: body.to_string() })
}

/// The picture in an answer with HTTP status `status` and body `body`, wherever the service put it.
pub fn parse_reply(status: u16, body: &str) -> Result<ArtImage, ArtError> {
    check_status(status, body)?;
    let v: Value = serde_json::from_str(body).map_err(|_| ArtError::NoPicture)?;
    let image = find_image(&v, 0).ok_or(ArtError::NoPicture)?;
    if image.data_base64.len() > MAX_IMAGE_BASE64 * 4 {
        return Err(ArtError::LargeImage);
    }
    Ok(image)
}

/// The first picture in `v`: a `data:image/…;base64,` address, or base64 data under a key that
/// holds it (`b64_json`, `data`, `bytes`), its type from a neighbouring `mime_type` /
/// `media_type` / `mimeType` / `content_type`, else from its first bytes.
fn find_image(v: &Value, depth: usize) -> Option<ArtImage> {
    if depth > 12 {
        return None;
    }
    match v {
        Value::String(s) => data_url(s),
        Value::Array(a) => a.iter().find_map(|x| find_image(x, depth + 1)),
        Value::Object(o) => {
            let named = ["mime_type", "media_type", "mimeType", "content_type", "contentType"]
                .iter()
                .find_map(|k| o.get(*k).and_then(Value::as_str))
                .filter(|t| t.starts_with("image/"));
            for k in ["b64_json", "data", "bytes", "image", "url"] {
                if let Some(s) = o.get(k).and_then(Value::as_str) {
                    if let Some(img) = data_url(s) {
                        return Some(img);
                    }
                    if s.len() > 64
                        && let Some(media_type) = named.and_then(known_type).or_else(|| sniff(s))
                    {
                        return Some(ArtImage { media_type, data_base64: s.to_string() });
                    }
                }
            }
            o.values().find_map(|x| find_image(x, depth + 1))
        }
        _ => None,
    }
}

/// A `data:image/png;base64,…` (or JPEG, WebP) address's picture.
fn data_url(s: &str) -> Option<ArtImage> {
    let rest = s.strip_prefix("data:")?;
    let (head, data) = rest.split_once(',')?;
    let media = head.strip_suffix(";base64")?;
    Some(ArtImage { media_type: known_type(media)?, data_base64: data.to_string() })
}

fn known_type(t: &str) -> Option<&'static str> {
    match t {
        "image/png" => Some("image/png"),
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/webp" => Some("image/webp"),
        _ => None,
    }
}

/// A base64 picture's type from its first bytes (PNG, JPEG, WebP).
fn sniff(b64: &str) -> Option<&'static str> {
    if b64.starts_with("iVBORw0KGgo") {
        Some("image/png")
    } else if b64.starts_with("/9j/") {
        Some("image/jpeg")
    } else if b64.starts_with("UklGR") {
        Some("image/webp")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

    #[test]
    fn each_service_is_asked_its_own_way_with_its_own_key() {
        let body = |r: &HttpRequest| serde_json::from_str::<Value>(&r.body).unwrap();
        let header = |r: &HttpRequest, k: &str| r.headers.iter().find(|h| h.0 == k).map(|h| h.1.clone());
        let g = request(Provider::Google, " gk ", "", "a fire truck from the front", 5).unwrap();
        assert_eq!(g.url, "https://generativelanguage.googleapis.com/v1beta/interactions");
        assert_eq!(header(&g, "x-goog-api-key").as_deref(), Some("gk"));
        assert_eq!(body(&g)["model"], "gemini-nano-banana-2.1");
        let text = body(&g)["input"][0]["text"].as_str().unwrap().to_string();
        assert!(text.starts_with("a fire truck from the front") && text.contains("at most 5 solid flat colours"), "{text}");
        let o = request(Provider::OpenAi, "ok", "gpt-image-1", "a heart", 3).unwrap();
        assert_eq!((o.url.as_str(), header(&o, "authorization").as_deref()), ("https://api.openai.com/v1/images/generations", Some("Bearer ok")));
        assert_eq!(body(&o)["model"], "gpt-image-1", "the model named");
        let f = request(Provider::Fal, "fk", "", "a heart", 3).unwrap();
        assert_eq!((f.url.as_str(), header(&f, "authorization").as_deref()), ("https://fal.run/fal-ai/flux/schnell", Some("Key fk")));
        assert_eq!(body(&f)["sync_mode"], true, "the picture comes back in the answer");
        let r = request(Provider::OpenRouter, "rk", "", "a heart", 3).unwrap();
        assert_eq!(r.url, "https://openrouter.ai/api/v1/images");
        assert_eq!(request(Provider::OpenAi, "", "", "a heart", 3), Err(ArtError::NoKey));
        assert_eq!(request(Provider::OpenAi, "k", "", "  ", 3), Err(ArtError::NoPrompt));
        assert!(request(Provider::Fal, "k", "../../evil", "a heart", 3).is_err(), "a model id can't leave its path");
        assert_eq!(Provider::from_id("fal"), Some(Provider::Fal));
        let x = request(Provider::Grok, "xk", "", "a heart", 3).unwrap();
        assert_eq!((x.url.as_str(), header(&x, "authorization").as_deref()), ("https://api.x.ai/v1/images/generations", Some("Bearer xk")));
        assert_eq!((body(&x)["model"].as_str(), body(&x)["response_format"].as_str()), (Some("grok-imagine-image-quality"), Some("b64_json")));
        assert_eq!(Provider::from_id("grok"), Some(Provider::Grok));
    }

    #[test]
    fn the_picture_is_found_in_every_services_answer() {
        let png = |v: Value| parse_reply(200, &v.to_string()).unwrap();
        // OpenAI, OpenRouter and Grok
        assert_eq!(png(json!({"data": [{"b64_json": PNG}]})).media_type, "image/png");
        assert_eq!(png(json!({"data": [{"b64_json": PNG, "media_type": "image/png"}]})).data_base64, PNG);
        // Google: the interaction's output image, or its steps
        assert_eq!(png(json!({"output_image": {"data": PNG}})).data_base64, PNG);
        assert_eq!(png(json!({"steps": [{"type": "model_output", "content": [{"type": "text", "text": "here"}, {"type": "image", "data": PNG, "mime_type": "image/png"}]}]})).data_base64, PNG);
        assert_eq!(png(json!({"candidates": [{"content": {"parts": [{"inlineData": {"mimeType": "image/png", "data": PNG}}]}}]})).data_base64, PNG);
        // fal, with sync_mode: a data address
        let fal = png(json!({"images": [{"url": format!("data:image/png;base64,{PNG}"), "content_type": "image/png"}]}));
        assert_eq!((fal.media_type, fal.data_base64.as_str()), ("image/png", PNG));
        // no picture; refusals
        assert_eq!(parse_reply(200, r#"{"data": [{"revised_prompt": "x"}]}"#), Err(ArtError::NoPicture));
        assert!(matches!(parse_reply(401, r#"{"error": {"message": "bad key"}}"#), Err(ArtError::Key(_))));
        assert_eq!(parse_reply(429, "{}"), Err(ArtError::Busy));
    }
}
