//! Flow lines in tatami fills: the rows follow drawn lines instead of running straight. One line
//! makes rows that copy it, moved across the fill (a Florentine effect); two make rows that
//! change shape evenly from the first line to the second (a liquid effect). Taken from other
//! selected art (`embroidery.fillAlongLines`), set by an agent (`embroidery.setFlowLines`) and
//! cleared (`embroidery.clearFlowLines`).

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::direction::Guide;
use sf_core::model::FillKind;
use vectorcraft_doc::{Document, NodeId};
use vectorcraft_embroidery::{PT_PER_MM, outlines_mm};

use super::embroidery::{ids_json, stitched_targets};
use super::{CommandSpec, Result, Session, bad, bool_or, cmd, has_doc, has_selection, id_param, leaf_targets, targets};
use crate::EngineError;

/// Flow lines one fill keeps, and points in each.
const MAX_LINES: usize = 2;
const MAX_POINTS: usize = 20_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.setFlowLines",
            "Set Flow Lines",
            [],
            None,
            "{id, lines: [[[x, y], [x, y], …], …]} (points; none, one or two lines) replace a tatami fill's flow lines: its rows follow them, copying one line or changing shape from the first to the second → {lines}",
            has_doc,
            set_flow_lines
        ),
        cmd!(
            "embroidery.lineAsDirection",
            "Use Line as Direction",
            ["Embroidery"],
            None,
            "{ids?, keep?: bool (keep the lines' art; default false)} each other selected line (a path that isn't stitched) becomes a stitch direction line of the selected stitched shapes it crosses: their stitches run parallel to it, from its first point to its last, as the Inclination tool's lines; several lines across a shape make its stitches turn from one to the next; one undo step → {ids, lines}",
            has_selection,
            line_as_direction
        ),
        cmd!(
            "embroidery.fillAlongLines",
            "Fill Along Lines",
            ["Embroidery"],
            None,
            "{ids?, keep?: bool (keep the lines' art; default false)} the selected tatami fills sew their rows along the other selected lines (the first two): one line, rows that copy it, moved across the fill (a Florentine effect); two, rows that change shape evenly from one to the other (a liquid effect); draw them across or beside the fill, the same way round → {ids, lines}",
            has_selection,
            fill_along_lines
        ),
        cmd!(
            "embroidery.waveFill",
            "Wave Fill",
            ["Embroidery"],
            None,
            "{ids?, waves?: whole waves across the shape (3; 1–20), height?: mm from crest to trough (5; 0.5–60), angle?: degrees the rows run at (0)} the selected shapes sewn as tatami fills whose rows follow a wave: one wavy flow line made for each, so every row is a copy of it moved across the shape; one undo step → {ids}",
            has_selection,
            wave_fill
        ),
        cmd!(
            "embroidery.clearFlowLines",
            "Clear Flow Lines",
            ["Embroidery"],
            None,
            "{ids?} the selected fills sew straight rows again → {ids}",
            has_selection,
            clear_flow_lines
        ),
    ]
}

/// Whether an object is stitched as a tatami fill (the fill whose rows follow flow lines).
fn tatami(d: &Document, id: NodeId) -> bool {
    d.node(id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e| matches!(&e.stitch, StitchType::Fill(f) if f.kind == FillKind::Tatami))
}

/// Points a wave line is drawn with.
const WAVE_POINTS: usize = 400;

fn wave_fill(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.waveFill";
    let waves = super::embroidery_lettering::ranged(p, "waves", 1.0..=20.0, CMD, "`waves` is 1 to 20")?.unwrap_or(3.0).round();
    let height = super::embroidery_lettering::ranged(p, "height", 0.5..=60.0, CMD, "`height` is 0.5 to 60 mm")?.unwrap_or(5.0);
    let angle = super::embroidery_lettering::ranged(p, "angle", -360.0..=360.0, CMD, "`angle` is in degrees")?.unwrap_or(0.0);
    let ids: Vec<u64> = match p.get("ids").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(Value::as_u64).collect(),
        None => s.active().map(|d| d.selection.objects.iter().map(|n| n.0).collect()).unwrap_or_default(),
    };
    if ids.is_empty() {
        return Err(bad(CMD, "select the shapes to fill with waves"));
    }
    let shapes = super::embroidery_stumpwork::closed_shapes(s, &ids, CMD, "wave fills")?;
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Wave Fill".into()));
    }
    let done = (|| -> Result<Vec<u64>> {
        let mut made = Vec::new();
        for (id, n) in &shapes {
            let Some(b) = n.geometric_bounds() else { continue };
            if !tatami(&s.doc()?.doc, NodeId(*id)) {
                s.execute("select.set", &json!({ "ids": [id] }))?;
                s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            }
            // a line through the middle along the rows, past the shape at both ends, rising and
            // falling across it `waves` times
            let (sin, cos) = angle.to_radians().sin_cos();
            let reach = b.width().hypot(b.height()) * 0.6;
            let c = b.center();
            let amp = height / 2.0 * PT_PER_MM;
            let line: Vec<Value> = (0..=WAVE_POINTS)
                .map(|k| {
                    let t = k as f64 / WAVE_POINTS as f64 * 2.0 - 1.0;
                    let along = t * reach;
                    // the waves counted over the shape's own width along the rows
                    let span = (b.width() * cos.abs() + b.height() * sin.abs()).max(1.0);
                    let across = amp * (along / span * waves * std::f64::consts::TAU).sin();
                    json!([c.x + along * cos - across * sin, c.y + along * sin + across * cos])
                })
                .collect();
            s.execute("embroidery.setFlowLines", &json!({ "id": id, "lines": [line] }))?;
            made.push(*id);
        }
        Ok(made)
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let made = done?;
    s.execute("select.set", &json!({ "ids": made }))?;
    Ok(json!({ "ids": made }))
}

/// Lines a shape takes from one Use Line as Direction, and points read from each line.
const MAX_DIRECTION_LINES: usize = 200;

fn line_as_direction(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.lineAsDirection";
    let keep = bool_or(p, "keep", false);
    let d = &s.doc()?.doc;
    let picked = d.paint_order(leaf_targets(s, &targets(s, p)?)?);
    let (shapes, art): (Vec<NodeId>, Vec<NodeId>) = picked.iter().partition(|id| d.node(**id).is_some_and(|n| n.embroidery.is_some()));
    if shapes.is_empty() || art.is_empty() {
        return Err(bad(CMD, "select stitched shapes and the lines drawn across them for their stitch direction"));
    }
    // each line: its first and last point (pt), and the shapes it crosses (a point of it inside)
    let areas: Vec<(NodeId, vectorcraft_geom::BezPath)> = shapes.iter().filter_map(|id| Some((*id, d.node(*id)?.stroke_path()?))).collect();
    let mut adds: Vec<(NodeId, Guide)> = Vec::new();
    let mut used: Vec<NodeId> = Vec::new();
    for id in art.iter().take(MAX_DIRECTION_LINES) {
        let Some(n) = d.node(*id) else { continue };
        for line in outlines_mm(n).into_iter().filter(|l| l.len() >= 2 && l.len() <= MAX_POINTS) {
            let pts: Vec<kurbo::Point> = line.iter().map(|q| kurbo::Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM)).collect();
            let (Some(a), Some(b)) = (pts.first().copied(), pts.last().copied()) else { continue };
            if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) || (a - b).hypot() < 1e-3 {
                continue;
            }
            // the line's points and the middles between them
            let probes: Vec<kurbo::Point> = pts.windows(2).flat_map(|w| [w[0], w[0].midpoint(w[1])]).chain([b]).collect();
            for (shape, area) in &areas {
                if probes.iter().any(|q| kurbo::Shape::winding(area, *q) != 0) {
                    adds.push((*shape, Guide::new(sf_core::Point::new(a.x, a.y), sf_core::Point::new(b.x, b.y))));
                    if !used.contains(id) {
                        used.push(*id);
                    }
                }
            }
        }
    }
    if adds.is_empty() {
        return Err(bad(CMD, "none of the lines crosses a selected stitched shape: draw them across the shapes"));
    }
    let shapes_set: Vec<NodeId> = adds.iter().map(|(s, _)| *s).fold(Vec::new(), |mut v, s| {
        if !v.contains(&s) {
            v.push(s);
        }
        v
    });
    let lines = used.len();
    s.edit("Use Line as Direction", |d, sel| {
        for (shape, g) in &adds {
            let e = d.node_mut(*shape).ok_or(EngineError::NoNode(*shape))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(*shape))?;
            let at = super::embroidery::best_insert(&e.guides, g).min(e.guides.len());
            e.guides.insert(at, *g);
            e.auto_direction = false;
        }
        if !keep {
            for id in &used {
                if d.node(*id).is_some() {
                    d.remove(*id)?;
                }
            }
            sel.objects.retain(|id| !used.contains(id));
        }
        Ok(())
    })?;
    Ok(json!({ "ids": shapes_set.iter().map(|i| i.0).collect::<Vec<_>>(), "lines": lines }))
}

fn set_flow_lines(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.setFlowLines";
    let id = id_param(p, "id").ok_or_else(|| bad(CMD, "missing object `id`"))?;
    let d = &s.doc()?.doc;
    d.node(id).ok_or_else(|| bad(CMD, format!("no such object id {}", id.0)))?;
    if !tatami(d, id) {
        return Err(bad(CMD, "flow lines go in tatami fills"));
    }
    let list = p.get("lines").and_then(Value::as_array).ok_or_else(|| bad(CMD, "missing `lines` array"))?;
    if list.len() > MAX_LINES {
        return Err(bad(CMD, "a fill follows one or two lines"));
    }
    let mut lines = Vec::with_capacity(list.len());
    for line in list {
        let pts = line.as_array().ok_or_else(|| bad(CMD, format!("each line is [[x, y], …], not {line}")))?;
        if pts.len() > MAX_POINTS {
            return Err(bad(CMD, format!("a flow line has at most {MAX_POINTS} points")));
        }
        let mut out = Vec::with_capacity(pts.len());
        for q in pts {
            let c: Vec<f64> = q.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            let [x, y] = c.as_slice() else {
                return Err(bad(CMD, format!("each point is [x, y], not {q}")));
            };
            if !(x.is_finite() && y.is_finite()) {
                return Err(bad(CMD, "flow line points must be finite"));
            }
            out.push(sf_core::Point::new(*x, *y));
        }
        if out.len() < 2 {
            return Err(bad(CMD, "a flow line needs two points or more"));
        }
        lines.push(out);
    }
    s.edit("Set Flow Lines", |d, _| {
        let e = d.node_mut(id).ok_or(EngineError::NoNode(id))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(id))?;
        e.flow = lines;
        Ok(json!({ "lines": e.flow.len() }))
    })
}

fn fill_along_lines(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fillAlongLines";
    let keep = bool_or(p, "keep", false);
    let d = &s.doc()?.doc;
    let picked = d.paint_order(leaf_targets(s, &targets(s, p)?)?);
    let (fills, art): (Vec<NodeId>, Vec<NodeId>) = picked.iter().partition(|id| tatami(d, **id));
    if fills.is_empty() || art.is_empty() {
        return Err(bad(CMD, "select tatami fills and one or two lines for their rows to follow"));
    }
    // the first two lines of the art, in points
    let lines: Vec<Vec<sf_core::Point>> = art
        .iter()
        .filter_map(|id| d.node(*id))
        .flat_map(outlines_mm)
        .filter(|l| l.len() >= 2 && l.len() <= MAX_POINTS)
        .take(MAX_LINES)
        .map(|l| l.into_iter().map(|q| q * vectorcraft_embroidery::PT_PER_MM).collect())
        .collect();
    if lines.is_empty() {
        return Err(bad(CMD, "the lines to follow have no paths"));
    }
    let n = lines.len();
    s.edit("Fill Along Lines", |d, sel| {
        for id in &fills {
            let e = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut().ok_or(EngineError::NoNode(*id))?;
            e.flow = lines.clone();
        }
        if !keep {
            for id in &art {
                if d.node(*id).is_some() {
                    d.remove(*id)?;
                }
            }
            sel.objects.retain(|id| !art.contains(id));
        }
        Ok(())
    })?;
    Ok(json!({ "ids": fills.iter().map(|i| i.0).collect::<Vec<_>>(), "lines": n }))
}

fn clear_flow_lines(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> =
        ids.into_iter().filter(|id| d.node(*id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e: &Embroidery| !e.flow.is_empty())).collect();
    if ids.is_empty() {
        return Err(bad("embroidery.clearFlowLines", "the selection has no flow lines"));
    }
    s.edit("Clear Flow Lines", |d, _| {
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.flow.clear();
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}
