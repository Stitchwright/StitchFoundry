//! Digitizing input: columns clicked side by side (Column A/B).

use serde_json::{Value, json};
use sf_core::attr::Embroidery;
use sf_core::direction::Guide;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node, NodeKind};
use vectorcraft_geom::Point;
use vectorcraft_geom::path::{Anchor, PathData, SubPath};

use super::{CommandSpec, Result, Session, bad, cmd, has_doc, id_param};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.column",
        "Column A/B",
        [],
        None,
        "{points: [[x, y, curve?]…], id?} a satin column from points clicked on its two sides in turn (first side, second side, first…; `curve`: a smooth point), the stitches turning with each pair; with `id` the points replace that column's sides → {id}",
        has_doc,
        column
    )]
}

/// One side of a column through `pts`: smooth (Catmull-Rom) at curve points, else corners.
fn side(pts: &[(Point, bool)]) -> SubPath {
    let n = pts.len();
    let anchors = (0..n)
        .map(|i| {
            let (p, curve) = pts[i];
            if !curve || n < 2 {
                return Anchor::corner(p);
            }
            let prev = pts[i.saturating_sub(1)].0;
            let next = pts[(i + 1).min(n - 1)].0;
            let t = (next - prev) * (1.0 / 6.0);
            Anchor::with_handles(p, p - t, p + t)
        })
        .collect();
    SubPath::new(anchors, false)
}

fn column(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.column";
    let pts: Vec<(Point, bool)> = p
        .get("points")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter()
                .map(|v| {
                    let q = v.as_array()?;
                    let (x, y) = (q.first()?.as_f64()?, q.get(1)?.as_f64()?);
                    (x.is_finite() && y.is_finite()).then(|| (Point::new(x, y), q.get(2).and_then(Value::as_bool).unwrap_or(false)))
                })
                .collect::<Option<Vec<_>>>()
        })
        .filter(|v| !v.is_empty())
        .ok_or_else(|| bad(CMD, "`points` is a list of [x, y] or [x, y, curve]"))?;
    let a: Vec<(Point, bool)> = pts.iter().step_by(2).copied().collect();
    let b: Vec<(Point, bool)> = pts.iter().skip(1).step_by(2).copied().collect();
    let mut subs = vec![side(&a)];
    if !b.is_empty() {
        subs.push(side(&b));
    }
    let path = PathData::new(subs);
    // each pair of points sets the stitch angle there
    let guides: Vec<Guide> =
        a.iter().zip(&b).map(|(p, q)| Guide::new(sf_core::Point::new(p.0.x, p.0.y), sf_core::Point::new(q.0.x, q.0.y))).collect();
    if let Some(id) = id_param(p, "id") {
        if !s.doc()?.doc.node(id).is_some_and(|n| matches!(n.kind, NodeKind::Path { .. })) {
            return Err(bad(CMD, format!("{} is not a path", id.0)));
        }
        s.edit("Column A/B", |d, sel| {
            let n = d.node_mut(id).ok_or(EngineError::NoNode(id))?;
            if let NodeKind::Path { path: old, .. } = &mut n.kind {
                *old = path.clone();
            }
            n.embroidery.get_or_insert_with(|| Box::new(Embroidery::satin())).guides = guides.clone();
            sel.set([id]);
            Ok(())
        })?;
        return Ok(json!({ "id": id.0 }));
    }
    // new columns take the current satin properties and are drawn in the stroke colour
    let props = super::embroidery::current_props(s, "satin").unwrap_or_else(Embroidery::satin);
    let paint = [&s.paint.stroke, &s.paint.fill]
        .into_iter()
        .find(|p| matches!(p, Paint::Solid { .. }))
        .cloned()
        .unwrap_or_else(|| Paint::solid(Color::rgb8(0x1a, 0x3e, 0x8c)));
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Column A/B", |d, sel| {
        let id = d.alloc_id();
        let mut n = Node::path(id, path.clone(), Appearance::basic(Paint::None, paint.clone(), 0.75));
        let mut e = props.clone();
        e.guides = guides.clone();
        n.embroidery = Some(Box::new(e));
        d.insert(parent, usize::MAX, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}
