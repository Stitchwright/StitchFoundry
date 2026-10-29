//! The export history (NEW-12): every export that writes files (File › Export, Send to Machine,
//! Batch Output, multi-hooping, batch convert), when, from which document, to which files, and
//! whether it worked; listed newest first, and any of them run again.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, cmd, has_doc};

/// Exports remembered, at most.
const MAX: usize = 100;

/// The commands whose exports are remembered (when they write files).
pub const RECORDED: &[&str] = &[
    "document.export",
    "document.exportSelection",
    "embroidery.send",
    "embroidery.output",
    "embroidery.multiHoop",
    "embroidery.library.convert",
    "embroidery.exportImage",
];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "export.history",
            "Export History",
            [],
            None,
            "{limit?: 1–100 (100)} the exports of this session that wrote files (File › Export, Send to Machine, Batch Output, multi-hooping, batch convert), newest first → {exports: [{index, at: Unix seconds, command, document, format, paths: [path…], ok, error?}]}",
            always,
            history
        ),
        cmd!("export.history.clear", "Clear Export History", [], None, "{} forget the exports remembered → {cleared}", always, clear),
        cmd!(
            "export.again",
            "Export Again",
            [],
            None,
            "{index?: an export.history index (0: the newest)} run an export again as it was asked, on the active document, replacing the files it wrote → that command's result",
            has_doc,
            again
        ),
    ]
}

/// Whether `command` with `params` writes files (rather than returning them).
fn writes_files(command: &str, params: &Value) -> bool {
    let given = |k: &str| params.get(k).and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty());
    match command {
        "embroidery.send" | "embroidery.library.convert" => true,
        "embroidery.multiHoop" => given("dir"),
        "embroidery.output" => given("dir") || given("profile"),
        _ => given("path"),
    }
}

/// The files a result says were written.
fn written(r: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |v: &Value| {
        if let Some(p) = v.as_str().filter(|p| !p.is_empty())
            && !out.iter().any(|o| o == p)
        {
            out.push(p.to_string());
        }
    };
    add(&r["path"]);
    for f in r["files"].as_array().into_iter().flatten() {
        add(if f.is_object() { &f["path"] } else { f });
    }
    for c in r["converted"].as_array().into_iter().flatten() {
        add(&c["to"]);
    }
    for h in r["hoopings"].as_array().into_iter().flatten() {
        add(&h["file"]);
    }
    out.truncate(MAX);
    out
}

/// Remember an export `command` ran with `params`: what it wrote, or why it failed. Calls that
/// return the file instead of writing it are left out.
pub fn note(s: &mut Session, command: &str, params: &Value, result: std::result::Result<&Value, String>) {
    if !RECORDED.contains(&command) || !writes_files(command, params) {
        return;
    }
    // what was asked, without data passed in
    let mut asked = params.clone();
    if let Some(o) = asked.as_object_mut() {
        o.retain(|k, _| !k.to_ascii_lowercase().contains("base64"));
    }
    let document = s.active().map(|d| super::fileio::file_name(&d.doc.title)).unwrap_or_default();
    let format = match &result {
        Ok(r) => r["format"].as_str().or_else(|| params["format"].as_str()).unwrap_or_default().to_string(),
        Err(_) => params["format"].as_str().unwrap_or_default().to_string(),
    };
    let mut rec = json!({
        "at": vectorcraft_doc::metadata::now_unix(),
        "command": command,
        "document": document,
        "format": format,
        "params": asked,
        "paths": result.as_ref().map(|r| written(r)).unwrap_or_default(),
        "ok": result.is_ok(),
    });
    if let Err(e) = result {
        rec["error"] = json!(e);
    }
    let h = &mut s.export_history;
    h.push_front(rec);
    h.truncate(MAX);
}

fn history(s: &mut Session, p: &Value) -> Result<Value> {
    let limit = match p.get("limit") {
        None | Some(Value::Null) => MAX,
        Some(v) => v.as_u64().filter(|n| (1..=MAX as u64).contains(n)).ok_or_else(|| bad("export.history", "`limit` is 1 to 100"))? as usize,
    };
    let list: Vec<Value> = s
        .export_history
        .iter()
        .take(limit)
        .enumerate()
        .map(|(i, r)| {
            let mut r = r.clone();
            if let Some(o) = r.as_object_mut() {
                o.remove("params");
                o.insert("index".into(), json!(i));
            }
            r
        })
        .collect();
    Ok(json!({ "exports": list }))
}

fn clear(s: &mut Session, _: &Value) -> Result<Value> {
    let n = s.export_history.len();
    s.export_history.clear();
    Ok(json!({ "cleared": n }))
}

fn again(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "export.again";
    let index = match p.get("index") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_u64().ok_or_else(|| bad(CMD, "`index` is an export.history index"))? as usize,
    };
    let rec = s.export_history.get(index).cloned().ok_or_else(|| bad(CMD, format!("no export {index} in the history")))?;
    let command = rec["command"].as_str().filter(|c| RECORDED.contains(c)).ok_or_else(|| bad(CMD, "that export can't be run again"))?.to_string();
    let mut params = rec["params"].clone();
    // the files it wrote are written again, not numbered beside them
    if command.starts_with("embroidery.")
        && let Some(o) = params.as_object_mut()
    {
        o.insert("replace".into(), json!(true));
    }
    let r = s.execute(&command, &params);
    note(s, &command, &params, r.as_ref().map_err(ToString::to_string));
    r
}
