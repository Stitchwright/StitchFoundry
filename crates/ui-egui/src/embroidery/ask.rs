//! Ask About Embroidery (Help and Embroidery menus): a window where the user asks Claude about
//! machine embroidery or about StitchFoundry, with their own Anthropic API key (the one Design
//! from Prompt keeps). Unless the user turns it off, a summary of the design open now goes with
//! each question ([`design_summary`]), and Claude can look at the design and change it through
//! StitchFoundry's commands ([`ask::tools`]): commands that only read run at once ([`access`]);
//! changes wait for the user's Allow ([`decide`]); files, preferences, plug-ins and windows are
//! out of its reach. Each request goes out on a thread of its own through the host's `http_post`
//! service; [`poll`] picks the reply up and runs the round of commands it asks for.

use std::sync::mpsc::{Receiver, TryRecvError};

use egui::RichText;
use serde_json::{Value, json};
use vectorcraft_embroidery::MM_PER_PT;
use vectorcraft_embroidery::sf_core::ai::ArtError;
use vectorcraft_embroidery::sf_core::ai::ask::{self, Reply, ToolCall, ToolResult};
use vectorcraft_embroidery::sf_core::ai::{cloud, sewout};
use vectorcraft_engine::CommandSpec;

use super::prompt;
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The most objects the summary lists.
const MAX_LISTED: usize = 40;
/// The most Check Design findings the summary lists.
const MAX_FINDINGS: usize = 12;
/// The most commands one search returns.
const MAX_FOUND: usize = 20;

/// What the window shows of the conversation.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Question(String),
    Answer(String),
    /// A change to the design, made or declined, or a command that couldn't run.
    Note(String),
}

/// What a conversation is: questions, or a sew-out check and the questions after it (Check My
/// Sew-out: [`super::sewout`], its own instructions).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Ask,
    SewOut,
}

/// A sew-out check's headers for the StitchFoundry service: its id (the app makes it, the service
/// keeps the check's requests together by it), shared or not, and the check it follows up.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckHeaders {
    pub id: String,
    pub share: bool,
    pub follow_up: Option<String>,
}

/// Ask About Embroidery between frames. Not saved: a conversation ends with the app.
pub struct AskState {
    pub open: bool,
    pub mode: Mode,
    /// A sew-out check's id and what goes with it to the StitchFoundry service.
    pub check: Option<CheckHeaders>,
    /// The question being typed.
    pub input: String,
    /// The conversation as shown.
    pub entries: Vec<Entry>,
    /// The conversation as the API's messages, only ever added to (but for a question that got
    /// no answer, taken back whole).
    messages: Vec<Value>,
    /// Send a summary of the design open now with each question.
    pub include_design: bool,
    /// Keep the API key once Anthropic takes it.
    pub remember: bool,
    /// Show the API key row (it shows by itself while there is no key).
    pub show_key: bool,
    /// The question being answered.
    current: Option<Current>,
    /// The request on its way.
    pending: Option<Pending>,
    /// Changes waiting for the user's Allow.
    pub approval: Option<Approval>,
    /// Why the last question got no answer.
    pub error: Option<String>,
}

impl Default for AskState {
    fn default() -> Self {
        Self {
            open: false,
            mode: Mode::Ask,
            check: None,
            input: String::new(),
            entries: Vec::new(),
            messages: Vec::new(),
            include_design: true,
            remember: true,
            show_key: false,
            current: None,
            pending: None,
            approval: None,
            error: None,
        }
    }
}

struct Current {
    question: String,
    /// The conversation's length before the question (taken back to it when no answer comes).
    start: usize,
    /// The requests sent for it so far.
    rounds: usize,
    /// The key sent, kept once Anthropic takes it.
    keep: Option<String>,
}

struct Pending {
    rx: Receiver<Result<Reply, ArtError>>,
    /// When it was sent (egui time, seconds).
    since: f64,
}

/// A round of tool calls waiting for the user: the changes to allow, and the results of the
/// calls that already ran.
pub struct Approval {
    /// The changes asked for: (call, command id, params, what the window shows).
    pub changes: Vec<(String, String, Value, String)>,
    /// The results so far, in the order of the reply's calls; `None` for the changes.
    results: Vec<(String, Option<Result<String, String>>)>,
}

impl AskState {
    /// Claude's last answer in the conversation.
    pub fn last_answer(&self) -> Option<&str> {
        self.entries.iter().rev().find_map(|e| if let Entry::Answer(a) = e { Some(a.as_str()) } else { None })
    }

    /// The conversation as text (questions, answers and notes), for a report.
    pub fn transcript(&self) -> String {
        let lines: Vec<String> = self
            .entries
            .iter()
            .map(|e| match e {
                Entry::Question(q) => format!("Me: {q}"),
                Entry::Answer(a) => format!("Claude: {a}"),
                Entry::Note(n) => format!("({n})"),
            })
            .collect();
        lines.join("\n\n")
    }

    /// Whether a question is being answered (sent, or waiting for the user's Allow).
    pub fn asking(&self) -> bool {
        self.current.is_some()
    }

    /// Seconds since the request on its way was sent.
    pub fn waiting_for(&self, now: f64) -> Option<f64> {
        self.pending.as_ref().map(|p| (now - p.since).max(0.0))
    }
}

/// What Claude may do with a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// It only reads: it runs at once.
    Read,
    /// It changes the design: it waits for the user's Allow.
    Change,
    /// Out of reach: files, preferences, plug-ins, windows and the like.
    Blocked,
}

/// Command families Claude can't run: they reach outside the design (files, the machine folder,
/// preferences, plug-ins, libraries, the clipboard) or the app's windows.
const BLOCKED: &[&str] = &[
    "file.",
    "export.",
    "print.",
    "prefs.",
    "plugin.",
    "script.",
    "links.",
    "library.",
    "clipboard.",
    "pdf.",
    "webExport.",
    "flattener.",
    "shortcutSet.",
    "dataset.",
    "help.",
    "ui.",
    "app.",
    "window.",
    "command.",
    "document.export",
    "document.save",
    "document.serialize",
    "document.json",
    "text.addFontFiles",
    "text.findFontFiles",
    "embroidery.send",
    "embroidery.export",
    "embroidery.queue",
    "embroidery.jobs",
    "embroidery.library",
    "embroidery.output",
    "embroidery.font.save",
    "embroidery.fontDoc.save",
    "quilt.export",
];

/// Endings of commands that only read.
const READS: &[&str] = &[
    ".info",
    ".list",
    ".get",
    ".check",
    ".node",
    ".find",
    ".inspect",
    ".current",
    ".stitches",
    ".hoops",
    ".charts",
    ".samples",
    ".cost",
    ".plan",
    ".fonts",
    ".fontList",
    ".count",
];

/// What Claude may do with `spec`.
pub fn access(spec: &CommandSpec) -> Access {
    let id = spec.id;
    if blocked(id) {
        return Access::Blocked;
    }
    // selecting, and queries (not journalled) that read; a query that sets something still waits
    // for the user
    let reads = id.starts_with("select.") || (!spec.journal && READS.iter().any(|e| id.ends_with(e)));
    if reads { Access::Read } else { Access::Change }
}

/// Whether command `id` is out of Claude's reach (whether or not the engine has it: the file
/// commands of the menus are the app's own).
pub fn blocked(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    let reaches_out = ["export", ".save", ".import", ".delete", ".install", "folder", "presets"].iter().any(|w| lower.contains(w));
    reaches_out || BLOCKED.iter().any(|p| id.starts_with(p))
}

/// A command's failure as Claude is told it (in English: it isn't shown to the user).
fn to_claude(why: String) -> Result<String, String> {
    Err(why)
}

/// Whether `params` name a file or folder (never run from here, whatever the command).
fn names_a_file(params: &Value) -> bool {
    params.as_object().is_some_and(|o| o.keys().any(|k| matches!(k.as_str(), "path" | "paths" | "folder" | "file" | "files" | "dir" | "url")))
}

/// Open the window (`ui.askAboutEmbroidery`); with `{question}`, ask it at once.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    if !prompt::available(app) {
        return Err("Ask About Embroidery needs the desktop app for now".into());
    }
    prompt::load_key(app);
    app.embroidery.ask.open = true;
    let mut asking = false;
    if let Some(q) = p.get("question").and_then(Value::as_str).filter(|q| !q.trim().is_empty()) {
        app.embroidery.ask.input = q.to_string();
        send(app)?;
        asking = true;
    }
    Ok(json!({ "open": true, "asking": asking }))
}

/// A plain-text summary of the design open now, for the model: its size, stitches, threads,
/// hoop, machine, what Check Design finds and its objects in sewing order with their ids (the
/// selected ones marked). `None` without a document.
pub fn design_summary(app: &mut VectorcraftApp) -> Option<String> {
    app.session.active()?;
    let info = app.session.execute("embroidery.info", &json!({})).ok()?;
    let num = |k: &str| info.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let text = |k: &str| info.get(k).and_then(Value::as_str).map(str::to_string);
    let mut lines = Vec::new();
    if num("stitches") > 0.0 {
        lines.push(format!(
            "Size {:.0} × {:.0} mm; {} stitched objects; {} stitches; {} trims, {} jumps, {} colour changes, {} stops; about {} to sew.",
            num("widthMm"),
            num("heightMm"),
            num("objects"),
            num("stitches"),
            num("trims"),
            num("jumps"),
            num("colorChanges"),
            num("stops"),
            text("time").unwrap_or_default(),
        ));
        let threads: Vec<String> = info
            .get("colors")
            .and_then(Value::as_array)
            .map(|cs| {
                cs.iter()
                    .map(|c| {
                        let name = c.get("name").and_then(Value::as_str).filter(|n| !n.is_empty());
                        let hex = c.get("color").and_then(Value::as_str).unwrap_or("?");
                        name.map_or_else(|| hex.to_string(), |n| format!("{n} ({hex})"))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if !threads.is_empty() {
            lines.push(format!("Threads in sewing order: {}.", threads.join(", ")));
        }
    } else {
        lines.push("Nothing is stitched yet.".to_string());
    }
    let mut setup = Vec::new();
    if let Some(h) = text("hoop") {
        let fits = if info.get("fits").and_then(Value::as_bool) == Some(false) { ", and the design does not fit it" } else { "" };
        setup.push(format!("hoop {h}{fits}"));
    }
    if let Some(f) = text("fabric") {
        setup.push(format!("fabric {f}"));
    }
    if let Some(m) = text("machine") {
        setup.push(format!("machine {m}"));
    }
    if let Some(f) = text("machineFormat") {
        setup.push(format!("saves as {f}"));
    }
    if !setup.is_empty() {
        lines.push(format!("Set up: {}.", setup.join("; ")));
    }
    if let Ok(check) = app.session.execute("embroidery.check", &json!({})) {
        let findings: Vec<String> = check
            .get("findings")
            .and_then(Value::as_array)
            .map(|fs| fs.iter().filter_map(|f| f.get("message").and_then(Value::as_str)).take(MAX_FINDINGS).map(|m| format!("- {m}")).collect())
            .unwrap_or_default();
        if findings.is_empty() {
            lines.push("Check Design finds nothing.".to_string());
        } else {
            lines.push("Check Design finds:".to_string());
            lines.extend(findings);
        }
    }
    // the stitched objects in sewing order, then any other selected object
    let sewn: Vec<vectorcraft_doc::NodeId> = app.session.embroidery_plan().map(|p| p.nodes.clone()).unwrap_or_default();
    let st = app.session.active()?;
    let selected = &st.selection.objects;
    let mut ids = sewn.clone();
    ids.extend(selected.iter().filter(|id| !sewn.contains(id)));
    let mut seen = std::collections::HashSet::new();
    let listed: Vec<String> = ids
        .iter()
        .filter(|id| seen.insert(**id))
        .filter_map(|id| st.doc.node(*id).map(|n| (id, n)))
        .take(MAX_LISTED)
        .map(|(id, n)| {
            let stitch = n.embroidery.as_deref().map_or("no stitches", |e| e.stitch.label());
            let size = n.geometric_bounds().map(|b| format!(", {:.0} × {:.0} mm", b.width() * MM_PER_PT, b.height() * MM_PER_PT)).unwrap_or_default();
            let mark = if selected.contains(id) { " (selected)" } else { "" };
            format!("- #{} {}: {stitch}{size}{mark}", id.0, n.display_name())
        })
        .collect();
    if !listed.is_empty() {
        lines.push(format!("Objects in sewing order ({} selected):", selected.len()));
        lines.extend(listed);
    }
    Some(lines.join("\n"))
}

/// The commands matching `query`'s words, best first, as find_commands returns them.
pub fn find(app: &VectorcraftApp, query: &str) -> Value {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut found: Vec<(usize, &CommandSpec)> = vectorcraft_engine::command_specs()
        .iter()
        .filter(|c| !vectorcraft_engine::cmd::is_alias(c.id) && access(c) != Access::Blocked)
        .filter_map(|c| {
            let hay = format!("{} {} {}", c.id, c.label, c.menu.join(" ")).to_lowercase();
            let score = words.iter().filter(|w| hay.contains(w.as_str())).count();
            (score > 0).then_some((score, c))
        })
        .collect();
    // most words matched first, in the registry's order otherwise
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    let list: Vec<Value> = found
        .into_iter()
        .take(MAX_FOUND)
        .map(|(_, c)| {
            let info = c.info(&app.session);
            json!({
                "id": c.id,
                "label": c.label,
                "menu": c.menu.join(" › "),
                "canRunNow": info.enabled,
                "whyNot": info.disabled_reason,
                "changesTheDesign": access(c) == Access::Change,
                "params": c.params,
            })
        })
        .collect();
    json!(list)
}

/// How a command shows in the window: its label and menu.
fn describe(spec: &CommandSpec) -> String {
    let menu: Vec<&str> = spec.menu.iter().copied().filter(|m| !m.is_empty()).collect();
    if menu.is_empty() { spec.label.to_string() } else { format!("{} › {}", menu.join(" › "), spec.label) }
}

/// Send the question typed (Ask). It stays in the box until the answer comes.
pub fn send(app: &mut VectorcraftApp) -> Result<(), String> {
    if app.embroidery.ask.asking() {
        return Err("already asking: wait for the answer".into());
    }
    let fail = |app: &mut VectorcraftApp, e: ArtError| {
        let m = e.to_string();
        app.embroidery.ask.show_key |= matches!(e, ArtError::NoKey | ArtError::BadKey);
        app.embroidery.ask.error = Some(m.clone());
        Err(m)
    };
    let asked = app.embroidery.ask.entries.iter().filter(|e| matches!(e, Entry::Question(_))).count();
    if asked >= ask::MAX_QUESTIONS {
        let m = "This conversation is long: start a New Conversation.".to_string();
        app.embroidery.ask.error = Some(m.clone());
        return Err(m);
    }
    if let Err(e) = vectorcraft_embroidery::sf_core::ai::clean_key(&super::account::key_to_send(app)) {
        return fail(app, e);
    }
    let design = if app.embroidery.ask.include_design { design_summary(app) } else { None };
    let message = match ask::question(&app.embroidery.ask.input, design.as_deref()) {
        Ok(m) => m,
        Err(e) => return fail(app, e),
    };
    let credits = super::account::paying_with_credits(app);
    let keep = (app.embroidery.ask.remember && !credits).then(|| app.embroidery.prompt.key.trim().to_string());
    if keep.is_none() && !credits {
        prompt::keep_key(app.services.secrets.as_deref(), None);
    }
    let st = &mut app.embroidery.ask;
    let question = st.input.trim().to_string();
    st.current = Some(Current { question: question.clone(), start: st.messages.len(), rounds: 0, keep });
    st.messages.push(message);
    st.entries.push(Entry::Question(question));
    st.error = None;
    request(app)
}

/// Ask `question` in the conversation, shown in it as `shown` (the words typed in the box stay
/// there).
pub fn ask_as(app: &mut VectorcraftApp, question: &str, shown: &str) -> Result<(), String> {
    let typed = std::mem::replace(&mut app.embroidery.ask.input, question.to_string());
    let r = send(app);
    let st = &mut app.embroidery.ask;
    if r.is_ok() {
        if let Some(Entry::Question(q)) = st.entries.iter_mut().rev().find(|e| matches!(e, Entry::Question(_))) {
            *q = shown.to_string();
        }
        if let Some(c) = st.current.as_mut() {
            c.question = typed.clone();
        }
    }
    st.input = typed;
    r
}

/// Start a sew-out check (Check My Sew-out): a new conversation in [`Mode::SewOut`] whose first
/// question is `message` ([`sewout::first_message`]), shown as `shown`; paid with credits, shared
/// to improve StitchFoundry when `share`, following up check `follow_up` → its id.
pub fn begin_check(app: &mut VectorcraftApp, message: Value, shown: String, share: bool, follow_up: Option<String>) -> Result<String, String> {
    if app.embroidery.ask.asking() {
        return Err("already asking: wait for the answer".into());
    }
    clear(app);
    let key = super::account::key_to_send(app);
    let credits = super::account::paying_with_credits(app);
    let id = cloud::check_id(&message, app.embroidery.prompt.now);
    let st = &mut app.embroidery.ask;
    st.open = true;
    st.mode = Mode::SewOut;
    st.check = Some(CheckHeaders { id: id.clone(), share, follow_up });
    if let Err(e) = vectorcraft_embroidery::sf_core::ai::clean_key(&key) {
        st.show_key = true;
        let m = e.to_string();
        st.error = Some(m.clone());
        return Err(m);
    }
    let keep = (st.remember && !credits).then(|| app.embroidery.prompt.key.trim().to_string());
    st.current = Some(Current { question: String::new(), start: 0, rounds: 0, keep });
    st.messages.push(message);
    st.entries.push(Entry::Question(shown));
    st.error = None;
    request(app).map(|_| id)
}

/// Send the conversation as it stands; the reply comes in through [`poll`].
fn request(app: &mut VectorcraftApp) -> Result<(), String> {
    let sew = app.embroidery.ask.mode == Mode::SewOut;
    let send = if sew { sewout::request } else { ask::request };
    let key = super::account::key_to_send(app);
    let http = match send(&key, &app.embroidery.ask.messages) {
        Ok(h) => h,
        Err(e) => return give_up(app, e.to_string()),
    };
    if let Some(c) = app.embroidery.ask.current.as_mut() {
        c.rounds += 1;
    }
    // a sew-out check paid with credits goes to the service's own endpoint, with its id
    let (path, extra) = match app.embroidery.ask.check.as_ref().filter(|_| sew) {
        Some(c) => {
            let mut h = vec![("x-stitchfoundry-check", c.id.clone())];
            if c.share {
                h.push(("x-stitchfoundry-share", "1".to_string()));
            }
            if let Some(f) = &c.follow_up {
                h.push(("x-stitchfoundry-follow-up", f.clone()));
            }
            (cloud::SEWOUT, h)
        }
        None => (cloud::MESSAGES, Vec::new()),
    };
    let rx = match prompt::post_ai(app, http, "ask-about-embroidery", path, extra, ask::parse_reply) {
        Ok(rx) => rx,
        Err(e) => return give_up(app, e),
    };
    app.embroidery.ask.pending = Some(Pending { rx, since: app.embroidery.prompt.now });
    Ok(())
}

/// The question got no answer: the conversation goes back to before it (its question stays in
/// the box to ask again; changes already made stay, each undoable), and `m` is shown.
fn give_up(app: &mut VectorcraftApp, m: String) -> Result<(), String> {
    let st = &mut app.embroidery.ask;
    st.pending = None;
    st.approval = None;
    if let Some(c) = st.current.take() {
        st.messages.truncate(c.start);
        st.input = c.question;
        if let Some(i) = st.entries.iter().rposition(|e| matches!(e, Entry::Question(_))) {
            st.entries.remove(i);
        }
    }
    st.error = Some(m.clone());
    Err(m)
}

/// Each frame: take in the reply that arrived, or why none did.
pub fn poll(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let Some(p) = app.embroidery.ask.pending.as_ref() else { return };
    let r = match p.rx.try_recv() {
        Ok(r) => r,
        Err(TryRecvError::Empty) => {
            // The thread doesn't wake the window: look again shortly.
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        Err(TryRecvError::Disconnected) => Err(ArtError::Network("the request stopped".into())),
    };
    app.embroidery.ask.pending = None;
    // the credits it cost show next time the account is read
    app.embroidery.account.stale |= super::account::paying_with_credits(app);
    // Anthropic took the key unless it said otherwise.
    let keep = app.embroidery.ask.current.as_ref().and_then(|c| c.keep.clone());
    if let Some(key) = keep.filter(|_| !matches!(r, Err(ArtError::Key(_)))) {
        prompt::keep_key(app.services.secrets.as_deref(), Some(&key));
    }
    let reply = match r {
        Ok(reply) => reply,
        Err(e) => {
            app.embroidery.ask.show_key |= matches!(e, ArtError::Key(_));
            let _ = give_up(app, e.to_string()); // (shown in the window)
            return;
        }
    };
    let st = &mut app.embroidery.ask;
    st.messages.push(reply.message);
    if !reply.text.is_empty() {
        st.entries.push(Entry::Answer(reply.text));
    }
    if reply.calls.is_empty() {
        st.current = None;
        st.input.clear();
        return;
    }
    if st.current.as_ref().is_some_and(|c| c.rounds >= ask::MAX_ROUNDS) {
        let _ = give_up(app, "Claude took too many steps: ask again, more simply.".into());
        return;
    }
    run_calls(app, &reply.calls);
}

/// Answer one reply's tool calls: searches and reads at once, changes after the user's Allow.
fn run_calls(app: &mut VectorcraftApp, calls: &[ToolCall]) {
    let mut results = Vec::new();
    let mut changes = Vec::new();
    for call in calls {
        let result = match call.name.as_str() {
            ask::FIND => {
                let q = call.input.get("query").and_then(Value::as_str).unwrap_or_default();
                Some(Ok(find(app, q).to_string()))
            }
            ask::RUN => {
                let id = call.input.get("command").and_then(Value::as_str).unwrap_or_default();
                let params = call.input.get("params").cloned().unwrap_or_else(|| json!({}));
                match vectorcraft_engine::find_command(id).filter(|_| !blocked(id)) {
                    None if blocked(id) => Some(to_claude(format!(
                        "{id} can't be run from here: files, preferences, plug-ins and windows are out of reach; tell the user where to find it instead"
                    ))),
                    None => Some(to_claude(format!("there is no command {id:?}: look for one with {}", ask::FIND))),
                    Some(_) if !params.is_object() => Some(to_claude("params must be an object".to_string())),
                    Some(spec) if access(spec) == Access::Blocked || names_a_file(&params) => Some(to_claude(format!(
                        "{id} can't be run from here: files, preferences, plug-ins and windows are out of reach; tell the user where to find it instead"
                    ))),
                    Some(spec) if access(spec) == Access::Read => {
                        Some(app.session.execute(id, &params).map(|v| v.to_string()).map_err(|e| e.to_string()))
                    }
                    Some(spec) => {
                        changes.push((call.id.clone(), id.to_string(), params, describe(spec)));
                        None
                    }
                }
            }
            other => Some(to_claude(format!("there is no tool {other:?}"))),
        };
        results.push((call.id.clone(), result));
    }
    if changes.is_empty() {
        send_results(app, results);
    } else {
        app.embroidery.ask.approval = Some(Approval { changes, results });
    }
}

/// The user's answer to the changes waiting (Allow / Don't Allow): made or declined, then the
/// results go back to Claude.
pub fn decide(app: &mut VectorcraftApp, allow: bool) {
    let Some(Approval { changes, mut results }) = app.embroidery.ask.approval.take() else { return };
    for (call, id, params, label) in changes {
        let r = if allow {
            let r = app.run(&id, params);
            let note = match &r {
                Ok(_) => format!("{} {label}", tl!("Changed:")),
                Err(e) => format!("{} {label}: {}", tl!("Couldn't change:"), crate::i18n::msg(e)),
            };
            app.embroidery.ask.entries.push(Entry::Note(note));
            r.map(|v| v.to_string())
        } else {
            app.embroidery.ask.entries.push(Entry::Note(format!("{} {label}", tl!("Not changed:"))));
            to_claude("The user declined this change.".to_string())
        };
        if let Some(slot) = results.iter_mut().find(|(c, _)| *c == call) {
            slot.1 = Some(r);
        }
    }
    send_results(app, results);
}

/// Send a round's results back and wait for Claude to go on.
fn send_results(app: &mut VectorcraftApp, results: Vec<(String, Option<Result<String, String>>)>) {
    let results: Vec<ToolResult> =
        results.into_iter().map(|(id, r)| ToolResult { id, result: r.unwrap_or_else(|| to_claude("not run".to_string())) }).collect();
    app.embroidery.ask.messages.push(ask::tool_results(&results));
    let _ = request(app); // (a failure is shown in the window)
}

/// Start a new conversation (New Conversation): the answers so far, and any question on its way,
/// are dropped.
pub fn clear(app: &mut VectorcraftApp) {
    let st = &mut app.embroidery.ask;
    st.entries.clear();
    st.messages.clear();
    st.mode = Mode::Ask;
    st.check = None;
    st.current = None;
    st.pending = None;
    st.approval = None;
    st.error = None;
}

/// The window, while open.
pub fn window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.ask.open {
        return;
    }
    let t = Tokens::get(ctx);
    let now = ctx.input(|i| i.time);
    app.embroidery.prompt.now = now;
    let mut open = true;
    let mut ask_now = false;
    let mut new_conversation = false;
    let mut decision = None;
    let mut report = false;
    let title = if app.embroidery.ask.mode == Mode::SewOut { tl!("Check My Sew-out") } else { tl!("Ask About Embroidery") };
    egui::Window::new(title)
        .id(egui::Id::new("ask-about-embroidery"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([460.0, 560.0])
        .default_pos(ctx.content_rect().center() - egui::vec2(230.0, 280.0))
        .show(ctx, |ui| {
            let st = &app.embroidery.ask;
            // room under the conversation for the question box, its buttons, the key and the note
            let credits = super::account::paying_with_credits(app);
            let footer = if (st.show_key || app.embroidery.prompt.key.trim().is_empty()) && !credits { 270.0 } else { 210.0 };
            egui::ScrollArea::vertical()
                .id_salt("ask-conversation")
                .max_height((ui.available_height() - footer).max(120.0))
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if st.entries.is_empty() {
                        ui.label(RichText::new(tl!("Ask anything about machine embroidery or StitchFoundry: which stabilizer to use, why thread breaks, how to make letters sew cleaner, where a feature is. Claude can also change your design when you ask, after you allow it.")).color(t.text_dim));
                    }
                    for e in &st.entries {
                        ui.add_space(4.0);
                        match e {
                            Entry::Question(q) => {
                                ui.add_space(4.0);
                                ui.label(RichText::new(q).strong());
                            }
                            Entry::Answer(a) => {
                                ui.label(a);
                            }
                            Entry::Note(n) => {
                                ui.label(RichText::new(n).size(12.0).color(t.text_dim));
                            }
                        }
                    }
                    if let Some(a) = &st.approval {
                        ui.add_space(6.0);
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.label(RichText::new(tl!("Claude wants to change your design:")).strong());
                            for (_, _, _, label) in &a.changes {
                                ui.label(format!("- {label}"));
                            }
                            ui.horizontal(|ui| {
                                if crate::widgets::primary_button(ui, tl!("Allow")).clicked() {
                                    decision = Some(true);
                                }
                                if ui.button(tl!("Don't Allow")).clicked() {
                                    decision = Some(false);
                                }
                            });
                            ui.label(RichText::new(tl!("Edit › Undo takes each change back.")).size(11.0).color(t.text_dim));
                        });
                    } else if let Some(secs) = st.waiting_for(now) {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new());
                            ui.label(format!("{} {:.0} s", tl!("Thinking…"), secs));
                        });
                    }
                });
            ui.separator();
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
            let st = &mut app.embroidery.ask;
            let r = ui.add(
                egui::TextEdit::multiline(&mut st.input)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text(tl!("Type your question, then press Enter")),
            );
            // Enter asks; Shift+Enter starts a new line.
            if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift) {
                st.input = st.input.trim_end_matches('\n').to_string();
                ask_now = true;
            }
            ui.horizontal(|ui| {
                let can = !st.asking();
                if ui.add_enabled_ui(can, |ui| crate::widgets::primary_button(ui, tl!("Ask"))).inner.clicked() {
                    ask_now = true;
                }
                if ui.button(tl!("New Conversation")).clicked() {
                    new_conversation = true;
                }
                if st.mode == Mode::SewOut
                    && st.last_answer().is_some()
                    && ui.button(tl!("Report to StitchFoundry…")).on_hover_text(tl!("Send this sew-out and Claude's idea of what StitchFoundry could do better to its makers")).clicked()
                {
                    report = true;
                }
            });
            ui.checkbox(&mut st.include_design, tl!("Include my design"));
            // paid with Foundry credits: no key asked for
            if super::account::paid_line(app, ui, &t) {
                return;
            }
            let show_key = app.embroidery.ask.show_key || app.embroidery.prompt.key.trim().is_empty();
            if show_key {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tl!("Anthropic API key:")).color(t.text_dim));
                    ui.add(egui::TextEdit::singleline(&mut app.embroidery.prompt.key).password(true).desired_width(220.0).hint_text("sk-ant-…"));
                });
                ui.checkbox(&mut app.embroidery.ask.remember, tl!("Remember the key on this computer"));
            }
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tl!("Answers come from Claude with your own Anthropic API key, billed to your Anthropic account. They can be wrong: try a test sew before an important piece.")).size(11.0).color(t.text_dim));
                if show_key {
                    if ui.link(RichText::new(tl!("Get a key")).size(11.0)).clicked() {
                        app.open_url(prompt::KEY_URL);
                    }
                } else if ui.link(RichText::new(tl!("Change key")).size(11.0)).clicked() {
                    app.embroidery.ask.show_key = true;
                }
            });
        });
    if let Some(allow) = decision {
        decide(app, allow);
    }
    if new_conversation {
        clear(app);
    }
    if report {
        // (a failure is shown in the window)
        let _ = super::sewout::open_report(app, &json!({}));
    }
    if ask_now && !app.embroidery.ask.asking() {
        // (a failure is shown in the window)
        let _ = send(app);
    }
    if !open {
        app.embroidery.ask.open = false;
    }
}
