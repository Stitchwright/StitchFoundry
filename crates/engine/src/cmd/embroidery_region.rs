//! Fill Region: a click inside an area that lines enclose (a CAD drawing of loose lines and arcs,
//! a colouring-book outline) makes that area a fill, as a CAD program's boundary pick does. Gaps
//! up to a set width between the lines are closed, as drawings from CAD often have them. The
//! lines stay as they were; the area is a new closed shape above them, sewn as a fill. Built on
//! Live Paint (its faces of the planar arrangement and its gap detection), run on a copy.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_tools::builder::FACE_NAME;

use super::{CommandSpec, EngineError, NodeId, Point, Result, Session, bad, cmd, has_doc, point_param, str_param};

const CMD: &str = "embroidery.fillRegion";

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.fillRegion",
        "Fill Region",
        [],
        None,
        "{point: [x,y] (inside the area), ids?: the lines and shapes that bound it (else the selection; none selected: everything), gap?: mm (closes gaps between the lines up to this wide; 1; 0 for none, up to 25), type?: fill|satin (fill), color?: \"#rrggbb\" (the thread; else the current fill's colour)} the area the lines enclose round point as a new closed shape above them, sewn as type; the lines stay as they were; one undo step → {id, bounds: [x0,y0,x1,y1]}",
        has_doc,
        fill_region
    )]
}

fn fill_region(s: &mut Session, p: &Value) -> Result<Value> {
    let pt = point_param(p, "point").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "missing point [x,y]"))?;
    let gap = match p.get("gap") {
        None | Some(Value::Null) => 1.0,
        Some(v) => v.as_f64().filter(|g| (0.0..=25.0).contains(g)).ok_or_else(|| bad(CMD, "`gap` is 0 to 25 mm"))?,
    };
    let ty = str_param(p, "type").unwrap_or("fill");
    if !matches!(ty, "fill" | "satin") {
        return Err(bad(CMD, "`type` is fill or satin"));
    }
    let color = match str_param(p, "color") {
        Some(c) => Some(sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?.to_hex()),
        None => None,
    };
    let given = p.get("ids").and_then(Value::as_array).map(|list| list.iter().filter_map(Value::as_u64).collect::<Vec<u64>>());
    let selected: Vec<u64> = s.active().map(|d| d.selection.objects.iter().map(|n| n.0).collect()).unwrap_or_default();
    // the active document's own group: an undo group open in another document stays open
    let ours = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    let mut tries = vec![given.clone().unwrap_or_else(|| selected.clone())];
    // selected lines that don't enclose the click (the line just drawn): every line instead
    if ours.is_some() && given.is_none() && !selected.is_empty() {
        tries.push(Vec::new());
    }
    let mut last = Err(bad(CMD, "there are no lines to fill between"));
    for ids in tries {
        if ours.is_some() {
            s.begin_named_undo_group(Some("Fill Region".into()));
        }
        last = fill(s, ids, pt, gap, ty, color.as_deref());
        if let Some(uid) = ours {
            s.end_document_undo_group(uid, last.is_err());
        }
        if last.is_ok() {
            break;
        }
    }
    let (id, bounds) = last?;
    Ok(json!({ "id": id, "bounds": bounds }))
}

/// The area round `pt` that `ids` (none: everything) enclose, as a new shape sewn as `ty`.
fn fill(s: &mut Session, mut ids: Vec<u64>, pt: Point, gap: f64, ty: &str, color: Option<&str>) -> Result<(u64, Value)> {
    if ids.is_empty() {
        s.execute("select.all", &json!({}))?;
        ids = s.active().map(|d| d.selection.objects.iter().map(|n| n.0).collect()).unwrap_or_default();
        if ids.is_empty() {
            return Err(bad(CMD, "there are no lines to fill between"));
        }
    }
    // a copy made live, only the clicked area painted
    let copies = s.execute("edit.duplicate", &json!({ "ids": ids }))?["ids"].clone();
    s.execute("select.set", &json!({ "ids": copies }))?;
    s.execute("paint.setFill", &json!({ "none": true }))?;
    let live = s.execute("livePaint.make", &json!({ "ids": copies }))?["id"].as_u64().ok_or_else(|| bad(CMD, "the area wasn't found"))?;
    if gap > 0.0 {
        s.execute("livePaint.gapOptions", &json!({ "ids": [live], "detect": true, "size": (gap * PT_PER_MM).min(72.0) }))?;
    }
    let thread = color.map(str::to_string).unwrap_or_else(|| s.paint.fill.color().map_or_else(|| "#2f6fb0".into(), |c| c.to_hex()));
    s.execute("livePaint.fill", &json!({ "group": live, "point": [pt.x, pt.y], "color": thread }))
        .map_err(|_| EngineError::Other("no closed area there: click inside lines that enclose an area (or allow a wider gap)".into()))?;
    s.execute("livePaint.expand", &json!({ "ids": [live] }))?;
    // keep the painted area only; the lines' copies go
    s.edit("Fill Region", |d, _| {
        let n = d.node_mut(NodeId(live)).ok_or(EngineError::NoNode(NodeId(live)))?;
        if let Some(ch) = n.children_mut() {
            ch.retain(|c| c.name.as_deref() == Some(FACE_NAME) && !c.appearance.fill_paint().is_none());
        }
        Ok(())
    })?;
    s.execute("select.set", &json!({ "ids": [live] }))?;
    s.execute("object.ungroup", &json!({}))?;
    let id = s.active().and_then(|d| d.selection.objects.first().map(|n| n.0)).ok_or_else(|| bad(CMD, "the area wasn't made"))?;
    s.execute("select.set", &json!({ "ids": [id] }))?;
    s.execute("paint.setStroke", &json!({ "none": true }))?;
    s.execute("embroidery.set", &json!({ "type": ty }))?;
    s.execute("embroidery.setThread", &json!({ "color": thread, "ids": [id], "paint": false }))?;
    let b = s.doc()?.doc.bounds_of(&[NodeId(id)], false).map(|r| json!([r.x0, r.y0, r.x1, r.y1])).unwrap_or(Value::Null);
    Ok((id, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square of four loose lines, one corner left open by `gap` pt, and a line across it.
    fn lines(s: &mut Session, gap: f64) -> Vec<u64> {
        let d = [
            format!("M100 100 L{} 100", 200.0 - gap),
            "M200 100 L200 200".to_string(),
            "M200 200 L100 200".to_string(),
            "M100 200 L100 100".to_string(),
            "M150 100 L150 200".to_string(),
        ];
        d.iter().map(|d| s.execute("path.create", &json!({ "d": d })).unwrap()["id"].as_u64().unwrap()).collect()
    }

    #[test]
    fn a_click_between_loose_lines_fills_that_area() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let ids = lines(&mut s, 0.0);
        s.execute("select.set", &json!({"ids": ids})).unwrap();
        let r = s.execute(CMD, &json!({"point": [120, 150], "color": "#cc2200"})).unwrap();
        let b: Vec<f64> = r["bounds"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        // the left half only: the line across bounds it
        assert!((b[0] - 100.0).abs() < 1.0 && (b[2] - 150.0).abs() < 1.0 && (b[1] - 100.0).abs() < 1.0 && (b[3] - 200.0).abs() < 1.0, "{b:?}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert!(info["stitches"].as_u64().unwrap() > 50, "{info}");
        // the lines are still there, untouched
        let d = &s.doc().unwrap().doc;
        for i in &ids {
            assert!(d.node(NodeId(*i)).is_some());
        }
        // one undo takes the area away and leaves the lines
        s.execute("edit.undo", &json!({})).unwrap();
        let d = &s.doc().unwrap().doc;
        assert!(d.node(NodeId(r["id"].as_u64().unwrap())).is_none());
        assert!(ids.iter().all(|i| d.node(NodeId(*i)).is_some()));
    }

    #[test]
    fn small_gaps_close_and_wide_ones_say_so() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        // a 1.5 pt gap (about half a millimetre): closed by the default 1 mm
        let ids = lines(&mut s, 1.5);
        let r = s.execute(CMD, &json!({"ids": ids, "point": [180, 150]})).unwrap();
        let b = &r["bounds"];
        assert!((b[0].as_f64().unwrap() - 150.0).abs() < 1.0 && (b[2].as_f64().unwrap() - 200.0).abs() < 1.0, "{r}");
        // with no gap closing, the right half isn't closed
        assert!(s.execute(CMD, &json!({"ids": ids, "point": [180, 150], "gap": 0})).is_err());
        // the line just drawn selected: it encloses nothing, so every line bounds the area
        s.execute("select.set", &json!({"ids": [ids[4]]})).unwrap();
        let r = s.execute(CMD, &json!({"point": [120, 150]})).unwrap();
        assert!((r["bounds"][2].as_f64().unwrap() - 150.0).abs() < 1.0, "{r}");
        // outside every area, and bad settings
        assert!(s.execute(CMD, &json!({"ids": ids, "point": [500, 500]})).is_err());
        for bad in [json!({"point": [1, 1], "gap": 30}), json!({"point": [1, 1], "type": "run"}), json!({})] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
    }
}
