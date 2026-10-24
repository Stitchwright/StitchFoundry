//! Updates (UPD-01): once a day the desktop app asks GitHub for StitchFoundry's releases
//! (`api.github.com/repos/Stitchwright/StitchFoundry/releases`, nothing about the user sent but the
//! request itself) and, when one is newer than the running version, the Home screen says so with a
//! Download button that opens the release page. A release candidate is offered only to someone
//! running one; drafts and the unsigned "Latest build" are never offered. Help › Check for Updates
//! asks at once; Help › Check for Updates Automatically turns the daily check off. The web app is
//! always the newest and doesn't ask.

use std::sync::mpsc::{Receiver, channel};

use serde_json::{Value, json};

use crate::VectorcraftApp;

/// Where the releases are listed (newest first) and where a release's page lives.
const RELEASES_API: &str = "https://api.github.com/repos/Stitchwright/StitchFoundry/releases?per_page=20";
const RELEASES_PAGE: &str = "https://github.com/Stitchwright/StitchFoundry/releases/tag/";
/// The most of the answer read (the API's list is a few tens of kilobytes).
const MAX_ANSWER: u64 = 4_000_000;
/// How long between automatic checks (seconds).
const EVERY: u64 = 24 * 60 * 60;

/// A newer release: its version and its page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Newer {
    pub version: String,
    pub url: String,
}

/// The check running, and what the last one found.
#[derive(Default)]
pub struct UpdateState {
    rx: Option<Receiver<Result<Option<Newer>, String>>>,
    /// The newest release when it is newer than this one.
    pub newer: Option<Newer>,
    /// Help › Check for Updates asked: the answer goes to the status bar, even "up to date".
    manual: bool,
    /// The automatic check ran (or was decided against) this run.
    tried: bool,
}

/// A version `major.minor.patch` with an optional `-rc.N`: a release sorts after its candidates.
fn parse(v: &str) -> Option<(u64, u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let (Some(Some(a)), Some(Some(b)), Some(Some(c)), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return None;
    };
    let rc = match pre {
        None => u64::MAX,
        Some(p) => p.strip_prefix("rc.")?.parse().ok()?,
    };
    Some((a, b, c, rc))
}

/// The newest release in GitHub's answer `body` newer than `current`, if any. Drafts, tags that
/// aren't versions (the Latest build) and, unless `current` is one, release candidates are skipped.
pub fn newest(body: &[u8], current: &str) -> Result<Option<Newer>, String> {
    let now = parse(current).ok_or_else(|| format!("this version ({current}) isn't one to compare"))?;
    let candidate = now.3 != u64::MAX;
    let list: Value = serde_json::from_slice(body).map_err(|_| "GitHub's answer couldn't be read".to_string())?;
    let releases = list.as_array().ok_or("GitHub's answer isn't a list of releases")?;
    let mut best: Option<((u64, u64, u64, u64), String)> = None;
    for r in releases.iter().take(100) {
        let Some(tag) = r["tag_name"].as_str() else { continue };
        let Some(v) = parse(tag) else { continue };
        if r["draft"].as_bool() == Some(true) || (v.3 != u64::MAX && !candidate) || v <= now {
            continue;
        }
        if best.as_ref().is_none_or(|(b, _)| v > *b) {
            best = Some((v, tag.to_string()));
        }
    }
    Ok(best.map(|(_, tag)| {
        // the page is built from the tag, never taken from the answer
        let safe: String = tag.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')).collect();
        Newer { version: safe.trim_start_matches('v').to_string(), url: format!("{RELEASES_PAGE}{safe}") }
    }))
}

/// Seconds since 1970 (0 when the clock can't be read).
fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Starts a check in the background (desktop only: the web has no `http_get`).
fn start(app: &mut VectorcraftApp) -> bool {
    let Some(get) = app.services.http_get.clone() else { return false };
    let current = env!("CARGO_PKG_VERSION").to_string();
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let r = vectorcraft_engine::guard::catch_panic(|| match get(RELEASES_API, MAX_ANSWER) {
            Ok((200, body)) => newest(&body, &current),
            Ok((status, _)) => Err(format!("GitHub answered {status}: try again later")),
            Err(e) => Err(format!("can't reach GitHub: {e}")),
        })
        .unwrap_or_else(|msg| Err(format!("internal error: {msg} (please report this bug)")));
        // Nothing waits for it once the app has quit.
        let _ = tx.send(r);
    });
    if spawned.is_err() {
        return false;
    }
    app.updates.rx = Some(rx);
    app.ui.update_checked_at = now_secs();
    true
}

/// Each frame: the daily check when it is due, and a check's answer when it comes.
pub fn poll(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.updates.tried && app.frame > 2 {
        app.updates.tried = true;
        if app.ui.check_updates && now_secs().saturating_sub(app.ui.update_checked_at) >= EVERY {
            start(app);
        }
    }
    let Some(rx) = &app.updates.rx else { return };
    let Ok(r) = rx.try_recv() else {
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        return;
    };
    app.updates.rx = None;
    let manual = std::mem::take(&mut app.updates.manual);
    match r {
        Ok(Some(n)) => {
            if manual {
                app.status(crate::i18n::fmt(tl!("StitchFoundry {version} is out: it's on the Home screen"), &[("version", &n.version)]));
                // asked for: shown even if skipped before
                app.ui.update_skipped.clear();
            }
            app.updates.newer = Some(n);
        }
        Ok(None) => {
            app.updates.newer = None;
            if manual {
                app.status(crate::i18n::fmt(tl!("StitchFoundry {version} is the newest"), &[("version", env!("CARGO_PKG_VERSION"))]));
            }
        }
        Err(e) => {
            if manual {
                app.status(e);
            }
        }
    }
}

/// `ui.checkForUpdates {automatic?}`: with `automatic`, turns the daily check on or off;
/// without, checks now.
pub fn run(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    if let Some(on) = p.get("automatic") {
        // the menu's item flips it
        let on = match on {
            Value::String(s) if s == "toggle" => !app.ui.check_updates,
            _ => on.as_bool().ok_or("`automatic` is true, false or \"toggle\"")?,
        };
        app.ui.check_updates = on;
        return Ok(json!({ "automatic": on }));
    }
    if app.updates.rx.is_some() {
        return Ok(json!({ "checking": true }));
    }
    if !start(app) {
        return Err("checking for updates works in the desktop app".into());
    }
    app.updates.manual = true;
    Ok(json!({ "checking": true }))
}

/// The Home screen's notice: a newer version, Download and Not Now.
pub fn home_card(app: &mut VectorcraftApp, ui: &mut egui::Ui, t: &crate::theme::Tokens) {
    let Some(n) = app.updates.newer.clone() else { return };
    if app.ui.update_skipped == n.version {
        return;
    }
    egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(1.0, t.accent)).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            let line = crate::i18n::fmt(tl!("StitchFoundry {version} is out."), &[("version", &n.version)]);
            ui.label(egui::RichText::new(line).font(crate::theme::semibold(14.0)).color(t.text));
            if crate::widgets::primary_button(ui, tl!("Download")).clicked() {
                app.open_url(&n.url);
            }
            if ui.link(tl!("Not now")).clicked() {
                app.ui.update_skipped = n.version.clone();
            }
        });
    });
    ui.add_space(14.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, draft: bool, pre: bool) -> Value {
        json!({ "tag_name": tag, "draft": draft, "prerelease": pre, "html_url": "https://evil.example/" })
    }

    #[test]
    fn only_a_newer_release_is_offered_and_candidates_only_to_candidates() {
        let list = json!([
            release("latest-build", false, true),
            release("v0.15.0-rc.1", false, true),
            release("v0.14.1", true, false),
            release("v0.14.0", false, false),
            release("v0.14.0-rc.1", false, true),
            release("v0.13.1-rc.2", false, true),
        ]);
        let body = serde_json::to_vec(&list).unwrap();
        // a release candidate sees the next candidate and the release; the draft is never offered
        let n = newest(&body, "0.14.0-rc.1").unwrap().unwrap();
        assert_eq!(n.version, "0.15.0-rc.1");
        assert_eq!(n.url, "https://github.com/Stitchwright/StitchFoundry/releases/tag/v0.15.0-rc.1", "built from the tag, not the answer");
        // a release sees only releases: 0.14.0 is the newest, so nothing for it
        assert_eq!(newest(&body, "0.14.0").unwrap(), None);
        assert_eq!(newest(&body, "0.13.0").unwrap().unwrap().version, "0.14.0");
        // a release sorts after its candidates
        assert!(parse("0.14.0") > parse("0.14.0-rc.9"));
        assert!(parse("0.14.0-rc.10") > parse("0.14.0-rc.9"));
        // junk answers are errors, never panics
        for bad in [&b"not json"[..], b"{}", b"[1, null, {\"tag_name\": 5}]"] {
            assert!(newest(bad, "0.14.0").map(|n| n.is_none()).unwrap_or(true));
        }
        assert!(newest(b"[]", "nonsense").is_err());
    }

    #[test]
    fn the_check_runs_in_the_background_and_the_home_card_offers_the_download() {
        let services = crate::Services {
            http_get: Some(std::sync::Arc::new(|url: &str, _: u64| {
                assert!(url.starts_with("https://api.github.com/repos/Stitchwright/StitchFoundry/releases"));
                Ok((200, serde_json::to_vec(&json!([release("v99.0.0", false, false)])).unwrap()))
            })),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), services);
        let ctx = egui::Context::default();
        assert_eq!(app.run("ui.checkForUpdates", json!({})).unwrap()["checking"], true);
        for _ in 0..200 {
            poll(&mut app, &ctx);
            if app.updates.newer.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(app.updates.newer.as_ref().map(|n| n.version.as_str()), Some("99.0.0"));
        assert!(app.ui.status.contains("99.0.0"), "{}", app.ui.status);
        // Not now hides it until the next version
        app.ui.update_skipped = "99.0.0".into();
        // the automatic check can be turned off, and the menu shows it
        assert_eq!(app.run("ui.checkForUpdates", json!({"automatic": false})).unwrap()["automatic"], false);
        assert!(!app.ui.check_updates);
        assert!(app.run("ui.checkForUpdates", json!({"automatic": "x"})).is_err());
        assert_eq!(app.run("ui.checkForUpdates", json!({"automatic": "toggle"})).unwrap()["automatic"], true);
        // the web (no http_get) doesn't check
        let mut web = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        assert!(web.run("ui.checkForUpdates", json!({})).is_err());
    }
}
