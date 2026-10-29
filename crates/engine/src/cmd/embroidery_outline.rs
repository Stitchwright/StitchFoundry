//! Outlines and borders (`embroidery.addOutline`): a stitched outline round the selected shapes
//! and type, following them at a distance or framing them in a rectangle, an oval or a rounded
//! rectangle, sewn in a run, a bean run, satin or a motif after them.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::geom::{BBox, Polygon, poly};
use sf_core::model::{Motif, RunKind, RunParams, SatinParams};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node, NodeId, NodeKind};
use vectorcraft_embroidery::{MM_PER_PT, PT_PER_MM, node_thread, regions_mm};
use vectorcraft_geom::Rect;
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery::stitched_targets;
use super::{CommandSpec, Point, Result, Session, bad, cmd, has_selection, str_param};
use crate::EngineError;

const CMD: &str = "embroidery.addOutline";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.addOutline",
        "Add Outline",
        ["Embroidery"],
        None,
        "{ids?, shape?: offset|rectangle|oval|rounded (offset: following the shapes), offset?: mm out from them (1; may be negative for offset), stitch?: run|bean|satin|motif (satin), width?: mm (satin or motif, 2.5), motif?: wave|scallop|… or one of the design's motifs (wave)} a stitched outline or border round the selected stitched shapes, lines and type (a line: round the band its stitches cover), sewn after them in the top one's thread → {id}",
        has_selection,
        add_outline
    )]
}

/// What a stitched object covers (mm): a shape its area; a line (or a border) the band its
/// stitches cover, satin as wide as it is sewn and runs 1 mm, without the space a closed line
/// goes round.
pub(super) fn covered_mm(n: &Node) -> Vec<Polygon> {
    if !vectorcraft_embroidery::is_line_art(n) {
        return regions_mm(n);
    }
    let width = match n.embroidery.as_deref() {
        Some(e) if matches!(e.stitch, StitchType::Satin(_)) => e.width.unwrap_or(n.appearance.stroke_width() * MM_PER_PT).max(0.5),
        _ => 1.0,
    };
    let lines: Vec<(Vec<sf_core::Point>, bool)> = vectorcraft_embroidery::outlines_mm(n)
        .into_iter()
        .map(|l| {
            let closed = l.len() > 2 && l.first().zip(l.last()).is_some_and(|(a, b)| a.distance(*b) < 1e-6);
            (l, closed)
        })
        .collect();
    poly::stroke(&lines, width).into_iter().map(|p| Polygon::new(p.outer, Vec::new())).collect()
}

fn add_outline(s: &mut Session, p: &Value) -> Result<Value> {
    let shape = str_param(p, "shape").unwrap_or("offset");
    if !matches!(shape, "offset" | "rectangle" | "oval" | "rounded") {
        return Err(bad(CMD, format!("`shape` is offset, rectangle, oval or rounded, not `{shape}`")));
    }
    let offset = match p.get("offset") {
        None => 1.0,
        Some(v) => v.as_f64().filter(|v| (-20.0..=100.0).contains(v)).ok_or_else(|| bad(CMD, "`offset` is -20 to 100 mm"))?,
    };
    let width = match p.get("width") {
        None => 2.5,
        Some(v) => v.as_f64().filter(|v| (0.5..=12.0).contains(v)).ok_or_else(|| bad(CMD, "`width` is 0.5 to 12 mm"))?,
    };
    let (motif, custom_motif) = match str_param(p, "motif") {
        None => (Motif::Wave, None),
        Some(name) => super::embroidery_motifs::motif_by_name(s, name, CMD)?,
    };
    // A design's motif keeps its drawn proportions.
    let spacing = custom_motif.as_ref().map_or(width * 1.5, |m| (width / m.aspect()).clamp(0.5, 100.0));
    let stitch = match str_param(p, "stitch").unwrap_or("satin") {
        "run" => StitchType::Run(RunParams::default()),
        "bean" => StitchType::Run(RunParams { kind: RunKind::Triple, ..RunParams::default() }),
        "motif" => {
            StitchType::Run(RunParams { kind: RunKind::Motif, motif, width, spacing, stitch_length: 1.5, custom_motif, ..RunParams::default() })
        }
        "satin" => StitchType::Satin(SatinParams::default()),
        other => return Err(bad(CMD, format!("`stitch` is run, bean, satin or motif, not `{other}`"))),
    };
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = d.paint_order(ids.into_iter().filter(|id| d.node(*id).is_some_and(|n| n.embroidery.is_some())));
    let areas: Vec<Polygon> = ids.iter().filter_map(|id| d.node(*id)).flat_map(covered_mm).collect();
    let Some(&top) = ids.last() else {
        return Err(bad(CMD, "select stitched shapes or type"));
    };
    let union = poly::union(&areas);
    let pts = |r: &[sf_core::Point]| r.iter().map(|q| Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM)).collect::<Vec<_>>();
    let path = if shape == "offset" {
        let grown = poly::offset(&union, offset);
        let subs: Vec<SubPath> = grown.iter().flat_map(|g| g.rings()).filter(|r| r.len() >= 3).map(|r| SubPath::polyline(&pts(r), true)).collect();
        if subs.is_empty() {
            return Err(bad(CMD, "the selection has nothing to outline"));
        }
        PathData::new(subs)
    } else {
        let bb = BBox::from_points(union.iter().flat_map(|g| g.outer.iter().copied()));
        if bb.is_empty() {
            return Err(bad(CMD, "the selection has nothing to outline"));
        }
        let m = offset.max(0.0);
        let r = Rect::new((bb.min.x - m) * PT_PER_MM, (bb.min.y - m) * PT_PER_MM, (bb.max.x + m) * PT_PER_MM, (bb.max.y + m) * PT_PER_MM);
        match shape {
            "oval" => {
                // An oval round the box, not inscribed in it: √2 larger.
                let (c, k) = (r.center(), std::f64::consts::SQRT_2 * 0.5);
                vectorcraft_geom::shapes::ellipse(Rect::new(c.x - r.width() * k, c.y - r.height() * k, c.x + r.width() * k, c.y + r.height() * k))
            }
            "rounded" => vectorcraft_geom::shapes::rounded_rectangle(r, r.width().min(r.height()) * 0.15),
            _ => vectorcraft_geom::shapes::rectangle(r),
        }
    };
    let node = d.node(top).ok_or(EngineError::NoNode(top))?;
    let thread = node.embroidery.as_deref().map(|e| node_thread(node, e));
    let color = thread.as_ref().map_or_else(|| Color::rgb8(0x1a, 0x3e, 0x8c), |t| Color::rgb8(t.color.r, t.color.g, t.color.b));
    let mut e = Embroidery::new(stitch);
    e.width = Some(width);
    e.thread = thread;
    let id = s.edit("Add Outline", |d, sel| {
        let (parent, index, _) = d.position(top).ok_or(EngineError::NoNode(top))?;
        let id = d.alloc_id();
        let kind = NodeKind::Path { path: path.clone(), rule: vectorcraft_geom::FillRule::NonZero, live: None, clipping: false, guide: false };
        let mut n = Node::new(id, kind);
        n.appearance = Appearance::basic(Paint::None, Paint::solid(color), (width * PT_PER_MM).min(4.0)).into();
        n.name = Some("Outline".into());
        n.embroidery = Some(Box::new(e));
        d.insert(parent, index + 1, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}
