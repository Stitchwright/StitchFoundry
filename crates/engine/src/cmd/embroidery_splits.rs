//! Split lines in tatami fills: each stitch row gets a needle point where it crosses one, so the
//! lines show in the fill. Lines drawn by an agent (`embroidery.setSplits`), carved in from other
//! art (`embroidery.carveSplits`: a closed path presses in its outline like a carving stamp, and a
//! line repeated across the fill makes flexi splits), pressed in from the library of carving
//! stamps (`embroidery.carveStamp`) and cleared (`embroidery.clearSplits`).
//! Programmed splits, a pattern tiled over the fill, are the `texture` stitch parameter.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::model::FillKind;
use sf_core::stamps::Stamp;
use vectorcraft_doc::{Document, NodeId};
use vectorcraft_embroidery::{PT_PER_MM, outlines_mm, regions_mm};

use super::embroidery::{ids_json, stitched_targets};
use super::{
    CommandSpec, Result, Session, always, bad, bool_or, cmd, has_doc, has_selection, id_param, leaf_targets, point_param, str_param, targets,
};
use crate::EngineError;

/// Most split lines one fill keeps, and points in all of them together.
const MAX_LINES: usize = 2_000;
const MAX_POINTS: usize = 200_000;
/// Most copies one repeated line makes across a fill.
const MAX_COPIES: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.setSplits",
            "Set Split Lines",
            [],
            None,
            "{id, splits: [[[x, y], [x, y], …], …]} (points) replace a tatami fill's split lines: each stitch row gets a needle point where it crosses one, so the lines show in the fill → {splits}",
            has_doc,
            set_splits
        ),
        cmd!(
            "embroidery.carveSplits",
            "Carve Split Lines",
            ["Embroidery"],
            None,
            "{ids?, repeat?: mm (copies of each line this far apart across the fill: flexi splits), keep?: bool (keep the carving art; default false)} the selected tatami fills take the paths of the other selected art as split lines (a closed path presses in its outline, like a carving stamp); when every selected object is a tatami fill, the front-most one carves the others → {ids, lines}",
            has_selection,
            carve_splits
        ),
        cmd!(
            "embroidery.contourSplits",
            "Contour Split Lines",
            ["Embroidery"],
            None,
            "{ids?, spacing?: mm (0.5–100; 4)} split lines that follow the selected tatami fills' outlines: each outline, holes too, repeated inward every `spacing` until the shape runs out, so the fill shows rings along its edge → {ids, lines}",
            has_selection,
            contour_splits
        ),
        cmd!(
            "embroidery.carveStamp",
            "Carve Stamp",
            [],
            None,
            "{stamp: star|heart|flower|leaf|spiral|sun|moon|snowflake|paw|swirl|shell|butterfly|clover|feather|rings|diamond, ids?, at?: [x, y] (points; default: each fill's middle), size?: mm (the stamp's width and height, 2–500; default: 60% of the fill's smaller side), angle?: degrees} press a carving stamp from the library into the selected tatami fills as split lines, so it shows embossed in the stitching → {ids, lines}",
            has_selection,
            carve_stamp
        ),
        cmd!("embroidery.stamps", "Carving Stamps", [], None, "{} the carving stamps in the library → {stamps: [{id, name}]}", always, list_stamps),
        cmd!(
            "embroidery.clearSplits",
            "Clear Split Lines",
            ["Embroidery"],
            None,
            "{ids?} remove the split lines of the selected fills → {ids}",
            has_selection,
            clear_splits
        ),
    ]
}

/// Whether an object is stitched as a tatami fill (the fill that takes split lines).
fn tatami(d: &Document, id: NodeId) -> bool {
    d.node(id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e| matches!(&e.stitch, StitchType::Fill(f) if f.kind == FillKind::Tatami))
}

/// Checks that `lines` fit within the limits once added to `have` (lines, points).
fn check_size(cmd: &str, have: (usize, usize), lines: &[Vec<sf_core::Point>]) -> Result<()> {
    let n = have.0 + lines.len();
    let pts = have.1 + lines.iter().map(Vec::len).sum::<usize>();
    if n > MAX_LINES || pts > MAX_POINTS {
        return Err(bad(cmd, format!("too many split lines: a fill keeps at most {MAX_LINES} lines of {MAX_POINTS} points in all")));
    }
    Ok(())
}

fn set_splits(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.setSplits";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing object `id`"))?;
    let d = &s.doc()?.doc;
    d.node(id).ok_or_else(|| bad(CMD, format!("no such object id {}", id.0)))?;
    if !tatami(d, id) {
        return Err(bad(CMD, "split lines go in tatami fills"));
    }
    let list = p.get("splits").and_then(Value::as_array).ok_or_else(|| bad(CMD, "missing `splits` array"))?;
    let total: usize = list.iter().filter_map(Value::as_array).map(Vec::len).sum();
    if list.len() > MAX_LINES || total > MAX_POINTS {
        return Err(bad(CMD, format!("too many split lines: a fill keeps at most {MAX_LINES} lines of {MAX_POINTS} points in all")));
    }
    let mut lines = Vec::with_capacity(list.len());
    for line in list {
        let pts = line.as_array().ok_or_else(|| bad(CMD, format!("each split line is [[x, y], …], not {line}")))?;
        let mut out = Vec::with_capacity(pts.len());
        for q in pts {
            let c: Vec<f64> = q.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let [x, y] = c.as_slice() else {
                return Err(bad(CMD, format!("each point is [x, y], not {q}")));
            };
            if !(x.is_finite() && y.is_finite()) {
                return Err(bad(CMD, "split line points must be finite"));
            }
            out.push(sf_core::Point::new(*x, *y));
        }
        if out.len() < 2 {
            return Err(bad(CMD, "a split line needs two points or more"));
        }
        lines.push(out);
    }
    s.edit("Set Split Lines", |d, _| {
        let e = d.node_mut(id).ok_or(EngineError::NoNode(id))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(id))?;
        e.splits = lines;
        Ok(json!({ "splits": e.splits.len() }))
    })
}

/// Copies of `line` (mm) `step` apart across the box `lo`–`hi` (mm), moved square to the line's
/// overall direction.
fn repeated(line: &[sf_core::Point], step: f64, (lo, hi): (sf_core::Point, sf_core::Point)) -> Vec<Vec<sf_core::Point>> {
    let (Some(&a), Some(&b)) = (line.first(), line.last()) else { return Vec::new() };
    // A closed line's direction runs to its farthest point.
    let far = if a.distance(b) < 1e-9 { line.iter().copied().max_by(|p, q| a.distance(*p).total_cmp(&a.distance(*q))).unwrap_or(b) } else { b };
    let dir = far - a;
    if dir.length() < 1e-9 {
        return vec![line.to_vec()];
    }
    let n = dir.perp().normalized();
    let across = |p: sf_core::Point| p.dot(n);
    let (lmin, lmax) = line.iter().map(|&p| across(p)).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let corners = [lo, sf_core::Point::new(hi.x, lo.y), hi, sf_core::Point::new(lo.x, hi.y)];
    let (tmin, tmax) = corners.iter().map(|&p| across(p)).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| (a.min(v), b.max(v)));
    let (k0, k1) = (((tmin - lmax) / step).ceil(), ((tmax - lmin) / step).floor());
    if !(k0.is_finite() && k1.is_finite()) || k1 < k0 {
        return Vec::new();
    }
    let count = ((k1 - k0) as usize + 1).min(MAX_COPIES);
    (0..count).map(|i| line.iter().map(|&p| p + n * ((k0 + i as f64) * step)).collect()).collect()
}

fn carve_splits(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.carveSplits";
    let repeat = match p.get("repeat") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|r| r.is_finite() && *r >= 0.5).ok_or_else(|| bad(CMD, "`repeat` is a distance of 0.5 mm or more"))?),
    };
    let keep = bool_or(p, "keep", false);
    let d = &s.doc()?.doc;
    let picked = d.paint_order(leaf_targets(s, &targets(s, p)?)?);
    let (mut fills, mut carvers): (Vec<NodeId>, Vec<NodeId>) = picked.iter().partition(|id| tatami(d, **id));
    // Only fills: the front-most carves the others.
    if carvers.is_empty()
        && fills.len() >= 2
        && let Some(front) = fills.pop()
    {
        carvers.push(front);
    }
    if fills.is_empty() || carvers.is_empty() {
        return Err(bad(CMD, "select tatami fills and the art to carve into them"));
    }
    let lines: Vec<Vec<sf_core::Point>> = carvers.iter().filter_map(|id| d.node(*id)).flat_map(outlines_mm).filter(|l| l.len() >= 2).collect();
    if lines.is_empty() {
        return Err(bad(CMD, "the carving art has no paths"));
    }
    // Each fill's lines, in points.
    let mut plans: Vec<(NodeId, Vec<Vec<sf_core::Point>>)> = Vec::new();
    for id in &fills {
        let Some(n) = d.node(*id) else { continue };
        let mm: Vec<Vec<sf_core::Point>> = match repeat {
            Some(step) => {
                let pts: Vec<sf_core::Point> = regions_mm(n).iter().flat_map(|r| r.outer.iter().copied()).collect();
                let bb = sf_core::geom::BBox::from_points(pts);
                if bb.is_empty() {
                    continue;
                }
                lines.iter().flat_map(|l| repeated(l, step, (bb.min, bb.max))).collect()
            }
            None => lines.clone(),
        };
        let pt: Vec<Vec<sf_core::Point>> = mm.into_iter().map(|l| l.into_iter().map(|q| q * PT_PER_MM).collect()).collect();
        let have = n.embroidery.as_deref().map_or((0, 0), |e| (e.splits.len(), e.splits.iter().map(Vec::len).sum()));
        check_size(CMD, have, &pt)?;
        plans.push((*id, pt));
    }
    let added: usize = plans.iter().map(|(_, l)| l.len()).sum();
    let ids: Vec<NodeId> = plans.iter().map(|(id, _)| *id).collect();
    s.edit("Carve Split Lines", |d, sel| {
        for (id, lines) in &plans {
            let e = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(*id))?;
            e.splits.extend(lines.iter().cloned());
        }
        if !keep {
            for id in &carvers {
                if d.node(*id).is_some() {
                    d.remove(*id)?;
                }
            }
            sel.objects.retain(|id| !carvers.contains(id));
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "lines": added }))
}

fn carve_stamp(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.carveStamp";
    let name = str_param(p, "stamp").ok_or_else(|| bad(CMD, "missing `stamp` (embroidery.stamps lists them)"))?;
    let stamp = Stamp::from_id(name).ok_or_else(|| bad(CMD, format!("no stamp `{name}` (embroidery.stamps lists them)")))?;
    let at = match p.get("at") {
        None | Some(Value::Null) => None,
        Some(_) => Some(point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y]"))?),
    };
    let size = match p.get("size") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|v| (2.0..=500.0).contains(v)).ok_or_else(|| bad(CMD, "`size` is 2 to 500 mm"))?),
    };
    let angle = match p.get("angle") {
        None | Some(Value::Null) => 0.0,
        Some(v) => v.as_f64().filter(|v| v.is_finite() && v.abs() <= 3600.0).ok_or_else(|| bad(CMD, "`angle` is in degrees"))?,
    };
    let d = &s.doc()?.doc;
    let fills: Vec<NodeId> = leaf_targets(s, &targets(s, p)?)?.into_iter().filter(|id| tatami(d, *id)).collect();
    if fills.is_empty() {
        return Err(bad(CMD, "select tatami fills to press the stamp into"));
    }
    let (sin, cos) = (-angle).to_radians().sin_cos();
    let mut plans: Vec<(NodeId, Vec<Vec<sf_core::Point>>)> = Vec::new();
    for id in &fills {
        let Some(n) = d.node(*id) else { continue };
        let bb = sf_core::geom::BBox::from_points(regions_mm(n).iter().flat_map(|r| r.outer.clone()));
        if bb.is_empty() {
            continue;
        }
        // in the middle of the fill, or where asked; a little smaller than it
        let c = at.map_or(bb.center(), |q| sf_core::Point::new(q.x, q.y) * vectorcraft_embroidery::MM_PER_PT);
        let k = size.unwrap_or((bb.width().min(bb.height()) * 0.6).clamp(2.0, 500.0));
        let lines: Vec<Vec<sf_core::Point>> = stamp
            .lines()
            .into_iter()
            .map(|l| l.into_iter().map(|q| (c + sf_core::Point::new(q.x * cos - q.y * sin, q.x * sin + q.y * cos) * k) * PT_PER_MM).collect())
            .collect();
        let have = n.embroidery.as_deref().map_or((0, 0), |e| (e.splits.len(), e.splits.iter().map(Vec::len).sum()));
        check_size(CMD, have, &lines)?;
        plans.push((*id, lines));
    }
    let added: usize = plans.iter().map(|(_, l)| l.len()).sum();
    let ids: Vec<NodeId> = plans.iter().map(|(id, _)| *id).collect();
    s.edit("Carve Stamp", |d, _| {
        for (id, lines) in &plans {
            let e = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(*id))?;
            e.splits.extend(lines.iter().cloned());
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "lines": added }))
}

/// Most rings Contour Split Lines draws in one fill.
const MAX_RINGS: usize = 200;

fn contour_splits(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.contourSplits";
    let spacing = match p.get("spacing") {
        None | Some(Value::Null) => 4.0,
        Some(v) => v.as_f64().filter(|v| (0.5..=100.0).contains(v)).ok_or_else(|| bad(CMD, "`spacing` is 0.5 to 100 mm"))?,
    };
    let d = &s.doc()?.doc;
    let fills: Vec<NodeId> = leaf_targets(s, &targets(s, p)?)?.into_iter().filter(|id| tatami(d, *id)).collect();
    if fills.is_empty() {
        return Err(bad(CMD, "select tatami fills"));
    }
    let mut plans: Vec<(NodeId, Vec<Vec<sf_core::Point>>)> = Vec::new();
    for id in &fills {
        let Some(n) = d.node(*id) else { continue };
        // the outline moved in a spacing at a time, until nothing is left
        let mut lines: Vec<Vec<sf_core::Point>> = Vec::new();
        for region in regions_mm(n) {
            for k in 1..=MAX_RINGS {
                let inset = sf_core::geom::poly::offset(std::slice::from_ref(&region), -spacing * k as f64);
                if inset.is_empty() {
                    break;
                }
                for ring in inset.iter().flat_map(|q| q.rings()) {
                    let mut pts: Vec<sf_core::Point> = ring.iter().map(|q| *q * PT_PER_MM).collect();
                    if let Some(&first) = pts.first() {
                        pts.push(first);
                    }
                    if pts.len() >= 4 {
                        lines.push(pts);
                    }
                }
            }
        }
        if lines.is_empty() {
            continue;
        }
        let have = n.embroidery.as_deref().map_or((0, 0), |e| (e.splits.len(), e.splits.iter().map(Vec::len).sum()));
        check_size(CMD, have, &lines)?;
        plans.push((*id, lines));
    }
    if plans.is_empty() {
        return Err(bad(CMD, format!("the fills are too small for rings {spacing} mm apart")));
    }
    let added: usize = plans.iter().map(|(_, l)| l.len()).sum();
    let ids: Vec<NodeId> = plans.iter().map(|(id, _)| *id).collect();
    s.edit("Contour Split Lines", |d, _| {
        for (id, lines) in &plans {
            let e = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(*id))?;
            e.splits.extend(lines.iter().cloned());
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "lines": added }))
}

fn list_stamps(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({ "stamps": Stamp::ALL.iter().map(|s| json!({ "id": s.id(), "name": s.label() })).collect::<Vec<_>>() }))
}

fn clear_splits(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> =
        ids.into_iter().filter(|id| d.node(*id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e: &Embroidery| !e.splits.is_empty())).collect();
    if ids.is_empty() {
        return Err(bad("embroidery.clearSplits", "the selection has no split lines"));
    }
    s.edit("Clear Split Lines", |d, _| {
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.splits.clear();
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}
