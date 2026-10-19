//! Check My Sew-out through the service: photos of a sew-out looked at with the service's own
//! instructions and tools ([`sf_core::ai::sewout`]), so a free check can't pay for anything else.
//! Each account gets [`Config::free_checks`] checks a month free, then a check costs
//! [`Config::check_credits`] whatever it uses; a check is its first question (the photos) and up to
//! [`MAX_ROUNDS`] requests after it within a day, then requests are paid as any other.
//!
//! With the user's leave (`x-stitchfoundry-share: 1`) a check is kept to improve StitchFoundry:
//! its photos, what the app said about the design, Claude's answers and the changes it made, in
//! the store ([`crate::store::Store::keep_shared`]); `x-stitchfoundry-follow-up: <check>` ties a
//! later check ("did the fix work?") to it. Shared checks are private to the user and the project
//! and are deleted on request.

use serde_json::{Value, json};

/// Requests a check covers (its first question, the rounds of commands and follow-up questions).
pub const MAX_ROUNDS: u32 = 16;
/// How long a check lasts (seconds).
pub const CHECK_LIFE: u64 = 24 * 3600;
/// The most images a check's first question may hold (the photos and the expected look).
pub const MAX_IMAGES: usize = 7;

/// The year and month (`"2026-10"`) of unix time `t`, to count the month's free checks.
pub fn month(t: u64) -> String {
    // days to civil date (Howard Hinnant's algorithm)
    let z = (t / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}")
}

/// Whether `id` is a check id as the app makes them (8 to 32 hex digits; it names the check's
/// requests, which the service keeps apart by account).
pub fn good_id(id: &str) -> bool {
    (8..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Why `body` isn't a sew-out check, if it isn't: its first message must be the user's, with one to
/// [`MAX_IMAGES`] images.
pub fn not_a_check(body: &Value) -> Option<&'static str> {
    let Some(first) = body.get("messages").and_then(Value::as_array).and_then(|m| m.first()) else { return Some("a check starts with the photos") };
    if first.get("role").and_then(Value::as_str) != Some("user") {
        return Some("a check starts with the photos");
    }
    let images = first
        .get("content")
        .and_then(Value::as_array)
        .map_or(0, |c| c.iter().filter(|b| b.get("type").and_then(Value::as_str) == Some("image")).count());
    match images {
        0 => Some("a check needs a photo of the sew-out"),
        n if n > MAX_IMAGES => Some("too many photos in one check"),
        _ => None,
    }
}

/// The request with the service's own instructions and tools (whatever the app sent for them).
pub fn with_own_instructions(mut body: Value) -> Value {
    if let Some(o) = body.as_object_mut() {
        o.insert("system".into(), json!(sf_core::ai::sewout::system_prompt()));
        o.insert("tools".into(), sf_core::ai::ask::tools());
        o.remove("tool_choice");
    }
    body
}

/// What a shared check keeps of one round: the photos (from its first message; kept once), and a
/// line with the newest question's texts, the answer's text and the commands it asked to run.
pub fn shared_round(body: &Value, answer: &str, account: u64, follow_up: Option<&str>, now: u64) -> (Vec<(String, Vec<u8>)>, String) {
    let messages = body.get("messages").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut files = Vec::new();
    for b in messages.first().and_then(|m| m.get("content")).and_then(Value::as_array).into_iter().flatten() {
        if b.get("type").and_then(Value::as_str) != Some("image") {
            continue;
        }
        let ext = if b.pointer("/source/media_type").and_then(Value::as_str) == Some("image/png") { "png" } else { "jpg" };
        if let Some(bytes) = b.pointer("/source/data").and_then(Value::as_str).and_then(base64_decode) {
            files.push((format!("image-{}.{ext}", files.len() + 1), bytes));
        }
    }
    let texts = |m: &Value| -> Vec<String> {
        m.get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|b| b.get("text").and_then(Value::as_str).map(str::to_string))
            .collect()
    };
    let asked = messages.last().map(&texts).unwrap_or_default();
    let reply: Value = serde_json::from_str(answer).unwrap_or(Value::Null);
    let said = texts(&reply);
    let commands: Vec<Value> = reply
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|b| b.get("input").cloned())
        .collect();
    let line = json!({ "at": now, "account": account, "followUpOf": follow_up, "round": messages.len(), "asked": asked, "said": said, "commands": commands });
    (files, line.to_string())
}

/// The most reports an account may send a day.
pub const MAX_REPORTS_A_DAY: usize = 5;
/// The largest photo a report may hold (bytes, decoded).
pub const MAX_REPORT_PHOTO: usize = 2 * 1024 * 1024;

/// A sew-out report as sent (`POST /v1/report`, Report to StitchFoundry in the app): what the
/// user agreed to send, checked and cut to size.
#[derive(Debug, PartialEq)]
pub struct Report {
    pub check: Option<String>,
    pub title: String,
    pub report: String,
    pub note: String,
    /// (file name, bytes)
    pub photos: Vec<(String, Vec<u8>)>,
    pub conversation: String,
    pub design: String,
    pub app: String,
}

/// The report in `v`, or why it isn't one.
pub fn read_report(v: &Value) -> Result<Report, &'static str> {
    use sf_core::ai::sewout as limits;
    if v.get("consent").and_then(Value::as_bool) != Some(true) {
        return Err("a report is sent only with the user's leave (consent)");
    }
    let text = |key: &str, max: usize| -> String { v.get(key).and_then(Value::as_str).unwrap_or_default().trim().chars().take(max).collect() };
    let report = Report {
        check: v.get("check").and_then(Value::as_str).filter(|c| good_id(c)).map(str::to_string),
        title: text("title", limits::MAX_REPORT_TITLE).replace(['\r', '\n'], " "),
        report: text("report", limits::MAX_REPORT),
        note: text("note", limits::MAX_REPORT_NOTE),
        photos: Vec::new(),
        conversation: text("conversation", limits::MAX_REPORT_CONVERSATION),
        design: text("design", sf_core::ai::ask::MAX_DESIGN),
        app: text("app", 64),
    };
    if report.report.is_empty() && report.note.is_empty() {
        return Err("a report needs what went wrong");
    }
    let mut photos = Vec::new();
    for p in v.get("photos").and_then(Value::as_array).into_iter().flatten() {
        if photos.len() >= limits::MAX_PHOTOS {
            return Err("too many photos in one report");
        }
        let ext = match p.get("mediaType").and_then(Value::as_str) {
            Some("image/png") => "png",
            Some("image/jpeg") => "jpg",
            _ => return Err("a report's photos are JPEG or PNG"),
        };
        let bytes = p.get("data").and_then(Value::as_str).and_then(base64_decode).ok_or("a photo can't be read")?;
        if bytes.is_empty() || bytes.len() > MAX_REPORT_PHOTO {
            return Err("a photo is too large");
        }
        photos.push((format!("photo-{}.{ext}", photos.len() + 1), bytes));
    }
    Ok(Report { photos, ..report })
}

impl Report {
    /// The title, or one made from the report.
    pub fn subject(&self) -> String {
        let t = if self.title.is_empty() { self.report.lines().next().unwrap_or("Sew-out report").to_string() } else { self.title.clone() };
        let t: String = t.chars().take(120).collect();
        format!("Sew-out report: {t}")
    }

    /// What is kept beside the photos: a JSON line.
    pub fn kept(&self, id: &str, account: u64, email: &str, now: u64) -> String {
        json!({
            "id": id, "at": now, "account": account, "email": email, "check": self.check, "app": self.app,
            "title": self.title, "report": self.report, "note": self.note, "conversation": self.conversation, "design": self.design,
            "photos": self.photos.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        })
        .to_string()
    }

    /// The email to the owner (Resend's `POST /emails` body), the photos attached; replies go to
    /// the user.
    pub fn email(&self, from: &str, to: &str, id: &str, email: &str) -> Value {
        let mut text = format!("Report {id} from {email} (StitchFoundry {})\n\n", if self.app.is_empty() { "?" } else { &self.app });
        for (heading, body) in
            [("Report", &self.report), ("Their note", &self.note), ("The check's conversation", &self.conversation), ("The design", &self.design)]
        {
            if !body.is_empty() {
                text.push_str(&format!("== {heading} ==\n{body}\n\n"));
            }
        }
        text.push_str(
            "Sent with their leave to improve StitchFoundry: keep it private, never share or publish it. They can have it deleted at any time.\n",
        );
        let attachments: Vec<Value> = self.photos.iter().map(|(name, bytes)| json!({ "filename": name, "content": base64_encode(bytes) })).collect();
        json!({ "from": from, "to": [to], "reply_to": email, "subject": self.subject(), "text": text, "attachments": attachments })
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace() && *b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            acc |= val(c)? << (18 - 6 * i);
        }
        let n = chunk.len();
        if n < 2 {
            return None;
        }
        out.push((acc >> 16) as u8);
        if n > 2 {
            out.push((acc >> 8) as u8);
        }
        if n > 3 {
            out.push(acc as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_and_check_ids_and_base64() {
        assert_eq!(month(0), "1970-01");
        assert_eq!(month(1_792_281_600), "2026-10"); // 2026-10-18
        assert_eq!(month(1_798_761_599), "2026-12");
        assert_eq!(month(1_798_761_600), "2027-01");
        assert!(good_id("0123abcd") && good_id("ff00ff00ff00ff00ff00ff00ff00ff00"));
        assert!(!good_id("short") && !good_id("../etc/passwd!") && !good_id(&"a".repeat(33)));
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
    }

    #[test]
    fn only_photos_make_a_check_and_its_instructions_are_the_services() {
        let photo = json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "aGVsbG8="}});
        let ok = json!({"messages": [{"role": "user", "content": [photo, {"type": "text", "text": "what went wrong?"}]}], "system": "draw me a cat", "tools": []});
        assert_eq!(not_a_check(&ok), None);
        assert!(not_a_check(&json!({"messages": [{"role": "user", "content": [{"type": "text", "text": "draw a cat"}]}]})).is_some());
        let own = with_own_instructions(ok);
        assert!(own["system"].as_str().unwrap().contains("sew-out"));
        assert_eq!(own["tools"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_report_needs_leave_and_words_and_mails_its_photos() {
        let photo = json!({"mediaType": "image/jpeg", "data": "aGVsbG8="});
        let ok = json!({"consent": true, "title": "Gaps\non knit", "report": "The fill pulled in.", "photos": [photo], "app": "0.13.1"});
        let r = read_report(&ok).unwrap();
        assert_eq!(r.title, "Gaps on knit");
        assert_eq!(r.photos, vec![("photo-1.jpg".to_string(), b"hello".to_vec())]);
        assert!(read_report(&json!({"report": "x"})).is_err(), "no leave");
        assert!(read_report(&json!({"consent": true})).is_err(), "nothing said");
        assert!(read_report(&json!({"consent": true, "report": "x", "photos": [{"mediaType": "image/gif", "data": "aGk="}]})).is_err());
        let mail = r.email("StitchFoundry <reports@stitchfoundry.app>", "support@stitchfoundry.app", "r1", "denise@example.com");
        assert_eq!(mail["subject"], "Sew-out report: Gaps on knit");
        assert_eq!(mail["reply_to"], "denise@example.com");
        assert_eq!(mail["attachments"][0]["content"], "aGVsbG8=");
        assert!(mail["text"].as_str().unwrap().contains("The fill pulled in."));
        assert_eq!(base64_encode(b"hi"), "aGk=");
    }

    #[test]
    fn a_shared_round_keeps_the_photos_the_answer_and_the_commands() {
        let photo = json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": "aGVsbG8="}});
        let body = json!({"messages": [{"role": "user", "content": [photo, {"type": "text", "text": "Fabric: Jersey knit"}]}]});
        let answer = json!({"content": [{"type": "text", "text": "Overlap the shapes 1 mm."}, {"type": "tool_use", "id": "t", "name": "run_command", "input": {"command": "embroidery.set"}}]}).to_string();
        let (files, line) = shared_round(&body, &answer, 7, Some("abc"), 100);
        assert_eq!(files, vec![("image-1.jpg".to_string(), b"hello".to_vec())]);
        assert!(
            line.contains("Overlap the shapes") && line.contains("embroidery.set") && line.contains("Jersey knit") && line.contains("abc"),
            "{line}"
        );
    }
}
