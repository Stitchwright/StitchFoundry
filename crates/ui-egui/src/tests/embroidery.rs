//! The embroidery interface: Check Design and its findings.

use serde_json::json;
use vectorcraft_engine::Session;

use crate::VectorcraftApp;

fn app() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
    app
}

#[test]
fn check_design_lists_findings_in_the_panel() {
    let mut app = app();
    assert_eq!(app.run("ui.embroideryCheck", json!({})).unwrap()["count"], 0);
    assert_eq!(app.embroidery.checks.as_ref().map(Vec::len), Some(0));
    for _ in 0..3 {
        app.run("shape.rectangle", json!({"x": 50, "y": 50, "width": 60, "height": 60})).unwrap();
        app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    }
    let n = app.run("ui.embroideryCheck", json!({})).unwrap()["count"].as_u64().unwrap();
    assert!(n >= 1);
    let findings = app.embroidery.checks.clone().unwrap();
    assert!(findings.iter().any(|f| f["kind"] == "dense"));
    for f in &findings {
        assert!(!crate::embroidery::panel::finding_text_for_test(f).is_empty(), "{f}");
    }
}

/// Every piece of text one headless frame of `show` paints.
fn painted_text(app: &mut VectorcraftApp, show: fn(&mut VectorcraftApp, &mut egui::Ui)) -> Vec<String> {
    painted_text_in(app, show, 600.0)
}

/// [`painted_text`] on a screen `height` points high.
fn painted_text_in(app: &mut VectorcraftApp, show: fn(&mut VectorcraftApp, &mut egui::Ui), height: f32) -> Vec<String> {
    let ctx = egui::Context::default();
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, height))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| show(app, ui));
    out.textures_delta.clear();
    fn walk(s: &egui::Shape, into: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => into.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, into)),
            _ => {}
        }
    }
    let mut texts = Vec::new();
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
    texts
}

#[test]
fn the_panel_shows_a_fills_texture_and_split_lines() {
    let mut app = app();
    let id = app.run("shape.rectangle", json!({"x": 20, "y": 20, "width": 200, "height": 150})).unwrap()["id"].as_u64().unwrap();
    app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Texture") && !texts.iter().any(|t| t == "Texture size"), "{texts:?}");
    app.run("embroidery.setParams", json!({"params": {"texture": {"pattern": "scales"}}})).unwrap();
    app.run("embroidery.setSplits", json!({"id": id, "splits": [[[50, 0], [60, 200]]]})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Scales", "Texture size", "Texture angle", "Split lines: 1", "Clear Split Lines"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // Rows following flow lines, with a button to straighten them.
    app.run("embroidery.setFlowLines", json!({"id": id, "lines": [[[10, 30], [230, 10]], [[10, 190], [230, 160]]]})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Rows follow lines: 2", "Clear Flow Lines"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // Satin fills have neither.
    app.run("embroidery.set", json!({"type": "satinfill"})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(!texts.iter().any(|t| t == "Texture" || t == "Split lines: 1"), "{texts:?}");
}

#[test]
fn the_panel_offers_the_designs_own_motifs() {
    let mut app = app();
    let peak = app.run("path.create", json!({"d": "M 20 200 L 50 150 L 80 200"})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [peak]})).unwrap();
    app.run("embroidery.motif.make", json!({"name": "Peak"})).unwrap();
    let line = app.run("path.create", json!({"d": "M 20 100 L 300 100"})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [line]})).unwrap();
    app.run("embroidery.set", json!({"type": "motif"})).unwrap();
    // A built-in motif shows, and no Delete Motif.
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Wave") && !texts.iter().any(|t| t == "Delete Motif"), "{texts:?}");
    // Sewing the design's motif: its name shows, with Delete Motif.
    app.run("embroidery.motif.apply", json!({"name": "Peak"})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Peak") && texts.iter().any(|t| t == "Delete Motif"), "{texts:?}");
    // Deleted from the design, the run still shows the motif it sews, without the button.
    app.run("embroidery.motif.delete", json!({"name": "Peak"})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Peak") && !texts.iter().any(|t| t == "Delete Motif"), "{texts:?}");
}

#[test]
fn the_panel_shows_sculpture_runs_and_cross_styles() {
    let mut app = app();
    let line = app.run("path.create", json!({"d": "M 20 100 L 300 100"})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [line]})).unwrap();
    app.run("embroidery.set", json!({"type": "run"})).unwrap();
    app.run("embroidery.setParams", json!({"params": {"kind": "Sculpture"}})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Sculpture run", "Triple stitch length", "Single stitch length", "Triple stitches", "Single stitches"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // a string stitch: its long stitches' chord length and spacing
    app.run("embroidery.setParams", json!({"params": {"kind": "String"}})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["String stitch", "Chord length", "Spacing"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // a satin line sewn as motifs
    app.run("embroidery.set", json!({"type": "satin"})).unwrap();
    app.run("embroidery.setParams", json!({"params": {"motifColumn": {"motif": "Leaf"}}})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Sew as", "Motifs", "Leaf", "Motif length (× width)"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    app.run("shape.rectangle", json!({"x": 20, "y": 150, "width": 100, "height": 100})).unwrap();
    // a 3D motif fill receding: its strength and direction
    app.run("embroidery.set", json!({"type": "motiffill"})).unwrap();
    app.run("embroidery.setParams", json!({"params": {"warp": {"kind": "Perspective"}}})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["3D effect", "Perspective", "Strength", "Direction"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    app.run("embroidery.set", json!({"type": "cross"})).unwrap();
    app.run("embroidery.setParams", json!({"params": {"cross": "Upright"}})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Cross style") && texts.iter().any(|t| t == "Upright cross"), "{texts:?}");
}

#[test]
fn the_design_can_be_shown_repeated() {
    let mut app = app();
    app.run("shape.rectangle", json!({"x": 20, "y": 20, "width": 100, "height": 60})).unwrap();
    app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    let r = app.run("ui.embroideryRepeats", json!({"columns": 3, "rows": 2, "gap": 4})).unwrap();
    assert_eq!(r, json!({"columns": 3, "rows": 2, "gap": 4.0}));
    assert_eq!((app.embroidery.repeats, app.embroidery.repeat_gap), ([3, 2], 4.0));
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Repeats across", "Repeats down", "Gap between repeats"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // out of range: clamped; junk: left as it was
    let r = app.run("ui.embroideryRepeats", json!({"columns": 99, "rows": -3, "gap": "x"})).unwrap();
    assert_eq!(r, json!({"columns": 10, "rows": 1, "gap": 4.0}));
}

#[test]
fn the_panel_picks_the_motif_tools_motif() {
    let mut app = app();
    // Send to Machine: a folder to add first, then the folder and Send
    let texts = painted_text_in(&mut app, crate::embroidery::panel::send_body, 600.0);
    assert!(texts.iter().any(|t| t.starts_with("Add the folder your machine reads")), "{texts:?}");
    app.session.prefs.embroidery_send_dirs = vec!["/media/usb".into()];
    let texts = painted_text_in(&mut app, crate::embroidery::panel::send_body, 600.0);
    for want in ["Machine folder", "/media/usb", "Send", "Forget Folder"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    app.session.prefs.embroidery_send_dirs.clear();
    // the Click-to-Stitch tool's settings
    app.run("tool.select", json!({"tool": "embroideryClick"})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Stitch as", "Fill", "Tolerance"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    app.run("tool.select", json!({"tool": "embroideryMotif"})).unwrap();
    app.run("tool.setOption", json!({"tool": "embroideryMotif", "values": {"motif": "Star"}})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Motif") && texts.iter().any(|t| t == "Star"), "{texts:?}");
}

#[test]
fn buttonholes_are_added_in_a_row_from_the_panel() {
    use crate::embroidery::Buttonholes;
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    // three keyholes 20 mm apart, centred on (300, 200), running down the page
    let b = Buttonholes { style: 2, count: 3, spacing: 20.0, ..Buttonholes::default() };
    let p = b.params(vectorcraft_geom::Point::new(300.0, 200.0));
    assert_eq!((p["style"].as_str(), p["count"].as_u64()), (Some("keyhole"), Some(3)));
    let (at, to) = (&p["at"], &p["to"]);
    assert!((at[1].as_f64().unwrap() - (200.0 - 20.0 * PT)).abs() < 1e-9 && (to[1].as_f64().unwrap() - (200.0 + 20.0 * PT)).abs() < 1e-9, "{p}");
    assert!((at[0].as_f64().unwrap() - 300.0).abs() < 1e-9);
    // turned a quarter: the row runs across instead
    let p = Buttonholes { angle: 90.0, count: 2, ..Buttonholes::default() }.params(vectorcraft_geom::Point::new(0.0, 0.0));
    assert!((p["at"][0].as_f64().unwrap() - 12.5 * PT).abs() < 1e-9, "{p}");
    // they place
    let mut app = app();
    app.run("embroidery.addButtonholes", p).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Buttonholes"), "{texts:?}");
}

#[test]
fn the_panel_sets_lettering_in_a_loaded_font() {
    let mut app = app();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    assert!(texts.iter().any(|t| t == "Embroidery Lettering"), "{texts:?}");
    let texts = painted_text_in(&mut app, crate::embroidery::panel::lettering_body, 600.0);
    assert!(texts.iter().any(|t| t == "Load an embroidery font to set text with it."), "{texts:?}");
    let json = r#"{"name": "Panel Font", "size": 10, "min_scale": 0.5, "max_scale": 2, "leading": 60, "horiz_adv_x": {"A": 40}}"#;
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="100mm" height="100mm" viewBox="0 0 100 100"><g inkscape:groupmode="layer" inkscape:label="GlyphLayer-A" id="l"><path d="M2 100 L20 80" fill="none" stroke="#000" stroke-dasharray="1 1"/></g></svg>"##;
    app.run("embroidery.font.load", json!({"json": json, "svg": svg})).unwrap();
    // the font, the sizes it sews well at and the button to place the text
    let texts = painted_text_in(&mut app, crate::embroidery::panel::lettering_body, 600.0);
    for want in ["Panel Font", "Forget Font", "Sews well at 5.0–20.0 mm", "Place Lettering", "Baseline", "Straight"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // how it is sewn: its order, joins and trims
    for want in ["Sew order", "Left to right", "Join", "Closest", "Trims", "Where needed", "3D puff"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    assert!(!texts.iter().any(|t| t == "Move along"), "{texts:?}");
    let r = app.run("embroidery.lettering", json!({"text": "AA", "font": "Panel Font", "size": 10})).unwrap();
    assert_eq!(r["glyphs"].as_u64(), Some(2));
    // the new lettering is selected: the section shows what it says, to change it
    let texts = painted_text_in(&mut app, crate::embroidery::panel::lettering_body, 900.0);
    assert!(texts.iter().any(|t| t == "Update Lettering") && !texts.iter().any(|t| t == "Place Lettering"), "{texts:?}");
    // one letter of it to move, scale or turn
    for want in ["Letter", "Move along", "Move down", "Scale", "Turn"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    assert_eq!((app.embroidery.lettering_text.as_str(), app.embroidery.lettering_size), ("AA", 10.0));
    // a size typed outside what the font sews well at: warned of at once
    app.embroidery.lettering_size = 30.0;
    let texts = painted_text_in(&mut app, crate::embroidery::panel::lettering_body, 600.0);
    assert!(texts.iter().any(|t| t == "▲ Sews well at 5.0–20.0 mm"), "{texts:?}");
    app.embroidery.lettering_size = 10.0;
    // a monogram in the font
    let texts = painted_text_in(&mut app, crate::embroidery::panel::monogram_body, 600.0);
    for want in ["Font: Panel Font", "Classic", "Circle", "Place Monogram"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // team names: for the selected lettering
    app.embroidery.team_names = "Lee\n\nSmith\n".into();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::team_names_body, 600.0);
    assert!(texts.iter().any(|t| t == "Names: 2"), "{texts:?}");
    // nothing selected: new lettering again
    app.run("select.none", json!({})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::team_names_body, 600.0);
    assert!(texts.iter().any(|t| t.starts_with("Select lettering set in an embroidery font")), "{texts:?}");
    let texts = painted_text_in(&mut app, crate::embroidery::panel::lettering_body, 600.0);
    assert!(texts.iter().any(|t| t == "Place Lettering"), "{texts:?}");
    assert_eq!(app.embroidery.lettering_for, None);
}

#[test]
fn the_font_creator_section_makes_and_edits_fonts() {
    let mut app = app();
    // no font document: ways to start one
    let texts = painted_text_in(&mut app, crate::embroidery::panel::font_body, 900.0);
    for want in ["New Embroidery Font", "New Font from Type"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // a font document: its properties, its glyphs (still to digitize) and kerning
    app.run("embroidery.fontDoc.new", json!({"name": "Hand", "chars": "AB", "size": 20})).unwrap();
    app.run("embroidery.fontDoc.kern", json!({"pair": "A B", "value": 2})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::font_body, 900.0);
    for want in ["Capitals' height: 20.0 mm", "Glyphs: 2 · Still to digitize: 2", "A", "B", "A B", "Try in a New Document"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
}

#[test]
fn the_panel_edits_a_redwork_group_as_one() {
    let mut app = app();
    let a = app.run("path.create", json!({"d": "M 20 20 L 120 20 L 120 120 Z"})).unwrap()["id"].as_u64().unwrap();
    let b = app.run("path.create", json!({"d": "M 70 20 L 70 120"})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [a, b]})).unwrap();
    app.run("embroidery.redwork", json!({})).unwrap();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::show, 2400.0);
    for want in ["Redwork", "Run style", "Stitch length", "Sewn as one line that ends where it starts; the way between lines is retraced along them."]
    {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
}

#[test]
fn stitch_list_shows_every_record() {
    let mut app = app();
    assert!(painted_text(&mut app, crate::embroidery::stitch_list::show).iter().any(|t| t.contains("Nothing stitched yet")));
    app.run("shape.rectangle", json!({"x": 50, "y": 50, "width": 60, "height": 40})).unwrap();
    app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    let texts = painted_text(&mut app, crate::embroidery::stitch_list::show);
    for want in ["Function", "Stitch", "0", "1"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    // the first record is the jump to the start, then stitches with their lengths
    let plan = app.session.embroidery_plan().unwrap();
    let first = plan.pattern().stitches[0];
    assert!(texts.iter().any(|t| *t == format!("{:.2}", first.x)), "{texts:?}");
}

#[test]
fn styles_panel_lists_and_applies_styles() {
    let mut app = app();
    assert!(painted_text(&mut app, crate::embroidery::styles::show).iter().any(|t| t.starts_with("No styles yet")));
    app.run("shape.rectangle", json!({"x": 20, "y": 20, "width": 80, "height": 40})).unwrap();
    app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    app.run("embroidery.style.new", json!({"name": "Dense"})).unwrap();
    let texts = painted_text(&mut app, crate::embroidery::styles::show);
    assert!(texts.iter().any(|t| t == "Dense") && texts.iter().any(|t| t == "Tatami Fill · 1"), "{texts:?}");
    // an object changed since it took its style is marked with a +, here and in the Embroidery panel
    app.run("embroidery.setParams", json!({"params": {"spacing": 0.6}})).unwrap();
    assert!(painted_text(&mut app, crate::embroidery::styles::show).iter().any(|t| t == "Dense+"));
    assert!(painted_text(&mut app, crate::embroidery::panel::show).iter().any(|t| t == "Dense+"));
    // with nothing selected, the Embroidery panel shows the properties new objects take
    app.run("select.none", json!({})).unwrap();
    let texts = painted_text(&mut app, crate::embroidery::panel::show);
    assert!(texts.iter().any(|t| t == "Properties for New Objects") && texts.iter().any(|t| t == "Density"), "{texts:?}");
}

#[test]
fn the_player_travels_by_object_colour_trim_and_stitch() {
    use crate::embroidery::{TravelBy, object_at, travel};
    use vectorcraft_embroidery::sf_core::Cmd;
    let mut app = app();
    let mut ids = Vec::new();
    for (x, color) in [(20.0, "#cc2233"), (150.0, "#2233cc"), (280.0, "#cc2233")] {
        let r = app.run("shape.rectangle", json!({"x": x, "y": 40, "width": 80, "height": 50})).unwrap();
        ids.push(vectorcraft_doc::NodeId(r["id"].as_u64().unwrap()));
        app.run("embroidery.set", json!({"type": "fill"})).unwrap();
        app.run("embroidery.setThread", json!({"color": color})).unwrap();
    }
    let plan = app.session.embroidery_plan().unwrap();
    let st = &plan.pattern().stitches;
    let n = st.len();
    // by object: the end of each object in turn, and back
    let first = travel(&plan, 0, TravelBy::Object, true);
    assert_eq!(first, plan.records_of(ids[0]).last().unwrap().end);
    assert_eq!(object_at(&plan, first), Some(ids[0]));
    let second = travel(&plan, first, TravelBy::Object, true);
    assert_eq!(object_at(&plan, second), Some(ids[1]));
    assert_eq!(travel(&plan, second, TravelBy::Object, false), first);
    assert_eq!(travel(&plan, n, TravelBy::Object, true), n);
    // by colour and by trim: just after each colour change or trim
    let c = travel(&plan, 0, TravelBy::Color, true);
    assert_eq!(st[c - 1].cmd, Cmd::ColorChange);
    let t = travel(&plan, 0, TravelBy::Trim, true);
    assert_eq!(st[t - 1].cmd, Cmd::Trim);
    let f = travel(&plan, 0, TravelBy::Function, true);
    assert!(st[f - 1].cmd != Cmd::Stitch && f <= t.min(c));
    // by stitch: one record at a time
    assert_eq!((travel(&plan, 10, TravelBy::Stitch, true), travel(&plan, 0, TravelBy::Stitch, false)), (11, 0));
}

#[test]
fn the_sequence_list_keeps_hidden_objects_to_show_them_again() {
    let mut app = app();
    for x in [20.0, 150.0] {
        app.run("shape.rectangle", json!({"x": x, "y": 40, "width": 80, "height": 50})).unwrap();
        app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    }
    let id = app.session.active().unwrap().selection.objects[0];
    app.run("layer.setProps", json!({"ids": [id.0], "visible": false})).unwrap();
    let texts = painted_text(&mut app, crate::embroidery::sequence::show);
    for want in ["Hidden (not sewn)", "Travel by", "Object"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
}

#[test]
fn check_design_shows_its_findings_without_closing_the_panel() {
    let mut app = app();
    app.run("window.panel", json!({"panel": "embroidery"})).unwrap();
    assert_eq!(app.ui.open_panel.as_deref(), Some("embroidery"));
    // the panel's own Check Design button: the panel stays open
    app.run("ui.embroideryCheck", json!({})).unwrap();
    assert_eq!(app.ui.open_panel.as_deref(), Some("embroidery"));
    // from the menu with the panel closed: it opens to show the findings
    app.run("window.panel", json!({"panel": "embroidery"})).unwrap();
    assert_eq!(app.ui.open_panel, None);
    app.run("ui.embroideryCheck", json!({})).unwrap();
    assert_eq!(app.ui.open_panel.as_deref(), Some("embroidery"));
}

mod design_from_prompt {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::{Value, json};
    use vectorcraft_engine::Session;

    use crate::embroidery::prompt;
    use crate::{HttpRequest, SecretStore, Services, VectorcraftApp};

    const ART: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 1000"><circle cx="500" cy="500" r="400" fill="#e07020"/><circle cx="380" cy="420" r="70" fill="#ffffff"/><circle cx="620" cy="420" r="70" fill="#ffffff"/></svg>"##;

    #[derive(Clone, Default)]
    pub(super) struct Keys(Arc<Mutex<HashMap<String, String>>>);

    impl SecretStore for Keys {
        fn load(&self, name: &str) -> Option<String> {
            self.0.lock().unwrap().get(name).cloned()
        }
        fn save(&self, name: &str, value: Option<&str>) -> Result<(), String> {
            let mut m = self.0.lock().unwrap();
            match value {
                Some(v) => m.insert(name.to_string(), v.to_string()),
                None => m.remove(name),
            };
            Ok(())
        }
    }

    pub(super) type Sent = Arc<Mutex<Vec<HttpRequest>>>;

    /// An app whose AI service answers `(status, body)` after `delay`, recording what it was sent.
    pub(super) fn app(status: u16, body: String, delay: Duration) -> (VectorcraftApp, Sent, Keys) {
        let sent: Sent = Arc::default();
        let keys = Keys::default();
        let log = sent.clone();
        let services = Services {
            http_post: Some(Arc::new(move |r: &HttpRequest| {
                log.lock().unwrap().push(r.clone());
                std::thread::sleep(delay);
                Ok((status, body.clone()))
            })),
            secrets: Some(Box::new(keys.clone())),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        (app, sent, keys)
    }

    fn reply(svg: &str) -> String {
        json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": svg}], "stop_reason": "end_turn"}).to_string()
    }

    /// Poll as frames do until the answer is in.
    fn wait(app: &mut VectorcraftApp) {
        let ctx = egui::Context::default();
        for _ in 0..1000 {
            prompt::poll(app, &ctx);
            if app.embroidery.prompt.drawing_for(0.0).is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no answer");
    }

    /// Every piece of text the open dialog paints, once it has measured and placed itself.
    fn dialog_text(app: &mut VectorcraftApp) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, Default::default());
        let raw =
            || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() };
        for _ in 0..4 {
            ctx.run_ui(raw(), |ui| crate::dialogs::show(app, &ui.ctx().clone())).textures_delta.clear();
        }
        let mut out = ctx.run_ui(raw(), |ui| crate::dialogs::show(app, &ui.ctx().clone()));
        out.textures_delta.clear();
        fn walk(s: &egui::Shape, into: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => into.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, into)),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        texts
    }

    fn objects(app: &VectorcraftApp) -> usize {
        app.session.active().unwrap().doc.layers.iter().map(|l| l.children().map_or(0, Vec::len)).sum()
    }

    #[test]
    fn the_art_is_drawn_placed_and_digitized_with_the_users_key() {
        let (mut app, sent, keys) = app(200, reply(ART), Duration::ZERO);
        assert!(crate::menus::enabled(&app, "ui.designFromPrompt"));
        app.run("ui.designFromPrompt", json!({"prompt": "a cartoon fox head", "size": 60, "colors": 4})).unwrap();
        let d = app.ui.dialog.clone().unwrap();
        assert_eq!(d.kind, prompt::KIND);
        assert_eq!((d.str("prompt").as_str(), d.f64("size", 0.0), d.f64("colors", 0.0)), ("a cartoon fox head", 60.0, 4.0));
        let texts = dialog_text(&mut app);
        for want in ["Design from Prompt", "Describe the design:", "Anthropic API key:", "Generate"] {
            assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
        }
        // the key is typed into the dialog but is never one of its fields (agents read those)
        app.embroidery.prompt.key = " sk-ant-test ".into();
        crate::dialogs::confirm(&mut app).unwrap();
        assert!(app.ui.dialog.as_ref().is_some_and(|d| !d.fields.values().any(|v| v.as_str().is_some_and(|s| s.contains("sk-ant")))));
        assert!(dialog_text(&mut app).iter().any(|t| t.starts_with("Drawing the design…")));
        wait(&mut app);
        assert!(app.ui.dialog.is_none(), "placed: the dialog closes");
        assert!(app.ui.status.starts_with("Design placed: 3 stitched objects"), "{}", app.ui.status);
        let sent = sent.lock().unwrap();
        let r = &sent[0];
        assert!(r.headers.iter().any(|(k, v)| *k == "x-api-key" && v == "sk-ant-test"));
        let body: Value = serde_json::from_str(&r.body).unwrap();
        assert!(body["messages"][0]["content"].as_str().unwrap().contains("a cartoon fox head"));
        assert!(body["system"].as_str().unwrap().contains("At most 4 different fill colours"));
        assert_eq!(keys.load(prompt::KEY_NAME).as_deref(), Some("sk-ant-test"), "remembered");
        // one group of stitched shapes, named by its description, gone with one undo
        let st = app.session.active().unwrap();
        let g = st.doc.node(st.selection.objects[0]).unwrap();
        assert_eq!(g.name.as_deref(), Some("a cartoon fox head"));
        assert!(g.children().unwrap().iter().all(|c| c.embroidery.is_some()));
        assert!(app.session.embroidery_plan().is_some_and(|p| !p.is_empty()));
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(objects(&app), 0);
    }

    /// AI-05: the Effort chosen is kept and sent, the dialog shows what a drawing should cost
    /// before it is sent (and warns at Thorough), and the status what it took.
    #[test]
    fn the_effort_is_chosen_with_its_cost_shown_and_sent() {
        let used = json!({"type": "message", "model": "claude-opus-5-5", "content": [{"type": "text", "text": ART}], "stop_reason": "end_turn", "usage": {"input_tokens": 1800, "output_tokens": 4200}});
        let (mut app, sent, _) = app(200, used.to_string(), Duration::ZERO);
        app.run("ui.designFromPrompt", json!({"prompt": "a cartoon fox head", "size": 60, "colors": 4})).unwrap();
        let texts = dialog_text(&mut app);
        assert!(texts.iter().any(|t| t == "Effort") && texts.iter().any(|t| t == "Normal"), "{texts:?}");
        let cost = texts.iter().find(|t| t.starts_with("About $")).cloned().unwrap_or_else(|| panic!("no cost line in {texts:?}"));
        assert!(cost.contains("on your Anthropic account, at most $"), "{cost}");
        assert!(!texts.iter().any(|t| t.starts_with("Thorough looks up")), "no warning at Normal");
        app.run("embroidery.aiEffort", json!({"effort": "thorough"})).unwrap();
        let texts = dialog_text(&mut app);
        assert!(texts.iter().any(|t| t.starts_with("Thorough looks up what it draws")), "{texts:?}");
        let thorough = texts.iter().find(|t| t.starts_with("About $")).cloned().unwrap_or_default();
        assert_ne!(thorough, cost, "the estimate follows the effort");
        app.run("embroidery.aiEffort", json!({"effort": "quick"})).unwrap();
        app.embroidery.prompt.key = "sk-ant-test".into();
        crate::dialogs::confirm(&mut app).unwrap();
        wait(&mut app);
        let body: Value = serde_json::from_str(&sent.lock().unwrap()[0].body).unwrap();
        assert_eq!(body["output_config"]["effort"], "low");
        // the reply's usage: tokens and dollars on the user's own key
        // 1 800 in and 4 200 out: 6 000 tokens, $0.0072 + $0.084
        assert!(app.ui.status.ends_with("6000 tokens, about $0.09"), "{}", app.ui.status);
    }

    /// AI-04: at Thorough the drawing may be researched and goes back as it stitches to be
    /// checked; the better drawing that comes back is placed.
    #[test]
    fn a_thorough_drawing_is_checked_as_it_stitches() {
        // the service answers every request with a drawing, so the check redraws it
        let (mut app, sent, _) = app(200, reply(ART), Duration::ZERO);
        app.run("embroidery.aiEffort", json!({"effort": "thorough"})).unwrap();
        app.run("ui.designFromPrompt", json!({"prompt": "a 2023 Ford F-350 crew cab", "size": 60, "colors": 4})).unwrap();
        app.embroidery.prompt.key = "sk-ant-test".into();
        crate::dialogs::confirm(&mut app).unwrap();
        wait(&mut app);
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 2, "the drawing, then its check");
        let first: Value = serde_json::from_str(&sent[0].body).unwrap();
        assert_eq!((first["output_config"]["effort"].as_str(), first["tools"][0]["name"].as_str()), (Some("high"), Some("web_search")));
        let check: Value = serde_json::from_str(&sent[1].body).unwrap();
        let m = check["messages"].as_array().unwrap();
        assert_eq!((m.len(), m[0]["content"][0]["type"].as_str()), (1, Some("image")), "a conversation of its own");
        assert!(check.get("tools").is_none(), "the check doesn't search");
        drop(sent);
        assert!(app.ui.dialog.is_none(), "placed: the dialog closes");
        assert!(app.ui.status.contains("improved after checking its stitches"), "{}", app.ui.status);
        assert_eq!(objects(&app), 1, "one drawing placed, the better one");
    }

    #[test]
    fn what_goes_wrong_is_shown_in_the_dialog() {
        let err = json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}).to_string();
        let (mut app, sent, keys) = app(401, err, Duration::ZERO);
        keys.save(prompt::KEY_NAME, Some("sk-ant-old")).unwrap();
        app.run("ui.designFromPrompt", json!({"prompt": "a star"})).unwrap();
        assert_eq!(app.embroidery.prompt.key, "sk-ant-old", "the kept key is filled in");
        // nothing is sent without a key or a description
        app.embroidery.prompt.key.clear();
        assert_eq!(crate::dialogs::confirm(&mut app).unwrap_err(), "Enter your Anthropic API key first.");
        app.embroidery.prompt.key = "sk-ant-bad".into();
        if let Some(d) = app.ui.dialog.as_mut() {
            d.fields.insert("prompt".into(), json!("  "));
        }
        assert_eq!(crate::dialogs::confirm(&mut app).unwrap_err(), "Describe the design first.");
        assert!(sent.lock().unwrap().is_empty());
        if let Some(d) = app.ui.dialog.as_mut() {
            d.fields.insert("prompt".into(), json!("a star"));
        }
        crate::dialogs::confirm(&mut app).unwrap();
        wait(&mut app);
        assert!(app.ui.dialog.is_some(), "the dialog stays open to try again");
        assert_eq!(app.embroidery.prompt.error.as_deref(), Some("Anthropic refused the API key: invalid x-api-key"));
        assert!(dialog_text(&mut app).iter().any(|t| t == "Anthropic refused the API key: invalid x-api-key"));
        assert_eq!(keys.load(prompt::KEY_NAME).as_deref(), Some("sk-ant-old"), "a refused key isn't remembered");
        assert_eq!(objects(&app), 0);
        // Remember off forgets the kept key
        if let Some(d) = app.ui.dialog.as_mut() {
            d.fields.insert("remember".into(), json!(false));
        }
        crate::dialogs::confirm(&mut app).unwrap();
        assert_eq!(keys.load(prompt::KEY_NAME), None);
        wait(&mut app);
    }

    #[test]
    fn cancel_stops_waiting_and_the_web_has_no_service() {
        let (mut app, sent, _) = app(200, reply(ART), Duration::from_millis(150));
        app.run("ui.designFromPrompt", json!({"prompt": "a star"})).unwrap();
        app.embroidery.prompt.key = "sk-ant-test".into();
        crate::dialogs::confirm(&mut app).unwrap();
        assert!(crate::dialogs::confirm(&mut app).is_err(), "one request at a time");
        crate::dialogs::cancel(&mut app);
        assert!(app.ui.dialog.is_none() && app.embroidery.prompt.drawing_for(0.0).is_none());
        std::thread::sleep(Duration::from_millis(300));
        prompt::poll(&mut app, &egui::Context::default());
        assert_eq!(objects(&app), 0, "a cancelled drawing is never placed");
        assert_eq!(sent.lock().unwrap().len(), 1);
        // without the service (the web) it can't open
        let mut web = VectorcraftApp::new(Session::new(), Services::default());
        web.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        assert!(!crate::menus::enabled(&web, "ui.designFromPrompt"));
        assert!(web.run("ui.designFromPrompt", json!({})).is_err());
    }

    /// A 40 × 20 PNG: red on the left, blue on the right.
    const LOGO_PNG: &str =
        "iVBORw0KGgoAAAANSUhEUgAAACgAAAAUCAIAAABwJOjsAAAALElEQVR42mM8ISfHQC4Ik1tFtl4mhgECoxaPWjxq8ajFoxaPWjxq8ajF9AcAqiICSl834YsAAAAASUVORK5CYII=";

    #[test]
    fn an_image_is_redrawn_and_lands_on_it() {
        // a drawing on a page with the image's aspect: a circle on the left, a square on the right
        let art = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 500"><circle cx="250" cy="250" r="200" fill="#c81e1e"/><rect x="600" y="100" width="300" height="300" fill="#1e3cc8"/></svg>"##;
        let (mut app, sent, _) = app(200, reply(art), Duration::ZERO);
        assert!(!crate::menus::enabled(&app, "ui.redrawImage"), "no image selected");
        assert!(app.run("ui.redrawImage", json!({})).is_err());
        let placed = app.run("file.place", json!({"name": "logo.png", "dataBase64": LOGO_PNG, "at": [200, 150]})).unwrap();
        let image = vectorcraft_doc::NodeId(placed["ids"][0].as_u64().unwrap());
        assert!(crate::menus::enabled(&app, "ui.redrawImage"));
        app.run("ui.redrawImage", json!({"prompt": "thicker letters"})).unwrap();
        let d = app.ui.dialog.clone().unwrap();
        assert_eq!(d.fields["image"], json!(image.0));
        // the image is 40 × 20 pt (72 ppi), 14 mm across: brought up to the 20 mm least size
        assert_eq!(d.f64("size", 0.0), 20.0);
        let texts = dialog_text(&mut app);
        for want in ["Redraw Image with Claude", "What to change (optional):", "Only redraw artwork you own or have permission to use."] {
            assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
        }
        app.embroidery.prompt.key = "sk-ant-test".into();
        crate::dialogs::confirm(&mut app).unwrap();
        wait(&mut app);
        assert!(app.ui.dialog.is_none(), "{:?}", app.embroidery.prompt.error);
        // the image went along as a PNG, before the instruction and the notes
        let body: Value = serde_json::from_str(&sent.lock().unwrap()[0].body).unwrap();
        let content = &body["messages"][0]["content"];
        assert_eq!(content[0]["source"]["media_type"], "image/png");
        assert!(content[0]["source"]["data"].as_str().unwrap().len() > 50);
        assert!(content[1]["text"].as_str().unwrap().ends_with("What to change: thicker letters"));
        // the art lands on the image: its page is the image's place, scaled about its centre to 20 mm
        let st = app.session.active().unwrap();
        let g = st.doc.node(st.selection.objects[0]).unwrap();
        assert_eq!(g.name.as_deref(), Some("Redrawn image"));
        assert!(g.children().unwrap().iter().all(|c| c.embroidery.is_some()));
        let img = st.doc.node(image).unwrap().geometric_bounds().unwrap();
        let k = 20.0 * vectorcraft_embroidery::PT_PER_MM / img.width();
        let (w, h) = (img.width() * k, img.height() * k);
        let (x0, y0) = (img.center().x - w / 2.0, img.center().y - h / 2.0);
        let b = g.geometric_bounds().unwrap();
        for (got, want) in [(b.x0, x0 + 0.05 * w), (b.x1, x0 + 0.9 * w), (b.y0, y0 + 0.1 * h), (b.y1, y0 + 0.9 * h)] {
            assert!((got - want).abs() < 0.5, "{b:?} in a frame at ({x0}, {y0}) {w} × {h}");
        }
    }

    /// AI-03: another service paints the picture with its own key; Claude then redraws it as flat
    /// shapes in the thread colours asked for, and the design is placed; the service's key is kept.
    #[test]
    fn another_services_picture_is_redrawn_by_claude_then_placed() {
        let sent: Sent = Arc::default();
        let keys = Keys::default();
        keys.save(prompt::KEY_NAME, Some("sk-ant-test")).unwrap();
        let log = sent.clone();
        let mut png = Vec::new();
        image::RgbImage::from_pixel(64, 64, image::Rgb([200, 30, 30]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let picture = json!({"data": [{"b64_json": vectorcraft_format::base64_encode(&png)}]}).to_string();
        let services = Services {
            http_post: Some(Arc::new(move |r: &HttpRequest| {
                log.lock().unwrap().push(r.clone());
                Ok(if r.url.contains("openai.com") { (200, picture.clone()) } else { (200, reply(ART)) })
            })),
            secrets: Some(Box::new(keys.clone())),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        app.run("ui.designFromPrompt", json!({"prompt": "a fire truck from the front", "colors": 4, "picture": "openai"})).unwrap();
        assert_eq!(app.ui.dialog.as_ref().unwrap().str("maker"), "openai");
        let text = dialog_text(&mut app);
        assert!(text.iter().any(|t| t == "OpenAI GPT Image API key:"), "{text:?}");
        // without the service's key nothing goes
        assert!(crate::dialogs::confirm(&mut app).is_err());
        assert!(sent.lock().unwrap().is_empty());
        app.embroidery.prompt.picture_keys.insert("openai", "sk-openai-test".into());
        app.embroidery.prompt.error = None;
        crate::dialogs::confirm(&mut app).unwrap();
        wait(&mut app);
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 2, "the picture, then the redrawing");
        assert_eq!(sent[0].url, "https://api.openai.com/v1/images/generations");
        assert!(sent[0].headers.contains(&("authorization", "Bearer sk-openai-test".to_string())));
        let redraw: Value = serde_json::from_str(&sent[1].body).unwrap();
        assert_eq!(redraw["messages"][0]["content"][0]["type"], "image", "Claude sees the picture");
        assert!(redraw["system"].as_str().unwrap().contains("At most 4 different fill colours"));
        assert!(app.embroidery.prompt.error.is_none(), "{:?}", app.embroidery.prompt.error);
        assert!(app.ui.dialog.is_none(), "placed: the dialog closes");
        assert_eq!(keys.load("openai-api-key").as_deref(), Some("sk-openai-test"), "the service's key is kept");
    }
}

#[test]
fn the_colour_count_is_previewed_with_its_threads_before_digitizing() {
    let mut app = app();
    // a photo-like ramp from red to blue on white
    let img = image::RgbaImage::from_fn(120, 80, |x, y| {
        if (10..110).contains(&x) && (10..70).contains(&y) {
            let t = (x - 10) as f32 / 100.0;
            image::Rgba([(220.0 * (1.0 - t)) as u8, 40, (220.0 * t) as u8, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    });
    let mut png = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    let placed = app.run("file.place", json!({"name": "ramp.png", "dataBase64": vectorcraft_format::base64_encode(&png)})).unwrap();
    let id = placed["ids"][0].as_u64().unwrap();
    app.run("select.set", json!({"ids": [id]})).unwrap();
    let frame = |app: &mut VectorcraftApp| {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::embroidery::panel::auto_digitize_rows(app, ui));
        out.textures_delta.clear();
    };
    frame(&mut app);
    let few = app.embroidery.colour_preview.as_ref().map(|p| (p.key, p.info["sewn"].as_u64().unwrap())).unwrap();
    assert_eq!(few.0, (id, 6, true));
    // more colours: the preview is made again, with more threads to sew
    app.embroidery.digitize_colors = 20;
    frame(&mut app);
    let many = app.embroidery.colour_preview.as_ref().map(|p| (p.key, p.info["sewn"].as_u64().unwrap())).unwrap();
    assert_eq!(many.0, (id, 20, true));
    assert!(many.1 > few.1, "{many:?} {few:?}");
    let p = app.embroidery.colour_preview.as_ref().unwrap();
    assert_eq!(p.info["threadChanges"].as_u64(), Some(many.1 - 1));
    // the white round it is the background, shown but not sewn
    assert!(p.info["colors"].as_array().unwrap().iter().any(|c| c["background"] == true));
}

mod ask_about_embroidery {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use serde_json::{Value, json};
    use vectorcraft_engine::Session;

    use super::design_from_prompt::Keys;
    use crate::embroidery::ask::{self, Access, Entry};
    use crate::embroidery::prompt;
    use crate::{HttpRequest, SecretStore, Services, VectorcraftApp};

    type Sent = Arc<Mutex<Vec<HttpRequest>>>;

    /// An app whose AI service answers with `replies` in turn, recording what it was sent, with an
    /// API key kept.
    fn app(replies: Vec<(u16, String)>) -> (VectorcraftApp, Sent, Keys) {
        let sent: Sent = Arc::default();
        let keys = Keys::default();
        keys.save(prompt::KEY_NAME, Some("sk-ant-test")).unwrap();
        let queue = Arc::new(Mutex::new(VecDeque::from(replies)));
        let log = sent.clone();
        let services = Services {
            http_post: Some(Arc::new(move |r: &HttpRequest| {
                log.lock().unwrap().push(r.clone());
                Ok(queue.lock().unwrap().pop_front().unwrap_or((500, String::new())))
            })),
            secrets: Some(Box::new(keys.clone())),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        (app, sent, keys)
    }

    fn reply(stop: &str, content: Value) -> (u16, String) {
        (200, json!({"type": "message", "model": "claude-opus-5-5", "content": content, "stop_reason": stop}).to_string())
    }

    fn answer(text: &str) -> (u16, String) {
        reply("end_turn", json!([{"type": "text", "text": text}]))
    }

    fn body(sent: &Sent, i: usize) -> Value {
        serde_json::from_str(&sent.lock().unwrap()[i].body).unwrap()
    }

    /// Poll as frames do until the reply is in (or a change waits for the user).
    fn wait(app: &mut VectorcraftApp) {
        let ctx = egui::Context::default();
        for _ in 0..1000 {
            ask::poll(app, &ctx);
            if app.embroidery.ask.waiting_for(0.0).is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no answer");
    }

    /// Every piece of text the window paints.
    fn window_text(app: &mut VectorcraftApp) -> Vec<String> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, Default::default());
        let raw =
            || egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() };
        for _ in 0..3 {
            ctx.run_ui(raw(), |ui| ask::window(app, &ui.ctx().clone())).textures_delta.clear();
        }
        let out = ctx.run_ui(raw(), |ui| ask::window(app, &ui.ctx().clone()));
        fn walk(s: &egui::Shape, into: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => into.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, into)),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        texts
    }

    fn rectangle(app: &mut VectorcraftApp) -> u64 {
        let r = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 100, "height": 50})).unwrap();
        app.run("embroidery.set", json!({"type": "fill"})).unwrap();
        r["id"].as_u64().or_else(|| r["ids"][0].as_u64()).unwrap_or_else(|| app.session.active().unwrap().selection.objects[0].0)
    }

    #[test]
    fn a_question_is_answered_with_the_conversation_and_the_design() {
        let (mut app, sent, _) = app(vec![answer("Use a cut-away stabilizer on knits."), answer("A tear-away one.")]);
        assert!(crate::menus::enabled(&app, "ui.askAboutEmbroidery"));
        let id = rectangle(&mut app);
        let r = app.run("ui.askAboutEmbroidery", json!({"question": "Which stabilizer for a T-shirt?"})).unwrap();
        assert_eq!(r, json!({"open": true, "asking": true}));
        wait(&mut app);
        let st = &app.embroidery.ask;
        assert!(st.open && st.input.is_empty() && st.error.is_none() && !st.asking(), "{:?}", st.error);
        assert_eq!(
            st.entries,
            [Entry::Question("Which stabilizer for a T-shirt?".into()), Entry::Answer("Use a cut-away stabilizer on knits.".into())]
        );
        let texts = window_text(&mut app);
        for want in ["Ask About Embroidery", "Which stabilizer for a T-shirt?", "Use a cut-away stabilizer on knits.", "Ask", "Include my design"] {
            assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Anthropic API key:"), "a kept key isn't asked for: {texts:?}");
        // the design went along in the question, its object by id
        let b = body(&sent, 0);
        let summary = b["messages"][0]["content"][0]["text"].as_str().unwrap();
        assert!(summary.starts_with("The design open now") && summary.contains("stitches;") && summary.contains(&format!("#{id} ")), "{summary}");
        assert!(summary.contains("(selected)"), "{summary}");
        // a second question carries the first and its answer exactly; without the design this time
        app.embroidery.ask.include_design = false;
        app.embroidery.ask.input = "And on a towel?".into();
        ask::send(&mut app).unwrap();
        wait(&mut app);
        let b2 = body(&sent, 1);
        let m = b2["messages"].as_array().unwrap();
        assert_eq!(m.len(), 3);
        assert_eq!(
            m[..2],
            b["messages"].as_array().unwrap()[..1]
                .iter()
                .cloned()
                .chain([json!({"role": "assistant", "content": [{"type": "text", "text": "Use a cut-away stabilizer on knits."}]})])
                .collect::<Vec<_>>()[..]
        );
        assert_eq!(m[2]["content"], json!([{"type": "text", "text": "And on a towel?"}]));
        assert_eq!(b2["system"], b["system"], "the instructions stay the same");
        // New Conversation starts over
        ask::clear(&mut app);
        assert!(app.embroidery.ask.entries.is_empty());
    }

    #[test]
    fn claude_reads_at_once_and_changes_the_design_once_allowed() {
        let (mut app, sent, _) = app(vec![
            reply(
                "tool_use",
                json!([
                    {"type": "thinking", "thinking": "", "signature": "s1"},
                    {"type": "tool_use", "id": "t1", "name": "find_commands", "input": {"query": "stitch type"}},
                    {"type": "tool_use", "id": "t2", "name": "run_command", "input": {"command": "embroidery.info", "params": {}}},
                ]),
            ),
            reply(
                "tool_use",
                json!([
                    {"type": "text", "text": "I'll make it satin."},
                    {"type": "tool_use", "id": "t3", "name": "run_command", "input": {"command": "embroidery.set", "params": {"type": "satin"}}},
                    {"type": "tool_use", "id": "t4", "name": "run_command", "input": {"command": "file.save", "params": {}}},
                ]),
            ),
            answer("Done: the rectangle is satin now."),
        ]);
        let id = rectangle(&mut app);
        app.run("ui.askAboutEmbroidery", json!({"question": "Make it satin"})).unwrap();
        wait(&mut app);
        // the search and the read ran at once and went back; the change waits
        let b = body(&sent, 1);
        let results = b["messages"][2]["content"].as_array().unwrap();
        assert_eq!(b["messages"][1]["content"][0]["signature"], "s1", "the reply went back unchanged");
        let found: Value = serde_json::from_str(results[0]["content"].as_str().unwrap()).unwrap();
        assert!(found.as_array().unwrap().iter().any(|c| c["id"] == "embroidery.set" && c["changesTheDesign"] == true), "{found}");
        assert!(results[1]["content"].as_str().unwrap().contains("\"stitches\""));
        let st = &app.embroidery.ask;
        let a = st.approval.as_ref().expect("the change waits for the user");
        assert_eq!(a.changes.len(), 1);
        assert_eq!(a.changes[0].1, "embroidery.set");
        let texts = window_text(&mut app);
        assert!(texts.iter().any(|t| t == "Claude wants to change your design:") && texts.iter().any(|t| t == "Allow"), "{texts:?}");
        let stitch = |app: &VectorcraftApp| {
            let st = app.session.active().unwrap();
            st.doc.node(vectorcraft_doc::NodeId(id)).unwrap().embroidery.as_deref().unwrap().stitch.id().to_string()
        };
        assert_eq!(stitch(&app), "fill", "nothing changes before Allow");
        ask::decide(&mut app, true);
        wait(&mut app);
        assert_eq!(stitch(&app), "satin");
        // the change's result and the blocked file command went back together
        let b = body(&sent, 2);
        let results = b["messages"][4]["content"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].get("is_error").is_none(), "{results:?}");
        assert_eq!(results[1]["is_error"], true);
        assert!(results[1]["content"].as_str().unwrap().contains("can't be run from here"));
        let st = &app.embroidery.ask;
        assert!(!st.asking());
        assert!(st.entries.iter().any(|e| matches!(e, Entry::Note(n) if n.starts_with("Changed:"))), "{:?}", st.entries);
        assert_eq!(st.entries.last(), Some(&Entry::Answer("Done: the rectangle is satin now.".into())));
        app.run("edit.undo", json!({})).unwrap();
        assert_eq!(stitch(&app), "fill", "one undo takes it back");
    }

    #[test]
    fn a_declined_change_is_not_made() {
        let (mut app, sent, _) = app(vec![
            reply(
                "tool_use",
                json!([{"type": "tool_use", "id": "t1", "name": "run_command", "input": {"command": "embroidery.set", "params": {"type": "satin"}}}]),
            ),
            answer("Then I'll leave it as it is."),
        ]);
        rectangle(&mut app);
        app.run("ui.askAboutEmbroidery", json!({"question": "Make it satin"})).unwrap();
        wait(&mut app);
        ask::decide(&mut app, false);
        wait(&mut app);
        let b = body(&sent, 1);
        assert_eq!(b["messages"][2]["content"][0]["content"], "The user declined this change.");
        assert!(app.embroidery.ask.entries.iter().any(|e| matches!(e, Entry::Note(n) if n.starts_with("Not changed:"))));
        let st = app.session.active().unwrap();
        assert_eq!(st.doc.node(st.selection.objects[0]).unwrap().embroidery.as_deref().unwrap().stitch.id(), "fill");
    }

    /// The web: the browser's fetch answers later, on the UI thread, and the key stays in its store.
    #[test]
    fn the_web_sends_through_the_browser() {
        let sent: Sent = Arc::default();
        let log = sent.clone();
        type Done = Box<dyn FnOnce(Result<(u16, String), String>)>;
        let later: std::rc::Rc<std::cell::RefCell<Vec<Done>>> = Default::default();
        let queue = later.clone();
        let keys = Keys::default();
        let services = Services {
            http_post_async: Some(std::rc::Rc::new(move |r: HttpRequest, done| {
                log.lock().unwrap().push(r);
                queue.borrow_mut().push(done);
            })),
            secrets: Some(Box::new(keys.clone())),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(Session::new(), services);
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        assert!(crate::menus::enabled(&app, "ui.askAboutEmbroidery") && crate::menus::enabled(&app, "ui.designFromPrompt"));
        app.embroidery.prompt.key = "sk-ant-web".into();
        app.run("ui.askAboutEmbroidery", json!({"question": "Which needle for denim?"})).unwrap();
        assert_eq!(sent.lock().unwrap().len(), 1);
        assert!(app.embroidery.ask.waiting_for(0.0).is_some(), "waiting for the browser");
        // the browser answers
        let done = later.borrow_mut().pop().unwrap();
        done(Ok(answer("A 90/14 or 100/16 jeans needle.")));
        ask::poll(&mut app, &egui::Context::default());
        assert_eq!(app.embroidery.ask.entries.last(), Some(&Entry::Answer("A 90/14 or 100/16 jeans needle.".into())));
        assert_eq!(keys.load(prompt::KEY_NAME).as_deref(), Some("sk-ant-web"), "remembered in the browser's store");
    }

    #[test]
    fn what_claude_may_run() {
        let access = |id: &str| ask::access(vectorcraft_engine::find_command(id).unwrap());
        for id in ["embroidery.info", "embroidery.check", "document.node", "select.all"] {
            assert_eq!(access(id), Access::Read, "{id}");
        }
        for id in ["embroidery.set", "shape.rectangle", "edit.undo", "embroidery.setParams"] {
            assert_eq!(access(id), Access::Change, "{id}");
        }
        for id in ["prefs.set", "plugin.install", "embroidery.export", "embroidery.send", "text.addFontFiles", "document.exportForScreens"] {
            assert_eq!(access(id), Access::Blocked, "{id}");
        }
        // the menus' own file commands too
        assert!(ask::blocked("file.save") && ask::blocked("ui.saveForMyMachine") && !ask::blocked("embroidery.set"));
    }

    #[test]
    fn without_a_key_or_a_question_nothing_is_sent() {
        let err = json!({"type": "error", "error": {"message": "invalid x-api-key"}}).to_string();
        let (mut app, sent, keys) = app(vec![(401, err)]);
        keys.save(prompt::KEY_NAME, None).unwrap();
        app.run("ui.askAboutEmbroidery", json!({})).unwrap();
        assert!(window_text(&mut app).iter().any(|t| t == "Anthropic API key:"), "no key yet: its row shows");
        app.embroidery.ask.input = "Why does my thread break?".into();
        assert_eq!(ask::send(&mut app).unwrap_err(), "Enter your Anthropic API key first.");
        app.embroidery.prompt.key = "sk-ant-bad".into();
        app.embroidery.ask.input = "   ".into();
        assert_eq!(ask::send(&mut app).unwrap_err(), "Type a question first.");
        assert!(sent.lock().unwrap().is_empty());
        app.embroidery.ask.input = "Why does my thread break?".into();
        ask::send(&mut app).unwrap();
        assert!(ask::send(&mut app).is_err(), "one question at a time");
        wait(&mut app);
        let st = &app.embroidery.ask;
        assert_eq!(st.error.as_deref(), Some("Anthropic refused the API key: invalid x-api-key"));
        assert_eq!(st.input, "Why does my thread break?", "the question stays to ask again");
        assert!(st.entries.is_empty() && !st.asking(), "{:?}", st.entries);
        assert_eq!(keys.load(prompt::KEY_NAME), None, "a refused key isn't remembered");
        assert!(window_text(&mut app).iter().any(|t| t == "Anthropic refused the API key: invalid x-api-key"));
        // the web has no service yet
        let mut web = VectorcraftApp::new(Session::new(), Services::default());
        assert!(!crate::menus::enabled(&web, "ui.askAboutEmbroidery"));
        assert!(web.run("ui.askAboutEmbroidery", json!({})).is_err());
    }

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, _| image::Rgb([(x % 255) as u8, 40, 40]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80).encode_image(&img).unwrap();
        out
    }

    /// CLOUD-02: signed in to StitchFoundry, the AI features are paid with credits: requests go to
    /// the service with the session in place of an API key, its refusals read as its own, and a
    /// sew-out check goes to its own endpoint with its id (and shared when asked).
    #[test]
    fn signed_in_the_ai_features_are_paid_with_credits_through_the_service() {
        use crate::embroidery::account;
        let token = "sfs_0123456789abcdef";
        let (mut app, sent, keys) = app(vec![
            (200, json!({"token": token, "account": {"email": "denise@example.com", "credits": 100}}).to_string()),
            (200, json!({"email": "denise@example.com", "credits": 100, "freeChecks": 3, "packs": [], "payments": false}).to_string()),
            answer("Cut-away on a T-shirt."),
            (402, json!({"type": "error", "error": {"type": "billing_error", "message": "not enough credits"}}).to_string()),
            answer("The gap is the knit pulling in."),
        ]);
        app.embroidery.account.server = "https://cloud.example".into();
        assert_eq!(app.run("ui.account", json!({})).unwrap()["signedIn"], false);
        app.embroidery.account.email = "denise@example.com".into();
        app.embroidery.account.password = "short".into();
        // checked before anything is sent
        account::test_sign_in(&mut app, false);
        assert_eq!(app.embroidery.account.error.as_deref(), Some("A password needs at least 8 characters."));
        assert!(sent.lock().unwrap().is_empty());
        app.embroidery.account.error = None;
        let ctx = egui::Context::default();
        let poll = |app: &mut VectorcraftApp| {
            for _ in 0..400 {
                account::poll(app, &ctx);
                if app.embroidery.account.error.is_some()
                    || !app.embroidery.account.token.is_empty() && app.embroidery.account.info.as_ref().is_some_and(|i| i.get("freeChecks").is_some())
                {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        };
        app.embroidery.account.password = "fire truck 1".into();
        account::test_sign_in(&mut app, false);
        poll(&mut app);
        let st = &app.embroidery.account;
        assert_eq!(st.token, token);
        assert!(st.password.is_empty() && st.use_credits, "signing in is choosing credits; the password is forgotten");
        assert_eq!(keys.load(vectorcraft_embroidery::sf_core::ai::cloud::TOKEN_NAME).as_deref(), Some(token));
        assert_eq!(sent.lock().unwrap()[0].url, "https://cloud.example/v1/signin");
        assert_eq!(sent.lock().unwrap()[1].url, "https://cloud.example/v1/account");
        assert!(account::paying_with_credits(&app));
        // a question goes to the service, with the session, not the Anthropic key
        app.run("ui.askAboutEmbroidery", json!({"question": "Which stabilizer?"})).unwrap();
        wait(&mut app);
        let r = sent.lock().unwrap()[2].clone();
        assert_eq!(r.url, "https://cloud.example/v1/messages");
        assert!(r.headers.contains(&("authorization", format!("Bearer {token}"))));
        assert!(r.headers.iter().all(|h| h.0 != "x-api-key"), "the Anthropic key stays here");
        // out of credits: said as StitchFoundry's
        app.embroidery.ask.input = "And the needle?".into();
        ask::send(&mut app).unwrap();
        wait(&mut app);
        assert_eq!(app.embroidery.ask.error.as_deref(), Some("Not enough Foundry credits: buy more in Foundry Account."));
        // a shared sew-out check goes to its own endpoint
        let id =
            ask::begin_check(&mut app, json!({"role": "user", "content": [{"type": "text", "text": "photos"}]}), "My sew-out".into(), true, None)
                .unwrap();
        wait(&mut app);
        let r = sent.lock().unwrap()[4].clone();
        assert_eq!(r.url, "https://cloud.example/v1/sewout");
        assert!(r.headers.contains(&("x-stitchfoundry-check", id.clone())) && r.headers.contains(&("x-stitchfoundry-share", "1".to_string())));
        // paying with credits off: back to the own key and Anthropic
        app.embroidery.account.use_credits = false;
        assert!(!account::paying_with_credits(&app));
        assert_eq!(account::key_to_send(&mut app), "sk-ant-test");
    }

    /// SO-01: photos of a sew-out are made the size Claude looks at; a check sends them with how
    /// the design should look, its summary and the answers, under the check's own instructions, and
    /// the answer (and the questions after it) are an Ask conversation in its sew-out mode.
    #[test]
    fn a_sew_out_check_sends_the_photos_and_the_design_and_answers_in_the_ask_window() {
        use crate::embroidery::sewout;
        let (image, size, thumb) = sewout::prepare(&jpeg(3200, 2000)).unwrap();
        assert_eq!(image.media_type, "image/jpeg");
        assert_eq!(size, (1568, 980), "made no larger than Claude looks at");
        assert!(thumb.size[0] <= 96 && thumb.size[1] <= 96);
        assert_eq!(sewout::prepare(&jpeg(400, 300)).unwrap().1, (400, 300), "never enlarged");
        assert!(sewout::prepare(b"ftypheic not a photo").unwrap_err().contains("JPEG"));

        let (mut app, sent, _) = app(vec![
            answer("The front sewed well. The gap above the bumper is the knit pulling in: overlap the shapes 1 mm."),
            answer("Cut-away, always, on a T-shirt."),
        ]);
        assert!(crate::menus::enabled(&app, "ui.checkSewout"));
        rectangle(&mut app);
        app.run("embroidery.applyFabric", json!({"name": "Jersey knit (T-shirt)"})).unwrap();
        assert_eq!(app.run("ui.checkSewout", json!({})).unwrap(), json!({"open": true, "photos": 0, "error": null}));
        assert!(sewout::check(&mut app).is_err(), "no photo yet");
        assert_eq!(app.embroidery.sewout.error.as_deref(), Some("Add a photo of the sew-out first."));
        sewout::add_photo(&mut app, "front.jpg", &jpeg(800, 600));
        sewout::add_photo(&mut app, "notes.txt", b"hello");
        assert_eq!(app.embroidery.sewout.photos.len(), 1);
        assert!(app.embroidery.sewout.error.as_deref().unwrap().starts_with("This photo can't be read"));
        app.embroidery.sewout.happened = vec![false, false, true, false, false, false];
        app.embroidery.sewout.notes = "a gap above the bumper".into();
        sewout::check(&mut app).unwrap();
        assert!(!app.embroidery.sewout.open && app.embroidery.ask.open);
        assert_eq!(app.embroidery.ask.mode, ask::Mode::SewOut);
        wait(&mut app);
        let b = body(&sent, 0);
        assert!(b["system"].as_str().unwrap().contains("sew-out"));
        let c = b["messages"][0]["content"].as_array().unwrap();
        let images: Vec<&str> = c.iter().filter(|x| x["type"] == "image").map(|x| x["source"]["media_type"].as_str().unwrap()).collect();
        assert_eq!(images.len(), 2, "the photo, then how the design should look");
        let text: String = c.iter().filter_map(|x| x["text"].as_str()).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Fabric: Jersey knit (T-shirt)") && text.contains("bobbin thread didn't cut"), "{text}");
        assert!(text.contains("My notes: a gap above the bumper") && text.contains("The design that was sewn"), "{text}");
        let st = &app.embroidery.ask;
        assert_eq!(st.entries[0], Entry::Question("My sew-out (1 photos)".into()));
        assert!(matches!(&st.entries[1], Entry::Answer(a) if a.contains("overlap the shapes")));
        assert!(window_text(&mut app).iter().any(|t| t == "Check My Sew-out"), "the window says what it is");
        // a question after it keeps the check's instructions and the photos
        app.embroidery.ask.input = "Which stabilizer?".into();
        ask::send(&mut app).unwrap();
        wait(&mut app);
        let b = body(&sent, 1);
        assert!(b["system"].as_str().unwrap().contains("sew-out"));
        assert_eq!(b["messages"].as_array().unwrap().len(), 3);
        assert_eq!(b["messages"][0]["content"][1]["type"], "image");
        // a new conversation is a plain Ask again
        ask::clear(&mut app);
        assert_eq!(app.embroidery.ask.mode, ask::Mode::Ask);
    }

    /// SO-02: after a check, Report to StitchFoundry has Claude write what StitchFoundry could do
    /// better, and sends it with the photos only when the user agrees and is signed in.
    #[test]
    fn a_sew_out_report_is_drafted_by_claude_and_sent_with_leave() {
        use crate::embroidery::sewout;
        let (mut app, sent, _) = app(vec![
            answer("The gap above the bumper is the knit pulling in: overlap the shapes 1 mm."),
            answer(
                "Title: Gaps where shapes meet on knit\nThe shapes were digitized edge to edge. StitchFoundry could overlap touching shapes on stretchy fabrics.",
            ),
            (200, json!({"report": "report-1-1", "emailed": true, "refunded": 10}).to_string()),
        ]);
        rectangle(&mut app);
        assert!(!crate::menus::enabled(&app, "ui.reportSewout"), "a check first");
        assert!(app.run("ui.reportSewout", json!({})).is_err());
        app.run("ui.checkSewout", json!({})).unwrap();
        sewout::add_photo(&mut app, "front.jpg", &jpeg(800, 600));
        sewout::check(&mut app).unwrap();
        wait(&mut app);
        assert!(crate::menus::enabled(&app, "ui.reportSewout"));
        let r = app.run("ui.reportSewout", json!({"draft": true})).unwrap();
        assert_eq!(r["drafting"], true);
        wait(&mut app);
        sewout::poll_report(&mut app, &egui::Context::default());
        let f = &app.embroidery.sewout.report;
        assert_eq!(f.title, "Gaps where shapes meet on knit");
        assert!(f.text.starts_with("The shapes were digitized") && !f.drafting);
        let asked = body(&sent, 1);
        let q = asked["messages"].as_array().unwrap().last().unwrap()["content"].to_string();
        assert!(q.contains("people who make StitchFoundry"), "{q}");
        // not without leave, nor signed out
        assert!(app.run("ui.reportSewout", json!({"send": true})).is_err());
        assert!(app.run("ui.reportSewout", json!({"consent": true, "send": true})).unwrap_err().contains("Sign in"));
        assert_eq!(sent.lock().unwrap().len(), 2, "nothing sent");
        app.embroidery.account.server = "https://cloud.example".into();
        app.embroidery.account.token = "sfs_0123456789abcdef".into();
        app.run("ui.reportSewout", json!({"note": "On a cotton T-shirt.", "send": true})).unwrap();
        let ctx = egui::Context::default();
        for _ in 0..400 {
            sewout::poll_report(&mut app, &ctx);
            if app.embroidery.sewout.report.sent.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.embroidery.sewout.report.sent.as_deref(), Some("report-1-1"));
        assert_eq!(app.embroidery.sewout.report.refunded, 10);
        let r = sent.lock().unwrap()[2].clone();
        assert_eq!(r.url, "https://cloud.example/v1/report");
        assert!(r.headers.contains(&("authorization", "Bearer sfs_0123456789abcdef".to_string())));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["consent"], true);
        assert_eq!(b["photos"].as_array().unwrap().len(), 1);
        assert_eq!(b["note"], "On a cotton T-shirt.");
        assert!(b["conversation"].as_str().unwrap().contains("overlap the shapes 1 mm"));
    }

    /// PUB-01: Publish to MakerFoundry sends the design, in the format chosen, with the sew-out
    /// photos and the Foundry Account session, and opens the draft's page; MakerFoundry's messages
    /// are shown as it wrote them, and an ended sign-in opens Foundry Account.
    #[test]
    fn a_design_is_published_to_makerfoundry_as_a_draft() {
        use crate::embroidery::{publish, sewout};
        let said = |status: u16, m: &str| (status, json!({"error": {"message": m}}).to_string());
        let (mut app, sent, keys) = app(vec![
            (201, json!({"id": 42, "url": "https://makerfoundry.app/designs/42", "kind": "embroidery", "format": "PES"}).to_string()),
            said(429, "You've sent 50 designs today: try again tomorrow."),
            said(401, "Your sign-in has ended: sign in again."),
            (413, String::new()),
            (201, json!({"id": 7, "url": "http://127.0.0.1:8080/designs/7", "kind": "embroidery", "format": "DST"}).to_string()),
        ]);
        let publish = |app: &mut VectorcraftApp, p: Value| {
            app.run(publish::COMMAND, p).unwrap();
            let ctx = egui::Context::default();
            for _ in 0..400 {
                publish::poll(app, &ctx);
                if !app.run(publish::COMMAND, json!({})).unwrap()["sending"].as_bool().unwrap() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("no answer");
        };
        // nothing stitched, or signed out: not now
        assert!(!crate::menus::enabled(&app, publish::COMMAND));
        rectangle(&mut app);
        assert!(!crate::menus::enabled(&app, publish::COMMAND), "signed out");
        assert!(app.run(publish::COMMAND, json!({"send": true})).unwrap_err().contains("Sign in"));
        assert!(sent.lock().unwrap().is_empty());
        app.embroidery.account.token = "sfs_0123456789abcdef".into();
        assert!(crate::menus::enabled(&app, publish::COMMAND));
        sewout::add_photo(&mut app, "front.jpg", &jpeg(400, 300));
        app.embroidery.publish.open = false;

        // PES by default, with the photos: the draft's page opens
        publish(&mut app, json!({"send": true}));
        let r = sent.lock().unwrap()[0].clone();
        assert_eq!(r.url, "https://makerfoundry.app/api/v1/designs");
        assert!(r.headers.contains(&("authorization", "Bearer sfs_0123456789abcdef".to_string())));
        assert!(r.headers.contains(&("content-type", "application/json".to_string())));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert!(b["file_name"].as_str().unwrap().ends_with(".pes"), "{}", b["file_name"]);
        let file = vectorcraft_format::base64_decode(b["file"].as_str().unwrap()).unwrap();
        assert!(file.starts_with(b"#PES"), "a PES file");
        assert_eq!(b["photos"].as_array().unwrap().len(), 1);
        assert!(b["app"].as_str().unwrap().starts_with("StitchFoundry "));
        assert_eq!(app.pending_url.as_deref(), Some("https://makerfoundry.app/designs/42"));
        assert_eq!(app.embroidery.publish.sent.as_ref().map(|d| d.id), Some(42));

        // MakerFoundry's own sentence, as it is
        publish(&mut app, json!({"format": "DST", "photos": false, "send": true}));
        assert_eq!(app.embroidery.publish.error.as_deref(), Some("You've sent 50 designs today: try again tomorrow."));
        let b: Value = serde_json::from_str(&sent.lock().unwrap()[1].body).unwrap();
        assert!(b["file_name"].as_str().unwrap().ends_with(".dst"));
        assert_eq!(b["photos"], json!([]));
        // the sign-in ended: Foundry Account opens on Sign In
        publish(&mut app, json!({"send": true}));
        assert_eq!(app.embroidery.publish.error.as_deref(), Some("Your sign-in has ended: sign in again."));
        assert!(app.embroidery.account.open && app.embroidery.account.sign_in_shown);
        publish(&mut app, json!({"send": true}));
        assert_eq!(app.embroidery.publish.error.as_deref(), Some("The design is too big for MakerFoundry."));

        // a MakerFoundry on this computer, kept for next time; nothing but https:// elsewhere
        publish(&mut app, json!({"server": "http://127.0.0.1:8080", "send": true}));
        assert_eq!(sent.lock().unwrap()[4].url, "http://127.0.0.1:8080/api/v1/designs");
        assert_eq!(app.pending_url.as_deref(), Some("http://127.0.0.1:8080/designs/7"));
        assert_eq!(keys.load("makerfoundry-server").as_deref(), Some("http://127.0.0.1:8080"));
        assert!(app.run(publish::COMMAND, json!({"server": "http://makerfoundry.example", "send": true})).unwrap_err().contains("https://"));
        assert!(app.run(publish::COMMAND, json!({"format": "svg"})).is_err());
        assert_eq!(sent.lock().unwrap().len(), 5);
    }

    /// The hub's answer to an offer: the address to upload the file to, and the preview's too
    /// when `preview`.
    fn stitchshop_slots(preview: bool) -> (u16, String) {
        let mut v = json!({"id": "it_1", "url": "https://store.example/inbox/9/it_1?X-Amz-Signature=ab", "method": "PUT", "expiresIn": 900, "headers": {"x-amz-checksum-sha256": "q83v"}});
        if preview {
            v["preview"] =
                json!({"url": "https://store.example/inbox/9/it_1.preview?s=1", "method": "PUT", "headers": {"content-type": "image/png"}});
        }
        (200, v.to_string())
    }

    /// Polls Send to StitchShop until it is no longer sending.
    fn stitchshop_wait(app: &mut VectorcraftApp) {
        use crate::embroidery::stitchshop;
        let ctx = egui::Context::default();
        for _ in 0..400 {
            stitchshop::poll(app, &ctx);
            if !app.run(stitchshop::COMMAND, json!({})).unwrap()["sending"].as_bool().unwrap() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("no answer");
    }

    /// Checks the job order's design was offered to shop s2 with its details, its file and preview
    /// uploaded where the hub said, and the offer marked done; returns the file name offered.
    fn assert_offered_and_uploaded(log: &[HttpRequest]) -> String {
        use crate::Method;
        use vectorcraft_embroidery::sf_core::ai::stitchshop::sha256_hex;
        assert_eq!(log[1].url, "https://hub.stitchbench.app/v1/inbox/put");
        let b: Value = serde_json::from_str(&log[1].body).unwrap();
        let name = b["fileName"].as_str().unwrap();
        assert!(name.starts_with("J-0001 ") && name.ends_with(".dst"), "{name}");
        assert_eq!(b["toShopId"], "s2");
        let m = &b["meta"];
        assert_eq!((m["job"].as_str(), m["customer"].as_str(), m["quantity"].as_u64()), (Some("J-0001"), Some("Rovers FC"), Some(24)));
        assert_eq!((m["placement"].as_str(), m["due"].as_str(), m["notes"].as_str()), (Some("left-chest"), Some("2026-11-01"), Some("Club crest")));
        assert_eq!((m["program"].as_str(), m["format"].as_str()), (Some("StitchFoundry"), Some("dst")));
        assert!(m["stitches"].as_u64().unwrap() > 100 && m["widthMm"].as_f64().unwrap() > 30.0, "{m}");
        // the file goes to the store as announced, with the hub's headers; then the preview
        let file = log[2].bytes.clone().unwrap();
        assert_eq!((log[2].method, log[2].url.as_str()), (Method::Put, "https://store.example/inbox/9/it_1?X-Amz-Signature=ab"));
        assert!(file.starts_with(b"LA:"), "a DST file");
        assert_eq!((b["size"].as_u64(), b["sha256"].as_str()), (Some(file.len() as u64), Some(sha256_hex(&file).as_str())));
        assert_eq!(log[2].headers, vec![("x-amz-checksum-sha256", "q83v".to_string())]);
        let png = log[3].bytes.clone().unwrap();
        assert_eq!(log[3].url, "https://store.example/inbox/9/it_1.preview?s=1");
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(b["preview"], json!({"size": png.len(), "sha256": sha256_hex(&png), "type": "image/png"}));
        assert_eq!((log[4].url.as_str(), log[4].body.as_str()), ("https://hub.stitchbench.app/v1/inbox/done", r#"{"id":"it_1"}"#));
        name.to_string()
    }

    /// SHOP-01: Send to StitchShop asks the hub for the account's shops, offers the design (DST,
    /// its preview and its details, the job order's too), uploads the file and the preview to the
    /// addresses the hub signed, and says it is done; Take It Back retracts it. The hub's
    /// messages are shown as it wrote them, and an ended sign-in opens Foundry Account.
    #[test]
    fn a_design_is_sent_to_the_shops_stitchshop_inbox() {
        use crate::Method;
        use crate::embroidery::stitchshop;
        let said = |status: u16, m: &str| (status, json!({"error": m}).to_string());
        let slots = stitchshop_slots;
        let linked = "No StitchHub account is linked to this Foundry Account yet. In your shop's Settings, Foundry Account, link it, or sign in at the hub's account page with it.";
        let (mut app, sent, keys) = app(vec![
            (200, json!({"shops": [{"id": "s1", "name": "Main St", "program": "StitchShop"}, {"id": "s2", "name": "Back Room", "program": "StitchBench"}]}).to_string()),
            slots(true),
            (200, String::new()),
            (200, String::new()),
            (200, json!({"ok": true}).to_string()),
            (200, json!({"ok": true, "retracted": 1}).to_string()),
            said(404, linked),
            slots(false),
            (403, "<Error><Code>SignatureDoesNotMatch</Code></Error>".into()),
            said(401, "The Foundry Account service doesn't know that token: sign in to StitchFoundry again."),
        ]);
        let wait = stitchshop_wait;
        // nothing stitched, or signed out: not now
        assert!(!crate::menus::enabled(&app, stitchshop::COMMAND));
        rectangle(&mut app);
        assert!(!crate::menus::enabled(&app, stitchshop::COMMAND), "signed out");
        assert!(app.run(stitchshop::COMMAND, json!({"send": true})).unwrap_err().contains("Sign in"));
        assert!(sent.lock().unwrap().is_empty());
        app.embroidery.account.token = "sfs_0123456789abcdef".into();
        assert!(crate::menus::enabled(&app, stitchshop::COMMAND));
        app.embroidery.stitchshop.open = false;

        // opening asks for the shops, for the picker
        app.run(stitchshop::COMMAND, json!({})).unwrap();
        wait(&mut app);
        let r = sent.lock().unwrap()[0].clone();
        assert_eq!((r.method, r.url.as_str()), (Method::Get, "https://hub.stitchbench.app/v1/inbox/shops"));
        assert!(r.headers.contains(&("authorization", "Bearer sfs_0123456789abcdef".to_string())));
        let st = app.run(stitchshop::COMMAND, json!({})).unwrap();
        assert_eq!(st["shops"][1], json!({"id": "s2", "name": "Back Room", "program": "StitchBench"}));
        assert_eq!(st["format"], "dst");

        // from a job order, to one shop: offered, uploaded (file then preview), done
        let job = app
            .run("embroidery.job.create", json!({"customer": "Rovers FC", "quantity": 24, "due": "2026-11-01", "garment": "Navy polo", "placement": "left-chest", "notes": "Club crest"}))
            .unwrap();
        app.run(stitchshop::COMMAND, json!({"job": job["id"], "shop": "s2", "send": true})).unwrap();
        wait(&mut app);
        assert_eq!(app.embroidery.stitchshop.error, None);
        let name = assert_offered_and_uploaded(&sent.lock().unwrap());
        let name = name.as_str();
        let st = app.run(stitchshop::COMMAND, json!({})).unwrap();
        assert_eq!(st["sent"]["shop"], "Back Room");

        // Take It Back
        app.run(stitchshop::COMMAND, json!({"retract": true})).unwrap();
        wait(&mut app);
        assert_eq!(sent.lock().unwrap()[5].url, "https://hub.stitchbench.app/v1/inbox/retract");
        let st = app.run(stitchshop::COMMAND, json!({})).unwrap();
        assert_eq!((st["sent"].is_null(), st["retracted"].as_str()), (true, Some(name)));

        // the hub's own sentence, as it is
        app.run(stitchshop::COMMAND, json!({"job": null, "preview": false, "send": true})).unwrap();
        wait(&mut app);
        assert_eq!(app.embroidery.stitchshop.error.as_deref(), Some(linked));
        assert!(serde_json::from_str::<Value>(&sent.lock().unwrap()[6].body).unwrap().get("preview").is_none());
        // the store refused the upload: said, and nothing is done
        app.run(stitchshop::COMMAND, json!({"send": true})).unwrap();
        wait(&mut app);
        assert_eq!(app.embroidery.stitchshop.error.as_deref(), Some("The design couldn't be uploaded (error 403): try again."));
        assert_eq!(sent.lock().unwrap().len(), 9);
        // the sign-in ended: Foundry Account opens on Sign In
        app.run(stitchshop::COMMAND, json!({"send": true})).unwrap();
        wait(&mut app);
        assert!(app.embroidery.stitchshop.error.as_deref().unwrap().contains("sign in to StitchFoundry again"));
        assert!(app.embroidery.account.open && app.embroidery.account.sign_in_shown);

        // a hub on this computer, kept for next time; nothing but https:// elsewhere
        app.run(stitchshop::COMMAND, json!({"hub": "http://127.0.0.1:8787"})).unwrap();
        assert_eq!(keys.load("stitchshop-hub").as_deref(), Some("http://127.0.0.1:8787"));
        assert!(app.run(stitchshop::COMMAND, json!({"hub": "http://hub.example"})).unwrap_err().contains("https://"));
        assert!(app.run(stitchshop::COMMAND, json!({"format": "svg"})).is_err());
        assert!(app.run(stitchshop::COMMAND, json!({"job": 999})).is_err());
    }
}

/// VIEW-01: the free viewer shows the design with its size, stitches and sewing time, nothing to
/// edit, Get StitchFoundry Free and Edit It Here; Edit It Here is the full app again.
#[test]
fn the_viewer_shows_the_design_and_its_facts_with_nothing_to_edit() {
    let mut app = app();
    app.run("embroidery.newDesign", json!({"hoop": "Brother 100 × 100 (4×4)"})).unwrap();
    app.run("shape.rectangle", json!({"x": 60, "y": 60, "width": 80, "height": 60})).unwrap();
    app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    assert_eq!(app.run("ui.viewer", json!({})).unwrap()["viewer"], true);
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let frame = |app: &mut VectorcraftApp| {
        let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1100.0, 700.0))), ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| crate::embroidery::viewer::show(app, ui));
        out.textures_delta.clear();
        fn walk(s: &egui::Shape, into: &mut Vec<String>) {
            match s {
                egui::Shape::Text(t) => into.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, into)),
                _ => {}
            }
        }
        let mut texts = Vec::new();
        out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        texts
    };
    frame(&mut app);
    let texts = frame(&mut app);
    for want in ["Get StitchFoundry Free", "Edit It Here"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
    let facts = texts.iter().find(|t| t.contains(" mm · ")).unwrap_or_else(|| panic!("no facts in {texts:?}"));
    assert!(facts.contains(" × 21 mm") && facts.contains("stitches") && facts.ends_with("to sew"), "{facts}");
    assert_eq!(app.run("ui.viewer", json!({"on": false})).unwrap()["viewer"], false);
    assert!(!app.ui.viewer);
}

#[test]
fn auto_digitize_offers_a_border_round_the_subject() {
    let mut app = app();
    let texts = painted_text_in(&mut app, crate::embroidery::panel::auto_digitize_rows, 900.0);
    assert!(texts.iter().any(|t| t == "Border round the subject"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "Border width" || t == "Add Border"), "{texts:?}");
    app.embroidery.border_on = true;
    let texts = painted_text_in(&mut app, crate::embroidery::panel::auto_digitize_rows, 900.0);
    for want in ["Border width", "Satin edge", "Add Border"] {
        assert!(texts.iter().any(|t| t == want), "{want} missing from {texts:?}");
    }
}
