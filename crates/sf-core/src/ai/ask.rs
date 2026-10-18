//! Ask About Embroidery: questions about machine embroidery or about using StitchFoundry, answered
//! by Claude, which can also look at the design open now and, with the user's leave, change it by
//! running StitchFoundry's commands (its two tools, [`tools`]). As with [`super::ArtRequest`], no
//! network code lives here: the app sends the [`HttpRequest`] that [`request`] builds, reads the
//! answer with [`parse_reply`], runs the commands asked for and sends their results back
//! ([`tool_results`]) until Claude has answered.
//!
//! The conversation is kept as the API's own messages and only ever added to: replies go back
//! exactly as they came (thinking blocks included), and the design summary rides in each question
//! rather than in the instructions, which stay the same for the whole conversation.

use serde_json::{Value, json};

use super::{API_URL, API_VERSION, ArtError, FALLBACK_BETA, HttpRequest, MODEL, check_status, clean_key, reply_text};

/// The most tokens one reply (thinking included) may use.
pub const MAX_TOKENS: u32 = 16_000;
/// The longest question sent, in characters.
pub const MAX_QUESTION: usize = 2_000;
/// The most questions one conversation holds (then: New Conversation).
pub const MAX_QUESTIONS: usize = 30;
/// The most requests one question may take (each command round is one).
pub const MAX_ROUNDS: usize = 16;
/// The longest design summary sent, in characters.
pub const MAX_DESIGN: usize = 6_000;
/// The longest command result sent back, in characters (the start of it).
pub const MAX_RESULT: usize = 20_000;

/// The tool that searches the commands.
pub const FIND: &str = "find_commands";
/// The tool that runs one.
pub const RUN: &str = "run_command";

/// How StitchFoundry is laid out, so answers can name the right menu items.
pub(crate) const APP_GUIDE: &str = "\
StitchFoundry is a desktop and web app for machine embroidery digitizing, built on a vector \
drawing app. Where things are:
- Embroidery menu: Make a Design… (a step-by-step guide from words, a picture, a shape, appliqué \
or a quilting block), Add Words…, Save for My Machine (writes the machine file to the USB folder \
set in My Machine…), Check Design, Density Map, Compare Designs…
- Embroidery › Create: Convert Artwork to Embroidery, Auto-Digitize Image, Design from Prompt…, \
Redraw Image with Claude…, Digitizing Widget, Cross Stitch, Lettering Kiosk.
- Embroidery › Stitch Type: Run, Bean Run, Backstitch, Stem Stitch, Blanket, Satin, Tatami Fill, \
Satin Fill, Contour Fill, Spiral Fill, Radial Fill, Cross Fill, Meander Fill, Motif Fill, \
Appliqué, Manual Stitches, Sequins, Beads, Rhinestones, Cord, Boring, No Stitches.
- Embroidery › Stitch Direction: Inclination Tool (Shift+I; drag lines across a shape to turn \
its stitches), Use Line as Direction (a drawn line crossing a shape sets its stitch angle), Auto \
Direction…, Reverse Stitch Direction, Swap Satin Sides, Convert to Satin Column.
- Embroidery › Split Lines, Motifs, Add to Design (rings, outlines, borders, laydown, \
buttonholes, echo and background quilting), Shape (remove or fill holes, automatic knife, \
branch, redwork, break apart), Colors and Order (sort by thread colour, match threads to a \
chart, merge colours, auto sequence), Stitches (freeze, regenerate), Special Machines (chain, \
moss, chenille, longarm quilting), Embroidery Fonts, Export Machine File.
- The Embroidery panel (Window › Embroidery) holds the selected object's stitch settings: \
density (spacing), stitch length, underlay, pull compensation, angle, start and end points, \
plus the hoop, fabric, lettering, team names, monograms and design settings such as trims and \
tie-ins. The Stitch Sequence and Stitch List panels order and edit the stitches.
- Watch It Sew (the stitch player) shows the design sewing. File › My Machine… sets the \
machine, its hoops and file format (VP3, PES, JEF, EXP, XXX, DST and more).
- Edit › Undo undoes any step. Help › StitchFoundry Help… (F1) opens the user guide.";

/// The instructions, the same for every request of a conversation.
pub fn system_prompt() -> String {
    format!(
        "You answer questions inside StitchFoundry, an embroidery digitizing app, for people who \
sew on home and small-business embroidery machines; many are beginners and not at home with \
computers. Answer what was asked, in plain words, as briefly as the question allows: a few \
sentences, or a short list of steps. Write plain text only: no headings, tables, bold or code, \
and \"- \" for list items. Name StitchFoundry's menu items exactly as listed below when one does \
what the user needs; when you are not sure the app has something, say so rather than inventing \
a menu item. Give measurements in millimetres with inches in brackets. When a problem could \
damage the machine or the fabric, say so first. For questions that have nothing to do with \
embroidery, sewing or this app, say briefly that you can only help with embroidery.\n\n\
Each question may start with a summary of the design open now, written by the app; objects are \
named by their ids (#12). You can look at the design and change it with your tools: \
{FIND} finds StitchFoundry's commands and how to call them, and {RUN} runs one. Commands that \
only read (embroidery.info, embroidery.check, document.node and the like) run at once. Change \
the design only when the user asks for a change or agrees to one you suggested: each change is \
shown to the user, who allows or declines it; a declined change comes back as an error, so \
don't try it again, and say what you would have done instead. Most commands act on the selected \
objects, so select them first (select.set {{\"ids\": [12]}}). Document positions and sizes are in \
points (1 mm = 2.835 pt) unless a command's parameters say mm. After changing the design, say \
in one or two sentences what changed; Edit › Undo takes each change back.\n\n{APP_GUIDE}"
    )
}

/// The two tools: finding commands and running one.
pub fn tools() -> Value {
    json!([
        {
            "name": FIND,
            "description": "Search StitchFoundry's commands by a few words, such as \"satin width\", \"underlay\", \"trim\", \"stitch type\" or \"hoop\". Returns up to 20 commands, best first, each with its id, its label and menu, whether it can run now, and its parameters as \"{key, key?: …} what it does → result\" (a ? marks an optional key).",
            "input_schema": {
                "type": "object",
                "properties": { "query": { "type": "string", "description": "Words to look for in command ids, labels and menus" } },
                "required": ["query"],
            },
        },
        {
            "name": RUN,
            "description": "Run one StitchFoundry command, by its id from find_commands, with its parameters. Commands that only read run at once and return their result as JSON. Commands that change the design wait until the user allows them; one the user declines returns an error. File, export, preference and window commands can't be run from here.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command id, such as embroidery.set" },
                    "params": { "type": "object", "description": "The command's parameters (an empty object when it takes none)" },
                },
                "required": ["command"],
            },
        },
    ])
}

/// A question as the user message it is sent as: the design summary (if the user lets it go
/// along), then the question. Trimmed and checked first.
pub fn question(question: &str, design: Option<&str>) -> Result<Value, ArtError> {
    let q = question.trim();
    if q.is_empty() {
        return Err(ArtError::NoQuestion);
    }
    if q.chars().count() > MAX_QUESTION {
        return Err(ArtError::LongQuestion);
    }
    let mut blocks = Vec::new();
    if let Some(d) = design.map(str::trim).filter(|d| !d.is_empty()) {
        let d: String = d.chars().take(MAX_DESIGN).collect();
        blocks.push(json!({ "type": "text", "text": format!("The design open now (from the app, not from the user):\n{d}") }));
    }
    blocks.push(json!({ "type": "text", "text": q }));
    Ok(json!({ "role": "user", "content": blocks }))
}

/// A command's result for Claude: `Ok` text or the error why it didn't run.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub result: Result<String, String>,
}

/// The results of every tool call of one reply, as the one user message they go back in.
pub fn tool_results(results: &[ToolResult]) -> Value {
    let blocks: Vec<Value> = results
        .iter()
        .map(|r| match &r.result {
            Ok(text) => {
                let text: String = text.chars().take(MAX_RESULT).collect();
                json!({ "type": "tool_result", "tool_use_id": r.id, "content": text })
            }
            Err(e) => json!({ "type": "tool_result", "tool_use_id": r.id, "content": e, "is_error": true }),
        })
        .collect();
    json!({ "role": "user", "content": blocks })
}

/// The HTTP request for the conversation `messages` (the API's messages, oldest first, ending
/// with a question or tool results), with API key `key`.
pub fn request(key: &str, messages: &[Value]) -> Result<HttpRequest, ArtError> {
    request_with(key, system_prompt(), messages)
}

/// The request for a conversation with instructions `system` and the same tools (Check My
/// Sew-out's too: [`super::sewout`]).
pub(crate) fn request_with(key: &str, system: String, messages: &[Value]) -> Result<HttpRequest, ArtError> {
    let key = clean_key(key)?;
    let body = json!({
        "model": MODEL,
        "max_tokens": MAX_TOKENS,
        "fallbacks": "default",
        "output_config": { "effort": "medium" },
        "system": system,
        "tools": tools(),
        "messages": messages,
    });
    Ok(HttpRequest {
        url: API_URL.to_string(),
        headers: vec![
            ("content-type", "application/json".to_string()),
            ("x-api-key", key.to_string()),
            ("anthropic-version", API_VERSION.to_string()),
            ("anthropic-beta", FALLBACK_BETA.to_string()),
        ],
        body: body.to_string(),
    })
}

/// A tool call in a reply.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// A reply: the assistant message to keep, its text and the tool calls to answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// The reply as the assistant message added to the conversation, unchanged.
    pub message: Value,
    /// Its text blocks joined (cut-off text ends in "…").
    pub text: String,
    /// The tools to run before Claude can go on; empty once it has answered.
    pub calls: Vec<ToolCall>,
    /// The model that answered (another one when the request fell back).
    pub model: String,
}

/// The reply with HTTP status `status` and body `body`.
pub fn parse_reply(status: u16, body: &str) -> Result<Reply, ArtError> {
    check_status(status, body)?;
    let v: Value = serde_json::from_str(body).map_err(|_| ArtError::NoAnswer)?;
    let stop = v.get("stop_reason").and_then(Value::as_str);
    if stop == Some("refusal") {
        return Err(ArtError::RefusedAnswer);
    }
    let content = v.get("content").filter(|c| c.is_array()).cloned().ok_or(ArtError::NoAnswer)?;
    let calls: Vec<ToolCall> = if stop == Some("tool_use") {
        content
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            .filter_map(|b| {
                Some(ToolCall {
                    id: b.get("id")?.as_str()?.to_string(),
                    name: b.get("name")?.as_str()?.to_string(),
                    input: b.get("input").cloned().unwrap_or_else(|| json!({})),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut text = reply_text(&v).trim().to_string();
    if text.is_empty() && calls.is_empty() {
        return Err(ArtError::NoAnswer);
    }
    if stop == Some("max_tokens") && !text.is_empty() {
        text.push('…');
    }
    let model = v.get("model").and_then(Value::as_str).unwrap_or(MODEL).to_string();
    Ok(Reply { message: json!({ "role": "assistant", "content": content }), text, calls, model })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(stop: &str, content: Value) -> String {
        json!({"type": "message", "model": MODEL, "content": content, "stop_reason": stop}).to_string()
    }

    #[test]
    fn the_request_holds_the_tools_the_instructions_and_the_conversation() {
        let q = question("  Which stabilizer for a T-shirt?  ", Some("Size 80 × 60 mm")).unwrap();
        let http = request(" sk-ant-test ", std::slice::from_ref(&q)).unwrap();
        assert_eq!(http.url, API_URL);
        let header = |k: &str| http.headers.iter().find(|h| h.0 == k).map(|h| h.1.as_str());
        assert_eq!(header("x-api-key"), Some("sk-ant-test"));
        assert_eq!(header("anthropic-beta"), Some(FALLBACK_BETA));
        let body: Value = serde_json::from_str(&http.body).unwrap();
        assert_eq!(body["model"], MODEL);
        assert_eq!(body["max_tokens"], MAX_TOKENS);
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["output_config"]["effort"], "medium");
        assert!(body.get("tool_choice").is_none() && body.get("thinking").is_none());
        let names: Vec<&str> = body["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, [FIND, RUN]);
        let system = body["system"].as_str().unwrap();
        assert!(system.contains("Use Line as Direction") && system.contains("plain text only") && system.contains("select.set"), "{system}");
        // the design rides in the question, before it
        assert_eq!(
            body["messages"],
            json!([{"role": "user", "content": [
                {"type": "text", "text": "The design open now (from the app, not from the user):\nSize 80 × 60 mm"},
                {"type": "text", "text": "Which stabilizer for a T-shirt?"},
            ]}])
        );
        // the instructions don't change with the design: the conversation is only added to
        assert_eq!(system, system_prompt());
    }

    #[test]
    fn questions_and_keys_are_checked_before_sending() {
        assert_eq!(question("  ", None), Err(ArtError::NoQuestion));
        assert_eq!(question(&"x".repeat(MAX_QUESTION + 1), None), Err(ArtError::LongQuestion));
        assert_eq!(question("q", Some("  ")).unwrap()["content"].as_array().unwrap().len(), 1, "an empty summary is left out");
        assert_eq!(request(" ", &[]), Err(ArtError::NoKey));
        assert_eq!(request("sk ant", &[]), Err(ArtError::BadKey));
    }

    #[test]
    fn a_reply_with_tool_calls_is_kept_whole_and_its_calls_listed() {
        let content = json!([
            {"type": "thinking", "thinking": "", "signature": "sig"},
            {"type": "text", "text": "Let me look."},
            {"type": "tool_use", "id": "toolu_1", "name": RUN, "input": {"command": "embroidery.info", "params": {}}},
            {"type": "tool_use", "id": "toolu_2", "name": FIND, "input": {"query": "satin width"}},
        ]);
        let r = parse_reply(200, &reply("tool_use", content.clone())).unwrap();
        assert_eq!(r.message, json!({"role": "assistant", "content": content}), "thinking blocks go back unchanged");
        assert_eq!(r.text, "Let me look.");
        assert_eq!(r.calls.len(), 2);
        assert_eq!(r.calls[0], ToolCall { id: "toolu_1".into(), name: RUN.into(), input: json!({"command": "embroidery.info", "params": {}}) });
        // their results go back in one message, an error marked as one
        let m = tool_results(&[
            ToolResult { id: "toolu_1".into(), result: Ok("x".repeat(MAX_RESULT + 10)) },
            ToolResult { id: "toolu_2".into(), result: Err("the user declined this change".into()) },
        ]);
        assert_eq!(m["role"], "user");
        assert_eq!(m["content"][0]["content"].as_str().unwrap().len(), MAX_RESULT);
        assert_eq!(
            m["content"][1],
            json!({"type": "tool_result", "tool_use_id": "toolu_2", "content": "the user declined this change", "is_error": true})
        );
    }

    #[test]
    fn answers_and_their_errors() {
        let a = parse_reply(200, &reply("end_turn", json!([{"type": "text", "text": "  Use a cut-away stabilizer.  "}]))).unwrap();
        assert_eq!((a.text.as_str(), a.calls.len(), a.model.as_str()), ("Use a cut-away stabilizer.", 0, MODEL));
        assert_eq!(parse_reply(200, &reply("max_tokens", json!([{"type": "text", "text": "First,"}]))).unwrap().text, "First,…");
        assert_eq!(parse_reply(200, &reply("end_turn", json!([]))), Err(ArtError::NoAnswer));
        assert_eq!(parse_reply(200, &reply("refusal", json!([]))), Err(ArtError::RefusedAnswer));
        assert_eq!(parse_reply(200, "not json"), Err(ArtError::NoAnswer));
        assert_eq!(parse_reply(429, ""), Err(ArtError::Busy));
        assert!(matches!(parse_reply(401, r#"{"error": {"message": "invalid x-api-key"}}"#), Err(ArtError::Key(m)) if m == "invalid x-api-key"));
        assert!(ArtError::NoAnswer.retryable());
    }
}
