//! Eyelets: small holes cut in the fabric and bound with satin, for lacing, ribbon or broderie
//! anglaise. Behaviour from BERNINA's public Eyelet Set guide (the hole opened a little smaller
//! than the eyelet is to be, its edge then stitched round;
//! <https://bernina.com/en-IN/Learn-Create-IN/Projects-Downloads/Accessory-Projects/Accessories-tips/Foot-Book-Eyelet-Set-82-with-Eyelet-Foot-92/BERNINA-FootBook-EyeletSet82.pdf>)
//! and the usual way they are sewn in the hoop: the hole's edge stitched to hold the fabric, a
//! stop to cut it open, then satin over the cut edge. Each step its own thread named for what to
//! do, so the machine stops before the satin. Code and wording are our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, Result, Session, bad, cmd, has_doc, str_param};

const CMD: &str = "embroidery.eyelet";
const HOLE: (&str, &str) = ("#9aa5b1", "Hole: stitched round it; take the hoop off and cut it open inside the stitching");
const BIND: &str = "Eyelet: satin over the cut edge";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.eyelet",
        "Make Eyelets",
        ["Embroidery"],
        None,
        "{ids?: closed shapes to make into eyelets (else the selection; none selected: a round eyelet in the middle of the design), diameter?: mm the round eyelet's hole is across (4; 1.5–20), width?: mm the satin binding is wide (1.5; 0.8–4), color?: \"#rrggbb\" (the binding's thread; else the shape's)} each hole an eyelet sewn in two steps, each its own thread: the hole's edge stitched round twice to hold the fabric (then the machine stops for it to be cut open inside the stitching), and a satin binding over the cut edge; one undo step → {eyelets, ids}",
        has_doc,
        eyelet
    )]
}

fn eyelet(s: &mut Session, p: &Value) -> Result<Value> {
    let diameter = super::embroidery_lettering::ranged(p, "diameter", 1.5..=20.0, CMD, "`diameter` is 1.5 to 20 mm")?.unwrap_or(4.0);
    let width = super::embroidery_lettering::ranged(p, "width", 0.8..=4.0, CMD, "`width` is 0.8 to 4 mm")?.unwrap_or(1.5);
    let color = match str_param(p, "color") {
        Some(c) => Some(sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?.to_hex()),
        None => None,
    };
    let ids: Vec<u64> = match p.get("ids").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(Value::as_u64).collect(),
        None => s.active().map(|d| d.selection.objects.iter().map(|n| n.0).collect()).unwrap_or_default(),
    };
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Make Eyelets".into()));
    }
    let done = (|| -> Result<Vec<u64>> {
        // the round one made here: its binding drawn just outside the hole (overlapping the cut
        // edge by 0.3 mm), so the hole stays as wide as asked
        let mut round_binding = None;
        let holes: Vec<(u64, Option<String>)> = if ids.is_empty() {
            // a round one in the middle of the design
            let c = {
                let d = &s.doc()?.doc;
                d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
            };
            let r = diameter / 2.0 * PT_PER_MM;
            let made = s.execute("shape.ellipse", &json!({ "x": c.x - r, "y": c.y - r, "width": 2.0 * r, "height": 2.0 * r }))?;
            let rb = r + (width / 2.0 - 0.3).max(0.0) * PT_PER_MM;
            let b = s.execute("shape.ellipse", &json!({ "x": c.x - rb, "y": c.y - rb, "width": 2.0 * rb, "height": 2.0 * rb }))?;
            round_binding = b["id"].as_u64();
            vec![(made["id"].as_u64().ok_or_else(|| bad(CMD, "the eyelet wasn't made"))?, None)]
        } else {
            super::embroidery_stumpwork::closed_shapes(s, &ids, CMD, "eyelets")?
                .into_iter()
                .map(|(id, n)| (id, n.embroidery.as_deref().and_then(|e| e.thread.as_ref()).map(|t| t.color.to_hex())))
                .collect()
        };
        let mut made = Vec::new();
        for (id, own) in holes {
            let thread = color.clone().or(own).unwrap_or_else(|| "#f4f1ea".into());
            // the binding above the hole's stitching, so it sews after it
            let bind = match round_binding.take() {
                Some(b) => b,
                None => s.execute("edit.duplicate", &json!({ "ids": [id] }))?["ids"]
                    .get(0)
                    .and_then(Value::as_u64)
                    .ok_or_else(|| bad(CMD, "the eyelet wasn't copied"))?,
            };
            for (node, ty, params, (c, name)) in [
                (id, "run", json!({ "passes": 2, "stitch_length": 1.5, "own_block": true }), (HOLE.0, HOLE.1)),
                (bind, "satin", json!({ "width": width, "own_block": true }), (thread.as_str(), BIND)),
            ] {
                s.execute("select.set", &json!({ "ids": [node] }))?;
                s.execute("paint.setFill", &json!({ "none": true }))?;
                s.execute("paint.setStroke", &json!({ "color": c }))?;
                s.execute("embroidery.set", &json!({ "type": ty }))?;
                s.execute("embroidery.setParams", &json!({ "params": params }))?;
                s.execute("embroidery.setThread", &json!({ "color": c, "name": name, "ids": [node], "paint": false }))?;
            }
            made.extend([id, bind]);
        }
        s.execute("select.set", &json!({ "ids": made }))?;
        Ok(made)
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let made = done?;
    Ok(json!({ "eyelets": made.len() / 2, "ids": made }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_eyelet_is_a_stitched_hole_a_stop_and_a_satin_binding() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 100 × 100 (4×4)"})).unwrap();
        let r = s.execute(CMD, &json!({"diameter": 5, "color": "#ffffff"})).unwrap();
        assert_eq!(r["eyelets"], 1);
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        let names: Vec<&str> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
        assert_eq!(names, [HOLE.1, BIND], "{info}");
        assert_eq!(info["colorChanges"].as_u64(), Some(1), "a stop to cut it");
        // the satin binding is outside the 5 mm hole: about 5 mm plus twice its width across, the
        // hole itself left open
        let w = info["widthMm"].as_f64().unwrap();
        assert!((6.8..8.6).contains(&w), "{w}");
        let c = s.doc().unwrap().doc.artboards[0].rect.center();
        let (cx, cy) = (c.x / PT_PER_MM, c.y / PT_PER_MM);
        let pat = s.embroidery_plan().unwrap().pattern().clone();
        let inner = pat.stitches.iter().rev().take(80).map(|st| (st.x - cx).hypot(st.y - cy)).fold(f64::MAX, f64::min);
        assert!(inner > 2.0, "the satin keeps out of the hole: {inner}");
        // one undo takes it away; selected shapes become eyelets; bad settings refused
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap_or(0), 0);
        let a = s.execute("shape.ellipse", &json!({"x": 100, "y": 100, "width": 20, "height": 30})).unwrap()["id"].as_u64().unwrap();
        let b = s.execute("shape.ellipse", &json!({"x": 160, "y": 100, "width": 20, "height": 30})).unwrap()["id"].as_u64().unwrap();
        assert_eq!(s.execute(CMD, &json!({"ids": [a, b]})).unwrap()["eyelets"], 2);
        for bad in [json!({"diameter": 0.5}), json!({"width": 9}), json!({"color": "white"})] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
    }
}
