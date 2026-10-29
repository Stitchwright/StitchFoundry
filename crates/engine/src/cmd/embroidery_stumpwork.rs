//! StumpWork: raised, free-standing pieces (petals, leaves, wings) with a wire round their edge,
//! sewn on water-soluble stabilizer, cut out, rinsed and bent to shape. Behaviour from BERNINA's
//! public help on its stumpwork tools (a guide track for the wire, the wire laid and tacked down
//! into it, the piece's fill, then a satin or blanket cover over the wire, last; the wire set by its
//! gauge; <https://softwarehelp.bernina.com/en/MainHelp/PART_Special/cutwork/cutwork-16.htm>,
//! <https://softwarehelp.bernina.com/fr/MainHelp/PART_Special/cutwork/Stumpwork_concepts.htm>).
//! Each step is its own thread named for what to do, so the machine stops before it, as the
//! in-the-hoop projects do. Code and wording are our own.

use serde_json::{Value, json};

use super::{CommandSpec, NodeId, Result, Session, bad, bool_or, cmd, has_doc, str_param};
use vectorcraft_doc::NodeKind;

const CMD: &str = "embroidery.stumpwork";
/// The wire track's thread: the hooping is said with it, since it sews first.
const TRACK: (&str, &str) = ("#9aa5b1", "Wire track: hoop organza over water-soluble stabilizer");
/// The tackdown: a thread of its own (threads are told apart by colour), hidden by the cover.
const WIRE: (&str, &str) = ("#7b8794", "Wire: lay it on the track, tails at the start, and hold it");
/// The cover's name, when its colour isn't the fill's (else it goes on in the fill's thread).
const COVER: &str = "Cover: over the wire";
/// What to do after, added to the design's notes once.
const FINISH: &str = "Stumpwork: take the hoop off, cut each piece out close to its edge (not into the cover), rinse the stabilizer away, let it dry, then bend the wire to shape. Push the wire tails through the fabric where the piece goes and fold them under, or stitch the piece down by hand.";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.stumpwork",
        "Make StumpWork",
        ["Embroidery"],
        None,
        "{ids?: closed shapes (else the selection), gauge?: the wire's AWG number (28; 24–34, a bigger number a thinner wire), cover?: satin|blanket (satin), coverWidth?: mm (2.5; 1–6), color?: \"#rrggbb\" (the cover's thread; else the piece's), fill?: bool (keep or give the piece a fill inside the wire; true)} each shape a stumpwork piece, its steps sewn in order, each its own thread named for what to do: the wire's guide track, the wire tacked down with a zigzag as wide as the wire, the piece's fill, and the cover over the wire; the finishing (cut out, rinse, bend) goes in the design's notes; one undo step → {pieces, wireMm, steps: [thread names]}",
        has_doc,
        stumpwork
    )]
}

/// The closed shapes among `ids`, looking inside groups: (id, node). Refused when one is an open
/// line, type or a picture, or none is closed. `what` names them in the message.
pub(super) fn closed_shapes(s: &Session, ids: &[u64], cmd: &str, what: &str) -> Result<Vec<(u64, vectorcraft_doc::Node)>> {
    fn walk(n: &vectorcraft_doc::Node, out: &mut Vec<(u64, vectorcraft_doc::Node)>, bad_kind: &mut bool, open: &mut bool) {
        match &n.kind {
            NodeKind::Path { path, .. } => {
                if path.to_bezpath().elements().iter().any(|e| matches!(e, kurbo::PathEl::ClosePath)) {
                    out.push((n.id.0, n.clone()));
                } else {
                    *open = true;
                }
            }
            NodeKind::Group { children, .. } => children.iter().for_each(|c| walk(c, out, bad_kind, open)),
            _ => *bad_kind = true,
        }
    }
    let d = &s.doc()?.doc;
    let (mut out, mut bad_kind, mut open) = (Vec::new(), false, false);
    for id in ids {
        let n = d.node(NodeId(*id)).ok_or_else(|| bad(cmd, format!("no object {id}")))?;
        walk(n, &mut out, &mut bad_kind, &mut open);
    }
    if bad_kind {
        return Err(bad(cmd, format!("{what} are made from closed shapes: convert type to outlines first")));
    }
    if open || out.is_empty() {
        return Err(bad(cmd, format!("{what} are made from closed shapes: close the path first")));
    }
    Ok(out)
}

/// A wire's diameter (mm) from its American Wire Gauge number.
fn awg_mm(gauge: f64) -> f64 {
    0.127 * 92f64.powf((36.0 - gauge) / 39.0)
}

fn stumpwork(s: &mut Session, p: &Value) -> Result<Value> {
    let gauge = super::embroidery_lettering::ranged(p, "gauge", 24.0..=34.0, CMD, "`gauge` is an AWG number, 24 to 34")?.unwrap_or(28.0);
    let cover = str_param(p, "cover").unwrap_or("satin");
    if !["satin", "blanket"].contains(&cover) {
        return Err(bad(CMD, "`cover` is satin or blanket"));
    }
    let width = super::embroidery_lettering::ranged(p, "coverWidth", 1.0..=6.0, CMD, "`coverWidth` is 1 to 6 mm")?.unwrap_or(2.5);
    let color = match str_param(p, "color") {
        Some(c) => Some(sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?.to_hex()),
        None => None,
    };
    let fill = bool_or(p, "fill", true);
    let ids: Vec<u64> = match p.get("ids").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(Value::as_u64).collect(),
        None => s.active().map(|d| d.selection.objects.iter().map(|n| n.0).collect()).unwrap_or_default(),
    };
    if ids.is_empty() {
        return Err(bad(CMD, "select the closed shapes to make into stumpwork pieces"));
    }
    // closed paths only (a group's own closed shapes), with each one's thread colour and whether
    // it already fills
    let pieces: Vec<(u64, bool, Option<String>)> = closed_shapes(s, &ids, CMD, "stumpwork pieces")?
        .into_iter()
        .map(|(id, n)| {
            let emb = n.embroidery.as_deref();
            let fills = emb.is_some_and(|e| matches!(e.stitch, sf_core::attr::StitchType::Fill(_)));
            (id, fills, emb.and_then(|e| e.thread.as_ref()).map(|t| t.color.to_hex()))
        })
        .collect();
    let wire = awg_mm(gauge);
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Make StumpWork".into()));
    }
    let done = make(s, &pieces, wire, cover, width, color.as_deref(), fill);
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let made = done?;
    let mut steps = vec![TRACK.1.to_string(), WIRE.1.to_string()];
    if fill {
        steps.push("the piece's fill".into());
    }
    steps.push(COVER.to_string());
    Ok(json!({ "pieces": pieces.len(), "wireMm": (wire * 100.0).round() / 100.0, "steps": steps, "ids": made }))
}

fn make(
    s: &mut Session,
    pieces: &[(u64, bool, Option<String>)],
    wire: f64,
    cover: &str,
    width: f64,
    color: Option<&str>,
    fill: bool,
) -> Result<Vec<u64>> {
    let mut made = Vec::new();
    for (id, fills, own) in pieces {
        let cover_color = color.map(str::to_string).or_else(|| own.clone()).unwrap_or_else(|| "#2f6b3a".into());
        // copies directly above the shape, the newest nearest it: made last first, so the sewing
        // order is the shape (the track), the tackdown, the fill, the cover
        let dup = |s: &mut Session| -> Result<u64> {
            let r = s.execute("edit.duplicate", &json!({ "ids": [id] }))?;
            r["ids"].get(0).and_then(Value::as_u64).ok_or_else(|| bad(CMD, "the piece wasn't copied"))
        };
        let cover_id = dup(s)?;
        let fill_id = if fill { Some(dup(s)?) } else { None };
        let tack_id = dup(s)?;
        // a line along the edge, not the area: its art unfilled, stroked
        let edge = |s: &mut Session, id: u64, color: &str| -> Result<()> {
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("paint.setFill", &json!({ "none": true }))?;
            s.execute("paint.setStroke", &json!({ "color": color }))?;
            Ok(())
        };
        let setp = |s: &mut Session, id: u64, ty: &str, params: Value, thread: (&str, &str)| -> Result<()> {
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("embroidery.set", &json!({ "type": ty }))?;
            let mut all = params;
            all["own_block"] = json!(true);
            s.execute("embroidery.setParams", &json!({ "params": all }))?;
            s.execute("embroidery.setThread", &json!({ "color": thread.0, "name": thread.1, "ids": [id], "paint": false }))?;
            Ok(())
        };
        // the guide track: a double run along the edge, where the wire goes
        edge(s, *id, TRACK.0)?;
        setp(s, *id, "run", json!({ "passes": 2, "stitch_length": 2.0 }), TRACK)?;
        // the wire held down: a zigzag just wider than it, the needle either side of it
        edge(s, tack_id, WIRE.0)?;
        setp(s, tack_id, "zigzag", json!({ "width": wire + 0.8, "spacing": 1.6 }), WIRE)?;
        if let Some(f) = fill_id {
            if !fills {
                s.execute("select.set", &json!({ "ids": [f] }))?;
                s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            }
            s.execute("select.set", &json!({ "ids": [f] }))?;
            s.execute("embroidery.setParams", &json!({ "params": { "own_block": true } }))?;
            made.push(f);
        }
        // the cover over the wire, last: satin (or blanket) centred on the edge
        let width = width.max(wire + 1.2);
        let params = if cover == "satin" { json!({ "width": width }) } else { json!({ "width": width, "spacing": 0.8 }) };
        let fill_color = own.clone().unwrap_or_default();
        edge(s, cover_id, &cover_color)?;
        if fill_id.is_some() && cover_color.eq_ignore_ascii_case(&fill_color) {
            // the fill's thread goes on: no stop, its name kept
            s.execute("select.set", &json!({ "ids": [cover_id] }))?;
            s.execute("embroidery.set", &json!({ "type": cover }))?;
            s.execute("embroidery.setParams", &json!({ "params": params }))?;
        } else {
            setp(s, cover_id, cover, params, (&cover_color, COVER))?;
        }
        made.extend([*id, tack_id, cover_id]);
    }
    // the finishing, once, in the notes the worksheet prints
    let notes = s.doc()?.doc.embroidery.as_deref().map(|e| e.notes.clone()).unwrap_or_default();
    if !notes.contains(FINISH) {
        let notes = if notes.trim().is_empty() { FINISH.to_string() } else { format!("{}\n\n{FINISH}", notes.trim_end()) };
        s.execute("embroidery.settings", &json!({ "notes": notes.chars().take(2000).collect::<String>() }))?;
    }
    s.execute("select.set", &json!({ "ids": made }))?;
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_gauges_are_their_diameters() {
        assert!((awg_mm(28.0) - 0.321).abs() < 0.005, "{}", awg_mm(28.0));
        assert!((awg_mm(24.0) - 0.511).abs() < 0.005);
        assert!(awg_mm(34.0) < awg_mm(30.0), "a bigger number is a thinner wire");
    }

    #[test]
    fn a_shape_becomes_a_stumpwork_piece_sewn_in_its_steps() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let r = s.execute("shape.ellipse", &json!({"x": 150, "y": 150, "width": 120, "height": 70})).unwrap();
        let leaf = r["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [leaf]})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": "#2e8b57"})).unwrap();
        let r = s.execute(CMD, &json!({"gauge": 30})).unwrap();
        assert_eq!(r["pieces"], 1);
        assert!((r["wireMm"].as_f64().unwrap() - 0.25).abs() < 0.01, "{r}");
        // the threads in sewing order: the track, the wire, the fill, the cover; each a stop
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        let names: Vec<&str> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
        assert_eq!(names.first(), Some(&TRACK.1), "{names:?}");
        assert_eq!(names.get(1), Some(&WIRE.1), "{names:?}");
        // the fill and the cover in the piece's thread, one after the other: no stop between
        let colors: Vec<&str> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["color"].as_str()).collect();
        assert_eq!(colors, ["#9aa5b1", "#7b8794", "#2e8b57"], "{names:?}");
        // the cover sews last: a satin as wide as asked round the piece's edge
        let pat = s.embroidery_plan().unwrap().pattern().clone();
        assert!(pat.stitch_count() > 300, "{}", pat.stitch_count());
        // the satin cover is the last stitches: a column round the edge, not across the leaf
        let last: Vec<_> = pat.stitches.iter().rev().filter(|st| st.cmd == sf_core::Cmd::Stitch).take(60).collect();
        let steps: Vec<f64> = last.windows(2).map(|w| (w[0].x - w[1].x).hypot(w[0].y - w[1].y)).collect();
        assert!(steps.iter().all(|d| *d < 6.0), "short satin stitches across the edge: {steps:?}");
        assert!(s.doc().unwrap().doc.embroidery.as_deref().unwrap().notes.contains("rinse the stabilizer away"));
        // one undo takes it back to the plain shape
        s.execute("edit.undo", &json!({})).unwrap();
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["colors"].as_array().unwrap().len(), 1, "{info}");
        // blanket, no fill
        s.execute("select.set", &json!({"ids": [leaf]})).unwrap();
        let r = s.execute(CMD, &json!({"cover": "blanket", "fill": false, "color": "#aa3355"})).unwrap();
        assert_eq!(r["steps"].as_array().unwrap().len(), 3, "{r}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        let names: Vec<&str> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
        assert_eq!(names.last(), Some(&COVER), "a cover in its own colour is its own step: {names:?}");
        // open lines and bad settings are refused
        let line = s.execute("path.create", &json!({"d": "M10 10 L100 40"})).unwrap()["id"].as_u64().unwrap();
        assert!(s.execute(CMD, &json!({"ids": [line]})).is_err());
        for bad in [json!({"ids": [leaf], "gauge": 10}), json!({"ids": [leaf], "cover": "cord"}), json!({"ids": [leaf], "coverWidth": 9})] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
        s.execute("select.none", &json!({})).unwrap();
        assert!(s.execute(CMD, &json!({})).is_err(), "nothing selected");
    }
}
