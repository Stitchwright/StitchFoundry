//! Sewing order: branching (a group of objects sewn as one path), breaking a branch
//! or a many-part object apart, and sequencing the whole design.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::geom::{Polygon, poly};
use sf_core::model::{RunKind, RunParams};
use sf_core::sequence::{Item, sequence};
use vectorcraft_doc::{Node, NodeId, NodeKind};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_embroidery::parts;
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery::ids_json;
use super::{CommandSpec, Point, Result, Session, bad, checked_ids_param, cmd, has_doc, has_selection, str_param};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.branch",
            "Branch",
            [],
            None,
            "{ids?} sew the selected stitched objects (two or more) as one path: they become a branch group, sewn in the order with the fewest colour changes and the shortest travel, the thread carried from one to the next as running stitches where the ones after cover it (else trimmed as usual) → {id}",
            has_selection,
            branch
        ),
        cmd!(
            "embroidery.redwork",
            "Redwork",
            [],
            None,
            "{ids?, kind?: run|bean|backstitch|stemstitch (run)} sew the selected line art (paths, compound paths and type, whatever their own stitching) as redwork: one line that starts and ends at the same point, every line sewn once in the run style and the way between them retraced along lines already sewn as a single run; lines ending within 0.3 mm of another meet it. They become a redwork group in the first thread given to them, else the first line's stroke colour (Break Apart gives them back) → {id}",
            has_selection,
            redwork
        ),
        cmd!(
            "embroidery.knife",
            "Automatic Knife",
            [],
            None,
            "{ids?, overlap?: mm (0.3)} cut the selected stitched shapes round their holes and where they branch (at sharp inside corners) into simpler pieces, each its own object with the same stitching and its own automatic direction (a single angle or turning), overlapping by `overlap` along the cuts; each shape's pieces become a branch group sewn as one path → {ids}",
            has_selection,
            knife
        ),
        cmd!(
            "embroidery.breakApart",
            "Break Apart",
            [],
            None,
            "{ids?} separate the selection again: a branch or redwork group gives back its objects, and a stitched path or compound path made of separate parts becomes one object per part, each stitched the same way (holes stay with their part) → {ids}",
            has_selection,
            break_apart
        ),
        cmd!(
            "embroidery.autoSequence",
            "Auto Sequence",
            [],
            None,
            "{ids?: groups} reorder the stitched objects (within each layer or group; with ids, only those inside these groups) for the fewest colour changes, then the shortest travel, keeping objects that overlap in their stacking order → {moved}",
            has_doc,
            auto_sequence
        ),
    ]
}

/// A branch, redwork or lettering group: a group sewn by what it holds.
fn is_branch(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Group { .. })
        && n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, StitchType::Branch | StitchType::Redwork(_) | StitchType::Lettering(_)))
}

fn redwork(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.redwork";
    let kind = match str_param(p, "kind").unwrap_or("run") {
        "run" | "single" => RunKind::Single,
        "bean" | "triple" => RunKind::Triple,
        "backstitch" | "back" => RunKind::Back,
        "stemstitch" | "stem" => RunKind::Stem,
        other => return Err(bad(CMD, format!("`kind` is run, bean, backstitch or stemstitch, not `{other}`"))),
    };
    let roots = match p.get("ids") {
        Some(_) => super::edit::roots_of(&s.doc()?.doc, checked_ids_param(s, p, "ids", CMD)?),
        None => super::edit::selected_roots(s)?,
    };
    // the first thread given to a line, else the first line's stroke colour
    let thread = {
        let d = &s.doc()?.doc;
        let (mut given, mut stroke) = (None, None);
        for id in &roots {
            if let Some(n) = d.node(*id) {
                n.walk(&mut |c| {
                    if vectorcraft_embroidery::stitchable(c) {
                        given = given.take().or_else(|| c.embroidery.as_deref().and_then(|e| e.thread.clone()));
                        stroke = stroke.take().or_else(|| Some(vectorcraft_embroidery::node_thread(c, &Embroidery::run())));
                    }
                });
            }
        }
        given.or(stroke).ok_or_else(|| bad(CMD, "select line art (paths or type) to sew as redwork"))?
    };
    let gid = s.edit("Redwork", |d, sel| {
        // one selected group becomes redwork; else the selection is grouped
        let single_group = (roots.len() == 1).then(|| roots[0]).filter(|id| d.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Group { .. })));
        let gid = match single_group {
            Some(g) => g,
            None => super::object::group_nodes(d, &roots)?,
        };
        let mut e = Embroidery::new(StitchType::Redwork(RunParams { kind, ..RunParams::default() }));
        e.thread = Some(thread.clone());
        let n = d.node_mut(gid).ok_or(EngineError::NoNode(gid))?;
        n.embroidery = Some(Box::new(e));
        n.name.get_or_insert_with(|| "Redwork".to_string());
        sel.set([gid]);
        Ok(gid)
    })?;
    Ok(json!({ "id": gid.0 }))
}

fn branch(s: &mut Session, _p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.branch";
    let roots = super::edit::selected_roots(s)?;
    let stitched = {
        let d = &s.doc()?.doc;
        let mut count = 0usize;
        for id in &roots {
            if let Some(n) = d.node(*id) {
                count += stitched_nodes_under(n);
            }
        }
        count
    };
    if stitched < 2 {
        return Err(bad(CMD, "select two or more stitched objects to sew as one path"));
    }
    let gid = s.edit("Branch", |d, sel| {
        // one selected group of stitched objects becomes the branch; else the selection is grouped
        let single_group = (roots.len() == 1).then(|| roots[0]).filter(|id| d.node(*id).is_some_and(|n| matches!(n.kind, NodeKind::Group { .. })));
        let gid = match single_group {
            Some(g) => g,
            None => super::object::group_nodes(d, &roots)?,
        };
        d.node_mut(gid).ok_or(EngineError::NoNode(gid))?.embroidery = Some(Box::new(Embroidery::new(StitchType::Branch)));
        sel.set([gid]);
        Ok(gid)
    })?;
    Ok(json!({ "id": gid.0 }))
}

fn knife(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.knife";
    let overlap = match p.get("overlap") {
        None | Some(Value::Null) => 0.3,
        Some(v) => v.as_f64().filter(|v| (0.0..=3.0).contains(v)).ok_or_else(|| bad(CMD, "`overlap` is 0 to 3 mm"))?,
    };
    // What each selected shape is cut into (mm).
    let mut plans: Vec<(NodeId, Vec<Polygon>)> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in super::embroidery::stitched_targets(s, p)? {
            let Some(n) = d.node(id) else { continue };
            let area = n.embroidery.as_deref().is_some_and(|e| {
                matches!(e.stitch, StitchType::Fill(_)) || (matches!(e.stitch, StitchType::Satin(_)) && !vectorcraft_embroidery::is_line_art(n))
            });
            if !area || !matches!(n.kind, NodeKind::Path { .. } | NodeKind::Compound { .. }) {
                continue;
            }
            let mut pieces = Vec::new();
            for region in vectorcraft_embroidery::regions_mm(n) {
                let cut: Vec<Polygon> = poly::split_holes(&region).iter().flat_map(poly::split_branches).collect();
                for piece in &cut {
                    // overlaps along the cuts: each piece grown, but only into the shape
                    if overlap > 0.0 && cut.len() > 1 {
                        pieces.extend(poly::intersection(&poly::offset(std::slice::from_ref(piece), overlap), std::slice::from_ref(&region)));
                    } else {
                        pieces.push(piece.clone());
                    }
                }
            }
            if pieces.len() >= 2 {
                plans.push((id, pieces));
            }
        }
    }
    if plans.is_empty() {
        return Err(bad(CMD, "select stitched shapes with holes or branches to cut (lines and simple shapes have nothing to cut)"));
    }
    let groups = s.edit("Automatic Knife", |d, sel| {
        let mut groups = Vec::new();
        for (id, pieces) in &plans {
            let n = d.node(*id).cloned().ok_or(EngineError::NoNode(*id))?;
            let (par, idx, _) = d.position(*id).ok_or(EngineError::NoNode(*id))?;
            let mut children = Vec::new();
            for piece in pieces {
                let pid = d.alloc_id();
                let subs: Vec<SubPath> = piece
                    .rings()
                    .filter(|r| r.len() >= 3)
                    .map(|r| SubPath::polyline(&r.iter().map(|q| Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM)).collect::<Vec<_>>(), true))
                    .collect();
                let mut c = Node::path(pid, PathData::new(subs), (*n.appearance).clone());
                if let NodeKind::Path { rule, .. } = &mut c.kind {
                    *rule = vectorcraft_geom::FillRule::EvenOdd;
                }
                c.name = n.name.clone();
                c.opacity = n.opacity;
                c.embroidery = n.embroidery.clone().map(|mut e| {
                    // each piece finds its own direction; where sewing starts and stops belonged
                    // to the whole
                    e.guides.clear();
                    e.auto_direction = true;
                    e.entry = None;
                    e.exit = None;
                    e.frozen = None;
                    e
                });
                children.push(Arc::new(c));
            }
            d.remove(*id)?;
            let gid = d.alloc_id();
            let mut g = Node::group(gid, children);
            g.name = n.name.clone();
            g.embroidery = Some(Box::new(Embroidery::new(StitchType::Branch)));
            d.insert(par, idx, g)?;
            groups.push(gid);
        }
        sel.set(groups.iter().copied());
        Ok(groups)
    })?;
    Ok(ids_json(&groups))
}

/// How many stitched objects `n` is or holds (a branch group counts its members).
fn stitched_nodes_under(n: &Node) -> usize {
    if n.embroidery.is_some() && !matches!(n.kind, NodeKind::Group { .. } | NodeKind::Layer { .. }) {
        return 1;
    }
    n.children().into_iter().flatten().map(|c| stitched_nodes_under(c)).sum()
}

/// The parts of a stitched path or compound path: groups of its subpaths, an outer
/// outline with its holes each. Fewer than two → nothing to break apart.
fn node_parts(n: &Node) -> Vec<Vec<SubPath>> {
    let subs: Vec<SubPath> = match &n.kind {
        NodeKind::Path { path, .. } => path.subpaths.clone(),
        NodeKind::Compound { children, .. } => children
            .iter()
            .flat_map(|c| match &c.kind {
                NodeKind::Path { path, .. } => path.subpaths.clone(),
                _ => Vec::new(),
            })
            .collect(),
        _ => return Vec::new(),
    };
    let refs: Vec<&SubPath> = subs.iter().collect();
    let owner = parts(&refs);
    let mut groups: BTreeMap<usize, Vec<SubPath>> = BTreeMap::new();
    for (sp, o) in subs.iter().zip(owner) {
        groups.entry(o).or_default().push(sp.clone());
    }
    groups.into_values().collect()
}

fn break_apart(s: &mut Session, _p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.breakApart";
    let roots = super::edit::selected_roots(s)?;
    // what each root becomes: a branch's members, or the parts of a many-part object
    let mut plans: Vec<(NodeId, Option<Vec<Vec<SubPath>>>)> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &roots {
            let Some(n) = d.node(*id) else { continue };
            if is_branch(n) {
                plans.push((*id, None));
            } else if n.embroidery.is_some() {
                let p = node_parts(n);
                if p.len() >= 2 {
                    plans.push((*id, Some(p)));
                }
            }
        }
    }
    if plans.is_empty() {
        return Err(bad(CMD, "select a branch, or a stitched object made of separate parts"));
    }
    let out = s.edit("Break Apart", |d, sel| {
        let mut out = Vec::new();
        for (id, parts) in &plans {
            let n = d.node(*id).cloned().ok_or(EngineError::NoNode(*id))?;
            let (par, idx, _) = d.position(*id).ok_or(EngineError::NoNode(*id))?;
            d.remove(*id)?;
            match parts {
                // a branch: its members take its place
                None => {
                    for (k, c) in n.children().into_iter().flatten().enumerate() {
                        let mut c = (**c).clone();
                        c.opacity *= n.opacity;
                        out.push(c.id);
                        d.insert(par, idx + k, c)?;
                    }
                }
                // one path per part, looking and stitched like the whole
                Some(parts) => {
                    for (k, subs) in parts.iter().enumerate() {
                        let pid = d.alloc_id();
                        let mut c = Node::path(pid, PathData::new(subs.clone()), (*n.appearance).clone());
                        if let NodeKind::Path { rule, .. } = &mut c.kind {
                            *rule = vectorcraft_geom::FillRule::EvenOdd;
                        }
                        c.name = n.name.clone();
                        c.opacity = n.opacity;
                        c.embroidery = n.embroidery.clone().map(|mut e| {
                            // where sewing starts and stops belonged to the whole
                            e.entry = None;
                            e.exit = None;
                            e.frozen = None;
                            e
                        });
                        out.push(pid);
                        d.insert(par, idx + k, c)?;
                    }
                }
            }
        }
        sel.set(out.iter().copied());
        Ok(out)
    })?;
    Ok(ids_json(&out))
}

fn auto_sequence(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.autoSequence";
    // With `ids`, only the objects inside those groups trade places.
    let only = match p.get("ids") {
        Some(_) => Some(checked_ids_param(s, p, "ids", CMD)?),
        None => None,
    };
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    // each stitched node as one item: its thread and what its objects cover
    let mut items: BTreeMap<NodeId, Item> = BTreeMap::new();
    for (k, obj) in plan.design.objects.iter().enumerate() {
        let Some(node) = plan.nodes.get(k) else { continue };
        let area = sf_core::engine::object_polygons(obj);
        let bbox = obj.bbox();
        let e = items.entry(*node).or_insert_with(|| Item { color: obj.color, area: Vec::new(), bbox });
        e.area.extend(area);
        e.bbox = e.bbox.union(bbox);
    }
    let mut moved = 0usize;
    s.edit("Auto Sequence", |d, _| {
        // siblings are reordered among themselves, parent by parent
        let mut parents: Vec<Option<NodeId>> = items.keys().map(|id| d.parent_of(*id)).collect();
        parents.sort_by_key(|p| p.map(|x| x.0));
        parents.dedup();
        let inside = |d: &vectorcraft_doc::Document, mut at: Option<NodeId>| {
            while let Some(id) = at {
                if only.as_ref().is_none_or(|o| o.contains(&id)) {
                    return true;
                }
                at = d.parent_of(id);
            }
            false
        };
        for parent in parents.into_iter().flatten() {
            if !inside(d, Some(parent)) {
                continue;
            }
            let Some(children) = d.node(parent).and_then(|n| n.children()).cloned() else { continue };
            let slots: Vec<usize> = children.iter().enumerate().filter(|(_, c)| items.contains_key(&c.id)).map(|(i, _)| i).collect();
            if slots.len() < 2 {
                continue;
            }
            let list: Vec<Item> = slots.iter().filter_map(|&i| items.get(&children[i].id).cloned()).collect();
            let order = sequence(&list, None);
            moved += order.iter().enumerate().filter(|(i, k)| i != *k).count();
            // the stitched objects trade places; anything else stays where it is
            let nodes: Vec<Arc<Node>> = order.iter().filter_map(|&k| slots.get(k).map(|&i| children[i].clone())).collect();
            let ch = d.children_mut(Some(parent))?;
            for (&slot, node) in slots.iter().zip(nodes) {
                ch[slot] = node;
            }
        }
        Ok(())
    })?;
    Ok(json!({ "moved": moved }))
}
