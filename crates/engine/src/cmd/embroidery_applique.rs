//! Appliqué pieces (AP-04, AP-05): several pieces combined into one sequence (every placement
//! line, one stop, every tack-down, one stop, every cover), and pieces made ready to sew: open
//! outlines closed and overlapping pieces broken up so each lies in its own layer.

use std::sync::Arc;

use serde_json::{Value, json};
use sf_core::attr::StitchType;
use vectorcraft_doc::{Appearance, Document, Node, NodeId, NodeKind};
use vectorcraft_geom::FillRule;

use super::embroidery::stitched_targets;
use super::{CommandSpec, Result, Session, bad, bool_or, cmd, has_selection};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.applique.combine",
            "Combine Appliqué",
            [],
            None,
            "{ids?} the selected appliqué pieces made one compound path sewn together: every placement line, one stop to lay the fabric, every tack-down, one stop to trim, every cover (the frontmost piece's settings) → {id, pieces}",
            has_selection,
            combine
        ),
        cmd!(
            "embroidery.applique.prepare",
            "Prepare Appliqué",
            [],
            None,
            "{ids?, close?: bool (true: close open outlines), breakUp?: bool (true: cut each piece where pieces in front of it overlap it, so every piece lies in its own layer)} make the selected appliqué pieces ready to sew → {closed, ids}",
            has_selection,
            prepare
        ),
    ]
}

fn is_applique(d: &Document, id: NodeId) -> bool {
    d.node(id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e| matches!(e.stitch, StitchType::Applique(_)))
}

fn applique_targets(s: &Session, p: &Value, cmd: &str) -> Result<Vec<NodeId>> {
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = ids.into_iter().filter(|id| is_applique(d, *id)).collect();
    if ids.is_empty() {
        return Err(bad(cmd, "select appliqué pieces"));
    }
    Ok(ids)
}

fn combine(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.applique.combine";
    let ids = applique_targets(s, p, CMD)?;
    let Some(&top) = ids.last() else { return Err(bad(CMD, "select appliqué pieces")) };
    let mut emb = s.doc()?.doc.node(top).and_then(|n| n.embroidery.clone()).ok_or_else(|| bad(CMD, "select appliqué pieces"))?;
    if let StitchType::Applique(a) = &mut emb.stitch {
        a.combine = true;
    }
    let one = ids.len() == 1;
    let id = s.edit("Combine Appliqué", |d, sel| {
        if one {
            // already one path or compound: its pieces sewn together
            let n = d.node_mut(top).ok_or(EngineError::NoNode(top))?;
            n.embroidery = Some(emb.clone());
            sel.set([top]);
            return Ok(top);
        }
        let appearance = d.node(top).map(|n| n.appearance.clone()).unwrap_or_default();
        let (par, idx, _) = d.position(top).ok_or(EngineError::NoNode(top))?;
        let mut pieces: Vec<Arc<Node>> = Vec::new();
        for id in &ids {
            let n = d.node(*id).cloned().ok_or(EngineError::NoNode(*id))?;
            match &n.kind {
                NodeKind::Path { .. } => pieces.push(Arc::new(n)),
                NodeKind::Compound { children, .. } => pieces.extend(children.iter().cloned()),
                _ => return Err(bad(CMD, "appliqué pieces are paths or compound paths")),
            }
        }
        let cid = d.alloc_id();
        let mut c = Node::new(cid, NodeKind::Compound { children: Vec::new(), rule: FillRule::NonZero });
        c.appearance = appearance;
        c.embroidery = Some(emb.clone());
        d.insert(par, idx + 1, c)?;
        for id in &ids {
            d.remove(*id)?;
        }
        let ch = d.node_mut(cid).and_then(|n| n.children_mut()).ok_or(EngineError::NoNode(cid))?;
        for mut piece in pieces {
            let pn = Arc::make_mut(&mut piece);
            pn.embroidery = None;
            pn.appearance = Appearance::default().into();
            ch.push(piece);
        }
        sel.set([cid]);
        Ok(cid)
    })?;
    let pieces = s.embroidery_plan().map_or(0, |plan| {
        plan.nodes
            .iter()
            .zip(&plan.design.objects)
            .filter(|(n, _)| **n == id)
            .map(|(_, o)| match &o.kind {
                sf_core::model::ObjectKind::Applique(a) => a.pieces().count(),
                _ => 0,
            })
            .sum::<usize>()
    });
    Ok(json!({ "id": id.0, "pieces": pieces }))
}

fn prepare(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.applique.prepare";
    let ids = applique_targets(s, p, CMD)?;
    let (close, break_up) = (bool_or(p, "close", true), bool_or(p, "breakUp", true));
    let mut closed = 0;
    if close {
        let open = |n: &Node| n.path_data().map_or(0, |pd| pd.subpaths.iter().filter(|sp| !sp.closed && sp.anchors.len() >= 2).count());
        {
            let d = &s.doc()?.doc;
            for id in &ids {
                let Some(n) = d.node(*id) else { continue };
                closed += match &n.kind {
                    NodeKind::Compound { children, .. } => children.iter().map(|c| open(c)).sum(),
                    _ => open(n),
                };
            }
        }
        if closed > 0 {
            s.edit("Close Appliqué Outlines", |d, _| {
                for id in &ids {
                    let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
                    let close_all = |n: &mut Node| {
                        if let NodeKind::Path { path, .. } = &mut n.kind {
                            for sp in path.subpaths.iter_mut().filter(|sp| sp.anchors.len() >= 2) {
                                sp.closed = true;
                            }
                        }
                    };
                    if let Some(children) = n.children_mut() {
                        for c in children {
                            close_all(Arc::make_mut(c));
                        }
                    } else {
                        close_all(n);
                    }
                }
                Ok(())
            })?;
        }
    }
    let mut out: Vec<u64> = ids.iter().map(|i| i.0).collect();
    if break_up && ids.len() >= 2 {
        s.execute("select.set", &json!({ "ids": out }))?;
        let r = s.execute("object.pathfinder.trim", &json!({}))?;
        out = r["ids"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
    }
    Ok(json!({ "closed": closed, "ids": out }))
}
