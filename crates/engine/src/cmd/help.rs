//! Help → project links (this app's GitHub repository: source code, issues and releases; the
//! commands return the URL; the frontend opens it) and the user guide (`docs/embroidery.md`,
//! compiled in): its topics, one topic's text, a search, and the topic for a tool (F1).

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, cmd, str_param};

/// This app's source repository.
pub const GITHUB_URL: &str = "https://github.com/Stitchwright/StitchFoundry";

/// Where bug reports go: StitchHub's report page, which files them as issues on this app's GitHub repository.
pub const BUG_REPORT_URL: &str = "https://hub.stitchbench.app/report-a-bug";

/// The user guide, compiled in for the Help window.
const GUIDE: &str = include_str!("../../../../docs/embroidery.md");

/// Most topics a search returns.
const MAX_FOUND: usize = 40;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "help.github", "StitchFoundry on GitHub", ["Help"], None, "{} → {url} source code, issues and releases", always, |_, _| url(GITHUB_URL.into())),
        cmd!(
            query "help.reportBug",
            "Report a Bug…",
            ["Help"],
            None,
            "{} → {url} StitchHub's bug report page, filled in with this app, its version and the system; a report becomes an issue on GitHub",
            always,
            |_, _| url(bug_report_url(env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH))
        ),
        cmd!(query "help.links", "Links", [], None, "{} → {github}", always, |_, _| Ok(json!({ "github": GITHUB_URL }))),
        cmd!(
            query "help.topics",
            "Help Topics",
            [],
            None,
            "{query?: words (topics whose title, then whose text, holds them all)} the user guide's topics, in order → {topics: [{id, title}]}",
            always,
            topics
        ),
        cmd!(query "help.topic", "Help Topic", [], None, "{id} one topic of the user guide → {id, title, text (Markdown)}", always, topic),
        cmd!(
            query "help.forTool",
            "Help for Tool",
            [],
            None,
            "{tool?: a tool id (the active tool)} the guide's topic about a tool, or Quick start → {id, title}",
            always,
            for_tool
        ),
    ]
}

/// A guide topic: its anchor id (as GitHub makes them), title and text.
struct Topic {
    id: String,
    title: String,
    text: &'static str,
}

/// The anchor GitHub gives a heading: lower case, spaces as hyphens, other punctuation left out.
fn slug(title: &str) -> String {
    title
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c.to_lowercase().next().unwrap_or(c)),
            _ => None,
        })
        .collect()
}

/// The guide's topics: the part before the first `## ` heading ("overview"), then one per
/// `## ` heading.
fn guide() -> Vec<Topic> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut title = String::from("Overview");
    let mut id = String::from("overview");
    let mut at = 0;
    for line in GUIDE.split_inclusive('\n') {
        if let Some(h) = line.strip_prefix("## ") {
            if let Some(text) = GUIDE.get(start..at) {
                out.push(Topic { id: std::mem::take(&mut id), title: std::mem::take(&mut title), text });
            }
            title = h.trim().to_string();
            id = slug(&title);
            start = at + line.len();
        }
        at += line.len();
    }
    if let Some(text) = GUIDE.get(start..) {
        out.push(Topic { id, title, text });
    }
    out
}

fn topics(_: &mut Session, p: &Value) -> Result<Value> {
    let all = guide();
    let words: Vec<String> = str_param(p, "query").unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
    let list: Vec<&Topic> = if words.is_empty() {
        all.iter().collect()
    } else {
        let has = |t: &str| {
            let t = t.to_lowercase();
            words.iter().all(|w| t.contains(w.as_str()))
        };
        let mut found: Vec<&Topic> = all.iter().filter(|t| has(&t.title)).collect();
        found.extend(all.iter().filter(|t| !has(&t.title) && has(t.text)));
        found.truncate(MAX_FOUND);
        found
    };
    Ok(json!({ "topics": list.iter().map(|t| json!({ "id": t.id, "title": t.title })).collect::<Vec<_>>() }))
}

fn topic(_: &mut Session, p: &Value) -> Result<Value> {
    let id = str_param(p, "id").ok_or_else(|| bad("help.topic", "missing `id`"))?;
    let all = guide();
    let t = all.iter().find(|t| t.id == id).ok_or_else(|| bad("help.topic", "no such topic: see help.topics"))?;
    Ok(json!({ "id": t.id, "title": t.title, "text": t.text.trim() }))
}

/// The topic for each tool with its own, by tool id.
const TOOL_TOPICS: &[(&str, &str)] = &[
    ("embroideryBrush", "the-digitizing-widget-and-brush"),
    ("embroideryDirection", "stitch-direction-automatic-then-editable"),
    ("embroideryStitches", "editing-single-stitches"),
    ("embroideryColumn", "stitch-types"),
    ("embroideryMotif", "stitch-types"),
    ("embroideryClick", "click-to-stitch"),
    ("embroideryCutout", "auto-digitize"),
    ("embroideryLasso", "click-to-stitch"),
    ("embroideryRegion", "fill-region"),
    ("embroideryCrossStitch", "cross-stitch-patterns"),
    ("type", "lettering"),
    ("areaType", "lettering"),
    ("typeOnPath", "lettering"),
];

fn for_tool(s: &mut Session, p: &Value) -> Result<Value> {
    let tool = str_param(p, "tool").map(str::to_string).unwrap_or_else(|| s.tool_id().to_string());
    let id = TOOL_TOPICS.iter().find(|(t, _)| *t == tool).map_or("quick-start", |(_, id)| id);
    let all = guide();
    let t = all.iter().find(|t| t.id == id).or_else(|| all.first()).ok_or_else(|| bad("help.forTool", "the guide is empty"))?;
    Ok(json!({ "id": t.id, "title": t.title }))
}

/// The report page's address with the program, version and system in it (the page reads them; nothing else is sent).
fn bug_report_url(version: &str, os: &str, arch: &str) -> String {
    format!("{BUG_REPORT_URL}?program=stitchfoundry&version={}&system={}", percent_encode(version), percent_encode(&format!("{os} {arch}")))
}

/// Percent-encodes everything but letters, digits and `-._~`.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn url(u: String) -> Result<Value> {
    Ok(json!({ "url": u }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Session;

    #[test]
    fn links() {
        let mut s = Session::new();
        assert_eq!(s.execute("help.github", &json!({})).unwrap()["url"], GITHUB_URL);
        assert_eq!(s.execute("help.links", &json!({})).unwrap()["github"], GITHUB_URL);
        // VectorCraft's community links (its Discord and website) are not this app's
        assert!(s.execute("help.discord", &json!({})).is_err());
        assert!(s.execute("help.website", &json!({})).is_err());
    }

    #[test]
    fn report_a_bug_opens_the_hub_page_with_the_version_and_system() {
        let mut s = Session::new();
        let u = s.execute("help.reportBug", &json!({})).unwrap()["url"].as_str().unwrap().to_string();
        assert!(u.starts_with("https://hub.stitchbench.app/report-a-bug?program=stitchfoundry&version="), "{u}");
        assert!(u.contains(&format!("version={}", env!("CARGO_PKG_VERSION"))), "{u}");
        assert!(u.contains("&system="), "{u}");
        assert_eq!(
            bug_report_url("0.3.1 beta", "mac os", "aarch64"),
            "https://hub.stitchbench.app/report-a-bug?program=stitchfoundry&version=0.3.1%20beta&system=mac%20os%20aarch64"
        );
        // Anything odd in the text can't add a parameter or leave the address.
        assert_eq!(percent_encode("a&b=c#d é"), "a%26b%3Dc%23d%20%C3%A9");
    }

    #[test]
    fn the_guide_is_in_topics_found_by_words_and_by_tool() {
        let mut s = Session::new();
        let all = s.execute("help.topics", &json!({})).unwrap();
        let topics = all["topics"].as_array().unwrap();
        assert!(topics.len() > 30);
        assert_eq!(topics[0]["id"], "overview");
        assert!(topics.iter().any(|t| t["id"] == "quick-start" && t["title"] == "Quick start"));
        // every topic opens, its text without its heading
        for t in topics {
            let one = s.execute("help.topic", &json!({"id": t["id"]})).unwrap();
            assert_eq!(one["title"], t["title"]);
            assert!(!one["text"].as_str().unwrap().starts_with("## "));
        }
        // the guide's own links name topics that exist
        let quick = s.execute("help.topic", &json!({"id": "overview"})).unwrap();
        for link in quick["text"].as_str().unwrap().split("](#").skip(1) {
            let id = link.split(')').next().unwrap();
            assert!(topics.iter().any(|t| t["id"] == id), "{id}");
        }
        // search: titles first
        let found = s.execute("help.topics", &json!({"query": "appliqué"})).unwrap();
        let found = found["topics"].as_array().unwrap();
        assert!(found[0]["title"].as_str().unwrap().to_lowercase().contains("appliqué"), "{found:?}");
        assert!(s.execute("help.topics", &json!({"query": "zzzqqq"})).unwrap()["topics"].as_array().unwrap().is_empty());
        assert!(s.execute("help.topic", &json!({"id": "nope"})).is_err());
        // F1: a tool's topic, else Quick start
        assert_eq!(s.execute("help.forTool", &json!({"tool": "embroideryClick"})).unwrap()["id"], "click-to-stitch");
        assert_eq!(s.execute("help.forTool", &json!({"tool": "pen"})).unwrap()["id"], "quick-start");
        for (_, id) in TOOL_TOPICS {
            assert!(topics.iter().any(|t| t["id"] == *id), "{id}");
        }
    }
}
