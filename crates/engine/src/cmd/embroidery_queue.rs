//! The send queue (HW-01): designs sent to a machine's folder (a USB stick, the machine's folder,
//! a network share) wait in a queue while it isn't there and go when it is, in the order they
//! were sent; kept across sessions. Machines are reached through the folders they read: vendor
//! protocols aren't publicly documented.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, bool_or, cmd, has_doc, str_param};

/// Designs waiting or sent kept, at most, and the largest machine file queued (bytes).
const MAX_ITEMS: usize = 100;
const MAX_BYTES: usize = 8 << 20;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.queue",
            "Send Queue",
            [],
            None,
            "{} the designs sent to machine folders, oldest first: waiting for their folder, sent (to which file) or failed (why) → {items: [{id, name, folder, format, bytes, status: waiting|sent|failed, path?, error?, added}], waiting}",
            always,
            list
        ),
        cmd!(
            "embroidery.queue.add",
            "Add to Send Queue",
            [],
            None,
            "{folder?: the machine's folder (the last one used), format?: as Export Machine File (the design's machine, else dst), name?: the file's name (the design's)} the design's machine file sent to the folder: written now when the folder is there, else kept in the queue until it is (embroidery.queue.run) → {id, status, path?}",
            has_doc,
            add
        ),
        cmd!(
            "embroidery.queue.run",
            "Send Waiting Designs",
            [],
            None,
            "{} send the waiting designs whose folders are there now, in the order they were queued → {sent: [path…], waiting, failed}",
            always,
            run
        ),
        cmd!(
            "embroidery.queue.remove",
            "Remove from Send Queue",
            [],
            None,
            "{id?, done?: bool (remove every sent or failed one)} take designs out of the queue → {removed}",
            always,
            remove
        ),
    ]
}

/// A design in the send queue.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct QueueItem {
    pub id: u64,
    /// The file's name, without its extension.
    pub name: String,
    pub folder: String,
    pub format: String,
    /// The machine file, base64 (emptied once sent).
    pub data: String,
    pub bytes: usize,
    /// waiting, sent or failed.
    pub state: String,
    pub path: Option<String>,
    pub error: Option<String>,
    pub added: Option<i64>,
}

impl QueueItem {
    fn json(&self) -> Value {
        json!({ "id": self.id, "name": self.name, "folder": self.folder, "format": self.format, "bytes": self.bytes,
            "status": self.state, "path": self.path, "error": self.error, "added": self.added })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn folder_there(dir: &str) -> bool {
    std::path::Path::new(dir).is_dir()
}

#[cfg(target_arch = "wasm32")]
fn folder_there(_: &str) -> bool {
    false
}

/// Write `item` into its folder (a number added when the name is taken) → the path written.
fn deliver(item: &QueueItem) -> std::result::Result<String, String> {
    let bytes = vectorcraft_format::base64_decode(&item.data).ok_or("the queued file is damaged")?;
    let stem = super::embroidery_lettering::safe_name(&item.name);
    let base = std::path::Path::new(&item.folder);
    let mut path = base.join(format!("{stem}.{}", item.format));
    let mut k = 2;
    while super::embroidery::file_there(&path) && k < 1000 {
        path = base.join(format!("{stem}-{k}.{}", item.format));
        k += 1;
    }
    let path = path.to_string_lossy().to_string();
    super::fileio::write_file(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let q = &s.prefs.embroidery_queue;
    let items: Vec<Value> = q.iter().map(QueueItem::json).collect();
    Ok(json!({ "items": items, "waiting": q.iter().filter(|i| i.state == "waiting").count() }))
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.queue.add";
    let folder = match str_param(p, "folder").map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.to_string(),
        None => s
            .prefs
            .embroidery_send_dirs
            .first()
            .cloned()
            .ok_or_else(|| bad(CMD, "no machine folder yet: add one in the Send to Machine section (or give `folder`)"))?,
    };
    let name = match str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => super::fileio::file_stem(&s.doc()?.doc.title),
    };
    let mut q = json!({ "center": true, "name": name });
    if let Some(f) = str_param(p, "format") {
        q["format"] = json!(f);
    }
    let out = super::embroidery::export_machine_file(s, &q)?;
    let data = out["dataBase64"].as_str().unwrap_or_default().to_string();
    let bytes = out["bytes"].as_u64().unwrap_or(0) as usize;
    if bytes > MAX_BYTES {
        return Err(bad(CMD, "the machine file is larger than 8 MB"));
    }
    let queue = &mut s.prefs.embroidery_queue;
    // the oldest sent or failed make room
    while queue.len() >= MAX_ITEMS {
        match queue.iter().position(|i| i.state != "waiting") {
            Some(k) => {
                queue.remove(k);
            }
            None => return Err(bad(CMD, format!("{MAX_ITEMS} designs are waiting already: send or remove some first"))),
        }
    }
    let id = queue.iter().map(|i| i.id).max().unwrap_or(0) + 1;
    let mut item = QueueItem {
        id,
        name,
        folder,
        format: out["format"].as_str().unwrap_or("dst").to_string(),
        data,
        bytes,
        state: "waiting".into(),
        added: vectorcraft_doc::metadata::now_unix(),
        ..QueueItem::default()
    };
    // waiting only while its folder isn't there; the ones queued before it go first
    if folder_there(&item.folder) && !queue.iter().any(|i| i.state == "waiting" && i.folder == item.folder) {
        match deliver(&item) {
            Ok(path) => {
                item.state = "sent".into();
                item.path = Some(path);
                item.data.clear();
            }
            Err(e) => {
                item.state = "failed".into();
                item.error = Some(e);
            }
        }
    }
    let r = json!({ "id": id, "status": item.state, "path": item.path });
    queue.push(item);
    Ok(r)
}

fn run(s: &mut Session, _: &Value) -> Result<Value> {
    let mut sent = Vec::new();
    for item in s.prefs.embroidery_queue.iter_mut().filter(|i| i.state == "waiting") {
        if !folder_there(&item.folder) {
            continue;
        }
        match deliver(item) {
            Ok(path) => {
                item.state = "sent".into();
                item.data.clear();
                sent.push(path.clone());
                item.path = Some(path);
            }
            Err(e) => {
                item.state = "failed".into();
                item.error = Some(e);
            }
        }
    }
    let q = &s.prefs.embroidery_queue;
    Ok(json!({
        "sent": sent,
        "waiting": q.iter().filter(|i| i.state == "waiting").count(),
        "failed": q.iter().filter(|i| i.state == "failed").count(),
    }))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.queue.remove";
    let before = s.prefs.embroidery_queue.len();
    match (p.get("id").and_then(Value::as_u64), bool_or(p, "done", false)) {
        (Some(id), _) => s.prefs.embroidery_queue.retain(|i| i.id != id),
        (None, true) => s.prefs.embroidery_queue.retain(|i| i.state == "waiting"),
        (None, false) => return Err(bad(CMD, "give an `id`, or `done: true`")),
    }
    Ok(json!({ "removed": before - s.prefs.embroidery_queue.len() }))
}
