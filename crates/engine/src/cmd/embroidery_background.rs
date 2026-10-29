//! Quilt Background (HV-01): quilting round a design. The selected design keeps an unstitched
//! margin round it, and the rest of the hoop (or a block of the size given, round the design) is
//! quilted in one running line: a stipple meander, or echo lines that follow the design's outline
//! outward and the block's edge inward. Behaviour from mySewnet's public help on its Quilt Block
//! wizard (<https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Quilt_Block_Wizard_Examples.htm>:
//! an unstitched margin round the embroidery, stipple by default, echo lines following the
//! block's outline outside and the embroidery's inside, a 4 mm gap and 2 mm stitches). Code and
//! wording are our own.

use std::sync::Arc;

use serde_json::{Value, json};
use sf_core::geom::{Point as MmPoint, Polygon, poly, polyline};
use vectorcraft_embroidery::{MM_PER_PT, PT_PER_MM};

use super::embroidery_lettering::ranged;
use super::{CommandSpec, NodeId, Result, Session, bad, cmd, has_selection, str_param, targets};

/// Echo rings made at most.
const MAX_RINGS: usize = 400;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.quiltBackground",
        "Quilt Background",
        ["Embroidery"],
        None,
        "{ids?, fill?: stipple|echo (stipple), margin?: mm left unstitched round the design (3; 0–50), spacing?: mm between the quilting's lines (4; 1.5–30), stitch?: mm (2; 1–6), width?: mm, height?: mm (the block round the design; default: the hoop, 3 mm in from its edge), color?: \"#rrggbb\" (the design's first thread)} quilting round the selected design: a margin left clear round it and the rest of the block quilted in one running line, a stipple meander or echo lines following the design outward and the block's edge inward, sewn first (under the design), in its own thread, selected → {id, fill, lengthMm}",
        has_selection,
        quilt_background
    )]
}

/// A rectangle as a polygon (mm).
fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon {
    Polygon::new(vec![MmPoint::new(x0, y0), MmPoint::new(x1, y0), MmPoint::new(x1, y1), MmPoint::new(x0, y1)], vec![])
}

/// Echo quilting in `block` round `keep_out`: rings `spacing` apart following the design outward
/// while they stay inside the block, then rings following the block's edge inward while they still
/// hold the design's last ring half a spacing clear. Each family is one line (its rings joined
/// where they are closest, a short way); the two are sewn apart, the machine jumping between
/// them, so no stitch runs across the background from one to the other.
fn echo_round(block: &Polygon, keep_out: &[Polygon], spacing: f64) -> Vec<Vec<MmPoint>> {
    let tiny = |ps: &[Polygon]| ps.iter().all(|p| p.area().abs() < 0.01);
    let mut outward: Vec<Vec<MmPoint>> = Vec::new();
    let mut outermost = keep_out.to_vec();
    for k in 1..MAX_RINGS {
        let ring = poly::offset_round(keep_out, spacing * k as f64);
        if ring.is_empty() || !tiny(&poly::difference(&ring, std::slice::from_ref(block))) {
            break;
        }
        outward.extend(ring.iter().map(|p| polyline::closed(&p.outer)));
        outermost = ring;
    }
    // the block's edge inward: each ring's parts clear of the design's last ring, so the corners
    // and the bands beyond the design's rings fill too
    let clear = poly::offset_round(&outermost, spacing * 0.5);
    let mut inward: Vec<Vec<MmPoint>> = Vec::new();
    for k in 0..MAX_RINGS {
        let inset = poly::offset(std::slice::from_ref(block), -(spacing * k as f64));
        let Some(r) = inset.into_iter().max_by(|a, b| a.area().abs().total_cmp(&b.area().abs())) else { break };
        if r.area().abs() < 1.0 {
            break;
        }
        let ring = densify(&polyline::closed(&r.outer), 1.0);
        let outside = |q: &MmPoint| !clear.iter().any(|c| c.contains(*q));
        // the ring cut where it enters the clear zone; a ring wholly clear stays whole
        let mut run: Vec<MmPoint> = Vec::new();
        let mut pieces: Vec<Vec<MmPoint>> = Vec::new();
        for q in &ring {
            if outside(q) {
                run.push(*q);
            } else if run.len() >= 2 {
                pieces.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
        if run.len() >= 2 {
            pieces.push(run);
        }
        if pieces.is_empty() {
            break;
        }
        inward.extend(pieces);
    }
    // each family joined where its pieces come close; a join longer than a spacing and a half
    // would cross open background, so the line is cut there and the machine jumps
    [outward, inward]
        .iter()
        .filter_map(|rings| sf_core::quilt::join(rings, 1e-6).map(|j| j.line))
        .flat_map(|l| split_long(&l, spacing * 1.5))
        // and no specks: a piece shorter than a spacing isn't worth its jump
        .filter(|l| l.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>() >= spacing)
        .collect()
}

/// `line` cut wherever a step is longer than `max` mm.
fn split_long(line: &[MmPoint], max: f64) -> Vec<Vec<MmPoint>> {
    let mut out: Vec<Vec<MmPoint>> = vec![Vec::new()];
    for (i, q) in line.iter().enumerate() {
        if i > 0 && line.get(i - 1).is_some_and(|prev| prev.distance(*q) > max) {
            out.push(Vec::new());
        }
        if let Some(last) = out.last_mut() {
            last.push(*q);
        }
    }
    out
}

/// `line` with points at most `step` mm apart.
fn densify(line: &[MmPoint], step: f64) -> Vec<MmPoint> {
    let mut out = Vec::with_capacity(line.len() * 2);
    for w in line.windows(2) {
        let n = ((w[0].distance(w[1]) / step).ceil() as usize).clamp(1, 10_000);
        for i in 0..n {
            out.push(w[0].lerp(w[1], i as f64 / n as f64));
        }
    }
    out.extend(line.last());
    out
}

fn quilt_background(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.quiltBackground";
    let echo = match str_param(p, "fill").unwrap_or("stipple") {
        "stipple" => false,
        "echo" => true,
        _ => return Err(bad(CMD, "`fill` is stipple or echo")),
    };
    let margin = ranged(p, "margin", 0.0..=50.0, CMD, "`margin` is 0 to 50 mm")?.unwrap_or(3.0);
    let spacing = ranged(p, "spacing", 1.5..=30.0, CMD, "`spacing` is 1.5 to 30 mm")?.unwrap_or(4.0);
    let stitch = ranged(p, "stitch", 1.0..=6.0, CMD, "`stitch` is 1 to 6 mm")?.unwrap_or(2.0);
    let size =
        (ranged(p, "width", 10.0..=2000.0, CMD, "`width` is 10 to 2000 mm")?, ranged(p, "height", 10.0..=2000.0, CMD, "`height` is 10 to 2000 mm")?);
    let color = match str_param(p, "color") {
        Some(c) => {
            sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?;
            Some(c.to_string())
        }
        None => None,
    };
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    // the design's shapes (mm), and its box
    let (shapes, dbox, field) = {
        let d = &s.doc()?.doc;
        let field = d.artboards.first().map(|a| a.rect).ok_or_else(|| bad(CMD, "the design has no hoop"))?;
        let mut shapes: Vec<Polygon> = Vec::new();
        let mut dbox: Option<vectorcraft_geom::Rect> = None;
        for id in &ids {
            let Some(n) = d.node(*id) else { continue };
            if let Some(b) = n.geometric_bounds() {
                dbox = Some(dbox.map_or(b, |a| a.union(b)));
            }
            n.walk(&mut |c| shapes.extend(vectorcraft_embroidery::regions_mm(c)));
        }
        (shapes, dbox.ok_or_else(|| bad(CMD, "select the design to quilt round"))?, field)
    };
    // lines with no area (runs) keep out as their box
    let mm = |v: f64| v * MM_PER_PT;
    let mut keep = poly::union(&shapes);
    if keep.iter().all(|q| q.area().abs() < 0.01) {
        keep = vec![rect(mm(dbox.x0), mm(dbox.y0), mm(dbox.x1), mm(dbox.y1))];
    }
    let keep_out = poly::offset_round(&keep, margin);
    // the block: the size given round the design, else the hoop a little in from its edge
    let block = match size {
        (Some(w), Some(h)) => {
            let c = dbox.center();
            rect(mm(c.x) - w / 2.0, mm(c.y) - h / 2.0, mm(c.x) + w / 2.0, mm(c.y) + h / 2.0)
        }
        (None, None) => rect(mm(field.x0) + 3.0, mm(field.y0) + 3.0, mm(field.x1) - 3.0, mm(field.y1) - 3.0),
        _ => return Err(bad(CMD, "give both `width` and `height`, or neither")),
    };
    let lines: Vec<Vec<MmPoint>> = if echo {
        echo_round(&block, &keep_out, spacing)
    } else {
        // the block with the design (and its margin) cut out, stippled piece by piece
        let area = poly::difference(std::slice::from_ref(&block), &keep_out);
        let pieces: Vec<Vec<MmPoint>> =
            area.iter().filter(|a| a.area().abs() > spacing * spacing).map(|a| sf_core::quilt::meander(a, spacing, 1.0)).collect();
        sf_core::quilt::join(&pieces, 1e-6).map(|j| j.line).into_iter().filter(|l| l.len() >= 2).collect()
    };
    if lines.is_empty() {
        return Err(bad(CMD, "there's no room round the design to quilt: a larger block, or a narrower margin"));
    }
    if lines.iter().map(Vec::len).sum::<usize>() > sf_core::quilt::MAX_POINTS {
        return Err(bad(CMD, "too much quilting: a wider spacing, or a smaller block"));
    }
    let length: f64 = lines.iter().flat_map(|l| l.windows(2)).map(|w| w[0].distance(w[1])).sum();
    let thread = match color {
        Some(c) => c,
        None => s.execute("embroidery.info", &json!({}))?["colors"].get(0).and_then(|c| c["color"].as_str()).unwrap_or("#1f2933").to_string(),
    };
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Quilt Background".into()));
    }
    let done = (|| -> Result<u64> {
        let mut d = String::new();
        for line in &lines {
            for (i, q) in line.iter().enumerate() {
                d.push_str(&format!("{}{:.2} {:.2} ", if i == 0 { "M" } else { "L" }, q.x * PT_PER_MM, q.y * PT_PER_MM));
            }
        }
        let id = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(CMD, "the quilting wasn't made"))?;
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("embroidery.set", &json!({ "type": "run" }))?;
        s.execute("embroidery.setParams", &json!({ "params": { "stitch_length": stitch, "own_block": true } }))?;
        s.execute("embroidery.setThread", &json!({ "color": thread, "name": "Quilting", "ids": [id], "paint": true }))?;
        // sewn first: behind everything in its layer
        s.edit("Quilt Background", |d, sel| {
            let parent = d.parent_of(NodeId(id));
            let node = d.remove(NodeId(id))?;
            d.insert(parent, 0, Arc::unwrap_or_clone(node))?;
            sel.set([NodeId(id)]);
            Ok(())
        })?;
        Ok(id)
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let id = done?;
    Ok(json!({ "id": id, "fill": if echo { "echo" } else { "stipple" }, "lengthMm": length.round() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_doc::NodeKind;

    fn with_heart() -> (Session, u64) {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let id = s.execute("embroidery.addShape", &json!({"name": "Heart", "size": 50})).unwrap()["id"].as_u64().unwrap();
        (s, id)
    }

    /// Stipple round a heart: one quilting line in its own thread, sewn first, none of it within
    /// the margin of the heart, all of it in the hoop; one undo takes it back.
    #[test]
    fn stipple_keeps_its_margin_round_the_design_and_sews_first() {
        let (mut s, heart) = with_heart();
        let r = s.execute("embroidery.quiltBackground", &json!({"ids": [heart], "margin": 4})).unwrap();
        assert!(r["lengthMm"].as_f64().unwrap() > 500.0, "{r}");
        let id = r["id"].as_u64().unwrap();
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        assert_eq!(info["colors"][0]["name"], "Quilting", "sewn first: {info}");
        // no quilting point within 3 mm of the heart (4 mm margin, the line's own wobble aside)
        let d = &s.doc().unwrap().doc;
        let heart_regions = vectorcraft_embroidery::regions_mm(d.node(NodeId(heart)).unwrap());
        let near = poly::offset_round(&heart_regions, 3.0);
        let NodeKind::Path { path, .. } = &d.node(NodeId(id)).unwrap().kind else { panic!("a path") };
        let pts: Vec<MmPoint> =
            path.subpaths.iter().flat_map(|sp| sp.anchors.iter().map(|a| MmPoint::new(a.p.x * MM_PER_PT, a.p.y * MM_PER_PT))).collect();
        assert!(pts.len() > 50);
        assert!(pts.iter().all(|q| !near.iter().any(|n| n.contains(*q))), "the margin is kept clear");
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(id)).is_none());
        assert!(s.doc().unwrap().doc.node(NodeId(heart)).is_some());
    }

    /// Echo: rings round the heart and in from the block's edge; a block of the size asked; bad
    /// params refused.
    #[test]
    fn echo_rings_and_a_block_of_its_own_size() {
        let (mut s, heart) = with_heart();
        let r = s.execute("embroidery.quiltBackground", &json!({"ids": [heart], "fill": "echo", "width": 100, "height": 100, "spacing": 5})).unwrap();
        assert_eq!(r["fill"], "echo");
        // both families of rings: round the heart, and in from the block's edge, sewn apart
        let d = &s.doc().unwrap().doc;
        let NodeKind::Path { path, .. } = &d.node(NodeId(r["id"].as_u64().unwrap())).unwrap().kind else { panic!("a path") };
        assert!(path.subpaths.len() >= 2, "the two families sewn apart");
        // no stitch crosses open background: every step within a line is short
        for sp in &path.subpaths {
            for w in sp.anchors.windows(2) {
                assert!(w[0].p.distance(w[1].p) * MM_PER_PT <= 5.0 * 1.5 + 0.01, "a long step: {:?}", (w[0].p, w[1].p));
            }
        }
        assert!(path.subpaths.iter().all(|sp| sp.anchors.len() >= 4) && path.subpaths.iter().any(|sp| sp.anchors.len() > 20));
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert!(info["widthMm"].as_f64().unwrap() <= 101.0 && info["widthMm"].as_f64().unwrap() > 90.0, "the 100 mm block: {info}");
        let (mut s, heart) = with_heart();
        for bad in [
            json!({"ids": [heart], "fill": "pebbles"}),
            json!({"ids": [heart], "spacing": 0.5}),
            json!({"ids": [heart], "width": 100}),
            json!({"ids": [heart], "color": "blue"}),
            json!({"ids": [heart], "width": 20, "height": 20, "margin": 10}),
        ] {
            assert!(s.execute("embroidery.quiltBackground", &bad).is_err(), "{bad}");
        }
    }
}
