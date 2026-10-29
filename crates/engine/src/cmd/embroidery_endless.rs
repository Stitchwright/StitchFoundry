//! Endless (HV-01): a border sewn hooping after hooping. The selected design is repeated along the
//! way the fabric moves as many times as fit the hoop, with start marks on the side the fabric
//! moves toward and end marks on the other: after each hooping the fabric is moved on and hooped
//! again, and the needle, stopped at each start mark, is lined up with the last hooping's end mark.
//! Behaviour from mySewnet's public help on Endless Embroidery
//! (<https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Endless_Embroidery.htm>): fabric
//! direction, four kinds of end mark (corner, single stitch, tie-off, line), marks on both sides or
//! one, a gap between repeats, as many repeats as fit. Code and wording are our own.

use std::sync::Arc;

use serde_json::{Value, json};
use vectorcraft_doc::{Document, Node};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::{Affine, Point, Vec2};

use super::embroidery_lettering::ranged;
use super::{CommandSpec, NodeId, Result, Session, bad, bool_or, cmd, has_selection, str_param, targets};

/// Repeats in one hooping at most.
const MAX_REPEATS: usize = 100;
/// How far outside the design the marks go (mm).
const MARK_OUT: f64 = 4.0;
/// The length of a corner's or a line's legs (mm).
const LEG: f64 = 5.0;

/// The threads the marks are sewn in, named for what to do: the machine stops at each.
const START: (&str, &str) = ("#d0342c", "Start marks: line the needle up with the last hooping's end marks");
const END: (&str, &str) = ("#2a7de1", "End marks: re-hoop with these where the start marks go");

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.endless",
        "Endless Border",
        ["Embroidery"],
        None,
        "{ids?, direction?: up|down|left|right (the way the fabric moves through the hoop; up for a tall hoop and left for a wide one), marker?: corner|single|tieoff|line (the end marks: a small L of long stitches, its corner the point to line up with; one long stitch to the point; a few stitches on the point; three stitches in a line ending at it; corner), sides?: both|one (marks on both sides of the border, or the first side only; both), repeat?: bool (as many repeats as fit the hoop; true), gap?: mm between repeats (0; −50 % of the design to 100)} the selected design as a border sewn hooping after hooping: repeated along the fabric's direction as many times as fit the hoop, start marks before it on the side the fabric moves toward and end marks after it on the other, each in a thread of its own so the machine stops at them, all in a group selected (the design replaced) → {id, repeats, pitch: mm the fabric moves each hooping}",
        has_selection,
        endless
    )]
}

/// A direction's unit vector on the page (y down): the way the fabric moves.
fn unit(direction: &str) -> Option<Vec2> {
    match direction {
        "up" => Some(Vec2::new(0.0, -1.0)),
        "down" => Some(Vec2::new(0.0, 1.0)),
        "left" => Some(Vec2::new(-1.0, 0.0)),
        "right" => Some(Vec2::new(1.0, 0.0)),
        _ => None,
    }
}

/// An end mark's points (page units) for alignment point `p`, the band running along `along`
/// (from the start side toward the end side) and `out` pointing away from the border.
fn end_mark(kind: &str, p: Point, along: Vec2, out: Vec2) -> Vec<Point> {
    let leg = LEG * PT_PER_MM;
    match kind {
        // an L: in toward the design, to the point, then out
        "corner" => vec![p - along * leg, p, p + out * leg],
        // one long stitch from inside onto the point
        "single" => vec![p - out * leg, p],
        // a few stitches on the point
        "tieoff" => vec![p, p + out * 0.4 * PT_PER_MM, p - out * 0.4 * PT_PER_MM, p],
        // three long stitches in a line along the band, ending at the point
        _ => vec![p - along * 3.0 * leg, p - along * 2.0 * leg, p - along * leg, p],
    }
}

fn endless(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.endless";
    let marker = str_param(p, "marker").unwrap_or("corner");
    if !["corner", "single", "tieoff", "line"].contains(&marker) {
        return Err(bad(CMD, "`marker` is corner, single, tieoff or line"));
    }
    let both = match str_param(p, "sides").unwrap_or("both") {
        "both" => true,
        "one" => false,
        _ => return Err(bad(CMD, "`sides` is both or one")),
    };
    let fill = bool_or(p, "repeat", true);
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    let (field, dbox) = {
        let d = &s.doc()?.doc;
        let field = d.artboards.first().map(|a| a.rect).ok_or_else(|| bad(CMD, "the design has no hoop"))?;
        let dbox = ids
            .iter()
            .filter_map(|id| d.node(*id).and_then(|n| n.geometric_bounds()))
            .reduce(|a, b| a.union(b))
            .ok_or_else(|| bad(CMD, "select the design to repeat"))?;
        (field, dbox)
    };
    if !(dbox.width().is_finite() && dbox.height().is_finite()) || dbox.width() <= 0.0 || dbox.height() <= 0.0 {
        return Err(bad(CMD, "the design has no size"));
    }
    let direction = match str_param(p, "direction") {
        Some(d) => d,
        None if field.height() >= field.width() => "up",
        None => "left",
    };
    // the fabric moves toward the start side: the band runs from there to the end side
    let toward = unit(direction).ok_or_else(|| bad(CMD, "`direction` is up, down, left or right"))?;
    let along = -toward;
    let vertical = toward.x == 0.0;
    let length = if vertical { dbox.height() } else { dbox.width() };
    let gap = ranged(p, "gap", -50.0..=100.0, CMD, "`gap` is −50 to 100 mm")?.unwrap_or(0.0) * PT_PER_MM;
    let pitch = length + gap.max(-length / 2.0);
    // from the start marks to the end marks is a pitch a repeat: as many as fit the hoop's
    // length, the marks inside it
    let room = (if vertical { field.height() } else { field.width() }) - 2.0 * PT_PER_MM;
    if pitch > room + 1e-6 {
        return Err(bad(CMD, "the design is longer than the hoop along the fabric's direction: make it smaller, the gap narrower or turn the hoop"));
    }
    let repeats = if fill { ((room / pitch).floor() as usize).clamp(1, MAX_REPEATS) } else { 1 };
    let span = pitch * repeats as f64;
    let mid = field.center();
    let start_pt = mid - along * (span / 2.0);
    // the end marks: where the next hooping's start marks go
    let end_pt = start_pt + along * span;
    let dc = dbox.center();
    let offsets: Vec<Vec2> = (0..repeats).map(|k| (start_pt + along * (pitch * k as f64 + length / 2.0)) - dc).collect();
    // the marks just outside the design across the band
    let across = Vec2::new(along.y.abs(), along.x.abs());
    let half_w = (if vertical { dbox.width() } else { dbox.height() }) / 2.0 + MARK_OUT * PT_PER_MM;
    let half_w = half_w.min((if vertical { field.width() } else { field.height() }) / 2.0 - 1.0 * PT_PER_MM);
    let sides: Vec<f64> = if both { vec![-1.0, 1.0] } else { vec![-1.0] };
    let pitch_mm = span / PT_PER_MM;
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Endless Border".into()));
    }
    let done = (|| -> Result<u64> {
        // the marks, each a stitched line in its own thread
        let mark = |s: &mut Session, pts: &[Point], thread: (&str, &str)| -> Result<u64> {
            let mut d = String::new();
            for (i, q) in pts.iter().enumerate() {
                d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, q.x, q.y));
            }
            let id = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(CMD, "a mark wasn't made"))?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("embroidery.set", &json!({ "type": "run" }))?;
            // long loose stitches, one a leg: easy to pick out after
            s.execute("embroidery.setParams", &json!({ "params": { "stitch_length": LEG, "own_block": true } }))?;
            s.execute("embroidery.setThread", &json!({ "color": thread.0, "name": thread.1, "ids": [id], "paint": false }))?;
            Ok(id)
        };
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        for side in &sides {
            let out = across * *side;
            let sp = start_pt + out * half_w;
            // a start mark: the needle brought to the point (a stitch there and back)
            starts.push(mark(s, &[sp + along * 0.3 * PT_PER_MM, sp], START)?);
            let ep = end_pt + out * half_w;
            ends.push(mark(s, &end_mark(marker, ep, along, out), END)?);
        }
        let parent = s.doc()?.insertion_parent();
        s.edit("Endless Border", |d, sel| {
            let originals: Vec<Arc<Node>> = ids.iter().filter_map(|id| d.node(*id).cloned()).map(Arc::new).collect();
            let take = |d: &mut Document, list: &[u64]| -> Vec<Arc<Node>> { list.iter().filter_map(|id| d.remove(NodeId(*id)).ok()).collect() };
            let mut children: Vec<Arc<Node>> = take(d, &starts);
            for (k, off) in offsets.iter().enumerate() {
                let m = Affine::translate(*off);
                let members: Vec<Arc<Node>> = originals
                    .iter()
                    .map(|n| {
                        let mut n = d.reid(n);
                        n.transform(m, true);
                        Arc::new(n)
                    })
                    .collect();
                let mut g = Node::group(d.alloc_id(), members);
                g.name = Some(format!("Repeat {}", k + 1));
                children.push(Arc::new(g));
            }
            children.extend(take(d, &ends));
            let mut all = Node::group(d.alloc_id(), children);
            all.name = Some("Endless border".into());
            let id = all.id;
            d.remove_many(&ids)?;
            d.insert(parent, usize::MAX, all)?;
            sel.set([id]);
            Ok(id.0)
        })
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let id = done?;
    Ok(json!({ "id": id, "repeats": repeats, "pitch": pitch_mm }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_doc::NodeKind;

    fn with_heart(hoop: &str) -> (Session, u64) {
        with_heart_in(json!(hoop))
    }

    fn with_heart_in(hoop: Value) -> (Session, u64) {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({ "hoop": hoop })).unwrap();
        let id = s.execute("embroidery.addShape", &json!({"name": "Heart", "size": 30})).unwrap()["id"].as_u64().unwrap();
        (s, id)
    }

    fn children(s: &Session, id: u64) -> Vec<Arc<Node>> {
        let d = &s.doc().unwrap().doc;
        let NodeKind::Group { children, .. } = &d.node(NodeId(id)).unwrap().kind else { panic!("a group") };
        children.clone()
    }

    /// In a tall hoop the fabric moves up: repeats down the hoop as many as fit, start marks above
    /// them and end marks below, the marks a pitch apart, everything inside the hoop and its own
    /// thread; one undo takes it back.
    #[test]
    fn a_tall_hoop_repeats_down_it_between_start_and_end_marks() {
        let (mut s, heart) = with_heart("Brother 130 × 180 (5×7)");
        let r = s.execute("embroidery.endless", &json!({"ids": [heart]})).unwrap();
        let repeats = r["repeats"].as_u64().unwrap() as usize;
        assert!(repeats >= 4, "{r}");
        let kids = children(&s, r["id"].as_u64().unwrap());
        // two start marks, the repeats, two end marks
        assert_eq!(kids.len(), repeats + 4);
        let b = |n: &Arc<Node>| n.geometric_bounds().unwrap();
        let (start, rep0, end) = (b(&kids[0]), b(&kids[2]), b(&kids[kids.len() - 1]));
        assert!(start.y1 <= rep0.y0 + 1.0, "start marks above the first repeat: {start:?} {rep0:?}");
        assert!(end.y0 > b(&kids[2 + repeats - 1]).y0, "end marks below the last");
        let field = s.doc().unwrap().doc.artboards[0].rect;
        assert!(kids.iter().all(|k| field.contains(b(k).center())), "inside the hoop");
        // the end marks are a pitch on from the start marks: where the next start goes
        let pitch = r["pitch"].as_f64().unwrap() * PT_PER_MM;
        assert!((end.y1 - start.y1 - pitch).abs() < 2.0 * PT_PER_MM, "{start:?} {end:?} {pitch}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        let names: Vec<String> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["name"].as_str().map(str::to_string)).collect();
        assert_eq!(names.first().map(String::as_str), Some(START.1), "{names:?}");
        assert_eq!(names.last().map(String::as_str), Some(END.1), "{names:?}");
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(heart)).is_some(), "one undo brings the design back");
    }

    /// A wide hoop runs left; one side, one repeat, each kind of mark; bad params refused.
    #[test]
    fn a_wide_hoop_one_side_and_each_mark() {
        let (mut s, heart) = with_heart_in(json!({"width": 260, "height": 160}));
        let r = s.execute("embroidery.endless", &json!({"ids": [heart], "sides": "one", "repeat": false, "marker": "line"})).unwrap();
        assert_eq!(r["repeats"], 1);
        let kids = children(&s, r["id"].as_u64().unwrap());
        assert_eq!(kids.len(), 3, "a start mark, the design, an end mark");
        let (start, end) = (kids[0].geometric_bounds().unwrap(), kids[2].geometric_bounds().unwrap());
        assert!(start.x0 < end.x0, "the fabric moves left: the start at the left");
        for marker in ["corner", "single", "tieoff"] {
            let (mut s, heart) = with_heart("Brother 130 × 180 (5×7)");
            assert!(s.execute("embroidery.endless", &json!({"ids": [heart], "marker": marker})).is_ok(), "{marker}");
        }
        let (mut s, heart) = with_heart("Brother 130 × 180 (5×7)");
        for bad in [
            json!({"ids": [heart], "marker": "dot"}),
            json!({"ids": [heart], "sides": "three"}),
            json!({"ids": [heart], "direction": "sideways"}),
            json!({"ids": [heart], "gap": 500}),
        ] {
            assert!(s.execute("embroidery.endless", &bad).is_err(), "{bad}");
        }
        assert_eq!(end_mark("corner", Point::new(0.0, 0.0), Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)).len(), 3);
    }
}
