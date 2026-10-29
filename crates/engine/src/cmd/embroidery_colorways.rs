//! Colorways: other thread schemes for the same design (CW-01). A colorway gives each of the
//! design's colours (its "slots": its threads in the order they are first used) a thread of its
//! own; the active one is what the stitches show, machine files carry and worksheets print.

use serde_json::{Value, json};
use sf_core::attr::{Colorway, DocEmbroidery, MAX_COLORWAYS};
use sf_core::{Rgb, Thread};

use super::{CommandSpec, Result, Session, bad, cmd, has_doc, str_param};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.colorway.list",
            "Colorways",
            [],
            None,
            "{} the design's colorways → {active: 0 (the design's own threads) or a colorway's number from 1, slots: [{color, name}] (the design's own threads in the order first used), colorways: [{name, threads: [{color, name, code, chart}]}]}",
            has_doc,
            list
        ),
        cmd!(
            "embroidery.colorway.add",
            "New Colorway",
            [],
            None,
            "{name?, from?: 0 (the design's own threads) or a colorway's number (the active one)} a new colorway, its threads copied, made the active one → {index}",
            has_doc,
            add
        ),
        cmd!(
            "embroidery.colorway.activate",
            "Use Colorway",
            [],
            None,
            "{index: 0 (the design's own threads) or a colorway's number} the colorway the stitches show, machine files carry and worksheets print → {index}",
            has_doc,
            activate
        ),
        cmd!(
            "embroidery.colorway.setThread",
            "Colorway Thread",
            [],
            None,
            "{slot: the design's colour (from 0), color: \"#rrggbb\", name?, code?, chart?, index?: a colorway's number (the active one)} give a colorway's slot a thread (the design's own threads change with embroidery.setThread) → {index, slot}",
            has_doc,
            set_thread
        ),
        cmd!(
            "embroidery.colorway.rename",
            "Rename Colorway",
            [],
            None,
            "{name, index?: a colorway's number (the active one)} → {index}",
            has_doc,
            rename
        ),
        cmd!(
            "embroidery.colorway.remove",
            "Delete Colorway",
            [],
            None,
            "{index?: a colorway's number (the active one)} delete a colorway; the design's own threads are used when it was the active one → {active}",
            has_doc,
            remove
        ),
    ]
}

fn config(s: &Session) -> Result<DocEmbroidery> {
    Ok(s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default())
}

/// The design's own threads, by slot.
fn slots(s: &Session) -> Vec<Thread> {
    s.embroidery_plan().map(|p| p.own_threads.clone()).unwrap_or_default()
}

/// A colorway's number from `index` (default: the active one), which must name one.
fn colorway_index(p: &Value, cfg: &DocEmbroidery, cmd: &str) -> Result<usize> {
    let i = match p.get("index") {
        None | Some(Value::Null) => cfg.colorway,
        Some(v) => v.as_u64().and_then(|i| usize::try_from(i).ok()).ok_or_else(|| bad(cmd, "`index` is a colorway's number, from 1"))?,
    };
    if i == 0 || i > cfg.colorways.len() {
        return Err(bad(cmd, "no such colorway: the design's own threads (0) change with Set Thread; make one with New Colorway"));
    }
    Ok(i)
}

fn save(s: &mut Session, label: &str, cfg: DocEmbroidery) -> Result<()> {
    s.edit(label, |d, _| {
        d.embroidery = Some(Box::new(cfg));
        Ok(())
    })
}

fn thread_json(t: &Thread) -> Value {
    json!({ "color": t.color.to_hex(), "name": t.name, "code": t.code, "chart": t.chart })
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    let cfg = config(s)?;
    let slots: Vec<Value> = slots(s).iter().map(|t| json!({ "color": t.color.to_hex(), "name": t.label() })).collect();
    let colorways: Vec<Value> =
        cfg.colorways.iter().map(|c| json!({ "name": c.name, "threads": c.threads.iter().map(thread_json).collect::<Vec<_>>() })).collect();
    Ok(json!({ "active": cfg.colorway, "slots": slots, "colorways": colorways }))
}

fn add(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.colorway.add";
    let mut cfg = config(s)?;
    if cfg.colorways.len() >= MAX_COLORWAYS {
        return Err(bad(CMD, format!("a design has {MAX_COLORWAYS} colorways at most")));
    }
    let own = slots(s);
    if own.is_empty() {
        return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type and thread first"));
    }
    let from = match p.get("from") {
        None | Some(Value::Null) => cfg.colorway,
        Some(v) => v
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i <= cfg.colorways.len())
            .ok_or_else(|| bad(CMD, "`from` is 0 or a colorway's number"))?,
    };
    let threads = cfg.threads_of(from, &own);
    let name = match str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.chars().take(60).collect(),
        None => format!("Colorway {}", cfg.colorways.len() + 1),
    };
    cfg.colorways.push(Colorway { name, threads });
    cfg.colorway = cfg.colorways.len();
    let index = cfg.colorway;
    save(s, "New Colorway", cfg)?;
    Ok(json!({ "index": index }))
}

fn activate(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.colorway.activate";
    let mut cfg = config(s)?;
    let i = p
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|i| usize::try_from(i).ok())
        .filter(|i| *i <= cfg.colorways.len())
        .ok_or_else(|| bad(CMD, "`index` is 0 (the design's own threads) or a colorway's number"))?;
    if cfg.colorway != i {
        cfg.colorway = i;
        save(s, "Use Colorway", cfg)?;
    }
    Ok(json!({ "index": i }))
}

fn set_thread(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.colorway.setThread";
    let mut cfg = config(s)?;
    let i = colorway_index(p, &cfg, CMD)?;
    let own = slots(s);
    let slot = p
        .get("slot")
        .and_then(Value::as_u64)
        .and_then(|k| usize::try_from(k).ok())
        .filter(|k| *k < own.len())
        .ok_or_else(|| bad(CMD, format!("`slot` is one of the design's {} colours, from 0", own.len())))?;
    let color = str_param(p, "color").and_then(Rgb::from_hex).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?;
    let text = |k: &str| str_param(p, k).map(|t| t.trim().chars().take(60).collect::<String>()).unwrap_or_default();
    let thread = Thread { color, name: text("name"), code: text("code"), chart: text("chart") };
    // a colorway made before the design gained colours: the new ones as the design has them
    let mut threads = cfg.threads_of(i, &own);
    if let Some(t) = threads.get_mut(slot) {
        *t = thread;
    }
    if let Some(c) = cfg.colorways.get_mut(i - 1) {
        c.threads = threads;
    }
    save(s, "Colorway Thread", cfg)?;
    Ok(json!({ "index": i, "slot": slot }))
}

fn rename(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.colorway.rename";
    let mut cfg = config(s)?;
    let i = colorway_index(p, &cfg, CMD)?;
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?;
    if let Some(c) = cfg.colorways.get_mut(i - 1) {
        c.name = name.chars().take(60).collect();
    }
    save(s, "Rename Colorway", cfg)?;
    Ok(json!({ "index": i }))
}

fn remove(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.colorway.remove";
    let mut cfg = config(s)?;
    let i = colorway_index(p, &cfg, CMD)?;
    cfg.colorways.remove(i - 1);
    // the active one gone: the design's own; one before it: its number one less
    cfg.colorway = match cfg.colorway {
        a if a == i => 0,
        a if a > i => a - 1,
        a => a,
    };
    let active = cfg.colorway;
    save(s, "Delete Colorway", cfg)?;
    Ok(json!({ "active": active }))
}
