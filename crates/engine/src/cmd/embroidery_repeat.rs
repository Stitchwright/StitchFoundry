//! Repeat Design (HV-01): copies of the selected design round a circle, in a row, round the
//! inside of the hoop as a frame, or along a shape's outline, each turned to follow it, every
//! other one mirrored if asked. Behaviour from mySewnet's public help on Encore
//! (<https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Encore.htm>): repeats 2 to 360
//! round a circle or along a line, as many as fit a gap apart round a frame or a shape, copies in
//! sewing order clockwise (a line left to right). Code and wording are our own.

use std::sync::Arc;

use kurbo::Shape;
use serde_json::{Value, json};
use vectorcraft_doc::{Node, NodeKind};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::{Affine, Point, Vec2};

use super::embroidery_lettering::ranged;
use super::{CommandSpec, NodeId, Result, Session, bad, bool_or, cmd, has_selection, str_param, targets};

/// Copies made at most.
const MAX_REPEATS: usize = 360;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.repeat",
        "Repeat Design",
        ["Embroidery"],
        None,
        "{ids?, along?: circle|line|frame|shape (circle), repeats?: 2–360 (circle 8, line 5; a frame or shape takes as many as fit), position?: standard|mirrored (every other copy mirrored), radius?: mm (circle: as small as keeps the copies a gap apart), gap?: mm between copies (2; −50 % of the design's width to 100), margin?: mm from the hoop's edge (frame: 5), corners?: bool (frame: a copy in each corner, true)} the selected design repeated: round a circle about its middle, each copy turned to face out; in a row; round the inside of the hoop; or along the outline of a closed shape also selected (the larger), which stays. The copies replace the design, in a group in sewing order (clockwise; a row left to right), selected → {id, copies}",
        has_selection,
        repeat
    )]
}

/// Where a copy goes and how it is turned (radians, clockwise on the page).
type Place = (Point, f64);

/// `n` places round a circle of radius `r` about `c`, clockwise from the top, each turned so its
/// bottom faces the middle.
fn on_circle(c: Point, r: f64, n: usize) -> Vec<Place> {
    (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / n as f64;
            (c + Vec2::new(a.sin(), -a.cos()) * r, a)
        })
        .collect()
}

/// `n` places in a row about `c`, `step` apart, upright.
fn in_row(c: Point, step: f64, n: usize) -> Vec<Place> {
    let first = c.x - step * (n as f64 - 1.0) / 2.0;
    (0..n).map(|k| (Point::new(first + k as f64 * step, c.y), 0.0)).collect()
}

/// A closed outline as points, clockwise on the page (y down), flattened.
fn outline(bez: &kurbo::BezPath) -> Vec<Point> {
    let mut pts: Vec<Point> = Vec::new();
    kurbo::flatten(bez.iter(), 0.25, |el| match el {
        kurbo::PathEl::MoveTo(q) if pts.is_empty() => pts.push(q),
        kurbo::PathEl::LineTo(q) => pts.push(q),
        _ => {}
    });
    let area: f64 = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum();
    if area < 0.0 {
        pts.reverse();
    }
    pts
}

/// Places `step` apart (evened out so they close up) round a closed outline, each turned along
/// it; with `skip_corners`, none within half a step of `corners`.
fn along(pts: &[Point], step: f64, corners: &[Point], skip_corners: bool) -> Vec<Place> {
    let segs: Vec<(Point, Point)> = pts.iter().zip(pts.iter().cycle().skip(1)).map(|(a, b)| (*a, *b)).collect();
    let total: f64 = segs.iter().map(|(a, b)| a.distance(*b)).sum();
    if !(total.is_finite() && total > 0.0 && step > 0.0) {
        return vec![];
    }
    let n = ((total / step).floor() as usize).clamp(1, MAX_REPEATS);
    let even = total / n as f64;
    let mut out = Vec::with_capacity(n);
    let (mut seg, mut walked) = (0usize, 0.0);
    for k in 0..n {
        let want = k as f64 * even;
        while let Some((a, b)) = segs.get(seg) {
            let len = a.distance(*b);
            // a place on a vertex starts the next side
            if walked + len > want + 1e-9 || seg + 1 == segs.len() {
                let t = if len > 1e-9 { ((want - walked) / len).clamp(0.0, 1.0) } else { 0.0 };
                let q = a.lerp(*b, t);
                let at_corner = corners.iter().any(|c| c.distance(q) < even / 2.0);
                if at_corner && skip_corners {
                    break;
                }
                let heading = |(a, b): &(Point, Point)| {
                    let d = *b - *a;
                    d.y.atan2(d.x)
                };
                let mut angle = heading(&(*a, *b));
                // on a vertex: halfway between the side before and this one
                if t < 1e-6 {
                    let before = if seg == 0 { segs.last() } else { segs.get(seg - 1) };
                    if let Some(prev) = before.map(heading) {
                        let mut turn = angle - prev;
                        while turn > std::f64::consts::PI {
                            turn -= std::f64::consts::TAU;
                        }
                        while turn < -std::f64::consts::PI {
                            turn += std::f64::consts::TAU;
                        }
                        angle = prev + turn / 2.0;
                    }
                }
                out.push((q, angle));
                break;
            }
            walked += len;
            seg += 1;
        }
    }
    out
}

fn repeat(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.repeat";
    let kind = str_param(p, "along").unwrap_or("circle");
    if !["circle", "line", "frame", "shape"].contains(&kind) {
        return Err(bad(CMD, "`along` is circle, line, frame or shape"));
    }
    let mirrored = match str_param(p, "position").unwrap_or("standard") {
        "standard" => false,
        "mirrored" => true,
        _ => return Err(bad(CMD, "`position` is standard or mirrored")),
    };
    let repeats = match p.get("repeats") {
        None | Some(Value::Null) => {
            if kind == "line" {
                5
            } else {
                8
            }
        }
        Some(v) => {
            v.as_u64().filter(|n| (2..=MAX_REPEATS as u64).contains(n)).map(|n| n as usize).ok_or_else(|| bad(CMD, "`repeats` is 2 to 360"))?
        }
    };
    let margin = ranged(p, "margin", 0.0..=200.0, CMD, "`margin` is 0 to 200 mm")?.unwrap_or(5.0) * PT_PER_MM;
    let corners = bool_or(p, "corners", true);
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    // the design, and the shape to follow (along a shape: the larger of what is selected)
    let (design, dbox, path) = {
        let d = &s.doc()?.doc;
        let mut items: Vec<(NodeId, vectorcraft_geom::Rect)> =
            ids.iter().filter_map(|id| d.node(*id).and_then(|n| n.geometric_bounds()).map(|b| (*id, b))).collect();
        let mut path = None;
        if kind == "shape" {
            if items.len() < 2 {
                return Err(bad(CMD, "select the design and the shape to repeat it along"));
            }
            items.sort_by(|a, b| b.1.area().total_cmp(&a.1.area()));
            let Some(((big, _), rest)) = items.split_first() else { return Err(bad(CMD, "select the design and a shape")) };
            path = match d.node(*big).map(|n| &n.kind) {
                Some(NodeKind::Path { path, .. }) if path.subpaths.iter().any(|sp| sp.closed) => Some(path.to_bezpath()),
                _ => return Err(bad(CMD, "the shape to repeat along (the larger) is a closed path")),
            };
            items = rest.to_vec();
        }
        let dbox = items.iter().map(|r| r.1).reduce(|a, b| a.union(b)).ok_or_else(|| bad(CMD, "select a design to repeat"))?;
        (items.into_iter().map(|r| r.0).collect::<Vec<_>>(), dbox, path)
    };
    if !(dbox.width().is_finite() && dbox.height().is_finite()) || dbox.width() <= 0.0 {
        return Err(bad(CMD, "the design has no size"));
    }
    let w = dbox.width();
    let gap = ranged(p, "gap", -50.0..=100.0, CMD, "`gap` is −50 to 100 mm")?.unwrap_or(2.0) * PT_PER_MM;
    // copies overlap by half their width at most
    let gap = gap.max(-w / 2.0);
    let step = w + gap;
    let c = dbox.center();
    let places: Vec<Place> = match kind {
        "circle" => {
            let fit = step * repeats as f64 / std::f64::consts::TAU + dbox.height() / 2.0;
            let r = ranged(p, "radius", 1.0..=1000.0, CMD, "`radius` is 1 to 1000 mm")?.map_or(fit, |r| r * PT_PER_MM);
            // round the design's middle: the circle's top copy where the design was
            on_circle(c + Vec2::new(0.0, r), r, repeats)
        }
        "line" => in_row(c, step, repeats),
        "frame" => {
            let field = s.doc()?.doc.artboards.first().map(|a| a.rect).ok_or_else(|| bad(CMD, "the design has no hoop"))?;
            let inset = margin + dbox.height() / 2.0;
            let r = field.inset(-inset);
            if r.width() <= step || r.height() <= step {
                return Err(bad(CMD, "the design is too large to go round the hoop: make it smaller, or the margin narrower"));
            }
            let pts = [Point::new(r.x0, r.y0), Point::new(r.x1, r.y0), Point::new(r.x1, r.y1), Point::new(r.x0, r.y1)];
            along(&pts, step, &pts, !corners)
        }
        _ => {
            let Some(bez) = path else { return Err(bad(CMD, "select the shape to repeat along")) };
            let bb = bez.bounding_box();
            if !(bb.width().is_finite() && bb.height().is_finite()) || bb.width() * bb.height() > 1e12 {
                return Err(bad(CMD, "the shape is too large"));
            }
            along(&outline(&bez), step, &[], false)
        }
    };
    if places.is_empty() {
        return Err(bad(CMD, "no copies fit: make the design smaller, the gap narrower or the shape larger"));
    }
    let parent = s.doc()?.insertion_parent();
    let copies = places.len();
    let id = s.edit("Repeat Design", |d, sel| {
        let originals: Vec<Arc<Node>> = design.iter().filter_map(|id| d.node(*id).cloned()).map(Arc::new).collect();
        let mut groups = Vec::with_capacity(places.len());
        for (k, (q, a)) in places.iter().enumerate() {
            let flip = if mirrored && k % 2 == 1 { Affine::scale_non_uniform(-1.0, 1.0) } else { Affine::IDENTITY };
            let m = Affine::translate(q.to_vec2()) * Affine::rotate(*a) * flip * Affine::translate(-c.to_vec2());
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
        all.name = Some("Repeat".into());
        let id = all.id;
        d.remove_many(&design)?;
        d.insert(parent, usize::MAX, all)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0, "copies": copies }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_heart() -> (Session, u64) {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let r = s.execute("embroidery.addShape", &json!({"name": "Heart", "size": 20})).unwrap();
        (s, r["id"].as_u64().unwrap())
    }

    fn copies(s: &Session, id: u64) -> Vec<vectorcraft_geom::Rect> {
        let d = &s.doc().unwrap().doc;
        let NodeKind::Group { children, .. } = &d.node(NodeId(id)).unwrap().kind else { panic!("a group") };
        children.iter().map(|c| c.geometric_bounds().unwrap()).collect()
    }

    /// Round a circle: as many copies as asked, all the same distance from one middle, a gap
    /// apart; the design itself replaced; undo brings it back.
    #[test]
    fn copies_go_round_a_circle_a_gap_apart() {
        let (mut s, heart) = session_with_heart();
        let r = s.execute("embroidery.repeat", &json!({"ids": [heart], "repeats": 6})).unwrap();
        assert_eq!(r["copies"], 6);
        let id = r["id"].as_u64().unwrap();
        let b = copies(&s, id);
        let mid = b.iter().map(|r| r.center().to_vec2()).fold(Vec2::ZERO, |a, v| a + v) / 6.0;
        let dist: Vec<f64> = b.iter().map(|r| (r.center().to_vec2() - mid).hypot()).collect();
        // (a heart's box isn't round its turning point: near enough)
        assert!(dist.iter().all(|d| (d - dist[0]).abs() < dist[0] * 0.1), "{dist:?}");
        for (k, x) in b.iter().enumerate() {
            for y in b.iter().skip(k + 1) {
                assert!(x.center().distance(y.center()) > 15.0 * PT_PER_MM, "copies apart: {x:?} {y:?}");
            }
        }
        assert!(s.doc().unwrap().doc.node(NodeId(heart)).is_none(), "the copies replace the design");
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(heart)).is_some());
    }

    /// In a row, left to right, evenly; mirrored flips every other one.
    #[test]
    fn a_row_mirrored_and_a_frame_round_the_hoop() {
        let (mut s, heart) = session_with_heart();
        let r = s.execute("embroidery.repeat", &json!({"ids": [heart], "along": "line", "repeats": 4, "position": "mirrored", "gap": 3})).unwrap();
        let b = copies(&s, r["id"].as_u64().unwrap());
        assert_eq!(b.len(), 4);
        let xs: Vec<f64> = b.iter().map(|r| r.center().x).collect();
        assert!(xs.windows(2).all(|w| w[1] > w[0]), "left to right: {xs:?}");
        let steps: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(steps.iter().all(|d| (d - steps[0]).abs() < 0.5), "{steps:?}");
        assert!((steps[0] - (20.0 + 3.0) * PT_PER_MM).abs() < 2.0 * PT_PER_MM, "the width and the gap: {steps:?}");
        // the frame: copies round the inside of the hoop, none outside it
        let (mut s, heart) = session_with_heart();
        s.execute("object.scale", &json!({"ids": [heart], "sx": 50, "sy": 50})).unwrap();
        let r = s.execute("embroidery.repeat", &json!({"ids": [heart], "along": "frame", "margin": 3})).unwrap();
        let n = r["copies"].as_u64().unwrap();
        assert!(n > 12, "{r}");
        let field = s.doc().unwrap().doc.artboards[0].rect;
        let b = copies(&s, r["id"].as_u64().unwrap());
        assert!(b.iter().all(|x| field.inset(1.0).contains(x.center()) && x.x0 > field.x0 - 1.0 && x.y1 < field.y1 + 1.0), "inside the hoop");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        // no corners: fewer copies
        let (mut s, heart) = session_with_heart();
        s.execute("object.scale", &json!({"ids": [heart], "sx": 50, "sy": 50})).unwrap();
        let r2 = s.execute("embroidery.repeat", &json!({"ids": [heart], "along": "frame", "margin": 3, "corners": false})).unwrap();
        assert!(r2["copies"].as_u64().unwrap() < n, "{r2}");
    }

    /// Along a shape: copies round a circle drawn as the shape, which stays; bad params refused.
    #[test]
    fn along_a_shape_and_bad_params() {
        let (mut s, heart) = session_with_heart();
        s.execute("object.scale", &json!({"ids": [heart], "sx": 40, "sy": 40})).unwrap();
        let ring = s.execute("shape.ellipse", &json!({"x": 100, "y": 100, "width": 250, "height": 250})).unwrap()["id"].as_u64().unwrap();
        let r = s.execute("embroidery.repeat", &json!({"ids": [heart, ring], "along": "shape", "gap": 1})).unwrap();
        assert!(r["copies"].as_u64().unwrap() >= 10, "{r}");
        assert!(s.doc().unwrap().doc.node(NodeId(ring)).is_some(), "the shape stays");
        let (mut s, heart) = session_with_heart();
        for bad in [
            json!({"ids": [heart], "along": "spiral"}),
            json!({"ids": [heart], "repeats": 1}),
            json!({"ids": [heart], "repeats": 361}),
            json!({"ids": [heart], "position": "upside"}),
            json!({"ids": [heart], "along": "shape"}),
            json!({"ids": [heart], "gap": 500}),
        ] {
            assert!(s.execute("embroidery.repeat", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn places_round_a_square_turn_at_the_corners() {
        let sq = [Point::new(0.0, 0.0), Point::new(100.0, 0.0), Point::new(100.0, 100.0), Point::new(0.0, 100.0)];
        let all = along(&sq, 25.0, &sq, false);
        assert_eq!(all.len(), 16);
        // the top edge upright, the right edge a quarter turn, a corner halfway
        assert!(all[1].1.abs() < 1e-9 && (all[5].1 - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{all:?}");
        assert!((all[4].1 - std::f64::consts::FRAC_PI_4).abs() < 1e-9, "the corner: {:?}", all[4]);
        let no_corners = along(&sq, 25.0, &sq, true);
        assert_eq!(no_corners.len(), 12);
        assert!(along(&sq, 0.0, &sq, false).is_empty());
    }
}
