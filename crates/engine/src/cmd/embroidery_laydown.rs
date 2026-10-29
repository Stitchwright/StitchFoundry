//! Laydown (`embroidery.addLaydown`): an open fill sewn first under the selected shapes and type,
//! a little beyond them, so the pile of towels, fleece and knits lies flat under the design.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::geom::{Polygon, poly};
use sf_core::model::{FillKind, FillParams, FillUnderlay};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node, NodeId, NodeKind};
use vectorcraft_embroidery::{PT_PER_MM, node_thread, regions_mm};
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery::stitched_targets;
use super::{CommandSpec, Point, Result, Session, bad, cmd, has_selection};
use crate::EngineError;

const CMD: &str = "embroidery.addLaydown";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.addLaydown",
        "Add Laydown",
        ["Embroidery"],
        None,
        "{ids?, margin?: mm beyond the shapes (1.5), spacing?: mm between its rows (2.5), angle?: degrees (45)} an open tatami sewn first under the selected stitched shapes and type, a little beyond them, in the bottom one's thread: it flattens the pile of towels, fleece and knits → {id}",
        has_selection,
        add_laydown
    )]
}

/// A number param within `range`, `default` when missing.
fn ranged(p: &Value, key: &str, default: f64, range: std::ops::RangeInclusive<f64>) -> Result<f64> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v.as_f64().filter(|v| range.contains(v)).ok_or_else(|| bad(CMD, format!("`{key}` is {} to {}", range.start(), range.end()))),
    }
}

fn add_laydown(s: &mut Session, p: &Value) -> Result<Value> {
    let margin = ranged(p, "margin", 1.5, 0.0..=20.0)?;
    let spacing = ranged(p, "spacing", 2.5, 1.0..=10.0)?;
    let angle = ranged(p, "angle", 45.0, -360.0..=360.0)?;
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    // The areas the selected stitched objects cover (lines have none), bottom first.
    let ids: Vec<NodeId> = d.paint_order(ids.into_iter().filter(|id| d.node(*id).is_some_and(|n| n.embroidery.is_some())));
    let areas: Vec<Polygon> = ids.iter().filter_map(|id| d.node(*id)).flat_map(regions_mm).collect();
    let Some(&bottom) = ids.first() else {
        return Err(bad(CMD, "select stitched shapes or type"));
    };
    let grown = poly::offset(&poly::union(&areas), margin);
    if grown.is_empty() {
        return Err(bad(CMD, "the selection covers no area (lines get no laydown)"));
    }
    let subpaths: Vec<SubPath> = grown
        .iter()
        .flat_map(|g| g.rings())
        .filter(|r| r.len() >= 3)
        .map(|r| SubPath::polyline(&r.iter().map(|q| Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM)).collect::<Vec<_>>(), true))
        .collect();
    let node = d.node(bottom).ok_or(EngineError::NoNode(bottom))?;
    let thread = node.embroidery.as_deref().map(|e| node_thread(node, e));
    let color = thread.as_ref().map_or_else(|| Color::rgb8(0xf2, 0xf2, 0xf2), |t| Color::rgb8(t.color.r, t.color.g, t.color.b));
    let params = FillParams {
        kind: FillKind::Tatami,
        angle,
        spacing,
        stitch_length: 4.0,
        trapunto: true,
        underlay: FillUnderlay { edge_run: false, tatami: false, ..FillUnderlay::default() },
        ..FillParams::default()
    };
    let mut e = Embroidery::new(StitchType::Fill(params));
    e.auto_direction = false;
    e.thread = thread;
    let id = s.edit("Add Laydown", |d, sel| {
        let (parent, index, _) = d.position(bottom).ok_or(EngineError::NoNode(bottom))?;
        let id = d.alloc_id();
        let kind =
            NodeKind::Path { path: PathData::new(subpaths), rule: vectorcraft_geom::FillRule::EvenOdd, live: None, clipping: false, guide: false };
        let mut n = Node::new(id, kind);
        n.appearance = Appearance::basic(Paint::solid(color), Paint::None, 0.0).into();
        n.name = Some("Laydown".into());
        n.embroidery = Some(Box::new(e));
        d.insert(parent, index, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}
