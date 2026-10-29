//! Borders (`embroidery.addBorder`): a filled band round a design's silhouette, as a sticker's or
//! a patch's border is, for a subject cut out of a photo (Remove Background, then Auto-Digitize)
//! or any stitched shapes. The silhouette is grown by the border's width and smoothed (gaps and
//! notches narrower than `smooth` closed, so the band doesn't follow every hair and notch of a
//! photo's edge), filled in the border's thread and sewn first, under the design: the design is
//! cut out of it but for an overlap (so no fabric shows between them), not sewn over twice. A
//! satin (or run) edge round its outside can finish it.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::geom::{Polygon, poly};
use sf_core::model::{FillParams, RunParams, SatinParams};
use sf_core::threads::Rgb;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node, NodeId, NodeKind};
use vectorcraft_embroidery::{PT_PER_MM, node_thread};
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery::stitched_targets;
use super::{CommandSpec, Point, Result, Session, bad, cmd, has_doc, str_param};
use crate::EngineError;

const CMD: &str = "embroidery.addBorder";
/// How far the band reaches under the design (mm), so no fabric shows between them.
const UNDER: f64 = 0.5;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.addBorder",
        "Add Border",
        ["Embroidery"],
        None,
        "{ids?: the stitched shapes or a group of them such as Auto-Digitize's (else the selection), width?: mm the band reaches out from the design (3; 1–30), color?: \"#rrggbb\" (the band's thread; #ffffff), smooth?: mm (2: notches and gaps narrower than this are bridged, the band's outline rounded; 0–15), edge?: satin|run|none (satin: an edge round the band's outside), edgeWidth?: mm (satin, 1.5; 0.8–6), edgeColor?: \"#rrggbb\" (the design's darkest thread)} a filled band round the design's silhouette, as a sticker's or a patch's border, sewn first under the design (which leaves it out where it covers it), and an edge round it; one undo step → {id, edge?: id, widthMm, heightMm}",
        has_doc,
        add_border
    )]
}

/// `color` as a thread colour, or the error naming `key`.
fn colour(p: &Value, key: &str) -> Result<Option<Rgb>> {
    match str_param(p, key) {
        Some(c) => Ok(Some(Rgb::from_hex(c).ok_or_else(|| bad(CMD, format!("`{key}` is \"#rrggbb\"")))?)),
        None => Ok(None),
    }
}

/// `key` as mm in `range`, else `default`.
fn mm(p: &Value, key: &str, default: f64, range: std::ops::RangeInclusive<f64>) -> Result<f64> {
    match p.get(key).filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(v) => v.as_f64().filter(|v| range.contains(v)).ok_or_else(|| bad(CMD, format!("`{key}` is {} to {} mm", range.start(), range.end()))),
    }
}

fn add_border(s: &mut Session, p: &Value) -> Result<Value> {
    let width = mm(p, "width", 3.0, 1.0..=30.0)?;
    let smooth = mm(p, "smooth", 2.0, 0.0..=15.0)?;
    let edge_width = mm(p, "edgeWidth", 1.5, 0.8..=6.0)?;
    let edge = str_param(p, "edge").unwrap_or("satin");
    if !matches!(edge, "satin" | "run" | "none") {
        return Err(bad(CMD, "`edge` is satin, run or none"));
    }
    let band = colour(p, "color")?.unwrap_or(Rgb::new(0xff, 0xff, 0xff));
    let edge_colour = colour(p, "edgeColor")?;
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = d.paint_order(ids.into_iter().filter(|id| d.node(*id).is_some_and(|n| n.embroidery.is_some())));
    let Some(&bottom) = ids.first() else {
        return Err(bad(CMD, "select stitched shapes, or a digitized photo, to put a border round"));
    };
    let areas: Vec<Polygon> = ids.iter().filter_map(|id| d.node(*id)).flat_map(super::embroidery_outline::covered_mm).collect();
    // the silhouette grown by the band and by `smooth`, then shrunk back by `smooth`: gaps and
    // notches narrower than it are bridged; only its outside counts (the design fills the rest)
    let silhouette = poly::union(&areas);
    let grown = poly::offset_round(&poly::offset_round(&silhouette, width + smooth), -smooth);
    let outers: Vec<Vec<sf_core::Point>> = grown.into_iter().map(|g| g.outer).filter(|r| r.len() >= 3).collect();
    if outers.is_empty() {
        return Err(bad(CMD, "the selection has nothing to put a border round"));
    }
    // the band: inside the outline, less the design but for the overlap under its edge
    let whole: Vec<Polygon> = outers.iter().map(|r| Polygon::new(r.clone(), Vec::new())).collect();
    let band_area = poly::difference(&whole, &poly::offset_round(&silhouette, -UNDER));
    let darkest = ids
        .iter()
        .filter_map(|id| d.node(*id))
        .filter_map(|n| n.embroidery.as_deref().map(|e| node_thread(n, e).color))
        .min_by(|a, b| a.to_lab()[0].total_cmp(&b.to_lab()[0]));
    let edge_colour = edge_colour.or(darkest).unwrap_or(Rgb::new(0x1a, 0x1a, 0x1a));
    let to_pt = |r: &[sf_core::Point]| r.iter().map(|q| Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM)).collect::<Vec<_>>();
    let path = PathData::new(outers.iter().map(|r| SubPath::polyline(&to_pt(r), true)).collect());
    let band_path =
        PathData::new(band_area.iter().flat_map(|g| g.rings()).filter(|r| r.len() >= 3).map(|r| SubPath::polyline(&to_pt(r), true)).collect());
    let (x0, y0, x1, y1) =
        outers.iter().flatten().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, q| (b.0.min(q.x), b.1.min(q.y), b.2.max(q.x), b.3.max(q.y)));
    let rgb = |c: Rgb| Color::rgb8(c.r, c.g, c.b);
    let (ids_out, edge_id) = s.edit("Add Border", |d, sel| {
        let (parent, index, _) = d.position(bottom).ok_or(EngineError::NoNode(bottom))?;
        let kind = |path: &PathData, rule| NodeKind::Path { path: path.clone(), rule, live: None, clipping: false, guide: false };
        // the band, under the design (its holes, where the design is, by the even-odd rule)
        let id = d.alloc_id();
        let mut n = Node::new(id, kind(&band_path, vectorcraft_geom::FillRule::EvenOdd));
        n.appearance = Appearance::basic(Paint::solid(rgb(band)), Paint::None, 0.0).into();
        n.name = Some("Border".into());
        let mut e = Embroidery::new(StitchType::Fill(FillParams::default()));
        e.thread = Some(sf_core::Thread::new(band, band.to_hex()));
        n.embroidery = Some(Box::new(e));
        d.insert(parent, index, n)?;
        // its edge, right after it
        let edge_id = if edge == "none" {
            None
        } else {
            let eid = d.alloc_id();
            let mut n = Node::new(eid, kind(&path, vectorcraft_geom::FillRule::NonZero));
            let stroke = if edge == "satin" { edge_width } else { 0.5 };
            n.appearance = Appearance::basic(Paint::None, Paint::solid(rgb(edge_colour)), (stroke * PT_PER_MM).min(4.0)).into();
            n.name = Some("Border Edge".into());
            let mut e =
                Embroidery::new(if edge == "satin" { StitchType::Satin(SatinParams::default()) } else { StitchType::Run(RunParams::default()) });
            e.width = (edge == "satin").then_some(edge_width);
            e.thread = Some(sf_core::Thread::new(edge_colour, edge_colour.to_hex()));
            n.embroidery = Some(Box::new(e));
            d.insert(parent, index + 1, n)?;
            Some(eid)
        };
        sel.set(std::iter::once(id).chain(edge_id));
        Ok((id, edge_id))
    })?;
    Ok(json!({ "id": ids_out.0, "edge": edge_id.map(|e| e.0), "widthMm": x1 - x0, "heightMm": y1 - y0 }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::Cmd;

    #[test]
    fn a_border_is_a_band_round_the_design_sewn_first_with_an_edge() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        // a 40 × 30 mm shape with a notch 1 mm wide cut into its top
        let (x, y) = (30.0 * PT_PER_MM, 30.0 * PT_PER_MM);
        let mm = |v: f64| v * PT_PER_MM;
        let d = format!(
            "M {x} {y} L {} {y} L {} {} L {} {} L {} {y} L {} {y} L {} {} L {x} {} Z",
            x + mm(19.5),
            x + mm(19.5),
            y + mm(12.0),
            x + mm(20.5),
            y + mm(12.0),
            x + mm(20.5),
            x + mm(40.0),
            x + mm(40.0),
            y + mm(30.0),
            y + mm(30.0)
        );
        let shape = s.execute("path.create", &json!({"d": d})).unwrap()["id"].as_u64().unwrap();
        s.execute("select.set", &json!({"ids": [shape]})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": "#7a4b2a"})).unwrap();
        let before = s.execute("embroidery.info", &json!({})).unwrap();
        let r = s.execute(CMD, &json!({"ids": [shape], "width": 4})).unwrap();
        // 4 mm all round: 48 × 38 mm, the notch bridged
        let (w, h) = (r["widthMm"].as_f64().unwrap(), r["heightMm"].as_f64().unwrap());
        assert!((w - 48.0).abs() < 0.6 && (h - 38.0).abs() < 0.6, "{r}");
        let doc = &s.doc().unwrap().doc;
        let band = doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap();
        let NodeKind::Path { path, .. } = &band.kind else { panic!() };
        assert_eq!(path.subpaths.len(), 2, "the outline, the notch bridged, and the design's hole");
        let edge = doc.node(NodeId(r["edge"].as_u64().unwrap())).unwrap();
        let NodeKind::Path { path, .. } = &edge.kind else { panic!() };
        assert_eq!(path.subpaths.len(), 1, "the edge goes round the outside only");
        // sewn first: white band, then the edge, then the design
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        let colours: Vec<&str> = info["colors"].as_array().unwrap().iter().map(|c| c["color"].as_str().unwrap()).collect();
        assert_eq!(colours, ["#ffffff", "#7a4b2a"], "the band, then the edge in the design's darkest thread and the design");
        // the design leaves out the band where it covers it: far fewer stitches than a filled
        // 48 × 38 mm area under a filled 40 × 30 mm one
        let more = info["stitches"].as_u64().unwrap() - before["stitches"].as_u64().unwrap();
        let first_block = s.embroidery_plan().unwrap().pattern().stitches.iter().take_while(|st| st.cmd != Cmd::ColorChange).count() as u64;
        assert!(first_block < before["stitches"].as_u64().unwrap(), "the band ({first_block}) is less than the design it goes round");
        assert!(more > 0);
        // one undo step takes both away
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"], before["stitches"]);
        // no edge; bad settings; nothing stitched
        let r = s.execute(CMD, &json!({"ids": [shape], "edge": "none", "color": "#000000"})).unwrap();
        assert!(r["edge"].is_null());
        for bad in [json!({"ids": [shape], "width": 0.2}), json!({"ids": [shape], "edge": "zigzag"}), json!({"ids": [shape], "color": "white"})] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
        let plain = s.execute("shape.rectangle", &json!({"x": 300, "y": 300, "width": 20, "height": 20})).unwrap()["id"].as_u64().unwrap();
        assert!(s.execute(CMD, &json!({"ids": [plain]})).is_err());
    }
}
