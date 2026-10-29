//! Artwork made elsewhere placed as embroidery (`embroidery.placeArt`): Design from Prompt's
//! drawings, or any flat-colour SVG an agent has. One undo step: the art is placed, what can't be
//! stitched is left out, and its shapes get stitch types sewn in drawing order, each one cutting
//! what it covers out of the fills sewn before it, in the design chart's threads.

use serde_json::{Value, json};
use sf_core::attr::StitchType;
use vectorcraft_color::Paint;
use vectorcraft_doc::{Node, NodeId, NodeKind};
use vectorcraft_embroidery::{PT_PER_MM, is_line_art, node_thread, regions_mm};
use vectorcraft_geom::Rect;

use super::{CommandSpec, Point, Result, Session, bad, cmd, finite_numbers, has_doc, leaf_targets, str_param};
use crate::EngineError;

const CMD: &str = "embroidery.placeArt";

/// How far (mm) the fills below run in under a shape drawn on top: little, so small details on
/// top (eyes, highlights) don't stack thread layers under their edges.
const OVERLAP: f64 = 0.4;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.placeArt",
        "Place Artwork as Embroidery",
        [],
        None,
        "{svg, size?: mm (longest side; default 90% of the hoop's shorter side, else 100), at?: [x, y] (default: the first artboard's centre), frame?: [x, y, width, height] (instead of size and at: the SVG's page fitted into this rectangle, aspect kept, so art redrawn from an image lands on it), name?, minArea?: mm² (1)} place flat-colour SVG art (Design from Prompt's) without reading files it links to; images, unpainted shapes and areas under minArea are left out, and the shapes get stitch types, each cutting what it covers out of the fills before it, in the design chart's threads, sewn in drawing order with same-coloured shapes brought together where the stacking allows → {id, objects, satin, fill, run, removed, colors, warnings}",
        has_doc,
        place_art
    )]
}

fn place_art(s: &mut Session, p: &Value) -> Result<Value> {
    // The art and its stitches are one undo step.
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_undo_group();
    }
    let res = art(s, p);
    if let Some(uid) = group {
        s.end_document_undo_group(uid, res.is_err());
    }
    res
}

/// Whether placed art keeps an object: images, hidden objects, clipping paths and shapes painted
/// with neither fill nor stroke can't be stitched.
fn keep(n: &Node) -> bool {
    let unpainted = || matches!(n.appearance.fill_paint(), Paint::None) && matches!(n.appearance.stroke_paint(), Paint::None);
    n.visible
        && match &n.kind {
            NodeKind::Image(_) | NodeKind::Path { clipping: true, .. } => false,
            NodeKind::Path { .. } | NodeKind::Compound { .. } => !unpainted(),
            _ => true,
        }
}

/// The areas of the placed art smaller than `min_area` (mm²): too small to sew. A compound path
/// counts as one shape.
fn specks(n: &Node, min_area: f64, out: &mut Vec<NodeId>) {
    match &n.kind {
        NodeKind::Path { .. } | NodeKind::Compound { .. } => {
            if !is_line_art(n) && regions_mm(n).iter().map(sf_core::geom::Polygon::area).sum::<f64>() < min_area {
                out.push(n.id);
            }
        }
        _ => {
            for c in n.children().into_iter().flatten() {
                specks(c, min_area, out);
            }
        }
    }
}

fn art(s: &mut Session, p: &Value) -> Result<Value> {
    let svg = str_param(p, "svg").ok_or_else(|| bad(CMD, "missing `svg`"))?;
    if svg.len() > sf_core::ai::MAX_SVG {
        return Err(bad(CMD, "the artwork is over 1 MB"));
    }
    let size = match p.get("size").filter(|v| !v.is_null()) {
        Some(v) => Some(v.as_f64().filter(|v| v.is_finite() && (5.0..=2000.0).contains(v)).ok_or_else(|| bad(CMD, "size must be 5–2000 mm"))?),
        None => None,
    };
    let at = match p.get("at").filter(|v| !v.is_null()) {
        Some(v) => Some(finite_numbers(v).map(|[x, y]| Point::new(x, y)).ok_or_else(|| bad(CMD, "at must be [x, y]"))?),
        None => None,
    };
    let frame = match p.get("frame").filter(|v| !v.is_null()) {
        Some(v) => Some(
            finite_numbers(v)
                .filter(|[_, _, w, h]| *w > 0.0 && *h > 0.0)
                .map(|[x, y, w, h]| Rect::new(x, y, x + w, y + h))
                .ok_or_else(|| bad(CMD, "frame must be [x, y, width, height] with a positive size"))?,
        ),
        None => None,
    };
    let min_area = p.get("minArea").and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(1.0).max(0.0);
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Artwork");
    // Its size: as asked, else to fit the hoop whatever its shape, else 100 mm.
    let hoop = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.hoop.clone());
    let mm = size.unwrap_or_else(|| hoop.map_or(100.0, |h| 0.9 * h.width.min(h.height)));
    // Its size is the size of what is stitched: what can't be is left out first (specks, after).
    let left_out = std::cell::Cell::new(0usize);
    let kept = |n: &Node| {
        let k = keep(n);
        if !k {
            left_out.set(left_out.get() + 1);
        }
        k
    };
    let placement = match frame {
        Some(r) => super::place::Fit::Frame(r),
        None => super::place::Fit::Longest(mm * PT_PER_MM, at),
    };
    let (gid, warnings) = super::place::place_svg(s, CMD, "Place Artwork", name, svg, placement, &kept)?;
    let drop = {
        let g = s.doc()?.doc.node(gid).ok_or(EngineError::NoNode(gid))?;
        let mut out = Vec::new();
        for c in g.children().into_iter().flatten() {
            specks(c, min_area, &mut out);
        }
        out
    };
    if !drop.is_empty() {
        s.edit("Place Artwork", |d, _| {
            for id in &drop {
                d.remove(*id)?;
            }
            Ok(())
        })?;
    }
    s.execute("embroidery.convert", &json!({ "ids": [gid.0] }))?;
    let ids: Vec<NodeId> = {
        let d = &s.doc()?.doc;
        leaf_targets(s, &[gid])?.into_iter().filter(|id| d.node(*id).is_some_and(|n| n.embroidery.is_some())).collect()
    };
    if ids.is_empty() {
        return Err(bad(CMD, "the artwork has no shapes to stitch"));
    }
    let list: Vec<u64> = ids.iter().map(|i| i.0).collect();
    // Shapes drawn on top cut what they cover out of the fills below (no stitches piled up).
    s.execute("embroidery.setParams", &json!({ "ids": list, "params": { "remove_underlying": true, "underlying_overlap": OVERLAP } }))?;
    s.execute("embroidery.matchThreads", &json!({ "ids": list }))?;
    // Fewer colour changes where the stacking allows (each one stops a single-needle machine).
    s.execute("embroidery.autoSequence", &json!({ "ids": [gid.0] }))?;
    let (mut satin, mut fill, mut run, mut colors) = (0usize, 0usize, 0usize, Vec::new());
    let d = &s.doc()?.doc;
    for n in ids.iter().filter_map(|id| d.node(*id)) {
        let Some(e) = n.embroidery.as_deref() else { continue };
        match e.stitch {
            StitchType::Satin(_) => satin += 1,
            StitchType::Fill(_) => fill += 1,
            _ => run += 1,
        }
        let c = node_thread(n, e).color.to_u32();
        if !colors.contains(&c) {
            colors.push(c);
        }
    }
    Ok(json!({
        "id": gid.0, "objects": ids.len(), "satin": satin, "fill": fill, "run": run, "removed": left_out.get() + drop.len(),
        "colors": colors.len(), "warnings": warnings
    }))
}
