//! Check My Sew-out: photos of a finished sew-out, with the design that was sewn and a few answers
//! about the sewing, looked at by Claude, which says what went wrong and why and, with the user's
//! leave, fixes the design through the same tools as Ask About Embroidery ([`super::ask::tools`]).
//! The check is the first question of an Ask conversation ([`first_message`]) with its own
//! instructions ([`system_prompt`], [`request`]): the follow-up questions, the tool rounds and
//! the replies go as in [`super::ask`].

use serde_json::{Value, json};

use super::ask::{APP_GUIDE, FIND, MAX_DESIGN, RUN, request_with};
use super::{ArtError, ArtImage, HttpRequest, MAX_IMAGE_BASE64};

/// The most photos one check sends.
pub const MAX_PHOTOS: usize = 6;
/// The longest notes sent, in characters.
pub const MAX_NOTES: usize = 1_000;

/// What the user says about the sewing that a photo can't show; empty fields are left out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Answers {
    pub machine: String,
    pub fabric: String,
    pub stabilizer: String,
    pub needle: String,
    /// What happened while it sewed, each in the user's language or the app's words.
    pub happened: Vec<String>,
    /// Anything else, in the user's words.
    pub notes: String,
}

impl Answers {
    fn text(&self) -> String {
        let mut lines = Vec::new();
        for (label, v) in [("Machine", &self.machine), ("Fabric", &self.fabric), ("Stabilizer", &self.stabilizer), ("Needle", &self.needle)] {
            let v = v.trim();
            if !v.is_empty() {
                lines.push(format!("{label}: {v}"));
            }
        }
        let happened: Vec<&str> = self.happened.iter().map(|h| h.trim()).filter(|h| !h.is_empty()).collect();
        lines.push(format!(
            "While it sewed: {}",
            if happened.is_empty() { "nothing went wrong that I noticed".to_string() } else { happened.join("; ") }
        ));
        let notes: String = self.notes.trim().chars().take(MAX_NOTES).collect();
        if !notes.is_empty() {
            lines.push(format!("My notes: {notes}"));
        }
        lines.join("\n")
    }
}

/// The instructions for a check and the conversation after it.
pub fn system_prompt() -> String {
    format!(
        "You look at photos of a finished machine-embroidery sew-out inside StitchFoundry, an \
embroidery digitizing app, for people who sew on home and small-business machines; many are \
beginners. Say what went wrong, why, and how to fix it, as an experienced digitizer and machine \
embroiderer would.\n\n\
The first message holds the user's photos (the front, often the back, close-ups), then, when \
the design that was sewn is open, a picture of how it should look drawn from its own stitches \
and a summary of the design written by the app: its stitch types, density, compensation, \
underlay, order, trims and what Check Design finds. It ends with what the user says about the \
machine, fabric, stabilizer and what happened while it sewed.\n\n\
How to answer:\n\
- Look before you judge: compare the photos with the expected picture. Name only what you can \
see, where it is (\"the gap above the bumper\", \"the top of the S\"), and say how sure you are. \
If the photos can't settle it (no back, too blurry, too far away), say which photo would.\n\
- For each problem give the likely cause, then the fix, split into two groups: what the design \
can fix (overlap where shapes meet, pull and push compensation, density, underlay, the fabric \
setting, stitch direction, trims, order, small details too fine to sew) and what the machine or \
sewer can fix (top and bobbin tension, the needle, the bobbin, threading, stabilizer, hooping, \
topping, speed). Read the back: top thread showing there, loops or nests point to tension and \
threading, not the design.\n\
- Start with what went well, briefly; then the problems, the most important first; at most \
five. Plain text only: no headings, tables, bold or code, and \"- \" for list items. \
Measurements in millimetres with inches in brackets. Say first when something could damage the \
machine.\n\
- Offer to make the design fixes. Change the design only when the user agrees: then find the \
commands with {FIND} and run them with {RUN}; each change is shown to the user, who allows or \
declines it; a declined change comes back as an error, so don't try it again. Commands that \
only read (embroidery.info, embroidery.check, document.node and the like) run at once. Most \
commands act on the selected objects, so select them first (select.set {{\"ids\": [12]}}). \
Objects are named by their ids (#12). Document positions and sizes are in points (1 mm = 2.835 \
pt) unless a command's parameters say mm. After changing it, say what changed and suggest a \
test sew on a scrap of the same fabric; Edit › Undo takes each change back.\n\
- Without the design open, give the machine and sewing advice, and say that opening the design \
that was sewn lets you fix it too.\n\
- For photos that aren't of embroidery, say briefly that you can only look at sew-outs.\n\n{APP_GUIDE}"
    )
}

/// The check as the first user message: the photos, then the expected look and the design
/// summary when the design is open, then the answers.
pub fn first_message(photos: &[ArtImage], expected: Option<&ArtImage>, design: Option<&str>, answers: &Answers) -> Result<Value, ArtError> {
    if photos.is_empty() {
        return Err(ArtError::NoPhoto);
    }
    if photos.iter().chain(expected).any(|p| p.data_base64.len() > MAX_IMAGE_BASE64) {
        return Err(ArtError::LargeImage);
    }
    let image = |p: &ArtImage| json!({ "type": "image", "source": { "type": "base64", "media_type": p.media_type, "data": p.data_base64 } });
    let mut blocks: Vec<Value> = Vec::new();
    let n = photos.len().min(MAX_PHOTOS);
    blocks.push(json!({ "type": "text", "text": format!("My sew-out: {n} photo{}.", if n == 1 { "" } else { "s" }) }));
    blocks.extend(photos.iter().take(MAX_PHOTOS).map(image));
    if let Some(e) = expected {
        blocks.push(json!({ "type": "text", "text": "How the design should look, drawn by the app from its stitches:" }));
        blocks.push(image(e));
    }
    if let Some(d) = design.map(str::trim).filter(|d| !d.is_empty()) {
        let d: String = d.chars().take(MAX_DESIGN).collect();
        blocks.push(json!({ "type": "text", "text": format!("The design that was sewn (from the app, not from the user):\n{d}") }));
    }
    blocks.push(json!({ "type": "text", "text": format!("{}\n\nWhat went wrong, and how do I fix it?", answers.text()) }));
    Ok(json!({ "role": "user", "content": blocks }))
}

/// The question that has Claude write a check up for StitchFoundry's makers (Report to
/// StitchFoundry): sent in the check's conversation, its answer is the report's first draft.
pub const REPORT_QUESTION: &str = "Please write a short report about this sew-out for the people who make StitchFoundry. \
Start with a one-line title. Then say what went wrong, whether StitchFoundry's stitches, settings or advice were part of the \
cause (or whether it was the machine, thread, fabric, stabilizer or hooping), and what StitchFoundry itself could do better so \
other people don't run into it: a setting it should choose differently, a check it should make, a warning it should give. \
Plain text, under 250 words. Don't change the design.";

/// The longest report title, report and note sent (characters), and the most conversation kept.
pub const MAX_REPORT_TITLE: usize = 200;
pub const MAX_REPORT: usize = 8_000;
pub const MAX_REPORT_NOTE: usize = 4_000;
pub const MAX_REPORT_CONVERSATION: usize = 40_000;

/// What Report to StitchFoundry sends: the title, the report (Claude's draft as the user left
/// it), the user's note, and with them the photos, the check's conversation and the design's
/// summary when the user includes them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub check: Option<String>,
    pub title: String,
    pub report: String,
    pub note: String,
    pub photos: Vec<ArtImage>,
    pub conversation: Option<String>,
    pub design: Option<String>,
    /// The app's version.
    pub app: String,
}

impl Report {
    /// The body of `POST /v1/report`, the user having agreed to send it.
    pub fn body(&self) -> Result<Value, ArtError> {
        let cut = |s: &str, n: usize| -> String { s.trim().chars().take(n).collect() };
        let report = cut(&self.report, MAX_REPORT);
        if report.is_empty() && self.note.trim().is_empty() {
            return Err(ArtError::EmptyReport);
        }
        if self.photos.iter().any(|p| p.data_base64.len() > MAX_IMAGE_BASE64) {
            return Err(ArtError::LargeImage);
        }
        let photos: Vec<Value> = self.photos.iter().take(MAX_PHOTOS).map(|p| json!({ "mediaType": p.media_type, "data": p.data_base64 })).collect();
        Ok(json!({
            "consent": true,
            "check": self.check,
            "title": cut(&self.title, MAX_REPORT_TITLE),
            "report": report,
            "note": cut(&self.note, MAX_REPORT_NOTE),
            "photos": photos,
            "conversation": self.conversation.as_deref().map(|c| cut(c, MAX_REPORT_CONVERSATION)),
            "design": self.design.as_deref().map(|d| cut(d, MAX_DESIGN)),
            "app": cut(&self.app, 64),
        }))
    }
}

/// A report's draft from Claude's answer to [`REPORT_QUESTION`] → (title, the rest): its first
/// line is the title (without a "Title:" or heading mark).
pub fn split_report(answer: &str) -> (String, String) {
    let answer = answer.trim();
    let (first, rest) = answer.split_once('\n').unwrap_or((answer, ""));
    let title = first.trim().trim_start_matches('#').trim();
    let title = title.strip_prefix("Title:").or_else(|| title.strip_prefix("TITLE:")).unwrap_or(title).trim().trim_matches('*').trim();
    (title.chars().take(MAX_REPORT_TITLE).collect(), rest.trim().to_string())
}

/// The HTTP request for a check's conversation `messages`, with API key `key`.
pub fn request(key: &str, messages: &[Value]) -> Result<HttpRequest, ArtError> {
    request_with(key, system_prompt(), messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> ArtImage {
        ArtImage { media_type: "image/jpeg", data_base64: "/9j/4AAQ".into() }
    }

    #[test]
    fn a_report_carries_what_the_user_chose_and_no_more() {
        let r = Report {
            check: Some("0123abcd".into()),
            title: "  Gap above the bumper  ".into(),
            report: "The fill pulled in on knit.".into(),
            photos: vec![png()],
            app: "0.13.1".into(),
            ..Default::default()
        };
        let b = r.body().unwrap();
        assert_eq!(b["consent"], true);
        assert_eq!(b["title"], "Gap above the bumper");
        assert_eq!(b["photos"].as_array().unwrap().len(), 1);
        assert!(b["conversation"].is_null() && b["design"].is_null());
        assert!(Report::default().body().is_err(), "nothing to report");
        assert_eq!(split_report("Title: **Gaps on knit**\nThe fill pulled in."), ("Gaps on knit".to_string(), "The fill pulled in.".to_string()));
        assert_eq!(split_report("# Gaps\n\nMore"), ("Gaps".to_string(), "More".to_string()));
    }

    #[test]
    fn a_check_sends_the_photos_the_expected_look_the_design_and_the_answers() {
        let answers = Answers {
            machine: "Husqvarna Viking Designer Diamond deLuxe".into(),
            fabric: "Jersey knit (T-shirt)".into(),
            happened: vec!["the bobbin thread didn't cut".into()],
            notes: "  a gap above the bumper  ".into(),
            ..Default::default()
        };
        let expected = ArtImage { media_type: "image/png", data_base64: "iVBORw0KGgo=".into() };
        let m = first_message(&[png(), png()], Some(&expected), Some("Size 245 × 136 mm\n#5 satin"), &answers).unwrap();
        assert_eq!(m["role"], "user");
        let c = m["content"].as_array().unwrap();
        let kinds: Vec<&str> = c.iter().map(|b| b["type"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["text", "image", "image", "text", "image", "text", "text"]);
        assert_eq!(c[0]["text"], "My sew-out: 2 photos.");
        assert_eq!(c[4]["source"]["media_type"], "image/png");
        assert!(c[5]["text"].as_str().unwrap().contains("#5 satin"));
        let last = c[6]["text"].as_str().unwrap();
        assert!(last.contains("Fabric: Jersey knit (T-shirt)") && last.contains("the bobbin thread didn't cut"), "{last}");
        assert!(last.contains("My notes: a gap above the bumper") && !last.contains("Stabilizer:"), "{last}");
        // photos only, no design open: still a check
        let m = first_message(&[png()], None, None, &Answers::default()).unwrap();
        let c = m["content"].as_array().unwrap();
        assert_eq!(c.len(), 3);
        assert!(c[2]["text"].as_str().unwrap().contains("nothing went wrong that I noticed"));
        assert_eq!(first_message(&[], None, None, &Answers::default()), Err(ArtError::NoPhoto));
        let big = ArtImage { media_type: "image/jpeg", data_base64: "A".repeat(MAX_IMAGE_BASE64 + 1) };
        assert_eq!(first_message(&[big], None, None, &Answers::default()), Err(ArtError::LargeImage));
        // at most MAX_PHOTOS go
        let many = vec![png(); MAX_PHOTOS + 3];
        let m = first_message(&many, None, None, &Answers::default()).unwrap();
        assert_eq!(m["content"].as_array().unwrap().iter().filter(|b| b["type"] == "image").count(), MAX_PHOTOS);
    }

    #[test]
    fn the_check_has_its_own_instructions_and_asks_tools() {
        let q = first_message(&[png()], None, None, &Answers::default()).unwrap();
        let http = request("sk-ant-test", &[q]).unwrap();
        let body: Value = serde_json::from_str(&http.body).unwrap();
        let system = body["system"].as_str().unwrap();
        assert!(system.contains("sew-out") && system.contains("what the machine or") && system.contains(FIND), "{system}");
        let names: Vec<&str> = body["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, [FIND, RUN]);
    }
}
