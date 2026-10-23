use serde_json::{Value, json};
use sf_core::threads::Rgb;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> NodeId {
    let r = s.execute("shape.rectangle", &json!({"x": x, "y": y, "width": w, "height": h})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

fn stitches(s: &Session) -> usize {
    s.embroidery_plan().map_or(0, |p| p.pattern().stitch_count())
}

#[test]
fn stitch_type_params_and_undo() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 100.0, 50.0);
    assert_eq!(stitches(&s), 0, "plain art is not stitched");
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let dense = stitches(&s);
    assert!(dense > 500, "{dense}");
    s.execute("embroidery.setParams", &json!({"params": {"spacing": 0.8, "stitchLength": 5.0}})).unwrap();
    let sparse = stitches(&s);
    assert!(sparse < dense, "{sparse} < {dense}");
    let e = s.doc().unwrap().doc.node(a).unwrap().embroidery.clone().unwrap();
    let sf_core::attr::StitchType::Fill(f) = &e.stitch else { panic!("{e:?}") };
    assert!((f.spacing - 0.8).abs() < 1e-9 && (f.stitch_length - 5.0).abs() < 1e-9);
    // bad values change nothing
    assert!(s.execute("embroidery.setParams", &json!({"params": {"spacing": "dense"}})).is_err());
    assert_eq!(stitches(&s), sparse);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(stitches(&s), dense);
    s.execute("embroidery.set", &json!({"type": "none"})).unwrap();
    assert_eq!(stitches(&s), 0);
}

#[test]
fn every_type_and_brush_stitches() {
    for t in ["run", "bean", "satin", "fill", "satinfill", "contour", "spiral", "radial", "cross", "meander", "motiffill", "applique", "manual"] {
        let mut s = session();
        rect(&mut s, 10.0, 10.0, 80.0, 40.0);
        s.execute("embroidery.set", &json!({"type": t})).unwrap();
        assert!(stitches(&s) > 3, "{t}: {}", stitches(&s));
    }
    let pts: Vec<Vec<f64>> = (0..40).map(|i| vec![20.0 + i as f64 * 4.0, 100.0 + (i as f64 * 0.3).sin() * 30.0]).collect();
    for brush in ["run", "bean", "steil", "tapered", "calligraphy", "pressure", "applique", "fill", "satin"] {
        let mut s = session();
        let r = s.execute("embroidery.draw", &json!({"brush": brush, "points": pts, "width": 4.0, "pressures": [0.2, 1.0, 0.5]})).unwrap();
        assert!(r["id"].as_u64().is_some());
        assert!(stitches(&s) > 3, "{brush}: {}", stitches(&s));
    }
    for (stamp, brush) in [("ellipse", "fill"), ("rectangle", "satin"), ("star", "fill"), ("heart", "applique"), ("arrow", "fill"), ("line", "steil")]
    {
        let mut s = session();
        s.execute("embroidery.draw", &json!({"brush": brush, "stamp": stamp, "from": [100, 100], "to": [200, 180]})).unwrap();
        assert!(stitches(&s) > 3, "{stamp}");
    }
}

#[test]
fn sequins_beads_stones_cord_and_boring_are_placed_and_sewn() {
    use sf_core::decor::{DecorKind, DecorLayout};
    use sf_core::model::ObjectKind;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // a line of sequins: 60 mm, 5 mm sequins edge to edge: 12, a stop to load them
    let line = NodeId(
        s.execute("path.create", &json!({"d": format!("M {} {} L {} {}", 10.0 * PT, 10.0 * PT, 70.0 * PT, 10.0 * PT)})).unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    s.execute("embroidery.set", &json!({"type": "sequin"})).unwrap();
    let decor = |s: &Session, kind: DecorKind| -> Vec<sf_core::decor::DecorObject> {
        s.embroidery_plan()
            .unwrap()
            .design
            .objects
            .iter()
            .filter_map(|o| match &o.kind {
                ObjectKind::Decor(d) if d.params.kind == kind => Some(d.clone()),
                _ => None,
            })
            .collect()
    };
    let seq = decor(&s, DecorKind::Sequin);
    assert_eq!(seq.len(), 1);
    assert_eq!(seq[0].items.len(), 12, "{:?}", seq[0].items.len());
    let pat = s.embroidery_plan().unwrap().pattern().clone();
    assert!(pat.stitches.iter().any(|st| st.cmd == sf_core::Cmd::Stop));
    // a fill of 4 mm sequins in a square, honeycombed, flipped every other, a colour of their own
    let sq = rect(&mut s, 20.0 * PT, 30.0 * PT, 40.0 * PT, 40.0 * PT);
    s.execute("embroidery.set", &json!({"type": "sequin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"layout": "fill", "size": 4, "flip": true, "color": "#d4af37"}})).unwrap();
    let fill = decor(&s, DecorKind::Sequin).into_iter().find(|d| d.params.layout == DecorLayout::Fill).unwrap();
    assert!(fill.items.len() >= 80, "{}", fill.items.len());
    assert!(fill.items.iter().any(|i| i.flip) && fill.items.iter().any(|i| !i.flip));
    // rhinestones round the square's edge are not sewn: the stitch count doesn't change
    let before = stitches(&s);
    let st = rect(&mut s, 80.0 * PT, 30.0 * PT, 30.0 * PT, 30.0 * PT);
    s.execute("embroidery.set", &json!({"type": "stone"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"layout": "border"}})).unwrap();
    assert!(!decor(&s, DecorKind::Stone)[0].items.is_empty());
    assert_eq!(stitches(&s), before, "stones aren't sewn");
    // beads along a line, a cord couched along another, a boring hole
    s.execute("path.create", &json!({"d": format!("M {} {} L {} {}", 10.0 * PT, 90.0 * PT, 60.0 * PT, 90.0 * PT)})).unwrap();
    s.execute("embroidery.set", &json!({"type": "bead"})).unwrap();
    s.execute("path.create", &json!({"d": format!("M {} {} C {} {} {} {} {} {}", 10.0 * PT, 110.0 * PT, 40.0 * PT, 90.0 * PT, 60.0 * PT, 130.0 * PT, 90.0 * PT, 110.0 * PT)})).unwrap();
    s.execute("embroidery.set", &json!({"type": "cord"})).unwrap();
    s.execute("shape.ellipse", &json!({"x": 120.0 * PT, "y": 100.0 * PT, "width": 8.0 * PT, "height": 8.0 * PT})).unwrap();
    s.execute("embroidery.set", &json!({"type": "boring"})).unwrap();
    assert!(!decor(&s, DecorKind::Bead)[0].items.is_empty());
    assert_eq!(decor(&s, DecorKind::Cord)[0].lines.len(), 1);
    assert_eq!(decor(&s, DecorKind::Boring)[0].lines.len(), 1);
    // the count for ordering supplies
    let r = s.execute("embroidery.decor.count", &json!({})).unwrap();
    assert!(r["total"].as_u64().unwrap() > 100 && r["cordMm"].as_f64().unwrap() > 80.0, "{r}");
    assert!(r["pieces"].as_array().unwrap().iter().any(|p| p["kind"] == "sequin" && p["color"] == "#d4af37"), "{r}");
    assert!(s.execute("embroidery.decor.palette", &json!({})).unwrap()["sizes"].as_array().unwrap().len() >= 15);
    // moved with its object
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 10.0 * PT, "dy": 0})).unwrap();
    let moved = decor(&s, DecorKind::Sequin).into_iter().find(|d| d.params.layout == DecorLayout::Run).unwrap();
    assert!((moved.items[0].at.x - 22.5).abs() < 0.1, "{:?}", moved.items[0].at);
    let _ = (sq, st);
    // machine files keep the sequins' and beads' stitches
    let dst = s.execute("embroidery.export", &json!({"format": "dst"})).unwrap();
    assert!(dst["stitches"].as_u64().unwrap() > 300);
}

#[test]
fn an_opened_machine_files_stitches_become_runs_satins_and_fills() {
    use sf_core::attr::StitchType;
    let mut s = session();
    // a tatami square, a satin bar and a run, each its own thread
    rect(&mut s, 50.0, 50.0, 150.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#1d3557"})).unwrap();
    rect(&mut s, 260.0, 60.0, 260.0, 22.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    s.execute("path.create", &json!({"d": "M 260 200 C 330 140 420 260 520 200"})).unwrap();
    s.execute("embroidery.set", &json!({"type": "run"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#2a9d8f"})).unwrap();
    let before = stitches(&s);
    // VP3 keeps the threads' colours
    let vp3 = s.execute("embroidery.export", &json!({"format": "vp3"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s.execute("file.place", &json!({"name": "design.vp3", "dataBase64": vp3})).unwrap();
    s.execute("select.none", &json!({})).unwrap();
    // with nothing selected: every manual stitch object
    let r = s.execute("embroidery.stitchesToObjects", &json!({})).unwrap();
    assert!(r["fills"].as_u64().unwrap() >= 1 && r["satins"].as_u64().unwrap() >= 1 && r["runs"].as_u64().unwrap() >= 1, "{r}");
    let d = &s.doc().unwrap().doc;
    let mut kinds = Vec::new();
    d.walk(|n| {
        if let Some(e) = n.embroidery.as_deref() {
            let c = e.thread.as_ref().map(|t| t.color.to_hex()).unwrap_or_default();
            kinds.push((c, std::mem::discriminant(&e.stitch)));
        }
    });
    let has = |c: &str, st: StitchType| kinds.iter().any(|(k, d)| k == c && *d == std::mem::discriminant(&st));
    assert!(has("#1d3557", StitchType::Fill(Default::default())), "the square is a fill in its thread: {kinds:?}");
    assert!(has("#d62828", StitchType::Satin(Default::default())), "the bar is a satin: {kinds:?}");
    assert!(has("#2a9d8f", StitchType::Run(Default::default())), "the line is a run: {kinds:?}");
    assert!(!kinds.iter().any(|(_, d)| *d == std::mem::discriminant(&StitchType::Manual)), "no manual stitches left");
    // sewn again much as they were
    let after = stitches(&s);
    assert!(after * 2 > before && after < before * 2, "{before} → {after}");
    // one undo step back
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.execute("embroidery.stitchesToObjects", &json!({"ids": [999_999]})).is_err());
}

/// NEW-28: a texture on a turning satin fill is pressed in along the turning.
#[test]
fn a_texture_embosses_a_turning_satin_fill() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 200.0, 40.0);
    s.execute("embroidery.set", &json!({"type": "satinfill"})).unwrap();
    s.execute("embroidery.addGuide", &json!({"id": a.0, "a": [150, 90], "b": [150, 150]})).unwrap();
    s.execute("embroidery.addGuide", &json!({"id": a.0, "a": [250, 90], "b": [280, 150]})).unwrap();
    let plain = stitches(&s);
    s.execute("embroidery.setParams", &json!({"params": {"texture": {"pattern": "diamonds", "size": 4}}})).unwrap();
    let embossed = stitches(&s);
    assert!(embossed > plain + 100, "{plain} → {embossed}");
    s.execute("embroidery.setParams", &json!({"params": {"texture": null}})).unwrap();
    assert_eq!(stitches(&s), plain);
}

#[test]
fn inclination_lines_turn_and_follow_the_object() {
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 200.0, 40.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    // made satin: its lines placed at once, editable (DIR-02), in the same undo step
    let placed = s.doc().unwrap().doc.node(a).unwrap().embroidery.clone().unwrap();
    assert!(placed.guides.len() >= 2 && !placed.auto_direction, "{placed:?}");
    s.execute("embroidery.clearGuides", &json!({})).unwrap();
    s.execute("embroidery.addGuide", &json!({"id": a.0, "a": [150, 90], "b": [150, 150]})).unwrap();
    s.execute("embroidery.addGuide", &json!({"id": a.0, "a": [250, 90], "b": [260, 150]})).unwrap();
    let n = s.doc().unwrap().doc.node(a).unwrap().embroidery.clone().unwrap();
    assert_eq!(n.guides.len(), 2);
    assert!(!n.auto_direction);
    s.execute("embroidery.moveGuide", &json!({"id": a.0, "index": 1, "b": [240, 150]})).unwrap();
    // moving the object moves its guides
    s.execute("object.move", &json!({"dx": 10, "dy": 20})).unwrap();
    let g = s.doc().unwrap().doc.node(a).unwrap().embroidery.clone().unwrap().guides[0];
    assert!((g.a.x - 160.0).abs() < 1e-6 && (g.a.y - 110.0).abs() < 1e-6, "{g:?}");
    s.execute("embroidery.removeGuide", &json!({"id": a.0, "index": 0})).unwrap();
    assert!(s.execute("embroidery.removeGuide", &json!({"id": a.0, "index": 5})).is_err());
    s.execute("embroidery.clearGuides", &json!({})).unwrap();
    let e = s.doc().unwrap().doc.node(a).unwrap().embroidery.clone().unwrap();
    assert!(e.guides.is_empty() && e.auto_direction);
}

#[test]
fn auto_direction_places_editable_guides() {
    let mut s = session();
    // a long thin bar → turning guides; a square → an angle
    let bar = rect(&mut s, 50.0, 50.0, 300.0, 20.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let sq = rect(&mut s, 50.0, 200.0, 100.0, 100.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.autoDirection", &json!({"ids": [bar.0, sq.0]})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert!(d.node(bar).unwrap().embroidery.as_ref().unwrap().guides.len() >= 2);
    let e = d.node(sq).unwrap().embroidery.clone().unwrap();
    assert!(e.guides.is_empty() && !e.auto_direction);
}

#[test]
fn convert_thread_beads_settings_info_and_sorting() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 100.0, 50.0);
    let b = rect(&mut s, 200.0, 10.0, 100.0, 50.0);
    let c = rect(&mut s, 400.0, 10.0, 100.0, 50.0);
    s.execute("edit.selectAll", &json!({})).ok();
    s.execute("embroidery.convert", &json!({"ids": [a.0, b.0, c.0]})).unwrap();
    assert!(stitches(&s) > 1000);
    s.execute("embroidery.setThread", &json!({"ids": [a.0, c.0], "color": "#d62828", "name": "Red"})).unwrap();
    s.execute("embroidery.setThread", &json!({"ids": [b.0], "color": "#1a3e8c"})).unwrap();
    let before = s.embroidery_plan().unwrap().pattern().count(sf_core::Cmd::ColorChange);
    assert_eq!(before, 2);
    s.execute("embroidery.sortByColor", &json!({})).unwrap();
    let after = s.embroidery_plan().unwrap().pattern().count(sf_core::Cmd::ColorChange);
    assert_eq!(after, 1, "red objects sewn together");
    s.execute("embroidery.setBeads", &json!({"id": a.0, "entry": [10, 10], "exit": null})).unwrap();
    assert!(s.doc().unwrap().doc.node(a).unwrap().embroidery.as_ref().unwrap().entry.is_some());
    s.execute("embroidery.settings", &json!({"hoop": "Brother 130 × 180 (5×7)", "trimThreshold": 3.0})).unwrap();
    s.execute("embroidery.applyFabric", &json!({"name": "Towel / terry"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert!(info["stitches"].as_u64().unwrap() > 1000);
    assert_eq!(info["colorChanges"], 1);
    assert_eq!(info["fabric"], "Towel / terry");
    assert!(info["fits"].as_bool().is_some());
    let plan = s.execute("embroidery.plan", &json!({"limit": 10})).unwrap();
    assert_eq!(plan["stitches"].as_array().unwrap().len(), 10);
    assert!(s.execute("embroidery.settings", &json!({"hoop": "nope"})).is_err());
}

/// A placed image 100 × 60 mm: a black block on the left, a red one on the right, a grey ramp
/// between them, on white.
fn placed_photo(s: &mut Session) -> u64 {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let r = vectorcraft_trace::Raster::from_fn(200, 120, |x, y| {
        if (10..60).contains(&x) && (10..110).contains(&y) {
            [0, 0, 0, 255]
        } else if (140..190).contains(&x) && (10..110).contains(&y) {
            [210, 20, 20, 255]
        } else if (70..130).contains(&x) && (40..80).contains(&y) {
            let v = (255 - (x - 70) * 4) as u8;
            [v, v, v, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    let png = r.encode_png();
    let r = s
        .execute(
            "file.place",
            &json!({"name": "photo.png", "dataBase64": vectorcraft_format::base64_encode(&png), "rect": [0, 0, 100.0 * PT, 60.0 * PT]}),
        )
        .unwrap();
    let id = r["id"].as_u64().or_else(|| r["ids"][0].as_u64()).unwrap();
    s.execute("select.set", &json!({"ids": [id]})).unwrap();
    id
}

#[test]
fn an_images_colours_match_threads_and_it_is_prepared_to_digitize() {
    let mut s = session();
    let img = placed_photo(&mut s);
    let r = s.execute("embroidery.image.colors", &json!({"colors": 3})).unwrap();
    let colors = r["colors"].as_array().unwrap();
    assert_eq!(colors.len(), 3, "{r}");
    // white covers most of it; black and red are there, each with a thread from the chart
    let white = sf_core::Rgb::new(255, 255, 255);
    assert!(sf_core::Rgb::from_hex(colors[0]["color"].as_str().unwrap()).unwrap().delta_e(white) < 12.0, "{r}");
    let near = |hex: &str| {
        colors.iter().any(|c| sf_core::Rgb::from_hex(c["color"].as_str().unwrap()).unwrap().delta_e(sf_core::Rgb::from_hex(hex).unwrap()) < 12.0)
    };
    assert!(near("#000000") && near("#d21414"), "{r}");
    assert!(colors.iter().all(|c| c["thread"]["color"].is_string()), "{r}");
    // prepared: a copy over it, three colours, selected
    let r = s.execute("embroidery.image.prepare", &json!({"colors": 3})).unwrap();
    let id = r["id"].as_u64().unwrap();
    assert_ne!(id, img);
    assert_eq!(r["colors"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(s.doc().unwrap().selection.objects.iter().map(|i| i.0).collect::<Vec<_>>(), vec![id]);
    let r = s.execute("embroidery.image.colors", &json!({"colors": 6})).unwrap();
    assert!(r["colors"].as_array().unwrap().len() <= 3, "the prepared copy has three colours: {r}");
    // replacing the original
    s.execute("select.set", &json!({"ids": [img]})).unwrap();
    s.execute("embroidery.image.prepare", &json!({"colors": 0, "smooth": 2, "replace": true})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(img)).is_none());
    assert!(s.execute("embroidery.image.prepare", &json!({"colors": 1})).is_err());
    s.execute("select.none", &json!({})).unwrap();
    assert!(s.execute("embroidery.image.colors", &json!({})).is_err(), "no image");
}

#[test]
fn a_photo_is_stitched_as_lines_coral_or_colours() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let img = placed_photo(&mut s);
    // lines: one line of needle points, zigzagging wide over the black, straight over the white
    let r = s.execute("embroidery.photoStitch", &json!({"mode": "lines", "spacing": 2})).unwrap();
    assert_eq!(r["ids"].as_array().unwrap().len(), 1, "{r}");
    let points_in = |s: &Session, x0: f64, x1: f64| {
        let mut n = 0;
        let plan = s.embroidery_plan().unwrap();
        for st in &plan.pattern().stitches {
            if st.cmd == sf_core::Cmd::Stitch && st.x >= x0 && st.x < x1 {
                n += 1;
            }
        }
        n
    };
    // the black block (5–30 mm) holds far more needle points than as much white (30–35 mm)
    let (black, white) = (points_in(&s, 10.0, 25.0), points_in(&s, 30.5, 34.5) * 15 / 4);
    assert!(black > 2 * white.max(1), "{black} {white}");
    assert!(s.embroidery_plan().unwrap().pattern().threads.iter().any(|t| t.color.to_hex() == "#000000"));
    // coral: points crowd in the dark
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [img]})).unwrap();
    let r = s.execute("embroidery.photoStitch", &json!({"mode": "coral", "density": 2, "color": "#1d3557"})).unwrap();
    assert!(r["points"].as_u64().unwrap() > 1000, "{r}");
    let (black, white) = (points_in(&s, 10.0, 25.0), points_in(&s, 30.5, 34.5) * 15 / 4);
    assert!(black > 5 * white.max(1), "{black} {white}");
    // colour: fills of the main colours, the lightest first, in the chart's threads
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [img]})).unwrap();
    let r = s.execute("embroidery.photoStitch", &json!({"mode": "color", "colors": 3, "cell": 1.5})).unwrap();
    let ids: Vec<u64> = r["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert!(ids.len() >= 2, "{r}");
    let d = &s.doc().unwrap().doc;
    let lightness = |id: u64| {
        let n = d.node(NodeId(id)).unwrap();
        let t = n.embroidery.as_ref().unwrap().thread.clone().unwrap();
        assert!(matches!(n.embroidery.as_ref().unwrap().stitch, sf_core::attr::StitchType::Fill(_)));
        t.color.to_lab()[0]
    };
    assert!(lightness(ids[0]) >= lightness(*ids.last().unwrap()), "lightest first");
    let _ = PT;
    // bad requests
    s.execute("select.set", &json!({"ids": [img]})).unwrap();
    for bad in [json!({"mode": "mosaic"}), json!({"spacing": 0.1}), json!({"mode": "color", "cell": 0.01}), json!({"color": "black"})] {
        assert!(s.execute("embroidery.photoStitch", &bad).is_err(), "{bad}");
    }
}

/// HV-01: sepia and black and white sew a photo by lightness in browns or greys; colour tile sews
/// it as small tiles, each one thread colour.
#[test]
fn a_photo_is_stitched_in_sepia_greys_or_colour_tiles() {
    let mut s = session();
    let img = placed_photo(&mut s);
    let threads = |s: &Session, ids: &[u64]| -> Vec<sf_core::Rgb> {
        let d = &s.doc().unwrap().doc;
        ids.iter().map(|id| d.node(NodeId(*id)).unwrap().embroidery.as_ref().unwrap().thread.clone().unwrap().color).collect()
    };
    let ids_of = |r: &Value| -> Vec<u64> { r["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect() };
    // sepia: browns (red at least green at least blue), the lightest first, the red block brown
    let r = s.execute("embroidery.photoStitch", &json!({"mode": "sepia", "cell": 1.5})).unwrap();
    let ids = ids_of(&r);
    assert!(ids.len() >= 2, "{r}");
    let t = threads(&s, &ids);
    for c in &t {
        let lab = c.to_lab();
        assert!(lab[2] >= -2.0 && lab[1].abs() < 30.0, "a brown: {}", c.to_hex());
    }
    assert!(t[0].to_lab()[0] >= t.last().unwrap().to_lab()[0], "lightest first");
    // black and white: greys only
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [img]})).unwrap();
    let r = s.execute("embroidery.photoStitch", &json!({"mode": "mono", "cell": 1.5, "colors": 5})).unwrap();
    for c in threads(&s, &ids_of(&r)) {
        let lab = c.to_lab();
        assert!(lab[1].abs() < 12.0 && lab[2].abs() < 12.0, "a grey: {}", c.to_hex());
    }
    // colour tile: every shape gives tiles, one fill a colour, the red block's tiles red
    for shape in ["square", "diamond", "hexagon"] {
        s.execute("edit.undo", &json!({})).unwrap();
        s.execute("select.set", &json!({"ids": [img]})).unwrap();
        let r = s.execute("embroidery.photoStitch", &json!({"mode": "tile", "shape": shape, "size": 3, "colors": 4})).unwrap();
        let ids = ids_of(&r);
        assert!(r["tiles"].as_u64().unwrap() > 50, "{shape}: {r}");
        assert!(threads(&s, &ids).iter().any(|c| c.r > c.g.saturating_add(80)), "{shape}: a red thread");
        let d = &s.doc().unwrap().doc;
        let NodeKind::Path { path, .. } = &d.node(NodeId(ids[0])).unwrap().kind else { panic!("a path") };
        let subpaths = path.to_bezpath().elements().iter().filter(|e| matches!(e, kurbo::PathEl::MoveTo(_))).count();
        assert!(subpaths > 1, "{shape}: one path holds that colour's tiles");
        assert!(s.embroidery_plan().unwrap().pattern().stitches.len() > 100);
    }
    s.execute("select.set", &json!({"ids": [img]})).unwrap();
    for bad in [
        json!({"mode": "tile", "shape": "star"}),
        json!({"mode": "tile", "size": 0.5}),
        json!({"mode": "sepia", "colors": 1}),
        json!({"mode": "tile", "gap": 3}),
    ] {
        assert!(s.execute("embroidery.photoStitch", &bad).is_err(), "{bad}");
    }
}

/// PC-05: a lasso drawn loosely round a red disc on white closes on the disc's edge, in its colour.
#[test]
fn the_magnetic_lasso_snaps_to_the_edge_round_a_disc() {
    use sf_core::attr::StitchType;
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(200, 200, |x, y| {
        let (dx, dy) = (f64::from(x) - 100.0, f64::from(y) - 100.0);
        if dx.hypot(dy) <= 50.0 { [200, 30, 30, 255] } else { [250, 250, 250, 255] }
    });
    let png = r.encode_png();
    s.execute("file.place", &json!({"name": "disc.png", "dataBase64": vectorcraft_format::base64_encode(&png)})).unwrap();
    let xf = {
        let st = s.doc().unwrap();
        let mut xf = None;
        st.doc.walk(|n| {
            if let NodeKind::Image(im) = &n.kind {
                xf = Some(im.xf);
            }
        });
        xf.unwrap()
    };
    let spot = |x: f64, y: f64| {
        let p = xf * kurbo::Point::new(x, y);
        json!([p.x, p.y])
    };
    // eight points a few pixels off the disc's edge, in and out
    let pts: Vec<Value> = (0..8)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::FRAC_PI_4;
            let r = if i % 2 == 0 { 54.0 } else { 47.0 };
            spot(100.0 + r * a.cos(), 100.0 + r * a.sin())
        })
        .collect();
    let res = s.execute("embroidery.magneticLasso", &json!({"points": pts})).unwrap();
    assert_eq!(res["color"], "#c81e1e");
    let id = NodeId(res["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    let n = st.doc.node(id).unwrap();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Fill(_))));
    assert_eq!(st.selection.objects, vec![id]);
    // every point of the border is on the disc's edge, within two pixels
    let NodeKind::Path { path, .. } = &n.kind else { panic!() };
    let inv = xf.inverse();
    let bez = path.to_bezpath();
    let mut far = 0.0f64;
    for el in bez.elements() {
        if let Some(p) = el.end_point() {
            let q = inv * p;
            far = far.max(((q.x - 100.0).hypot(q.y - 100.0) - 50.0).abs());
        }
    }
    assert!(far <= 2.0, "the border strays {far} px from the edge");
    // a plain path; bad requests
    let res = s.execute("embroidery.magneticLasso", &json!({"points": pts, "as": "none"})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(res["id"].as_u64().unwrap())).unwrap().embroidery.is_none());
    for bad in [
        json!({"points": [spot(0.0, 0.0), spot(1.0, 1.0)]}),
        json!({"points": pts, "width": 0}),
        json!({"points": pts, "contrast": 101}),
        json!({"points": pts, "as": "cross"}),
        json!({"points": pts, "guide": "x"}),
        json!({"points": [[1.0, "a"], [2, 2], [3, 3]]}),
    ] {
        assert!(s.execute("embroidery.magneticLasso", &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_click_on_an_image_stitches_its_region() {
    use sf_core::attr::StitchType;
    let mut s = session();
    // a red ring and a blue bar on white, and a blue square apart from the bar
    let r = vectorcraft_trace::Raster::from_fn(200, 120, |x, y| {
        if (20..100).contains(&x) && (20..100).contains(&y) && !((50..70).contains(&x) && (50..70).contains(&y)) {
            [210, 20, 20, 255]
        } else if ((120..190).contains(&x) && (56..64).contains(&y)) || ((150..160).contains(&x) && (95..105).contains(&y)) {
            [20, 20, 200, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    let png = r.encode_png();
    s.execute("file.place", &json!({"name": "art.png", "dataBase64": vectorcraft_format::base64_encode(&png)})).unwrap();
    let xf = {
        let st = s.doc().unwrap();
        let mut xf = None;
        st.doc.walk(|n| {
            if let NodeKind::Image(im) = &n.kind {
                xf = Some(im.xf);
            }
        });
        xf.unwrap()
    };
    let spot = |x: f64, y: f64| {
        let p = xf * kurbo::Point::new(x, y);
        json!([p.x, p.y])
    };
    // the ring as tatami, its hole kept
    let res = s.execute("embroidery.clickStitch", &json!({"at": spot(25.5, 25.5)})).unwrap();
    assert_eq!((res["as"].as_str(), res["pixels"].as_u64(), res["color"].as_str()), (Some("fill"), Some(80 * 80 - 400), Some("#d21414")));
    let id = NodeId(res["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    let n = st.doc.node(id).unwrap();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Fill(_))));
    let NodeKind::Path { path, .. } = &n.kind else { panic!() };
    assert_eq!(path.subpaths.len(), 2);
    assert_eq!(st.selection.objects, vec![id]);
    // without the hole; the bar as a running centre line; satin
    let res = s.execute("embroidery.clickStitch", &json!({"at": spot(25.5, 25.5), "as": "fillNoHoles"})).unwrap();
    let NodeKind::Path { path, .. } = &s.doc().unwrap().doc.node(NodeId(res["id"].as_u64().unwrap())).unwrap().kind.clone() else { panic!() };
    assert_eq!(path.subpaths.len(), 1);
    let res = s.execute("embroidery.clickStitch", &json!({"at": spot(150.5, 60.5), "as": "centerline"})).unwrap();
    let n = s.doc().unwrap().doc.node(NodeId(res["id"].as_u64().unwrap())).unwrap().clone();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Run(_))));
    let NodeKind::Path { path, .. } = &n.kind else { panic!() };
    assert_eq!(path.subpaths.len(), 1);
    assert!(s.execute("embroidery.clickStitch", &json!({"at": spot(150.5, 60.5), "as": "satin"})).is_ok());
    assert!(stitches(&s) > 500, "{}", stitches(&s));
    // one undo step each
    let before = s.doc().unwrap().doc.layers[0].children().unwrap().len();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.layers[0].children().unwrap().len(), before - 1);
    // off the image, a bad kind or tolerance
    assert!(s.execute("embroidery.clickStitch", &json!({"at": spot(-50.0, -50.0)})).is_err());
    assert!(s.execute("embroidery.clickStitch", &json!({"at": spot(25.5, 25.5), "as": "cross"})).is_err());
    assert!(s.execute("embroidery.clickStitch", &json!({"at": spot(25.5, 25.5), "tolerance": 300})).is_err());
    // PC-04: with contiguous off, the click takes every patch of the bar's colour, not only the bar
    let joined = s.execute("embroidery.clickStitch", &json!({"at": spot(150.5, 60.5)})).unwrap()["pixels"].as_u64().unwrap();
    let all = s.execute("embroidery.clickStitch", &json!({"at": spot(150.5, 60.5), "contiguous": false})).unwrap()["pixels"].as_u64().unwrap();
    assert!(all > joined, "{all} pixels of its colour, {joined} joined to the spot");
}

#[test]
fn designs_go_to_the_machine_folder() {
    let dir = std::env::temp_dir().join(format!("sf-send-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let folder = dir.to_string_lossy().to_string();
    let mut s = session();
    // no folder yet; nothing stitched yet
    assert!(s.execute("embroidery.send", &json!({})).is_err());
    assert!(s.execute("embroidery.send", &json!({"folder": folder})).is_err());
    let sq = rect(&mut s, 100.0, 100.0, 60.0, 40.0);
    s.execute("select.set", &json!({"ids": [sq.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.send", &json!({"folder": folder, "name": "Cap logo"})).unwrap();
    let first = r["path"].as_str().unwrap().to_string();
    assert!(first.ends_with("Cap logo.dst") && std::fs::metadata(&first).unwrap().len() > 100, "{first}");
    // the folder remembered: sent there again, under a new name, or over the old one
    let r = s.execute("embroidery.send", &json!({"name": "Cap logo"})).unwrap();
    assert!(r["path"].as_str().unwrap().ends_with("Cap logo-2.dst"), "{r}");
    let r = s.execute("embroidery.send", &json!({"name": "Cap logo", "replace": true, "format": "pes"})).unwrap();
    assert!(r["path"].as_str().unwrap().ends_with("Cap logo.pes"), "{r}");
    let t = s.execute("embroidery.send.targets", &json!({})).unwrap();
    assert_eq!(t["targets"], json!([{"folder": folder, "ready": true}]));
    // a folder that isn't there (a USB stick taken out): said so, nothing written
    let gone = dir.join("gone").to_string_lossy().to_string();
    s.execute("embroidery.send.targets", &json!({"add": gone})).unwrap();
    assert!(s.execute("embroidery.send", &json!({})).is_err());
    let t = s.execute("embroidery.send.targets", &json!({"forget": gone})).unwrap();
    assert_eq!(t["targets"].as_array().unwrap().len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn partial_cover_leaves_out_edges_sewn_over_later() {
    let mut s = session();
    let patch = rect(&mut s, 100.0, 100.0, 100.0, 100.0);
    s.execute("select.set", &json!({"ids": [patch.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "applique"})).unwrap();
    let whole = stitches(&s);
    // a fill sewn later over the patch's right edge
    let over = rect(&mut s, 170.0, 80.0, 60.0, 140.0);
    s.execute("select.set", &json!({"ids": [over.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let with_fill = stitches(&s);
    s.execute("select.set", &json!({"ids": [patch.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"partial": true}})).unwrap();
    let partial = stitches(&s);
    assert!(partial < with_fill && partial > with_fill - whole, "{whole} {with_fill} {partial}");
}

#[test]
fn an_outline_goes_round_the_art_as_it_sews() {
    use sf_core::attr::StitchType;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // two squares 5 mm apart: one outline round both, 2 mm off
    let a = rect(&mut s, 100.0, 100.0, 30.0, 30.0);
    let b = rect(&mut s, 130.0 + 5.0 * PT, 100.0, 30.0, 30.0);
    for id in [a, b] {
        s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    }
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let r = s.execute("embroidery.outlineAround", &json!({"gap": 2, "kind": "run"})).unwrap();
    assert_eq!(r["pieces"].as_u64(), Some(1));
    let id = NodeId(r["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    let n = st.doc.node(id).unwrap();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Run(_))));
    let o = n.geometric_bounds().unwrap();
    let art = st.doc.node(a).unwrap().geometric_bounds().unwrap();
    assert!((art.x0 - o.x0 - 2.0 * PT).abs() < 0.5 && (art.y0 - o.y0 - 2.0 * PT).abs() < 0.5, "{o:?} {art:?}");
    assert_eq!(st.selection.objects, vec![id]);
    // satin: its inner edge 2 mm off, the path half its width further
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    let r = s.execute("embroidery.outlineAround", &json!({"width": 4, "color": "#ff0000"})).unwrap();
    let st = s.doc().unwrap();
    let n = st.doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap();
    let e = n.embroidery.as_deref().unwrap();
    assert!(matches!(e.stitch, StitchType::Satin(_)) && e.width == Some(4.0) && e.thread.as_ref().unwrap().color == Rgb::new(255, 0, 0));
    let o = n.geometric_bounds().unwrap();
    assert!((art.x0 - o.x0 - 4.0 * PT).abs() < 0.5, "{o:?}");
    // nothing stitched selected; values out of range
    let plain = rect(&mut s, 300.0, 300.0, 10.0, 10.0);
    s.execute("select.set", &json!({"ids": [plain.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "none"})).unwrap();
    assert!(s.execute("embroidery.outlineAround", &json!({})).is_err());
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    for bad in
        [json!({"gap": -1}), json!({"width": 50}), json!({"kind": "zigzag"}), json!({"color": "red"}), json!({"kind": "emboss", "shape": "star"})]
    {
        assert!(s.execute("embroidery.outlineAround", &bad).is_err(), "{bad}");
    }
    // knockdown: a light fill of the area, sewn first, under the art
    let r = s.execute("embroidery.outlineAround", &json!({"kind": "knockdown", "gap": 1})).unwrap();
    let k = NodeId(r["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    assert_eq!(st.doc.layers[0].children().unwrap()[0].id, k);
    let n = st.doc.node(k).unwrap();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Fill(f)) if f.spacing > 1.0));
    assert!((art.x0 - n.geometric_bounds().unwrap().x0 - 1.0 * PT).abs() < 0.5);
    // emboss: a circle round it, the art's area left out
    let r = s.execute("embroidery.outlineAround", &json!({"kind": "emboss", "shape": "circle", "extend": 5})).unwrap();
    let st = s.doc().unwrap();
    let n = st.doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap();
    let NodeKind::Path { path, .. } = &n.kind else { panic!() };
    assert_eq!(path.subpaths.len(), 2);
    let e = n.geometric_bounds().unwrap();
    assert!(e.width() > art.width() + 10.0 * PT, "{e:?}");
}

#[test]
fn monograms_stack_wear_ornaments_and_ring_borders() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let kids = |s: &Session, id: NodeId| -> Vec<(String, vectorcraft_geom::Rect)> {
        s.doc()
            .unwrap()
            .doc
            .node(id)
            .unwrap()
            .children()
            .unwrap()
            .iter()
            .map(|n| (n.name.clone().unwrap_or_default(), n.geometric_bounds().unwrap()))
            .collect()
    };
    // stacked: one above the other, centred on the same line
    let r = s.execute("embroidery.monogram", &json!({"letters": "AB", "font": "Test Script", "style": "stacked", "at": [300, 300]})).unwrap();
    let k = kids(&s, NodeId(r["id"].as_u64().unwrap()));
    assert!((k[0].1.center().x - k[1].1.center().x).abs() < 30.0 && k[0].1.y1 < k[1].1.y1, "{k:?}");
    // ornaments either side, three oval rings round it all
    let r = s
        .execute(
            "embroidery.monogram",
            &json!({"letters": "AB", "font": "Test Script", "border": "oval", "rings": 3, "ornament": "hearts", "at": [300, 300]}),
        )
        .unwrap();
    let k = kids(&s, NodeId(r["id"].as_u64().unwrap()));
    let names: Vec<&str> = k.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names.iter().filter(|n| **n == "Ornament").count(), 2, "{names:?}");
    assert_eq!(names.iter().filter(|n| n.starts_with("Border")).count(), 3, "{names:?}");
    let ornaments: Vec<_> = k.iter().filter(|(n, _)| n == "Ornament").map(|(_, b)| *b).collect();
    let letters = k[0].1.union(k[1].1);
    assert!(ornaments[0].x1 < letters.x0 && ornaments[1].x0 > letters.x1, "{ornaments:?} {letters:?}");
    // every border kind and ornament makes a monogram
    for border in ["octagon", "heart", "shield"] {
        assert!(s.execute("embroidery.monogram", &json!({"letters": "A", "font": "Test Script", "border": border})).is_ok(), "{border}");
    }
    for ornament in ["stars", "dots", "swirls"] {
        assert!(s.execute("embroidery.monogram", &json!({"letters": "A", "font": "Test Script", "ornament": ornament})).is_ok(), "{ornament}");
    }
    // what it doesn't know
    for bad in [json!({"rings": 5}), json!({"ornament": "skulls"}), json!({"style": "spiral"})] {
        let mut p = json!({"letters": "A", "font": "Test Script"});
        p.as_object_mut().unwrap().extend(bad.as_object().unwrap().clone());
        assert!(s.execute("embroidery.monogram", &p).is_err(), "{bad}");
    }
}

#[test]
fn copies_scatter_over_a_shape_a_gap_apart() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let area = rect(&mut s, 0.0, 0.0, 100.0 * PT, 80.0 * PT);
    let motif = rect(&mut s, 200.0 * PT, 0.0, 6.0 * PT, 6.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("select.set", &json!({"ids": [area.0, motif.0]})).unwrap();
    let r = s.execute("embroidery.scatter", &json!({"count": 12, "gap": 3})).unwrap();
    assert_eq!(r["copies"].as_u64(), Some(12), "{r}");
    let centres = |s: &Session, id: NodeId| -> Vec<vectorcraft_geom::Point> {
        let st = s.doc().unwrap();
        st.doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap().center()).collect()
    };
    let id = NodeId(r["id"].as_u64().unwrap());
    let cs = centres(&s, id);
    // inside the shape, the motif's half-diagonal in from its edge; a gap apart
    let half = 3.0 * 2f64.sqrt() * PT;
    for (i, a) in cs.iter().enumerate() {
        assert!(a.x >= half - 1e-6 && a.x <= 100.0 * PT - half + 1e-6 && a.y >= half - 1e-6 && a.y <= 80.0 * PT - half + 1e-6, "{a:?}");
        for b in &cs[i + 1..] {
            assert!(a.distance(*b) >= 2.0 * half + 3.0 * PT - 1e-6, "{a:?} {b:?}");
        }
    }
    // the copies keep the motif's stitching; the same seed scatters the same way
    let st = s.doc().unwrap();
    let first = st.doc.node(id).unwrap().children().unwrap()[0].clone();
    assert!(first.children().unwrap()[0].embroidery.is_some());
    s.execute("select.set", &json!({"ids": [area.0, motif.0]})).unwrap();
    let again = s.execute("embroidery.scatter", &json!({"count": 12, "gap": 3})).unwrap();
    assert_eq!(centres(&s, NodeId(again["id"].as_u64().unwrap())), cs);
    // evenly: a grid as full as it fits
    s.execute("select.set", &json!({"ids": [area.0, motif.0]})).unwrap();
    let r = s.execute("embroidery.scatter", &json!({"count": 500, "spread": "even", "gap": 2, "turn": true})).unwrap();
    assert!(r["copies"].as_u64().unwrap() > 30, "{r}");
    // what can't be done
    s.execute("select.set", &json!({"ids": [motif.0]})).unwrap();
    assert!(s.execute("embroidery.scatter", &json!({})).is_err(), "one object");
    s.execute("select.set", &json!({"ids": [area.0, motif.0]})).unwrap();
    for bad in [json!({"gap": 101}), json!({"spread": "grid"}), json!({"count": 0}), json!({"count": 9999})] {
        assert!(s.execute("embroidery.scatter", &bad).is_err(), "{bad}");
    }
}

#[test]
fn placement_templates_print_life_size() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 80.0 * PT, 60.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.worksheet", &json!({"kind": "template", "placement": "left-chest", "barcode": "none"})).unwrap();
    assert_eq!(r["scale"].as_f64(), Some(100.0), "life size: {r}");
    let mut texts = Vec::new();
    let mut lines = Vec::new();
    s.doc().unwrap().doc.walk(|n| match &n.kind {
        NodeKind::Text(t) => texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>()),
        NodeKind::Path { path, .. } => lines.push((path.subpaths.len(), n.geometric_bounds().unwrap())),
        _ => {}
    });
    for want in ["Placement template", "Up", "Left chest: usually 75–100 mm wide"] {
        assert!(texts.iter().any(|t| t.starts_with(want)), "{want} missing from {texts:?}");
    }
    // the 100 mm bar: three strokes, 100 mm wide
    assert!(lines.iter().any(|(k, b)| *k == 3 && (b.width() - 100.0 * PT).abs() < 1e-6), "{lines:?}");
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    // on a production sheet too; a placement it doesn't know
    s.execute("embroidery.worksheet", &json!({"placement": "cap-front", "barcode": "none"})).unwrap();
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    assert!(s.execute("embroidery.worksheet", &json!({"placement": "hat"})).is_err());
    // larger than the page at life size: no template
    rect(&mut s, 0.0, 0.0, 400.0 * PT, 300.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert!(s.execute("embroidery.worksheet", &json!({"kind": "template"})).is_err());
}

#[test]
fn fabrics_recommend_a_stabiliser() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 30.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.settings", &json!({"fabric": "Fleece / sweatshirt"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    let f = info["fabricDetails"].as_array().unwrap().iter().find(|f| f["name"] == "Fleece / sweatshirt").unwrap().clone();
    assert_eq!((f["stabilizer"].as_str(), f["advice"].as_str()), (Some("cut-away"), Some("Cut-away, with a water-soluble topping")));
    // every built-in fabric says one
    assert!(info["fabricDetails"].as_array().unwrap().iter().all(|f| f["stabilizer"].is_string()));
    // on the worksheet
    s.execute("embroidery.worksheet", &json!({"barcode": "none"})).unwrap();
    let mut texts = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t == "Stabiliser: Cut-away, with a water-soluble topping"), "{texts:?}");
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    // a fabric of the design's own: its stabiliser set
    s.execute("embroidery.fabric.save", &json!({"name": "Chiffon", "from": "Silk / lightweight", "stabilizer": "wash-away", "topping": false}))
        .unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    let f = info["fabricDetails"].as_array().unwrap().iter().find(|f| f["name"] == "Chiffon").unwrap().clone();
    assert_eq!(f["advice"].as_str(), Some("Wash-away"));
    assert!(s.execute("embroidery.fabric.save", &json!({"name": "Chiffon", "stabilizer": "glue"})).is_err());
}

#[test]
fn designs_start_and_end_where_asked() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 30.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.settings", &json!({"start": "top-left", "end": "center"})).unwrap();
    assert_eq!((r["settings"]["start"].as_str(), r["settings"]["end"].as_str()), (Some("top-left"), Some("centre")), "{r}");
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["settings"]["end"].as_str(), Some("centre"));
    let plan = s.embroidery_plan().unwrap();
    let p = plan.pattern();
    let b = p.bounds();
    let last = p.stitches.iter().rev().find(|s| s.is_move()).unwrap();
    let c = sf_core::Point::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
    assert!(last.pos().distance(c) < 0.5, "{last:?} {b:?}");
    let first = p.stitches.iter().find(|s| s.cmd == sf_core::Cmd::Stitch).unwrap().pos();
    assert!(first.distance(b.min) < first.distance(b.max), "{first:?}");
    s.execute("embroidery.settings", &json!({"start": null, "end": "none"})).unwrap();
    assert!(s.execute("embroidery.info", &json!({})).unwrap()["settings"]["end"].is_null());
    for bad in [json!({"start": "middle"}), json!({"end": 3})] {
        assert!(s.execute("embroidery.settings", &bad).is_err(), "{bad}");
    }
}

#[test]
fn an_object_in_the_same_colour_can_be_a_block_of_its_own() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 30.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    let b = rect(&mut s, 60.0 * PT, 20.0 * PT, 30.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    let blocks = |s: &Session| s.embroidery_plan().unwrap().pattern().color_blocks().len();
    assert_eq!(blocks(&s), 1);
    s.execute("embroidery.setParams", &json!({"ids": [b.0], "params": {"ownBlock": true}})).unwrap();
    assert_eq!(blocks(&s), 2);
    // kept in the file: two colours of the same thread
    let r = s.execute("embroidery.export", &json!({"format": "dst"})).unwrap();
    assert_eq!(r["colors"].as_u64(), Some(2));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(blocks(&s), 1);
}

#[test]
fn designs_start_from_templates_built_in_and_ones_own() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = Session::new();
    let list = s.execute("embroidery.templates", &json!({})).unwrap();
    assert!(list["templates"].as_array().unwrap().len() >= 7);
    // a round patch: its page, its hoop and its satin border sewn already
    s.execute("embroidery.template.new", &json!({"name": "round patch 75 mm"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["hoop"].as_str(), Some("Brother 100 × 100 (4×4)"));
    assert!(info["stitches"].as_u64().unwrap() > 100 && (info["widthMm"].as_f64().unwrap() - 75.0).abs() < 2.0, "{info}");
    let ab = s.doc().unwrap().doc.artboards[0].rect;
    assert!((ab.width() / PT - 110.0).abs() < 0.5);
    // the area is a guide, not stitched
    let mut guides = 0;
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Path { guide: true, .. } = n.kind {
            guides += 1;
        }
    });
    assert_eq!(guides, 1);
    // a plain one: a guide and nothing sewn
    s.execute("embroidery.template.new", &json!({"name": "Cap front"})).unwrap();
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["hoop"].as_str(), Some("Cap frame 130 × 60"));
    // one's own: kept, opened again as it was
    rect(&mut s, 30.0 * PT, 30.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let stitches = s.embroidery_plan().unwrap().pattern().stitch_count();
    s.execute("embroidery.template.save", &json!({"name": "Club cap"})).unwrap();
    let docs = s.documents().len();
    s.execute("embroidery.template.new", &json!({"name": "club CAP"})).unwrap();
    assert_eq!(s.documents().len(), docs + 1);
    assert_eq!(s.embroidery_plan().unwrap().pattern().stitch_count(), stitches);
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["hoop"].as_str(), Some("Cap frame 130 × 60"));
    assert!(s.execute("embroidery.template.save", &json!({"name": "Full back"})).is_err());
    assert!(s.execute("embroidery.template.new", &json!({"name": "Nothing"})).is_err());
    s.execute("embroidery.template.delete", &json!({"name": "Club cap"})).unwrap();
    assert!(s.execute("embroidery.template.delete", &json!({"name": "Club cap"})).is_err());
}

#[test]
fn thread_charts_of_ones_own_come_from_csv_and_the_design() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#cc3300"})).unwrap();
    // a supplier's list, then used like the built-in charts
    let csv = "Number,Description,Hex\n501,Flame,#ff4400\n502,Deep Sea,#003366\n";
    let r = s.execute("embroidery.chart.import", &json!({"name": "Shop stock", "text": csv})).unwrap();
    assert_eq!(r["threads"].as_u64(), Some(2));
    let list = s.execute("embroidery.charts", &json!({"name": "Shop stock"})).unwrap();
    assert!(list["charts"].as_array().unwrap().iter().any(|c| c["name"] == "Shop stock" && c["custom"] == true), "{list}");
    assert_eq!(list["threads"][1]["code"].as_str(), Some("502"));
    s.execute("embroidery.matchThreads", &json!({"chart": "Shop stock", "all": true})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["colors"][0]["color"].as_str(), Some("#ff4400"), "{info}");
    assert!(info["charts"].as_array().unwrap().iter().any(|c| c == "Shop stock"));
    s.execute("embroidery.recolor", &json!({"scheme": "invert", "chart": "Shop stock"})).unwrap();
    // written out as CSV, read back the same
    let out = s.execute("embroidery.chart.export", &json!({"name": "Shop stock"})).unwrap();
    let text = out["text"].as_str().unwrap();
    assert!(text.starts_with("code,name,color\n501,Flame,#ff4400"), "{text}");
    s.execute("embroidery.chart.import", &json!({"name": "Again", "text": text})).unwrap();
    let again = s.execute("embroidery.charts", &json!({"name": "Again"})).unwrap();
    assert_eq!(again["threads"], list["threads"]);
    // the built-in ones export too, and a chart of the design's own threads
    assert!(s.execute("embroidery.chart.export", &json!({"name": "Generic"})).unwrap()["threads"].as_u64().unwrap() > 10);
    let r = s.execute("embroidery.chart.save", &json!({"name": "This job"})).unwrap();
    assert_eq!(r["threads"].as_u64(), Some(1));
    s.execute("embroidery.chart.save", &json!({"name": "Two", "threads": [{"color": "#000000", "name": "Black"}, {"color": "#ffffff"}]})).unwrap();
    assert_eq!(s.prefs.embroidery_thread_charts.len(), 4);
    for (cmd, bad) in [
        ("embroidery.chart.import", json!({"name": "Generic", "text": csv})),
        ("embroidery.chart.import", json!({"name": "X", "text": "a,b\n1,2"})),
        ("embroidery.chart.import", json!({"name": "X"})),
        ("embroidery.chart.save", json!({"name": "X", "threads": []})),
        ("embroidery.chart.save", json!({"name": "X", "threads": [{"color": "red"}]})),
        ("embroidery.chart.export", json!({"name": "Nowhere"})),
        ("embroidery.chart.delete", json!({"name": "Generic"})),
    ] {
        assert!(s.execute(cmd, &bad).is_err(), "{cmd} {bad}");
    }
    s.execute("embroidery.chart.delete", &json!({"name": "Two"})).unwrap();
    assert_eq!(s.prefs.embroidery_thread_charts.len(), 3);
}

#[test]
fn a_garment_photo_goes_behind_the_design_life_size_and_unstitched() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let png = std::env::temp_dir().join(format!("sf-garment-{}.png", std::process::id()));
    let mut photo = session();
    rect(&mut photo, 0.0, 0.0, 400.0, 300.0);
    photo.execute("document.export", &json!({"path": png.to_string_lossy(), "format": "png"})).unwrap();
    let mut s = session();
    rect(&mut s, 40.0 * PT, 40.0 * PT, 60.0 * PT, 30.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let before = s.embroidery_plan().unwrap().pattern().stitch_count();
    let r = s.execute("embroidery.background", &json!({"path": png.to_string_lossy(), "width": 400})).unwrap();
    assert_eq!(r["widthMm"].as_f64(), Some(400.0));
    let id = NodeId(r["ids"][0].as_u64().unwrap());
    let d = &s.doc().unwrap().doc;
    // on a template layer, its middle on the design's, 400 mm across
    let layer = d.layers.iter().find(|l| matches!(l.kind, NodeKind::Layer { template: true, .. })).expect("a template layer");
    assert!(layer.children().is_some_and(|c| c.iter().any(|n| n.id == id)));
    let b = d.node(id).unwrap().bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false).unwrap();
    assert!((b.width() / PT - 400.0).abs() < 1.0, "{b:?}");
    assert!(((b.x0 + b.x1) / 2.0 / PT - 70.0).abs() < 2.0 && ((b.y0 + b.y1) / 2.0 / PT - 55.0).abs() < 2.0, "{b:?}");
    // never stitched
    assert_eq!(s.embroidery_plan().unwrap().pattern().stitch_count(), before);
    for bad in [
        json!({}),
        json!({"path": png.to_string_lossy(), "width": 10}),
        json!({"path": "/nowhere.png"}),
        json!({"path": png.to_string_lossy(), "at": [1]}),
    ] {
        assert!(s.execute("embroidery.background", &bad).is_err(), "{bad}");
    }
    let _ = std::fs::remove_file(&png);
}

#[test]
fn the_product_visualizer_shows_the_design_life_size_on_the_product() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 120.0 * PT, 40.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    // a T-shirt, left chest: the design wider than usual there is warned of
    let r = s.execute("embroidery.visualize", &json!({})).unwrap();
    assert_eq!((r["product"].as_str(), r["placement"].as_str(), r["view"].as_str()), (Some("tshirt"), Some("left-chest"), Some("front")), "{r}");
    assert_eq!(r["warnings"].as_array().unwrap().len(), 1, "{r}");
    assert!((r["widthMm"].as_f64().unwrap() - 120.0).abs() < 2.0);
    let stitches_in = |s: &Session, red: vectorcraft_color::Color| {
        let mut b = kurbo::Rect::ZERO;
        let mut found = false;
        s.doc().unwrap().doc.walk(|n| {
            if let NodeKind::Path { path, .. } = &n.kind
                && n.appearance
                    .items
                    .iter()
                    .any(|i| matches!(i, vectorcraft_doc::AppearanceItem::Stroke(st) if st.paint == vectorcraft_color::Paint::solid(red)))
            {
                let r = kurbo::Shape::bounding_box(&path.to_bezpath());
                b = if found { b.union(r) } else { r };
                found = true;
            }
        });
        b
    };
    let red = vectorcraft_color::Color::rgb8(0xd6, 0x28, 0x28);
    let st = stitches_in(&s, red);
    // life size, its middle where a left-chest design goes (500, 220 mm on the shirt, 40 mm margin)
    assert!((st.width() / PT - 120.0).abs() < 3.0, "{}", st.width() / PT);
    assert!((st.center().x / PT - 540.0).abs() < 3.0 && (st.center().y / PT - 260.0).abs() < 3.0, "{:?}", st.center());
    assert!(s.doc().unwrap().doc.title.ends_with("on T-shirt"));
    // 200 mm from the shirt's top, the neckline at 80
    assert!((r["belowNeckMm"].as_f64().unwrap() - 120.0).abs() < 1.0, "{r}");
    // the usual work areas of the front: chests, full front, sleeve (no pocket), dashed
    s.set_active(0);
    let r = s.execute("embroidery.visualize", &json!({"areas": true})).unwrap();
    assert_eq!(r["areas"], 4, "{r}");
    let mut dashed = 0;
    s.doc().unwrap().doc.walk(|n| {
        if n.appearance.stroke().is_some_and(|st| st.dash.is_some()) {
            dashed += 1;
        }
    });
    assert_eq!(dashed, 4);
    s.set_active(0);
    let r = s.execute("embroidery.visualize", &json!({"product": "hoodie", "placement": "full-back", "areas": true})).unwrap();
    assert_eq!(r["areas"], 2, "the back: full back and yoke");
    // full front is wide enough; the back of a navy polo for the full back
    s.set_active(0);
    let r = s.execute("embroidery.visualize", &json!({"placement": "full-front"})).unwrap();
    assert!(r["warnings"].as_array().unwrap().is_empty());
    s.set_active(0);
    let r = s.execute("embroidery.visualize", &json!({"product": "polo", "placement": "full-back", "color": "#102040"})).unwrap();
    assert_eq!(r["view"].as_str(), Some("back"));
    for product in ["hoodie", "cap", "snapback", "trucker", "dadcap", "tote", "towel"] {
        s.set_active(0);
        let r = s.execute("embroidery.visualize", &json!({"product": product, "areas": true})).unwrap();
        assert_eq!(r["product"].as_str(), Some(product));
        assert_eq!(r["belowNeckMm"].is_null(), product != "hoodie", "{r}");
        assert!(r["areas"].as_u64().unwrap() >= 1);
    }
    // a photo of the customer's own: the design on it, the photo under it
    let png = std::env::temp_dir().join(format!("sf-mockup-{}.png", std::process::id()));
    let mut photo = session();
    rect(&mut photo, 0.0, 0.0, 300.0, 200.0);
    photo.execute("document.export", &json!({"path": png.to_string_lossy(), "format": "png"})).unwrap();
    s.set_active(0);
    let docs = s.documents().len();
    let r = s.execute("embroidery.visualize", &json!({"image": png.to_string_lossy(), "imageWidth": 400, "at": [200, 150]})).unwrap();
    assert_eq!((r["product"].as_str(), s.documents().len()), (Some("photo"), docs + 1), "{r}");
    let st = stitches_in(&s, red);
    assert!((st.center().x / PT - 200.0).abs() < 3.0 && (st.center().y / PT - 150.0).abs() < 3.0, "{:?}", st.center());
    let d = &s.doc().unwrap().doc;
    let first = d.layers.first().and_then(|l| l.children().and_then(|c| c.first().cloned())).unwrap();
    assert!(matches!(first.kind, NodeKind::Image(_) | NodeKind::Group { .. }), "the photo is at the back");
    let _ = std::fs::remove_file(&png);
    s.set_active(0);
    for bad in [
        json!({"product": "mug"}),
        json!({"placement": "cap-front"}),
        json!({"product": "cap", "placement": "left-chest"}),
        json!({"color": "red"}),
        json!({"image": "/nowhere/photo.png"}),
        json!({"image": "/nowhere/photo.png", "imageWidth": 5}),
    ] {
        assert!(s.execute("embroidery.visualize", &bad).is_err(), "{bad}");
    }
}

#[test]
fn near_colours_merge_into_one_thread_and_their_blocks_join() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // red, blue, a red a shade off, apart from each other: three colour blocks
    for (k, color) in ["#d62828", "#1d3557", "#d72a29"].iter().enumerate() {
        rect(&mut s, (10.0 + 30.0 * k as f64) * PT, 10.0 * PT, 20.0 * PT, 20.0 * PT);
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": color, "name": format!("thread {k}")})).unwrap();
    }
    let blocks = |s: &Session| s.embroidery_plan().unwrap().pattern().color_blocks().len();
    assert_eq!(blocks(&s), 3);
    // only the very same colour: nothing to merge
    let r = s.execute("embroidery.colors.merge", &json!({"tolerance": 0})).unwrap();
    assert_eq!(r["merged"], json!([]), "{r}");
    let r = s.execute("embroidery.colors.merge", &json!({})).unwrap();
    assert_eq!(r["merged"], json!([{"from": "#d72a29", "to": "#d62828"}]), "{r}");
    assert_eq!((r["threads"].clone(), r["blocks"].clone()), (json!([3, 2]), json!([3, 2])), "{r}");
    assert_eq!(blocks(&s), 2);
    // the red sewn first kept its name
    let names: Vec<String> = s.embroidery_plan().unwrap().pattern().threads.iter().map(|t| t.name.clone()).collect();
    assert_eq!(names.iter().filter(|n| *n == "thread 0").count(), 1, "{names:?}");
    // one undo step
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(blocks(&s), 3);
    assert!(s.execute("embroidery.colors.merge", &json!({"tolerance": 99})).is_err());
}

#[test]
fn the_threads_recolour_by_a_map_or_a_scheme_in_one_undo() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let a = rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#ff0000"})).unwrap();
    let b = rect(&mut s, 20.0 * PT, 60.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#0000ff"})).unwrap();
    s.execute("select.none", &json!({})).unwrap();
    let threads =
        |s: &Session| -> Vec<String> { s.embroidery_plan().unwrap().stats.clone().unwrap().colors.iter().map(|c| c.thread.color.to_hex()).collect() };
    assert_eq!(threads(&s), ["#ff0000", "#0000ff"]);
    // a map: red becomes green, blue stays
    let r = s.execute("embroidery.recolor", &json!({"map": [{"from": "#FF0000", "to": "#00ff00"}]})).unwrap();
    assert_eq!(r["changed"].as_u64(), Some(1));
    assert_eq!(threads(&s), ["#00ff00", "#0000ff"]);
    // the art repainted to match
    let fill = s.doc().unwrap().doc.node(a).unwrap().appearance.fill_paint();
    assert_eq!(fill, vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(0, 255, 0)));
    // grey, one undo step for all of them
    s.execute("embroidery.recolor", &json!({"scheme": "grayscale"})).unwrap();
    let grey = threads(&s);
    assert!(grey.iter().all(|c| c[1..3] == c[3..5] && c[3..5] == c[5..7]), "{grey:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(threads(&s), ["#00ff00", "#0000ff"]);
    // the hue turned, matched to a chart: chart threads, with names
    let r = s.execute("embroidery.recolor", &json!({"scheme": "hue", "degrees": 120, "chart": "Generic"})).unwrap();
    assert_eq!(r["colors"].as_array().unwrap().len(), 2, "{r}");
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert!(info["colors"].as_array().unwrap().iter().all(|c| !c["name"].as_str().unwrap().is_empty()), "{info}");
    // only the selection
    s.execute("select.set", &json!({"ids": [b.0]})).unwrap();
    s.execute("embroidery.recolor", &json!({"scheme": "mono", "color": "#ffcc00"})).unwrap();
    let after = threads(&s);
    assert_eq!(after.len(), 2);
    for bad in [
        json!({}),
        json!({"scheme": "sepia"}),
        json!({"scheme": "mono"}),
        json!({"map": [{"from": "red", "to": "#000000"}]}),
        json!({"scheme": "hue", "degrees": 999}),
        json!({"map": [{"from": "#123456", "to": "#000000"}], "all": true}),
        json!({"scheme": "invert", "chart": "Nowhere"}),
    ] {
        assert!(s.execute("embroidery.recolor", &bad).is_err(), "{bad}");
    }
}

#[test]
fn the_smallest_hoop_is_chosen_turned_and_ones_own_kept() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // 170 mm wide, 110 high: a 5×7 hoop holds it turned
    rect(&mut s, 20.0 * PT, 20.0 * PT, 170.0 * PT, 110.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.hoop.auto", &json!({"brand": "brother"})).unwrap();
    assert_eq!((r["hoop"].as_str(), r["rotated"].as_bool()), (Some("Brother 130 × 180 (5×7)"), Some(true)), "{r}");
    assert_eq!((r["width"].as_f64(), r["height"].as_f64()), (Some(180.0), Some(130.0)));
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!((info["fits"].as_bool(), info["hoopSize"][0].as_f64()), (Some(true), Some(180.0)), "{info}");
    // not turned: the next size up that holds it as it is
    let r = s.execute("embroidery.hoop.auto", &json!({"brand": "brother", "turn": false})).unwrap();
    assert_eq!(r["hoop"].as_str(), Some("Brother 200 × 200 (8×8)"));
    // turned back by hand
    s.execute("embroidery.hoop.auto", &json!({"brand": "brother"})).unwrap();
    let r = s.execute("embroidery.hoop.rotate", &json!({})).unwrap();
    assert_eq!((r["width"].as_f64(), r["rotated"].as_bool()), (Some(130.0), Some(false)));
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["fits"].as_bool(), Some(false));
    // magnetic frames are hoops too; any brand: the smallest field of all
    let r = s.execute("embroidery.hoop.auto", &json!({"brand": "magnetic"})).unwrap();
    assert!(r["hoop"].as_str().unwrap().starts_with("Magnetic frame"), "{r}");
    // one's own, smaller than any standard one that fits, then chosen
    s.execute("embroidery.hoop.saveCustom", &json!({"name": "Shop frame", "width": 175, "height": 115})).unwrap();
    let r = s.execute("embroidery.hoop.auto", &json!({})).unwrap();
    assert_eq!(r["hoop"].as_str(), Some("Shop frame"));
    let list = s.execute("embroidery.hoops", &json!({})).unwrap();
    let own = list["hoops"].as_array().unwrap().iter().find(|h| h["name"] == "Shop frame").unwrap();
    assert_eq!((own["custom"].as_bool(), own["fits"].as_bool()), (Some(true), Some(true)));
    s.execute("embroidery.settings", &json!({"hoop": "Shop frame"})).unwrap();
    assert!(s.execute("embroidery.hoop.saveCustom", &json!({"name": "Jacket back 360 × 360", "width": 300, "height": 300})).is_err());
    assert!(s.execute("embroidery.hoop.saveCustom", &json!({"name": "Tiny", "width": 5, "height": 300})).is_err());
    s.execute("embroidery.hoop.deleteCustom", &json!({"name": "shop FRAME"})).unwrap();
    assert!(s.execute("embroidery.hoop.deleteCustom", &json!({"name": "Shop frame"})).is_err());
    // too big for any hoop
    rect(&mut s, 0.0, 0.0, 900.0 * PT, 900.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert!(s.execute("embroidery.hoop.auto", &json!({})).is_err());
    assert!(s.execute("embroidery.hoop.auto", &json!({"special": true})).is_err());
}

#[test]
fn worksheets_carry_the_shop_and_leave_out_what_is_hidden() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let texts = |s: &Session| {
        let mut v = Vec::new();
        s.doc().unwrap().doc.walk(|n| {
            if let NodeKind::Text(t) = &n.kind {
                v.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
            }
        });
        v
    };
    let png = std::env::temp_dir().join(format!("sf-logo-{}.png", std::process::id()));
    let mut logo = session();
    rect(&mut logo, 0.0, 0.0, 200.0, 80.0);
    logo.execute("document.export", &json!({"path": png.to_string_lossy(), "format": "png"})).unwrap();
    s.execute(
        "embroidery.worksheet.setup",
        &json!({"company": "Stitch & Co", "details": "1 High St · 01234 567890", "logo": png.to_string_lossy(), "hide": ["colors", "barcode"]}),
    )
    .unwrap();
    s.execute("embroidery.worksheet", &json!({})).unwrap();
    let t = texts(&s);
    assert!(t.iter().any(|x| x.contains("Stitch & Co") && x.contains("High St")), "{t:?}");
    assert!(!t.iter().any(|x| x == "Colours in sewing order"), "{t:?}");
    let mut images = 0;
    s.doc().unwrap().doc.walk(|n| {
        if matches!(n.kind, NodeKind::Image(_)) {
            images += 1;
        }
    });
    assert_eq!(images, 1, "the logo");
    // asked for now: shown again
    s.set_active(0);
    s.execute("embroidery.worksheet", &json!({"hide": []})).unwrap();
    assert!(texts(&s).iter().any(|x| x == "Colours in sewing order"));
    let r = s.execute("embroidery.worksheet.setup", &json!({})).unwrap();
    assert_eq!(r["hide"], json!(["colors", "barcode"]));
    assert!(s.execute("embroidery.worksheet.setup", &json!({"hide": ["everything"]})).is_err());
    let _ = std::fs::remove_file(&png);
}

#[test]
fn a_multi_position_hoop_sews_a_file_a_position_without_marks() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // three 100 × 40 mm bands down a 280 mm length
    for i in 0..3 {
        rect(&mut s, 20.0 * PT, (20.0 + 100.0 * f64::from(i)) * PT, 100.0 * PT, 40.0 * PT);
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    }
    let dir = std::env::temp_dir().join(format!("sf-multipos-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = s.execute("embroidery.multiHoop", &json!({"hoop": "Multi-position 130 × 300 (3 × 130 × 180)", "dir": dir.to_string_lossy()})).unwrap();
    assert_eq!((r["columns"].as_u64(), r["rows"].as_u64()), (Some(1), Some(3)), "{r}");
    for h in r["hoopings"].as_array().unwrap() {
        let bytes = std::fs::read(h["file"].as_str().unwrap()).unwrap();
        let p = sf_formats::read(sf_formats::Format::Dst, &bytes).unwrap();
        let b = p.bounds();
        assert!(b.min.x >= -65.1 && b.max.x <= 65.1 && b.min.y >= -90.1 && b.max.y <= 90.1, "{b:?}");
        assert_eq!(p.color_blocks().len(), 1, "no registration marks: the fabric isn't hooped again");
    }
    let list = s.execute("embroidery.hoops", &json!({})).unwrap();
    assert!(list["hoops"].as_array().unwrap().iter().any(|h| h["name"].as_str().unwrap().starts_with("Multi-position")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_object_larger_than_a_hooping_is_split_over_hoopings() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // a 300 × 40 mm band: no 130 × 180 hooping holds it
    rect(&mut s, 10.0 * PT, 20.0 * PT, 300.0 * PT, 40.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let whole = s.embroidery_plan().unwrap().pattern().stitch_count();
    assert!(s.execute("embroidery.multiHoop", &json!({"hoop": "Brother 130 × 180 (5×7)", "split": false})).is_err());
    let dir = std::env::temp_dir().join(format!("sf-split-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = s.execute("embroidery.multiHoop", &json!({"hoop": "Brother 130 × 180 (5×7)", "dir": dir.to_string_lossy(), "marks": false})).unwrap();
    let hoopings = r["hoopings"].as_array().unwrap();
    assert!(hoopings.len() >= 3 && hoopings.iter().all(|h| h["split"].as_u64() == Some(1)), "{r}");
    // every piece inside its hoop; together about the whole (plus ties)
    let mut total = 0;
    for h in hoopings {
        let bytes = std::fs::read(h["file"].as_str().unwrap()).unwrap();
        let p = sf_formats::read(sf_formats::Format::Dst, &bytes).unwrap();
        let b = p.bounds();
        assert!(b.min.x >= -65.1 && b.max.x <= 65.1 && b.min.y >= -90.1 && b.max.y <= 90.1, "{b:?}");
        total += p.stitch_count();
    }
    assert!(total >= whole && total < whole + whole / 4, "{total} vs {whole}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_design_sits_at_a_fixed_place_in_the_hoop() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.settings", &json!({"hoop": "Brother 100 × 100 (4×4)"})).unwrap();
    let centre_of = |s: &mut Session| {
        let r = s.execute("embroidery.export", &json!({"format": "dst"})).unwrap();
        let bytes = vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap();
        let b = sf_formats::read(sf_formats::Format::Dst, &bytes).unwrap().bounds();
        ((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0)
    };
    let (x, y) = centre_of(&mut s);
    assert!(x.abs() < 0.5 && y.abs() < 0.5, "centred: {x} {y}");
    // top left of the hoop
    s.execute("embroidery.settings", &json!({"hoopPosition": [-20, -30]})).unwrap();
    let (x, y) = centre_of(&mut s);
    assert!((x + 20.0).abs() < 0.5 && (y + 30.0).abs() < 0.5, "placed: {x} {y}");
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!((info["fits"].as_bool(), info["settings"]["hoopPosition"][0].as_f64()), (Some(true), Some(-20.0)), "{info}");
    // too far out: the design no longer fits
    s.execute("embroidery.settings", &json!({"hoopPosition": [-40, 0]})).unwrap();
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["fits"].as_bool(), Some(false));
    // centred again
    s.execute("embroidery.settings", &json!({"hoopPosition": "centre"})).unwrap();
    let (x, _) = centre_of(&mut s);
    assert!(x.abs() < 0.5);
    for bad in [json!({"hoopPosition": [1]}), json!({"hoopPosition": "left"}), json!({"hoopPosition": [1e9, 0]})] {
        assert!(s.execute("embroidery.settings", &bad).is_err(), "{bad}");
    }
}

#[test]
fn queued_designs_wait_for_their_folder_and_go_when_it_is_there() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let usb = std::env::temp_dir().join(format!("sf-queue-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&usb);
    let folder = usb.to_string_lossy().to_string();
    // the stick isn't in: the designs wait, in order
    let a = s.execute("embroidery.queue.add", &json!({"folder": folder, "name": "Logo", "format": "dst"})).unwrap();
    assert_eq!(a["status"].as_str(), Some("waiting"));
    s.execute("embroidery.queue.add", &json!({"folder": folder, "name": "Logo", "format": "pes"})).unwrap();
    let r = s.execute("embroidery.queue.run", &json!({})).unwrap();
    assert_eq!((r["waiting"].as_u64(), r["sent"].as_array().unwrap().len()), (Some(2), 0));
    // it's in: they go, and the next one goes at once
    std::fs::create_dir_all(&usb).unwrap();
    let r = s.execute("embroidery.queue.run", &json!({})).unwrap();
    assert_eq!(r["sent"].as_array().unwrap().len(), 2, "{r}");
    assert!(usb.join("Logo.dst").exists() && usb.join("Logo.pes").exists());
    let c = s.execute("embroidery.queue.add", &json!({"folder": folder, "name": "Logo", "format": "dst"})).unwrap();
    assert_eq!(c["status"].as_str(), Some("sent"));
    assert!(c["path"].as_str().unwrap().ends_with("Logo-2.dst"), "{c}");
    let list = s.execute("embroidery.queue", &json!({})).unwrap();
    assert_eq!((list["items"].as_array().unwrap().len(), list["waiting"].as_u64()), (3, Some(0)));
    assert_eq!(s.execute("embroidery.queue.remove", &json!({"done": true})).unwrap()["removed"].as_u64(), Some(3));
    assert!(s.execute("embroidery.queue.remove", &json!({})).is_err());
    let _ = std::fs::remove_dir_all(&usb);
}

#[test]
fn clipart_motifs_place_digitized() {
    let mut s = session();
    let list = s.execute("embroidery.clipart", &json!({})).unwrap();
    let names: Vec<String> = list["clipart"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect();
    assert!(names.len() >= 15, "{names:?}");
    // every motif places and sews
    for name in &names {
        let r = s.execute("embroidery.clipart.insert", &json!({"name": name, "size": 30})).unwrap();
        assert!(r["objects"].as_u64().unwrap() >= 1, "{name}: {r}");
    }
    assert!(s.embroidery_plan().unwrap().pattern().stitch_count() > 1000);
    // one colour asked for, about the size asked for
    let mut s = session();
    s.execute("embroidery.clipart.insert", &json!({"name": "flower", "size": 50, "color": "#123456"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["colors"].as_array().unwrap().len(), 1, "{info}");
    let w = info["widthMm"].as_f64().unwrap().max(info["heightMm"].as_f64().unwrap());
    assert!((w - 50.0).abs() < 6.0, "{w}");
    for bad in [json!({"name": "Dragon"}), json!({}), json!({"name": "Star", "size": 1}), json!({"name": "Star", "color": "gold"})] {
        assert!(s.execute("embroidery.clipart.insert", &bad).is_err(), "{bad}");
    }
}

#[test]
fn job_orders_keep_the_order_and_print_its_worksheet() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.job.create", &json!({"customer": "Rovers FC", "quantity": 24, "due": "2026-11-01", "garment": "Navy polo", "placement": "left-chest", "notes": "Club crest"})).unwrap();
    assert_eq!(r["number"].as_str(), Some("J-0001"));
    let id = r["id"].as_u64().unwrap();
    let r = s.execute("embroidery.job.create", &json!({"customer": "Cafe Uno"})).unwrap();
    assert_eq!(r["number"].as_str(), Some("J-0002"));
    // listed newest first, found by text, by status
    let list = s.execute("embroidery.jobs", &json!({})).unwrap();
    assert_eq!(list["jobs"][0]["customer"].as_str(), Some("Cafe Uno"));
    assert_eq!(s.execute("embroidery.jobs", &json!({"query": "rovers"})).unwrap()["jobs"].as_array().unwrap().len(), 1);
    s.execute("embroidery.job.update", &json!({"id": id, "status": "production", "quantity": 30})).unwrap();
    let busy = s.execute("embroidery.jobs", &json!({"status": "production"})).unwrap();
    assert_eq!((busy["jobs"][0]["quantity"].as_u64(), busy["jobs"].as_array().unwrap().len()), (Some(30), 1));
    // the worksheet: the job's number as its barcode, its order in the notes
    s.execute("embroidery.job.worksheet", &json!({"id": id})).unwrap();
    let mut texts = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t == "J-0001"), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("30 pieces")) && texts.iter().any(|t| t.contains("Navy polo")), "{texts:?}");
    // its files, named after it
    s.set_active(0);
    let dir = std::env::temp_dir().join(format!("sf-jobs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let r = s
        .execute("embroidery.job.output", &json!({"id": id, "dir": dir.to_string_lossy(), "formats": ["dst"], "worksheet": "none", "design": false}))
        .unwrap();
    assert!(r["files"][0]["path"].as_str().unwrap().contains("J-0001"), "{r}");
    let _ = std::fs::remove_dir_all(&dir);
    for (cmd, bad) in [
        ("embroidery.job.create", json!({})),
        ("embroidery.job.create", json!({"customer": "X", "quantity": 0})),
        ("embroidery.job.update", json!({"id": id, "status": "lost"})),
        ("embroidery.job.update", json!({"id": 99})),
        ("embroidery.job.worksheet", json!({"id": id, "kind": "pattern"})),
        ("embroidery.jobs", json!({"status": "lost"})),
    ] {
        assert!(s.execute(cmd, &bad).is_err(), "{cmd} {bad}");
    }
    s.execute("embroidery.job.delete", &json!({"id": id})).unwrap();
    assert!(s.execute("embroidery.job.delete", &json!({"id": id})).is_err());
}

#[test]
fn the_stitch_view_prints_life_size_or_fitted() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.printView", &json!({})).unwrap();
    assert_eq!((r["pages"].as_u64(), r["scale"].as_f64()), (Some(1), Some(1.0)), "{r}");
    // the stitches as a picture, life size: 40 mm and 2 mm round it
    let d = &s.doc().unwrap().doc;
    let mut images = Vec::new();
    d.walk(|n| {
        if matches!(n.kind, NodeKind::Image(_)) {
            images.push(n.bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false).unwrap());
        }
    });
    assert_eq!(images.len(), 1);
    assert!((images[0].width() / PT - 44.0).abs() < 2.0, "{:?}", images[0]);
    // a big design: over several pages life size, on one page fitted
    s.set_active(0);
    rect(&mut s, 0.0, 0.0, 400.0 * PT, 300.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.printView", &json!({"page": "letter"})).unwrap();
    assert!(r["pages"].as_u64().unwrap() >= 4, "{r}");
    s.set_active(0);
    let r = s.execute("embroidery.printView", &json!({"scale": "fit", "view": "flat"})).unwrap();
    assert!(r["pages"].as_u64() == Some(1) && r["scale"].as_f64().unwrap() < 0.6, "{r}");
    s.set_active(0);
    for bad in [json!({"scale": "half"}), json!({"page": "a3"}), json!({"view": "3d"})] {
        assert!(s.execute("embroidery.printView", &bad).is_err(), "{bad}");
    }
}

#[test]
fn the_design_exports_as_an_image_of_its_stitches() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    let png = |r: &Value| image::load_from_memory(&vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap()).unwrap().to_rgba8();
    let r = s.execute("embroidery.exportImage", &json!({})).unwrap();
    let img = png(&r);
    // 40 × 20 mm and 3 mm round it, 10 pixels a millimetre
    assert!((img.width() as i64 - 460).abs() <= 12 && (img.height() as i64 - 260).abs() <= 12, "{}×{}", img.width(), img.height());
    assert_eq!(img.get_pixel(1, 1).0, [255, 255, 255, 255], "white round it");
    let c = img.get_pixel(img.width() / 2, img.height() / 2).0;
    assert!(c[0] > 90 && c[0] > c[1] + 40 && c[0] > c[2] + 40 && c[3] == 255, "red thread in the middle: {c:?}");
    // lit thread varies in shade; flat thread doesn't
    let shades = |img: &image::RgbaImage| {
        let mut v: Vec<[u8; 4]> = (60..200).map(|x| img.get_pixel(x, img.height() / 2).0).filter(|p| p[0] > p[1] + 40).collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    assert!(shades(&img) > 3);
    let flat = png(&s.execute("embroidery.exportImage", &json!({"view": "flat", "ppmm": 4, "background": "transparent"})).unwrap());
    assert_eq!(flat.get_pixel(0, 0).0[3], 0, "transparent round it");
    assert!(flat.width() < 200);
    // written to a file, and remembered among the exports
    let path = std::env::temp_dir().join(format!("sf-stitch-image-{}.png", std::process::id()));
    let r = s.execute("embroidery.exportImage", &json!({"path": path.to_string_lossy(), "background": "#102040"})).unwrap();
    assert!(std::fs::read(&path).unwrap().starts_with(b"\x89PNG") && r["width"].as_u64().is_some());
    assert_eq!(s.export_history.front().and_then(|e| e["command"].as_str()), Some("embroidery.exportImage"));
    let _ = std::fs::remove_file(&path);
    for bad in [json!({"ppmm": 0}), json!({"ppmm": 41}), json!({"view": "3d"}), json!({"background": "blue"}), json!({"margin": -1})] {
        assert!(s.execute("embroidery.exportImage", &bad).is_err(), "{bad}");
    }
}

#[test]
fn stitch_lists_and_gcode_export_the_stitches_as_text() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    let stitches = s.embroidery_plan().unwrap().pattern().stitch_count();
    let text = |r: &Value| String::from_utf8(vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
    let csv = text(&s.execute("embroidery.export", &json!({"format": "csv"})).unwrap());
    assert!(csv.starts_with("index,command,x,y,thread,color,name\n"));
    assert_eq!(csv.lines().filter(|l| l.contains(",stitch,")).count(), stitches);
    assert!(csv.lines().filter(|l| l.contains(",stitch,")).all(|l| l.contains(",1,#d62828,")));
    let json_text = text(&s.execute("embroidery.export", &json!({"format": "JSON", "name": "Badge"})).unwrap());
    let v: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!((v["name"].as_str(), v["threads"][0]["color"].as_str()), (Some("Badge"), Some("#d62828")));
    // the stitches about the centre: a 40 mm wide design spans about ±20 mm
    let xs: Vec<f64> = v["stitches"].as_array().unwrap().iter().filter_map(|s| s[0].as_f64()).collect();
    let (lo, hi) = xs.iter().fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
    assert!((lo + 20.0).abs() < 1.5 && (hi - 20.0).abs() < 1.5, "{lo} {hi}");
    let g = text(&s.execute("embroidery.export", &json!({"format": "gcode", "gcode": {"feed": 900, "zStep": 0, "stitchCommand": "M3"}})).unwrap());
    assert!(g.contains(" F900\n") && !g.contains("G1 Z") && g.lines().filter(|l| *l == "M3").count() == stitches, "{}", &g[..300.min(g.len())]);
    assert!(s.execute("embroidery.export", &json!({"format": "gcode", "gcode": {"feed": -1}})).is_err());
    // in a batch, beside the machine file
    let dir = std::env::temp_dir().join(format!("sf-textual-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let r = s
        .execute(
            "embroidery.output",
            &json!({"dir": dir.to_string_lossy(), "formats": ["dst", "gcode", "csv"], "worksheet": "none", "design": false, "name": "Job"}),
        )
        .unwrap();
    assert!(dir.join("Job.gcode").exists() && dir.join("Job.csv").exists(), "{r}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exports_that_write_files_are_remembered_and_run_again() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let dir = std::env::temp_dir().join(format!("sf-exports-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    // a file written, files given back (not remembered), a failure, a batch
    let svg = dir.join("art.svg").to_string_lossy().to_string();
    s.execute("document.export", &json!({"path": svg, "format": "svg"})).unwrap();
    s.execute("document.export", &json!({"format": "svg"})).unwrap();
    s.execute("embroidery.export", &json!({"format": "dst"})).unwrap();
    assert!(s.execute("embroidery.output", &json!({"dir": dir.join("missing").to_string_lossy(), "formats": ["dst"]})).is_err());
    s.execute("embroidery.output", &json!({"dir": d, "formats": ["pes"], "worksheet": "none", "design": false, "name": "Job"})).unwrap();
    let h = s.execute("export.history", &json!({})).unwrap();
    let list = h["exports"].as_array().unwrap();
    let commands: Vec<&str> = list.iter().filter_map(|e| e["command"].as_str()).collect();
    assert_eq!(commands, ["embroidery.output", "embroidery.output", "document.export"], "{h}");
    assert!(list[0]["ok"].as_bool().unwrap() && list[0]["paths"][0].as_str().unwrap().ends_with("Job.pes"), "{h}");
    assert!(!list[1]["ok"].as_bool().unwrap() && list[1]["error"].as_str().unwrap().contains("isn't there"));
    assert_eq!(list[2]["paths"][0].as_str(), Some(svg.as_str()));
    assert!(list[0].get("params").is_none(), "what was asked stays inside");
    // again: the same files written again, not numbered beside them, and remembered
    std::fs::remove_file(dir.join("Job.pes")).unwrap();
    let r = s.execute("export.again", &json!({})).unwrap();
    assert!(r["files"][0]["path"].as_str().unwrap().ends_with("Job.pes") && dir.join("Job.pes").exists(), "{r}");
    std::fs::write(&svg, b"old").unwrap();
    s.execute("export.again", &json!({"index": 3})).unwrap();
    assert!(std::fs::read_to_string(&svg).unwrap().contains("<svg"));
    assert_eq!(s.execute("export.history", &json!({"limit": 2})).unwrap()["exports"].as_array().unwrap().len(), 2);
    assert!(s.execute("export.again", &json!({"index": 99})).is_err());
    assert_eq!(s.execute("export.history.clear", &json!({})).unwrap()["cleared"].as_u64(), Some(5));
    assert!(s.execute("export.again", &json!({})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn batch_output_writes_machine_files_worksheet_and_design_in_one_step() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let dir = std::env::temp_dir().join(format!("sf-output-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.to_string_lossy().to_string();
    let (docs, active) = (s.documents().len(), s.active_index());
    let r = s.execute("embroidery.output", &json!({"dir": d, "formats": ["dst", "PES", "dst"], "name": "Logo"})).unwrap();
    let files = r["files"].as_array().unwrap();
    let kinds: Vec<&str> = files.iter().filter_map(|f| f["kind"].as_str()).collect();
    assert_eq!(kinds, ["machine", "machine", "design", "worksheet"], "{r}");
    let read = |f: &Value| std::fs::read(f["path"].as_str().unwrap()).unwrap();
    let dst = sf_formats::read(sf_formats::Format::Dst, &read(&files[0])).unwrap();
    assert!(dst.stitch_count() > 100 && files[1]["path"].as_str().unwrap().ends_with("Logo.pes"));
    assert!(read(&files[3]).starts_with(b"%PDF") && files[3]["path"].as_str().unwrap().ends_with("Logo-worksheet.pdf"));
    assert!(files[2]["path"].as_str().unwrap().ends_with("Logo.stitchfoundry") && !read(&files[2]).is_empty());
    // the documents open stay as they were
    assert_eq!((s.documents().len(), s.active_index()), (docs, active));
    // the names taken: all numbered alike, unless replacing
    let r = s.execute("embroidery.output", &json!({"dir": d, "formats": ["dst"], "worksheet": "none", "design": false, "name": "Logo"})).unwrap();
    assert_eq!(r["name"].as_str(), Some("Logo-2"), "{r}");
    let r = s
        .execute("embroidery.output", &json!({"dir": d, "formats": ["dst"], "worksheet": "none", "design": false, "name": "Logo", "replace": true}))
        .unwrap();
    assert_eq!(r["name"].as_str(), Some("Logo"));
    // a profile kept by name, its folder and choices used, what is asked winning
    s.execute(
        "embroidery.output.saveProfile",
        &json!({"name": "Tajima shop", "formats": ["dst", "exp"], "worksheet": "approval", "design": false, "dir": d}),
    )
    .unwrap();
    let list = s.execute("embroidery.output.profiles", &json!({})).unwrap();
    assert_eq!(list["profiles"][0]["formats"], json!(["dst", "exp"]));
    let r = s.execute("embroidery.output", &json!({"profile": "TAJIMA SHOP", "name": "Cap"})).unwrap();
    let kinds: Vec<&str> = r["files"].as_array().unwrap().iter().filter_map(|f| f["format"].as_str()).collect();
    assert_eq!(kinds, ["dst", "exp", "pdf"], "{r}");
    let r = s.execute("embroidery.output", &json!({"profile": "tajima shop", "name": "Cap", "worksheet": "none", "replace": true})).unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 2);
    // saved again: replaced, not added
    s.execute("embroidery.output.saveProfile", &json!({"name": "tajima SHOP", "worksheet": "none"})).unwrap();
    assert_eq!(s.prefs.embroidery_output_profiles.len(), 1);
    // without formats: the design's machine's (else DST)
    let r = s.execute("embroidery.output", &json!({"dir": d, "worksheet": "none", "design": false, "name": "Plain"})).unwrap();
    assert_eq!(r["files"][0]["format"].as_str(), Some("dst"));
    for bad in [
        json!({"dir": d, "formats": ["zzz"]}),
        json!({"dir": d, "worksheet": "poster"}),
        json!({"formats": ["dst"]}),
        json!({"dir": dir.join("missing").to_string_lossy(), "formats": ["dst"]}),
        json!({"profile": "nobody"}),
        json!({"dir": d, "formats": [], "worksheet": "none", "design": false}),
        json!({"dir": d, "design": "yes"}),
    ] {
        assert!(s.execute("embroidery.output", &bad).is_err(), "{bad}");
    }
    assert_eq!((s.documents().len(), s.active_index()), (docs, active));
    s.execute("embroidery.output.deleteProfile", &json!({"name": "Tajima Shop"})).unwrap();
    assert!(s.execute("embroidery.output.deleteProfile", &json!({"name": "Tajima Shop"})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_design_library_lists_machine_files_with_their_figures() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    let pattern = s.embroidery_plan().unwrap().pattern().clone();
    let dir = std::env::temp_dir().join(format!("sf-library-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("caps")).unwrap();
    let write = |name: &std::path::Path, f: sf_formats::Format| {
        std::fs::write(name, sf_formats::write(f, &pattern, &sf_formats::WriteOptions::default()).unwrap()).unwrap();
    };
    write(&dir.join("Rose.dst"), sf_formats::Format::Dst);
    write(&dir.join("caps").join("Logo.pes"), sf_formats::Format::Pes);
    std::fs::write(dir.join("broken.jef"), b"not a design").unwrap();
    std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();
    let folder = dir.to_string_lossy().to_string();
    let r = s.execute("embroidery.library.scan", &json!({"folder": folder})).unwrap();
    let names: Vec<&str> = r["designs"].as_array().unwrap().iter().filter_map(|d| d["name"].as_str()).collect();
    assert_eq!(names, ["Logo", "Rose"], "{r}");
    assert_eq!(r["failed"].as_array().unwrap().len(), 1, "the broken file said: {r}");
    let rose = &r["designs"][1];
    assert_eq!((rose["format"].as_str(), rose["colors"].as_u64()), (Some("dst"), Some(1)));
    assert!((rose["widthMm"].as_f64().unwrap() - 40.0).abs() < 1.5 && rose["stitches"].as_u64().unwrap() > 100, "{rose}");
    assert!(!rose["preview"][0]["runs"].as_array().unwrap().is_empty());
    // searched, not into subfolders, sorted by stitches
    let r = s.execute("embroidery.library.scan", &json!({"folder": folder, "query": "ROSE"})).unwrap();
    assert_eq!(r["count"].as_u64(), Some(1));
    let r = s.execute("embroidery.library.scan", &json!({"folder": folder, "recursive": false})).unwrap();
    assert_eq!(r["count"].as_u64(), Some(1));
    assert!(s.execute("embroidery.library.scan", &json!({"folder": folder, "sort": "colour"})).is_err());
    // kept while the file is the same
    assert!(s.design_library.len() >= 2);
    // a catalogue of them: one page here
    let r = s.execute("embroidery.library.catalogue", &json!({"folder": folder, "columns": 3})).unwrap();
    assert_eq!((r["designs"].as_u64(), r["pages"].as_u64()), (Some(2), Some(1)));
    let st = s.doc().unwrap();
    assert!(st.doc.title.ends_with("catalogue"));
    let mut texts = Vec::new();
    st.doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t == "Rose") && texts.iter().any(|t| t.contains("DST")), "{texts:?}");
    assert!(s.execute("embroidery.library.scan", &json!({})).is_err());
    let empty = dir.join("caps").join("none");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(s.execute("embroidery.library.catalogue", &json!({"folder": empty.to_string_lossy()})).is_err());
    // batch convert: the folder's designs to JEF beside them, the broken one said, then again
    // numbered, and the chosen ones to VP3 in another folder
    let r = s.execute("embroidery.library.convert", &json!({"folder": folder, "format": "jef"})).unwrap();
    let to: Vec<&str> = r["converted"].as_array().unwrap().iter().filter_map(|c| c["to"].as_str()).collect();
    assert!(to.len() == 2 && to.iter().all(|t| t.ends_with(".jef")) && to.iter().any(|t| t.ends_with("caps/Logo.jef")), "{r}");
    assert_eq!((r["skipped"].as_array().unwrap().len(), r["failed"].as_array().unwrap().len()), (1, 0), "broken.jef is already JEF: {r}");
    let jef = sf_formats::read(sf_formats::Format::Jef, &std::fs::read(dir.join("Rose.jef")).unwrap()).unwrap();
    let dst = sf_formats::read(sf_formats::Format::Dst, &std::fs::read(dir.join("Rose.dst")).unwrap()).unwrap();
    assert!(jef.stitch_count().abs_diff(dst.stitch_count()) <= dst.stitch_count() / 50, "{} vs {}", jef.stitch_count(), dst.stitch_count());
    let r = s.execute("embroidery.library.convert", &json!({"folder": folder, "format": "jef", "recursive": false, "query": "rose"})).unwrap();
    assert!(r["converted"][0]["to"].as_str().unwrap().ends_with("Rose-2.jef"), "{r}");
    let out = dir.join("vp3");
    std::fs::create_dir_all(&out).unwrap();
    let rose_path = dir.join("Rose.dst").to_string_lossy().to_string();
    let r = s
        .execute("embroidery.library.convert", &json!({"files": [rose_path, "/nowhere/x.pes"], "format": ".VP3", "dir": out.to_string_lossy()}))
        .unwrap();
    assert!(out.join("Rose.vp3").exists() && r["failed"].as_array().unwrap().len() == 1, "{r}");
    for bad in [
        json!({"folder": folder}),
        json!({"folder": folder, "format": "svg"}),
        json!({"format": "dst"}),
        json!({"files": [], "format": "dst"}),
        json!({"files": [rose_path], "format": "dst", "dir": out.join("none").to_string_lossy()}),
    ] {
        assert!(s.execute("embroidery.library.convert", &bad).is_err(), "{bad}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn colorways_rethread_the_design_everywhere() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let a = rect(&mut s, 20.0 * PT, 20.0 * PT, 30.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    rect(&mut s, 60.0 * PT, 20.0 * PT, 30.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#1a3e8c"})).unwrap();
    let colours = |s: &Session| s.embroidery_plan().unwrap().pattern().threads.iter().map(|t| t.color.to_hex()).collect::<Vec<_>>();
    assert_eq!(colours(&s), ["#d62828", "#1a3e8c"]);
    let l = s.execute("embroidery.colorway.list", &json!({})).unwrap();
    assert_eq!((l["active"].as_u64(), l["slots"].as_array().unwrap().len()), (Some(0), 2), "{l}");
    // a copy, made the active one, then its first colour green
    let r = s.execute("embroidery.colorway.add", &json!({"name": "Spring"})).unwrap();
    assert_eq!(r["index"].as_u64(), Some(1));
    s.execute("embroidery.colorway.setThread", &json!({"slot": 0, "color": "#2a9d4b", "name": "Leaf"})).unwrap();
    assert_eq!(colours(&s), ["#2a9d4b", "#1a3e8c"]);
    // the art keeps its own colour; machine files carry the colorway
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    let r = s.execute("embroidery.export", &json!({"format": "pes"})).unwrap();
    assert_eq!(r["colors"].as_u64(), Some(2));
    // back to the design's own, and to the colorway again
    s.execute("embroidery.colorway.activate", &json!({"index": 0})).unwrap();
    assert_eq!(colours(&s), ["#d62828", "#1a3e8c"]);
    s.execute("embroidery.colorway.activate", &json!({"index": 1})).unwrap();
    assert_eq!(colours(&s)[0], "#2a9d4b");
    // printed on the worksheet
    s.execute("embroidery.worksheet", &json!({"barcode": "none"})).unwrap();
    let mut texts = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t.contains("Colorway: Spring")), "{texts:?}");
    // and every colorway's threads, the one in use marked
    assert!(texts.iter().any(|t| t == "Colorways"), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("Spring (in use): 1. Leaf, 2. ")), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("The design's own: 1. ")), "{texts:?}");
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    // a second, renamed; undo the rename
    s.execute("embroidery.colorway.add", &json!({"from": 0})).unwrap();
    assert_eq!(colours(&s)[0], "#d62828", "copied from the design's own");
    s.execute("embroidery.colorway.rename", &json!({"name": "Night"})).unwrap();
    let l = s.execute("embroidery.colorway.list", &json!({})).unwrap();
    assert_eq!(l["colorways"][1]["name"], "Night");
    s.execute("edit.undo", &json!({})).unwrap();
    let l = s.execute("embroidery.colorway.list", &json!({})).unwrap();
    assert_eq!(l["colorways"][1]["name"], "Colorway 2");
    // deleting the first keeps the second active, now number 1
    let r = s.execute("embroidery.colorway.remove", &json!({"index": 1})).unwrap();
    assert_eq!(r["active"].as_u64(), Some(1));
    let r = s.execute("embroidery.colorway.remove", &json!({})).unwrap();
    assert_eq!(r["active"].as_u64(), Some(0));
    assert_eq!(colours(&s), ["#d62828", "#1a3e8c"]);
    // bad requests
    for (cmd, p) in [
        ("embroidery.colorway.activate", json!({"index": 3})),
        ("embroidery.colorway.setThread", json!({"slot": 0, "color": "#000000"})),
        ("embroidery.colorway.rename", json!({"name": "x"})),
        ("embroidery.colorway.remove", json!({})),
        ("embroidery.colorway.add", json!({"from": 7})),
    ] {
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
    }
    s.execute("embroidery.colorway.add", &json!({})).unwrap();
    for p in [json!({"slot": 2, "color": "#000000"}), json!({"slot": 0, "color": "green"})] {
        assert!(s.execute("embroidery.colorway.setThread", &p).is_err(), "{p}");
    }
}

#[test]
fn costs_are_estimated_and_printed() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    assert!(s.execute("embroidery.cost", &json!({})).is_err(), "nothing stitched");
    rect(&mut s, 50.0 * PT, 50.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    let stitches = info["stitches"].as_f64().unwrap();
    assert!(info["costEstimate"].is_null(), "not shown until asked for");
    s.execute("embroidery.settings", &json!({"cost": {"perThousand": 2, "threadPerKm": 0, "setup": 10, "currency": "€"}})).unwrap();
    let c = s.execute("embroidery.cost", &json!({"quantity": 5})).unwrap();
    let want = (stitches / 1000.0 * 2.0 * 100.0).round() / 100.0;
    assert!((c["stitches"].as_f64().unwrap() - want).abs() < 0.011 && c["setup"].as_f64() == Some(2.0), "{c}");
    assert!((c["total"].as_f64().unwrap() - 5.0 * c["perPiece"].as_f64().unwrap()).abs() < 0.03 && c["currency"] == "€", "{c}");
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["settings"]["cost"]["heads"].as_u64(), Some(1));
    assert!(info["costEstimate"]["text"].as_str().unwrap().starts_with('€'), "{info}");
    // on the worksheet
    s.execute("embroidery.worksheet", &json!({})).unwrap();
    let mut texts = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t.starts_with("Cost: €")), "{texts:?}");
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    // off; bad values
    s.execute("embroidery.settings", &json!({"cost": null})).unwrap();
    assert!(s.execute("embroidery.info", &json!({})).unwrap()["costEstimate"].is_null());
    for bad in [json!({"cost": 1}), json!({"cost": {"heads": 0}}), json!({"cost": {"perHour": -1}}), json!({"cost": {"currency": "dollars"}})] {
        assert!(s.execute("embroidery.settings", &bad).is_err(), "{bad}");
    }
    assert!(s.execute("embroidery.cost", &json!({"quantity": 0})).is_err());
}

#[test]
fn basting_is_sewn_first_round_the_design_or_inside_the_hoop() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 50.0 * PT, 50.0 * PT, 40.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = s.embroidery_plan().unwrap().pattern().clone();
    let r = s.execute("embroidery.settings", &json!({"basting": true})).unwrap();
    assert_eq!(r["settings"]["basting"]["margin"].as_f64(), Some(5.0), "{r}");
    let p = s.embroidery_plan().unwrap().pattern().clone();
    // 5 mm round: 50 × 30 mm, a needle point at most 6 mm on
    let b = p.bounds();
    assert!((b.width() - 50.0).abs() < 0.2 && (b.height() - 30.0).abs() < 0.2, "{b:?}");
    assert!(p.stitch_count() > plain.stitch_count() + 20 && p.threads.len() == plain.threads.len());
    // in the info, then round a 100 × 100 hoop 10 mm in
    assert!(s.execute("embroidery.info", &json!({})).unwrap()["settings"]["basting"].is_object());
    s.execute("embroidery.settings", &json!({"hoop": {"width": 100, "height": 100}, "basting": {"hoop": true, "margin": 10}})).unwrap();
    let b = s.embroidery_plan().unwrap().pattern().bounds();
    assert!((b.width() - 80.0).abs() < 0.2 && (b.height() - 80.0).abs() < 0.2, "{b:?}");
    // off again; bad values
    s.execute("embroidery.settings", &json!({"basting": false})).unwrap();
    assert_eq!(s.embroidery_plan().unwrap().pattern().stitch_count(), plain.stitch_count());
    for bad in [json!({"basting": 5}), json!({"basting": {"margin": -1}}), json!({"basting": {"length": 50}}), json!({"basting": {"hoop": 1}})] {
        assert!(s.execute("embroidery.settings", &bad).is_err(), "{bad}");
    }
}

#[test]
fn big_designs_split_into_hoopings_with_registration_marks() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // three 60 mm squares across 300 mm: too wide for a 130 × 180 hoop
    for i in 0..3 {
        rect(&mut s, (20.0 + 120.0 * f64::from(i)) * PT, 20.0 * PT, 60.0 * PT, 60.0 * PT);
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    }
    let dir = std::env::temp_dir().join(format!("sf-multihoop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = s.execute("embroidery.multiHoop", &json!({"hoop": "Brother 130 × 180 (5×7)", "dir": dir.to_string_lossy()})).unwrap();
    assert_eq!((r["columns"].as_u64(), r["rows"].as_u64()), (Some(3), Some(1)), "{r}");
    let hoopings = r["hoopings"].as_array().unwrap();
    assert_eq!(hoopings.len(), 3, "{r}");
    assert!(hoopings.iter().all(|h| h["objects"].as_u64() == Some(1)), "{r}");
    // each file fits the hoop, centred on it, and starts with the registration crosses
    for h in hoopings {
        let bytes = std::fs::read(h["file"].as_str().unwrap()).unwrap();
        let (_, p) = sf_formats::read_any(&bytes, Some("dst")).unwrap();
        let b = p.bounds();
        assert!(b.min.x >= -65.1 && b.max.x <= 65.1 && b.min.y >= -90.1 && b.max.y <= 90.1, "{b:?}");
        assert!(p.color_blocks().len() >= 2, "the marks are a colour of their own");
    }
    // the plan only, without marks; a hoop too small for an object: split, or refused
    let r = s.execute("embroidery.multiHoop", &json!({"width": 130, "height": 180, "marks": false})).unwrap();
    assert!(r["files"].as_array().unwrap().is_empty());
    assert!(s.execute("embroidery.multiHoop", &json!({"width": 40, "height": 40, "overlap": 5, "split": false})).is_err());
    assert!(s.execute("embroidery.multiHoop", &json!({"width": 130, "height": 180, "overlap": 80})).is_err());
    // marks of one's own: three targets in each overlap, in blue
    let first = |s: &mut Session| {
        let _ = std::fs::remove_dir_all(&dir);
        let r = s.execute("embroidery.multiHoop", &json!({"hoop": "Brother 130 × 180 (5×7)", "dir": dir.to_string_lossy()})).unwrap();
        let bytes = std::fs::read(r["hoopings"][1]["file"].as_str().unwrap()).unwrap();
        sf_formats::read_any(&bytes, Some("dst")).unwrap().1
    };
    let crosses = first(&mut s);
    // a sheet a hooping to print
    let docs = s.documents().len();
    let r = s.execute("embroidery.multiHoop", &json!({"hoop": "Brother 130 × 180 (5×7)", "sheets": true})).unwrap();
    assert_eq!(r["sheets"].as_u64(), Some(3), "{r}");
    assert_eq!(s.documents().len(), docs + 1);
    let sheet = &s.doc().unwrap().doc;
    assert_eq!(sheet.artboards.len(), 3);
    let mut texts = Vec::new();
    sheet.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t == "Hooping 2 of 3") && texts.iter().any(|t| t.contains("column 2, row 1")), "{texts:?}");
    s.set_active(0);
    s.execute("embroidery.settings", &json!({"registration": {"shape": "target", "size": 4, "perOverlap": 3, "color": "#0055ff"}})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["settings"]["registration"]["shape"].as_str(), Some("target"), "{info}");
    let targets = first(&mut s);
    // the middle hooping shares two overlaps: 6 marks, each a ring and two lines
    let jumps_before_colour = |p: &sf_core::Pattern| {
        p.stitches.iter().take_while(|st| st.cmd != sf_core::Cmd::ColorChange).filter(|st| st.cmd == sf_core::Cmd::Jump).count()
    };
    assert!(jumps_before_colour(&targets) > jumps_before_colour(&crosses), "{} vs {}", jumps_before_colour(&targets), jumps_before_colour(&crosses));
    for bad in [
        json!({"registration": {"shape": "star"}}),
        json!({"registration": {"size": 50}}),
        json!({"registration": {"perOverlap": 0}}),
        json!({"registration": {"color": "blue"}}),
        json!({"registration": 3}),
    ] {
        assert!(s.execute("embroidery.settings", &bad).is_err(), "{bad}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn worksheets_show_the_design_and_its_figures() {
    let mut s = session();
    assert!(s.execute("embroidery.worksheet", &json!({})).is_err());
    rect(&mut s, 20.0, 20.0, 120.0, 80.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    rect(&mut s, 160.0, 20.0, 20.0, 140.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#1a3e8c"})).unwrap();
    let r = s.execute("embroidery.worksheet", &json!({"kind": "approval", "customer": "Lee", "notes": "Left chest"})).unwrap();
    assert_eq!((r["kind"].as_str(), r["colors"].as_u64(), r["scale"].as_f64()), (Some("approval"), Some(2), Some(100.0)));
    let st = s.doc().unwrap();
    assert!(st.doc.title.ends_with("worksheet"), "{}", st.doc.title);
    let mut texts = Vec::new();
    let mut lines = 0;
    st.doc.walk(|n| match &n.kind {
        NodeKind::Text(t) => texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>()),
        NodeKind::Path { .. } if matches!(n.appearance.stroke_paint(), Paint::Solid { .. }) => lines += 1,
        _ => {}
    });
    for want in ["Approval sheet", "Stitches: ", "Colours in sewing order", "Notes: Left chest", "Approved by"] {
        assert!(texts.iter().any(|t| t.contains(want)), "{want} missing from {texts:?}");
    }
    assert!(texts.iter().any(|t| t.contains("Customer: Lee")), "{texts:?}");
    assert!(lines >= 2, "the stitches drawn in two colours: {lines}");
    assert!(s.execute("embroidery.worksheet", &json!({"kind": "invoice"})).is_err());
    // a pattern on Letter paper: the design over the page
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    let r = s.execute("embroidery.worksheet", &json!({"kind": "pattern", "page": "letter"})).unwrap();
    assert_eq!(r["kind"].as_str(), Some("pattern"));
    let st = s.doc().unwrap();
    assert_eq!((st.doc.artboards[0].rect.width(), st.doc.artboards[0].rect.height()), (612.0, 792.0));
    // one line of figures, its count of colours said right
    let mut figures = Vec::new();
    st.doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            figures.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(figures.iter().any(|t| t.ends_with("2 colours")), "{figures:?}");
    assert!(s.execute("embroidery.worksheet", &json!({"page": "a3"})).is_err());
    // a barcode at the top right: the design's name by default, as bars inside the margin
    let barcode = |s: &Session| {
        let st = s.doc().unwrap();
        let mut found = None;
        st.doc.walk(|n| {
            if n.name.as_deref() == Some("Barcode") {
                found = Some((n.geometric_bounds().unwrap(), n.kind.clone()));
            }
        });
        found
    };
    let (b, kind) = barcode(&s).expect("a barcode");
    assert!(b.x1 <= 612.0 - 40.0 + 1e-6 && b.x0 > 300.0 && (b.y1 - b.y0 - 44.0).abs() < 1e-6, "{b:?}");
    let NodeKind::Path { path, .. } = kind else { panic!("bars") };
    // Code 128 of "Untitled-1": start, 10 characters, check and stop: 13 symbols of 3 bars, 4 in the stop
    assert_eq!(path.subpaths.len(), 12 * 3 + 4);
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    s.execute("embroidery.worksheet", &json!({"barcode": "code39", "barcodeText": "job 42"})).unwrap();
    let (_, kind) = barcode(&s).unwrap();
    let NodeKind::Path { path, .. } = kind else { panic!("bars") };
    // *JOB 42*: 8 characters of 5 bars
    assert_eq!(path.subpaths.len(), 8 * 5);
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    s.execute("embroidery.worksheet", &json!({"barcode": "none"})).unwrap();
    assert!(barcode(&s).is_none());
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    for bad in [json!({"barcode": "qr"}), json!({"barcode": "code39", "barcodeText": "a_b"}), json!({"barcodeText": "é"})] {
        assert!(s.execute("embroidery.worksheet", &bad).is_err(), "{bad}");
    }
    // the design's own notes, kept with it and printed when none are given, wrapped
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    let long = format!("Left chest, 9 cm below the collar. {}", "Use white bobbin thread. ".repeat(6));
    s.execute("embroidery.settings", &json!({"notes": long})).unwrap();
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["notes"].as_str(), Some(long.trim()));
    s.execute("embroidery.worksheet", &json!({})).unwrap();
    let mut texts = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    let at = texts.iter().position(|t| t.starts_with("Notes: ")).unwrap();
    let notes: Vec<&String> = texts[at..].iter().filter(|t| t.contains("bobbin")).collect();
    assert!(notes.len() >= 2 && notes[0].starts_with("Notes: Left chest"), "{texts:?}");
    assert!(notes.iter().all(|t| t.chars().count() <= 110), "{notes:?}");
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    s.execute("embroidery.settings", &json!({"notes": null})).unwrap();
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["notes"].as_str(), Some(""));
    assert!(s.execute("embroidery.settings", &json!({"notes": 5})).is_err());
}

#[test]
fn auto_digitize_traces_an_image_into_satin_and_fill() {
    let mut s = session();
    // A white logo: a blue disc, a long red bar and a speck.
    let r = vectorcraft_trace::Raster::from_fn(300, 200, |x, y| {
        let (dx, dy) = (x as f64 - 90.0, y as f64 - 90.0);
        if dx * dx + dy * dy <= 60.0 * 60.0 {
            [20, 40, 200, 255]
        } else if (60..260).contains(&x) && (170..178).contains(&y) {
            [220, 30, 30, 255]
        } else if (280..282).contains(&x) && (20..22).contains(&y) {
            [20, 160, 40, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    let png = r.encode_png();
    s.execute("file.place", &json!({"name": "logo.png", "dataBase64": vectorcraft_format::base64_encode(&png)})).unwrap();
    let before = s.doc().unwrap().doc.clone();
    let res = s.execute("embroidery.autoDigitize", &json!({"colors": 4})).unwrap();
    assert_eq!(res["objects"], 2, "{res}");
    assert_eq!(res["satin"], 1, "the bar is satin: {res}");
    assert_eq!(res["fill"], 1, "the disc is tatami: {res}");
    assert!(stitches(&s) > 1000, "{}", stitches(&s));
    let d = &s.doc().unwrap().doc;
    let images = d.layers[0].children().unwrap().iter().filter(|n| matches!(n.kind, NodeKind::Image(_))).count();
    assert_eq!(images, 1, "the artwork stays under the stitches");
    // One undo step back to the plain image.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.layers[0].children().unwrap().len(), before.layers[0].children().unwrap().len());
    assert_eq!(stitches(&s), 0);
    // Nothing to trace without an image.
    s.execute("select.none", &json!({})).unwrap();
    assert!(s.execute("embroidery.autoDigitize", &json!({})).is_err());
}

/// AD-FIX-01: a speck dropped as too small, or a highlight left out, leaves no hole in the shape
/// round it: the fabric doesn't show through.
#[test]
fn auto_digitize_leaves_no_holes_where_specks_are_dropped() {
    let mut s = session();
    // a blue disc with a small red speck and a white highlight in it
    let r = vectorcraft_trace::Raster::from_fn(300, 200, |x, y| {
        let (dx, dy) = (x as f64 - 100.0, y as f64 - 100.0);
        if (97..103).contains(&x) && (97..103).contains(&y) {
            [220, 30, 30, 255]
        } else if (130..137).contains(&x) && (97..104).contains(&y) {
            [255, 255, 255, 255]
        } else if dx * dx + dy * dy <= 80.0 * 80.0 {
            [20, 40, 200, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    s.execute("file.place", &json!({"name": "disc.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.autoDigitize", &json!({"colors": 3, "minArea": 10, "noise": 4})).unwrap();
    assert_eq!(res["objects"], 1, "only the disc is kept: {res}");
    let d = &s.doc().unwrap().doc;
    let g = d.node(NodeId(res["id"].as_u64().unwrap())).unwrap();
    let disc = &g.children().unwrap()[0];
    let regions = vectorcraft_embroidery::regions_mm(disc);
    let whole: Vec<_> = regions.iter().map(|r| sf_core::geom::Polygon::from_ring(r.outer.clone())).collect();
    let b = whole.iter().map(|p| p.bbox()).reduce(|a, b| a.union(b)).unwrap();
    let mm = (b.max.x - b.min.x) / 160.0; // a pixel, in mm
    let centre = sf_core::Point::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
    for at in [centre, sf_core::Point::new(centre.x + 33.0 * mm, centre.y)] {
        assert!(regions.iter().any(|r| r.contains(at)), "a hole at {at:?}: {:?}", regions.iter().map(|r| r.holes.len()).collect::<Vec<_>>());
    }
}

/// AD-FIX-02: where two colours meet, a seam the trace left between them (here a thin white
/// line) is sewn by one of them: no fabric shows between neighbouring colours.
#[test]
fn auto_digitize_leaves_no_gaps_between_colours() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(300, 200, |x, y| {
        if !(20..180).contains(&y) || !(20..280).contains(&x) || (x == 150 && (50..150).contains(&y)) {
            [255, 255, 255, 255]
        } else if x < 150 {
            [20, 40, 200, 255]
        } else {
            [220, 30, 30, 255]
        }
    });
    s.execute("file.place", &json!({"name": "two.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.autoDigitize", &json!({"colors": 3, "noise": 1, "minArea": 0.1})).unwrap();
    assert_eq!(res["objects"], 2, "{res}");
    let d = &s.doc().unwrap().doc;
    let g = d.node(NodeId(res["id"].as_u64().unwrap())).unwrap();
    let all: Vec<_> = g.children().unwrap().iter().flat_map(|n| vectorcraft_embroidery::regions_mm(n)).collect();
    let union = sf_core::geom::poly::union(&all);
    let holes: Vec<f64> = union.iter().flat_map(|p| p.holes.iter()).map(|h| sf_core::geom::Polygon::from_ring(h.clone()).area()).collect();
    assert!(holes.is_empty(), "fabric shows between the colours: {holes:?} ({res})");
}

/// AD-03: thin lines (a dark stripe across a shape, a short dash) are sewn as runs after the
/// shapes, on top; the dash is kept although its area is under `minArea`, and the shape under the
/// stripe has no gap left for it.
#[test]
fn auto_digitize_sews_thin_lines_as_runs_on_top() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(300, 200, |x, y| {
        let (dx, dy) = (x as f64 - 100.0, y as f64 - 100.0);
        if ((60..140).contains(&x) || (230..238).contains(&x)) && (99..102).contains(&y) {
            [15, 15, 15, 255]
        } else if dx * dx + dy * dy <= 80.0 * 80.0 {
            [20, 40, 200, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    s.execute("file.place", &json!({"name": "lines.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.autoDigitize", &json!({"colors": 3, "noise": 2, "minArea": 5})).unwrap();
    assert_eq!(res["run"], 2, "the stripe and the dash are runs: {res}");
    let d = &s.doc().unwrap().doc;
    let g = d.node(NodeId(res["id"].as_u64().unwrap())).unwrap();
    let kinds: Vec<bool> =
        g.children().unwrap().iter().map(|n| matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(sf_core::attr::StitchType::Run(_)))).collect();
    assert_eq!(kinds, [false, true, true], "the runs are sewn last");
    // the dark stripe is sewn down its middle, three times over, not round its outline
    for n in &g.children().unwrap()[1..] {
        assert!(n.path_data().unwrap().subpaths.iter().all(|sp| !sp.closed), "a line, not a loop");
        let Some(sf_core::attr::StitchType::Run(r)) = n.embroidery.as_deref().map(|e| &e.stitch) else { panic!() };
        assert_eq!(r.kind, sf_core::model::RunKind::Triple);
    }
    // the disc under the stripe is whole
    let disc = &g.children().unwrap()[0];
    let holes: usize = vectorcraft_embroidery::regions_mm(disc).iter().map(|r| r.holes.len()).sum();
    assert_eq!(holes, 0, "no gap is left under the stripe");
}

/// BG-01: Remove Background cuts a red disc out of a noisy green and blue background: the copy
/// over the photo is transparent round it, and Auto-Digitize then sews only the disc.
#[test]
fn remove_background_cuts_out_the_subject() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(160, 120, |x, y| {
        let (dx, dy) = (x as f64 - 80.0, y as f64 - 60.0);
        let n = ((x * 7 + y * 13) % 23) as u8;
        if dx * dx + dy * dy <= 35.0 * 35.0 {
            [210 + n / 2, 40 + n, 30, 255]
        } else if x < 80 {
            [40 + n, 150 + n, 60, 255]
        } else {
            [30, 60 + n, 170 + n, 255]
        }
    });
    s.execute("file.place", &json!({"name": "photo.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.image.removeBackground", &json!({"box": [0.15, 0.1, 0.7, 0.8]})).unwrap();
    let kept = res["kept"].as_f64().unwrap();
    let disc = std::f64::consts::PI * 35.0 * 35.0 / (160.0 * 120.0);
    assert!((kept - disc).abs() < disc / 6.0, "kept {kept}, the disc is {disc}");
    let d = &s.doc().unwrap().doc;
    let id = NodeId(res["id"].as_u64().unwrap());
    let NodeKind::Image(im) = &d.node(id).unwrap().kind else { panic!("not an image") };
    let cut = vectorcraft_trace::Raster::decode(&d.images[&im.key].bytes).unwrap();
    let alpha = |x: u32, y: u32| cut.rgba[((y * cut.width + x) * 4 + 3) as usize];
    assert_eq!((alpha(80, 60), alpha(5, 5), alpha(30, 60), alpha(130, 60)), (255, 0, 0, 0));
    assert_eq!(s.doc().unwrap().selection.objects, vec![id], "the copy is selected");
    // digitized: only the disc's red
    s.execute("embroidery.autoDigitize", &json!({"colors": 4})).unwrap();
    for t in &s.embroidery_plan().unwrap().pattern().threads {
        assert!(t.color.r > t.color.g && t.color.r > t.color.b, "only red is sewn, not {t:?}");
    }
    // bad parameters are refused
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    for bad in
        [json!({"box": [0, 0, 2, 1]}), json!({"box": [0, 0]}), json!({"keep": [[0.5]]}), json!({"remove": [[0.5, 0.5, 0]]}), json!({"keep": "x"})]
    {
        assert!(s.execute("embroidery.image.removeBackground", &bad).is_err(), "{bad}");
    }
}

/// PC-01: with no box and no marks Remove Background finds the subject itself (Select Subject),
/// here a red disc off the photo's centre on a plain backdrop.
#[test]
fn remove_background_finds_the_subject_without_a_box() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(200, 150, |x, y| {
        let (dx, dy) = (x as f64 - 120.0, y as f64 - 70.0);
        let n = ((x * 5 + y * 11) % 13) as u8;
        if dx * dx + dy * dy <= 40.0 * 40.0 { [200 + n, 40, 30, 255] } else { [225 + n, 228 + n, 222, 255] }
    });
    s.execute("file.place", &json!({"name": "photo.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.image.removeBackground", &json!({})).unwrap();
    assert_eq!(res["subject"], true, "Select Subject found it: {res}");
    let d = &s.doc().unwrap().doc;
    let NodeKind::Image(im) = &d.node(NodeId(res["id"].as_u64().unwrap())).unwrap().kind else { panic!("not an image") };
    let cut = vectorcraft_trace::Raster::decode(&d.images[&im.key].bytes).unwrap();
    let alpha = |x: u32, y: u32| cut.rgba[((y * cut.width + x) * 4 + 3) as usize];
    assert_eq!((alpha(120, 70), alpha(140, 90), alpha(20, 20), alpha(30, 130)), (255, 255, 0, 0), "{res}");
}

/// PH-01: when the cut from the photo's edge keeps the whole frame (a shaded ball on a soft
/// gradient), Select Subject's answer alone is the cut-out, not the two together.
#[test]
fn remove_background_ignores_a_subject_that_fills_the_frame() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(240, 200, |x, y| {
        let (fx, fy) = (f64::from(x), f64::from(y));
        let (dx, dy) = (fx - 120.0, fy - 100.0);
        let c = if dx.hypot(dy) < 60.0 {
            let shade = 1.1 - 0.6 * (dx + dy + 85.0) / 170.0;
            [230.0 * shade, 120.0 * shade, 30.0 * shade]
        } else {
            [200.0 + fx * 0.1, 215.0 - fy * 0.05, 235.0]
        };
        let [r, g, b] = c.map(|v| v.clamp(0.0, 255.0) as u8);
        [r, g, b, 255]
    });
    s.execute("file.place", &json!({"name": "ball.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.image.removeBackground", &json!({})).unwrap();
    // the ball is π·60² of 240 × 200 pixels: about 24 %
    let kept = res["kept"].as_f64().unwrap();
    assert!((0.18..0.32).contains(&kept), "kept {kept}: {res}");
}

/// PC-02: a stroke of the Cut-Out Brush puts back a part the cut left out, and an Alt stroke
/// (`remove`) takes away a part it kept; each is one step.
#[test]
fn refining_a_cutout_puts_back_and_takes_away() {
    let mut s = session();
    // a red disc (left) and a blue square (right) on noisy grey
    let r = vectorcraft_trace::Raster::from_fn(200, 100, |x, y| {
        let n = ((x * 7 + y * 13) % 17) as u8;
        let (dx, dy) = (x as f64 - 50.0, y as f64 - 50.0);
        if dx * dx + dy * dy <= 25.0 * 25.0 {
            [210 + n / 2, 40, 30, 255]
        } else if (125..175).contains(&x) && (25..75).contains(&y) {
            [30, 60, 200 + n / 2, 255]
        } else {
            [120 + n, 120 + n, 118 + n, 255]
        }
    });
    s.execute("file.place", &json!({"name": "two.png", "rect": [0, 0, 200, 100], "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())}))
        .unwrap();
    // the box takes in only the disc
    let res = s.execute("embroidery.image.removeBackground", &json!({"box": [0.05, 0.1, 0.45, 0.8], "replace": true})).unwrap();
    let alpha = |s: &Session, id: u64, x: u32, y: u32| {
        let d = &s.doc().unwrap().doc;
        let NodeKind::Image(im) = &d.node(NodeId(id)).unwrap().kind else { panic!("not an image") };
        let cut = vectorcraft_trace::Raster::decode(&d.images[&im.key].bytes).unwrap();
        cut.rgba[((y * cut.width + x) * 4 + 3) as usize]
    };
    let id = res["id"].as_u64().unwrap();
    assert_eq!((alpha(&s, id, 50, 50), alpha(&s, id, 150, 50)), (255, 0), "the square is left out");
    // the image's box in the document: the pixel (x, y) is at the same point
    let res = s.execute("embroidery.image.refineCutout", &json!({"keep": [[140, 50], [160, 50]], "radius": 6})).unwrap();
    let id = res["id"].as_u64().unwrap();
    assert_eq!((alpha(&s, id, 150, 50), alpha(&s, id, 130, 30), alpha(&s, id, 50, 50)), (255, 255, 255), "the square is back: {res}");
    assert_eq!(alpha(&s, id, 100, 50), 0, "the backdrop between them stays out");
    let res = s.execute("embroidery.image.refineCutout", &json!({"remove": [[40, 50], [60, 50]], "radius": 6})).unwrap();
    let id = res["id"].as_u64().unwrap();
    assert_eq!((alpha(&s, id, 50, 50), alpha(&s, id, 150, 50)), (0, 255), "the disc is taken away");
    // one step each
    s.execute("edit.undo", &json!({})).unwrap();
    let back = s.doc().unwrap().selection.objects.clone();
    assert_eq!(back.len(), 1);
    for bad in [json!({}), json!({"keep": [[1]]}), json!({"keep": [[1, 2]], "radius": 0}), json!({"keep": "x"})] {
        assert!(s.execute("embroidery.image.refineCutout", &bad).is_err(), "{bad}");
    }
}

/// PC-02: the before-and-after preview is a small photo and its cut-out; a lower sensitivity
/// keeps at least as much, a higher one takes at least as much away.
#[test]
fn the_cutout_preview_follows_the_sensitivity() {
    let mut s = session();
    let r = vectorcraft_trace::Raster::from_fn(400, 300, |x, y| {
        let n = ((x * 7 + y * 13) % 29) as u8;
        let (dx, dy) = (x as f64 - 200.0, y as f64 - 150.0);
        if dx * dx + dy * dy <= 80.0 * 80.0 { [190 + n / 2, 60 + n, 40, 255] } else { [90 + n, 140 + n, 80 + n, 255] }
    });
    s.execute("file.place", &json!({"name": "p.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let at = |sens: u32, s: &mut Session| s.execute("embroidery.image.cutoutPreview", &json!({"sensitivity": sens, "size": 120})).unwrap();
    let mid = at(50, &mut s);
    assert_eq!((mid["width"].as_u64(), mid["height"].as_u64()), (Some(120), Some(90)));
    for key in ["dataBase64", "beforeBase64"] {
        let png = vectorcraft_format::base64_decode(mid[key].as_str().unwrap()).unwrap();
        assert!(vectorcraft_trace::Raster::decode(&png).is_ok(), "{key}");
    }
    let (low, high) = (at(0, &mut s)["kept"].as_f64().unwrap(), at(100, &mut s)["kept"].as_f64().unwrap());
    let mid = mid["kept"].as_f64().unwrap();
    assert!(low >= mid && mid >= high && mid > 0.1, "{low} {mid} {high}");
    assert!(s.execute("embroidery.image.cutoutPreview", &json!({"sensitivity": 101})).is_err());
    assert!(s.execute("embroidery.image.cutoutPreview", &json!({"size": 10})).is_err());
}

#[test]
fn smart_auto_digitize_sews_details_as_runs_outlines_and_matches_a_chart() {
    let mut s = session();
    // a blue disc and a hairline (2 px: 0.7 mm) on white
    let r = vectorcraft_trace::Raster::from_fn(300, 200, |x, y| {
        let (dx, dy) = (x as f64 - 90.0, y as f64 - 90.0);
        if dx * dx + dy * dy <= 60.0 * 60.0 {
            [20, 40, 200, 255]
        } else if (60..260).contains(&x) && (175..177).contains(&y) {
            [20, 20, 20, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    s.execute("file.place", &json!({"name": "logo.png", "dataBase64": vectorcraft_format::base64_encode(&r.encode_png())})).unwrap();
    let res = s.execute("embroidery.autoDigitize", &json!({"colors": 4, "outline": "shapes", "chart": "Generic"})).unwrap();
    assert_eq!((res["fill"].as_u64(), res["run"].as_u64(), res["outlines"].as_u64()), (Some(1), Some(1), Some(1)), "{res}");
    // the threads are the chart's
    let chart = sf_core::threads::charts().into_iter().find(|c| c.name == "Generic").unwrap();
    let plan = s.embroidery_plan().unwrap();
    for t in &plan.pattern().threads {
        assert!(chart.threads.iter().any(|c| c.color == t.color), "{t:?} isn't in the chart");
    }
    // the hairline's run is sewn, the outline in the darkest thread
    assert!(stitches(&s) > 1000);
    // the whole design outlined instead, no details
    s.execute("edit.undo", &json!({})).unwrap();
    let mut img = None;
    s.doc().unwrap().doc.walk(|n| {
        if matches!(n.kind, NodeKind::Image(_)) {
            img = Some(n.id.0);
        }
    });
    s.execute("select.set", &json!({"ids": [img.unwrap()]})).unwrap();
    let res = s.execute("embroidery.autoDigitize", &json!({"colors": 4, "outline": "design", "details": 0})).unwrap();
    assert_eq!((res["run"].as_u64(), res["outlines"].as_u64(), res["satin"].as_u64()), (Some(0), Some(1), Some(1)), "{res}");
    for bad in [json!({"outline": "box"}), json!({"chart": "Nope"}), json!({"outlineColor": "blue"})] {
        assert!(s.execute("embroidery.autoDigitize", &bad).is_err(), "{bad}");
    }
}

#[test]
fn type_is_stitched_from_its_glyphs() {
    let mut s = session();
    // Small type (24 pt, about 8.5 mm): satin columns, with the counters of B, o and b
    // cut so the satin can turn around them.
    let small = NodeId(s.execute("text.create", &json!({"x": 20, "y": 100, "text": "Bob", "size": 24})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [small.0]})).unwrap();
    s.execute("embroidery.convert", &json!({})).unwrap();
    let e = s.doc().unwrap().doc.node(small).unwrap().embroidery.clone().unwrap();
    assert!(matches!(e.stitch, sf_core::attr::StitchType::Satin(_)), "{e:?}");
    let plan = s.embroidery_plan().unwrap();
    assert!(plan.design.objects.len() >= 6, "counters split into columns: {}", plan.design.objects.len());
    assert!(stitches(&s) > 200, "{}", stitches(&s));
    // Big type (400 pt, about 140 mm): stems too thick for satin are tatami.
    let big = NodeId(s.execute("text.create", &json!({"x": 20, "y": 400, "text": "I", "size": 400})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [big.0]})).unwrap();
    s.execute("embroidery.convert", &json!({})).unwrap();
    let e = s.doc().unwrap().doc.node(big).unwrap().embroidery.clone().unwrap();
    assert!(matches!(e.stitch, sf_core::attr::StitchType::Fill(_)), "{e:?}");
    // Editing the text restitches it.
    let before = stitches(&s);
    s.execute("select.set", &json!({"ids": [small.0]})).unwrap();
    s.execute("text.setText", &json!({"text": "Bobby"})).unwrap();
    assert!(stitches(&s) > before, "{} > {before}", stitches(&s));
}

#[test]
fn brush_strokes_are_sewn_before_or_after_an_object() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 60.0, 40.0);
    let b = rect(&mut s, 100.0, 10.0, 60.0, 40.0);
    let index = |s: &Session, id: NodeId| s.doc().unwrap().doc.position(id).unwrap().1;
    let stroke = |s: &mut Session, extra: serde_json::Value| {
        let mut p = json!({"brush": "run", "points": [[10, 80], [60, 90], [110, 80]]});
        p.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        s.execute("embroidery.draw", &p).map(|r| NodeId(r["id"].as_u64().unwrap()))
    };
    let after_a = stroke(&mut s, json!({"order": "after", "relativeTo": a.0})).unwrap();
    assert_eq!(index(&s, after_a), index(&s, a) + 1, "sewn right after a");
    let before_b = stroke(&mut s, json!({"order": "before", "relativeTo": b.0})).unwrap();
    assert_eq!(index(&s, before_b) + 1, index(&s, b), "sewn right before b");
    let top = stroke(&mut s, json!({})).unwrap();
    assert!(index(&s, top) > index(&s, b), "sewn last by default");
    assert!(stroke(&mut s, json!({"order": "after"})).is_err(), "after what?");
    assert!(stroke(&mut s, json!({"order": "sideways", "relativeTo": a.0})).is_err());
}

#[test]
fn machine_files_export_and_open_as_manual_stitches() {
    let mut s = session();
    rect(&mut s, 20.0, 20.0, 120.0, 80.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#d62828"})).unwrap();
    rect(&mut s, 160.0, 20.0, 20.0, 140.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#1a3e8c"})).unwrap();
    let before = stitches(&s);
    assert!(before > 1000, "{before}");
    let files: Vec<(&str, String)> = super::cmd::embroidery::MACHINE_EXTS
        .iter()
        .map(|ext| {
            let r = s.execute("embroidery.export", &json!({"format": ext})).unwrap();
            assert!(r["bytes"].as_u64().unwrap() > 100, "{ext}: {r}");
            (*ext, r["dataBase64"].as_str().unwrap().to_string())
        })
        .collect();
    for (ext, data) in files {
        let r = s.execute("document.open", &json!({"name": format!("design.{ext}"), "dataBase64": data})).unwrap();
        assert_eq!(r["format"], ext, "{r}");
        let st = s.doc().unwrap();
        let paths: Vec<_> = st.doc.layers[0].children().unwrap().iter().filter(|n| n.embroidery.is_some()).cloned().collect();
        let after = stitches(&s);
        assert!(after * 10 >= before * 9 && after * 10 <= before * 13, "{ext}: {after} stitches from {before}");
        let colors: Vec<Rgb> = paths.iter().filter_map(|n| n.embroidery.as_ref()?.thread.as_ref().map(|t| t.color)).collect();
        assert_eq!(colors.len(), 2, "{ext}: one stitch path per thread");
        // HUS's palette has 29 colours: the nearest of them is further off
        let near = if ext == "hus" { 45.0 } else { 25.0 };
        if !["dst", "exp", "u01", "tap", "dsb"].contains(&ext) {
            assert!(colors[0].delta_e(Rgb::new(0xd6, 0x28, 0x28)) < near, "{ext}: {colors:?}");
            assert!(colors[1].delta_e(Rgb::new(0x1a, 0x3e, 0x8c)) < near, "{ext}: {colors:?}");
        }
    }
    // File › Place brings a machine file in as stitches too.
    let dst = s.execute("embroidery.export", &json!({"format": "dst"})).unwrap()["dataBase64"].as_str().unwrap().to_string();
    s.execute("file.new", &json!({"width": 600, "height": 600})).unwrap();
    s.execute("file.place", &json!({"name": "design.dst", "dataBase64": dst})).unwrap();
    let placed = stitches(&s);
    assert!(placed * 10 >= before * 9, "placed {placed} of {before}");
    // Nothing to export without stitches; unknown formats are refused.
    s.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
    assert!(s.execute("embroidery.export", &json!({"format": "dst"})).is_err());
    assert!(s.execute("embroidery.export", &json!({"format": "zip"})).is_err());
}

#[test]
fn minus_front_cuts_a_hole_and_keeps_the_stitches() {
    let mut s = session();
    let a = rect(&mut s, 20.0, 20.0, 160.0, 120.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#2a9d4b"})).unwrap();
    let full = stitches(&s);
    let hole = NodeId(s.execute("shape.ellipse", &json!({"x": 60, "y": 40, "width": 80, "height": 80})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [a.0, hole.0]})).unwrap();
    s.execute("object.pathfinder.minusFront", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let stitched: Vec<_> = st.doc.layers[0].children().unwrap().iter().filter(|n| n.embroidery.is_some()).cloned().collect();
    assert_eq!(stitched.len(), 1, "the result keeps the back object's stitches");
    let e = stitched[0].embroidery.as_ref().unwrap();
    assert_eq!(e.thread.as_ref().map(|t| t.color), Some(Rgb::new(0x2a, 0x9d, 0x4b)));
    let holed = stitches(&s);
    assert!(holed < full, "fewer stitches round the hole: {holed} of {full}");
    // No needle goes down inside the hole (a circle of radius 40 pt round (100, 80) pt).
    let plan = s.embroidery_plan().unwrap();
    let (c, r) = (sf_core::Point::new(100.0, 80.0) * vectorcraft_embroidery::MM_PER_PT, 40.0 * vectorcraft_embroidery::MM_PER_PT);
    let inside = plan.pattern().stitches.iter().filter(|st| st.cmd == sf_core::Cmd::Stitch && st.pos().distance(c) < r - 0.5).count();
    assert_eq!(inside, 0, "stitches inside the hole");
}

#[test]
fn run_styles_passes_ties_and_satin_options() {
    let mut s = session();
    let line = NodeId(s.execute("shape.line", &json!({"x1": 20, "y1": 50, "x2": 300, "y2": 50})).unwrap()["id"].as_u64().unwrap());
    s.execute("embroidery.set", &json!({"type": "run"})).unwrap();
    let once = stitches(&s);
    s.execute("embroidery.setParams", &json!({"params": {"passes": 2}})).unwrap();
    let back = stitches(&s);
    assert!(back > once * 2 - 10 && back < once * 2 + 10, "a backtrack sews the line twice: {back} vs {once}");
    s.execute("embroidery.set", &json!({"type": "backstitch"})).unwrap();
    let e = s.doc().unwrap().doc.node(line).unwrap().embroidery.clone().unwrap();
    let sf_core::attr::StitchType::Run(r) = &e.stitch else { panic!("{e:?}") };
    assert_eq!(r.kind, sf_core::model::RunKind::Back);
    s.execute("embroidery.set", &json!({"type": "stemstitch"})).unwrap();
    assert!(stitches(&s) > once, "stem stitch sews forward two, back one");
    // ties: the object can turn off the design's tie-in and tie-off
    let tied = stitches(&s);
    s.execute("embroidery.setParams", &json!({"params": {"tieIn": false, "tieOff": false}})).unwrap();
    assert_eq!(stitches(&s), tied - 8);
    let e = s.doc().unwrap().doc.node(line).unwrap().embroidery.clone().unwrap();
    assert_eq!((e.tie_in, e.tie_off), (Some(false), Some(false)));
    s.execute("embroidery.setParams", &json!({"params": {"tieIn": null, "tieOff": null}})).unwrap();
    assert_eq!(stitches(&s), tied, "null follows the design again");

    // satin: twin-sided pull compensation, double zigzag, preserve long stitches
    let mut s = session();
    let col = rect(&mut s, 50.0, 50.0, 200.0, 40.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let plain = stitches(&s);
    s.execute(
        "embroidery.setParams",
        &json!({"params": {"pullComp2": 0.4, "preserveLong": true, "underlay": {"auto": false, "zigzag": true, "doubleZigzag": true}}}),
    )
    .unwrap();
    let e = s.doc().unwrap().doc.node(col).unwrap().embroidery.clone().unwrap();
    let sf_core::attr::StitchType::Satin(p) = &e.stitch else { panic!("{e:?}") };
    assert_eq!(p.pull_comp_2, Some(0.4));
    assert!(p.preserve_long && p.underlay.double_zigzag);
    assert_ne!(stitches(&s), plain);
    s.execute("embroidery.setParams", &json!({"params": {"pullComp2": null}})).unwrap();
    let e = s.doc().unwrap().doc.node(col).unwrap().embroidery.clone().unwrap();
    let sf_core::attr::StitchType::Satin(p) = &e.stitch else { panic!("{e:?}") };
    assert_eq!(p.pull_comp_2, None);
}

fn stitched_path(s: &mut Session, d: &str, ty: &str) -> NodeId {
    let id = NodeId(s.execute("path.create", &json!({"d": d})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": ty})).unwrap();
    id
}

fn first_and_last(s: &Session, id: NodeId) -> (sf_core::Point, sf_core::Point) {
    let plan = s.embroidery_plan().unwrap();
    let pat = plan.pattern();
    let ranges = plan.records_of(id);
    let moves: Vec<sf_core::Point> =
        ranges.iter().flat_map(|r| pat.stitches[r.clone()].iter()).filter(|s| s.cmd == sf_core::Cmd::Stitch).map(|s| s.pos()).collect();
    (moves[0], *moves.last().unwrap())
}

#[test]
fn reverse_swaps_where_sewing_starts() {
    let mut s = session();
    let id = stitched_path(&mut s, "M20 100 L300 100", "run");
    let (a, b) = first_and_last(&s, id);
    s.execute("embroidery.reverse", &json!({})).unwrap();
    let (a2, b2) = first_and_last(&s, id);
    assert!(a2.distance(b) < 0.5 && b2.distance(a) < 0.5, "{a:?}→{b:?} became {a2:?}→{b2:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(first_and_last(&s, id).0.distance(a) < 0.5);
    s.execute("select.none", &json!({})).unwrap();
    assert!(s.execute("embroidery.reverse", &json!({})).is_err());
}

#[test]
fn holes_are_removed_or_filled() {
    let mut s = session();
    // a 100 pt square with a 40 pt and a 4 pt square hole
    let d = "M10 10 H110 V110 H10 Z M30 30 V70 H70 V30 Z M90 90 V94 H94 V90 Z";
    let id = stitched_path(&mut s, d, "fill");
    let subpaths = |s: &Session| s.doc().unwrap().doc.node(id).unwrap().path_data().unwrap().subpaths.len();
    assert_eq!(subpaths(&s), 3);
    // only holes under 5 mm² (the 4 pt square is ~2 mm²)
    let r = s.execute("embroidery.removeHoles", &json!({"maxArea": 5.0})).unwrap();
    assert_eq!(r["removed"], 1);
    assert_eq!(subpaths(&s), 2);
    // fill the remaining hole: a new stitched object, sewn right after
    let r = s.execute("embroidery.fillHoles", &json!({})).unwrap();
    let hole = NodeId(r["ids"][0].as_u64().unwrap());
    let st = s.doc().unwrap();
    assert_eq!(st.doc.position(hole).unwrap().1, st.doc.position(id).unwrap().1 + 1);
    assert!(st.doc.node(hole).unwrap().embroidery.is_some());
    assert!(!s.embroidery_plan().unwrap().records_of(hole).is_empty(), "the hole fill is stitched");
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    let r = s.execute("embroidery.removeHoles", &json!({})).unwrap();
    assert_eq!(r["removed"], 1);
    assert_eq!(subpaths(&s), 1);
    assert!(s.execute("embroidery.fillHoles", &json!({})).is_err(), "no holes left");
    assert!(s.execute("embroidery.removeHoles", &json!({"maxArea": -1})).is_err());
}

#[test]
fn select_same_thread_and_stitch_type() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 50.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#cc2233"})).unwrap();
    let b = rect(&mut s, 100.0, 10.0, 50.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#cc2233"})).unwrap();
    let c = rect(&mut s, 200.0, 10.0, 50.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#2233cc"})).unwrap();
    rect(&mut s, 300.0, 10.0, 50.0, 50.0); // not stitched
    let selected = |s: &Session| {
        let mut v: Vec<u64> = s.doc().unwrap().selection.objects.iter().map(|i| i.0).collect();
        v.sort();
        v
    };
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("select.same.thread", &json!({})).unwrap();
    assert_eq!(selected(&s), vec![a.0, b.0]);
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("select.same.stitchType", &json!({})).unwrap();
    assert_eq!(selected(&s), vec![a.0, c.0]);
}

#[test]
fn design_checks_find_sewing_problems() {
    let mut s = session();
    let kinds = |s: &mut Session| -> Vec<String> {
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        r["findings"].as_array().unwrap().iter().map(|f| f["kind"].as_str().unwrap().to_string()).collect()
    };
    assert!(kinds(&mut s).is_empty(), "an empty design has no problems");
    // four fills stacked on the same spot
    for _ in 0..4 {
        rect(&mut s, 50.0, 50.0, 60.0, 60.0);
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    }
    let k = kinds(&mut s);
    assert!(k.contains(&"dense".to_string()), "{k:?}");
    // a wide satin without auto split: long stitches (but under the 12 mm the
    // machine allows, which would cut them in two)
    rect(&mut s, 200.0, 50.0, 200.0, 30.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"splitLength": 0}})).unwrap();
    // tiny lettering
    let t = s.execute("text.create", &json!({"x": 50, "y": 300, "text": "abc", "size": 8})).unwrap();
    s.execute("select.set", &json!({"ids": [t["id"]]})).unwrap();
    s.execute("embroidery.convert", &json!({})).unwrap();
    // a hoop too small for the design
    s.execute("embroidery.settings", &json!({"hoop": "Brother 100 × 100 (4×4)"})).unwrap();
    let r = s.execute("embroidery.check", &json!({})).unwrap();
    let k: Vec<&str> = r["findings"].as_array().unwrap().iter().map(|f| f["kind"].as_str().unwrap()).collect();
    for want in ["dense", "longStitches", "smallLettering", "hoop"] {
        assert!(k.contains(&want), "{want} missing from {k:?}");
    }
    let dense: Vec<&Value> = r["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == "dense").collect();
    assert!(dense.iter().any(|f| f["ids"].as_array().unwrap().len() == 4 && f["at"].as_array().is_some()), "the four stacked fills: {dense:?}");
}

#[test]
fn satin_shapes_have_no_dense_ends() {
    // square ends: rows parallel to the end, no fan converging on a corner
    for (w, h) in [(200.0, 40.0), (40.0, 200.0), (150.0, 25.0), (60.0, 10.0)] {
        let mut s = session();
        rect(&mut s, 50.0, 50.0, w, h);
        s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        let dense: Vec<&Value> = r["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == "dense").collect();
        assert!(dense.is_empty(), "{w}×{h}: {dense:?}");
    }
    // lettering: no letter piles thread up at its stroke ends or where strokes meet
    for (text, size) in [("Sample", 60), ("WAVE", 120), ("small agw", 24)] {
        let mut s = session();
        let t = s.execute("text.create", &json!({"x": 40, "y": 200, "text": text, "size": size})).unwrap();
        s.execute("select.set", &json!({"ids": [t["id"]]})).unwrap();
        s.execute("embroidery.convert", &json!({})).unwrap();
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        let dense: Vec<&Value> = r["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == "dense").collect();
        assert!(dense.is_empty(), "{text}: {dense:?}");
    }
    // two overlapping fills, or a satin border along a fill's edge, are normal
    for border in [false, true] {
        let mut s = session();
        for x in if border { vec![100.0] } else { vec![100.0, 110.0] } {
            rect(&mut s, x, 100.0, 120.0, 90.0);
            s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        }
        if border {
            let edge: Vec<Vec<f64>> = (0..=24).map(|i| vec![100.0 + i as f64 * 5.0, 100.0]).collect();
            s.execute("embroidery.draw", &json!({"brush": "steil", "points": edge, "width": 3.0})).unwrap();
        }
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        assert!(r["findings"].as_array().unwrap().iter().all(|f| f["kind"] != "dense"), "{border}: {r}");
    }
}

#[test]
fn satin_lines_have_smart_corners() {
    let dense = |s: &mut Session| -> Vec<f64> {
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        r["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == "dense").filter_map(|f| f["value"].as_f64()).collect()
    };
    let l: Vec<Vec<f64>> =
        (0..=24).map(|i| vec![100.0 + i as f64 * 5.0, 100.0]).chain((1..=18).map(|i| vec![220.0, 100.0 + i as f64 * 5.0])).collect();
    for width in [3.0, 4.0] {
        let mut s = session();
        let id = s.execute("embroidery.draw", &json!({"brush": "steil", "points": l, "width": width})).unwrap()["id"].clone();
        assert!(dense(&mut s).is_empty(), "{width} mm: {:?}", dense(&mut s));
        // rows turning round the corner pile thread up inside it
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        s.execute("embroidery.setParams", &json!({"params": {"corners": "Turn"}})).unwrap();
        assert!(!dense(&mut s).is_empty(), "{width} mm turned");
    }
    // a closed outline painted as a stroke is a satin border round the shape, not a fill
    let mut s = session();
    let id = s
        .execute("embroidery.draw", &json!({"brush": "steil", "stamp": "rectangle", "from": [100, 100], "to": [300, 250], "width": 3.0}))
        .unwrap()["id"]
        .clone();
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let (x0, y0, x1, y1) = (100.0 / PT, 100.0 / PT, 300.0 / PT, 250.0 / PT);
    let off_outline = |s: &Session| -> usize {
        let plan = s.embroidery_plan().unwrap();
        plan.pattern()
            .stitches
            .iter()
            .filter(|st| st.cmd == sf_core::Cmd::Stitch)
            .filter(|st| {
                let (x, y) = (st.x, st.y);
                // how far from the outline, measured square (a mitered corner is square)
                let inside = (x - x0).min(x1 - x).min(y - y0).min(y1 - y);
                let outside = (x0 - x).max(x - x1).max(y0 - y).max(y - y1);
                inside.max(outside) > 2.0
            })
            .count()
    };
    assert!(stitches(&s) > 500 && off_outline(&s) == 0, "{} stitches, {} off the outline", stitches(&s), off_outline(&s));
    assert!(dense(&mut s).is_empty(), "{:?}", dense(&mut s));
    // its thread colour paints the stroke, so it stays a border
    s.execute("select.set", &json!({"ids": [id]})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#cc2200"})).unwrap();
    assert_eq!(off_outline(&s), 0);
    let n = s.doc().unwrap().doc.node(NodeId(id.as_u64().unwrap())).unwrap().clone();
    assert!(n.appearance.fill_paint().is_none(), "{:?}", n.appearance.fill_paint());
}

/// Needle positions (mm) of one object's stitches.
fn needles(s: &Session, id: NodeId) -> Vec<sf_core::Point> {
    let plan = s.embroidery_plan().unwrap();
    let pat = plan.pattern();
    plan.records_of(id).iter().flat_map(|r| pat.stitches[r.clone()].iter()).filter(|s| s.cmd == sf_core::Cmd::Stitch).map(|s| s.pos()).collect()
}

#[test]
fn objects_remove_the_stitching_below_them() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let below = rect(&mut s, 100.0, 100.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let above = rect(&mut s, 150.0, 140.0, 80.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let (x0, y0, x1, y1) = (150.0 / PT, 140.0 / PT, 230.0 / PT, 200.0 / PT);
    // stitches of the lower fill more than `margin` mm inside the upper one
    let under = |s: &Session, margin: f64| {
        needles(s, below).iter().filter(|p| p.x > x0 + margin && p.x < x1 - margin && p.y > y0 + margin && p.y < y1 - margin).count()
    };
    let all = needles(&s, below).len();
    assert!(under(&s, 0.0) > 100, "{}", under(&s, 0.0));
    s.execute("select.set", &json!({"ids": [above.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"removeUnderlying": true}})).unwrap();
    // nothing deeper than the 1 mm overlap (and the rows' pull compensation), but the
    // overlap itself is still sewn
    assert_eq!(under(&s, 1.5), 0);
    assert!(under(&s, 0.0) > 0);
    // the hole is about 13 % of the fill; rows broken by it add turns and travel
    assert!(needles(&s, below).len() + 150 < all, "{} of {all}", needles(&s, below).len());
    s.execute("embroidery.setParams", &json!({"params": {"underlyingOverlap": 0.0}})).unwrap();
    assert_eq!(under(&s, 0.5), 0);
    s.execute("embroidery.setParams", &json!({"params": {"removeUnderlying": false}})).unwrap();
    assert_eq!(needles(&s, below).len(), all);
    // a fill covered completely makes no stitches at all
    let mut s = session();
    let small = rect(&mut s, 150.0, 150.0, 40.0, 40.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    rect(&mut s, 100.0, 100.0, 200.0, 200.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"removeUnderlying": true}})).unwrap();
    assert!(needles(&s, small).is_empty());
    let r = s.execute("embroidery.check", &json!({})).unwrap();
    assert!(r["findings"].as_array().unwrap().iter().any(|f| f["kind"] == "noStitches" && f["ids"][0] == small.0), "{r}");
    // the stitches of a fill cut in two still belong to it
    let mut s = session();
    let wide = rect(&mut s, 100.0, 100.0, 300.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    rect(&mut s, 220.0, 80.0, 40.0, 100.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"removeUnderlying": true}})).unwrap();
    let pts = needles(&s, wide);
    let (left, right) = (pts.iter().filter(|p| p.x < 220.0 / PT).count(), pts.iter().filter(|p| p.x > 260.0 / PT).count());
    let deep = pts.iter().filter(|p| p.x > 220.0 / PT + 1.5 && p.x < 260.0 / PT - 1.5).count();
    assert!(left > 100 && right > 100 && deep == 0, "{left} left, {right} right, {deep} deep under the bar");
}

#[test]
fn machine_profiles_set_limits_and_the_export_format() {
    let mut s = session();
    rect(&mut s, 50.0, 50.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["machines"].as_array().unwrap().len(), 7, "{info}");
    assert_eq!(info["machine"], "");
    // a machine sets its limits and speed first; values given with it win
    let r = s.execute("embroidery.settings", &json!({"machine": "PES", "maxJump": 7.0})).unwrap();
    assert_eq!(r["machine"], "pes");
    assert_eq!((r["settings"]["machine_speed"].as_f64(), r["settings"]["max_jump"].as_f64()), (Some(650.0), Some(7.0)), "{r}");
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!((info["machine"].as_str(), info["settings"]["maxJump"].as_f64()), (Some("pes"), Some(7.0)), "{info}");
    // export writes the machine's format unless another is asked for
    let e = s.execute("embroidery.export", &json!({})).unwrap();
    assert_eq!(e["format"], "pes");
    let bytes = vectorcraft_format::base64_decode(e["dataBase64"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with(b"#PES"));
    assert_eq!(s.execute("embroidery.export", &json!({"format": "dst"})).unwrap()["format"], "dst");
    // unknown machines are refused; null forgets the machine but keeps its values
    assert!(s.execute("embroidery.settings", &json!({"machine": "abc"})).is_err());
    let r = s.execute("embroidery.settings", &json!({"machine": null})).unwrap();
    assert!(r.get("machine").is_none(), "{r}");
    assert_eq!(r["settings"]["machine_speed"].as_f64(), Some(650.0));
    assert_eq!(s.execute("embroidery.export", &json!({})).unwrap()["format"], "dst");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["machine"], "pes");
}

#[test]
fn machine_profiles_can_be_changed_and_made() {
    let mut s = session();
    rect(&mut s, 50.0, 50.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    // a standard profile takes the design's machine settings, and keeps its name, format and kind
    s.execute("embroidery.settings", &json!({"machine": "vp3", "machineSpeed": 700.0, "tieLength": 1.2, "trimSeconds": 8.0})).unwrap();
    let r = s.execute("embroidery.machine.save", &json!({"name": "VP3", "fromDesign": true})).unwrap();
    assert_eq!(
        (r["name"].as_str(), r["standard"].as_bool(), r["edited"].as_bool(), r["created"].as_bool()),
        (Some("vp3"), Some(true), Some(true), Some(false)),
        "{r}"
    );
    assert!(s.execute("embroidery.machine.save", &json!({"name": "vp3", "format": "pes"})).is_err(), "a standard profile keeps its format");
    assert!(s.execute("embroidery.machine.save", &json!({"name": "vp3", "newName": "Mine"})).is_err(), "and its name");
    // choosing it sets its values, the advanced ones too
    s.execute("embroidery.settings", &json!({"machine": null, "machineSpeed": 500.0, "tieLength": 0.8})).unwrap();
    let r = s.execute("embroidery.settings", &json!({"machine": "vp3"})).unwrap();
    let st = &r["settings"];
    assert_eq!((st["machine_speed"].as_f64(), st["tie_length"].as_f64(), st["trim_seconds"].as_f64()), (Some(700.0), Some(1.2), Some(8.0)), "{r}");
    // one of your own starts from its format's profile
    let r = s
        .execute("embroidery.machine.save", &json!({"name": "Diamond deLuxe", "format": "VP3", "speed": 850.0, "advanced": {"tieStyle": "triangle"}}))
        .unwrap();
    assert_eq!((r["created"].as_bool(), r["standard"].as_bool(), r["format"].as_str()), (Some(true), Some(false), Some("vp3")), "{r}");
    let list = s.execute("embroidery.machine.list", &json!({})).unwrap();
    let all = list["machines"].as_array().unwrap();
    assert_eq!(all.len(), 8, "{list}");
    let d = &all[7];
    assert_eq!(
        (d["name"].as_str(), d["label"].as_str(), d["kind"].as_str(), d["speed"].as_f64(), d["trimSeconds"].as_f64()),
        (Some("Diamond deLuxe"), Some("Diamond deLuxe"), Some("home"), Some(850.0), Some(8.0)),
        "{d}"
    );
    assert_eq!((d["advanced"]["tieStyle"].as_str(), d["advanced"]["tieLength"].as_f64()), (Some("triangle"), Some(1.2)), "{d}");
    assert_eq!((all[5]["label"].as_str(), all[5]["edited"].as_bool()), (Some("VP3: home machines"), Some(true)));
    // the design using it keeps its format too, and is exported in it
    let r = s.execute("embroidery.settings", &json!({"machine": "diamond DELUXE"})).unwrap();
    assert_eq!(
        (r["machine"].as_str(), r["machine_format"].as_str(), r["settings"]["tie_style"].as_str()),
        (Some("Diamond deLuxe"), Some("vp3"), Some("triangle"))
    );
    assert_eq!(s.execute("embroidery.export", &json!({})).unwrap()["format"], "vp3");
    // renamed and given another format: the design follows
    s.execute("embroidery.machine.save", &json!({"name": "Diamond deLuxe", "newName": "Diamond", "format": "pes"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!((info["machine"].as_str(), info["machineFormat"].as_str()), (Some("Diamond"), Some("pes")), "{info}");
    assert_eq!(s.execute("embroidery.export", &json!({})).unwrap()["format"], "pes");
    // names stay apart from the formats and each other; values stay sensible
    s.execute("embroidery.machine.save", &json!({"name": "Shop", "format": "dst"})).unwrap();
    for p in [
        json!({"name": "Diamond", "newName": "dst"}),
        json!({"name": "Diamond", "newName": "shop"}),
        json!({"name": "Diamond", "newName": " "}),
        json!({"name": "Tower", "format": "emb"}),
        json!({"name": "Tower"}),
        json!({"name": "Diamond", "speed": 0}),
        json!({"name": "Diamond", "advanced": {"tieStyle": "knot"}}),
        json!({"name": "Diamond", "advanced": 3}),
        json!({}),
    ] {
        assert!(s.execute("embroidery.machine.save", &p).is_err(), "{p}");
    }
    // deleted, the design keeps its name and format
    s.execute("embroidery.machine.delete", &json!({"name": "diamond"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["machines"].as_array().unwrap().len(), 8, "the standard ones and Shop");
    assert_eq!((info["machine"].as_str(), info["machineFormat"].as_str()), (Some("Diamond"), Some("pes")));
    assert_eq!(s.execute("embroidery.export", &json!({})).unwrap()["format"], "pes");
    // a standard profile goes back to how it ships; one as it ships keeps no copy
    let r = s.execute("embroidery.machine.delete", &json!({"name": "VP3"})).unwrap();
    assert_eq!((r["name"].as_str(), r["reset"].as_bool()), (Some("vp3"), Some(true)));
    assert!(s.execute("embroidery.machine.delete", &json!({"name": "vp3"})).is_err(), "nothing to reset");
    assert!(s.execute("embroidery.machine.delete", &json!({"name": "Tower"})).is_err());
    s.execute("embroidery.machine.save", &json!({"name": "dst", "speed": 900.0})).unwrap();
    assert_eq!(s.prefs.embroidery_machines.len(), 2);
    assert_eq!(s.execute("embroidery.machine.save", &json!({"name": "dst", "speed": 1000.0})).unwrap()["edited"], false);
    assert_eq!(s.prefs.embroidery_machines.len(), 1, "only Shop");
    // they are kept with the preferences, which a reset keeps
    let json = s.prefs.to_json();
    assert_eq!(json["embroideryMachines"][0]["name"], "Shop");
    let back: Prefs = serde_json::from_value(json).unwrap();
    assert_eq!(back.embroidery_machines, s.prefs.embroidery_machines);
    s.prefs.reset(None);
    assert_eq!(s.prefs.embroidery_machines.len(), 1);
}

/// An object's own stitches: (x, y, command) in document points.
fn own(s: &mut Session, id: NodeId) -> (bool, Vec<(f64, f64, String)>) {
    let r = s.execute("embroidery.stitches", &json!({"id": id.0})).unwrap();
    let list = r["stitches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|q| (q[0].as_f64().unwrap(), q[1].as_f64().unwrap(), q[2].as_str().unwrap().to_string()))
        .collect();
    (r["frozen"].as_bool().unwrap(), list)
}

fn records(s: &Session, cmd: sf_core::Cmd) -> usize {
    s.embroidery_plan().map_or(0, |p| p.pattern().stitches.iter().filter(|st| st.cmd == cmd).count())
}

#[test]
fn stitches_can_be_frozen_edited_and_regenerated() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let id = rect(&mut s, 50.0, 50.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let (frozen, generated) = own(&mut s, id);
    assert!(!frozen && generated.len() > 100, "{}", generated.len());
    let total = stitches(&s);
    // freezing changes nothing that is sewn
    s.execute("embroidery.freeze", &json!({})).unwrap();
    assert_eq!(stitches(&s), total);
    let (frozen, list) = own(&mut s, id);
    assert!(frozen && list.len() == generated.len());
    assert!(s.execute("embroidery.freeze", &json!({})).is_err(), "frozen already");
    // a moved stitch is sewn where it was moved to
    s.execute("embroidery.stitches.edit", &json!({"id": id.0, "ops": [{"op": "move", "indices": [10], "by": [5.0, 0.0]}]})).unwrap();
    let moved = own(&mut s, id).1[10].clone();
    assert!((moved.0 - list[10].0 - 5.0).abs() < 1e-6 && (moved.1 - list[10].1).abs() < 1e-6);
    let target = sf_core::Point::new(moved.0 / PT, moved.1 / PT);
    let plan = s.embroidery_plan().unwrap();
    assert!(plan.pattern().stitches.iter().any(|st| st.cmd == sf_core::Cmd::Stitch && st.pos().distance(target) < 1e-3));
    // delete three, insert one in front
    s.execute(
        "embroidery.stitches.edit",
        &json!({"id": id.0, "ops": [{"op": "delete", "indices": [2, 1, 0]}, {"op": "insert", "after": -1, "at": [52.0, 52.0]}]}),
    )
    .unwrap();
    let list = own(&mut s, id).1;
    assert_eq!(list.len(), generated.len() - 2);
    assert_eq!((list[0].0, list[0].1, list[0].2.as_str()), (52.0, 52.0, "stitch"));
    // machine functions at single stitches
    let (stops, trims) = (records(&s, sf_core::Cmd::Stop), records(&s, sf_core::Cmd::Trim));
    s.execute("embroidery.function.insert", &json!({"id": id.0, "at": 50, "kind": "stop"})).unwrap();
    assert_eq!(own(&mut s, id).1[51].2, "stop");
    assert_eq!(records(&s, sf_core::Cmd::Stop), stops + 1);
    s.execute("embroidery.function.insert", &json!({"id": id.0, "at": 80, "kind": "trim"})).unwrap();
    assert_eq!(records(&s, sf_core::Cmd::Trim), trims + 1);
    s.execute("embroidery.function.insert", &json!({"id": id.0, "at": 70, "kind": "jump"})).unwrap();
    assert_eq!(own(&mut s, id).1[70].2, "jump");
    s.execute("embroidery.function.clear", &json!({"id": id.0, "at": 70})).unwrap();
    assert_eq!(own(&mut s, id).1[70].2, "stitch");
    s.execute("embroidery.function.clear", &json!({"id": id.0, "at": 51})).unwrap();
    assert_eq!(records(&s, sf_core::Cmd::Stop), stops);
    // bad requests change nothing
    let before = own(&mut s, id).1;
    for (cmd, p) in [
        ("embroidery.function.clear", json!({"id": id.0, "at": 3})),
        ("embroidery.function.insert", json!({"id": id.0, "at": 100000, "kind": "trim"})),
        ("embroidery.function.insert", json!({"id": id.0, "at": 3, "kind": "colour"})),
        ("embroidery.stitches.edit", json!({"id": id.0, "ops": [{"op": "move", "indices": [100000], "by": [1, 1]}]})),
        ("embroidery.stitches.edit", json!({"id": id.0, "ops": [{"op": "spin"}]})),
        ("embroidery.stitches.edit", json!({"id": id.0, "ops": [{"op": "insert", "after": -2, "at": [0, 0]}]})),
    ] {
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
    }
    assert_eq!(own(&mut s, id).1, before);
    // frozen stitches follow the object when it moves
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 20, "dy": 0})).unwrap();
    let shifted = own(&mut s, id).1;
    assert!((shifted[5].0 - before[5].0 - 20.0).abs() < 1e-6 && (shifted[5].1 - before[5].1).abs() < 1e-6);
    // regenerate drops the edits; undo brings them back
    s.execute("embroidery.regenerate", &json!({})).unwrap();
    assert!(!own(&mut s, id).0);
    assert!(s.execute("embroidery.regenerate", &json!({})).is_err(), "nothing to regenerate");
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(own(&mut s, id), (true, shifted));
}

#[test]
fn objects_can_stop_the_machine_after_them() {
    let mut s = session();
    let a = rect(&mut s, 50.0, 50.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    rect(&mut s, 200.0, 50.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert_eq!(records(&s, sf_core::Cmd::Stop), 0);
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"stopAfter": true}})).unwrap();
    assert_eq!(records(&s, sf_core::Cmd::Stop), 1);
    // the stop comes after the first object's last stitch
    let plan = s.embroidery_plan().unwrap();
    let last = plan.records_of(a).last().map(|r| r.end).unwrap();
    assert_eq!(plan.pattern().stitches[last - 1].cmd, sf_core::Cmd::Stop);
}

fn color_changes(s: &Session) -> usize {
    records(s, sf_core::Cmd::ColorChange)
}

#[test]
fn branches_carry_the_thread_under_what_is_sewn_next() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // two squares 40 pt (14 mm) apart and a bar across both, sewn last
    let a = rect(&mut s, 50.0, 100.0, 60.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let b = rect(&mut s, 150.0, 100.0, 60.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let bar = rect(&mut s, 40.0, 115.0, 180.0, 30.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setBeads", &json!({"id": a.0, "exit": [110.0, 130.0]})).unwrap();
    s.execute("embroidery.setBeads", &json!({"id": b.0, "entry": [150.0, 130.0]})).unwrap();
    let trims = records(&s, sf_core::Cmd::Trim);
    assert!(trims >= 2, "{trims}");
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    assert!(s.execute("embroidery.branch", &json!({})).is_err(), "one object is not a branch");
    s.execute("select.set", &json!({"ids": [a.0, b.0, bar.0]})).unwrap();
    let g = NodeId(s.execute("embroidery.branch", &json!({})).unwrap()["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    assert_eq!(st.doc.node(g).unwrap().embroidery.as_ref().unwrap().stitch.id(), "branch");
    assert_eq!(st.doc.parent_of(a), Some(g));
    assert_eq!(st.selection.objects, vec![g]);
    // the way from the first square to the second runs under the bar, and the second
    // ends under the bar too: no trims but the last
    assert_eq!(records(&s, sf_core::Cmd::Trim), 1);
    let plan = s.embroidery_plan().unwrap();
    let (end, start) = (plan.records_of(a).last().unwrap().end, plan.records_of(b)[0].start);
    let travel: Vec<sf_core::Point> = plan.pattern().stitches[end..start].iter().map(|r| r.pos()).collect();
    assert!(plan.pattern().stitches[end..start].iter().all(|r| r.cmd == sf_core::Cmd::Stitch));
    assert!(travel.len() >= 5, "{travel:?}");
    assert!(travel.windows(2).all(|w| w[0].distance(w[1]) < 2.6), "{travel:?}");
    assert!(travel.iter().all(|p| p.y > 115.0 / PT && p.y < 145.0 / PT), "under the bar: {travel:?}");
    // the group is not an object of its own
    let r = s.execute("embroidery.check", &json!({})).unwrap();
    assert!(r["findings"].as_array().unwrap().iter().all(|f| f["ids"].as_array().is_none_or(|ids| !ids.contains(&json!(g.0)))), "{r}");
    // breaking it apart gives the objects back, sewn on their own again
    let r = s.execute("embroidery.breakApart", &json!({})).unwrap();
    let mut ids: Vec<u64> = r["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    ids.sort();
    assert_eq!(ids, vec![a.0, b.0, bar.0]);
    let st = s.doc().unwrap();
    assert!(st.doc.node(g).is_none());
    assert_eq!(st.doc.parent_of(a), st.doc.parent_of(bar));
    assert!(st.doc.position(a).unwrap().1 < st.doc.position(b).unwrap().1 && st.doc.position(b).unwrap().1 < st.doc.position(bar).unwrap().1);
    assert_eq!(records(&s, sf_core::Cmd::Trim), trims);
    // grouping first and branching the group marks that group
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let g2 = NodeId(s.execute("object.group", &json!({})).unwrap()["id"].as_u64().unwrap());
    assert_eq!(s.execute("embroidery.branch", &json!({})).unwrap()["id"], g2.0);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(g2).unwrap().embroidery.is_none());
}

#[test]
fn break_apart_gives_each_part_its_own_object() {
    let mut s = session();
    // a square with a hole, and a second square beside it
    let d = "M10 10 H110 V110 H10 Z M30 30 V90 H90 V30 Z M150 10 H250 V110 H150 Z";
    let id = stitched_path(&mut s, d, "fill");
    s.execute("embroidery.setThread", &json!({"color": "#cc2233"})).unwrap();
    let r = s.execute("embroidery.breakApart", &json!({})).unwrap();
    let ids: Vec<NodeId> = r["ids"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    assert_eq!(ids.len(), 2);
    let st = s.doc().unwrap();
    assert!(st.doc.node(id).is_none());
    let subpaths: Vec<usize> = ids.iter().map(|i| st.doc.node(*i).unwrap().path_data().unwrap().subpaths.len()).collect();
    assert_eq!(subpaths, vec![2, 1], "the hole stays with its square");
    for i in &ids {
        let e = st.doc.node(*i).unwrap().embroidery.clone().unwrap();
        assert_eq!(e.stitch.id(), "fill");
        assert_eq!(e.thread.unwrap().color, Rgb::new(0xcc, 0x22, 0x33));
        assert!(!s.embroidery_plan().unwrap().records_of(*i).is_empty());
    }
    // a single part has nothing to break apart
    s.execute("select.set", &json!({"ids": [ids[1].0]})).unwrap();
    assert!(s.execute("embroidery.breakApart", &json!({})).is_err());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(id).is_some());
}

#[test]
fn the_automatic_knife_cuts_shapes_into_pieces_that_turn_their_own_way() {
    let area = |s: &Session, id: NodeId| -> f64 {
        vectorcraft_embroidery::regions_mm(s.doc().unwrap().doc.node(id).unwrap()).iter().map(sf_core::geom::Polygon::area).sum()
    };
    let mut s = session();
    // a "T" filled as one shape: cut where the stem meets the bar
    let t = stitched_path(&mut s, "M10 10 H210 V50 H130 V210 H90 V50 H10 Z", "fill");
    s.execute("embroidery.setThread", &json!({"color": "#2255aa"})).unwrap();
    let whole = area(&s, t);
    let r = s.execute("embroidery.knife", &json!({})).unwrap();
    let ids: Vec<NodeId> = r["ids"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    let [g] = ids.as_slice() else { panic!("{r}") };
    let st = s.doc().unwrap();
    assert!(st.doc.node(t).is_none());
    let group = st.doc.node(*g).unwrap();
    assert_eq!(group.embroidery.as_deref().unwrap().stitch.id(), "branch");
    let pieces: Vec<NodeId> = group.children().unwrap().iter().map(|c| c.id).collect();
    assert!(pieces.len() >= 2, "{}", pieces.len());
    for p in &pieces {
        let e = emb(&s, *p);
        assert_eq!(e.stitch.id(), "fill");
        assert!(e.auto_direction && e.guides.is_empty());
        assert_eq!(e.thread.unwrap().color, Rgb::new(0x22, 0x55, 0xaa));
        assert!(!s.embroidery_plan().unwrap().records_of(*p).is_empty());
    }
    // the pieces cover the T, overlapping a little along the cuts
    let cut: f64 = pieces.iter().map(|p| area(&s, *p)).sum();
    assert!(cut > whole * 0.98 && cut < whole * 1.08, "{cut} {whole}");
    // undo brings the T back
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(t).is_some() && s.doc().unwrap().doc.node(*g).is_none());
    // a square ring is cut round its hole, without overlap if asked
    let ring = stitched_path(&mut s, "M300 10 H500 V210 H300 Z M340 50 V170 H460 V50 Z", "fill");
    let whole = area(&s, ring);
    let r = s.execute("embroidery.knife", &json!({"overlap": 0})).unwrap();
    let g = NodeId(r["ids"][0].as_u64().unwrap());
    let pieces: Vec<NodeId> = s.doc().unwrap().doc.node(g).unwrap().children().unwrap().iter().map(|c| c.id).collect();
    assert!(pieces.len() >= 2 && pieces.iter().all(|p| s.doc().unwrap().doc.node(*p).unwrap().path_data().unwrap().subpaths.len() == 1));
    let cut: f64 = pieces.iter().map(|p| area(&s, *p)).sum();
    assert!(cut > whole * 0.97 && cut <= whole * 1.001, "{cut} {whole}");
    // a plain square has nothing to cut; neither has a line
    rect(&mut s, 10.0, 300.0, 100.0, 100.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert!(s.execute("embroidery.knife", &json!({})).is_err());
    stitched_path(&mut s, "M10 450 L300 450", "run");
    assert!(s.execute("embroidery.knife", &json!({})).is_err());
    assert!(s.execute("embroidery.knife", &json!({"ids": [g.0], "overlap": 5})).is_err());
}

#[test]
fn fills_sew_their_rows_along_flow_lines() {
    let mut s = session();
    let a = rect(&mut s, 50.0, 50.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let straight = needles(&s, a);
    // two waves above and below the fill: its rows flow from one to the other
    let top = NodeId(s.execute("path.create", &json!({"d": "M 30 40 C 100 0 200 80 270 40"})).unwrap()["id"].as_u64().unwrap());
    let bottom = NodeId(s.execute("path.create", &json!({"d": "M 30 210 C 100 170 200 250 270 210"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [a.0, top.0, bottom.0]})).unwrap();
    let r = s.execute("embroidery.fillAlongLines", &json!({})).unwrap();
    assert_eq!((r["ids"].clone(), r["lines"].clone()), (json!([a.0]), json!(2)));
    let st = s.doc().unwrap();
    assert!(st.doc.node(top).is_none() && st.doc.node(bottom).is_none(), "the lines are used up");
    assert_eq!(emb(&s, a).flow.len(), 2);
    let flowing = needles(&s, a);
    assert!(flowing.len() > 300 && flowing != straight);
    // the rows bend: far fewer stitches run the one way the straight rows all do
    let angles = |pts: &[sf_core::Point]| -> Vec<f64> {
        pts.windows(2)
            .filter(|w| w[0].distance(w[1]) > 2.0)
            .map(|w| (w[1].y - w[0].y).atan2(w[1].x - w[0].x).to_degrees().rem_euclid(180.0))
            .collect()
    };
    let (bent, flat) = (angles(&flowing), angles(&straight));
    let near = |a: f64, d: f64| {
        let x = (a - d).rem_euclid(180.0);
        x.min(180.0 - x) < 2.0
    };
    let mode = (0..180).map(f64::from).max_by_key(|d| flat.iter().filter(|a| near(**a, *d)).count()).unwrap();
    let share = |v: &[f64]| v.iter().filter(|a| near(**a, mode)).count() as f64 / v.len().max(1) as f64;
    assert!(share(&bent) < share(&flat) * 0.5, "{} {} at {mode}°", share(&bent), share(&flat));
    // they move with the fill
    let before = emb(&s, a).flow[0][0];
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 10, "dy": 0})).unwrap();
    assert!((emb(&s, a).flow[0][0].x - before.x - 10.0).abs() < 1e-6);
    // cleared, the rows run straight again
    s.execute("embroidery.clearFlowLines", &json!({})).unwrap();
    assert!(emb(&s, a).flow.is_empty());
    assert!(s.execute("embroidery.clearFlowLines", &json!({})).is_err());
    // set by an agent; one line, or none
    s.execute("embroidery.setFlowLines", &json!({"id": a.0, "lines": [[[40, 120], [150, 90], [280, 120]]]})).unwrap();
    assert_eq!(emb(&s, a).flow.len(), 1);
    assert!(needles(&s, a).len() > 300);
    for bad in [json!([[[0, 0]]]), json!([[[0, 0], [1, 1]], [[0, 0], [1, 1]], [[0, 0], [1, 1]]]), json!([[[0, 0], ["x", 1]]]), json!("x")] {
        assert!(s.execute("embroidery.setFlowLines", &json!({"id": a.0, "lines": bad})).is_err(), "{bad}");
    }
    s.execute("embroidery.setFlowLines", &json!({"id": a.0, "lines": []})).unwrap();
    assert!(emb(&s, a).flow.is_empty());
    // nothing to follow, or nothing to fill
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    assert!(s.execute("embroidery.fillAlongLines", &json!({})).is_err());
}

/// The Wave Fill: a shape's tatami rows follow a wave across it, one flow line made for it.
#[test]
fn a_wave_fill_bends_the_rows_into_waves() {
    let mut s = session();
    let a = rect(&mut s, 50.0, 50.0, 240.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let straight = needles(&s, a);
    s.execute("embroidery.waveFill", &json!({"waves": 2, "height": 8})).unwrap();
    assert_eq!(emb(&s, a).flow.len(), 1, "one wave line");
    let wavy = needles(&s, a);
    assert!(wavy.len() > 300 && wavy != straight);
    // a row's points rise and fall: within the shape, the stitches span more height along a row
    // than the 0.4 mm a straight row would; one undo puts it back
    let flow = emb(&s, a).flow[0].clone();
    let (lo, hi) = flow.iter().fold((f64::MAX, f64::MIN), |(lo, hi), q| (lo.min(q.y), hi.max(q.y)));
    assert!((hi - lo) > 6.0 * vectorcraft_embroidery::PT_PER_MM * 0.9, "the line rises and falls 8 mm: {}", hi - lo);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(emb(&s, a).flow.is_empty());
    // an unfilled shape becomes a tatami fill; bad settings refused
    let b = rect(&mut s, 50.0, 300.0, 100.0, 100.0);
    s.execute("embroidery.waveFill", &json!({"ids": [b.0], "angle": 30})).unwrap();
    assert_eq!(emb(&s, b).flow.len(), 1);
    for bad in [json!({"ids": [b.0], "waves": 0}), json!({"ids": [b.0], "height": 100})] {
        assert!(s.execute("embroidery.waveFill", &bad).is_err(), "{bad}");
    }
}

#[test]
fn auto_sequence_groups_colours_and_keeps_overlaps_in_order() {
    let mut s = session();
    let stitched = |s: &mut Session, x: f64, y: f64, color: &str| {
        let id = rect(s, x, y, 50.0, 50.0);
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": color})).unwrap();
        id
    };
    let _a = stitched(&mut s, 10.0, 10.0, "#cc2233");
    let _b = stitched(&mut s, 100.0, 10.0, "#2233cc");
    let plain = rect(&mut s, 400.0, 300.0, 20.0, 20.0); // not stitched: stays where it is
    let _c = stitched(&mut s, 200.0, 10.0, "#cc2233");
    let _d = stitched(&mut s, 300.0, 10.0, "#2233cc");
    // blue on top of red: stays on top
    let e = stitched(&mut s, 10.0, 200.0, "#cc2233");
    let f = stitched(&mut s, 30.0, 220.0, "#2233cc");
    assert_eq!(color_changes(&s), 5);
    let index = |s: &Session, id: NodeId| s.doc().unwrap().doc.position(id).unwrap().1;
    let plain_at = index(&s, plain);
    // sorting by thread colour leaves what is not stitched in its place
    s.execute("embroidery.sortByColor", &json!({})).unwrap();
    assert_eq!(color_changes(&s), 1);
    assert_eq!(index(&s, plain), plain_at);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(color_changes(&s), 5);
    let r = s.execute("embroidery.autoSequence", &json!({})).unwrap();
    assert!(r["moved"].as_u64().unwrap() > 0);
    assert_eq!(color_changes(&s), 1);
    assert!(index(&s, e) < index(&s, f));
    assert_eq!(index(&s, plain), plain_at);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(color_changes(&s), 5);
    s.execute("edit.redo", &json!({})).unwrap();
    // in order already: nothing moves
    assert_eq!(s.execute("embroidery.autoSequence", &json!({})).unwrap()["moved"], 0);
}

#[test]
fn auto_sequence_can_keep_to_some_groups() {
    let mut s = session();
    let stitched = |s: &mut Session, x: f64, color: &str| {
        let id = rect(s, x, 10.0, 30.0, 30.0);
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": color})).unwrap();
        id.0
    };
    let group = |s: &mut Session, x: f64| {
        let ids: Vec<u64> =
            ["#cc2233", "#2233cc", "#cc2233", "#2233cc"].iter().enumerate().map(|(k, c)| stitched(s, x + k as f64 * 40.0, c)).collect();
        s.execute("select.set", &json!({"ids": ids})).unwrap();
        let g = s.execute("object.group", &json!({})).unwrap();
        NodeId(g["id"].as_u64().unwrap())
    };
    let (a, b) = (group(&mut s, 10.0), group(&mut s, 300.0));
    let order = |s: &Session, g: NodeId| -> Vec<NodeId> { s.doc().unwrap().doc.node(g).unwrap().children().unwrap().iter().map(|c| c.id).collect() };
    let before_b = order(&s, b);
    assert_eq!(color_changes(&s), 7);
    let r = s.execute("embroidery.autoSequence", &json!({"ids": [a.0]})).unwrap();
    assert!(r["moved"].as_u64().unwrap() > 0);
    // group a's colours come together; group b stays as it was
    let threads = |s: &Session, g: NodeId| -> Vec<String> {
        let d = &s.doc().unwrap().doc;
        order(s, g).iter().map(|id| d.node(*id).unwrap().embroidery.as_ref().unwrap().thread.as_ref().unwrap().color.to_hex()).collect()
    };
    let a_threads = threads(&s, a);
    assert_eq!(a_threads.windows(2).filter(|w| w[0] != w[1]).count(), 1, "{a_threads:?}");
    assert!(color_changes(&s) < 7);
    assert_eq!(order(&s, b), before_b);
    assert!(s.execute("embroidery.autoSequence", &json!({"ids": [u64::MAX]})).is_err());
}

#[test]
fn branch_travel_finds_its_way_round_corners() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // two squares joined by an L sewn after them: the straight way between them is bare
    let a = rect(&mut s, 50.0, 50.0, 60.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let b = rect(&mut s, 200.0, 200.0, 60.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let l = stitched_path(&mut s, "M65 60 H115 V215 H210 V245 H65 Z", "fill");
    // the first square's rows end at its corner (110, 110), under the L
    s.execute("embroidery.setBeads", &json!({"id": a.0, "exit": [110.0, 110.0]})).unwrap();
    s.execute("embroidery.setBeads", &json!({"id": b.0, "entry": [200.0, 230.0]})).unwrap();
    let trims = records(&s, sf_core::Cmd::Trim);
    s.execute("select.set", &json!({"ids": [a.0, b.0, l.0]})).unwrap();
    s.execute("embroidery.branch", &json!({})).unwrap();
    assert!(trims >= 2, "{trims}");
    assert_eq!(records(&s, sf_core::Cmd::Trim), 1);
    let plan = s.embroidery_plan().unwrap();
    let (end, start) = (plan.records_of(a).last().unwrap().end, plan.records_of(b)[0].start);
    let travel: Vec<sf_core::Point> = plan.pattern().stitches[end..start].iter().map(|r| r.pos()).collect();
    assert!(travel.len() >= 20, "{}", travel.len());
    // under the L (or the square it leads into), but for the last stitch's slack
    let under = |p: &sf_core::Point| {
        let (x, y) = (p.x * PT, p.y * PT);
        let vertical = (65.0..=115.0).contains(&x) && (60.0..=245.0).contains(&y);
        let horizontal = (65.0..=210.0).contains(&x) && (215.0..=245.0).contains(&y);
        let square = (200.0..=260.0).contains(&x) && (200.0..=260.0).contains(&y);
        vertical || horizontal || square
    };
    assert!(travel.iter().all(under), "{travel:?}");
    assert!(travel.windows(2).all(|w| w[0].distance(w[1]) < 2.6));
}

fn emb(s: &Session, id: NodeId) -> sf_core::attr::Embroidery {
    s.doc().unwrap().doc.node(id).unwrap().embroidery.as_deref().cloned().unwrap()
}

fn fill_params(s: &Session, id: NodeId) -> sf_core::model::FillParams {
    match emb(s, id).stitch {
        sf_core::attr::StitchType::Fill(f) => f,
        other => panic!("{other:?}"),
    }
}

#[test]
fn new_objects_take_the_current_properties() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#cc2233"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"spacing": 0.3, "pullComp": 0.5, "removeUnderlying": true}})).unwrap();
    assert_eq!(s.execute("embroidery.makeCurrent", &json!({})).unwrap()["type"], "fill");
    // a new fill takes them (not the thread); a satin does not
    let b = rect(&mut s, 150.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let f = fill_params(&s, b);
    assert_eq!((f.spacing, f.pull_comp), (0.3, 0.5));
    assert!(emb(&s, b).remove_underlying && emb(&s, b).thread.is_none());
    let c = rect(&mut s, 300.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let sf_core::attr::StitchType::Satin(sp) = emb(&s, c).stitch else { panic!() };
    assert_eq!(sp.spacing, sf_core::model::SatinParams::default().spacing);
    // changing an object's type gives it the type's stitch parameters, keeping the rest
    s.execute("embroidery.setThread", &json!({"color": "#2233cc"})).unwrap();
    s.execute("embroidery.set", &json!({"type": "tatami"})).unwrap();
    assert_eq!(fill_params(&s, c).spacing, 0.3);
    assert!(emb(&s, c).thread.is_some() && !emb(&s, c).remove_underlying);
    // with nothing selected, the panel edits the properties new objects take
    s.execute("select.none", &json!({})).unwrap();
    s.execute("embroidery.current.setParams", &json!({"type": "satin", "params": {"spacing": 0.25}})).unwrap();
    let d = rect(&mut s, 10.0, 100.0, 100.0, 30.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let sf_core::attr::StitchType::Satin(sp) = emb(&s, d).stitch else { panic!() };
    assert_eq!(sp.spacing, 0.25);
    // a run style that is a type of its own files the properties under it
    let r = s.execute("embroidery.current.setParams", &json!({"type": "run", "params": {"kind": "Triple", "stitchLength": 3.0}})).unwrap();
    assert_eq!(r["type"], "bean");
    let current = s.execute("embroidery.current", &json!({})).unwrap()["current"].clone();
    let mut keys: Vec<&str> = current.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["bean", "fill", "satin"]);
    // bad requests change nothing
    for p in [json!({"type": "manual", "params": {}}), json!({"type": "fill", "params": {"spacing": "dense"}}), json!({"type": "fill"})] {
        assert!(s.execute("embroidery.current.setParams", &p).is_err(), "{p}");
    }
    // reset one type, then all; undo brings them back
    assert_eq!(s.execute("embroidery.current.reset", &json!({"type": "tatami"})).unwrap()["reset"], 1);
    let e = rect(&mut s, 150.0, 100.0, 100.0, 30.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert_eq!(fill_params(&s, e).spacing, sf_core::model::FillParams::default().spacing);
    assert_eq!(s.execute("embroidery.current.reset", &json!({})).unwrap()["reset"], 2);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.execute("embroidery.current", &json!({})).unwrap()["current"].as_object().unwrap().len(), 2);
    let _ = a;
}

#[test]
fn styles_keep_objects_alike() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"spacing": 0.3}})).unwrap();
    assert_eq!(s.execute("embroidery.style.new", &json!({"name": "Dense"})).unwrap()["name"], "Dense");
    assert_eq!(emb(&s, a).style.as_deref(), Some("Dense"));
    assert!(s.execute("embroidery.style.new", &json!({"name": "Dense"})).is_err(), "names are unique");
    // two satins take the style, keeping their threads
    let b = rect(&mut s, 150.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#2233cc"})).unwrap();
    let c = rect(&mut s, 300.0, 10.0, 100.0, 50.0);
    s.execute("select.set", &json!({"ids": [b.0, c.0]})).unwrap();
    s.execute("embroidery.style.apply", &json!({"name": "Dense"})).unwrap();
    for id in [b, c] {
        assert_eq!(fill_params(&s, id).spacing, 0.3);
        assert_eq!(emb(&s, id).style.as_deref(), Some("Dense"));
    }
    assert!(emb(&s, b).thread.is_some());
    let list = s.execute("embroidery.style.list", &json!({})).unwrap();
    assert_eq!(list["styles"][0]["uses"], 3);
    assert_eq!(list["styles"][0]["type"], "fill");
    // redefining from a changed object changes every object that follows the style
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"spacing": 0.5, "pullComp": 0.4}})).unwrap();
    assert_eq!(s.execute("embroidery.style.redefine", &json!({"name": "Dense"})).unwrap()["updated"], 3);
    for id in [a, b, c] {
        let f = fill_params(&s, id);
        assert_eq!((f.spacing, f.pull_comp), (0.5, 0.4));
    }
    // rename, duplicate, release, delete
    s.execute("embroidery.style.rename", &json!({"name": "Dense", "to": "Firm"})).unwrap();
    assert_eq!(emb(&s, c).style.as_deref(), Some("Firm"));
    assert_eq!(s.execute("embroidery.style.duplicate", &json!({"name": "Firm"})).unwrap()["name"], "Firm copy");
    assert!(s.execute("embroidery.style.rename", &json!({"name": "Firm", "to": "Firm copy"})).is_err());
    s.execute("select.set", &json!({"ids": [b.0]})).unwrap();
    s.execute("embroidery.style.unlink", &json!({})).unwrap();
    assert!(emb(&s, b).style.is_none());
    assert!(s.execute("embroidery.style.unlink", &json!({})).is_err(), "follows none now");
    assert_eq!(s.execute("embroidery.style.delete", &json!({"name": "Firm"})).unwrap()["released"], 2);
    assert!(emb(&s, a).style.is_none());
    assert_eq!(fill_params(&s, a).spacing, 0.5, "objects keep the properties");
    assert!(s.execute("embroidery.style.apply", &json!({"name": "Firm"})).is_err());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(emb(&s, a).style.as_deref(), Some("Firm"));
}

#[test]
fn fabrics_can_be_made_for_the_design() {
    const WOVEN: &str = "Woven (twill, poplin)";
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.fabric.save", &json!({"name": "Canvas", "from": WOVEN, "fillSpacing": 0.5, "topping": true})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert!(info["fabrics"].as_array().unwrap().contains(&json!("Canvas")));
    let canvas = info["fabricDetails"].as_array().unwrap().iter().find(|f| f["name"] == "Canvas").unwrap().clone();
    assert_eq!((canvas["custom"].clone(), canvas["fillSpacing"].clone(), canvas["topping"].clone()), (json!(true), json!(0.5), json!(true)));
    s.execute("embroidery.applyFabric", &json!({"name": "Canvas"})).unwrap();
    assert_eq!(fill_params(&s, a).spacing, 0.5);
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["fabric"], "Canvas");
    // changing it keeps what was not given
    s.execute("embroidery.fabric.save", &json!({"name": "Canvas", "pullComp": 0.25})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    let canvas = info["fabricDetails"].as_array().unwrap().iter().find(|f| f["name"] == "Canvas").unwrap().clone();
    assert_eq!((canvas["fillSpacing"].clone(), canvas["pullComp"].clone()), (json!(0.5), json!(0.25)));
    // built-in names, bad values and unknown fabrics are refused
    for p in [
        json!({"name": WOVEN, "fillSpacing": 0.5}),
        json!({"name": "Thin", "fillSpacing": 9.0}),
        json!({"name": "Thin", "topping": "yes"}),
        json!({"name": "Thin", "from": "Velvet"}),
        json!({"name": " "}),
    ] {
        assert!(s.execute("embroidery.fabric.save", &p).is_err(), "{p}");
    }
    assert!(s.execute("embroidery.fabric.delete", &json!({"name": WOVEN})).is_err());
    s.execute("embroidery.fabric.delete", &json!({"name": "Canvas"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert!(!info["fabrics"].as_array().unwrap().contains(&json!("Canvas")));
    assert_eq!(info["fabric"], "");
}

#[test]
fn columns_are_clicked_side_by_side_and_continued() {
    let mut s = session();
    // pairs of points across a column turning a corner (two smooth ones)
    let pts = json!([[50, 50], [50, 80], [150, 50], [150, 80], [200, 110, true], [175, 125, true], [230, 190], [200, 200]]);
    let id = NodeId(s.execute("embroidery.column", &json!({"points": pts})).unwrap()["id"].as_u64().unwrap());
    {
        let st = s.doc().unwrap();
        let n = st.doc.node(id).unwrap();
        let subs = &n.path_data().unwrap().subpaths;
        assert_eq!((subs.len(), subs[0].anchors.len(), subs[1].anchors.len()), (2, 4, 4));
        assert_eq!(subs[0].anchors[2].kind, vectorcraft_geom::path::AnchorKind::Smooth);
        let e = n.embroidery.as_deref().unwrap();
        assert_eq!((e.stitch.id(), e.guides.len()), ("satin", 4));
        assert_eq!(st.selection.objects, vec![id]);
    }
    let before = needles(&s, id);
    assert!(before.len() > 100, "{}", before.len());
    // continuing it: more points, more column
    let more = json!([[50, 50], [50, 80], [150, 50], [150, 80], [200, 110, true], [175, 125, true], [230, 190], [200, 200], [240, 260], [205, 262]]);
    s.execute("embroidery.column", &json!({"id": id.0, "points": more})).unwrap();
    assert!(needles(&s, id).len() > before.len() + 20);
    assert_eq!(emb(&s, id).guides.len(), 5);
    // new columns take the current satin properties
    s.execute("select.none", &json!({})).unwrap();
    s.execute("embroidery.current.setParams", &json!({"type": "satin", "params": {"spacing": 0.3}})).unwrap();
    let id2 =
        NodeId(s.execute("embroidery.column", &json!({"points": [[300, 50], [300, 80], [400, 50], [400, 80]]})).unwrap()["id"].as_u64().unwrap());
    let sf_core::attr::StitchType::Satin(sp) = emb(&s, id2).stitch else { panic!() };
    assert_eq!(sp.spacing, 0.3);
    s.execute("select.set", &json!({"ids": [id2.0]})).unwrap();
    let g = NodeId(s.execute("object.group", &json!({})).unwrap()["id"].as_u64().unwrap());
    for p in [json!({}), json!({"points": []}), json!({"points": [["a", 1]]}), json!({"points": [[1, 2]], "id": g.0})] {
        assert!(s.execute("embroidery.column", &p).is_err(), "{p}");
    }
    // a run continued with the pen keeps its stitch type and sews the longer path
    let run = stitched_path(&mut s, "M300 300 L400 300", "run");
    let n0 = needles(&s, run).len();
    s.execute("path.appendAnchor", &json!({"id": run.0, "x": 400.0, "y": 400.0})).unwrap();
    assert_eq!(emb(&s, run).stitch.id(), "run");
    assert!(needles(&s, run).len() > n0 + 10);
}

#[test]
fn what_check_design_flags_is_a_design_setting() {
    let mut s = session();
    let kinds = |s: &mut Session| -> Vec<String> {
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        r["findings"].as_array().unwrap().iter().map(|f| f["kind"].as_str().unwrap().to_string()).collect()
    };
    // a 10.6 mm satin without auto split: long stitches by the 7 mm default (clothing)
    rect(&mut s, 50.0, 50.0, 200.0, 30.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"splitLength": 0}})).unwrap();
    assert!(kinds(&mut s).contains(&"longStitches".to_string()));
    s.execute("embroidery.settings", &json!({"checks": {"longStitch": 12.0}})).unwrap();
    assert!(!kinds(&mut s).contains(&"longStitches".to_string()));
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["settings"]["checks"]["longStitch"], 12.0);
    assert_eq!(info["settings"]["checks"]["denseLayers"], 4.5);
    // lower the density limit far enough and the satin itself is dense
    s.execute("embroidery.settings", &json!({"checks": {"denseLayers": 1.5}})).unwrap();
    assert!(kinds(&mut s).contains(&"dense".to_string()));
    for p in [json!({"checks": {"longStitch": 50}}), json!({"checks": {"tinyObject": "big"}}), json!({"checks": 5})] {
        assert!(s.execute("embroidery.settings", &p).is_err(), "{p}");
    }
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(!kinds(&mut s).contains(&"dense".to_string()));
}

#[test]
fn tie_styles_and_omitting_the_last_stitch() {
    let mut s = session();
    let id = stitched_path(&mut s, "M20 100 L300 100", "run");
    let count = |s: &Session| s.embroidery_plan().unwrap().pattern().stitches.iter().filter(|r| r.cmd == sf_core::Cmd::Stitch).count();
    let double = count(&s);
    // a tie-in and a tie-off: two stitches less each sewn singly, one less as triangles
    s.execute("embroidery.settings", &json!({"tieStyle": "single"})).unwrap();
    assert_eq!(count(&s), double - 4);
    s.execute("embroidery.settings", &json!({"tieStyle": "triangle"})).unwrap();
    assert_eq!(count(&s), double - 2);
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["settings"]["tieStyle"], "triangle");
    // a triangle's corner leaves the line
    let plan = s.embroidery_plan().unwrap();
    let ys: Vec<f64> = plan.pattern().stitches.iter().filter(|r| r.cmd == sf_core::Cmd::Stitch).map(|r| r.y).collect();
    assert!(ys.iter().any(|y| (y - ys[0]).abs() > 0.2), "{ys:?}");
    assert!(s.execute("embroidery.settings", &json!({"tieStyle": "knot"})).is_err());
    s.execute("embroidery.settings", &json!({"tieStyle": "double"})).unwrap();
    // omit last stitch: one needle point less at the end
    let last = |s: &Session| *needles(s, id).last().unwrap();
    let (before, end) = (needles(&s, id).len(), last(&s));
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"omitLast": true}})).unwrap();
    assert!(emb(&s, id).omit_last);
    assert_eq!(needles(&s, id).len(), before - 1);
    assert!(last(&s).distance(end) > 0.5);
}

#[test]
fn threads_are_matched_to_a_chart() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 100.0, 50.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#c81e2c"})).unwrap();
    let b = stitched_path(&mut s, "M20 100 L300 100", "run");
    s.execute("select.none", &json!({})).unwrap();
    let r = s.execute("embroidery.matchThreads", &json!({"chart": "Generic"})).unwrap();
    assert_eq!(r["matched"], 2);
    let chart = sf_core::threads::charts().into_iter().find(|c| c.name == "Generic").unwrap();
    for id in [a, b] {
        let t = emb(&s, id).thread.unwrap();
        assert!(chart.threads.contains(&t), "{t:?}");
    }
    // the art is painted the thread's colour
    let t = emb(&s, a).thread.unwrap();
    let fill = s.doc().unwrap().doc.node(a).unwrap().appearance.fill_paint().clone();
    assert_eq!(fill, vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(t.color.r, t.color.g, t.color.b)));
    assert!(s.execute("embroidery.matchThreads", &json!({"chart": "Velvet"})).is_err());
}

#[test]
fn an_objects_stitches_split_in_two() {
    let mut s = session();
    let id = rect(&mut s, 50.0, 50.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let all = own(&mut s, id).1;
    let total = stitches(&s);
    let r = s.execute("embroidery.stitches.split", &json!({"id": id.0, "at": 100})).unwrap();
    let ids: Vec<NodeId> = r["ids"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    assert_eq!((ids.len(), ids[0]), (2, id));
    // the object keeps the first 100 records, the copy sewn after it the rest; both frozen
    let (frozen_a, a) = own(&mut s, ids[0]);
    let (frozen_b, b) = own(&mut s, ids[1]);
    assert!(frozen_a && frozen_b);
    assert_eq!(a, all[..100].to_vec());
    assert_eq!(b, all[100..].to_vec());
    let st = s.doc().unwrap();
    assert_eq!(st.doc.position(ids[1]).unwrap().1, st.doc.position(id).unwrap().1 + 1);
    assert_eq!(st.selection.objects, ids);
    // the same stitches are sewn (bar the ties and the connection between the parts)
    assert!(stitches(&s).abs_diff(total) < 20, "{} vs {total}", stitches(&s));
    for at in [0, 1, all.len() - 1, all.len() + 5] {
        assert!(s.execute("embroidery.stitches.split", &json!({"id": id.0, "at": at})).is_err(), "{at}");
    }
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(ids[1]).is_none());
    assert!(!own(&mut s, id).0);
}

/// A drawing like Design from Prompt's: a frame painted with nothing, a head with two eyes and a
/// mouth on top, a speck and a linked image.
const PROMPT_ART: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000">
<rect width="1000" height="1000" fill="none"/>
<circle cx="500" cy="500" r="400" fill="#e07020"/>
<ellipse cx="380" cy="420" rx="70" ry="90" fill="#ffffff"/>
<ellipse cx="620" cy="420" rx="70" ry="90" fill="#fefefe"/>
<path d="M300 640 Q500 780 700 640 L690 690 Q500 830 310 690 Z" fill="#101010"/>
<circle cx="500" cy="300" r="3" fill="#101010"/>
<image href="/etc/hosts" x="0" y="0" width="10" height="10"/>
</svg>"##;

#[test]
fn place_art_places_stitches_and_threads_its_shapes_in_one_step() {
    let mut s = session();
    let depth = s.doc().unwrap().history.undo.len();
    let r = s.execute("embroidery.placeArt", &json!({"svg": PROMPT_ART, "size": 80.0, "name": "Fox"})).unwrap();
    assert_eq!(r["objects"], 4, "{r}");
    // (the SVG reader already leaves out the frame, painted with nothing)
    assert_eq!(r["removed"], 2, "the image and the speck: {r}");
    assert_eq!(r["fill"].as_u64().unwrap() + r["satin"].as_u64().unwrap(), 4, "{r}");
    assert_eq!(r["colors"], 3, "the two whites are one thread: {r}");
    let gid = NodeId(r["id"].as_u64().unwrap());
    {
        let st = s.doc().unwrap();
        let g = st.doc.node(gid).unwrap();
        assert_eq!(g.name.as_deref(), Some("Fox"));
        assert_eq!(st.selection.objects, vec![gid]);
        // 80 mm is the stitched art's longest side (not the unpainted frame's), centred on the artboard
        let b = g.geometric_bounds().unwrap();
        let mm = |pt: f64| pt * vectorcraft_embroidery::MM_PER_PT;
        assert!((mm(b.width().max(b.height())) - 80.0).abs() < 0.5, "{b:?}");
        let board = st.doc.artboards[0].rect.center();
        assert!((b.center().x - board.x).abs() < 1.0, "{b:?}");
        // drawing order is sewing order; each shape has a chart thread and cuts what it covers out of
        // the fills sewn before it
        let kids = g.children().unwrap();
        assert_eq!(kids.len(), 4);
        for k in kids {
            let e = k.embroidery.as_deref().unwrap();
            assert!(e.thread.as_ref().is_some_and(|t| !t.code.is_empty()), "{e:?}");
            assert!(e.remove_underlying);
        }
        let first = vectorcraft_embroidery::node_thread(&kids[0], kids[0].embroidery.as_deref().unwrap());
        assert!(first.color.r > 180 && first.color.b < 100, "the head is sewn first: {first:?}");
    }
    assert!(stitches(&s) > 500, "{}", stitches(&s));
    // one undo step
    assert_eq!(s.doc().unwrap().history.undo.len(), depth + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(gid).is_none());
    assert_eq!(stitches(&s), 0);
}

#[test]
fn place_art_fits_the_hoop_and_refuses_what_it_cannot_stitch() {
    let mut s = session();
    s.execute("embroidery.settings", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    let r = s.execute("embroidery.placeArt", &json!({"svg": PROMPT_ART})).unwrap();
    let b = s.doc().unwrap().doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap().geometric_bounds().unwrap();
    assert!((b.width() * vectorcraft_embroidery::MM_PER_PT - 117.0).abs() < 0.5, "90% of the hoop's shorter side: {b:?}");
    let before = serde_json::to_string(&s.doc().unwrap().doc).unwrap();
    let depth = s.doc().unwrap().history.undo.len();
    let image_only = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><image href="a.png" width="10" height="10"/></svg>"#;
    let specks_only = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000"><circle cx="5" cy="5" r="2" fill="#000"/><circle cx="995" cy="995" r="2" fill="#000"/></svg>"##;
    for p in [
        json!({}),
        json!({"svg": "a fox"}),
        json!({"svg": "<svg"}),
        json!({"svg": image_only}),
        json!({"svg": specks_only, "size": 100}),
        json!({"svg": PROMPT_ART, "size": 0}),
        json!({"svg": PROMPT_ART, "size": "big"}),
        json!({"svg": PROMPT_ART, "at": [1, "x"]}),
        json!({"svg": PROMPT_ART, "at": [1e308, 0]}),
        json!({"svg": format!("<svg>{}</svg>", " ".repeat(sf_core::ai::MAX_SVG))}),
    ] {
        assert!(s.execute("embroidery.placeArt", &p).is_err(), "{p}");
        assert_eq!(serde_json::to_string(&s.doc().unwrap().doc).unwrap(), before, "{p}: nothing changes");
        assert_eq!(s.doc().unwrap().history.undo.len(), depth, "{p}");
    }
}

#[test]
fn place_art_can_land_on_an_images_frame() {
    let mut s = session();
    // a page twice as wide as tall, the circle in its left half
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 500"><circle cx="250" cy="250" r="200" fill="#d02020"/></svg>"##;
    let r = s.execute("embroidery.placeArt", &json!({"svg": svg, "frame": [100, 100, 200, 100], "size": 300})).unwrap();
    let b = s.doc().unwrap().doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap().geometric_bounds().unwrap();
    // the page maps onto the frame (0.2 a unit), so the circle is at (150, 150) with radius 40
    for (got, want) in [(b.x0, 110.0), (b.y0, 110.0), (b.x1, 190.0), (b.y1, 190.0)] {
        assert!((got - want).abs() < 0.5, "{b:?}");
    }
    for bad in [json!([0, 0, 0, 10]), json!([0, 0, 10]), json!([1e308, 0, 10, 10]), json!("x")] {
        assert!(s.execute("embroidery.placeArt", &json!({"svg": svg, "frame": bad})).is_err(), "{bad}");
    }
}

#[test]
fn a_texture_is_a_tatami_stitch_parameter() {
    use sf_core::model::TexturePattern;
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 150.0, 100.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a);
    s.execute("embroidery.setParams", &json!({"params": {"texture": {"pattern": "waves"}}})).unwrap();
    let t = fill_params(&s, a).texture.unwrap();
    assert_eq!((t.pattern, t.size, t.angle), (TexturePattern::Waves, 6.0, 0.0));
    let waves = needles(&s, a);
    assert_ne!(waves, plain);
    // a change of size keeps the pattern
    s.execute("embroidery.setParams", &json!({"params": {"texture": {"size": 9}}})).unwrap();
    let t = fill_params(&s, a).texture.unwrap();
    assert_eq!((t.pattern, t.size), (TexturePattern::Waves, 9.0));
    assert!(s.execute("embroidery.setParams", &json!({"params": {"texture": {"pattern": "plaid"}}})).is_err());
    s.execute("embroidery.setParams", &json!({"params": {"texture": null}})).unwrap();
    assert!(fill_params(&s, a).texture.is_none());
    assert_eq!(needles(&s, a), plain);
}

/// How many of `pts` (mm) lie on the ellipse round `c` (pt) with radii `r` (pt).
fn on_ellipse(pts: &[sf_core::Point], c: (f64, f64), r: (f64, f64)) -> usize {
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    pts.iter()
        .filter(|p| {
            let (dx, dy) = ((p.x - c.0 * MM) / (r.0 * MM), (p.y - c.1 * MM) / (r.1 * MM));
            ((dx * dx + dy * dy).sqrt() - 1.0).abs() * r.1.min(r.0) * MM < 0.05
        })
        .count()
}

#[test]
fn carving_presses_a_shapes_outline_into_the_fill_below() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a);
    let stamp = NodeId(s.execute("shape.ellipse", &json!({"x": 60, "y": 40, "width": 90, "height": 80})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [a.0, stamp.0]})).unwrap();
    let r = s.execute("embroidery.carveSplits", &json!({})).unwrap();
    assert_eq!((r["ids"].clone(), r["lines"].as_u64()), (json!([a.0]), Some(1)));
    assert!(s.doc().unwrap().doc.node(stamp).is_none(), "the stamp is used up");
    let lines = emb(&s, a).splits;
    assert_eq!(lines.len(), 1);
    let (first, last) = (lines[0][0], *lines[0].last().unwrap());
    assert!(first.distance(last) < 1e-9, "a closed outline");
    // The rows have needle points along the outline now.
    let carved = needles(&s, a);
    let (before, after) = (on_ellipse(&plain, (105.0, 80.0), (45.0, 40.0)), on_ellipse(&carved, (105.0, 80.0), (45.0, 40.0)));
    assert!(after > before + 100, "{before} → {after}");
    // The lines move with the fill.
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 10, "dy": 0})).unwrap();
    assert!((emb(&s, a).splits[0][0].x - first.x - 10.0).abs() < 1e-6);
    // Undo brings back the stamp; Clear Split Lines takes the lines out.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(stamp).is_some() && emb(&s, a).splits.is_empty());
    s.execute("edit.redo", &json!({})).unwrap();
    s.execute("embroidery.clearSplits", &json!({"ids": [a.0]})).unwrap();
    assert!(emb(&s, a).splits.is_empty());
    assert_eq!(needles(&s, a), plain);
    assert!(s.execute("embroidery.clearSplits", &json!({"ids": [a.0]})).is_err(), "nothing to clear");
}

#[test]
fn stamps_from_the_library_press_into_fills() {
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a);
    // the library
    let list = s.execute("embroidery.stamps", &json!({})).unwrap();
    assert_eq!(list["stamps"].as_array().unwrap().len(), sf_core::stamps::Stamp::ALL.len());
    // a star in the middle, 60% as wide as the fill is high
    let r = s.execute("embroidery.carveStamp", &json!({"stamp": "star"})).unwrap();
    assert_eq!((r["ids"].clone(), r["lines"].as_u64()), (json!([a.0]), Some(1)));
    let lines = emb(&s, a).splits;
    let (lo, hi) = lines[0].iter().fold(((f64::INFINITY, f64::INFINITY), (f64::NEG_INFINITY, f64::NEG_INFINITY)), |(lo, hi), p| {
        ((lo.0.min(p.x), lo.1.min(p.y)), (hi.0.max(p.x), hi.1.max(p.y)))
    });
    assert!(((hi.1 - lo.1) - 0.6 * 150.0 * 0.91).abs() < 2.0, "{lo:?} {hi:?}");
    assert!((((lo.0 + hi.0) * 0.5) - 110.0).abs() < 1.0, "{lo:?} {hi:?}");
    assert_ne!(needles(&s, a), plain);
    // where asked, as big as asked, turned
    s.execute("embroidery.carveStamp", &json!({"stamp": "leaf", "at": [60, 60], "size": 20, "angle": 30})).unwrap();
    let lines = emb(&s, a).splits;
    assert_eq!(lines.len(), 1 + sf_core::stamps::Stamp::Leaf.lines().len());
    let far = lines[1..].iter().flatten().map(|p| p.distance(sf_core::Point::new(60.0, 60.0)) * MM).fold(0.0, f64::max);
    assert!((9.0..=10.0 + 1e-6).contains(&far), "{far}");
    // undo takes it out again
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(emb(&s, a).splits.len(), 1);
    // bad stamps and sizes, and things that are not tatami fills
    for bad in [
        json!({}),
        json!({"stamp": "nope"}),
        json!({"stamp": "star", "size": 0.5}),
        json!({"stamp": "star", "at": [1]}),
        json!({"stamp": "star", "angle": "x"}),
    ] {
        assert!(s.execute("embroidery.carveStamp", &bad).is_err(), "{bad}");
    }
    s.execute("embroidery.set", &json!({"type": "satinfill"})).unwrap();
    assert!(s.execute("embroidery.carveStamp", &json!({"stamp": "star"})).is_err());
}

#[test]
fn contour_split_lines_follow_the_outline_inward() {
    let mut s = session();
    // 200 × 150 pt: 70.6 × 52.9 mm, so rings 5 mm apart run out after five
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a);
    let r = s.execute("embroidery.contourSplits", &json!({"spacing": 5})).unwrap();
    assert_eq!(r["lines"].as_u64(), Some(5), "{r}");
    let lines = emb(&s, a).splits;
    assert!(lines.iter().all(|l| l.first() == l.last() && l.len() >= 5));
    // the first ring is 5 mm in from the left edge
    let left = lines[0].iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    assert!((left - (10.0 + 5.0 * vectorcraft_embroidery::PT_PER_MM)).abs() < 0.5, "{left}");
    assert_ne!(needles(&s, a), plain);
    // too far apart for any ring, too close, or not a tatami fill
    assert!(s.execute("embroidery.contourSplits", &json!({"spacing": 100})).is_err());
    assert!(s.execute("embroidery.contourSplits", &json!({"spacing": 0.1})).is_err());
    s.execute("embroidery.set", &json!({"type": "satinfill"})).unwrap();
    assert!(s.execute("embroidery.contourSplits", &json!({})).is_err());
}

#[test]
fn a_repeated_line_carves_flexi_splits_across_the_fill() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let wave = NodeId(s.execute("path.create", &json!({"d": "M 0 50 C 60 20 120 80 230 50"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [a.0, wave.0]})).unwrap();
    assert!(s.execute("embroidery.carveSplits", &json!({"repeat": 0.1})).is_err());
    let r = s.execute("embroidery.carveSplits", &json!({"repeat": 5, "keep": true})).unwrap();
    // 150 pt (53 mm) of fill, a copy every 5 mm, and the copies reaching in from beyond it.
    let n = r["lines"].as_u64().unwrap();
    assert!((10..=20).contains(&n), "{n}");
    assert_eq!(emb(&s, a).splits.len() as u64, n);
    assert!(s.doc().unwrap().doc.node(wave).is_some(), "kept");
    // Fills alone: the front-most carves the others.
    let b = rect(&mut s, 50.0, 50.0, 60.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let r = s.execute("embroidery.carveSplits", &json!({})).unwrap();
    assert_eq!(r["ids"], json!([a.0]));
    assert!(s.doc().unwrap().doc.node(b).is_none());
    // Nothing to carve with, or nothing to carve into.
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    assert!(s.execute("embroidery.carveSplits", &json!({})).is_err());
    s.execute("select.set", &json!({"ids": [wave.0]})).unwrap();
    assert!(s.execute("embroidery.carveSplits", &json!({})).is_err());
}

#[test]
fn split_lines_set_by_an_agent_are_checked() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a);
    for bad in [json!([[[0, 0]]]), json!([[["x", 0], [1, 1]]]), json!([[[0, 0], [null, 1]]]), json!("lines"), json!([[0, 0, 1, 1]])] {
        assert!(s.execute("embroidery.setSplits", &json!({"id": a.0, "splits": bad})).is_err(), "{bad}");
    }
    let r = s.execute("embroidery.setSplits", &json!({"id": a.0, "splits": [[[100, 0], [110, 300]], [[0, 80], [80, 20], [300, 90]]]})).unwrap();
    assert_eq!(r["splits"], 2);
    assert_ne!(needles(&s, a), plain);
    // Only tatami fills take them.
    s.execute("embroidery.set", &json!({"type": "satinfill"})).unwrap();
    assert!(s.execute("embroidery.setSplits", &json!({"id": a.0, "splits": [[[100, 0], [110, 300]]]})).is_err());
}

#[test]
fn a_thread_makes_lines_runs_and_shapes_fills() {
    use sf_core::attr::StitchType;
    let mut s = session();
    let line = NodeId(s.execute("path.create", &json!({"d": "M 20 100 C 100 50 200 150 300 100"})).unwrap()["id"].as_u64().unwrap());
    let a = rect(&mut s, 10.0, 150.0, 200.0, 150.0);
    s.execute("embroidery.setThread", &json!({"ids": [line.0, a.0], "color": "#b03a2a"})).unwrap();
    assert!(matches!(emb(&s, line).stitch, StitchType::Run(_)), "{:?}", emb(&s, line).stitch);
    assert!(matches!(emb(&s, a).stitch, StitchType::Fill(_)), "{:?}", emb(&s, a).stitch);
    assert!(!needles(&s, line).is_empty());
}

#[test]
fn a_motif_is_placed_as_a_stitched_run() {
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    let r = s.execute("embroidery.placeMotif", &json!({"motif": "heart", "size": 20, "at": [200, 150]})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    assert!(matches!(emb(&s, id).stitch, sf_core::attr::StitchType::Run(_)));
    let pts = needles(&s, id);
    assert!(pts.len() > 20, "{}", pts.len());
    // 20 mm long, centred on (200, 150) pt.
    let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
    assert!((hi - lo - 20.0).abs() < 0.5 && ((lo + hi) * 0.5 - 200.0 * MM).abs() < 0.5, "{lo}..{hi}");
    assert_eq!(s.doc().unwrap().selection.objects, vec![id]);
    for bad in [json!({"motif": "dragon"}), json!({"motif": "star", "size": 0}), json!({"motif": "star", "at": ["x", 0]}), json!({})] {
        assert!(s.execute("embroidery.placeMotif", &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_drawn_path_becomes_one_of_the_designs_motifs() {
    use sf_core::attr::StitchType;
    use sf_core::model::{FillKind, RunKind};
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let path = |s: &mut Session, d: String| NodeId(s.execute("path.create", &json!({ "d": d })).unwrap()["id"].as_u64().unwrap());
    let mut s = session();
    // A peak drawn upward, 20 mm long and 10 mm high.
    let (w, h) = (20.0 / MM, 10.0 / MM);
    let peak = path(&mut s, format!("M 100 300 L {} {} L {} 300", 100.0 + w / 2.0, 300.0 - h, 100.0 + w));
    s.execute("select.set", &json!({"ids": [peak.0]})).unwrap();
    let r = s.execute("embroidery.motif.make", &json!({})).unwrap();
    assert_eq!((r["name"].as_str(), r["points"].as_u64(), r["updated"].as_u64()), (Some("Motif 1"), Some(3), Some(0)));
    let list = s.execute("embroidery.motif.list", &json!({})).unwrap();
    assert_eq!((list["motifs"][0]["name"].as_str(), list["motifs"][0]["uses"].as_u64()), (Some("Motif 1"), Some(0)));
    assert!((list["motifs"][0]["aspect"].as_f64().unwrap() - 0.5).abs() < 1e-6, "{list}");
    // A line sews it, as high as its proportions make it for the repeat, its peaks up as drawn.
    let line = path(&mut s, "M 20 100 L 300 100".into());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.motif.apply", &json!({"name": "Motif 1"})).unwrap();
    let StitchType::Run(r) = emb(&s, line).stitch else { panic!() };
    assert_eq!((r.kind, r.custom_motif.as_ref().map(|m| m.name.as_str())), (RunKind::Motif, Some("Motif 1")));
    assert!((r.width - r.spacing * 0.5).abs() < 1e-9, "{} {}", r.width, r.spacing);
    let pts = needles(&s, line);
    let y0 = 100.0 * MM;
    assert!(pts.iter().all(|p| p.y <= y0 + 1e-6) && pts.iter().any(|p| p.y < y0 - r.width * 0.99), "{pts:?}");
    // A shape becomes a motif fill of it, rows far enough apart; a satin can't sew it.
    let a = rect(&mut s, 10.0, 150.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.motif.apply", &json!({"name": "Motif 1"})).unwrap();
    let f = fill_params(&s, a);
    assert_eq!((f.kind, f.custom_motif.as_ref().map(|m| m.points.len())), (FillKind::Motif, Some(3)));
    assert!((f.spacing - f.motif_size * 0.5 * 1.25).abs() < 1e-9, "{}", f.spacing);
    rect(&mut s, 400.0, 150.0, 20.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    assert!(s.execute("embroidery.motif.apply", &json!({"name": "Motif 1"})).is_err());
    // Place Motif and borders take its name (any case) too.
    let placed = NodeId(s.execute("embroidery.placeMotif", &json!({"motif": "motif 1", "size": 20})).unwrap()["id"].as_u64().unwrap());
    assert_eq!(s.doc().unwrap().doc.node(placed).unwrap().name.as_deref(), Some("Motif 1"));
    let placed_pts = needles(&s, placed);
    let (lo, hi) = placed_pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
    assert!((hi - lo - 10.0).abs() < 0.3, "as high as drawn: {lo}..{hi}");
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    let border =
        NodeId(s.execute("embroidery.addOutline", &json!({"stitch": "motif", "motif": "Motif 1", "width": 2.0})).unwrap()["id"].as_u64().unwrap());
    let StitchType::Run(b) = emb(&s, border).stitch else { panic!() };
    assert!(b.custom_motif.is_some() && (b.spacing - 4.0).abs() < 1e-9, "{}", b.spacing);
    // Remade under its name from another drawing, it changes in everything sewing it.
    let dip = path(&mut s, format!("M 100 400 L {} {} L {} 400", 100.0 + w / 2.0, 400.0 + h / 2.0, 100.0 + w));
    s.execute("select.set", &json!({"ids": [dip.0]})).unwrap();
    let r = s.execute("embroidery.motif.make", &json!({"name": "Motif 1"})).unwrap();
    assert_eq!(r["updated"].as_u64(), Some(3), "{r}");
    let StitchType::Run(r) = emb(&s, line).stitch else { panic!() };
    let dipped = r.custom_motif.unwrap().points;
    assert!(dipped.len() == 3 && (dipped[1][0] - 0.5).abs() < 1e-9 && (dipped[1][1] - 1.0).abs() < 1e-9, "{dipped:?}");
    assert_eq!(s.execute("embroidery.motif.list", &json!({})).unwrap()["motifs"].as_array().map(Vec::len), Some(1));
    // Deleted, it leaves the design but not what sews it; undo brings it back.
    s.execute("embroidery.motif.delete", &json!({"name": "Motif 1"})).unwrap();
    assert_eq!(s.execute("embroidery.motif.list", &json!({})).unwrap()["motifs"], json!([]));
    assert!(fill_params(&s, a).custom_motif.is_some());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.execute("embroidery.motif.list", &json!({})).unwrap()["motifs"][0]["uses"].as_u64(), Some(3));
    // Bad asks change nothing.
    for (cmd, p) in [
        ("embroidery.motif.apply", json!({"name": "Dragon"})),
        ("embroidery.motif.apply", json!({})),
        ("embroidery.motif.delete", json!({"name": "Dragon"})),
        ("embroidery.motif.make", json!({"ids": [a.0 + 1000]})),
        ("embroidery.placeMotif", json!({"motif": "Dragon"})),
    ] {
        assert!(s.execute(cmd, &p).is_err(), "{cmd} {p}");
    }
    let dot = path(&mut s, "M 10 10 L 10.1 10".into());
    s.execute("select.set", &json!({"ids": [dot.0]})).unwrap();
    assert!(s.execute("embroidery.motif.make", &json!({})).is_err(), "too small");
}

#[test]
fn redwork_sews_line_art_as_one_line_back_to_its_start() {
    use sf_core::attr::StitchType;
    use sf_core::model::RunKind;
    let mut s = session();
    let path = |s: &mut Session, d: &str| NodeId(s.execute("path.create", &json!({ "d": d })).unwrap()["id"].as_u64().unwrap());
    // A window: a square and a cross in it, drawn as three paths; one is stitched as a fill.
    let square = path(&mut s, "M 100 100 L 200 100 L 200 200 L 100 200 Z");
    let bar = path(&mut s, "M 150 100 L 150 200");
    let rail = path(&mut s, "M 100 150 L 200 150");
    s.execute("select.set", &json!({"ids": [bar.0]})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#b03a2a"})).unwrap();
    s.execute("select.set", &json!({"ids": [square.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("select.set", &json!({"ids": [square.0, bar.0, rail.0]})).unwrap();
    let before = stitches(&s);
    let g = NodeId(s.execute("embroidery.redwork", &json!({"kind": "bean"})).unwrap()["id"].as_u64().unwrap());
    let StitchType::Redwork(r) = emb(&s, g).stitch else { panic!("{:?}", emb(&s, g).stitch) };
    assert_eq!(r.kind, RunKind::Triple);
    assert_eq!(emb(&s, g).thread.map(|t| t.color), Some(Rgb::new(0xb0, 0x3a, 0x2a)), "the first line's thread");
    // One object: its art's own stitching (the fill) is left out; it starts and ends at one point.
    let plan = s.embroidery_plan().unwrap();
    assert_eq!(plan.nodes.iter().filter(|n| **n == g).count(), 1);
    assert!(!plan.nodes.contains(&square), "the fill isn't sewn");
    let pts = needles(&s, g);
    assert!(pts.len() > 100 && pts.first().unwrap().distance(*pts.last().unwrap()) < 1e-6, "{}", pts.len());
    assert_ne!(stitches(&s), before);
    // The panel's rows act on the group; Set Type keeps it redwork unless asked for none.
    s.execute("embroidery.setParams", &json!({"params": {"kind": "Back", "stitchLength": 2.0}})).unwrap();
    let StitchType::Redwork(r) = emb(&s, g).stitch else { panic!() };
    assert_eq!((r.kind, r.stitch_length), (RunKind::Back, 2.0));
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert!(matches!(emb(&s, g).stitch, StitchType::Redwork(_)));
    // Break Apart gives the paths back, stitched as before.
    s.execute("select.set", &json!({"ids": [g.0]})).unwrap();
    s.execute("embroidery.breakApart", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(g).is_none());
    assert!(matches!(emb(&s, square).stitch, StitchType::Fill(_)));
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(matches!(emb(&s, g).stitch, StitchType::Redwork(_)));
    // A path can be redwork on its own: its subpaths (here touching) sewn as one line.
    let star = path(&mut s, "M 300 100 L 340 180 L 260 180 Z M 300 180 L 300 260");
    s.execute("select.set", &json!({"ids": [star.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "redwork"})).unwrap();
    let pts = needles(&s, star);
    assert!(pts.first().unwrap().distance(*pts.last().unwrap()) < 1e-6);
    // Bad asks.
    assert!(s.execute("embroidery.redwork", &json!({"kind": "satin"})).is_err());
    s.execute("select.set", &json!({"ids": []})).unwrap();
    assert!(s.execute("embroidery.redwork", &json!({})).is_err());
}

#[test]
fn buttonholes_are_satin_columns_round_an_open_slit() {
    use sf_core::attr::StitchType;
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    let children =
        |s: &Session, g: NodeId| -> Vec<NodeId> { s.doc().unwrap().doc.node(g).unwrap().children().unwrap().iter().map(|c| c.id).collect() };
    let r = s.execute("embroidery.addButtonholes", &json!({"at": [200, 200], "length": 15, "width": 4})).unwrap();
    let g = NodeId(r["id"].as_u64().unwrap());
    // A run holding the fabric, a side, a bartack, the other side and a bartack, in that order.
    let kids = children(&s, g);
    let names: Vec<String> = kids.iter().map(|k| s.doc().unwrap().doc.node(*k).unwrap().name.clone().unwrap()).collect();
    assert_eq!(names, ["Holding run", "Side", "Bartack", "Side", "Bartack"]);
    assert!(matches!(emb(&s, kids[0]).stitch, StitchType::Run(_)));
    assert!(kids[1..].iter().all(|k| matches!(emb(&s, *k).stitch, StitchType::Satin(_))));
    // The slit stays open; the buttonhole is 15 mm plus its bartacks long and 4 mm wide.
    let (cx, cy) = (200.0 * MM, 200.0 * MM);
    let pts: Vec<sf_core::Point> = kids.iter().flat_map(|k| needles(&s, *k)).collect();
    assert!(pts.len() > 100, "{}", pts.len());
    assert!(!pts.iter().any(|p| (p.x - cx).abs() < 7.0 && (p.y - cy).abs() < 0.15), "a needle point in the slit");
    let (w, h) = pts.iter().fold((0.0f64, 0.0f64), |(w, h), p| (w.max((p.x - cx).abs()), h.max((p.y - cy).abs())));
    assert!((8.5..=10.0).contains(&w) && (1.9..=2.2).contains(&h), "{w} {h}");
    // A string of three, evenly spaced and turned upright; rounded and keyhole ends.
    let r =
        s.execute("embroidery.addButtonholes", &json!({"at": [100, 100], "to": [100, 300], "count": 3, "angle": 90, "style": "rounded"})).unwrap();
    let ids: Vec<u64> = r["ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(children(&s, NodeId(r["id"].as_u64().unwrap())).len(), 3);
    let centre = |s: &Session, id: u64| {
        let pts: Vec<sf_core::Point> = children(s, NodeId(id)).iter().flat_map(|k| needles(s, *k)).collect();
        let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        let xs = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
        ((lo + hi) * 0.5, hi - lo, xs.1 - xs.0)
    };
    let (y1, long, across) = centre(&s, ids[1]);
    assert!((y1 - 200.0 * MM).abs() < 2.0 && long > across * 2.0, "{y1} {long} {across}");
    assert_eq!(children(&s, NodeId(ids[0])).len(), 3, "a run, the sides round the end and a bartack");
    let k = NodeId(s.execute("embroidery.addButtonholes", &json!({"style": "keyhole", "at": [400, 100]})).unwrap()["id"].as_u64().unwrap());
    let eye: Vec<sf_core::Point> = children(&s, k).iter().flat_map(|c| needles(&s, *c)).collect();
    let tall = eye.iter().fold(0.0f64, |m, p| m.max((p.y - 100.0 * MM).abs()));
    assert!(tall > 2.2, "the eye is wider than the sides: {tall}");
    // Bad asks.
    for bad in [json!({"style": "oval"}), json!({"length": 1}), json!({"count": 0}), json!({"count": 51}), json!({"at": "x"}), json!({"width": -2})] {
        assert!(s.execute("embroidery.addButtonholes", &bad).is_err(), "{bad}");
    }
}

#[test]
fn outlines_go_round_lines_and_rings_are_placed() {
    use sf_core::attr::StitchType;
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    // A satin line 3 mm wide: its outline goes round the band it covers, 1 mm beyond its edge.
    let line = NodeId(s.execute("path.create", &json!({"d": "M 100 100 L 300 100"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"width": 3.0}})).unwrap();
    let outline = NodeId(s.execute("embroidery.addOutline", &json!({"stitch": "run"})).unwrap()["id"].as_u64().unwrap());
    let pts = needles(&s, outline);
    let y0 = 100.0 * MM;
    let reach = pts.iter().fold(0.0f64, |m, p| m.max((p.y - y0).abs()));
    let nearest = pts.iter().filter(|p| p.x > 110.0 * MM && p.x < 290.0 * MM).fold(f64::INFINITY, |m, p| m.min((p.y - y0).abs()));
    assert!((2.4..=2.7).contains(&nearest) && reach < 2.7, "{nearest} {reach}");
    // A run's outline goes round it too (as if 1 mm wide).
    let run = NodeId(s.execute("path.create", &json!({"d": "M 100 300 L 300 300"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [run.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "run"})).unwrap();
    assert!(s.execute("embroidery.addOutline", &json!({"stitch": "run"})).is_ok());
    // Rings: satin round a circle, a fill between two, a bean run round one.
    let ring = |s: &mut Session, p: Value| NodeId(s.execute("embroidery.placeRing", &p).unwrap()["id"].as_u64().unwrap());
    let c = (400.0 * MM, 300.0 * MM);
    let radii = |s: &Session, id: NodeId| {
        needles(s, id).iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), p| {
            let r = ((p.x - c.0).powi(2) + (p.y - c.1).powi(2)).sqrt();
            (lo.min(r), hi.max(r))
        })
    };
    let satin = ring(&mut s, json!({"at": [400, 300], "diameter": 30, "width": 4}));
    assert!(matches!(emb(&s, satin).stitch, StitchType::Satin(_)));
    let (lo, hi) = radii(&s, satin);
    assert!(lo > 10.5 && hi < 15.4, "{lo} {hi}");
    let fill = ring(&mut s, json!({"at": [400, 300], "diameter": 30, "width": 6, "stitch": "fill"}));
    assert!(matches!(emb(&s, fill).stitch, StitchType::Fill(_)));
    let (lo, hi) = radii(&s, fill);
    assert!(lo > 8.5 && hi < 15.5, "{lo} {hi}");
    let bean = ring(&mut s, json!({"at": [400, 300], "diameter": 30, "width": 2, "stitch": "bean"}));
    let (lo, hi) = radii(&s, bean);
    assert!((lo - 14.0).abs() < 0.3 && (hi - 14.0).abs() < 0.3, "{lo} {hi}");
    for bad in [json!({"stitch": "motif"}), json!({"diameter": 1}), json!({"diameter": 20, "width": 11}), json!({"at": [0]})] {
        assert!(s.execute("embroidery.placeRing", &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_tapered_satin_line_sews_graded_motifs() {
    use sf_core::attr::StitchType;
    let mut s = session();
    let line = NodeId(s.execute("path.create", &json!({"d": "M 100 100 C 200 60 300 140 400 100"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"width": 8.0, "profile": "Tapered"}})).unwrap();
    let satin = needles(&s, line);
    s.execute("embroidery.setParams", &json!({"params": {"motifColumn": {"motif": "Heart", "length": 1.2}}})).unwrap();
    let StitchType::Satin(p) = emb(&s, line).stitch else { panic!() };
    let m = p.motif_column.unwrap();
    assert_eq!((m.motif, m.length), (sf_core::model::Motif::Heart, 1.2));
    let motifs = needles(&s, line);
    assert!(motifs.len() > 50 && motifs != satin);
    s.execute("embroidery.setParams", &json!({"params": {"motifColumn": null}})).unwrap();
    assert_eq!(needles(&s, line), satin);
}

#[test]
fn sculpture_runs_and_cross_styles_are_stitch_parameters() {
    use sf_core::attr::StitchType;
    use sf_core::model::{CrossStyle, RunKind};
    let mut s = session();
    let line = NodeId(s.execute("path.create", &json!({"d": "M 20 100 L 300 100"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "run"})).unwrap();
    let single = needles(&s, line).len();
    s.execute("embroidery.setParams", &json!({"params": {"kind": "Sculpture", "groups": [2, 2], "spacing": 1.5}})).unwrap();
    let StitchType::Run(r) = emb(&s, line).stitch else { panic!() };
    assert_eq!((r.kind, r.groups), (RunKind::Sculpture, [2, 2]));
    // Triple stitches go there and back: more needle points than a single run.
    assert!(needles(&s, line).len() > single * 3 / 2, "{} {single}", needles(&s, line).len());
    // A cross fill's style.
    let a = rect(&mut s, 10.0, 150.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "cross"})).unwrap();
    let full = needles(&s, a);
    s.execute("embroidery.setParams", &json!({"params": {"cross": "Half"}})).unwrap();
    assert_eq!(fill_params(&s, a).cross, CrossStyle::Half);
    let half = needles(&s, a);
    assert!(half.len() * 3 < full.len() * 2, "{} {}", half.len(), full.len());
    s.execute("embroidery.setParams", &json!({"params": {"cross": "Double"}})).unwrap();
    assert!(needles(&s, a).len() > full.len());
    assert!(s.execute("embroidery.setParams", &json!({"params": {"cross": "Diagonal"}})).is_err());
    assert_eq!(fill_params(&s, a).cross, CrossStyle::Double);
}

#[test]
fn motif_fills_and_graded_motif_runs_are_stitch_parameters() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "motiffill"})).unwrap();
    let waves = needles(&s, a);
    assert!(waves.len() > 300, "{}", waves.len());
    s.execute("embroidery.setParams", &json!({"params": {"motif": "Heart", "motifSize": 6}})).unwrap();
    let f = fill_params(&s, a);
    assert_eq!((f.kind, f.motif, f.motif_size), (sf_core::model::FillKind::Motif, sf_core::model::Motif::Heart, 6.0));
    assert_ne!(needles(&s, a), waves);
    // a motif run that grows along its line
    let line = NodeId(s.execute("path.create", &json!({"d": "M 20 250 L 300 250"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "motif"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"widthEnd": 8}})).unwrap();
    let sf_core::attr::StitchType::Run(r) = emb(&s, line).stitch else { panic!() };
    assert_eq!(r.width_end, Some(8.0));
    // a paired motif fill, and every texture by its name
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"motifPair": "Star"}})).unwrap();
    assert_eq!(fill_params(&s, a).motif_pair, Some(sf_core::model::Motif::Star));
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    for pattern in sf_core::model::TexturePattern::ALL {
        s.execute("embroidery.setParams", &json!({"params": {"texture": {"pattern": pattern.id()}}})).unwrap();
        assert_eq!(fill_params(&s, a).texture.map(|t| t.pattern), Some(pattern));
        assert!(needles(&s, a).len() > 100, "{pattern:?}");
    }
}

#[test]
fn three_d_motif_fills_are_a_stitch_parameter() {
    use sf_core::model::WarpKind;
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "motiffill"})).unwrap();
    let flat = needles(&s, a);
    // a bowl, with the default strength
    s.execute("embroidery.setParams", &json!({"params": {"warp": {"kind": "GlobeIn"}}})).unwrap();
    let w = fill_params(&s, a).warp.unwrap();
    assert_eq!((w.kind, w.amount, w.angle), (WarpKind::GlobeIn, 0.5, 90.0));
    let bowl = needles(&s, a);
    assert!(bowl.len() > 100 && bowl != flat);
    // receding to the right
    s.execute("embroidery.setParams", &json!({"params": {"warp": {"kind": "Perspective", "angle": 0}}})).unwrap();
    let w = fill_params(&s, a).warp.unwrap();
    assert_eq!((w.kind, w.angle), (WarpKind::Perspective, 0.0));
    assert_ne!(needles(&s, a), bowl);
    // flat again
    s.execute("embroidery.setParams", &json!({"params": {"warp": null}})).unwrap();
    assert!(fill_params(&s, a).warp.is_none());
    assert_eq!(needles(&s, a), flat);
}

#[test]
fn the_motif_tool_places_motifs_where_you_click_and_drag() {
    use vectorcraft_tools::{Mods, PointerEvent, PointerKind};
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    let v = crate::ViewInfo::default();
    s.select_tool("embroideryMotif", v).unwrap();
    s.execute("tool.setOption", &json!({"tool": "embroideryMotif", "values": {"motif": "Heart"}})).ok();
    s.set_tool_options(Some("embroideryMotif"), json!({"motif": "Heart"}).as_object().unwrap());
    let ev = |kind, x: f64, y: f64| PointerEvent::new(kind, x, y).with_mods(Mods::default());
    let count = |s: &Session| {
        let mut n = 0;
        s.doc().unwrap().doc.walk(|_| n += 1);
        n
    };
    let before = count(&s);
    // a click: one heart 10 mm long round where it was clicked
    s.pointer(&ev(PointerKind::Down, 100.0, 100.0), v).unwrap();
    s.pointer(&ev(PointerKind::Up, 100.0, 100.0), v).unwrap();
    assert_eq!(count(&s), before + 1);
    let id = *s.doc().unwrap().selection.objects.first().unwrap();
    let n = s.doc().unwrap().doc.node(id).unwrap().clone();
    assert_eq!(n.name.as_deref(), Some("Heart"));
    let b = n.geometric_bounds().unwrap();
    assert!((b.width() * MM - 10.0).abs() < 0.5 && b.center().distance(vectorcraft_geom::Point::new(100.0, 100.0)) < 15.0, "{b:?}");
    // a drag: as long as the drag, upright along it
    s.pointer(&ev(PointerKind::Down, 50.0, 300.0), v).unwrap();
    s.pointer(&ev(PointerKind::Drag, 120.0, 300.0), v).unwrap();
    s.pointer(&ev(PointerKind::Up, 200.0, 300.0), v).unwrap();
    assert_eq!(count(&s), before + 2);
    let id = *s.doc().unwrap().selection.objects.first().unwrap();
    let b = s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap();
    assert!((b.width() - 150.0).abs() < 2.0 && (b.center().x - 125.0).abs() < 2.0, "{b:?}");
    // one undo step each
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(count(&s), before + 1);
    // Escape during a drag places nothing
    s.pointer(&ev(PointerKind::Down, 50.0, 400.0), v).unwrap();
    s.pointer(&ev(PointerKind::Drag, 150.0, 400.0), v).unwrap();
    s.tool_key(vectorcraft_tools::ToolKey::Escape, Mods::default(), v).unwrap();
    assert_eq!(count(&s), before + 1);
}

#[test]
fn my_motifs_carry_motifs_from_design_to_design() {
    let mut s = session();
    let peak = NodeId(s.execute("path.create", &json!({"d": "M 0 0 L 10 -10 L 20 0"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [peak.0]})).unwrap();
    s.execute("embroidery.motif.make", &json!({"name": "Peak"})).unwrap();
    let r = s.execute("embroidery.motif.keep", &json!({"name": "Peak"})).unwrap();
    assert_eq!(r["kept"].as_u64(), Some(1));
    assert!(s.execute("embroidery.motif.keep", &json!({"name": "Nope"})).is_err());
    // a new design: the kept motif applies, and joins it
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let line = NodeId(s.execute("path.create", &json!({"d": "M 20 100 L 300 100"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.motif.apply", &json!({"name": "peak"})).unwrap();
    let sf_core::attr::StitchType::Run(r) = emb(&s, line).stitch else { panic!() };
    assert_eq!(r.custom_motif.map(|m| m.name), Some("Peak".to_string()));
    assert_eq!(s.execute("embroidery.motif.list", &json!({})).unwrap()["motifs"].as_array().unwrap().len(), 1);
    // Place Motif finds it too
    assert!(s.execute("embroidery.placeMotif", &json!({"motif": "Peak"})).is_ok());
    // listed, then forgotten; the design keeps its copy
    assert_eq!(s.execute("embroidery.motif.library", &json!({})).unwrap()["motifs"][0]["name"], "Peak");
    s.execute("embroidery.motif.forget", &json!({"name": "PEAK"})).unwrap();
    assert!(s.prefs.embroidery_motifs.is_empty());
    assert!(s.execute("embroidery.motif.forget", &json!({"name": "Peak"})).is_err());
    assert_eq!(s.execute("embroidery.motif.list", &json!({})).unwrap()["motifs"].as_array().unwrap().len(), 1);
}

/// An Ink/Stitch design of our own: a bean stitch run sewn twice, a satin column with three
/// rungs, a fill at 30° and a plain stroke Ink/Stitch would sew as satin.
const INKSTITCH_DESIGN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkstitch="http://inkstitch.org/namespace" width="100mm" height="60mm" viewBox="0 0 100 60">
  <path id="run" d="M 10 10 L 90 10" style="fill:none;stroke:#cc0000;stroke-width:0.3;stroke-dasharray:1,0.5" inkstitch:running_stitch_length_mm="2" inkstitch:bean_stitch_repeats="1" inkstitch:repeats="2"/>
  <path id="satin" d="M 10 20 L 90 20 M 10 26 L 90 26 M 30 18 L 30 28 M 50 18 L 50 28 M 70 18 L 70 28" style="fill:none;stroke:#0000cc;stroke-width:0.5" inkstitch:satin_column="True" inkstitch:zigzag_spacing_mm="0.35" inkstitch:pull_compensation_mm="0.2 0.3" inkstitch:contour_underlay="True" inkstitch:center_walk_underlay="False"/>
  <path id="fill" d="M 10 35 H 50 V 55 H 10 Z" style="fill:#00aa00;stroke:none" inkstitch:angle="30" inkstitch:row_spacing_mm="0.3" inkstitch:max_stitch_length_mm="3.5"/>
  <path id="plain" d="M 60 35 L 90 55" style="fill:none;stroke:#333333;stroke-width:3"/>
</svg>"##;

#[test]
fn ink_stitch_designs_open_with_their_stitch_settings() {
    use sf_core::attr::StitchType;
    use sf_core::model::{FillKind, RunKind};
    let mut s = Session::new();
    let r = crate::cmd::fileio::open_bytes(&mut s, "design.svg", INKSTITCH_DESIGN.as_bytes(), None).unwrap();
    assert!(r["warnings"].to_string().contains("Ink/Stitch"), "{r}");
    let node = |s: &Session, name: &str| {
        let mut found = None;
        s.doc().unwrap().doc.walk(|n| {
            if n.name.as_deref() == Some(name) {
                found = Some(n.clone());
            }
        });
        found.unwrap()
    };
    let emb = |name: &str| node(&s, name).embroidery.as_deref().cloned().unwrap();
    // a bean stitch sewn twice, 2 mm stitches
    let StitchType::Run(run) = emb("run").stitch else { panic!() };
    assert_eq!((run.kind, run.stitch_length, run.passes), (RunKind::Triple, 2.0, 2));
    // a satin column between its two rails, its three rungs as inclination lines
    let e = emb("satin");
    let StitchType::Satin(p) = &e.stitch else { panic!() };
    assert_eq!((p.spacing, p.pull_comp, p.pull_comp_2), (0.35, 0.2, Some(0.3)));
    assert!(!p.underlay.auto && p.underlay.edge_run && !p.underlay.center_run && !p.underlay.zigzag);
    assert_eq!((e.guides.len(), node(&s, "satin").path_data().unwrap().subpaths.len()), (3, 2));
    // a fill at 30°, rows 0.3 mm apart
    let StitchType::Fill(f) = emb("fill").stitch else { panic!() };
    assert_eq!((f.kind, f.angle, f.spacing, f.stitch_length), (FillKind::Tatami, 30.0, 0.3, 3.5));
    // a plain stroke: a satin line as wide as the stroke
    let e = emb("plain");
    assert!(matches!(e.stitch, StitchType::Satin(_)) && (e.width.unwrap() - 3.0).abs() < 0.01, "{:?}", e.width);
    assert!(stitches(&s) > 300);
    // An SVG with no Ink/Stitch settings opens as plain art.
    let plain = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100"><path id="p" d="M10 10 L90 90" stroke="#000" fill="none"/></svg>"##;
    crate::cmd::fileio::open_bytes(&mut s, "plain.svg", plain.as_bytes(), None).unwrap();
    assert!(node(&s, "p").embroidery.is_none());
}

/// A tiny embroidery font of our own in Ink/Stitch's format: capitals 10 mm high, "A" a satin
/// column and a run, "B" a fill, kerned closer as "AB", and a default glyph.
const TEST_FONT_JSON: &str = r#"{"name": "Test Script", "font_license": "MIT", "leading": 60, "size": 10,
  "min_scale": 0.5, "max_scale": 2, "letter_case": "upper", "default_glyph": "?", "horiz_adv_x_default": null,
  "horiz_adv_x_space": 20, "units_per_em": 60, "horiz_adv_x": {"A": 40, "B": 36, "?": 30}, "kerning_pairs": {"A B": 4}}"#;
const TEST_FONT_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd" xmlns:inkstitch="http://inkstitch.org/namespace" width="100mm" height="100mm" viewBox="0 0 100 100">
  <sodipodi:namedview id="nv"><sodipodi:guide position="0,20" orientation="0,1" inkscape:label="baseline" id="g1"/></sodipodi:namedview>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-A" id="la" style="display:none">
    <path id="a1" d="M 2 80 L 15 70 M 6 80 L 18 70 M 4 74 L 10 79" style="fill:none;stroke:#000000" inkstitch:satin_column="True"/>
    <path id="a2" d="M 15 70 L 30 80" style="fill:none;stroke:#000000;stroke-dasharray:1,1" inkstitch:running_stitch_length_mm="2"/>
  </g>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-B" id="lb" style="display:none">
    <path id="b1" d="M 2 70 H 25 V 80 H 2 Z" style="fill:#000000;stroke:none" inkstitch:angle="45"/>
  </g>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-?" id="lq" style="display:none">
    <path id="q1" d="M 2 70 L 20 80" style="fill:none;stroke:#000000;stroke-dasharray:1,1"/>
  </g>
</svg>"##;

#[test]
fn the_kiosk_sets_a_name_in_a_template_and_locks_with_a_pin() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // no font yet
    assert!(s.execute("embroidery.kiosk.order", &json!({"text": "AB"})).is_err());
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.kiosk.order", &json!({"text": "AB", "template": "name tape 100 × 30", "color": "#D62828"})).unwrap();
    assert_eq!(
        (r["template"].as_str(), r["font"].as_str(), r["color"].as_str(), r["title"].as_str()),
        (Some("Name tape 100 × 30"), Some("Test Script"), Some("#d62828"), Some("AB")),
        "{r}"
    );
    // inside the tape's border (90 × 20 mm), its middle the tape's (65, 30 mm)
    assert!(r["widthMm"].as_f64().unwrap() <= 90.5 && r["heightMm"].as_f64().unwrap() <= 20.5, "{r}");
    let id = NodeId(r["id"].as_u64().unwrap());
    let d = &s.doc().unwrap().doc;
    let b = d.node(id).unwrap().bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false).unwrap();
    assert!((b.center().x / PT - 65.0).abs() < 1.0 && (b.center().y / PT - 30.0).abs() < 1.0, "{b:?}");
    let threads: Vec<String> = s.embroidery_plan().unwrap().pattern().threads.iter().map(|t| t.color.to_hex()).collect();
    assert!(threads.iter().any(|t| t == "#d62828"), "{threads:?}");
    // a long name made smaller to fit
    let r = s.execute("embroidery.kiosk.order", &json!({"text": "ABABABABABABABAB"})).unwrap();
    assert!(r["widthMm"].as_f64().unwrap() <= 90.5, "{r}");
    // the shop's offer, locked with a PIN
    let r = s.execute("embroidery.kiosk", &json!({"on": true, "pin": "2468", "templates": ["cap front"], "colors": ["#FFFFFF"]})).unwrap();
    assert_eq!(
        (r["on"].clone(), r["locked"].clone(), r["templates"].clone(), r["colors"].clone()),
        (json!(true), json!(true), json!(["Cap front"]), json!(["#ffffff"]))
    );
    assert!(s.execute("embroidery.kiosk.order", &json!({"text": "AB", "template": "Left chest logo"})).is_err());
    assert!(s.execute("embroidery.kiosk.order", &json!({"text": "AB", "color": "#000000"})).is_err());
    let r = s.execute("embroidery.kiosk.order", &json!({"text": "AB"})).unwrap();
    assert_eq!((r["template"].as_str(), r["color"].as_str()), (Some("Cap front"), Some("#ffffff")));
    // changing or leaving it takes the PIN
    assert!(s.execute("embroidery.kiosk", &json!({"fonts": []})).is_err());
    assert!(s.execute("embroidery.kiosk", &json!({"on": false, "pin": "1357"})).is_err());
    s.execute("embroidery.kiosk", &json!({"on": false, "pin": "2468"})).unwrap();
    let r = s.execute("embroidery.kiosk", &json!({})).unwrap();
    assert_eq!((r["on"].clone(), r["locked"].clone()), (json!(false), json!(true)));
    assert!(s.execute("embroidery.kiosk", &json!({"pin": "12"})).is_err());
    assert!(s.execute("embroidery.kiosk", &json!({"templates": ["nope"]})).is_err());
    assert!(s.execute("embroidery.kiosk", &json!({"format": "zzz"})).is_err());
    assert!(s.execute("embroidery.kiosk.order", &json!({"text": "  "})).is_err());
}

#[test]
fn embroidery_fonts_set_lettering_from_their_glyphs() {
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    // loaded from its files' text (a folder on the desktop)
    let r = s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    assert_eq!(
        (r["name"].as_str(), r["glyphs"].as_u64(), r["minSize"].as_f64(), r["maxSize"].as_f64()),
        (Some("Test Script"), Some(3), Some(5.0), Some(20.0))
    );
    // (the fonts built into the app, when it is built with them, aside)
    let listed = s.execute("embroidery.fonts", &json!({})).unwrap();
    assert_eq!(listed["fonts"].as_array().unwrap().iter().filter(|f| f["builtIn"] == false).count(), 1);
    // "ab" set in capitals at the font's own size, from (100, 300) on its baseline
    let r = s.execute("embroidery.lettering", &json!({"text": "ab", "font": "test script", "at": [100, 300]})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(2));
    assert!(r["warnings"].as_array().unwrap().is_empty());
    let id = NodeId(r["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    let all = st.doc.node(id).unwrap();
    let glyphs: Vec<_> = all.children().unwrap().iter().map(|g| (g.name.clone().unwrap(), g.geometric_bounds().unwrap())).collect();
    assert_eq!(glyphs.iter().map(|g| g.0.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    // the art sits on the baseline: the glyphs' bottoms at y = 300 (they reach 20 mm-units up
    // from the page bottom, where the baseline guide is)
    let (a, b) = (glyphs[0].1, glyphs[1].1);
    assert!((a.y1 - 300.0).abs() < 0.5 && (b.y1 - 300.0).abs() < 0.5, "{a:?} {b:?}");
    // B starts A's advance (40) less the kerning (4) along: 36 font units, 1 mm each
    assert!((b.x0 - (100.0 + (36.0 + 2.0) / MM)).abs() < 0.5, "{b:?}");
    // its glyphs sew: a satin column, a run and a fill
    assert!(stitches(&s) > 100);
    // twice the size, with a missing character as the default glyph; past the font's best sizes,
    // a warning
    let r = s.execute("embroidery.lettering", &json!({"text": "A%B", "font": "Test Script", "size": 20})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(3));
    let r = s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "size": 50})).unwrap();
    assert!(r["warnings"].to_string().contains("up to 20.0 mm"), "{r}");
    // bad requests
    assert!(s.execute("embroidery.lettering", &json!({"text": "A", "font": "Nope"})).is_err());
    assert!(s.execute("embroidery.lettering", &json!({"text": " ", "font": "Test Script"})).is_err());
    assert!(s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "size": 0})).is_err());
    assert!(s.execute("embroidery.font.load", &json!({"json": "{}", "svg": TEST_FONT_SVG})).is_err());
    assert!(s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": "<svg/>"})).is_err());
    assert!(s.execute("embroidery.font.load", &json!({})).is_err());
}

#[test]
fn embroidery_font_folders_are_remembered() {
    let dir = std::env::temp_dir().join(format!("sf-font-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("font.json"), TEST_FONT_JSON).unwrap();
    std::fs::write(dir.join("ltr.svg"), TEST_FONT_SVG).unwrap();
    let path = dir.to_string_lossy().to_string();
    let mut s = session();
    let r = s.execute("embroidery.font.load", &json!({"path": path})).unwrap();
    assert_eq!((r["name"].as_str(), r["folder"].as_str()), (Some("Test Script"), Some(path.as_str())));
    assert_eq!((s.prefs.embroidery_font_dirs.len(), s.prefs.embroidery_font_dirs.first()), (1, Some(&path)));
    // loaded again: remembered once
    s.execute("embroidery.font.load", &json!({"path": path})).unwrap();
    assert_eq!(s.prefs.embroidery_font_dirs.len(), 1);
    // a later session loads it again when lettering needs it
    let mut later = session();
    later.prefs.embroidery_font_dirs = s.prefs.embroidery_font_dirs.clone();
    assert!(later.embroidery_fonts.is_empty());
    later.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script"})).unwrap();
    let r = later.execute("embroidery.fonts", &json!({})).unwrap();
    let mine = r["fonts"].as_array().unwrap().iter().find(|f| f["name"] == "Test Script").cloned().unwrap();
    assert_eq!(mine["folder"].as_str(), Some(path.as_str()));
    // forgotten: unloaded and not remembered
    later.execute("embroidery.font.forget", &json!({"name": "test script"})).unwrap();
    assert!(later.embroidery_fonts.iter().all(|f| f.built_in.is_some()) && later.prefs.embroidery_font_dirs.is_empty());
    assert!(later.execute("embroidery.font.forget", &json!({"name": "Test Script"})).is_err());
    // a folder gone: skipped with a warning, still remembered
    let mut gone = session();
    gone.prefs.embroidery_font_dirs = vec!["/nonexistent/sf-font".into()];
    let r = gone.execute("embroidery.fonts", &json!({})).unwrap();
    assert!(r["fonts"].as_array().unwrap().is_empty() && r["warnings"].to_string().contains("nonexistent"), "{r}");
    assert_eq!(gone.prefs.embroidery_font_dirs.len(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn team_names_write_one_machine_file_per_name() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    // nothing to set the names in yet
    assert!(s.execute("embroidery.teamNames.export", &json!({"names": ["A"], "dir": "/tmp/x"})).is_err());
    let id = NodeId(s.execute("embroidery.lettering", &json!({"text": "AB", "font": "Test Script"})).unwrap()["id"].as_u64().unwrap());
    let before = s.doc().unwrap().doc.clone();
    let dir = std::env::temp_dir().join(format!("sf-team-names-{}", std::process::id()));
    let path = dir.to_string_lossy().to_string();
    let r = s.execute("embroidery.teamNames.export", &json!({"names": ["AB", "BA", "A/B", "ab", "A\nB"], "dir": path, "format": "dst"})).unwrap();
    assert_eq!(r["count"].as_u64(), Some(5));
    // named after the names: made safe, kept apart
    let names: Vec<String> = r["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| std::path::Path::new(f.as_str().unwrap()).file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, ["AB.dst", "BA.dst", "A_B.dst", "ab 2.dst", "A_B 2.dst"]);
    // machine files whose stitches differ by name; the design as it was, no undo step taken
    let ab = std::fs::read(dir.join("AB.dst")).unwrap();
    let ba = std::fs::read(dir.join("BA.dst")).unwrap();
    assert!(ab.len() > 512 && ab.get(512..) != ba.get(512..));
    assert!(std::sync::Arc::ptr_eq(&before, &s.doc().unwrap().doc));
    assert_eq!(s.doc().unwrap().doc.node(id).unwrap().children().unwrap().len(), 2);
    // names from a table: a column, its header left out, quoted commas kept
    let r = s.execute("embroidery.teamNames.read", &json!({"csv": "Name,Number\n\"Smith, J\",7\nLee,9\n,4\n", "header": true})).unwrap();
    assert_eq!(r["names"], json!(["Smith, J", "Lee"]));
    let r = s.execute("embroidery.teamNames.read", &json!({"csv": "Name,Number\nLee,9\n", "header": true, "column": 1})).unwrap();
    assert_eq!(r["names"], json!(["9"]));
    // bad requests
    assert!(s.execute("embroidery.teamNames.export", &json!({"names": [], "dir": path})).is_err());
    assert!(s.execute("embroidery.teamNames.export", &json!({"names": ["A"]})).is_err());
    assert!(s.execute("embroidery.teamNames.export", &json!({"names": ["A"], "dir": path, "format": "doc"})).is_err());
    assert!(s.execute("embroidery.teamNames.read", &json!({})).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn monograms_set_their_letters_inside_a_border() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.monogram", &json!({"letters": "ABA", "font": "Test Script", "at": [300, 300], "border": "circle"})).unwrap();
    assert_eq!(r["letters"].as_u64(), Some(3));
    let id = NodeId(r["id"].as_u64().unwrap());
    let parts = s.doc().unwrap().doc.node(id).unwrap().children().unwrap().clone();
    assert_eq!(parts.len(), 4);
    // three lettering groups, the middle one larger, each set again as lettering can be
    let size = |n: &vectorcraft_doc::Node| match &n.embroidery.as_deref().unwrap().stitch {
        sf_core::attr::StitchType::Lettering(l) => l.size,
        other => panic!("{other:?}"),
    };
    assert!((size(&parts[0]) - 10.0).abs() < 1e-9 && (size(&parts[1]) - 16.0).abs() < 1e-9 && (size(&parts[2]) - 10.0).abs() < 1e-9);
    // left to right, each centred on the monogram's middle; the border round them all
    let b: Vec<vectorcraft_geom::Rect> = parts.iter().map(|n| n.geometric_bounds().unwrap()).collect();
    assert!(b[0].x1 < b[1].x0 && b[1].x1 < b[2].x0, "{b:?}");
    assert!(b[..3].iter().all(|r| (r.center().y - 300.0).abs() < 0.5), "{b:?}");
    assert!(b[3].x0 < b[0].x0 && b[3].x1 > b[2].x1 && b[3].y0 < b[1].y0 && b[3].y1 > b[1].y1, "{b:?}");
    assert!(matches!(parts[3].embroidery.as_deref().unwrap().stitch, sf_core::attr::StitchType::Satin(_)));
    assert!(stitches(&s) > 200);
    // in a line, one size on one baseline, a scalloped border
    let r =
        s.execute("embroidery.monogram", &json!({"letters": "AB", "font": "Test Script", "style": "line", "border": "scallop", "size": 12})).unwrap();
    let parts = s.doc().unwrap().doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap().children().unwrap().clone();
    let b: Vec<vectorcraft_geom::Rect> = parts.iter().map(|n| n.geometric_bounds().unwrap()).collect();
    assert!((b[0].y1 - b[1].y1).abs() < 0.01 && (size(&parts[0]) - 12.0).abs() < 1e-9 && (size(&parts[1]) - 12.0).abs() < 1e-9);
    let sf_core::attr::StitchType::Run(run) = &parts[2].embroidery.as_deref().unwrap().stitch else { panic!() };
    assert_eq!(run.kind, sf_core::model::RunKind::Motif);
    // bad requests
    for bad in [
        json!({"letters": "", "font": "Test Script"}),
        json!({"letters": "ABCD", "font": "Test Script"}),
        json!({"letters": "A", "font": "Nope"}),
        json!({"letters": "A", "font": "Test Script", "border": "star"}),
        json!({"letters": "A", "font": "Test Script", "style": "fancy"}),
        json!({"letters": "A", "font": "Test Script", "size": 0}),
    ] {
        assert!(s.execute("embroidery.monogram", &bad).is_err(), "{bad}");
    }
}

#[test]
fn embroidery_fonts_are_made_in_font_documents() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let r = s.execute("embroidery.fontDoc.new", &json!({"name": "Hand", "chars": "AB", "size": 20})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(2));
    let (a, guides) = {
        let st = s.doc().unwrap();
        assert_eq!(st.doc.artboards.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["A", "B"]);
        let a = st.doc.artboards[0].clone();
        (a.rect, st.doc.guides.iter().filter(|g| g.artboard == Some(a.id)).map(|g| g.pos).collect::<Vec<_>>())
    };
    // each artboard with its baseline (8 mm, 0.4 × the size, up from the bottom) and cap height
    let base = a.y1 - 8.0 * PT;
    assert_eq!(guides.len(), 2);
    assert!((guides[0] - base).abs() < 1e-9 && (guides[1] - (base - 20.0 * PT)).abs() < 1e-9, "{guides:?}");
    // nothing digitized yet
    assert!(s.execute("embroidery.fontDoc.save", &json!({})).is_err());
    // A: a satin line from the baseline up to the capitals' height, on A's artboard
    let d = format!("M {} {} L {} {}", a.x0 + 10.0, base, a.x0 + 20.0, base - 20.0 * PT);
    let id = s.execute("path.create", &json!({"d": d})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [id]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    // properties and kerning
    s.execute("embroidery.fontDoc.setProps", &json!({"letterCase": "upper", "space": 5, "maxScale": 3})).unwrap();
    s.execute("embroidery.fontDoc.kern", &json!({"pair": "A B", "value": 2})).unwrap();
    let info = s.execute("embroidery.fontDoc.info", &json!({})).unwrap();
    assert_eq!((info["missing"].as_u64(), info["glyphs"][0]["done"].as_bool()), (Some(1), Some(true)));
    assert_eq!(info["font"]["kerning"]["A B"].as_f64(), Some(2.0));
    // saved: loaded for lettering at once
    let r = s.execute("embroidery.fontDoc.save", &json!({})).unwrap();
    assert_eq!((r["name"].as_str(), r["glyphs"].as_u64()), (Some("Hand"), Some(1)));
    assert_eq!(r["missing"], json!(["B"]));
    s.execute("file.new", &json!({"width": 600, "height": 600})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "a", "font": "Hand", "at": [100, 500]})).unwrap();
    let lid = NodeId(r["id"].as_u64().unwrap());
    // the glyph as digitized: on the baseline, 20 mm high, 10 pt in from where the line starts
    let b = s.doc().unwrap().doc.node(lid).unwrap().geometric_bounds().unwrap();
    assert!((b.y1 - 500.0).abs() < 0.5 && (b.height() - 20.0 * PT).abs() < 0.5 && (b.x0 - 110.0).abs() < 0.5, "{b:?}");
    assert!(stitches(&s) > 20);
    // saved to a folder: a later session loads it from there
    let dir = std::env::temp_dir().join(format!("sf-font-made-{}", std::process::id()));
    let path = dir.to_string_lossy().to_string();
    // the font document is the second open (the session started with one)
    s.execute("document.activate", &json!({"index": 1})).unwrap();
    s.execute("embroidery.fontDoc.save", &json!({"folder": path})).unwrap();
    assert!(dir.join("font.json").is_file() && dir.join("glyphs.stitchfoundry").is_file());
    let mut later = session();
    later.prefs.embroidery_font_dirs = s.prefs.embroidery_font_dirs.clone();
    assert!(later.execute("embroidery.lettering", &json!({"text": "A", "font": "Hand"})).is_ok());
    // more glyphs below; and a document that isn't a font
    let r = s.execute("embroidery.fontDoc.addGlyphs", &json!({"chars": "BCfi", "names": ["fi"]})).unwrap();
    assert_eq!(r["added"].as_u64(), Some(4));
    assert!(s.execute("embroidery.fontDoc.addGlyphs", &json!({"chars": "AB"})).is_err());
    assert!(s.execute("embroidery.fontDoc.kern", &json!({"pair": "AB", "value": 1})).is_err());
    assert!(s.execute("embroidery.fontDoc.setProps", &json!({"letterCase": "title"})).is_err());
    s.execute("file.new", &json!({"width": 100, "height": 100})).unwrap();
    assert!(s.execute("embroidery.fontDoc.save", &json!({})).is_err());
    assert!(s.execute("embroidery.fontDoc.info", &json!({})).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn embroidery_fonts_open_to_edit_and_merge() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let boxes = |s: &Session, id: NodeId| -> Vec<vectorcraft_geom::Rect> {
        s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect()
    };
    // "%" isn't in the font: its default glyph, "?", stands in
    let set = |s: &mut Session| {
        NodeId(s.execute("embroidery.lettering", &json!({"text": "AB%", "font": "Test Script", "at": [100, 300]})).unwrap()["id"].as_u64().unwrap())
    };
    let id = set(&mut s);
    let before = boxes(&s, id);
    // opened as a font document: a glyph a artboard, with its art, its properties and kerning
    let r = s.execute("embroidery.font.edit", &json!({"name": "test script"})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(3));
    let info = s.execute("embroidery.fontDoc.info", &json!({})).unwrap();
    assert_eq!(info["missing"].as_u64(), Some(0), "{info}");
    assert_eq!(info["glyphs"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect::<Vec<_>>(), ["?", "A", "B"]);
    assert_eq!(
        (info["font"]["size"].as_f64(), info["font"]["letter_case"].as_str(), info["font"]["default_glyph"].as_str()),
        (Some(10.0), Some("upper"), Some("?"))
    );
    assert!((info["font"]["kerning"]["A B"].as_f64().unwrap() - 4.0).abs() < 1e-6, "{info}");
    assert!((info["glyphs"][1]["width"].as_f64().unwrap() - 40.0).abs() < 1e-6, "{info}");
    // saved again as it was: lettering in it sets the same
    s.execute("embroidery.fontDoc.save", &json!({})).unwrap();
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    let id = set(&mut s);
    let after = boxes(&s, id);
    assert_eq!(after.len(), before.len());
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (a.x0 - b.x0).abs() < 0.01 && (a.y0 - b.y0).abs() < 0.01 && (a.x1 - b.x1).abs() < 0.01 && (a.y1 - b.y1).abs() < 0.01,
            "{before:?} {after:?}"
        );
    }
    // merged into a new font twice the size: A and B added, as tall as its capitals
    s.execute("embroidery.fontDoc.new", &json!({"name": "Big", "chars": "C", "size": 20})).unwrap();
    let r = s.execute("embroidery.fontDoc.merge", &json!({"font": "Test Script", "chars": "AB"})).unwrap();
    assert_eq!(r["added"].as_u64(), Some(2));
    let info = s.execute("embroidery.fontDoc.info", &json!({})).unwrap();
    assert_eq!(info["glyphs"].as_array().unwrap().iter().map(|g| g["name"].as_str().unwrap()).collect::<Vec<_>>(), ["C", "A", "B"]);
    assert!((info["glyphs"][1]["width"].as_f64().unwrap() - 80.0).abs() < 1e-6 && info["glyphs"][1]["done"] == true, "{info}");
    // again: nothing new to add; an unknown font
    assert!(s.execute("embroidery.fontDoc.merge", &json!({"font": "Test Script", "chars": "AB"})).is_err());
    assert!(s.execute("embroidery.fontDoc.merge", &json!({"font": "Nope"})).is_err());
    assert!(s.execute("embroidery.font.edit", &json!({"name": "Nope"})).is_err());
}

#[test]
fn embroidery_fonts_start_from_type() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // the family bundled with the app: H, i and g at capitals 20 mm high
    let r = s.execute("embroidery.fontDoc.fromType", &json!({"family": "source sans 3", "chars": "Hig ", "size": 20})).unwrap();
    assert_eq!((r["glyphs"].as_u64(), r["family"].as_str()), (Some(3), Some("Source Sans 3")));
    let info = s.execute("embroidery.fontDoc.info", &json!({})).unwrap();
    assert_eq!(info["missing"].as_u64(), Some(0), "{info}");
    let st = s.doc().unwrap();
    let (h_board, descent) = (st.doc.artboards[0].rect, info["font"]["descent"].as_f64().unwrap());
    // H: on the baseline, as tall as the capitals, stitched; g reaches below the baseline
    let art: Vec<(String, vectorcraft_geom::Rect, bool)> = st.doc.layers[0]
        .children()
        .unwrap()
        .iter()
        .map(|n| (n.name.clone().unwrap(), n.geometric_bounds().unwrap(), n.embroidery.is_some()))
        .collect();
    let base = h_board.y1 - descent * PT;
    let h = &art.iter().find(|a| a.0 == "H").unwrap().1;
    assert!((h.y1 - base).abs() < 0.5 && (h.height() - 20.0 * PT).abs() < 0.5, "{h:?}");
    let g = &art.iter().find(|a| a.0 == "g").unwrap().1;
    assert!(art.iter().all(|a| a.2) && g.y1 > base + 1.0, "{art:?}");
    // the H artboard as wide as the family's own advance, wider than the i's
    let widths: Vec<f64> = info["glyphs"].as_array().unwrap().iter().map(|g| g["width"].as_f64().unwrap()).collect();
    assert!(widths[0] > widths[1] && widths[1] > 1.0, "{widths:?}");
    // saved: lettering in it
    s.execute("embroidery.fontDoc.save", &json!({})).unwrap();
    assert!(s.execute("embroidery.lettering", &json!({"text": "Hig", "font": "Source Sans 3"})).is_ok());
    // a family or style there isn't
    assert!(s.execute("embroidery.fontDoc.fromType", &json!({"family": "No Such Family"})).is_err());
    assert!(s.execute("embroidery.fontDoc.fromType", &json!({"family": "Source Sans 3", "style": "Hairline Oblique"})).is_err());
}

#[test]
fn designs_become_glyphs_to_type_with() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    s.execute("embroidery.fontDoc.new", &json!({"name": "Keys", "chars": "a", "size": 20})).unwrap();
    // a design off the artboards: a stitched square
    let sq = rect(&mut s, 1000.0, 1000.0, 60.0, 40.0);
    s.execute("select.set", &json!({"ids": [sq.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let r = s.execute("embroidery.fontDoc.assign", &json!({"glyph": "★"})).unwrap();
    assert_eq!(r["glyph"].as_str(), Some("★"));
    // a new artboard for it, the square on its baseline a margin (2 mm) in, the board a margin wider either side
    let (board, b) = {
        let st = s.doc().unwrap();
        (st.doc.artboards.iter().find(|a| a.name == "★").unwrap().rect, st.doc.node(sq).unwrap().geometric_bounds().unwrap())
    };
    let base = board.y1 - 8.0 * PT;
    assert!((b.y1 - base).abs() < 0.01 && (b.x0 - (board.x0 + 2.0 * PT)).abs() < 0.01, "{board:?} {b:?}");
    assert!((board.width() - (60.0 + 4.0 * PT)).abs() < 0.01, "{board:?}");
    // a wide design onto a glyph's artboard: it widens for it; one undo step puts it back
    let wide = rect(&mut s, 1000.0, 1200.0, 200.0, 30.0);
    s.execute("select.set", &json!({"ids": [wide.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.fontDoc.assign", &json!({"glyph": "a"})).unwrap();
    let a_width = |s: &Session| s.doc().unwrap().doc.artboards.iter().find(|a| a.name == "a").unwrap().rect.width();
    assert!((a_width(&s) - (200.0 + 4.0 * PT)).abs() < 0.01);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!((a_width(&s) - 12.0 * PT).abs() < 0.01, "{}", a_width(&s));
    // typed: the designs as glyphs
    s.execute("embroidery.fontDoc.save", &json!({})).unwrap();
    s.execute("file.new", &json!({"width": 600, "height": 400})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "★★", "font": "Keys", "at": [50, 300]})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let kids: Vec<vectorcraft_geom::Rect> =
        s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect();
    assert_eq!(kids.len(), 2);
    assert!(kids.iter().all(|k| (k.width() - 60.0).abs() < 0.5 && (k.y1 - 300.0).abs() < 0.5), "{kids:?}");
    // no glyph, nothing selected, a name too long
    s.execute("document.activate", &json!({"index": 1})).unwrap();
    s.execute("select.set", &json!({"ids": []})).unwrap();
    assert!(s.execute("embroidery.fontDoc.assign", &json!({"glyph": "b"})).is_err());
    s.execute("select.set", &json!({"ids": [sq.0]})).unwrap();
    assert!(s.execute("embroidery.fontDoc.assign", &json!({})).is_err());
    assert!(s.execute("embroidery.fontDoc.assign", &json!({"glyph": "abcdefghi"})).is_err());
}

#[test]
fn lettering_sews_in_order_joined_and_trimmed() {
    use sf_core::attr::Embroidery;
    use vectorcraft_doc::{Node, NodeKind};
    // the first (or last) art with stitches in a glyph
    fn end(n: &Node, last: bool) -> Option<&Embroidery> {
        match &n.kind {
            NodeKind::Group { children, .. } if last => children.iter().rev().find_map(|c| end(c, last)),
            NodeKind::Group { children, .. } => children.iter().find_map(|c| end(c, last)),
            _ => n.embroidery.as_deref(),
        }
    }
    // each glyph sewn: its name, left edge, start and stop, and whether the thread is cut after it
    type Sewn = (String, f64, Option<sf_core::Point>, Option<sf_core::Point>, bool);
    fn sewn(s: &Session, id: NodeId) -> Vec<Sewn> {
        let st = s.doc().unwrap();
        let kids = st.doc.node(id).unwrap().children().unwrap();
        kids.iter()
            .map(|g| {
                let (a, z) = (end(g, false).unwrap(), end(g, true).unwrap());
                (g.name.clone().unwrap(), g.geometric_bounds().unwrap().x0, a.entry, z.exit, z.trim_after)
            })
            .collect()
    }
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let set = |s: &mut Session, text: &str, extra: Value| -> Vec<Sewn> {
        let mut p = json!({"text": text, "font": "Test Script", "at": [100, 300]});
        p.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let id = NodeId(s.execute("embroidery.lettering", &p).unwrap()["id"].as_u64().unwrap());
        sewn(s, id)
    };
    let xs = |v: &[Sewn]| v.iter().map(|g| g.1).collect::<Vec<_>>();
    // left to right as it was; right to left; from the middle out: the second, first, third, fourth
    let plain = set(&mut s, "ABAB", json!({}));
    let l = xs(&plain);
    assert!(l.windows(2).all(|w| w[0] < w[1]), "{l:?}");
    assert!(plain.iter().all(|g| g.2.is_none() && g.3.is_none() && !g.4), "{plain:?}");
    let r = xs(&set(&mut s, "ABAB", json!({"order": "right"})));
    assert!(r.windows(2).all(|w| w[0] > w[1]), "{r:?}");
    let c = xs(&set(&mut s, "ABAB", json!({"order": "centre"})));
    assert!((c[0] - l[1]).abs() < 1e-9 && (c[1] - l[0]).abs() < 1e-9 && (c[2] - l[2]).abs() < 1e-9 && (c[3] - l[3]).abs() < 1e-9, "{c:?}");
    // the lines from the last up
    let u = set(&mut s, "A\nB", json!({"lines": "up"}));
    assert_eq!(u.iter().map(|g| g.0.as_str()).collect::<Vec<_>>(), ["B", "A"]);
    // along the baseline: each letter starts at its baseline's start and stops at its end, where the next starts
    let b = set(&mut s, "AA", json!({"join": "baseline"}));
    for g in &b {
        let (a, z) = (g.2.unwrap(), g.3.unwrap());
        assert!((a.y - 300.0).abs() < 1e-6 && (z.y - 300.0).abs() < 1e-6 && a.x < z.x, "{b:?}");
    }
    assert!((b[0].2.unwrap().x - 100.0).abs() < 1e-6 && (b[1].2.unwrap().x - b[0].3.unwrap().x).abs() < 1e-6, "{b:?}");
    // right to left along it: started at the end, stopped at the start
    let rb = set(&mut s, "AA", json!({"join": "baseline", "order": "right"}));
    assert!(rb.iter().all(|g| g.2.unwrap().x > g.3.unwrap().x), "{rb:?}");
    // the thread cut after each word, letter or line; not after the last
    let cut = |v: &[Sewn]| v.iter().map(|g| g.4).collect::<Vec<_>>();
    assert_eq!(cut(&set(&mut s, "AB A", json!({"trim": "word"}))), [false, true, false]);
    assert_eq!(cut(&set(&mut s, "AB\nA", json!({"trim": "line"}))), [false, true, false]);
    assert_eq!(cut(&set(&mut s, "AB A", json!({"trim": "letter"}))), [true, true, false]);
    // set again: what it said stays, what is given changes
    s.execute("embroidery.lettering.edit", &json!({"text": "BAB"})).unwrap();
    let id = s.doc().unwrap().selection.objects[0];
    assert_eq!(cut(&sewn(&s, id)), [true, true, false]);
    s.execute("embroidery.lettering.edit", &json!({"trim": "auto", "order": "centre"})).unwrap();
    let e = sewn(&s, id);
    assert_eq!((cut(&e), e[0].0.as_str()), (vec![false, false, false], "A"));
    // names it doesn't know
    for bad in [json!({"order": "middle"}), json!({"join": "nearest"}), json!({"trim": 5}), json!({"lines": "sideways"})] {
        assert!(s.execute("embroidery.lettering.edit", &bad).is_err(), "{bad}");
    }
}

#[test]
fn letters_move_scale_and_turn_one_at_a_time() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let id =
        NodeId(s.execute("embroidery.lettering", &json!({"text": "AAA", "font": "Test Script", "at": [100, 300]})).unwrap()["id"].as_u64().unwrap());
    let boxes = |s: &Session| -> Vec<vectorcraft_geom::Rect> {
        s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect()
    };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
    let b0 = boxes(&s);
    // the middle letter up 5 mm: only it moves
    let r = s.execute("embroidery.lettering.letter", &json!({"index": 1, "dy": -5})).unwrap();
    assert_eq!((r["index"].as_u64(), r["letter"]["dy"].as_f64()), (Some(1), Some(-5.0)));
    let b = boxes(&s);
    assert!(near(b[1].y1, b0[1].y1 - 5.0 * PT) && near(b[1].x0, b0[1].x0) && b[0] == b0[0] && b[2] == b0[2], "{b:?}");
    // twice the size about its baseline's middle, still up
    s.execute("embroidery.lettering.letter", &json!({"index": 1, "scale": 2})).unwrap();
    let b = boxes(&s);
    assert!(near(b[1].height(), b0[1].height() * 2.0) && near(b[1].y1, b0[1].y1 - 5.0 * PT), "{b:?}");
    // the first a quarter turn: as wide as it was tall
    s.execute("embroidery.lettering.letter", &json!({"index": 0, "angle": 90})).unwrap();
    let b = boxes(&s);
    assert!(near(b[0].width(), b0[0].height()), "{b:?}");
    // kept when set again with more text
    s.execute("embroidery.lettering.edit", &json!({"text": "AAAA"})).unwrap();
    let b = boxes(&s);
    assert!(b.len() == 4 && near(b[1].height(), b0[1].height() * 2.0) && near(b[3].height(), b0[2].height()), "{b:?}");
    // a letter selected inside the lettering, sewn right to left: the one it is
    s.execute("embroidery.lettering.edit", &json!({"order": "right"})).unwrap();
    let first_sewn = s.doc().unwrap().doc.node(id).unwrap().children().unwrap()[0].id;
    s.execute("select.set", &json!({"ids": [first_sewn.0]})).unwrap();
    assert_eq!(s.execute("embroidery.lettering.letter", &json!({"dx": 3})).unwrap()["index"].as_u64(), Some(3));
    // put back; undone
    s.execute("embroidery.lettering.letter", &json!({"id": id.0, "index": 1, "reset": true})).unwrap();
    let st = s.doc().unwrap();
    let Some(sf_core::attr::StitchType::Lettering(l)) = st.doc.node(id).unwrap().embroidery.as_deref().map(|e| &e.stitch) else { panic!() };
    assert_eq!(l.letters.iter().map(|a| a.index).collect::<Vec<_>>(), [0, 3]);
    s.execute("edit.undo", &json!({})).unwrap();
    let st = s.doc().unwrap();
    let Some(sf_core::attr::StitchType::Lettering(l)) = st.doc.node(id).unwrap().embroidery.as_deref().map(|e| &e.stitch) else { panic!() };
    assert_eq!(l.letters.len(), 3);
    // no letter that place, sizes out of range, the lettering itself with no index
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    for bad in [json!({"index": 9}), json!({"index": 0, "scale": 0}), json!({"index": 0, "angle": "x"}), json!({"dx": 1})] {
        assert!(s.execute("embroidery.lettering.letter", &bad).is_err(), "{bad}");
    }
}

#[test]
fn puff_sews_satin_over_foam_lettering_too() {
    use sf_core::attr::StitchType;
    let mut s = session();
    // a satin line and a fill: both puff, the fill as satin across it
    let line = s.execute("path.create", &json!({"d": "M 100 100 L 200 100"})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [line]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let sq = rect(&mut s, 300.0, 100.0, 40.0, 40.0);
    s.execute("select.set", &json!({"ids": [sq.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("select.set", &json!({"ids": [line, sq.0]})).unwrap();
    let r = s.execute("embroidery.puff", &json!({})).unwrap();
    assert_eq!((r["satin"].as_u64(), r["other"].as_u64(), r["on"].as_bool()), (Some(2), Some(0), Some(true)));
    let satin = |s: &Session| match s.doc().unwrap().doc.node(NodeId(line)).unwrap().embroidery.as_deref().map(|e| e.stitch.clone()) {
        Some(StitchType::Satin(p)) => p,
        other => panic!("{other:?}"),
    };
    let p = satin(&s);
    assert!(p.puff.is_some() && (p.spacing - 0.16).abs() < 1e-9 && !p.underlay.center_run && !p.underlay.auto);
    // again: all puff already, so off
    assert_eq!(s.execute("embroidery.puff", &json!({})).unwrap()["on"].as_bool(), Some(false));
    assert!(satin(&s).puff.is_none());
    // lettering: its glyphs' satin, kept when it is set again
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let id = NodeId(s.execute("embroidery.lettering", &json!({"text": "AB", "font": "Test Script", "puff": true})).unwrap()["id"].as_u64().unwrap());
    fn puffs(n: &vectorcraft_doc::Node, out: &mut Vec<bool>) {
        if let Some(c) = n.children().filter(|_| matches!(n.kind, vectorcraft_doc::NodeKind::Group { .. })) {
            c.iter().for_each(|k| puffs(k, out));
        } else if let Some(StitchType::Satin(p)) = n.embroidery.as_deref().map(|e| &e.stitch) {
            out.push(p.puff.is_some());
        }
    }
    let look = |s: &Session| {
        let mut v = Vec::new();
        puffs(s.doc().unwrap().doc.node(id).unwrap(), &mut v);
        v
    };
    assert!(!look(&s).is_empty() && look(&s).iter().all(|p| *p), "{:?}", look(&s));
    s.execute("embroidery.lettering.edit", &json!({"text": "AAB"})).unwrap();
    assert!(look(&s).iter().all(|p| *p));
    // off from the command, on the lettering itself
    s.execute("embroidery.puff", &json!({"ids": [id.0], "on": false})).unwrap();
    assert!(look(&s).iter().all(|p| !*p));
    s.execute("embroidery.lettering.edit", &json!({"text": "AB"})).unwrap();
    assert!(look(&s).iter().all(|p| !*p));
    assert!(s.execute("embroidery.puff", &json!({"cap": 9})).is_err());
    assert!(s.execute("embroidery.lettering.edit", &json!({"puff": "yes"})).is_err());
}

#[test]
fn puff_fills_and_puff_sewn_last() {
    use sf_core::attr::StitchType;
    let mut s = session();
    let ids: Vec<NodeId> = (0..3).map(|i| rect(&mut s, 100.0 + 100.0 * i as f64, 100.0, 40.0, 30.0)).collect();
    for id in &ids {
        s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    }
    assert!(s.execute("embroidery.puff.last", &json!({})).is_err());
    // the first filled shape made puff: satin across it; plain again, it fills again
    s.execute("embroidery.puff", &json!({"ids": [ids[0].0]})).unwrap();
    let kind = |s: &Session, id: NodeId| s.doc().unwrap().doc.node(id).unwrap().embroidery.as_deref().map(|e| e.stitch.clone());
    assert!(matches!(kind(&s, ids[0]), Some(StitchType::Satin(p)) if p.puff.is_some_and(|q| q.fill)));
    s.execute("embroidery.puff", &json!({"ids": [ids[0].0]})).unwrap();
    assert!(matches!(kind(&s, ids[0]), Some(StitchType::Fill(_))));
    s.execute("embroidery.puff", &json!({"ids": [ids[0].0]})).unwrap();
    // sewn last, with a stop after the shape sewn before it
    let r = s.execute("embroidery.puff.last", &json!({})).unwrap();
    assert_eq!((r["puff"].as_u64(), r["stop"].as_bool()), (Some(1), Some(true)));
    let st = s.doc().unwrap();
    let order: Vec<NodeId> = st.doc.layers[0].children().unwrap().iter().map(|n| n.id).collect();
    assert_eq!(order, [ids[1], ids[2], ids[0]]);
    let stops: Vec<bool> = ids.iter().map(|id| st.doc.node(*id).unwrap().embroidery.as_deref().unwrap().stop_after).collect();
    assert_eq!(stops, [false, false, true]);
    // one undo step
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(s.doc().unwrap().doc.layers[0].children().unwrap()[0].id, ids[0]);
}

#[test]
fn warped_lettering_sews_as_it_looks_and_stays_lettering() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "ABAB", "font": "Test Script", "at": [100, 300]})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let all = |s: &Session| -> Vec<sf_core::Point> {
        let plan = s.embroidery_plan().unwrap();
        plan.pattern().stitches.iter().filter(|s| s.cmd == sf_core::Cmd::Stitch).map(|s| s.pos()).collect()
    };
    let flat = all(&s);
    let width = |pts: &[sf_core::Point]| {
        let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
        hi - lo
    };
    let height = |pts: &[sf_core::Point]| {
        let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
        hi - lo
    };
    // a warp bends the stitches with the letters (an arc curves this line up, as the art), still sewn
    for style in ["arc", "flag", "bulge"] {
        s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
        s.execute("object.envelope.makeWithWarp", &json!({"style": style, "bend": 50})).unwrap();
        let warped = all(&s);
        assert!(!warped.is_empty() && warped != flat, "{style}");
        if style == "arc" {
            assert!(height(&warped) > height(&flat) * 1.2, "{style}: {} vs {}", height(&warped), height(&flat));
            assert!(width(&warped) > width(&flat) * 0.6, "{style}: {} vs {}", width(&warped), width(&flat));
        }
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(all(&s), flat, "{style}: undone");
    }
    // set again inside the envelope: still lettering, warped with its new text
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("object.envelope.makeWithWarp", &json!({"style": "arc", "bend": 50})).unwrap();
    let before = all(&s);
    s.execute("embroidery.lettering.edit", &json!({"id": id.0, "text": "AB"})).unwrap();
    let after = all(&s);
    assert!(!after.is_empty() && after.len() < before.len(), "{} → {}", before.len(), after.len());
}

#[test]
fn team_names_lay_out_together_and_list_on_a_worksheet() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "at": [100, 100]})).unwrap();
    let id = r["id"].as_u64().unwrap();
    let names = ["AB", "BA", "AAB", "B", "ABBA"];
    let r = s.execute("embroidery.teamNames.layout", &json!({"id": id, "names": names, "columns": 2, "gap": 5})).unwrap();
    assert_eq!((r["count"].as_u64(), r["columns"].as_u64(), r["rows"].as_u64()), (Some(5), Some(2), Some(3)), "{r}");
    let st = s.doc().unwrap();
    let group = st.doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap();
    let copies = group.children().unwrap();
    assert_eq!(copies.len(), 5);
    // each still lettering, saying its name; laid out in rows that don't overlap
    let said: Vec<String> = copies
        .iter()
        .map(|c| match c.embroidery.as_deref().map(|e| &e.stitch) {
            Some(sf_core::attr::StitchType::Lettering(l)) => l.text.clone(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(said, names);
    let b: Vec<_> = copies.iter().map(|c| c.geometric_bounds().unwrap()).collect();
    assert!(b[2].y0 >= b[0].y1 && b[1].x0 >= b[0].x1, "{b:?}");
    // the worksheet: a row a name and the totals
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    let r = s.execute("embroidery.teamNames.worksheet", &json!({"id": id, "names": names})).unwrap();
    assert_eq!(r["count"].as_u64(), Some(5));
    assert!(r["stitches"].as_u64().unwrap() > 0);
    let mut texts = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let NodeKind::Text(t) = &n.kind {
            texts.push(t.runs.iter().map(|r| r.text.clone()).collect::<String>());
        }
    });
    assert!(texts.iter().any(|t| t == "5. ABBA"), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("5 names")), "{texts:?}");
    s.execute("document.activate", &json!({"index": 0})).unwrap();
    for bad in [json!({"id": id, "names": []}), json!({"id": id, "names": ["A"], "columns": 0}), json!({"id": id, "names": ["A"], "gap": -1})] {
        assert!(s.execute("embroidery.teamNames.layout", &bad).is_err(), "{bad}");
    }
    assert!(s.execute("embroidery.teamNames.worksheet", &json!({"id": id, "names": ["A"], "page": "a3"})).is_err());
}

#[test]
fn monogram_templates_come_from_the_gallery_or_are_saved() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let list = s.execute("embroidery.monogram.templates", &json!({})).unwrap();
    let names: Vec<&str> = list["templates"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"Wedding heart") && names.len() >= 9, "{names:?}");
    // a gallery template sets the look; the request still wins where it says
    let r = s.execute("embroidery.monogram", &json!({"letters": "AB", "font": "Test Script", "template": "Double ring"})).unwrap();
    let rings = |s: &Session, id: u64| {
        let st = s.doc().unwrap();
        let mut n = 0;
        st.doc.node(NodeId(id)).unwrap().children().unwrap().iter().for_each(|c| {
            if c.name.as_deref().is_some_and(|n| n.starts_with("Border")) {
                n += 1;
            }
        });
        n
    };
    assert_eq!(rings(&s, r["id"].as_u64().unwrap()), 2, "two rings");
    let r = s.execute("embroidery.monogram", &json!({"letters": "AB", "font": "Test Script", "template": "Double ring", "rings": 1})).unwrap();
    assert_eq!(rings(&s, r["id"].as_u64().unwrap()), 1, "the request's rings win");
    // one of the user's own
    s.execute("embroidery.monogram.saveTemplate", &json!({"name": "Shop crest", "border": "shield", "rings": 3, "ornament": "stars", "size": 25}))
        .unwrap();
    assert_eq!(s.prefs.embroidery_monogram_templates.len(), 1);
    s.execute("embroidery.monogram", &json!({"letters": "A", "font": "Test Script", "template": "shop crest"})).unwrap();
    let list = s.execute("embroidery.monogram.templates", &json!({})).unwrap();
    let mine = list["templates"].as_array().unwrap().iter().find(|t| t["name"] == "Shop crest").unwrap().clone();
    assert_eq!((mine["builtin"].as_bool(), mine["rings"].as_u64(), mine["size"].as_f64()), (Some(false), Some(3), Some(25.0)));
    // the gallery's names are taken; bad values; deleting
    assert!(s.execute("embroidery.monogram.saveTemplate", &json!({"name": "Wedding heart"})).is_err());
    assert!(s.execute("embroidery.monogram.saveTemplate", &json!({"name": "X", "border": "star"})).is_err());
    assert!(s.execute("embroidery.monogram", &json!({"letters": "A", "font": "Test Script", "template": "nope"})).is_err());
    s.execute("embroidery.monogram.deleteTemplate", &json!({"name": "Shop crest"})).unwrap();
    assert!(s.execute("embroidery.monogram.deleteTemplate", &json!({"name": "Classic circle"})).is_err());
}

#[test]
fn lettering_stacks_vertically_and_follows_drawn_lines() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let boxes = |s: &Session, id: NodeId| -> Vec<vectorcraft_geom::Rect> {
        let st = s.doc().unwrap();
        st.doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect()
    };
    // vertical: one under another, centred on one line
    let r = s.execute("embroidery.lettering", &json!({"text": "AAA", "font": "Test Script", "at": [300, 100], "vertical": true})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let b = boxes(&s, id);
    assert!(b[0].y1 < b[1].y0 && b[1].y1 < b[2].y0, "{b:?}");
    assert!(b.iter().all(|r| (r.center().x - b[0].center().x).abs() < 0.5), "{b:?}");
    s.execute("embroidery.lettering.edit", &json!({"vertical": false})).unwrap();
    let b = boxes(&s, id);
    assert!(b[0].x1 <= b[1].x0 + 1e-6, "level again: {b:?}");
    // along a drawn shape: a long box, the text along its top edge from its first corner
    let line = rect(&mut s, 100.0, 590.0, 600.0, 20.0);
    let r = s.execute("embroidery.lettering", &json!({"text": "AAAA", "font": "Test Script", "path": line.0})).unwrap();
    let along = NodeId(r["id"].as_u64().unwrap());
    let b = boxes(&s, along);
    assert!(b.windows(2).all(|w| w[0].center().x < w[1].center().x), "along the line: {b:?}");
    assert!(b.iter().all(|r| r.y1 <= 596.0 && r.y1 >= 560.0), "on it: {b:?}");
    // straight again
    s.execute("embroidery.lettering.edit", &json!({"path": null})).unwrap();
    assert!(s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "path": 999_999})).is_err());
    assert!(s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "path": "x"})).is_err());
}

#[test]
fn lettering_sews_as_the_font_has_it_all_satin_or_all_fill_and_small() {
    use sf_core::attr::StitchType;
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "AB", "font": "Test Script", "size": 10})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let stitches = |s: &Session| -> Vec<Vec<StitchType>> {
        let st = s.doc().unwrap();
        st.doc
            .node(id)
            .unwrap()
            .children()
            .unwrap()
            .iter()
            .map(|g| g.children().unwrap().iter().filter_map(|c| c.embroidery.as_deref()).map(|e| e.stitch.clone()).collect())
            .collect()
    };
    // the font's own: A a satin column and a run, B a fill
    let font = stitches(&s);
    assert!(matches!(font[1][0], StitchType::Fill(_)), "{:?}", font[1]);
    // all satin: B's fill sewn as satin
    s.execute("embroidery.lettering.edit", &json!({"stitch": "satin"})).unwrap();
    assert!(matches!(stitches(&s)[1][0], StitchType::Satin(_)));
    // all fill: each letter one fill
    s.execute("embroidery.lettering.edit", &json!({"stitch": "fill"})).unwrap();
    let fills = stitches(&s);
    assert!(fills.iter().all(|g| g.len() == 1 && matches!(g[0], StitchType::Fill(_))), "{fills:?}");
    // back as the font has it
    s.execute("embroidery.lettering.edit", &json!({"stitch": "font"})).unwrap();
    assert_eq!(stitches(&s).len(), 2);
    assert!(matches!(stitches(&s)[0][0], StitchType::Satin(_)));
    // 10 mm: not small; 5 mm: a centre run under the satin, half the pull compensation
    let StitchType::Satin(big) = &stitches(&s)[0][0] else { panic!("satin") };
    assert!(big.underlay.auto || big.underlay.zigzag || big.underlay.edge_run);
    s.execute("embroidery.lettering.edit", &json!({"size": 5})).unwrap();
    let StitchType::Satin(small) = &stitches(&s)[0][0] else { panic!("satin") };
    assert!(!small.underlay.auto && small.underlay.center_run && !small.underlay.zigzag && small.short_stitches, "{small:?}");
    // off at 5 mm: the font's own again
    s.execute("embroidery.lettering.edit", &json!({"small": "off"})).unwrap();
    let StitchType::Satin(off) = &stitches(&s)[0][0] else { panic!("satin") };
    assert_eq!(off.underlay, big.underlay);
    assert!((small.pull_comp - off.pull_comp * 0.5).abs() < 1e-9, "{small:?} {off:?}");
    for bad in [json!({"stitch": "zigzag"}), json!({"small": "yes"})] {
        assert!(s.execute("embroidery.lettering.edit", &bad).is_err(), "{bad}");
    }
}

#[test]
fn applique_lettering_makes_each_letter_a_patch() {
    use sf_core::attr::StitchType;
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "AA", "font": "Test Script", "applique": "#336699"})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let kinds = |s: &Session| -> Vec<Vec<String>> {
        let st = s.doc().unwrap();
        st.doc
            .node(id)
            .unwrap()
            .children()
            .unwrap()
            .iter()
            .map(|g| g.children().unwrap().iter().filter_map(|c| c.embroidery.as_deref()).map(|e| e.stitch.type_name().to_string()).collect())
            .collect()
    };
    assert_eq!(kinds(&s), vec![vec!["applique".to_string()]; 2]);
    {
        let st = s.doc().unwrap();
        let g = &st.doc.node(id).unwrap().children().unwrap()[0];
        let e = g.children().unwrap()[0].embroidery.as_deref().unwrap().clone();
        let StitchType::Applique(a) = &e.stitch else { panic!("appliqué") };
        assert_eq!(a.fabric_color, sf_core::Rgb::new(0x33, 0x66, 0x99));
    }
    // sewn as appliqué: the machine stops to lay and trim the fabric
    let p = s.embroidery_plan().unwrap().pattern().clone();
    assert!(p.stitches.iter().filter(|s| s.cmd == sf_core::Cmd::Stop).count() >= 2, "stops for the fabric");
    // off again: the font's own stitching
    s.execute("embroidery.lettering.edit", &json!({"applique": false})).unwrap();
    assert!(kinds(&s).iter().flatten().all(|k| k != "applique"), "{:?}", kinds(&s));
    s.execute("embroidery.lettering.edit", &json!({"applique": true})).unwrap();
    assert_eq!(kinds(&s)[0], vec!["applique".to_string()]);
    assert!(s.execute("embroidery.lettering.edit", &json!({"applique": "blue"})).is_err());
}

#[test]
fn fonts_with_glyph_layers_inside_other_layers_load() {
    // the glyph layers grouped inside a layer of their own, as some fonts keep them
    let nested = TEST_FONT_SVG
        .replacen(
            r#"<g inkscape:groupmode="layer" inkscape:label="GlyphLayer-A""#,
            r#"<g inkscape:groupmode="layer" inkscape:label="Glyphs" id="all"><g inkscape:groupmode="layer" inkscape:label="GlyphLayer-A""#,
            1,
        )
        .replacen("</svg>", "</g></svg>", 1);
    assert_ne!(nested, TEST_FONT_SVG);
    let mut s = session();
    let r = s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": nested})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(3), "{r}");
    let r = s.execute("embroidery.lettering", &json!({"text": "AB", "font": "Test Script"})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(2));
}

#[test]
fn lettering_aligns_and_follows_arcs() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let set = |s: &mut Session, extra: serde_json::Value| -> Vec<vectorcraft_geom::Rect> {
        let mut p = json!({"text": "AAA", "font": "Test Script", "at": [300, 300]});
        p.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        let id = NodeId(s.execute("embroidery.lettering", &p).unwrap()["id"].as_u64().unwrap());
        s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect()
    };
    // "AAA" is 120 font units of 1 mm: starting, centred and ending on the origin, half a line apart
    let half = 60.0 * 72.0 / 25.4;
    let (l, c, r) = (set(&mut s, json!({})), set(&mut s, json!({"align": "centre"})), set(&mut s, json!({"align": "right"})));
    assert!((l[0].x0 - c[0].x0 - half).abs() < 0.01 && (c[0].x0 - r[0].x0 - half).abs() < 0.01, "{l:?} {c:?} {r:?}");
    // arched over a circle 40 mm in radius: the middle A highest; along its bottom: lowest
    let a = set(&mut s, json!({"align": "centre", "arc": 40}));
    assert!(a[1].y0 < a[0].y0 - 2.0 && a[1].y0 < a[2].y0 - 2.0, "{a:?}");
    let v = set(&mut s, json!({"align": "centre", "arc": -40}));
    assert!(v[1].y1 > v[0].y1 + 2.0 && v[1].y1 > v[2].y1 + 2.0, "{v:?}");
    // straightened where it is: level again
    s.execute("embroidery.lettering.edit", &json!({"arc": 0})).unwrap();
    let id = s.doc().unwrap().selection.objects[0];
    let st: Vec<_> = s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect();
    assert!(st.iter().all(|b| (b.y1 - st[0].y1).abs() < 0.01), "{st:?}");
    // bad arcs and alignments
    assert!(s.execute("embroidery.lettering.edit", &json!({"arc": 0.5})).is_err());
    assert!(s.execute("embroidery.lettering.edit", &json!({"arc": 1e9})).is_err());
    assert!(s.execute("embroidery.lettering.edit", &json!({"align": "middle"})).is_err());
    assert!(s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "arc": "x"})).is_err());
    // round a spiral: winding in (each A is 40 mm wide: 6 go a fifth of the way round, the
    // radius 11 mm less by the last)
    let sp = set(&mut s, json!({"text": "AAAAAA", "arc": 200, "spiral": 60}));
    let centre = vectorcraft_geom::Point::new(300.0, 300.0 + 200.0 * 72.0 / 25.4);
    let d: Vec<f64> = sp.iter().map(|b| b.center().distance(centre)).collect();
    let round = set(&mut s, json!({"text": "AAAAAA", "arc": 200}));
    let r: Vec<f64> = round.iter().map(|b| b.center().distance(centre)).collect();
    // against the same glyphs round the circle: further in at each
    let gap: Vec<f64> = d.iter().zip(&r).map(|(a, b)| a - b).collect();
    assert!(gap.windows(2).all(|w| w[1] < w[0]) && gap[5] < -20.0, "{gap:?}");
    s.execute("embroidery.lettering.edit", &json!({"spiral": 15})).unwrap();
    let spiral = |s: &Session| {
        let st = s.doc().unwrap();
        match st.doc.node(st.selection.objects[0]).unwrap().embroidery.as_deref().map(|e| &e.stitch) {
            Some(sf_core::attr::StitchType::Lettering(l)) => l.spiral,
            _ => f64::NAN,
        }
    };
    assert_eq!(spiral(&s), 15.0);
    // straightened: no spiral; a spiral needs an arc; out of range
    s.execute("embroidery.lettering.edit", &json!({"arc": 0})).unwrap();
    assert_eq!(spiral(&s), 0.0);
    assert!(s.execute("embroidery.lettering.edit", &json!({"spiral": 10})).is_err());
    assert!(s.execute("embroidery.lettering.edit", &json!({"arc": 40, "spiral": 5000})).is_err());
    assert!(s.execute("embroidery.lettering", &json!({"text": "A", "font": "Test Script", "spiral": 10})).is_err());
}

#[test]
fn lettering_is_set_again_where_it_is_now() {
    let mut s = session();
    s.execute("embroidery.font.load", &json!({"json": TEST_FONT_JSON, "svg": TEST_FONT_SVG})).unwrap();
    let r = s.execute("embroidery.lettering", &json!({"text": "AB", "font": "Test Script", "at": [100, 300]})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    let spec = |s: &Session| match &s.doc().unwrap().doc.node(id).unwrap().embroidery.as_deref().unwrap().stitch {
        sf_core::attr::StitchType::Lettering(l) => (**l).clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!((spec(&s).text.as_str(), spec(&s).size), ("AB", 10.0));
    let boxes = |s: &Session| -> Vec<vectorcraft_geom::Rect> {
        s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.geometric_bounds().unwrap()).collect()
    };
    let near = |a: &[vectorcraft_geom::Rect], b: &[vectorcraft_geom::Rect]| {
        a.len() == b.len()
            && a.iter()
                .zip(b)
                .all(|(p, q)| (p.x0 - q.x0).abs() < 0.01 && (p.y0 - q.y0).abs() < 0.01 && (p.x1 - q.x1).abs() < 0.01 && (p.y1 - q.y1).abs() < 0.01)
    };
    // moved, doubled and turned: the size follows the scale
    s.execute("object.move", &json!({"dx": 50, "dy": 0})).unwrap();
    s.execute("object.scale", &json!({"sx": 200, "origin": [150, 300]})).unwrap();
    s.execute("object.rotate", &json!({"angle": 30, "origin": [150, 300]})).unwrap();
    assert!((spec(&s).size - 20.0).abs() < 1e-6, "{:?}", spec(&s));
    // set again as it is: the glyphs come back where the transforms put them
    let moved = boxes(&s);
    s.execute("embroidery.lettering.edit", &json!({})).unwrap();
    assert!(near(&boxes(&s), &moved), "{moved:?} {:?}", boxes(&s));
    // a new text, at half the size: "BA", the B where the A began
    let r = s.execute("embroidery.lettering.edit", &json!({"text": "BA", "size": 10})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(2));
    let names: Vec<String> = s.doc().unwrap().doc.node(id).unwrap().children().unwrap().iter().map(|g| g.name.clone().unwrap()).collect();
    assert_eq!(names, ["B", "A"]);
    assert_eq!((spec(&s).text.as_str(), spec(&s).size), ("BA", 10.0));
    assert!(s.doc().unwrap().doc.node(id).unwrap().name.as_deref().unwrap().starts_with("BA"));
    // one undo step back to "AB"
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(near(&boxes(&s), &moved));
    // kept through save and open
    let doc = s.doc().unwrap().doc.clone();
    let back = vectorcraft_format::load(&vectorcraft_format::save(&doc, false)).unwrap();
    assert_eq!(back.node(id).unwrap().embroidery, doc.node(id).unwrap().embroidery);
    // a font that isn't loaded, bad sizes, nothing selected
    assert!(s.execute("embroidery.lettering.edit", &json!({"font": "Nope"})).is_err());
    assert!(s.execute("embroidery.lettering.edit", &json!({"size": 0})).is_err());
    assert!(s.execute("embroidery.lettering.edit", &json!({"text": ""})).is_err());
    // a glyph selected inside it edits the lettering
    let glyph = s.doc().unwrap().doc.node(id).unwrap().children().unwrap()[0].id;
    s.execute("select.set", &json!({"ids": [glyph.0]})).unwrap();
    assert!(s.execute("embroidery.lettering.edit", &json!({"text": "A"})).is_ok());
    assert_eq!(spec(&s).text, "A");
    // broken apart: its glyphs, no longer lettering
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("embroidery.breakApart", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(id).is_none());
    assert!(s.execute("embroidery.lettering.edit", &json!({"text": "B"})).is_err());
    assert!(stitches(&s) > 50);
}

#[test]
fn a_laydown_is_sewn_first_under_the_selected_shapes() {
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#336699"})).unwrap();
    let b = rect(&mut s, 160.0, 140.0, 80.0, 80.0);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let id = NodeId(s.execute("embroidery.addLaydown", &json!({"margin": 2.0})).unwrap()["id"].as_u64().unwrap());
    // Below both, in the bottom one's thread, as an open tatami with no underlay.
    let order = s.doc().unwrap().doc.paint_order([id, a, b]);
    assert_eq!(order, vec![id, a, b]);
    let e = emb(&s, id);
    assert_eq!(e.thread.as_ref().map(|t| t.color), Some(Rgb::new(0x33, 0x66, 0x99)));
    let f = fill_params(&s, id);
    assert!(f.trapunto && !f.underlay.edge_run && !f.underlay.tatami && (f.spacing - 2.5).abs() < 1e-9);
    // It covers both shapes and 2 mm more: from 100 − 2 mm to 240 + 2 mm across.
    let pts = needles(&s, id);
    let (lo, hi) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
    assert!((lo - (100.0 * MM - 2.0)).abs() < 0.6 && (hi - (240.0 * MM + 2.0)).abs() < 0.6, "{lo}..{hi}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(id).is_none());
    // Lines alone cover nothing; bad values change nothing.
    let line = NodeId(s.execute("path.create", &json!({"d": "M 0 0 L 50 50"})).unwrap()["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [line.0]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "run"})).unwrap();
    assert!(s.execute("embroidery.addLaydown", &json!({})).is_err());
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    for bad in [json!({"margin": -1}), json!({"spacing": 0.2}), json!({"angle": "x"})] {
        assert!(s.execute("embroidery.addLaydown", &bad).is_err(), "{bad}");
    }
}

#[test]
fn a_fill_blends_into_other_colours_across_its_rows() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a).len() as f64;
    let r = s.execute("embroidery.makeBlend", &json!({"colors": ["#ff0000", "#ffff00", "#0000ff"], "separate": true})).unwrap();
    let ids: Vec<NodeId> = r["ids"].as_array().unwrap().iter().map(|v| NodeId(v.as_u64().unwrap())).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0], a, "the fill itself is the first layer");
    assert_eq!(s.doc().unwrap().doc.paint_order(ids.clone()), ids, "stacked in order");
    let colors: Vec<Rgb> = ids.iter().map(|id| emb(&s, *id).thread.unwrap().color).collect();
    assert_eq!(colors, vec![Rgb::new(255, 0, 0), Rgb::new(255, 255, 0), Rgb::new(0, 0, 255)]);
    let (f0, f1, f2) = (fill_params(&s, ids[0]), fill_params(&s, ids[1]), fill_params(&s, ids[2]));
    assert_eq!(f0.spacing_profile, vec![[0.0, 0.4], [0.5, 0.0]]);
    assert_eq!(f1.spacing_profile, vec![[0.0, 0.0], [0.5, 0.4], [1.0, 0.0]]);
    assert_eq!(f2.spacing_profile, vec![[0.5, 0.0], [1.0, 0.4]]);
    assert!(f0.underlay.tatami && !f1.underlay.tatami && !f2.underlay.edge_run, "underlay once");
    // The layers together sew about as much as the plain fill.
    let total: f64 = ids.iter().map(|id| needles(&s, *id).len() as f64).sum();
    assert!((0.7..1.4).contains(&(total / plain)), "{total} vs {plain}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(ids[1]).is_none() && fill_params(&s, a).spacing_profile.is_empty());
    for bad in [json!({"colors": ["#ff0000"]}), json!({"colors": ["#ff0000", "plaid"]}), json!({"colors": vec!["#ff0000"; 7]}), json!({})] {
        assert!(s.execute("embroidery.makeBlend", &bad).is_err(), "{bad}");
    }
    // A spacing profile through setParams.
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"spacingProfile": [[0.0, 0.4], [0.5, 2.0], [1.0, 0.4]]}})).unwrap();
    assert!(((needles(&s, a).len() as f64) / plain) < 0.8, "sparser in the middle");
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    assert!(s.execute("embroidery.makeBlend", &json!({"colors": ["#ff0000", "#0000ff"]})).is_err(), "tatami only");
}

#[test]
fn outlines_and_borders_are_sewn_after_the_shapes() {
    const MM: f64 = vectorcraft_embroidery::MM_PER_PT;
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 100.0, 60.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#aa2233"})).unwrap();
    // A satin outline 1 mm out from the shape, above it, in its thread.
    let id = NodeId(s.execute("embroidery.addOutline", &json!({})).unwrap()["id"].as_u64().unwrap());
    assert_eq!(s.doc().unwrap().doc.paint_order([id, a]), vec![a, id]);
    let e = emb(&s, id);
    assert!(matches!(e.stitch, sf_core::attr::StitchType::Satin(_)));
    assert_eq!(e.thread.map(|t| t.color), Some(Rgb::new(0xaa, 0x22, 0x33)));
    let span = |id: NodeId, s: &Session| {
        let pts = needles(s, id);
        pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)))
    };
    let (lo, hi) = span(id, &s);
    // the satin column (2.5 mm) centred 1 mm out
    assert!((lo - (100.0 * MM - 1.0 - 1.25)).abs() < 0.6 && (hi - (200.0 * MM + 1.0 + 1.25)).abs() < 0.6, "{lo}..{hi}");
    // A rectangle border 4 mm out, a motif border, an oval.
    for (params, kind) in [
        (json!({"ids": [a.0], "shape": "rectangle", "offset": 4, "stitch": "bean"}), "run"),
        (json!({"ids": [a.0], "shape": "rounded", "stitch": "motif", "motif": "scallop"}), "run"),
        (json!({"ids": [a.0], "shape": "oval", "stitch": "run"}), "run"),
    ] {
        let id = NodeId(s.execute("embroidery.addOutline", &params).unwrap()["id"].as_u64().unwrap());
        assert_eq!(emb(&s, id).stitch.id(), kind, "{params}");
        assert!(needles(&s, id).len() > 20, "{params}");
    }
    for bad in [
        json!({"ids": [a.0], "shape": "hexagon"}),
        json!({"ids": [a.0], "stitch": "fill"}),
        json!({"ids": [a.0], "offset": 1e9}),
        json!({"ids": [a.0], "motif": "dragon"}),
    ] {
        assert!(s.execute("embroidery.addOutline", &bad).is_err(), "{bad}");
    }
}

#[test]
fn repeats_and_envelopes_are_sewn_as_they_look() {
    let mut s = session();
    let a = rect(&mut s, 200.0, 100.0, 40.0, 20.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let one = stitches(&s);
    // A radial repeat of six: every copy is sewn, where it shows.
    let rep = s.execute("object.repeat.radial", &json!({"ids": [a.0], "instances": 6, "radius": 80})).unwrap();
    assert!(rep["id"].as_u64().is_some());
    let six = stitches(&s);
    assert!(six as f64 > 5.0 * one as f64, "{one} → {six}");
    let plan = s.embroidery_plan().unwrap();
    assert_eq!(plan.records_of(a).len(), 6, "selecting the source shows all six");
    // The plan of an unchanged document is reused.
    assert!(std::sync::Arc::ptr_eq(&plan, &s.embroidery_plan().unwrap()));
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(stitches(&s), one);
    // An envelope bends the stitching with the art.
    let before: Vec<sf_core::Point> = needles(&s, a);
    s.execute("object.envelope.makeWithWarp", &json!({"ids": [a.0], "style": "arc", "bend": 50})).unwrap();
    let after = needles(&s, a);
    assert!(!after.is_empty() && after != before);
}

#[test]
fn cut_files_hold_stone_holes_and_applique_pieces() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    // nothing to cut yet
    rect(&mut s, 10.0 * PT, 10.0 * PT, 20.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert!(s.execute("embroidery.cutFile", &json!({})).is_err());
    // a line of 3 mm stones and an appliqué square
    s.execute("path.create", &json!({"d": format!("M {} {} L {} {}", 40.0 * PT, 20.0 * PT, 70.0 * PT, 20.0 * PT)})).unwrap();
    s.execute("embroidery.set", &json!({"type": "stone"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"size": 3}})).unwrap();
    rect(&mut s, 10.0 * PT, 40.0 * PT, 20.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "applique"})).unwrap();
    let r = s.execute("embroidery.cutFile", &json!({"what": "stones"})).unwrap();
    let holes = r["holes"].as_u64().unwrap();
    assert!(holes >= 5, "{r}");
    assert_eq!(r["pieces"], 0);
    let svg = r["text"].as_str().unwrap();
    assert_eq!(svg.matches("<circle").count() as u64, holes);
    assert!(svg.contains("r=\"1.650\""), "holes 0.3 mm larger than the stones: {svg}");
    assert_eq!(r["stones"][0]["count"].as_u64(), Some(holes));
    // everything as DXF: one circle a stone, four lines for the square
    let r = s.execute("embroidery.cutFile", &json!({"format": "dxf"})).unwrap();
    let dxf = r["text"].as_str().unwrap();
    assert!(dxf.starts_with("0\nSECTION\n2\nENTITIES\n") && dxf.ends_with("0\nEOF\n"));
    assert_eq!(dxf.matches("\nCIRCLE\n").count() as u64, holes);
    assert!(dxf.matches("\nLINE\n").count() >= 4);
    // mirrored: the stones (right of the square) swap to its left
    let x_of_first_circle = |svg: &str| -> f64 {
        let i = svg.find("<circle cx=\"").unwrap() + 12;
        svg[i..].split('"').next().unwrap().parse().unwrap()
    };
    let plain = s.execute("embroidery.cutFile", &json!({})).unwrap();
    let mirrored = s.execute("embroidery.cutFile", &json!({"mirror": true})).unwrap();
    assert_eq!(mirrored["mirrored"], true);
    let (a, b) = (x_of_first_circle(plain["text"].as_str().unwrap()), x_of_first_circle(mirrored["text"].as_str().unwrap()));
    assert!(a > 30.0 && b < 40.0, "{a} {b}");
    // bad values are refused
    assert!(s.execute("embroidery.cutFile", &json!({"format": "pdf"})).is_err());
    assert!(s.execute("embroidery.cutFile", &json!({"what": "sequins"})).is_err());
    // opened as a document to print
    let docs = s.documents().len();
    let r = s.execute("embroidery.cutFile", &json!({"open": true})).unwrap();
    assert_eq!(r["opened"], true);
    assert_eq!(s.documents().len(), docs + 1);
}

#[test]
fn applique_pieces_combine_and_are_prepared() {
    use sf_core::model::ObjectKind;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let stops = |s: &Session| s.embroidery_plan().unwrap().pattern().stitches.iter().filter(|st| st.cmd == sf_core::Cmd::Stop).count();
    let a = rect(&mut s, 10.0 * PT, 10.0 * PT, 20.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "applique"})).unwrap();
    let b = rect(&mut s, 50.0 * PT, 10.0 * PT, 20.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "applique"})).unwrap();
    assert_eq!(stops(&s), 4, "each piece its own two stops");
    // combined: one object of two pieces, two stops in all
    s.execute("select.set", &json!({"ids": [a.0, b.0]})).unwrap();
    let r = s.execute("embroidery.applique.combine", &json!({})).unwrap();
    assert_eq!(r["pieces"], 2, "{r}");
    assert_eq!(stops(&s), 2);
    let plan = s.embroidery_plan().unwrap();
    let appl: Vec<_> = plan.design.objects.iter().filter(|o| matches!(o.kind, ObjectKind::Applique(_))).collect();
    assert_eq!(appl.len(), 1);
    // undo brings the pieces back
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(stops(&s), 4);
    // the fabric's look is kept with the piece
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"fabric_pattern": "Check", "fabricColor2": "#203040"}})).unwrap();
    let e = s.doc().unwrap().doc.node(a).unwrap().embroidery.clone().unwrap();
    let sf_core::attr::StitchType::Applique(p) = &e.stitch else { panic!("appliqué") };
    assert_eq!(p.fabric_pattern, sf_core::model::FabricPattern::Check);
    assert_eq!(p.fabric_color_2, sf_core::Rgb::new(0x20, 0x30, 0x40));
    // prepare: an open outline is closed, overlapping pieces broken up
    let open = NodeId(
        s.execute("path.create", &json!({"d": format!("M {} {} L {} {} L {} {}", 20.0 * PT, 20.0 * PT, 40.0 * PT, 20.0 * PT, 40.0 * PT, 40.0 * PT)}))
            .unwrap()["id"]
            .as_u64()
            .unwrap(),
    );
    s.execute("embroidery.set", &json!({"type": "applique"})).unwrap();
    s.execute("select.set", &json!({"ids": [a.0, open.0]})).unwrap();
    let r = s.execute("embroidery.applique.prepare", &json!({})).unwrap();
    assert_eq!(r["closed"], 1, "{r}");
    assert!(!r["ids"].as_array().unwrap().is_empty());
    // the pieces don't overlap any more: their areas add up to their union's
    let plan = s.embroidery_plan().unwrap();
    let polys: Vec<_> =
        plan.design.objects.iter().filter(|o| matches!(o.kind, ObjectKind::Applique(_))).flat_map(sf_core::engine::object_polygons).collect();
    let sum: f64 = polys.iter().map(|p| p.area().abs()).sum();
    let union: f64 = sf_core::geom::poly::union(&polys).iter().map(|p| p.area().abs()).sum();
    assert!((sum - union).abs() < 1.0, "{sum} {union}");
}

#[test]
fn print_and_foil_are_shown_and_never_sewn() {
    use sf_core::decor::DecorKind;
    use sf_core::model::ObjectKind;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 10.0 * PT, 10.0 * PT, 30.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let sewn = stitches(&s);
    for (kind, name) in [(DecorKind::Print, "print"), (DecorKind::Foil, "foil")] {
        rect(&mut s, 50.0 * PT, 10.0 * PT, 30.0 * PT, 20.0 * PT);
        s.execute("embroidery.set", &json!({"type": name})).unwrap();
        let plan = s.embroidery_plan().unwrap();
        let d = plan
            .design
            .objects
            .iter()
            .find_map(|o| match &o.kind {
                ObjectKind::Decor(d) if d.params.kind == kind => Some(d.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(d.lines.len(), 1, "{name}: the shape's outline");
        assert_eq!(stitches(&s), sewn, "{name} isn't sewn");
        assert!(!plan.pattern().stitches.iter().any(|st| st.cmd == sf_core::Cmd::Stop), "{name}: no stop");
    }
}

#[test]
fn default_properties_saved_for_every_design() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let satin_pull = |s: &Session, id: NodeId| -> f64 {
        let e = s.doc().unwrap().doc.node(id).unwrap().embroidery.clone().unwrap();
        let sf_core::attr::StitchType::Satin(p) = &e.stitch else { panic!("satin") };
        p.pull_comp
    };
    // nothing to save before the design has its own
    assert!(s.execute("embroidery.current.saveAsDefault", &json!({})).is_err());
    s.execute("embroidery.current.setParams", &json!({"type": "satin", "params": {"pullComp": 0.7}})).unwrap();
    assert_eq!(s.execute("embroidery.current.saveAsDefault", &json!({"type": "satin"})).unwrap()["saved"], json!(["satin"]));
    // a new design: its satin takes 0.7 without setting anything
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let a = rect(&mut s, 10.0 * PT, 10.0 * PT, 5.0 * PT, 40.0 * PT);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    assert!((satin_pull(&s, a) - 0.7).abs() < 1e-9);
    assert_eq!(s.execute("embroidery.current", &json!({})).unwrap()["defaults"]["satin"]["stitch"]["pull_comp"], json!(0.7));
    // the design's own properties come first
    s.execute("embroidery.current.setParams", &json!({"type": "satin", "params": {"pullComp": 0.3}})).unwrap();
    let b = rect(&mut s, 30.0 * PT, 10.0 * PT, 5.0 * PT, 40.0 * PT);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    assert!((satin_pull(&s, b) - 0.3).abs() < 1e-9);
    // cleared: the built-in value again
    assert_eq!(s.execute("embroidery.current.clearDefault", &json!({})).unwrap()["cleared"], 1);
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    let c = rect(&mut s, 10.0 * PT, 10.0 * PT, 5.0 * PT, 40.0 * PT);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    assert!((satin_pull(&s, c) - 0.7).abs() > 1e-6);
}

#[test]
fn saving_a_stitched_design_as_svg_keeps_its_stitches() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    rect(&mut s, 10.0 * PT, 10.0 * PT, 30.0 * PT, 20.0 * PT);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"pullComp": 0.7}})).unwrap();
    let path = std::env::temp_dir().join(format!("sf-svg-save-{}.svg", std::process::id()));
    let path = path.to_string_lossy().to_string();
    s.execute("document.save", &json!({"path": path})).unwrap();
    // Ctrl+S again: the SVG path, written in place
    let again = s.execute("document.save", &json!({})).unwrap();
    assert_eq!(again["path"].as_str(), Some(path.as_str()), "{again}");
    s.execute("document.open", &json!({"path": path})).unwrap();
    let mut pulls = Vec::new();
    s.doc().unwrap().doc.walk(|n| {
        if let Some(e) = n.embroidery.as_deref()
            && let sf_core::attr::StitchType::Satin(p) = &e.stitch
        {
            pulls.push(p.pull_comp);
        }
    });
    let _ = std::fs::remove_file(&path);
    assert_eq!(pulls, vec![0.7], "the stitch settings come back from the SVG");
}

#[test]
fn check_design_follows_the_stitch_length_rules_for_clothing_and_other_items() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let kinds = |s: &mut Session| -> Vec<String> {
        let r = s.execute("embroidery.check", &json!({})).unwrap();
        r["findings"].as_array().unwrap().iter().map(|f| f["kind"].as_str().unwrap().to_string()).collect()
    };
    // a 9 mm satin, not split: too long on clothing, fine on a bag
    rect(&mut s, 10.0 * PT, 10.0 * PT, 40.0 * PT, 9.0 * PT);
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"splitLength": 0}})).unwrap();
    assert!(kinds(&mut s).contains(&"longStitches".to_string()));
    s.execute("embroidery.settings", &json!({"checks": {"garment": false}})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!((info["settings"]["checks"]["garment"].as_bool(), info["settings"]["checks"]["longStitch"].as_f64()), (Some(false), Some(12.1)));
    assert!(!kinds(&mut s).contains(&"longStitches".to_string()));
    s.execute("embroidery.settings", &json!({"checks": {"garment": true}})).unwrap();
    // a satin line 0.6 mm wide: too narrow to sit up
    s.execute("path.create", &json!({"d": format!("M {} {} L {} {}", 10.0 * PT, 40.0 * PT, 60.0 * PT, 40.0 * PT)})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"width": 0.6}})).unwrap();
    assert!(kinds(&mut s).contains(&"narrowSatin".to_string()));
    // a fill with 2 mm stitches: may ripple after washing
    rect(&mut s, 10.0 * PT, 60.0 * PT, 30.0 * PT, 30.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    assert!(!kinds(&mut s).contains(&"shortFill".to_string()), "the 4 mm default is fine");
    s.execute("embroidery.setParams", &json!({"params": {"stitchLength": 2.0}})).unwrap();
    assert!(kinds(&mut s).contains(&"shortFill".to_string()));
    // hand-placed stitches 0.2 mm apart: the needle sews in one spot
    let d: String = (0..30).map(|i| format!("{} {} {} ", if i == 0 { "M" } else { "L" }, (100.0 + 0.2 * f64::from(i)) * PT, 100.0 * PT)).collect();
    s.execute("path.create", &json!({"d": d})).unwrap();
    s.execute("embroidery.set", &json!({"type": "manual"})).unwrap();
    assert!(kinds(&mut s).contains(&"shortStitches".to_string()));
    // each rule can be turned off
    s.execute("embroidery.settings", &json!({"checks": {"shortStitch": 0, "narrowSatin": 0, "shortFill": 0}})).unwrap();
    let k = kinds(&mut s);
    assert!(!k.iter().any(|k| k == "shortStitches" || k == "narrowSatin" || k == "shortFill"), "{k:?}");
}

#[test]
fn fabric_recipes_set_underlay_types_push_compensation_and_clothing() {
    use sf_core::model::ObjectKind;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    s.execute("path.create", &json!({"d": format!("M {} {} L {} {}", 10.0 * PT, 10.0 * PT, 60.0 * PT, 10.0 * PT)})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    s.execute("embroidery.setParams", &json!({"params": {"width": 4}})).unwrap();
    rect(&mut s, 10.0 * PT, 30.0 * PT, 30.0 * PT, 30.0 * PT);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let planned = |s: &Session| {
        let plan = s.embroidery_plan().unwrap();
        let mut satin = None;
        let mut fill = None;
        for o in &plan.design.objects {
            match &o.kind {
                ObjectKind::Satin(x) => satin = Some(x.params.clone()),
                ObjectKind::Fill(x) => fill = Some(x.params.clone()),
                _ => {}
            }
        }
        (satin.unwrap(), fill.unwrap())
    };
    // a T-shirt: an edge run and a zigzag under satin, push compensation, clothing
    s.execute("embroidery.applyFabric", &json!({"name": "Jersey knit (T-shirt)"})).unwrap();
    let (sat, fill) = planned(&s);
    assert!(!sat.underlay.auto && sat.underlay.edge_run && sat.underlay.zigzag && !sat.underlay.center_run);
    assert!((sat.push_comp - 0.2).abs() < 1e-9 && (fill.push_comp - 0.2).abs() < 1e-9);
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!(info["settings"]["checks"]["garment"], true);
    // a towel isn't worn: long stitches from 12.1 mm
    s.execute("embroidery.applyFabric", &json!({"name": "Towel / terry"})).unwrap();
    let info = s.execute("embroidery.info", &json!({})).unwrap();
    assert_eq!((info["settings"]["checks"]["garment"].as_bool(), info["settings"]["checks"]["longStitch"].as_f64()), (Some(false), Some(12.1)));
    // leather: a centre run under satin, no edge run under fills (fewer holes)
    s.execute("embroidery.applyFabric", &json!({"name": "Leather / vinyl"})).unwrap();
    let (sat, fill) = planned(&s);
    assert!(sat.underlay.center_run && !sat.underlay.edge_run && !sat.underlay.zigzag);
    assert!(!fill.underlay.edge_run);
    // a fabric of one's own takes them too
    s.execute(
        "embroidery.fabric.save",
        &json!({"name": "Bag canvas", "pushComp": 0.3, "satinUnderlay": "zigzag", "fillEdgeRun": false, "garment": false}),
    )
    .unwrap();
    s.execute("embroidery.applyFabric", &json!({"name": "Bag canvas"})).unwrap();
    let (sat, fill) = planned(&s);
    assert!(sat.underlay.zigzag && !sat.underlay.edge_run && (sat.push_comp - 0.3).abs() < 1e-9 && !fill.underlay.edge_run);
    assert!(s.execute("embroidery.fabric.save", &json!({"name": "X", "satinUnderlay": "lots"})).is_err());
}

/// D3: a long rounded shape sewn as satin becomes a satin column between two open rails that
/// meet at its tips, with inclination lines, sewing about where it did.
#[test]
fn a_closed_shape_converts_to_a_satin_column_between_two_rails() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let id = NodeId(
        s.execute("shape.ellipse", &json!({"x": 20.0 * PT, "y": 40.0 * PT, "width": 60.0 * PT, "height": 8.0 * PT})).unwrap()["id"].as_u64().unwrap(),
    );
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let before = s.embroidery_plan().unwrap().pattern().bounds();
    let r = s.execute("embroidery.toColumn", &json!({})).unwrap();
    assert_eq!(r["ids"], json!([id.0]), "{r}");
    {
        let d = &s.doc().unwrap().doc;
        let n = d.node(id).unwrap();
        let NodeKind::Path { path, .. } = &n.kind else { panic!("a path") };
        assert_eq!(path.subpaths.len(), 2);
        assert!(path.subpaths.iter().all(|sp| !sp.closed));
        let e = n.embroidery.as_deref().unwrap();
        assert!(matches!(e.stitch, sf_core::attr::StitchType::Satin(_)));
        assert!(e.guides.len() >= 2);
        // both rails start at one tip and end at the other
        let ends = |i: usize| (path.subpaths[i].anchors.first().unwrap().p, path.subpaths[i].anchors.last().unwrap().p);
        let ((a0, a1), (b0, b1)) = (ends(0), ends(1));
        assert!(a0.distance(b0) < 0.5 && a1.distance(b1) < 0.5, "{a0:?} {b0:?} {a1:?} {b1:?}");
    }
    let after = s.embroidery_plan().unwrap().pattern().bounds();
    for (x, y) in [(before.min.x, after.min.x), (before.max.x, after.max.x), (before.min.y, after.min.y), (before.max.y, after.max.y)] {
        assert!((x - y).abs() < 1.5, "{before:?} → {after:?}");
    }
    // undo brings the shape back
    s.execute("edit.undo", &json!({})).unwrap();
    let d = &s.doc().unwrap().doc;
    let NodeKind::Path { path, .. } = &d.node(id).unwrap().kind else { panic!("a path") };
    assert!(path.subpaths.len() == 1 && path.subpaths[0].closed);
}

/// NEW-27: echo quilting is `count` runs stepping out `spacing` apart from the art; background
/// quilting a meander round it, the art left out, sewn first.
#[test]
fn echo_and_background_quilting_go_round_the_art() {
    use sf_core::attr::StitchType;
    use sf_core::model::FillKind;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let mut s = session();
    let a = rect(&mut s, 100.0, 100.0, 30.0, 30.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    let art = s.doc().unwrap().doc.node(a).unwrap().geometric_bounds().unwrap();
    let r = s.execute("embroidery.outlineAround", &json!({"kind": "echo", "gap": 2, "count": 4, "spacing": 3})).unwrap();
    let st = s.doc().unwrap();
    let n = st.doc.node(NodeId(r["id"].as_u64().unwrap())).unwrap();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Run(_))));
    let NodeKind::Path { path, .. } = &n.kind else { panic!("a path") };
    assert_eq!(path.subpaths.len(), 4);
    // the outermost 2 + 3 × 3 = 11 mm out
    let o = n.geometric_bounds().unwrap();
    assert!((art.x0 - o.x0 - 11.0 * PT).abs() < 0.8, "{o:?} {art:?}");
    assert!(s.execute("embroidery.outlineAround", &json!({"kind": "echo", "count": 0})).is_err());
    // stipple: a rectangle 5 mm beyond, the art left out, meander fill, first in the layer
    s.execute("select.set", &json!({"ids": [a.0]})).unwrap();
    let r = s.execute("embroidery.outlineAround", &json!({"kind": "stipple", "extend": 5})).unwrap();
    let k = NodeId(r["id"].as_u64().unwrap());
    let st = s.doc().unwrap();
    assert_eq!(st.doc.layers[0].children().unwrap()[0].id, k);
    let n = st.doc.node(k).unwrap();
    assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(StitchType::Fill(f)) if f.kind == FillKind::Meander));
    let NodeKind::Path { path, .. } = &n.kind else { panic!("a path") };
    assert_eq!(path.subpaths.len(), 2);
    assert!(stitches(&s) > 0);
}

/// NEW-01: Multi Blend as one object: the layers sew from the one fill, each in its thread and
/// BEY-05: a tartan fill lays the sett down the shape and across it, one fill per colour each
/// way, the warp first and the weft open on top; Undo puts the shape back.
#[test]
fn a_tartan_fill_weaves_the_sett_both_ways() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    let r = s.execute("embroidery.tartan", &json!({})).unwrap();
    assert_eq!(r["colors"].as_array().unwrap().len(), 4, "{r}");
    assert_eq!(r["objects"], 8, "{r}");
    let d = &s.doc().unwrap().doc;
    assert!(d.node(a).is_none());
    let g = d.node(vectorcraft_doc::NodeId(r["id"].as_u64().unwrap())).unwrap();
    let kids = g.children().unwrap();
    let names: Vec<String> = kids.iter().map(|k| k.embroidery.as_deref().unwrap().thread.clone().unwrap().name).collect();
    assert!(names[..4].iter().all(|n| n.starts_with("Tartan warp")) && names[4..].iter().all(|n| n.starts_with("Tartan weft")), "{names:?}");
    // the warp's rows run down, the weft's across
    let angle = |k: usize| match &kids[k].embroidery.as_deref().unwrap().stitch {
        sf_core::attr::StitchType::Fill(f) => f.angle,
        _ => f64::NAN,
    };
    assert_eq!((angle(0), angle(4)), (90.0, 0.0));
    assert!(stitches(&s) > 1000);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(a).is_some());
    assert!(s.execute("embroidery.tartan", &json!({"sett": [{"color": "#ff0000", "width": 99}]})).is_err());
}

/// BEY-05: cutwork splits a line by direction for the cutting needles, grouped by needle, and
/// Undo puts the line back.
#[test]
fn cutwork_splits_lines_by_direction_for_the_needles() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 100.0);
    let r = s.execute("embroidery.cutwork", &json!({})).unwrap();
    let needles = r["needles"].as_array().unwrap();
    // a box: its sides across for needle 1 (0°), up and down for needle 3 (90°)
    let got: Vec<u64> = needles.iter().map(|n| n["needle"].as_u64().unwrap()).collect();
    assert_eq!(got, vec![1, 3], "{r}");
    assert!(needles.iter().all(|n| n["lines"].as_u64().unwrap() >= 2), "{r}");
    let d = &s.doc().unwrap().doc;
    assert!(d.node(a).is_none(), "the line is replaced");
    let g = d.node(vectorcraft_doc::NodeId(r["id"].as_u64().unwrap())).unwrap();
    let kids = g.children().unwrap();
    // needle 1's lines first, then needle 3's, each named for its needle
    let names: Vec<String> = kids.iter().map(|k| k.embroidery.as_deref().unwrap().thread.clone().unwrap().name).collect();
    assert!(names.first().unwrap().starts_with("Cutwork needle 1") && names.last().unwrap().starts_with("Cutwork needle 3"), "{names:?}");
    assert!(stitches(&s) > 50);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(a).is_some(), "one undo step");
    assert!(s.execute("embroidery.cutwork", &json!({"needles": 5})).is_err());
}

/// BEY-05: a fill painted with a linear gradient blends its stops' colours the way the gradient
/// runs, each colour where its stop is, and the art keeps its gradient.
#[test]
fn a_linear_gradient_fill_blends_its_colours_along_the_gradient() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 120.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    // nothing to blend from a solid fill
    assert!(s.execute("embroidery.gradientBlend", &json!({})).is_err());
    let stops = json!([{"offset": 0, "color": "#ff0000"}, {"offset": 0.3, "color": "#ffff00"}, {"offset": 1, "color": "#0000ff"}]);
    s.execute("paint.setFill", &json!({"gradient": {"kind": "linear", "stops": stops}})).unwrap();
    let r = s.execute("embroidery.gradientBlend", &json!({})).unwrap();
    assert_eq!(r["ids"], json!([a.0]));
    let f = fill_params(&s, a);
    assert_eq!(f.blend.len(), 3);
    assert_eq!(f.blend[1].spacing_profile[1][0], 0.3_f32 as f64);
    assert!(matches!(s.doc().unwrap().doc.node(a).unwrap().appearance.fill_paint(), Paint::Gradient(_)), "the art keeps its gradient");
    // red sews on the left, yellow nearer it than the middle, blue on the right
    let plan = s.embroidery_plan().unwrap();
    let mean_x = |k: usize| {
        let own = &plan.built.spans[k].own;
        own.iter().map(|st| st.pos().x).sum::<f64>() / own.len() as f64
    };
    let (red, yellow, blue) = (mean_x(0), mean_x(1), mean_x(2));
    assert!(red < yellow && yellow < blue, "{red} {yellow} {blue}");
    // turned: a gradient running down blends top to bottom
    s.execute("paint.setFill", &json!({"gradient": {"kind": "linear", "stops": stops, "angle": -90}})).unwrap();
    s.execute("embroidery.gradientBlend", &json!({})).unwrap();
    let plan = s.embroidery_plan().unwrap();
    let mean_y = |k: usize| {
        let own = &plan.built.spans[k].own;
        own.iter().map(|st| st.pos().y).sum::<f64>() / own.len() as f64
    };
    assert!(mean_y(0) < mean_y(2), "{} {}", mean_y(0), mean_y(2));
}

/// with its own spacing, underlay under the first only; taking the blend out sews it plain again.
#[test]
fn a_blend_stays_one_object_and_sews_its_layers_in_their_threads() {
    let mut s = session();
    let a = rect(&mut s, 10.0, 10.0, 200.0, 150.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let plain = needles(&s, a).len() as f64;
    let r = s.execute("embroidery.makeBlend", &json!({"colors": ["#ff0000", "#ffff00", "#0000ff"]})).unwrap();
    assert_eq!(r["ids"], json!([a.0]));
    let d = &s.doc().unwrap().doc;
    assert_eq!(d.layers[0].children().unwrap().len(), 1, "no new objects");
    let f = fill_params(&s, a);
    assert_eq!(f.blend.len(), 3);
    assert_eq!(f.blend[1].spacing_profile, vec![[0.0, 0.0], [0.5, 0.4], [1.0, 0.0]]);
    assert_eq!(emb(&s, a).thread.unwrap().color, Rgb::new(255, 0, 0));
    // the plan: three objects from the one node, in the three threads, in order
    let plan = s.embroidery_plan().unwrap();
    let colors: Vec<Rgb> = plan.design.objects.iter().map(|o| plan.design.palette[o.color].color).collect();
    assert_eq!(colors, vec![Rgb::new(255, 0, 0), Rgb::new(255, 255, 0), Rgb::new(0, 0, 255)]);
    assert!(plan.nodes.iter().all(|n| *n == a));
    let total = needles(&s, a).len() as f64;
    assert!((0.7..1.4).contains(&(total / plain)), "{total} vs {plain}");
    // taking the blend out
    s.execute("embroidery.setParams", &json!({"params": {"blend": []}})).unwrap();
    assert!(fill_params(&s, a).blend.is_empty());
    assert_eq!(s.embroidery_plan().unwrap().design.objects.len(), 1);
    assert!(s.execute("embroidery.makeBlend", &json!({"colors": ["#ff0000", "#0000ff"], "separate": "yes"})).is_err());
}

/// Type made with the Type tools in an embroidery design sews at once (as satin); words follow a
/// line; type in an ordinary document is left unstitched.
#[test]
fn new_type_sews_in_a_design_and_words_follow_a_line() {
    let stitches = |s: &Session| s.embroidery_plan().map_or(0, |p| p.pattern().stitch_count());
    // an ordinary document: type is type
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s.execute("text.create", &json!({"x": 50, "y": 100, "text": "Plain", "size": 48})).unwrap();
    assert_eq!(stitches(&s), 0);
    // an embroidery design: the Type tool's type sews, and so does type on a path
    let mut s = Session::new();
    s.execute("embroidery.newDesign", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    let id = s.execute("text.create", &json!({"x": 50, "y": 100, "text": "Hello", "size": 48})).unwrap()["id"].as_u64().unwrap();
    assert!(stitches(&s) > 100, "typed words sew");
    let e = s.doc().unwrap().doc.node(NodeId(id)).unwrap().embroidery.clone().unwrap();
    assert!(matches!(e.stitch, sf_core::attr::StitchType::Satin(_)), "{e:?}");
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(id)).is_none(), "one undo step");
    // Add Words along an arc: type on the path, sewn, the words as tall as asked
    let arc = s.execute("shape.arc", &json!({"x1": 60, "y1": 220, "x2": 310, "y2": 100})).unwrap()["id"].as_u64().unwrap();
    let before = stitches(&s);
    let r = s.execute("embroidery.addWords", &json!({"text": "Happy Birthday", "height": 10, "path": arc})).unwrap();
    let words = r["id"].as_u64().unwrap();
    let n = s.doc().unwrap().doc.node(NodeId(words)).unwrap().clone();
    assert!(matches!(&n.kind, NodeKind::Text(t) if matches!(t.kind, vectorcraft_doc::TextKind::OnPath { .. })), "type on the path");
    assert!(stitches(&s) > before + 100, "{r}");
    // too long for the line at that height: made smaller, all on the line
    let arc = s.execute("shape.arc", &json!({"x1": 60, "y1": 420, "x2": 310, "y2": 300})).unwrap()["id"].as_u64().unwrap();
    let long = s.execute("embroidery.addWords", &json!({"text": "Happy Birthday to You", "height": 30, "path": arc})).unwrap();
    let id = vectorcraft_doc::NodeId(long["id"].as_u64().unwrap());
    assert!(crate::cmd::typecmd::with_overflow(&s, &[id], json!({})).get("overflow").is_none(), "{long}");
    let size = match &s.doc().unwrap().doc.node(id).unwrap().kind {
        NodeKind::Text(t) => t.first_style().size,
        _ => 0.0,
    };
    assert!(size > 0.0 && size < 30.0 * vectorcraft_embroidery::PT_PER_MM / 0.7 * 0.95, "made smaller: {size} pt");
    // not a line: said so
    let t = s.execute("text.create", &json!({"x": 10, "y": 10, "text": "x"})).unwrap()["id"].as_u64().unwrap();
    assert!(s.execute("embroidery.addWords", &json!({"text": "No", "path": t})).is_err());
}

/// Choosing a hoop makes the page its size, about the page's middle; one undo step.
#[test]
fn the_page_follows_the_hoop() {
    let mut s = Session::new();
    s.execute("embroidery.newDesign", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    let page = |s: &Session| s.doc().unwrap().doc.artboards[0].rect;
    let before = page(&s);
    s.execute("embroidery.settings", &json!({"hoop": {"width": 100, "height": 100}})).unwrap();
    let after = page(&s);
    assert!(
        (after.width() - 100.0 * vectorcraft_embroidery::PT_PER_MM).abs() < 1e-6
            && (after.height() - 100.0 * vectorcraft_embroidery::PT_PER_MM).abs() < 1e-6,
        "{after:?}"
    );
    assert!((after.center() - before.center()).hypot() < 1e-6, "same middle");
    // other settings leave the page alone
    s.execute("embroidery.settings", &json!({"fabric": "Denim / canvas"})).unwrap();
    assert_eq!(page(&s), after);
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(page(&s), before, "undone with the hoop");
}

/// Use Line as Direction: a line drawn across a stitched shape becomes its direction line and
/// goes; a line crossing nothing is refused; one undo step.
#[test]
fn a_line_across_a_shape_sets_its_stitch_direction() {
    let mut s = Session::new();
    s.execute("embroidery.newDesign", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    let shape = s.execute("shape.ellipse", &json!({"x": 60, "y": 100, "width": 240, "height": 240})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [shape]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let line = s.execute("shape.line", &json!({"x1": 180, "y1": 120, "x2": 180, "y2": 320})).unwrap()["id"].as_u64().unwrap();
    let far = s.execute("shape.line", &json!({"x1": 500, "y1": 500, "x2": 520, "y2": 560})).unwrap()["id"].as_u64().unwrap();
    // only a line far away: refused
    s.execute("select.set", &json!({"ids": [shape, far]})).unwrap();
    assert!(s.execute("embroidery.lineAsDirection", &json!({})).is_err());
    s.execute("select.set", &json!({"ids": [shape, line]})).unwrap();
    let r = s.execute("embroidery.lineAsDirection", &json!({})).unwrap();
    assert_eq!((r["ids"].clone(), r["lines"].as_u64()), (json!([shape]), Some(1)), "{r}");
    let d = &s.doc().unwrap().doc;
    let e = d.node(NodeId(shape)).unwrap().embroidery.clone().unwrap();
    assert_eq!(e.guides.len(), 1);
    let g = &e.guides[0];
    assert!((g.a.x - g.b.x).abs() < 0.5 && (g.a.y - g.b.y).abs() > 100.0, "upright: {g:?}");
    assert!(d.node(NodeId(line)).is_none(), "the line is the direction now");
    // the rows run upright: stitches mostly move up and down
    let pts: Vec<sf_core::Point> =
        s.embroidery_plan().unwrap().pattern().stitches.iter().filter(|st| st.cmd == sf_core::Cmd::Stitch).map(|st| st.pos()).collect();
    let (dx, dy) = pts.windows(2).fold((0.0, 0.0), |(x, y), w| (x + (w[1].x - w[0].x).abs(), y + (w[1].y - w[0].y).abs()));
    assert!(dy > dx * 3.0, "rows up and down: {dx} across, {dy} up and down");
    s.execute("edit.undo", &json!({})).unwrap();
    let d = &s.doc().unwrap().doc;
    assert!(d.node(NodeId(line)).is_some() && d.node(NodeId(shape)).unwrap().embroidery.as_ref().unwrap().guides.is_empty(), "one step");
}

/// Two touching shapes of one thread: the thread runs hidden under the second instead of a trim
/// (hideTravel, on by default); apart, or with hideTravel off, a trim as before.
#[test]
fn touching_shapes_of_one_thread_need_no_trim() {
    let trims = |s: &mut Session| s.execute("embroidery.info", &json!({})).unwrap()["trims"].as_u64().unwrap();
    let mut s = Session::new();
    s.execute("embroidery.newDesign", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    let a = s.execute("shape.rectangle", &json!({"x": 60, "y": 100, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
    let b = s.execute("shape.rectangle", &json!({"x": 160, "y": 100, "width": 100, "height": 100})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [a, b]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    s.execute("embroidery.setThread", &json!({"color": "#1f3a93"})).unwrap();
    // their sewn order puts the end of the first well away from the start of the second
    let hidden = trims(&mut s);
    s.execute("embroidery.settings", &json!({"hideTravel": false})).unwrap();
    let cut = trims(&mut s);
    assert!(hidden < cut, "hidden travel {hidden} trims, cut {cut}");
    assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["settings"]["hideTravel"], false);
    // apart: a trim either way
    s.execute("embroidery.settings", &json!({"hideTravel": true})).unwrap();
    s.execute("select.set", &json!({"ids": [b]})).unwrap();
    s.execute("object.move", &json!({"dx": 60, "dy": 0})).unwrap();
    assert_eq!(trims(&mut s), cut, "apart");
}

/// Add Words in an arc up, an arc down or a circle: the words on a curve made for them, sewn,
/// all of them on it.
#[test]
fn words_go_in_an_arc_or_a_circle() {
    for shape in ["arcUp", "arcDown", "circle"] {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
        let r = s.execute("embroidery.addWords", &json!({"text": "Grandma's Kitchen", "height": 12, "shape": shape})).unwrap();
        let id = NodeId(r["id"].as_u64().unwrap());
        let n = s.doc().unwrap().doc.node(id).unwrap().clone();
        assert!(matches!(&n.kind, NodeKind::Text(t) if matches!(t.kind, vectorcraft_doc::TextKind::OnPath { .. })), "{shape}");
        assert!(crate::cmd::typecmd::with_overflow(&s, &[id], json!({})).get("overflow").is_none(), "{shape}: all on the curve");
        assert!(s.embroidery_plan().unwrap().pattern().stitch_count() > 300, "{shape}");
    }
    let mut s = Session::new();
    s.execute("embroidery.newDesign", &json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    assert!(s.execute("embroidery.addWords", &json!({"text": "x", "shape": "zigzag"})).is_err());
}

/// DIR-02: Stitch Type › Satin places a shape's lines and the type in one undo step; a satin
/// line (a stroke) gets none (it turns along its path anyway).
#[test]
fn a_satin_gets_its_lines_in_one_step() {
    let mut s = Session::new();
    s.execute("embroidery.newDesign", &json!({"hoop": {"width": 180, "height": 130}})).unwrap();
    let id = s.execute("path.create", &json!({"d": "M 100 300 C 120 150, 300 100, 420 200 C 330 160, 180 190, 140 300 Z"})).unwrap()["id"]
        .as_u64()
        .unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    let e = s.doc().unwrap().doc.node(NodeId(id)).unwrap().embroidery.clone().unwrap();
    assert!(e.guides.len() >= 3, "{}", e.guides.len());
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(id)).unwrap().embroidery.is_none(), "one undo step");
    let line = s.execute("shape.line", &json!({"x1": 50, "y1": 50, "x2": 300, "y2": 80})).unwrap()["id"].as_u64().unwrap();
    s.execute("select.set", &json!({"ids": [line]})).unwrap();
    s.execute("embroidery.set", &json!({"type": "satin"})).unwrap();
    assert!(s.doc().unwrap().doc.node(NodeId(line)).unwrap().embroidery.as_ref().unwrap().guides.is_empty());
}

/// OP-06: a fabric's own fill stitch length (3.5 mm on a T-shirt) isn't flagged as too short.
#[test]
fn a_fabrics_fill_length_isnt_flagged_as_short() {
    let mut s = session();
    rect(&mut s, 100.0, 100.0, 200.0, 120.0);
    s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
    let short = |s: &mut Session| {
        let c = s.execute("embroidery.check", &json!({})).unwrap();
        c["findings"].as_array().unwrap().iter().filter(|f| f["kind"] == "shortFill").count()
    };
    s.execute("embroidery.applyFabric", &json!({"name": "Jersey knit (T-shirt)"})).unwrap();
    assert_eq!(short(&mut s), 0);
    // shorter than the fabric's own length still is
    s.execute("embroidery.setParams", &json!({"params": {"stitchLength": 2.5}})).unwrap();
    assert_eq!(short(&mut s), 1);
}

#[test]
fn patches_with_names_fill_hoops_sewn_colour_by_colour() {
    let mut s = session();
    let names: Vec<String> = (1..=10).map(|k| format!("Worker {k}")).collect();
    let q = json!({"shape": "circle", "width": 60, "names": names, "textColor": "#e63946", "hoop": "Brother 200 × 200 (8×8)"});
    // 60 mm circles 5 mm apart in a 200 mm field: 3 × 3 a hoopful, so two hoopfuls for ten names
    let r = s.execute("embroidery.patches.make", &q).unwrap();
    assert_eq!((r["perHoop"].as_u64(), r["hoopfuls"].as_u64(), r["made"].as_u64()), (Some(9), Some(2), Some(9)), "{r}");
    // every background, then every name, then every edge: three thread changes for the hoopful
    let plan = s.embroidery_plan().unwrap();
    assert_eq!(plan.pattern().color_blocks().len(), 3);
    let st = plan.stats.clone().unwrap();
    assert!(st.width_mm <= 195.0 && st.height_mm <= 195.0, "inside the hoop: {st:?}");
    // the second hoopful holds the tenth name
    let mut q2 = q.clone();
    q2["hoopful"] = json!(2);
    let r = s.execute("embroidery.patches.make", &q2).unwrap();
    assert_eq!(r["made"].as_u64(), Some(1));
    q2["hoopful"] = json!(3);
    assert!(s.execute("embroidery.patches.make", &q2).is_err());
    // a hoopful each as a machine file, and which names are in which
    let dir = std::env::temp_dir().join(format!("sf-patches-{}", std::process::id()));
    let mut qe = q.clone();
    qe["dir"] = json!(dir.to_string_lossy());
    qe["format"] = json!("dst");
    let docs = s.documents().len();
    let r = s.execute("embroidery.patches.export", &qe).unwrap();
    assert_eq!(r["files"].as_array().map(Vec::len), Some(2), "{r}");
    assert!(std::fs::read(dir.join("patches-1.dst")).unwrap().len() > 512);
    let list = std::fs::read_to_string(dir.join("patches-names.txt")).unwrap();
    assert!(list.contains("patches-1.dst: Worker 1,") && list.contains("patches-2.dst: Worker 10"), "{list}");
    assert_eq!(s.documents().len(), docs, "the hoopfuls' documents are closed again");
    std::fs::remove_dir_all(&dir).ok();
    // bad requests
    assert!(s.execute("embroidery.patches.make", &json!({"shape": "star"})).is_err());
    assert!(s.execute("embroidery.patches.make", &json!({"width": 400})).is_err());
    assert!(s.execute("embroidery.patches.make", &json!({"border": 1.0})).is_err());
    assert!(s.execute("embroidery.patches.make", &json!({"width": 250, "hoop": "Brother 100 × 100 (4×4)"})).is_err());
    assert!(s.execute("embroidery.patches.export", &json!({"names": ["A"]})).is_err());
}

#[test]
fn a_patch_copied_to_fill_its_hoop() {
    let mut s = session();
    // one name tape with words, in a 200 mm hoop: as many as fit, each colour sewn once
    let r = s.execute("embroidery.patches.make", &json!({"shape": "rounded", "width": 90, "height": 30, "text": "STAFF", "textColor": "#e63946", "count": 1, "hoop": "Brother 200 × 200 (8×8)"})).unwrap();
    assert_eq!(r["made"].as_u64(), Some(1));
    let r = s.execute("embroidery.patches.fillHoop", &json!({})).unwrap();
    // 90 × 30 tapes 5 mm apart in 190 mm: 2 across, 5 down
    assert_eq!((r["columns"].as_u64(), r["rows"].as_u64(), r["copies"].as_u64()), (Some(2), Some(5), Some(10)), "{r}");
    let plan = s.embroidery_plan().unwrap();
    assert_eq!(plan.pattern().color_blocks().len(), 3);
    let st = plan.stats.clone().unwrap();
    assert!(st.width_mm <= 195.0 && st.height_mm <= 195.0, "inside the hoop: {st:?}");
    // one undo step takes the copies away again
    s.execute("edit.undo", &json!({})).unwrap();
    let st = s.embroidery_plan().unwrap().stats.clone().unwrap();
    assert!(st.width_mm < 100.0, "{st:?}");
}

/// FONT-01: the fonts built into the app (a build with STITCHFOUNDRY_FONTS_DIR set) are listed
/// without loading anything, built in, with their licence's text, and set lettering. Skipped when
/// the build has none.
#[test]
fn built_in_fonts_are_there_without_loading_and_carry_their_licence() {
    let bundled = vectorcraft_embroidery::fonts::BUNDLED_FONTS;
    if bundled.is_empty() {
        eprintln!("skipped: built without the StitchFoundry fonts (set STITCHFOUNDRY_FONTS_DIR to a stitchfoundry-fonts checkout)");
        return;
    }
    let mut s = session();
    let r = s.execute("embroidery.fonts", &json!({})).unwrap();
    let built: Vec<&Value> = r["fonts"].as_array().unwrap().iter().filter(|f| f["builtIn"] == true).collect();
    assert_eq!(built.len(), bundled.len(), "{r}");
    assert!(r["warnings"].as_array().unwrap().is_empty(), "{r}");
    for f in &built {
        assert!(f["licenceText"].as_str().unwrap().contains("Open Font License"), "{f}");
        assert!(f["glyphs"].as_u64().unwrap() > 50, "{f}");
    }
    s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
    let name = built[0]["name"].as_str().unwrap();
    let l = s.execute("embroidery.lettering", &json!({"text": "Hello", "font": name, "size": 20})).unwrap();
    assert!(l["glyphs"].as_u64().unwrap() >= 5, "{l}");
    assert!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap() > 200);
}

/// Warps and distortions (Envelope Distort, and the Warp and Distort & Transform effects) carry
/// through to the stitches: the needle points cover the art as it shows, and keep within it.
#[test]
fn warped_and_distorted_art_is_sewn_as_it_looks() {
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let bbox = |pts: &[sf_core::Point]| {
        pts.iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(p.x * PT), b[1].min(p.y * PT), b[2].max(p.x * PT), b[3].max(p.y * PT)])
    };
    // the stitches reach each side of the art as it shows, within 1.5 mm, and no further
    let fits = |b: [f64; 4], art: vectorcraft_geom::Rect| {
        let t = 1.5 * PT;
        (b[0] - art.x0).abs() < t && (b[1] - art.y0).abs() < t && (b[2] - art.x1).abs() < t && (b[3] - art.y1).abs() < t
    };
    let start = |s: &mut Session| {
        let a = rect(s, 200.0, 200.0, 160.0, 80.0);
        s.execute("paint.setStroke", &json!({"none": true})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        a
    };
    let plain = {
        let mut s = session();
        let a = start(&mut s);
        bbox(&needles(&s, a))
    };
    for style in
        ["arc", "arcLower", "arcUpper", "arch", "bulge", "shellLower", "shellUpper", "flag", "wave", "fish", "rise", "inflate", "squeeze", "twist"]
    {
        let mut s = session();
        let a = start(&mut s);
        let env =
            NodeId(s.execute("object.envelope.makeWithWarp", &json!({"ids": [a.0], "style": style, "bend": 50})).unwrap()["id"].as_u64().unwrap());
        let art = s.doc().unwrap().doc.bounds_of(&[env], false).unwrap();
        let b = bbox(&needles(&s, a));
        assert!(fits(b, art) && b != plain, "envelope {style}: stitches {b:?}, art {art:?}");
    }
    for (effect, params) in [
        ("warp.arc", json!({})),
        ("warp.wave", json!({})),
        ("warp.twist", json!({})),
        ("distort.puckerBloat", json!({"amount": 50})),
        ("distort.roughen", json!({})),
        ("distort.twist", json!({})),
        ("distort.zigZag", json!({})),
        ("distort.transform", json!({"scaleH": 150})),
    ] {
        let mut s = session();
        let a = start(&mut s);
        s.execute("effect.apply", &json!({"effect": effect, "params": params, "ids": [a.0]})).unwrap();
        // the art as it shows: its path through its effects
        let n = s.doc().unwrap().doc.node(a).unwrap().clone();
        let NodeKind::Path { path, .. } = &n.kind else { panic!() };
        let shown = vectorcraft_render::effects::apply_geometry(&n.appearance.effects, path, n.geometric_bounds().unwrap()).bounds().unwrap();
        let b = bbox(&needles(&s, a));
        assert!(fits(b, shown) && b != plain, "{effect}: stitches {b:?}, art {shown:?}");
        // taken off again, it sews as before
        s.execute("effect.remove", &json!({"ids": [a.0], "index": 0})).unwrap();
        assert_eq!(bbox(&needles(&s, a)), plain, "{effect}");
    }
}
