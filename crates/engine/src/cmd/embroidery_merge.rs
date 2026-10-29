//! Colour merge (MFL-02): threads of the same colour, or near enough to pass for it, become one
//! thread, and the design is sequenced again so their blocks join: fewer colour changes at the
//! machine. Objects that overlap keep their stacking order; an object given its own colour block
//! keeps its stop.

use serde_json::{Value, json};
use sf_core::Rgb;
use sf_core::attr::StitchType;
use sf_core::threads::Thread;
use vectorcraft_color::Color;
use vectorcraft_doc::{NodeId, NodeKind};

use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.colors.merge",
        "Merge Colours",
        [],
        None,
        "{tolerance?: ΔE 0–30 (3: colours that pass for the same; 0: only the very same), sequence?: bool (true: sequence again so the merged colours' blocks join), paint?: bool (true: repaint the art)} the design's threads of the same colour, or within `tolerance`, made one: each becomes the one sewn first, its name and code kept; one undo step → {merged: [{from, to}], threads: [before, after], blocks: [before, after]}",
        has_doc,
        merge
    )]
}

/// The design's colour blocks as it sews now.
fn blocks(s: &mut Session) -> usize {
    s.embroidery_plan().map(|p| p.pattern().color_blocks().len()).unwrap_or(0)
}

fn merge(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.colors.merge";
    let tolerance = match p.get("tolerance") {
        None | Some(Value::Null) => 3.0,
        Some(v) => v.as_f64().filter(|t| (0.0..=30.0).contains(t)).ok_or_else(|| bad(CMD, "`tolerance` is 0 to 30 (ΔE)"))?,
    };
    let repaint = bool_or(p, "paint", true);
    let before = blocks(s);
    // each stitched object's thread, in sewing order
    let objects: Vec<(NodeId, Thread)> = {
        let d = &s.doc()?.doc;
        vectorcraft_embroidery::stitched_nodes(d)
            .iter()
            .filter_map(|n| {
                let e = n.embroidery.as_deref()?;
                if matches!(e.stitch, StitchType::Branch) || matches!(n.kind, NodeKind::Group { .. } | NodeKind::Layer { .. }) {
                    return None;
                }
                Some((n.id, vectorcraft_embroidery::node_thread(n, e)))
            })
            .collect()
    };
    if objects.is_empty() {
        return Err(bad(CMD, "nothing is stitched yet"));
    }
    // the threads kept: the first sewn of each colour, the ones near it joining it
    let mut kept: Vec<Thread> = Vec::new();
    let mut merged: Vec<(Rgb, Rgb)> = Vec::new();
    let mut changes: Vec<(NodeId, Thread)> = Vec::new();
    let mut distinct: Vec<Rgb> = Vec::new();
    for (id, t) in &objects {
        if !distinct.contains(&t.color) {
            distinct.push(t.color);
        }
        let near = kept
            .iter()
            .filter(|k| k.color == t.color || k.color.delta_e(t.color) <= tolerance)
            .min_by(|a, b| a.color.delta_e(t.color).partial_cmp(&b.color.delta_e(t.color)).unwrap_or(std::cmp::Ordering::Equal));
        match near {
            None => kept.push(t.clone()),
            // the same thread already
            Some(k) if k.color == t.color && k.name == t.name && k.code == t.code => {}
            Some(k) => {
                if !merged.iter().any(|(f, _)| *f == t.color) && k.color != t.color {
                    merged.push((t.color, k.color));
                }
                changes.push((*id, k.clone()));
            }
        }
    }
    if changes.is_empty() {
        return Ok(json!({ "merged": [], "threads": [distinct.len(), distinct.len()], "blocks": [before, before] }));
    }
    // one undo step for the threads and the new order, unless a group is open already
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Merge Colours".into()));
    }
    let new = super::embroidery::new_stitching(s);
    let done = s
        .edit("Merge Colours", |d, _| {
            for (id, t) in &changes {
                let c = Color::rgb8(t.color.r, t.color.g, t.color.b);
                super::embroidery::put_thread(d.node_mut(*id).ok_or(EngineError::NoNode(*id))?, &new, Some(t.clone()), repaint.then_some(c));
            }
            Ok(())
        })
        .and_then(|_| if bool_or(p, "sequence", true) { s.execute("embroidery.autoSequence", &json!({})).map(|_| ()) } else { Ok(()) });
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    done?;
    let after = blocks(s);
    let threads_after = distinct.len() - merged.len();
    let merged: Vec<Value> = merged.iter().map(|(f, t)| json!({ "from": f.to_hex(), "to": t.to_hex() })).collect();
    Ok(json!({ "merged": merged, "threads": [distinct.len(), threads_after], "blocks": [before, after] }))
}
