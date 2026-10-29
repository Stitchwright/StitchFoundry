//! Scatter copies (NEW-24): copies of a motif strewn over a shape, at random or evenly, a
//! minimum gap apart and kept inside it.

use std::sync::Arc;

use kurbo::Shape;
use serde_json::{Value, json};
use vectorcraft_doc::{Node, NodeKind};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::{Affine, Point};

use super::embroidery_lettering::ranged;
use super::{CommandSpec, NodeId, Result, Session, bad, bool_or, cmd, has_selection, str_param, targets};

/// Copies made at most.
const MAX_COPIES: usize = 500;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.scatter",
        "Scatter Copies",
        ["Embroidery"],
        None,
        "{ids?, count?: copies (20; 1–500), gap?: mm between copies (2; 0–100), spread?: random|even (random), seed?: a number (1: the same scatter each time), turn?: bool (each copy turned at random)} select a motif and the shape to strew it over (the larger of what is selected): copies of the motif a gap apart, kept inside the shape, in a group above it, selected; the shape stays → {id, copies}",
        has_selection,
        scatter
    )]
}

/// A small, steady pseudo-random sequence (the same `seed` gives the same scatter).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The distance from `q` to the nearest of `edges`.
fn edge_distance(q: Point, edges: &[(Point, Point)]) -> f64 {
    edges
        .iter()
        .map(|(a, b)| {
            let (ab, aq) = (*b - *a, q - *a);
            let len = ab.hypot2();
            let t = if len > 1e-12 { (aq.dot(ab) / len).clamp(0.0, 1.0) } else { 0.0 };
            (*a + ab * t).distance(q)
        })
        .fold(f64::INFINITY, f64::min)
}

fn scatter(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.scatter";
    let count = match p.get("count") {
        None | Some(Value::Null) => 20,
        Some(v) => v.as_u64().filter(|c| (1..=MAX_COPIES as u64).contains(c)).map(|c| c as usize).ok_or_else(|| bad(CMD, "`count` is 1 to 500"))?,
    };
    let gap = ranged(p, "gap", 0.0..=100.0, CMD, "`gap` is 0 to 100 mm")?.unwrap_or(2.0) * PT_PER_MM;
    let even = match str_param(p, "spread").unwrap_or("random") {
        "random" => false,
        "even" => true,
        _ => return Err(bad(CMD, "`spread` is random or even")),
    };
    let seed = p.get("seed").and_then(Value::as_u64).unwrap_or(1);
    let turn = bool_or(p, "turn", false);
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    // the shape: the largest of the selection; the motif: the rest
    let (motif, bez, mbox) = {
        let d = &s.doc()?.doc;
        let mut items: Vec<(NodeId, vectorcraft_geom::Rect)> =
            ids.iter().filter_map(|id| d.node(*id).and_then(|n| n.geometric_bounds()).map(|b| (*id, b))).collect();
        if items.len() < 2 {
            return Err(bad(CMD, "select a motif and the shape to scatter it over"));
        }
        items.sort_by(|a, b| (b.1.area()).total_cmp(&a.1.area()));
        let Some(((area, _), rest)) = items.split_first() else { return Err(bad(CMD, "select a motif and the shape to scatter it over")) };
        let bez = match d.node(*area).map(|n| &n.kind) {
            Some(NodeKind::Path { path, .. }) if path.subpaths.iter().any(|sp| sp.closed) => path.to_bezpath(),
            _ => return Err(bad(CMD, "the shape to scatter over (the larger) is a closed path")),
        };
        let mbox = rest.iter().map(|r| r.1).reduce(|a, b| a.union(b)).ok_or_else(|| bad(CMD, "select a motif"))?;
        (rest.iter().map(|r| r.0).collect::<Vec<_>>(), bez, mbox)
    };
    // the motif round its middle, `r` across at most; copies `spacing` apart, `r` inside the edge
    let c = mbox.center();
    let r = (mbox.width().hypot(mbox.height()) / 2.0).max(0.5);
    let spacing = 2.0 * r + gap;
    let mut edges = Vec::new();
    let (mut start, mut last) = (None, None);
    kurbo::flatten(bez.iter(), 0.25, |el| match el {
        kurbo::PathEl::MoveTo(q) => {
            if let (Some(a), Some(b)) = (last, start) {
                edges.push((a, b));
            }
            (start, last) = (Some(q), Some(q));
        }
        kurbo::PathEl::LineTo(q) => {
            if let Some(a) = last {
                edges.push((a, q));
            }
            last = Some(q);
        }
        kurbo::PathEl::ClosePath => {
            if let (Some(a), Some(b)) = (last, start) {
                edges.push((a, b));
            }
            last = start;
        }
        _ => {}
    });
    if let (Some(a), Some(b)) = (last, start) {
        edges.push((a, b));
    }
    let fits = |q: Point| bez.contains(q) && edge_distance(q, &edges) >= r;
    let b = bez.bounding_box();
    if !(b.width().is_finite() && b.height().is_finite()) || b.width() * b.height() > 1e12 {
        return Err(bad(CMD, "the shape is too large"));
    }
    let mut points: Vec<Point> = Vec::new();
    if even {
        // a triangular grid: the closest packing a gap apart
        let dy = spacing * 3f64.sqrt() / 2.0;
        let rows = ((b.height() / dy).ceil() as usize + 1).min(2000);
        let cols = ((b.width() / spacing).ceil() as usize + 1).min(2000);
        'grid: for row in 0..rows {
            let shift = if row % 2 == 1 { spacing / 2.0 } else { 0.0 };
            for col in 0..cols {
                let q = Point::new(b.x0 + shift + col as f64 * spacing, b.y0 + row as f64 * dy);
                if fits(q) {
                    points.push(q);
                    if points.len() >= count {
                        break 'grid;
                    }
                }
            }
        }
    } else {
        let mut rng = Lcg(seed ^ 0x9e37_79b9_7f4a_7c15);
        for _ in 0..(count * 200).min(100_000) {
            let q = Point::new(b.x0 + rng.next() * b.width(), b.y0 + rng.next() * b.height());
            if fits(q) && points.iter().all(|o| o.distance(q) >= spacing) {
                points.push(q);
                if points.len() >= count {
                    break;
                }
            }
        }
    }
    if points.is_empty() {
        return Err(bad(CMD, "the motif doesn't fit inside the shape: make it smaller or the gap narrower"));
    }
    let mut rng = Lcg(seed.wrapping_add(7));
    let angles: Vec<f64> = points.iter().map(|_| if turn { rng.next() * std::f64::consts::TAU } else { 0.0 }).collect();
    let parent = s.doc()?.insertion_parent();
    let copies = points.len();
    let id = s.edit("Scatter Copies", |d, sel| {
        let originals: Vec<Arc<Node>> = motif.iter().filter_map(|id| d.node(*id).cloned()).map(Arc::new).collect();
        let mut groups = Vec::with_capacity(points.len());
        for (k, (q, a)) in points.iter().zip(&angles).enumerate() {
            let m = Affine::translate(q.to_vec2()) * Affine::rotate(*a) * Affine::translate(-c.to_vec2());
            let children: Vec<Arc<Node>> = originals
                .iter()
                .map(|n| {
                    let mut n = d.reid(n);
                    n.transform(m, true);
                    Arc::new(n)
                })
                .collect();
            let mut g = Node::group(d.alloc_id(), children);
            g.name = Some(format!("Copy {}", k + 1));
            groups.push(Arc::new(g));
        }
        let mut all = Node::group(d.alloc_id(), groups);
        all.name = Some("Scatter".into());
        let id = all.id;
        d.insert(parent, usize::MAX, all)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0, "copies": copies }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_measure_the_nearest_side() {
        let sq = [
            (Point::new(0.0, 0.0), Point::new(10.0, 0.0)),
            (Point::new(10.0, 0.0), Point::new(10.0, 10.0)),
            (Point::new(10.0, 10.0), Point::new(0.0, 10.0)),
            (Point::new(0.0, 10.0), Point::new(0.0, 0.0)),
        ];
        assert!((edge_distance(Point::new(5.0, 5.0), &sq) - 5.0).abs() < 1e-12);
        assert!((edge_distance(Point::new(2.0, 7.0), &sq) - 2.0).abs() < 1e-12);
        let mut a = Lcg(3);
        let mut b = Lcg(3);
        assert!((0..50).all(|_| {
            let v = a.next();
            (0.0..1.0).contains(&v) && v == b.next()
        }));
    }
}
