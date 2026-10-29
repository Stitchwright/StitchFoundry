//! Embroidery editing and checking: reverse the sewing direction, remove or fill
//! holes, select by thread or stitch type, and look for sewing problems.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sf_core::Cmd;
use vectorcraft_doc::{Node, NodeId, NodeKind};
use vectorcraft_embroidery::{PT_PER_MM, node_thread, stitched_nodes};
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery::{ids_json, stitched_targets};
use super::{CommandSpec, Result, Session, bad, cmd, has_doc, has_selection, targets};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.reverse",
            "Reverse Stitch Direction",
            [],
            None,
            "{ids?} swap where the selected stitched objects start and stop sewing (their start and stop beads), so each one sews the other way → {ids}",
            has_selection,
            reverse
        ),
        cmd!(
            "embroidery.swapSides",
            "Swap Satin Sides",
            [],
            None,
            "{ids?} the selected satin columns start their zigzag on the other side (and so end on the other side too): a shorter way on to what is sewn next. Choosing it again swaps back → {ids}",
            has_selection,
            swap_sides
        ),
        cmd!(
            "embroidery.toColumn",
            "Convert to Satin Column",
            [],
            None,
            "{ids?} each selected closed shape (long and narrow, without holes) becomes a satin column between two rails: its two sides as open paths you can edit, pointed where its ends were round, the way its stitches turned kept as inclination lines; its satin settings stay (a fill: its satin column settings, else the defaults) → {ids, skipped: [id]}",
            has_selection,
            to_column
        ),
        cmd!(
            "embroidery.removeHoles",
            "Remove Holes",
            [],
            None,
            "{maxArea?: mm² (only holes smaller than this), ids?} delete the holes (inner outlines) of the selected paths and compound paths → {ids, removed}",
            has_selection,
            remove_holes
        ),
        cmd!(
            "embroidery.fillHoles",
            "Fill Holes",
            [],
            None,
            "{ids?} add an object in each hole of the selected stitched paths, with the same stitch type and look, sewn right after it (recolour one to sew the hole in another thread) → {ids}",
            has_selection,
            fill_holes
        ),
        cmd!(
            "select.same.thread",
            "Thread Color",
            ["Select", "Same"],
            None,
            "{} the stitched objects sewn in the first selected object's thread colour",
            has_selection,
            |s, _| {
                super::select::same(s, "select.same.thread", |a, b| match (a.embroidery.as_deref(), b.embroidery.as_deref()) {
                    (Some(ea), Some(eb)) => node_thread(a, ea).color == node_thread(b, eb).color,
                    _ => false,
                })
            }
        ),
        cmd!(
            "select.same.stitchType",
            "Stitch Type",
            ["Select", "Same"],
            None,
            "{} the stitched objects with the first selected object's stitch type (run styles and fill styles count as types)",
            has_selection,
            |s, _| {
                super::select::same(s, "select.same.stitchType", |a, b| match (a.embroidery.as_deref(), b.embroidery.as_deref()) {
                    (Some(ea), Some(eb)) => ea.stitch.label() == eb.stitch.label(),
                    _ => false,
                })
            }
        ),
        cmd!(
            query "embroidery.check",
            "Check Design",
            [],
            None,
            "{} look for sewing problems: very dense spots, long stitches (7 mm on clothing, else 12.1: embroidery.settings checks), many very short stitches, satin too narrow, fill stitches too short, tiny objects, small lettering, objects that make no stitches, a design larger than its hoop → {findings: [{kind: dense|longStitches|shortStitches|narrowSatin|shortFill|tiny|smallLettering|noStitches|hoop, severity: warning|info, message, ids, at?: [x, y] points, value?}], count}",
            has_doc,
            check
        ),
        cmd!(
            query "embroidery.densityMap",
            "Density Map",
            [],
            None,
            "{cell?: mm (1; 0.5–10, larger on a big design so the map stays within 250000 cells)} how deep the thread lies over the design: the layers of thread on each square cell (a layer is the fabric covered once: a tatami fill about 1.2, a satin column 2.5 to 3.5), what View › Density Map colours, and the depth Check Design calls dense → {origin: [x, y] points (the first cell's top left), cell: mm, columns, rows, layers: [row by row from the top, to 0.1], max, limit}",
            has_doc,
            density_map
        ),
    ]
}

/// Most cells `embroidery.densityMap` returns.
const MAX_MAP_CELLS: f64 = 250_000.0;

fn density_map(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.densityMap";
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let pat = plan.pattern();
    let limit =
        s.doc()?.doc.embroidery.as_deref().map(|e| e.checks.dense_layers).unwrap_or_else(|| sf_core::attr::CheckLimits::default().dense_layers);
    let b = pat.bounds();
    let area = if b.width().is_finite() && b.height().is_finite() { b.width().max(0.0) * b.height().max(0.0) } else { 0.0 };
    let cell = p.get("cell").and_then(Value::as_f64).filter(|c| c.is_finite()).unwrap_or(CELL).clamp(0.5, 10.0).max((area / MAX_MAP_CELLS).sqrt());
    let g = sf_core::density::LayerGrid::of(pat, cell);
    let round = |v: f32| (f64::from(v) * 10.0).round() / 10.0;
    Ok(json!({
        "origin": [g.origin.x * PT_PER_MM, g.origin.y * PT_PER_MM],
        "cell": (g.cell * 1000.0).round() / 1000.0,
        "columns": g.columns,
        "rows": g.rows,
        "layers": g.layers.iter().map(|v| round(*v)).collect::<Vec<_>>(),
        "max": round(g.max()),
        "limit": limit,
    }))
}

// ---------- reverse ----------

fn reverse(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.reverse";
    let ids = stitched_targets(s, p)?;
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let pat = plan.pattern();
    let mut ends: Vec<(NodeId, sf_core::Point, sf_core::Point)> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            if d.node(*id).and_then(|n| n.embroidery.as_deref()).is_none() {
                continue;
            }
            let ranges = plan.records_of(*id);
            let first = ranges.first().and_then(|r| pat.stitches.get(r.clone())).and_then(|st| st.iter().find(|s| s.cmd == Cmd::Stitch));
            let last = ranges.last().and_then(|r| pat.stitches.get(r.clone())).and_then(|st| st.iter().rev().find(|s| s.cmd == Cmd::Stitch));
            if let (Some(a), Some(b)) = (first, last) {
                ends.push((*id, a.pos() * PT_PER_MM, b.pos() * PT_PER_MM));
            }
        }
    }
    if ends.is_empty() {
        return Err(bad(CMD, "select stitched objects"));
    }
    s.edit("Reverse Stitch Direction", |d, _| {
        for (id, start, stop) in &ends {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.entry = Some(*stop);
                e.exit = Some(*start);
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&ends.iter().map(|e| e.0).collect::<Vec<_>>()))
}

// ---------- swap satin sides ----------

fn swap_sides(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.swapSides";
    let ids = stitched_targets(s, p)?;
    let satin: Vec<NodeId> = {
        let d = &s.doc()?.doc;
        ids.into_iter()
            .filter(|id| matches!(d.node(*id).and_then(|n| n.embroidery.as_deref()).map(|e| &e.stitch), Some(sf_core::attr::StitchType::Satin(_))))
            .collect()
    };
    if satin.is_empty() {
        return Err(bad(CMD, "select satin columns"));
    }
    s.edit("Swap Satin Sides", |d, _| {
        for id in &satin {
            if let Some(sf_core::attr::StitchType::Satin(sp)) =
                d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut().map(|e| &mut e.stitch)
            {
                sp.swap_sides = !sp.swap_sides;
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&satin))
}

// ---------- convert to satin column ----------

fn to_column(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.toColumn";
    let ids = stitched_targets(s, p)?;
    let mut jobs: Vec<(NodeId, PathData, sf_core::attr::Embroidery)> = Vec::new();
    let mut skipped: Vec<u64> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(n) = d.node(*id) else { continue };
            match n.embroidery.as_deref().filter(|_| matches!(n.kind, NodeKind::Path { .. })).and_then(|e| column_of(n, e)) {
                Some((path, e)) => jobs.push((*id, path, e)),
                None => skipped.push(id.0),
            }
        }
    }
    if jobs.is_empty() {
        return Err(bad(CMD, "select closed shapes without holes, long and narrow enough to sew as a satin column"));
    }
    s.edit("Convert to Satin Column", |d, _| {
        for (id, path, e) in &jobs {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            if let NodeKind::Path { path: p, .. } = &mut n.kind {
                *p = path.clone();
            }
            n.embroidery = Some(Box::new(e.clone()));
        }
        Ok(())
    })?;
    Ok(json!({ "ids": jobs.iter().map(|j| j.0.0).collect::<Vec<_>>(), "skipped": skipped }))
}

/// A closed shape as a satin column: its outline split into two sides where the automatic
/// direction (or the shape's own inclination lines) pairs them, each end's round shared out so
/// both sides meet at its tip, and the pairings kept as inclination lines.
fn column_of(n: &Node, e: &sf_core::attr::Embroidery) -> Option<(PathData, sf_core::attr::Embroidery)> {
    use sf_core::direction::{Guide, auto_guides, strip_from_guides};
    use sf_core::geom::polyline;
    let regions = vectorcraft_embroidery::regions_mm(n);
    let [region] = regions.as_slice() else { return None };
    if !region.holes.is_empty() {
        return None;
    }
    let ring = &region.outer;
    let mm = 1.0 / PT_PER_MM;
    let mut guides: Vec<Guide> = e.guides.iter().map(|g| Guide::new(g.a * mm, g.b * mm)).collect();
    if guides.len() < 2 {
        guides = auto_guides(ring)?;
    }
    let strip = strip_from_guides(ring, &guides)?;
    // a polyline cut in two halves at half its length
    let halves = |pts: &[sf_core::Point]| -> (Vec<sf_core::Point>, Vec<sf_core::Point>) {
        let half = polyline::length(pts) * 0.5;
        let (mut a, mut b, mut run) = (Vec::new(), Vec::new(), 0.0);
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let len = p.distance(q);
            if run + len < half {
                a.push(p);
            } else if run < half {
                let tip = p.lerp(q, if len > 1e-12 { (half - run) / len } else { 0.0 });
                a.extend([p, tip]);
                b.push(tip);
            }
            if run >= half {
                b.push(p);
            }
            run += len;
        }
        if let Some(&last) = pts.last() {
            b.push(last);
        }
        (a, b)
    };
    // side A: round the start from its tip, along side A, round the far end to its tip; side B the same
    let (start_a, start_b) = halves(&strip.cap_start);
    let (end_a, end_b) = halves(&strip.cap_end);
    let side = |head: Vec<sf_core::Point>, rails: &[Vec<sf_core::Point>], tail: Vec<sf_core::Point>| {
        let mut out: Vec<sf_core::Point> = Vec::new();
        for q in head.into_iter().chain(rails.iter().flatten().copied()).chain(tail) {
            if out.last().is_none_or(|l: &sf_core::Point| l.distance(q) > 1e-6) {
                out.push(q);
            }
        }
        polyline::simplify(&out, 0.02)
    };
    let a = side(start_a.into_iter().rev().collect(), &strip.rails_a, end_a);
    let b = side(start_b, &strip.rails_b, end_b.into_iter().rev().collect());
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let to_pt = |pts: &[sf_core::Point]| pts.iter().map(|q| vectorcraft_geom::Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM)).collect::<Vec<_>>();
    let path = PathData::new(vec![SubPath::polyline(&to_pt(&a), false), SubPath::polyline(&to_pt(&b), false)]);
    let params = match &e.stitch {
        sf_core::attr::StitchType::Satin(p) => p.clone(),
        sf_core::attr::StitchType::Fill(f) => f.satin_column.clone().unwrap_or_default(),
        _ => sf_core::model::SatinParams::default(),
    };
    let mut e = e.clone();
    e.stitch = sf_core::attr::StitchType::Satin(params);
    e.guides = strip.stations.iter().map(|(p, q)| Guide::new(*p * PT_PER_MM, *q * PT_PER_MM)).collect();
    e.width = None;
    e.splits.clear();
    e.flow.clear();
    Some((path, e))
}

// ---------- holes ----------

/// A subpath of a path or compound path: the node holding it (the path, or the
/// compound's child path) and its index there.
struct Ring {
    owner: NodeId,
    index: usize,
    sub: SubPath,
}

fn rings_of(n: &Node) -> Vec<Ring> {
    match &n.kind {
        NodeKind::Path { path, .. } => path.subpaths.iter().enumerate().map(|(index, sp)| Ring { owner: n.id, index, sub: sp.clone() }).collect(),
        NodeKind::Compound { children, .. } => children
            .iter()
            .flat_map(|c| match &c.kind {
                NodeKind::Path { path, .. } => {
                    path.subpaths.iter().enumerate().map(|(index, sp)| Ring { owner: c.id, index, sub: sp.clone() }).collect::<Vec<_>>()
                }
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The holes of a path or compound path: its rings and, for each hole, its area (mm²).
fn holes_of(n: &Node) -> Vec<(Ring, f64)> {
    let rings = rings_of(n);
    let subs: Vec<&SubPath> = rings.iter().map(|r| &r.sub).collect();
    let holes = vectorcraft_embroidery::holes(&subs);
    rings.into_iter().zip(holes).filter_map(|(r, h)| h.map(|a| (r, a))).collect()
}

/// The selected paths and compound paths (inside groups too).
fn shape_targets(s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    let d = &s.doc()?.doc;
    fn visit(n: &Node, out: &mut Vec<NodeId>) {
        match &n.kind {
            NodeKind::Path { .. } | NodeKind::Compound { .. } => out.push(n.id),
            _ => {
                for c in n.children().into_iter().flatten() {
                    visit(c, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    for id in targets(s, p)? {
        if let Some(n) = d.node(id) {
            visit(n, &mut out);
        }
    }
    Ok(out)
}

fn remove_holes(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.removeHoles";
    let max_area = match p.get("maxArea") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|a| a.is_finite() && *a > 0.0).ok_or_else(|| bad(CMD, "`maxArea` is a positive number of mm²"))?),
    };
    let ids = shape_targets(s, p)?;
    if ids.is_empty() {
        return Err(bad(CMD, "select paths or compound paths"));
    }
    // Per path that holds a hole: the subpath indices to delete.
    let mut doomed: BTreeMap<NodeId, BTreeSet<usize>> = BTreeMap::new();
    let mut touched = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(n) = d.node(*id) else { continue };
            let holes: Vec<(Ring, f64)> = holes_of(n).into_iter().filter(|(_, a)| max_area.is_none_or(|m| *a < m)).collect();
            if !holes.is_empty() {
                touched.push(*id);
            }
            for (r, _) in holes {
                doomed.entry(r.owner).or_default().insert(r.index);
            }
        }
    }
    let removed: usize = doomed.values().map(BTreeSet::len).sum();
    if removed == 0 {
        return Ok(json!({ "ids": [], "removed": 0 }));
    }
    s.edit("Remove Holes", |d, _| {
        let mut emptied = Vec::new();
        for (owner, indices) in &doomed {
            let n = d.node_mut(*owner).ok_or(EngineError::NoNode(*owner))?;
            if let NodeKind::Path { path, .. } = &mut n.kind {
                let mut i = 0;
                path.subpaths.retain(|_| {
                    let keep = !indices.contains(&i);
                    i += 1;
                    keep
                });
                if path.subpaths.is_empty() {
                    emptied.push(*owner);
                }
            }
        }
        // A compound's child path left without outlines goes.
        for id in emptied {
            if !touched.contains(&id) {
                d.remove(id)?;
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": touched.iter().map(|i| i.0).collect::<Vec<_>>(), "removed": removed }))
}

fn fill_holes(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fillHoles";
    let ids = stitched_targets(s, p)?;
    // Per stitched object: the outlines of its holes.
    let mut jobs: Vec<(NodeId, Vec<SubPath>)> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(n) = d.node(*id) else { continue };
            if n.embroidery.is_none() {
                continue;
            }
            let holes: Vec<SubPath> = holes_of(n).into_iter().map(|(r, _)| r.sub).filter(|sp| sp.anchors.len() >= 3).collect();
            if !holes.is_empty() {
                jobs.push((*id, holes));
            }
        }
    }
    if jobs.is_empty() {
        return Err(bad(CMD, "select stitched shapes that have holes"));
    }
    let made = s.edit("Fill Holes", |d, sel| {
        let mut made = Vec::new();
        for (id, holes) in &jobs {
            let (parent, index, _) = d.position(*id).ok_or(EngineError::NoNode(*id))?;
            let src = d.node(*id).ok_or(EngineError::NoNode(*id))?.clone();
            for (k, hole) in holes.iter().enumerate() {
                let mut sp = hole.clone();
                sp.closed = true;
                let nid = d.alloc_id();
                let kind = NodeKind::Path {
                    path: PathData::single(sp),
                    rule: vectorcraft_geom::FillRule::NonZero,
                    live: None,
                    clipping: false,
                    guide: false,
                };
                let mut n = Node::new(nid, kind);
                n.appearance = src.appearance.clone();
                n.name = Some("Hole fill".into());
                if let Some(e) = src.embroidery.as_deref() {
                    let mut e = e.clone();
                    // The hole has its own shape: its own direction and start and stop.
                    e.guides.clear();
                    e.auto_direction = true;
                    e.entry = None;
                    e.exit = None;
                    n.embroidery = Some(Box::new(e));
                }
                d.insert(parent, index + 1 + k, n)?;
                made.push(nid);
            }
        }
        sel.set(made.iter().copied());
        Ok(made)
    })?;
    Ok(ids_json(&made))
}

// ---------- design checks ----------

// What is flagged (too dense, long stitches, small lettering, tiny objects) is the
// design's `CheckLimits`. Thread lying more than 4.5 layers deep by default: a layer is
// the fabric covered once (a tatami fill is about 1.2 with its underlay, a satin column
// 2.5 to 3.5), so lettering, borders over fills and two overlapping fills stay under
// it, and four stacked fills, crossing columns or a column piling thread up inside a
// sharp turn go over it.

/// Density grid cell (mm).
const CELL: f64 = 1.0;
/// Most findings of one kind reported.
const MAX_PER_KIND: usize = 20;
/// Short stitches in one object before they are flagged.
const SHORT_STITCHES: usize = 10;

fn finding(kind: &str, severity: &str, message: String, ids: &[NodeId], at: Option<sf_core::Point>, value: Option<f64>) -> Value {
    let mut v = json!({ "kind": kind, "severity": severity, "message": message, "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() });
    if let Some(p) = at {
        v["at"] = json!([p.x * PT_PER_MM, p.y * PT_PER_MM]);
    }
    if let Some(x) = value {
        v["value"] = json!((x * 10.0).round() / 10.0);
    }
    v
}

/// [`check`]'s findings for each stitched object: no stitches, tiny (`tiny` mm across) and lettering
/// under `small_lettering` mm tall.
fn object_findings(
    doc: &vectorcraft_doc::Document,
    plan: &vectorcraft_embroidery::StitchPlan,
    (tiny, small_lettering): (f64, f64),
    findings: &mut Vec<Value>,
) {
    let pat = plan.pattern();
    for n in stitched_nodes(doc) {
        // a branch group only says how its members are sewn, lettering what they say
        if n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, sf_core::attr::StitchType::Branch | sf_core::attr::StitchType::Lettering(_))) {
            continue;
        }
        let ranges = plan.records_of(n.id);
        let moves: Vec<sf_core::Point> =
            ranges.iter().filter_map(|r| pat.stitches.get(r.clone())).flatten().filter(|s| s.cmd == Cmd::Stitch).map(|s| s.pos()).collect();
        let Some(bb) = (!moves.is_empty()).then(|| sf_core::BBox::from_points(moves.iter().copied())) else {
            findings.push(finding("noStitches", "info", "Makes no stitches".into(), &[n.id], None, None));
            continue;
        };
        if bb.width().max(bb.height()) < tiny {
            findings.push(finding("tiny", "info", "Tiny object: may not sew cleanly".into(), &[n.id], Some(bb.center()), None));
        } else if matches!(n.kind, NodeKind::Text(_)) && bb.height() < small_lettering {
            findings.push(finding(
                "smallLettering",
                "warning",
                format!("Lettering only {:.1} mm tall: letters under about {small_lettering} mm are hard to sew clearly", bb.height()),
                &[n.id],
                Some(bb.center()),
                Some(bb.height()),
            ));
        }
    }
}

fn check(s: &mut Session, _p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.check";
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let st = s.doc()?;
    let doc = &st.doc;
    let pat = plan.pattern();
    let lim = doc.embroidery.as_deref().map(|e| e.checks.clone()).unwrap_or_default();
    let (dense_layers, long_stitch, small_lettering, tiny) = (lim.dense_layers, lim.long_stitch, lim.small_lettering, lim.tiny_object);
    let mut findings: Vec<Value> = Vec::new();

    // Objects: no stitches, tiny, small lettering.
    object_findings(doc, &plan, (tiny, small_lettering), &mut findings);

    // Satin too narrow to sit up; tatami stitches short enough to ripple after washing.
    let mut flagged: Vec<(NodeId, &str)> = Vec::new();
    for (id, obj) in plan.nodes.iter().zip(&plan.design.objects) {
        match &obj.kind {
            sf_core::model::ObjectKind::Satin(sat) if lim.narrow_satin > 0.0 => {
                let w = sat.column.average_width();
                if w.is_finite() && w < lim.narrow_satin && !flagged.contains(&(*id, "narrowSatin")) {
                    flagged.push((*id, "narrowSatin"));
                    let at = sat.column.rungs.get(sat.column.rungs.len() / 2).map(|r| r.center());
                    findings.push(finding(
                        "narrowSatin",
                        "warning",
                        format!("Satin only {w:.1} mm wide: under about {} mm it sinks into the fabric; use a run or widen it", lim.narrow_satin),
                        &[*id],
                        at,
                        Some(w),
                    ));
                }
            }
            sf_core::model::ObjectKind::Fill(f)
                if f.params.kind == sf_core::model::FillKind::Tatami
                    && f.params.stitch_length < lim.short_fill
                    && !flagged.contains(&(*id, "shortFill")) =>
            {
                flagged.push((*id, "shortFill"));
                findings.push(finding(
                    "shortFill",
                    "info",
                    format!("Fill stitches {:.1} mm long: under about {} mm a fill can ripple after washing", f.params.stitch_length, lim.short_fill),
                    &[*id],
                    Some(f.shape.bbox().center()),
                    Some(f.params.stitch_length),
                ));
            }
            _ => {}
        }
    }

    // Stitches: needle to needle while the thread is attached.
    let mut long: BTreeMap<NodeId, (usize, f64, sf_core::Point)> = BTreeMap::new();
    let mut short: BTreeMap<NodeId, (usize, sf_core::Point)> = BTreeMap::new();
    let b = pat.bounds();
    let layers = sf_core::density::LayerGrid::of(pat, CELL);
    let (gw, gh, grid, cell_mm, origin) = (layers.columns, layers.rows, &layers.layers, layers.cell, layers.origin);
    let mut last_needle: Option<sf_core::Point> = None;
    for (i, rec) in pat.stitches.iter().enumerate() {
        match rec.cmd {
            Cmd::Stitch => {
                let q = rec.pos();
                if let Some(p) = last_needle {
                    let len = p.distance(q);
                    if len < lim.short_stitch
                        && let Some(id) = plan.node_at_record(i)
                    {
                        short.entry(id).or_insert((0, q)).0 += 1;
                    }
                    if len > long_stitch
                        && let Some(id) = plan.node_at_record(i)
                    {
                        let e = long.entry(id).or_insert((0, 0.0, q));
                        e.0 += 1;
                        if len > e.1 {
                            *e = (e.0, len, p.midpoint(q));
                        }
                    }
                }
                last_needle = Some(q);
            }
            Cmd::Trim | Cmd::ColorChange | Cmd::End => last_needle = None,
            Cmd::Jump | Cmd::Stop => {}
        }
    }
    // a few short stitches (ties, the ends of rows) are harmless; many in one object aren't
    for (id, (count, at)) in short.into_iter().filter(|(_, (n, _))| *n >= SHORT_STITCHES).take(MAX_PER_KIND) {
        findings.push(finding(
            "shortStitches",
            "warning",
            format!("{count} stitches shorter than {} mm: the needle sews in one spot and can break the thread", lim.short_stitch),
            &[id],
            Some(at),
            Some(count as f64),
        ));
    }
    for (id, (count, max, at)) in long.into_iter().take(MAX_PER_KIND) {
        findings.push(finding(
            "longStitches",
            "warning",
            format!("{count} stitches longer than {long_stitch} mm (up to {max:.1} mm) can snag"),
            &[id],
            Some(at),
            Some(max),
        ));
    }

    // Dense spots: clusters of windows with too many layers of thread. Thread is
    // measured over 2 × 2 cell windows, so a lock stitch's few short stitches or a
    // row landing on a cell edge doesn't count as a pile-up.
    let (ww, wh) = (gw.saturating_sub(1), gh.saturating_sub(1));
    let window: Vec<f32> = (0..ww * wh)
        .map(|w| (w % ww, w / ww))
        .map(|(x, y)| (grid[y * gw + x] + grid[y * gw + x + 1] + grid[(y + 1) * gw + x] + grid[(y + 1) * gw + x + 1]) / 4.0)
        .collect();
    let mut seen = vec![false; window.len()];
    let mut clusters: Vec<(f32, sf_core::Point)> = Vec::new();
    for start in 0..window.len() {
        if seen[start] || f64::from(window[start]) < dense_layers {
            continue;
        }
        let mut stack = vec![start];
        seen[start] = true;
        let (mut best, mut at) = (0.0f32, start);
        while let Some(c) = stack.pop() {
            if window[c] > best {
                (best, at) = (window[c], c);
            }
            let (x, y) = ((c % ww) as i64, (c / ww) as i64);
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= ww as i64 || ny >= wh as i64 {
                    continue;
                }
                let nc = ny as usize * ww + nx as usize;
                if !seen[nc] && f64::from(window[nc]) >= dense_layers {
                    seen[nc] = true;
                    stack.push(nc);
                }
            }
        }
        // a window's centre is the corner its four cells share
        let centre = sf_core::Point::new(origin.x + ((at % ww) as f64 + 1.0) * cell_mm, origin.y + ((at / ww) as f64 + 1.0) * cell_mm);
        clusters.push((best, centre));
    }
    clusters.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (layers, at) in clusters.into_iter().take(MAX_PER_KIND) {
        // The objects sewn within half a cell of the spot.
        let mut ids: Vec<NodeId> = Vec::new();
        for (i, rec) in pat.stitches.iter().enumerate() {
            if rec.cmd == Cmd::Stitch
                && rec.pos().distance(at) < CELL
                && let Some(id) = plan.node_at_record(i)
                && !ids.contains(&id)
            {
                ids.push(id);
            }
        }
        findings.push(finding(
            "dense",
            "warning",
            format!("Very dense: about {layers:.1} layers of thread here can break needles or pucker the fabric"),
            &ids,
            Some(at),
            Some(f64::from(layers)),
        ));
    }

    // The hoop.
    if let Some(h) = doc.embroidery.as_deref().and_then(|e| e.hoop.as_ref()) {
        let centered =
            sf_core::BBox::new(sf_core::Point::new(-b.width() / 2.0, -b.height() / 2.0), sf_core::Point::new(b.width() / 2.0, b.height() / 2.0));
        if !b.is_empty() && !h.fits(&centered) {
            findings.push(finding(
                "hoop",
                "warning",
                format!("The design ({:.0} × {:.0} mm) is larger than the {} hoop", b.width(), b.height(), h.name),
                &[],
                Some(b.center()),
                None,
            ));
        }
    }
    let count = findings.len();
    Ok(json!({ "findings": findings, "count": count }))
}
