//! Cutwork (BEY-05): machines with cutting needles cut the fabric with a blade that faces one
//! way, so each needle cuts only lines running within its own range of directions. Cutwork splits
//! the selected lines by direction, one running-stitch line per stretch, grouped by needle so
//! each needle cuts all of its stretches in one go (a colour change, a stop, between needles).

use serde_json::{Value, json};
use vectorcraft_doc::NodeId;

use super::{CommandSpec, Result, Session, bad, cmd, has_selection};

const CMD: &str = "embroidery.cutwork";
/// The needles' thread colours on screen (one per needle; the cuts aren't sewn in thread).
const NEEDLE_COLORS: [&str; 4] = ["#d62828", "#2a9d8f", "#3a5ba0", "#f4a261"];
/// Shortest stretch kept on its own (mm): shorter ones join the stretch before.
const MIN_STRETCH: f64 = 0.5;
/// Most points cut from one design.
const MAX_POINTS: usize = 200_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.cutwork",
        "Cutwork",
        ["Embroidery"],
        None,
        "{ids?, needles?: 1-4 (4), stitch?: mm (2.5; 1 to 6)} split the selected lines for a cutwork machine: each stretch running within one needle's range of directions (needle 1 from 0° to 180° / needles, needle 2 the next…; either way along the line) becomes a running-stitch line in that needle's colour, named after it; the lines are grouped by needle, needle 1's first, in one group that takes the selected line's place → {id, needles: [{needle, from, to, lines}]}",
        has_selection,
        cutwork
    )]
}

/// The needle (0-based of `n`) that cuts a stretch from `a` to `b`: by its direction either way.
fn needle_of(a: sf_core::Point, b: sf_core::Point, n: usize) -> usize {
    let deg = (-(b.y - a.y)).atan2(b.x - a.x).to_degrees().rem_euclid(180.0);
    let k = (deg / (180.0 / n as f64)).floor();
    if k.is_finite() { (k as usize).min(n.saturating_sub(1)) } else { 0 }
}

/// `line` cut into stretches, each with the needle that cuts it.
fn stretches(line: &[sf_core::Point], n: usize) -> Vec<(usize, Vec<sf_core::Point>)> {
    let mut out: Vec<(usize, Vec<sf_core::Point>)> = Vec::new();
    for w in line.windows(2) {
        let [a, b] = w else { continue };
        if a.distance(*b) < 1e-6 || !(a.is_finite() && b.is_finite()) {
            continue;
        }
        let k = needle_of(*a, *b, n);
        match out.last_mut() {
            Some((last, pts)) if *last == k => pts.push(*b),
            _ => out.push((k, vec![*a, *b])),
        }
    }
    // a stretch too short to cut on its own goes with the one before (and then the same
    // needle's next one with it)
    let mut merged: Vec<(usize, Vec<sf_core::Point>)> = Vec::new();
    for (k, pts) in out {
        let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        match merged.last_mut() {
            Some((last, prev)) if len < MIN_STRETCH || *last == k => prev.extend(pts.into_iter().skip(1)),
            _ => merged.push((k, pts)),
        }
    }
    merged
}

fn path_d(pts: &[sf_core::Point]) -> String {
    let mut d = String::new();
    for (i, p) in pts.iter().enumerate() {
        let q = vectorcraft_embroidery::pt(*p);
        d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, q.x, q.y));
    }
    d
}

fn cutwork(s: &mut Session, p: &Value) -> Result<Value> {
    let n = match p.get("needles") {
        None | Some(Value::Null) => 4,
        Some(v) => v.as_u64().filter(|n| (1..=4).contains(n)).ok_or_else(|| bad(CMD, "`needles` is 1 to 4"))? as usize,
    };
    let stitch = match p.get("stitch") {
        None | Some(Value::Null) => 2.5,
        Some(v) => v.as_f64().filter(|v| (1.0..=6.0).contains(v)).ok_or_else(|| bad(CMD, "`stitch` is 1 to 6 mm"))?,
    };
    let ids: Vec<NodeId> = super::embroidery::stitched_targets(s, p)?;
    // the lines, by needle
    let mut by_needle: Vec<Vec<Vec<sf_core::Point>>> = vec![Vec::new(); n];
    let mut points = 0usize;
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(node) = d.node(*id) else { continue };
            for line in vectorcraft_embroidery::outlines_mm(node) {
                points += line.len();
                if points > MAX_POINTS {
                    return Err(bad(CMD, "too many points to cut: simplify the lines first"));
                }
                for (k, pts) in stretches(&line, n) {
                    if let Some(list) = by_needle.get_mut(k) {
                        list.push(pts);
                    }
                }
            }
        }
    }
    if by_needle.iter().all(Vec::is_empty) {
        return Err(bad(CMD, "select lines to cut"));
    }
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Cutwork".into()));
    }
    let done = (|| -> Result<Value> {
        let span = 180.0 / n as f64;
        let mut made = Vec::new();
        let mut needles = Vec::new();
        for (k, lines) in by_needle.iter().enumerate() {
            let (from, to) = (span * k as f64, span * (k + 1) as f64);
            let color = NEEDLE_COLORS.get(k).copied().unwrap_or("#000000");
            let name = format!("Cutwork needle {} ({from:.0}°–{to:.0}°)", k + 1);
            for line in lines {
                let r = s.execute("path.create", &json!({ "d": path_d(line) }))?;
                let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "a line wasn't made"))?;
                s.execute("select.set", &json!({ "ids": [id] }))?;
                s.execute("embroidery.set", &json!({ "type": "run" }))?;
                s.execute("embroidery.setParams", &json!({ "params": { "stitch_length": stitch } }))?;
                s.execute("embroidery.setThread", &json!({ "color": color, "name": name, "ids": [id] }))?;
                made.push(id);
            }
            if !lines.is_empty() {
                needles.push(json!({ "needle": k + 1, "from": from, "to": to, "lines": lines.len() }));
            }
        }
        s.execute("edit.clear", &json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))?;
        s.execute("select.set", &json!({ "ids": made }))?;
        let g = s.execute("object.group", &json!({}))?;
        Ok(json!({ "id": g["id"], "needles": needles }))
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> sf_core::Point {
        sf_core::Point::new(x, y)
    }

    #[test]
    fn stretches_go_to_the_needle_for_their_direction() {
        // right, up, left along a box: 0°, 90° and 180° (= 0°) with four needles
        let line = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, -10.0), p(0.0, -10.0)];
        let st = stretches(&line, 4);
        let ks: Vec<usize> = st.iter().map(|(k, _)| *k).collect();
        assert_eq!(ks, vec![0, 2, 0]);
        // a diagonal at 45° is needle 2's, at 135° needle 4's
        assert_eq!(needle_of(p(0.0, 0.0), p(5.0, -5.0), 4), 1);
        assert_eq!(needle_of(p(0.0, 0.0), p(-5.0, -5.0), 4), 3);
        // either way along the line
        assert_eq!(needle_of(p(5.0, -5.0), p(0.0, 0.0), 4), 1);
        // one needle cuts it all in one stretch
        assert_eq!(stretches(&line, 1).len(), 1);
        // a tiny jog joins the stretch before
        let jog = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, -0.2), p(20.0, -0.2)];
        assert_eq!(stretches(&jog, 4).len(), 1);
    }
}
