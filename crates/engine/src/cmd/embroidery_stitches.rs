//! Stitch editing: an object's stitches kept as they are ("frozen") to edit them
//! by hand, and machine functions (trims, stops, jumps) at single stitches.
//!
//! An object's stitches are its own records in sewing order, in document points,
//! with the trims, stops and jumps between them: what its shape generates, or
//! what was frozen and edited. Indices in the commands count those records. The
//! lock stitches and connections the design adds round each object are not part
//! of them.

use serde_json::{Value, json};
use sf_core::{Cmd, Stitch};
use vectorcraft_doc::NodeId;
use vectorcraft_embroidery::PT_PER_MM;

use super::embroidery::{ids_json, stitched_targets};
use super::{CommandSpec, Result, Session, bad, cmd, has_doc, has_selection, id_param, str_param};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.stitches",
            "Object Stitches",
            [],
            None,
            "{id} an object's own stitches in sewing order (document points; its edited ones when frozen) → {id, frozen, stitches: [[x, y, stitch|jump|trim|stop]…]}",
            has_doc,
            stitches
        ),
        cmd!(
            "embroidery.freeze",
            "Freeze Stitches",
            [],
            None,
            "{ids?} keep the selected objects' stitches as they are now, to edit them by hand (the shape no longer regenerates them) → {ids}",
            has_selection,
            freeze
        ),
        cmd!(
            "embroidery.stitches.split",
            "Split Object",
            [],
            None,
            "{id, at} split an object's stitches in two at its record `at`: the object keeps the ones before, a copy sewn right after it takes the rest, both frozen (their stitches kept as they are) → {ids}",
            has_doc,
            split
        ),
        cmd!(
            "embroidery.regenerate",
            "Regenerate Stitches",
            [],
            None,
            "{ids?} drop hand edits: the selected objects' stitches come from their shapes again → {ids}",
            has_selection,
            regenerate
        ),
        cmd!(
            "embroidery.stitches.edit",
            "Edit Stitches",
            [],
            None,
            "{id, ops: [{op: move, indices, by: [dx, dy]} | {op: delete, indices} | {op: insert, after (−1 = first), at: [x, y], cmd?: stitch|jump|trim|stop}]} edit an object's stitches (document points), freezing them first → {id, stitches}",
            has_doc,
            edit
        ),
        cmd!(
            "embroidery.removeShortStitches",
            "Remove Short Stitches",
            [],
            None,
            "{ids?, min?: mm (0.3; 0.1–2)} drop stitches shorter than `min` from the selected (else every) imported or hand-placed object's stitches, keeping where each run starts and ends; objects whose stitches come from their shapes are left as they are (their stitch length decides) → {removed, ids}",
            has_doc,
            remove_short
        ),
        cmd!(
            "embroidery.function.insert",
            "Insert Machine Function",
            [],
            None,
            "{id, at, kind: trim|stop|jump} a trim or stop after stitch `at`, or make stitch `at` a jump (the frame moves there without sewing); freezes the object → {id, stitches}",
            has_doc,
            function_insert
        ),
        cmd!(
            "embroidery.function.clear",
            "Clear Machine Function",
            [],
            None,
            "{id, at} remove the trim or stop at record `at`, or sew jump `at` as a stitch → {id, stitches}",
            has_doc,
            function_clear
        ),
    ]
}

fn cmd_name(c: Cmd) -> &'static str {
    match c {
        Cmd::Stitch => "stitch",
        Cmd::Jump => "jump",
        Cmd::Trim => "trim",
        Cmd::Stop => "stop",
        Cmd::ColorChange => "colorChange",
        Cmd::End => "end",
    }
}

fn parse_cmd(s: &str) -> Option<Cmd> {
    match s {
        "stitch" => Some(Cmd::Stitch),
        "jump" => Some(Cmd::Jump),
        "trim" => Some(Cmd::Trim),
        "stop" => Some(Cmd::Stop),
        _ => None,
    }
}

/// The stitches object `id` sews (document points), and whether they are frozen.
fn current(s: &mut Session, id: NodeId, cmd: &str) -> Result<(Vec<Stitch>, bool)> {
    let frozen = {
        let n = s.doc()?.doc.node(id).ok_or_else(|| bad(cmd, format!("no such object id {}", id.0)))?;
        let e = n.embroidery.as_deref().ok_or_else(|| bad(cmd, "the object has no stitch type (Embroidery › Stitch Type)"))?;
        e.frozen.clone()
    };
    if let Some(f) = frozen {
        return Ok((f, true));
    }
    let plan = s.embroidery_plan().ok_or_else(|| bad(cmd, "no document"))?;
    let own = plan.own_stitches(id).into_iter().map(|s| Stitch::new(s.x * PT_PER_MM, s.y * PT_PER_MM, s.cmd)).collect();
    Ok((own, false))
}

/// Stores `stitches` as object `id`'s frozen stitches.
fn store(s: &mut Session, id: NodeId, label: &str, stitches: Vec<Stitch>) -> Result<Value> {
    let count = stitches.len();
    s.edit(label, |d, _| {
        let n = d.node_mut(id).ok_or(EngineError::NoNode(id))?;
        if let Some(e) = n.embroidery.as_deref_mut() {
            e.frozen = Some(stitches);
        }
        Ok(())
    })?;
    Ok(json!({ "id": id.0, "stitches": count }))
}

/// `st` without the stitches shorter than `min` (points) from the last one kept: a run's first and
/// last penetrations stay, and so does every jump, trim and stop → (the stitches, how many went).
fn without_short(st: &[Stitch], min: f64) -> (Vec<Stitch>, usize) {
    let mut out: Vec<Stitch> = Vec::with_capacity(st.len());
    let mut removed = 0;
    for (i, q) in st.iter().enumerate() {
        let last_of_run = st.get(i + 1).is_none_or(|n| n.cmd != Cmd::Stitch);
        let short = match out.last() {
            Some(prev) if q.cmd == Cmd::Stitch && prev.cmd == Cmd::Stitch => (q.x - prev.x).hypot(q.y - prev.y) < min,
            _ => false,
        };
        if short && !last_of_run {
            removed += 1;
        } else {
            out.push(*q);
        }
    }
    (out, removed)
}

/// Remove Short Stitches (PU-18): imported and hand-placed stitches only, the way a viewer cleans
/// a machine file; generated ones come from their shapes again on the next change.
fn remove_short(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.removeShortStitches";
    let min_mm = match p.get("min") {
        None | Some(Value::Null) => 0.3,
        Some(v) => v.as_f64().filter(|m| (0.1..=2.0).contains(m)).ok_or_else(|| bad(CMD, "`min` is 0.1 to 2 mm"))?,
    };
    let ids: Vec<NodeId> = if p.get("ids").is_none() && s.doc()?.selection.objects.is_empty() {
        vectorcraft_embroidery::stitched_nodes(&s.doc()?.doc).iter().map(|n| n.id).collect()
    } else {
        stitched_targets(s, p)?
    };
    let mut cleaned = Vec::new();
    let mut removed = 0;
    for id in ids {
        let hand = {
            let d = &s.doc()?.doc;
            d.node(id)
                .and_then(|n| n.embroidery.as_deref())
                .is_some_and(|e| e.frozen.is_some() || matches!(e.stitch, sf_core::attr::StitchType::Manual))
        };
        if !hand {
            continue;
        }
        let (st, _) = current(s, id, CMD)?;
        let (kept, gone) = without_short(&st, min_mm * PT_PER_MM);
        if gone > 0 {
            removed += gone;
            cleaned.push((id, kept));
        }
    }
    if !cleaned.is_empty() {
        s.edit("Remove Short Stitches", |d, _| {
            for (id, st) in &cleaned {
                if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                    e.frozen = Some(st.clone());
                }
            }
            Ok(())
        })?;
    }
    let mut out = ids_json(&cleaned.iter().map(|c| c.0).collect::<Vec<_>>());
    out["removed"] = json!(removed);
    Ok(out)
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn stitches(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.stitches";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing `id`"))?;
    let (st, frozen) = current(s, id, CMD)?;
    let list: Vec<Value> = st.iter().map(|q| json!([round(q.x), round(q.y), cmd_name(q.cmd)])).collect();
    Ok(json!({ "id": id.0, "frozen": frozen, "stitches": list }))
}

fn freeze(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.freeze";
    let ids = stitched_targets(s, p)?;
    let mut lists = Vec::new();
    for id in &ids {
        let (st, frozen) = current(s, *id, CMD)?;
        if !frozen && st.iter().any(|q| q.cmd == Cmd::Stitch) {
            lists.push((*id, st));
        }
    }
    if lists.is_empty() {
        return Err(bad(CMD, "the selected objects make no stitches, or are frozen already"));
    }
    s.edit("Freeze Stitches", |d, _| {
        for (id, st) in &lists {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.frozen = Some(st.clone());
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&lists.iter().map(|l| l.0).collect::<Vec<_>>()))
}

fn split(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.stitches.split";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing `id`"))?;
    let at = p.get("at").and_then(Value::as_u64).ok_or_else(|| bad(CMD, "missing `at`, a record index"))? as usize;
    let (st, _) = current(s, id, CMD)?;
    let stitches = |v: &[Stitch]| v.iter().filter(|q| q.cmd == Cmd::Stitch).count();
    if at >= st.len() || stitches(&st[..at]) < 2 || stitches(&st[at..]) < 2 {
        return Err(bad(CMD, "split where each part keeps two stitches or more"));
    }
    let (first, second) = (st[..at].to_vec(), st[at..].to_vec());
    let copy = s.edit("Split Object", |d, sel| {
        let (parent, index, _) = d.position(id).ok_or(EngineError::NoNode(id))?;
        let n = d.node(id).cloned().ok_or(EngineError::NoNode(id))?;
        let mut c = d.reid(&n);
        if let Some(e) = c.embroidery.as_deref_mut() {
            e.frozen = Some(second.clone());
            e.entry = None;
            e.exit = None;
        }
        let copy = d.insert(parent, index + 1, c)?;
        if let Some(e) = d.node_mut(id).ok_or(EngineError::NoNode(id))?.embroidery.as_deref_mut() {
            e.frozen = Some(first.clone());
        }
        sel.set([id, copy]);
        Ok(copy)
    })?;
    Ok(ids_json(&[id, copy]))
}

fn regenerate(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.regenerate";
    let ids = stitched_targets(s, p)?;
    let frozen: Vec<NodeId> = {
        let d = &s.doc()?.doc;
        ids.iter().copied().filter(|id| d.node(*id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e| e.frozen.is_some())).collect()
    };
    if frozen.is_empty() {
        return Err(bad(CMD, "no selected object has edited stitches"));
    }
    s.edit("Regenerate Stitches", |d, _| {
        for id in &frozen {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.frozen = None;
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&frozen))
}

fn point(v: Option<&Value>, what: &str) -> std::result::Result<(f64, f64), String> {
    let a = v.and_then(Value::as_array).ok_or_else(|| format!("`{what}` is [x, y]"))?;
    match (a.first().and_then(Value::as_f64), a.get(1).and_then(Value::as_f64)) {
        (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok((x, y)),
        _ => Err(format!("`{what}` is [x, y] (numbers)")),
    }
}

/// The record indices an op names, sorted, each checked against `len`.
fn indices(op: &Value, len: usize) -> std::result::Result<Vec<usize>, String> {
    let a = op.get("indices").and_then(Value::as_array).ok_or("missing `indices`")?;
    let mut out = Vec::with_capacity(a.len());
    for v in a {
        let i = v.as_u64().ok_or("indices are record numbers (0, 1, 2…)")? as usize;
        if i >= len {
            return Err(format!("no record {i}: the object has {len}"));
        }
        out.push(i);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

fn apply(st: &mut Vec<Stitch>, op: &Value) -> std::result::Result<(), String> {
    match op.get("op").and_then(Value::as_str) {
        Some("move") => {
            let (dx, dy) = point(op.get("by"), "by")?;
            for i in indices(op, st.len())? {
                st[i].x += dx;
                st[i].y += dy;
            }
        }
        Some("delete") => {
            for i in indices(op, st.len())?.into_iter().rev() {
                st.remove(i);
            }
        }
        Some("insert") => {
            let after = op.get("after").and_then(Value::as_i64).ok_or("missing `after` (−1 inserts first)")?;
            if after < -1 || after >= st.len() as i64 {
                return Err(format!("`after` is −1 to {}", st.len() as i64 - 1));
            }
            let (x, y) = point(op.get("at"), "at")?;
            let cmd = match op.get("cmd").and_then(Value::as_str) {
                None => Cmd::Stitch,
                Some(c) => parse_cmd(c).ok_or_else(|| format!("unknown cmd `{c}`: stitch, jump, trim or stop"))?,
            };
            st.insert((after + 1) as usize, Stitch::new(x, y, cmd));
        }
        Some(other) => return Err(format!("unknown op `{other}`: move, delete or insert")),
        None => return Err("each op needs `op`: move, delete or insert".into()),
    }
    Ok(())
}

fn edit(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.stitches.edit";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing `id`"))?;
    let ops = p.get("ops").and_then(Value::as_array).ok_or_else(|| bad(CMD, "missing `ops` (a list)"))?.clone();
    let (mut st, _) = current(s, id, CMD)?;
    for op in &ops {
        apply(&mut st, op).map_err(|m| bad(CMD, m))?;
    }
    store(s, id, "Edit Stitches", st)
}

/// The record index `at` of object `id`'s stitches.
fn record(p: &Value, len: usize, cmd: &str) -> Result<usize> {
    let at = p.get("at").and_then(Value::as_u64).ok_or_else(|| bad(cmd, "missing `at` (a record number)"))? as usize;
    if at >= len {
        return Err(bad(cmd, format!("no record {at}: the object has {len}")));
    }
    Ok(at)
}

fn function_insert(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.function.insert";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing `id`"))?;
    let kind = str_param(p, "kind").ok_or_else(|| bad(CMD, "missing `kind`: trim, stop or jump"))?;
    let (mut st, _) = current(s, id, CMD)?;
    let at = record(p, st.len(), CMD)?;
    let here = st[at];
    match kind {
        "trim" | "stop" => {
            let c = if kind == "trim" { Cmd::Trim } else { Cmd::Stop };
            st.insert(at + 1, Stitch::new(here.x, here.y, c));
        }
        "jump" if here.cmd == Cmd::Stitch => st[at].cmd = Cmd::Jump,
        "jump" => return Err(bad(CMD, format!("record {at} is a {}, not a stitch", cmd_name(here.cmd)))),
        other => return Err(bad(CMD, format!("unknown kind `{other}`: trim, stop or jump"))),
    }
    store(s, id, "Insert Machine Function", st)
}

fn function_clear(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.function.clear";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing `id`"))?;
    let (mut st, _) = current(s, id, CMD)?;
    let at = record(p, st.len(), CMD)?;
    match st[at].cmd {
        Cmd::Trim | Cmd::Stop | Cmd::ColorChange => {
            st.remove(at);
        }
        Cmd::Jump => st[at].cmd = Cmd::Stitch,
        Cmd::Stitch | Cmd::End => return Err(bad(CMD, format!("record {at} is a stitch: no machine function there"))),
    }
    store(s, id, "Clear Machine Function", st)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PU-18: short stitches go, a run's ends and the machine functions stay.
    #[test]
    fn short_stitches_are_removed_and_ends_kept() {
        let q = |x: f64, c: Cmd| Stitch::new(x, 0.0, c);
        let st = [q(0.0, Cmd::Stitch), q(0.2, Cmd::Stitch), q(3.0, Cmd::Stitch), q(3.1, Cmd::Stitch), q(3.1, Cmd::Trim), q(9.0, Cmd::Stitch)];
        let (kept, gone) = without_short(&st, 0.5);
        assert_eq!(gone, 1, "{kept:?}");
        assert_eq!(kept.iter().map(|k| k.x).collect::<Vec<_>>(), vec![0.0, 3.0, 3.1, 3.1, 9.0], "the run's last stitch stays");
        // in a design: a frozen object cleaned, one generated from its shape left as it is
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({})).unwrap();
        let a = s.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 60, "height": 40})).unwrap()["id"].as_u64().unwrap();
        s.execute("embroidery.set", &json!({"type": "run"})).unwrap();
        s.execute("embroidery.freeze", &json!({"ids": [a]})).unwrap();
        let first = s.execute("embroidery.stitches", &json!({"id": a})).unwrap()["stitches"][1].clone();
        let at = [first[0].as_f64().unwrap() + 0.2, first[1].as_f64().unwrap()];
        s.execute("embroidery.stitches.edit", &json!({"id": a, "ops": [{"op": "insert", "after": 1, "at": at}]})).unwrap();
        let b = s.execute("shape.rectangle", &json!({"x": 150, "y": 50, "width": 60, "height": 40})).unwrap()["id"].as_u64().unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("select.none", &json!({})).unwrap();
        let r = s.execute("embroidery.removeShortStitches", &json!({})).unwrap();
        assert!(r["removed"].as_u64().unwrap() >= 1, "{r}");
        assert_eq!(r["ids"], json!([a]), "the generated fill {b} is left: {r}");
        assert!(s.execute("embroidery.removeShortStitches", &json!({"min": 5})).is_err());
    }

    /// PU-04, PU-05: a satin column's spacing along it and random density, through Stitch
    /// Parameters.
    #[test]
    fn satin_density_along_the_column_and_at_random() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({})).unwrap();
        let id = s.execute("path.create", &json!({"d": "M60 100 L260 100"})).unwrap()["id"].as_u64().unwrap();
        s.execute("embroidery.set", &json!({"type": "satin", "ids": [id]})).unwrap();
        s.execute(
            "embroidery.setParams",
            &json!({"ids": [id], "params": {"width": 4, "auto_spacing": false, "underlay": {"auto": false, "center_run": false}}}),
        )
        .unwrap();
        let n = |s: &mut Session| s.execute("embroidery.stitches", &json!({"id": id})).unwrap()["stitches"].as_array().unwrap().len();
        let even = n(&mut s);
        s.execute("embroidery.setParams", &json!({"ids": [id], "params": {"spacing_along": [[0, 0.1], [1, 0.1]]}})).unwrap();
        let dense = n(&mut s);
        assert!(dense as f64 > even as f64 * 1.5, "{even} → {dense}");
        s.execute("embroidery.setParams", &json!({"ids": [id], "params": {"spacing_along": [], "random_density": 0.5}})).unwrap();
        let rough = n(&mut s);
        assert!((rough as f64 - even as f64).abs() < even as f64 * 0.3, "{even} vs {rough}");
    }
}
