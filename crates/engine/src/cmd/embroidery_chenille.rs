//! Chenille machines (CH-01): objects sewn as chain (link-like loops on top: outlines, borders,
//! light fills) or moss (a dense looped pile: fills in the coil or square-loop pattern), each at a
//! needle height; compound chenille (a moss fill with chain borders sewn before and after it, the
//! borders tying the moss in, as chenille has no bobbin thread); and the stop list a chenille
//! machine's operator works from. Machine files stop wherever the colour, the stitch or the
//! needle height changes, one stop each, as single-stop chenille machines take them: the machine
//! can't tell those stops apart, so the operator sets each from the list.
//!
//! Public descriptions:
//! https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Chenille/chenille_basics/chenille_basics-3.htm
//! https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Chenille/chenille_digitizing/chenille_digitizing-4.htm
//! https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Chenille/chenille_output/Output_to_Tajima_chenille_machines.htm

use serde_json::{Value, json};
use sf_core::attr::StitchType;
use sf_core::engine::ChenilleChange;
use sf_core::geom::{Polygon, poly};
use sf_core::model::{CHENILLE_HEIGHTS, Chenille, ChenilleStitch, FillKind, Motif, RunKind};
use vectorcraft_doc::NodeId;

use super::{CommandSpec, EngineError, Result, Session, bad, cmd, has_doc, has_selection, str_param};

/// Most borders before or after a compound chenille fill.
const MAX_BORDERS: u64 = 3;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.chenille.set",
            "Chenille Stitch",
            [],
            None,
            "{stitch: chain|moss|none, height?: needle height 1 to 9 (chain 3, moss 6), pattern?: coil|square (moss: the loops; coil), ids?} sew the selected stitched objects on a chenille machine: chain keeps their stitch type; moss makes fills coil or square-loop motif fills and runs coil or square-loop motif runs (satin can't be moss); none makes them lockstitch again → {ids}",
            has_selection,
            set
        ),
        cmd!(
            "embroidery.chenille.compound",
            "Compound Chenille…",
            ["Embroidery", "Chenille"],
            None,
            "{before?: chain borders sewn before the moss (1; 0 to 3), after?: sewn after it (1; 0 to 3), offset?: mm between borders (1.5; 0.5 to 10), pattern?: coil|square (coil), height?: the moss's needle height (6), borderHeight?: the borders' (3), ids?} each selected closed shape as compound chenille: chain borders following its outline inward, `offset` apart, before and after a moss fill of it; they take the shape's place in a group → {ids: [group], objects}",
            has_selection,
            compound
        ),
        cmd!(
            query "embroidery.chenille.stops",
            "Chenille Stops",
            ["Embroidery", "Chenille"],
            None,
            "{} the design's chenille stops, in sewing order, each with what the operator sets there (the machine's single stop codes can't tell them apart) → {start: {stitch, height}, stops: [{stop: n, change: color|stitch|height, color?: \"#rrggbb\", name?, stitch?: chain|moss, height?}]}",
            has_doc,
            stops
        ),
    ]
}

fn stitch_param(p: &Value, cmd: &str) -> Result<Option<ChenilleStitch>> {
    match str_param(p, "stitch") {
        Some("chain") => Ok(Some(ChenilleStitch::Chain)),
        Some("moss") => Ok(Some(ChenilleStitch::Moss)),
        Some("none") => Ok(None),
        _ => Err(bad(cmd, "`stitch` is chain, moss or none")),
    }
}

fn height(p: &Value, key: &str, default: u8, cmd: &str) -> Result<u8> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|v| u8::try_from(v).ok())
            .filter(|v| CHENILLE_HEIGHTS.contains(v))
            .ok_or_else(|| bad(cmd, format!("`{key}` is a needle height, 1 to 9"))),
    }
}

fn pattern(p: &Value, cmd: &str) -> Result<Motif> {
    match str_param(p, "pattern") {
        None | Some("coil") => Ok(Motif::Coil),
        Some("square") => Ok(Motif::SquareLoop),
        Some(_) => Err(bad(cmd, "`pattern` is coil or square")),
    }
}

/// Moss on `stitch`: fills become motif fills and runs motif runs in the moss pattern.
fn mossify(stitch: &mut StitchType, motif: Motif) -> std::result::Result<(), &'static str> {
    match stitch {
        StitchType::Fill(f) => {
            if f.kind != FillKind::Motif {
                f.kind = FillKind::Motif;
                f.spacing = 4.0;
                f.stitch_length = 2.0;
                f.motif_size = 4.0;
            }
            f.motif = motif;
            f.custom_motif = None;
            Ok(())
        }
        StitchType::Run(r) => {
            if r.kind != RunKind::Motif {
                r.kind = RunKind::Motif;
                r.width = 3.0;
                r.spacing = 4.0;
            }
            r.motif = motif;
            r.custom_motif = None;
            Ok(())
        }
        _ => Err("moss sews fills and runs: give the satin a fill or run stitch first"),
    }
}

fn set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.chenille.set";
    let stitch = stitch_param(p, CMD)?;
    let h = height(p, "height", if stitch == Some(ChenilleStitch::Moss) { 6 } else { 3 }, CMD)?;
    let motif = pattern(p, CMD)?;
    let ids = super::embroidery::stitched_targets(s, p)?;
    if ids.is_empty() {
        return Err(bad(CMD, "select stitched objects"));
    }
    s.edit("Chenille Stitch", |d, _| {
        for id in &ids {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            let Some(e) = n.embroidery.as_deref_mut() else { continue };
            match stitch {
                Some(ChenilleStitch::Moss) => {
                    mossify(&mut e.stitch, motif).map_err(|m| bad(CMD, m))?;
                    e.chenille = Some(Chenille::new(ChenilleStitch::Moss, h));
                }
                Some(ChenilleStitch::Chain) => e.chenille = Some(Chenille::new(ChenilleStitch::Chain, h)),
                None => e.chenille = None,
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn whole(p: &Value, key: &str, default: u64, max: u64, cmd: &str) -> Result<u64> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_u64().filter(|v| *v <= max).ok_or_else(|| bad(cmd, format!("`{key}` is 0 to {max}"))),
    }
}

/// A path's `d` (points) for `polys` (mm): every ring, holes too.
fn path_d(polys: &[Polygon]) -> String {
    let mut d = String::new();
    for ring in polys.iter().flat_map(Polygon::rings) {
        for (i, q) in ring.iter().enumerate() {
            let q = vectorcraft_embroidery::pt(*q);
            d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, q.x, q.y));
        }
        d.push_str("Z ");
    }
    d
}

fn compound(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.chenille.compound";
    let before = whole(p, "before", 1, MAX_BORDERS, CMD)?;
    let after = whole(p, "after", 1, MAX_BORDERS, CMD)?;
    let offset = match p.get("offset") {
        None | Some(Value::Null) => 1.5,
        Some(v) => v.as_f64().filter(|v| (0.5..=10.0).contains(v)).ok_or_else(|| bad(CMD, "`offset` is 0.5 to 10 mm"))?,
    };
    let motif = pattern(p, CMD)?;
    let moss_h = height(p, "height", 6, CMD)?;
    let border_h = height(p, "borderHeight", 3, CMD)?;
    let d = &s.doc()?.doc;
    let mut shapes: Vec<(NodeId, Vec<Polygon>)> = Vec::new();
    for id in super::embroidery::stitched_targets(s, p)? {
        let Some(node) = d.node(id) else { continue };
        let regions = vectorcraft_embroidery::regions_mm(node);
        if !regions.is_empty() {
            shapes.push((id, regions));
        }
    }
    if shapes.is_empty() {
        return Err(bad(CMD, "select closed shapes to make compound chenille"));
    }
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Compound Chenille".into()));
    }
    let done = (|| -> Result<Value> {
        let mut groups = Vec::new();
        let mut objects = 0usize;
        for (source, regions) in &shapes {
            // the chain borders: the outline inward, offset apart, the first half an offset in
            let rings: Vec<Vec<Polygon>> = (0..before.max(after))
                .map(|k| poly::offset(regions, -(offset * (k as f64 + 0.5))))
                .take_while(|r| r.iter().any(|q| q.area().abs() > 0.5))
                .collect();
            let border = |s: &mut Session, ring: &[Polygon], made: &mut Vec<u64>| -> Result<()> {
                let r = s.execute("path.create", &json!({ "d": path_d(ring) }))?;
                let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "a border wasn't made"))?;
                s.execute("select.set", &json!({ "ids": [id] }))?;
                s.execute("embroidery.set", &json!({ "type": "run" }))?;
                s.execute("embroidery.chenille.set", &json!({ "stitch": "chain", "height": border_h }))?;
                made.push(id);
                Ok(())
            };
            let mut made: Vec<u64> = Vec::new();
            // before the moss: outermost first
            for ring in rings.iter().take(before as usize) {
                border(s, ring, &mut made)?;
            }
            // the moss fill, the whole shape
            let r = s.execute("path.create", &json!({ "d": path_d(regions) }))?;
            let moss = r["id"].as_u64().ok_or_else(|| bad(CMD, "the moss fill wasn't made"))?;
            s.execute("select.set", &json!({ "ids": [moss] }))?;
            s.execute("path.setFillRule", &json!({ "rule": "evenOdd" }))?;
            s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            let pat = if motif == Motif::SquareLoop { "square" } else { "coil" };
            s.execute("embroidery.chenille.set", &json!({ "stitch": "moss", "height": moss_h, "pattern": pat }))?;
            made.push(moss);
            // after the moss: innermost first, working back out to the edge
            for ring in rings.iter().take(after as usize).rev() {
                border(s, ring, &mut made)?;
            }
            // the shape's thread for every part
            if let Some(t) = d_thread(s, *source) {
                s.execute("embroidery.setThread", &json!({ "color": t, "ids": made }))?;
            }
            objects += made.len();
            s.execute("edit.clear", &json!({ "ids": [source.0] }))?;
            s.execute("select.set", &json!({ "ids": made }))?;
            let g = s.execute("object.group", &json!({}))?;
            groups.push(g["id"].clone());
        }
        s.execute("select.set", &json!({ "ids": groups }))?;
        Ok(json!({ "ids": groups, "objects": objects }))
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    done
}

/// The thread colour (`#rrggbb`) node `id` sews in, if it has one.
fn d_thread(s: &Session, id: NodeId) -> Option<String> {
    let d = &s.doc().ok()?.doc;
    let e = d.node(id)?.embroidery.as_deref()?;
    e.thread.as_ref().map(|t| t.color.to_hex())
}

fn stops(s: &mut Session, _: &Value) -> Result<Value> {
    let plan = s.embroidery_plan().ok_or_else(|| bad("embroidery.chenille.stops", "no document"))?;
    let start = plan.design.objects.iter().find_map(|o| o.chenille.filter(|_| o.visible));
    let threads = &plan.pattern().threads;
    let list: Vec<Value> = plan
        .built
        .chenille_stops
        .iter()
        .enumerate()
        .map(|(k, st)| match st.change {
            ChenilleChange::Color(i) => {
                let t = threads.get(i);
                json!({ "stop": k + 1, "change": "color", "color": t.map(|t| t.color.to_hex()), "name": t.map(|t| t.label()) })
            }
            ChenilleChange::Stitch(c) => json!({ "stop": k + 1, "change": "stitch", "stitch": c.id() }),
            ChenilleChange::Height(h) => json!({ "stop": k + 1, "change": "height", "height": h }),
        })
        .collect();
    Ok(json!({
        "start": start.map(|c| json!({ "stitch": "chain", "height": c.height })),
        "stops": list,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::pattern::Cmd;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
        s
    }

    /// CH-01: chain and moss objects sew with a stop wherever the stitch or the needle height
    /// changes, each stop listed with what the operator sets there.
    #[test]
    fn chenille_objects_stop_where_the_stitch_or_the_height_changes() {
        let mut s = session();
        // a chain outline, then a moss fill, then another chain outline, all one colour
        let mut ids = Vec::new();
        for (x, kind) in [(30, "run"), (150, "fill"), (270, "run")] {
            let r = s.execute("shape.rectangle", &json!({"x": x, "y": 60, "width": 90, "height": 90})).unwrap();
            s.execute("embroidery.set", &json!({"type": kind})).unwrap();
            s.execute("embroidery.setThread", &json!({"color": "#7b1fa2"})).unwrap();
            ids.push(r["id"].as_u64().unwrap());
        }
        let stops_of = |s: &Session| s.embroidery_plan().unwrap().pattern().stitches.iter().filter(|r| r.cmd == Cmd::Stop).count();
        assert_eq!(stops_of(&s), 0, "a lockstitch design has no chenille stops");
        s.execute("embroidery.chenille.set", &json!({"stitch": "chain", "ids": [ids[0], ids[2]]})).unwrap();
        s.execute("embroidery.chenille.set", &json!({"stitch": "moss", "ids": [ids[1]], "pattern": "square"})).unwrap();
        // the fill became a square-loop motif fill
        let n = s.doc().unwrap().doc.node(NodeId(ids[1])).unwrap().clone();
        let e = n.embroidery.as_deref().unwrap();
        assert!(matches!(&e.stitch, StitchType::Fill(f) if f.kind == FillKind::Motif && f.motif == Motif::SquareLoop));
        assert_eq!(e.chenille, Some(Chenille::new(ChenilleStitch::Moss, 6)));
        // chain (3) → moss (6): two stops; moss (6) → chain (3): two more
        assert_eq!(stops_of(&s), 4);
        let r = s.execute("embroidery.chenille.stops", &json!({})).unwrap();
        assert_eq!(r["start"], json!({"stitch": "chain", "height": 3}));
        let changes: Vec<(String, Value)> = r["stops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|st| {
                let c = st["change"].as_str().unwrap().to_string();
                let v = if c == "stitch" { st["stitch"].clone() } else { st["height"].clone() };
                (c, v)
            })
            .collect();
        assert_eq!(
            changes,
            vec![
                ("stitch".to_string(), json!("moss")),
                ("height".to_string(), json!(6)),
                ("stitch".to_string(), json!("chain")),
                ("height".to_string(), json!(3)),
            ]
        );
        // a DST of it has the stops (a DST stop reads back as a colour change)
        let out = s.execute("embroidery.export", &json!({"format": "dst"})).unwrap();
        let bytes = vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap();
        let back = sf_formats::read(sf_formats::Format::Dst, &bytes).unwrap();
        let changes = back.stitches.iter().filter(|r| matches!(r.cmd, Cmd::ColorChange | Cmd::Stop)).count();
        assert_eq!(changes, 4, "{changes}");
        // none: lockstitch again, no stops
        s.execute("embroidery.chenille.set", &json!({"stitch": "none", "ids": ids})).unwrap();
        assert_eq!(stops_of(&s), 0);
        assert!(s.execute("embroidery.chenille.stops", &json!({})).unwrap()["start"].is_null());
    }

    /// Compound chenille: chain borders before and after a moss fill, in a group in the shape's
    /// place, in its thread; Undo takes it all back.
    #[test]
    fn compound_chenille_borders_a_moss_fill() {
        let mut s = session();
        let r = s.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 140, "height": 100})).unwrap();
        let id = r["id"].as_u64().unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": "#2e7d32"})).unwrap();
        let r = s.execute("embroidery.chenille.compound", &json!({"before": 2, "after": 1})).unwrap();
        assert_eq!(r["objects"], 4, "{r}");
        assert!(s.doc().unwrap().doc.node(NodeId(id)).is_none(), "the shape gives way to the group");
        let plan = s.embroidery_plan().unwrap();
        let kinds: Vec<Option<ChenilleStitch>> = plan.design.objects.iter().map(|o| o.chenille.map(|c| c.stitch)).collect();
        assert_eq!(kinds, vec![Some(ChenilleStitch::Chain), Some(ChenilleStitch::Chain), Some(ChenilleStitch::Moss), Some(ChenilleStitch::Chain)]);
        assert!(plan.pattern().threads.iter().all(|t| t.color.to_hex() == "#2e7d32"));
        // borders inside the shape: every needle point within its box
        let b = plan.pattern().bounds();
        assert!(b.width() <= 140.0 * vectorcraft_embroidery::MM_PER_PT + 1.0, "{b:?}");
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.node(NodeId(id)).is_some());
    }

    #[test]
    fn bad_chenille_params_are_refused() {
        let mut s = session();
        s.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 80, "height": 80})).unwrap();
        s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
        for p in [
            json!({"stitch": "loop"}),
            json!({"stitch": "chain", "height": 0}),
            json!({"stitch": "chain", "height": 10}),
            json!({"stitch": "moss", "pattern": "zigzag"}),
        ] {
            assert!(s.execute("embroidery.chenille.set", &p).is_err(), "{p}");
        }
        assert!(s.execute("embroidery.chenille.set", &json!({"stitch": "moss"})).is_err(), "satin can't be moss");
        assert!(s.execute("embroidery.chenille.set", &json!({"stitch": "chain"})).is_ok(), "satin can be chain");
        for p in [json!({"before": 4}), json!({"offset": 0.1}), json!({"height": 12})] {
            assert!(s.execute("embroidery.chenille.compound", &p).is_err(), "{p}");
        }
    }
}
