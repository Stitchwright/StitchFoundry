//! Lacework: an open, trellis-like fill that holds together on its own, for free-standing lace
//! sewn on water-soluble stabilizer. Behaviour from BERNINA's public help on its Lacework fill (an
//! open trellis pattern giving a lacy look, its layout chosen from presets;
//! <https://softwarehelp.bernina.com/en/MainHelp/PART_Decorative/decorative/Lacework.htm>) and
//! the free-standing lace practice its tutorials describe (the base sewn first, on a stabilizer
//! that is washed away). Each shape becomes layers of open rows crossing at the pattern's angles,
//! so the threads lock where they cross, and a satin edge round it; all one thread, sewn without a
//! stop. Code and wording are our own.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, bad, cmd, has_doc, str_param};

const CMD: &str = "embroidery.lacework";
/// What to do with it, added to the design's notes once.
const NOTES: &str = "Lacework: hoop two layers of fibrous water-soluble stabilizer, drum tight, and use the same thread in the bobbin as on top. After sewing, trim the stabilizer close to the lace, rinse it away in warm water, and pin the lace flat to dry.";

/// A pattern's row angles (degrees).
fn angles(pattern: &str) -> Option<&'static [f64]> {
    match pattern {
        "diamond" => Some(&[45.0, 135.0]),
        "grid" => Some(&[0.0, 90.0]),
        "triangle" => Some(&[0.0, 60.0, 120.0]),
        "star" => Some(&[0.0, 45.0, 90.0, 135.0]),
        _ => None,
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.lacework",
        "Make Lacework",
        ["Embroidery"],
        None,
        "{ids?: closed shapes (else the selection), pattern?: diamond|grid|triangle|star (diamond: rows at 45° and 135°; grid: 0° and 90°; triangle: 0°, 60° and 120°; star: all four), spacing?: mm between rows (2.5; 1.5–6), edge?: mm the satin edge is wide (3; 0 for none, else 1.5–8), color?: \"#rrggbb\" (the lace's thread; else the shape's)} each shape free-standing lace: open rows crossing at the pattern's angles, locking where they cross, then a satin edge round it, all in one thread with no stop; sewing it on water-soluble stabilizer goes in the design's notes; one undo step → {pieces, layers, ids}",
        has_doc,
        lacework
    )]
}

fn lacework(s: &mut Session, p: &Value) -> Result<Value> {
    let pattern = str_param(p, "pattern").unwrap_or("diamond");
    let layers = angles(pattern).ok_or_else(|| bad(CMD, "`pattern` is diamond, grid, triangle or star"))?;
    let spacing = super::embroidery_lettering::ranged(p, "spacing", 1.5..=6.0, CMD, "`spacing` is 1.5 to 6 mm")?.unwrap_or(2.5);
    let edge = match p.get("edge") {
        None | Some(Value::Null) => 3.0,
        Some(v) => v.as_f64().filter(|w| *w == 0.0 || (1.5..=8.0).contains(w)).ok_or_else(|| bad(CMD, "`edge` is 0 (none) or 1.5 to 8 mm"))?,
    };
    let color = match str_param(p, "color") {
        Some(c) => Some(sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?.to_hex()),
        None => None,
    };
    let ids: Vec<u64> = match p.get("ids").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(Value::as_u64).collect(),
        None => s.active().map(|d| d.selection.objects.iter().map(|n| n.0).collect()).unwrap_or_default(),
    };
    if ids.is_empty() {
        return Err(bad(CMD, "select the closed shapes to make into lace"));
    }
    let pieces: Vec<(u64, Option<String>)> = super::embroidery_stumpwork::closed_shapes(s, &ids, CMD, "lace pieces")?
        .into_iter()
        .map(|(id, n)| (id, n.embroidery.as_deref().and_then(|e| e.thread.as_ref()).map(|t| t.color.to_hex())))
        .collect();
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Make Lacework".into()));
    }
    let done = make(s, &pieces, layers, spacing, edge, color.as_deref());
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let made = done?;
    Ok(json!({ "pieces": pieces.len(), "layers": layers.len(), "ids": made }))
}

fn make(s: &mut Session, pieces: &[(u64, Option<String>)], layers: &[f64], spacing: f64, edge: f64, color: Option<&str>) -> Result<Vec<u64>> {
    let mut made = Vec::new();
    for (id, own) in pieces {
        let thread = color.map(str::to_string).or_else(|| own.clone()).unwrap_or_else(|| "#f4f1ea".into());
        // copies directly above the shape, the newest nearest it: made last first, so the
        // sewing order is the shape (the first layer), the other layers, the edge
        let dup = |s: &mut Session| -> Result<u64> {
            let r = s.execute("edit.duplicate", &json!({ "ids": [id] }))?;
            r["ids"].get(0).and_then(Value::as_u64).ok_or_else(|| bad(CMD, "the shape wasn't copied"))
        };
        let edge_id = if edge > 0.0 { Some(dup(s)?) } else { None };
        let mut net = vec![*id];
        for _ in 1..layers.len() {
            net.insert(1, dup(s)?);
        }
        for (layer, angle) in net.iter().zip(layers) {
            s.execute("select.set", &json!({ "ids": [layer] }))?;
            s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            s.execute(
                "embroidery.setParams",
                &json!({ "params": {
                    "spacing": spacing, "angle": angle, "stitch_length": 2.0, "pull_comp": 0.0, "auto_direction": false, "remove_underlying": false,
                    "underlay": { "auto": false, "edge_run": false, "tatami": false },
                } }),
            )?;
            s.execute("embroidery.setThread", &json!({ "color": thread, "name": "Lace", "ids": [layer], "paint": false }))?;
            made.push(*layer);
        }
        if let Some(e) = edge_id {
            s.execute("select.set", &json!({ "ids": [e] }))?;
            s.execute("paint.setFill", &json!({ "none": true }))?;
            s.execute("paint.setStroke", &json!({ "color": thread }))?;
            s.execute("embroidery.set", &json!({ "type": "satin" }))?;
            s.execute("embroidery.setParams", &json!({ "params": { "width": edge } }))?;
            s.execute("embroidery.setThread", &json!({ "color": thread, "name": "Lace", "ids": [e], "paint": false }))?;
            made.push(e);
        }
    }
    let notes = s.doc()?.doc.embroidery.as_deref().map(|e| e.notes.clone()).unwrap_or_default();
    if !notes.contains(NOTES) {
        let notes = if notes.trim().is_empty() { NOTES.to_string() } else { format!("{}\n\n{NOTES}", notes.trim_end()) };
        s.execute("embroidery.settings", &json!({ "notes": notes.chars().take(2000).collect::<String>() }))?;
    }
    s.execute("select.set", &json!({ "ids": made }))?;
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::attr::StitchType;
    use vectorcraft_doc::NodeId;

    #[test]
    fn a_shape_becomes_open_crossing_rows_and_a_satin_edge() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let r = s.execute("shape.ellipse", &json!({"x": 150, "y": 150, "width": 140, "height": 140})).unwrap();
        let disc = r["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [disc]})).unwrap();
        let r = s.execute(CMD, &json!({"pattern": "triangle", "color": "#ffffff"})).unwrap();
        assert_eq!((r["pieces"].as_u64(), r["layers"].as_u64()), (Some(1), Some(3)), "{r}");
        let ids: Vec<u64> = r["ids"].as_array().unwrap().iter().filter_map(Value::as_u64).collect();
        assert_eq!(ids.len(), 4, "three layers and the edge");
        let d = &s.doc().unwrap().doc;
        let angles: Vec<f64> = ids[..3]
            .iter()
            .map(|i| match &d.node(NodeId(*i)).unwrap().embroidery.as_deref().unwrap().stitch {
                StitchType::Fill(f) => {
                    assert!((f.spacing - 2.5).abs() < 1e-9, "open rows: {}", f.spacing);
                    f.angle
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(angles, [0.0, 60.0, 120.0]);
        assert!(matches!(d.node(NodeId(ids[3])).unwrap().embroidery.as_deref().unwrap().stitch, StitchType::Satin(_)), "the edge last");
        // one thread, no stop
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["colors"].as_array().unwrap().len(), 1, "{info}");
        assert_eq!(info["colorChanges"].as_u64(), Some(0), "{info}");
        assert!(s.doc().unwrap().doc.embroidery.as_deref().unwrap().notes.contains("water-soluble stabilizer"));
        // one undo
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap_or(0), 0);
        // every layer sews: three directions of rows are about three times one layer's stitches
        // (on a shape from Add Shape, whose fills leave out what is above them by default)
        let star = s.execute("embroidery.addShape", &json!({"name": "Star", "size": 80})).unwrap()["id"].as_u64().unwrap();
        let count = |s: &mut Session, pattern: &str| {
            s.execute("select.set", &json!({"ids": [star]})).unwrap();
            s.execute(CMD, &json!({"pattern": pattern, "edge": 0})).unwrap();
            let n = s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap();
            s.execute("edit.undo", &json!({})).unwrap();
            n
        };
        let two = count(&mut s, "grid");
        // one layer of the same rows, alone
        s.execute("select.set", &json!({"ids": [star]})).unwrap();
        s.execute("embroidery.setParams", &json!({"params": {"spacing": 2.5, "angle": 0, "stitch_length": 2.0, "auto_direction": false, "underlay": {"auto": false, "edge_run": false, "tatami": false}}})).unwrap();
        let one = s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap();
        assert!(two as f64 > 1.8 * one as f64, "no layer hides another: one layer {one}, two {two}");
        // no edge; bad settings refused
        s.execute("select.set", &json!({"ids": [disc]})).unwrap();
        assert_eq!(s.execute(CMD, &json!({"edge": 0})).unwrap()["ids"].as_array().unwrap().len(), 2);
        for bad in [json!({"ids": [disc], "pattern": "hex"}), json!({"ids": [disc], "spacing": 9}), json!({"ids": [disc], "edge": 1})] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
        let line = s.execute("path.create", &json!({"d": "M10 10 L100 40"})).unwrap()["id"].as_u64().unwrap();
        assert!(s.execute(CMD, &json!({"ids": [line]})).is_err());
    }
}
