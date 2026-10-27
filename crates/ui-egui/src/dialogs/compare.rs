//! Compare Designs (CMP-01): two designs side by side, each a machine file or the design open
//! now: a picture of each and their figures (`embroidery.compare`), with how much the second
//! differs from the first. Overlay draws the two on one picture at the same scale, centred on
//! each other, the first in blue and the second in red, so where they differ shows.

use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};

/// What one side shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Side {
    /// The design open now.
    Design,
    /// A machine file.
    File(String),
}

impl Side {
    fn param(&self) -> Value {
        match self {
            Side::Design => json!("design"),
            Side::File(p) => json!({ "path": p }),
        }
    }
}

/// The window's state.
#[derive(Debug, Default)]
pub struct CompareState {
    pub open: bool,
    pub a: Option<Side>,
    pub b: Option<Side>,
    /// Both designs on one picture instead of side by side.
    pub overlay: bool,
    /// The comparison of `a` and `b`, or why there is none; worked out when either changes.
    result: Option<Result<Value, String>>,
}

/// `ui.compareDesigns {a?, b?, overlay?}`: opens the window, a side each as `"design"` or a path.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let side = |v: &Value| match v {
        Value::String(s) if s == "design" => Some(Side::Design),
        Value::String(s) => Some(Side::File(s.clone())),
        _ => None,
    };
    let st = &mut app.embroidery.compare;
    st.open = true;
    if let Some(o) = p.get("overlay").and_then(Value::as_bool) {
        st.overlay = o;
    }
    if let Some(a) = p.get("a").and_then(side) {
        st.a = Some(a);
        st.result = None;
    }
    if let Some(b) = p.get("b").and_then(side) {
        st.b = Some(b);
        st.result = None;
    }
    // the design open now on the left, to start with
    if st.a.is_none() && app.session.embroidery_plan().is_some_and(|p| !p.is_empty()) {
        st.a = Some(Side::Design);
    }
    refresh(app);
    Ok(json!({ "open": true, "result": app.embroidery.compare.result.clone().and_then(Result::ok) }))
}

/// Works the comparison out when both sides are chosen.
fn refresh(app: &mut VectorcraftApp) {
    let st = &app.embroidery.compare;
    let (Some(a), Some(b)) = (st.a.clone(), st.b.clone()) else { return };
    if st.result.is_some() {
        return;
    }
    let r = app.session.execute("embroidery.compare", &json!({ "a": a.param(), "b": b.param() })).map_err(|e| e.to_string());
    app.embroidery.compare.result = Some(r);
}

/// The colours the overlay draws the first and the second design in.
const OVERLAY: [egui::Color32; 2] = [egui::Color32::from_rgb(31, 111, 235), egui::Color32::from_rgb(214, 48, 49)];

/// A design's preview runs (mm from its top left), `k` points a millimetre from `off`, in their
/// thread colours or all in `color`.
fn draw_runs(painter: &egui::Painter, figures: &Value, off: egui::Pos2, k: f32, color: Option<egui::Color32>) {
    for block in figures["preview"].as_array().map(Vec::as_slice).unwrap_or_default() {
        let c = block["color"]
            .as_str()
            .and_then(vectorcraft_embroidery::sf_core::Rgb::from_hex)
            .unwrap_or(vectorcraft_embroidery::sf_core::Rgb::new(0, 0, 0));
        let stroke = egui::Stroke::new(1.0, color.unwrap_or(egui::Color32::from_rgb(c.r, c.g, c.b)));
        for run in block["runs"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let pts: Vec<egui::Pos2> = run
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|q| Some(off + egui::vec2(q.get(0)?.as_f64()? as f32 * k, q.get(1)?.as_f64()? as f32 * k)))
                .collect();
            if pts.len() >= 2 {
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

/// Both designs on one picture `size` across: the same scale, centred on each other, the first
/// in blue and the second in red (half see-through, so both show where they cross).
fn overlay(ui: &mut egui::Ui, a: &Value, b: &Value, size: egui::Vec2) {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, 4.0, egui::Color32::from_rgb(250, 248, 242));
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let dims = |f: &Value| (f["widthMm"].as_f64().unwrap_or(0.0), f["heightMm"].as_f64().unwrap_or(0.0));
    let ((wa, ha), (wb, hb)) = (dims(a), dims(b));
    let (w, h) = (wa.max(wb), ha.max(hb));
    if !(w > 0.0 && h > 0.0) {
        return;
    }
    let inner = rect.shrink(10.0);
    let k = (f64::from(inner.width()) / w).min(f64::from(inner.height()) / h) as f32;
    for (f, (fw, fh), color) in [(a, (wa, ha), OVERLAY[0]), (b, (wb, hb), OVERLAY[1])] {
        let off = inner.center() - egui::vec2(fw as f32 * k, fh as f32 * k) * 0.5;
        draw_runs(ui.painter(), f, off, k, Some(color.gamma_multiply(0.6)));
    }
}

/// A design's picture: its preview runs (mm from its top left) fitted into `rect`.
fn picture(ui: &mut egui::Ui, figures: &Value, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let t = Tokens::get(ui.ctx());
    // light fabric, unless the threads are mostly light too (white on white doesn't show)
    let light = |c: &Value| {
        c.as_str()
            .and_then(vectorcraft_embroidery::sf_core::Rgb::from_hex)
            .map(|c| 0.299 * f32::from(c.r) + 0.587 * f32::from(c.g) + 0.114 * f32::from(c.b) > 200.0)
    };
    let threads = figures["threads"].as_array().map(Vec::as_slice).unwrap_or_default();
    let light_threads = threads.iter().filter(|c| light(c) == Some(true)).count();
    let fabric = if light_threads * 2 > threads.len() { egui::Color32::from_rgb(60, 62, 66) } else { egui::Color32::from_rgb(250, 248, 242) };
    ui.painter().rect_filled(rect, 4.0, fabric);
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let (w, h) = (figures["widthMm"].as_f64().unwrap_or(0.0), figures["heightMm"].as_f64().unwrap_or(0.0));
    if !(w > 0.0 && h > 0.0) {
        return;
    }
    let inner = rect.shrink(8.0);
    let k = (f64::from(inner.width()) / w).min(f64::from(inner.height()) / h) as f32;
    let off = inner.center() - egui::vec2(w as f32 * k, h as f32 * k) * 0.5;
    draw_runs(ui.painter(), figures, off, k, None);
}

/// The figure `key`'s row label.
fn label(key: &str) -> &'static str {
    match key {
        "stitches" => tl!("Stitches"),
        "trims" => tl!("Trims"),
        "jumps" => tl!("Jumps"),
        "colorChanges" => tl!("Colour changes"),
        "stops" => tl!("Stops"),
        "widthMm" => tl!("Width (mm)"),
        "heightMm" => tl!("Height (mm)"),
        "threadM" => tl!("Top thread (m)"),
        "bobbinM" => tl!("Bobbin thread (m)"),
        "seconds" => tl!("Sewing time"),
        "averageStitchMm" => tl!("Average stitch (mm)"),
        "maxStitchMm" => tl!("Longest stitch (mm)"),
        "shortStitches" => tl!("Stitches under 1 mm"),
        _ => tl!("Stitches over 7 mm"),
    }
}

/// The window, while open.
pub(super) fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.compare.open {
        return;
    }
    let mut open = true;
    let mut pick: Option<(bool, Option<Side>)> = None;
    let has_design = app.session.embroidery_plan().is_some_and(|p| !p.is_empty());
    egui::Window::new(tl!("Compare Designs")).open(&mut open).collapsible(false).default_size([620.0, 640.0]).show(ctx, |ui| {
        let t = Tokens::get(ui.ctx());
        let both = app.embroidery.compare.result.as_ref().is_some_and(Result::is_ok);
        if both {
            ui.horizontal(|ui| {
                let o = &mut app.embroidery.compare.overlay;
                ui.selectable_value(o, false, tl!("Side by Side"));
                ui.selectable_value(o, true, tl!("One over the Other"));
            });
        }
        let st = &app.embroidery.compare;
        let result = st.result.as_ref();
        let overlaid = st.overlay && both;
        ui.columns(2, |cols| {
            for (k, col) in cols.iter_mut().enumerate() {
                let first = k == 0;
                let side = if first { &st.a } else { &st.b };
                col.label(egui::RichText::new(if first { tl!("First design") } else { tl!("Second design") }).font(theme::semibold(13.0)));
                col.horizontal(|ui| {
                    if ui.button(tl!("Open File…")).clicked() {
                        pick = Some((first, None));
                    }
                    if has_design && ui.add_enabled(side.as_ref() != Some(&Side::Design), egui::Button::new(tl!("This Design"))).clicked() {
                        pick = Some((first, Some(Side::Design)));
                    }
                });
                let fig = result.and_then(|r| r.as_ref().ok()).map(|v| &v[if first { "a" } else { "b" }]);
                match (side, fig) {
                    (None, _) => {
                        col.label(egui::RichText::new(tl!("Choose a machine file or the design open now")).color(t.text_dim));
                    }
                    (Some(_), Some(f)) => {
                        if overlaid {
                            // its colour in the overlay below
                            col.label(egui::RichText::new(format!("■ {}", f["name"].as_str().unwrap_or_default())).color(OVERLAY[k]));
                        } else {
                            col.label(f["name"].as_str().unwrap_or_default());
                            picture(col, f, 240.0);
                        }
                    }
                    (Some(Side::File(p)), None) => {
                        col.label(p.as_str());
                    }
                    (Some(Side::Design), None) => {
                        col.label(tl!("This design"));
                    }
                }
            }
        });
        if overlaid && let Some(Ok(v)) = result {
            let w = ui.available_width().min(520.0);
            overlay(ui, &v["a"], &v["b"], egui::vec2(w, w * 0.75));
            ui.label(
                egui::RichText::new(tl!("The first design in blue, the second in red, at the same scale and centred on each other"))
                    .size(11.0)
                    .color(t.text_dim),
            );
        }
        ui.separator();
        match result {
            None => {
                ui.label(egui::RichText::new(tl!("Choose both designs to compare them")).color(t.text_dim));
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e.as_str()).color(t.error));
            }
            Some(Ok(v)) => {
                egui::Grid::new("compare-figures").striped(true).num_columns(4).show(ui, |ui| {
                    ui.label("");
                    ui.label(egui::RichText::new(tl!("First")).font(theme::semibold(12.0)));
                    ui.label(egui::RichText::new(tl!("Second")).font(theme::semibold(12.0)));
                    ui.label(egui::RichText::new(tl!("Difference")).font(theme::semibold(12.0)));
                    ui.end_row();
                    for d in v["differences"].as_array().map(Vec::as_slice).unwrap_or_default() {
                        let key = d["key"].as_str().unwrap_or_default();
                        ui.label(label(key));
                        let show = |x: &Value| {
                            if key == "seconds" {
                                let s = x.as_f64().unwrap_or(0.0).max(0.0) as u64;
                                format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
                            } else {
                                x.to_string()
                            }
                        };
                        ui.label(show(&d["a"]));
                        ui.label(show(&d["b"]));
                        match d["percent"].as_f64() {
                            Some(p) if p.abs() >= 0.05 => {
                                let big = p.abs() >= 10.0;
                                let text = egui::RichText::new(format!("{p:+.1}%"));
                                ui.label(if big { text.color(t.accent).strong() } else { text });
                            }
                            Some(_) => {
                                ui.label(egui::RichText::new(tl!("same")).color(t.text_dim));
                            }
                            None => {
                                ui.label(if d["a"] == d["b"] {
                                    egui::RichText::new(tl!("same")).color(t.text_dim)
                                } else {
                                    egui::RichText::new("—")
                                });
                            }
                        }
                        ui.end_row();
                    }
                });
                ui.label(
                    egui::RichText::new(tl!("Differences of 10% or more are highlighted. The real test is sewing both."))
                        .size(11.0)
                        .color(t.text_dim),
                );
            }
        }
    });
    if let Some((first, side)) = pick {
        let side = match side {
            Some(s) => Some(s),
            None => {
                let filters: &'static [&'static str] = vectorcraft_engine::cmd::embroidery::MACHINE_READ_EXTS;
                let f = crate::FilePick { filters: vec![crate::FilePick::filter("Machine embroidery files", filters)], ..Default::default() };
                crate::picks::open(app, &f).map(Side::File)
            }
        };
        if let Some(s) = side {
            let st = &mut app.embroidery.compare;
            if first {
                st.a = Some(s);
            } else {
                st.b = Some(s);
            }
            st.result = None;
            refresh(app);
        }
    }
    if !open {
        app.embroidery.compare.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The window compares the design open now with a file of it written denser.
    #[test]
    fn the_window_compares_two_designs() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 400, "height": 300})).unwrap();
        app.run("shape.rectangle", json!({"x": 50, "y": 50, "width": 150, "height": 100})).unwrap();
        app.run("embroidery.set", json!({"type": "fill"})).unwrap();
        let dir = std::env::temp_dir().join(format!("sf-compare-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("first.dst");
        let out = app.run("embroidery.export", json!({"format": "dst"})).unwrap();
        std::fs::write(&file, vectorcraft_format::base64_decode(out["dataBase64"].as_str().unwrap()).unwrap()).unwrap();
        app.run("embroidery.setParams", json!({"params": {"spacing": 0.3}})).unwrap();
        let r = app.run("ui.compareDesigns", json!({"a": file.to_string_lossy(), "b": "design"})).unwrap();
        assert_eq!(r["open"], true);
        let res = &r["result"];
        assert_eq!(res["a"]["name"], "first.dst");
        assert!(res["b"]["stitches"].as_u64() > res["a"]["stitches"].as_u64());
        // overlaid: the window draws one picture of both
        app.run("ui.compareDesigns", json!({"overlay": true})).unwrap();
        assert!(app.embroidery.compare.overlay);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut texts = Vec::new();
        for _ in 0..3 {
            let mut out = ctx.run_ui(
                egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() },
                |ui| show(&mut app, &ui.ctx().clone()),
            );
            out.textures_delta.clear();
            fn walk(s: &egui::Shape, into: &mut Vec<String>) {
                match s {
                    egui::Shape::Text(t) => into.push(t.galley.text().to_string()),
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, into)),
                    _ => {}
                }
            }
            texts.clear();
            out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
        }
        assert!(texts.iter().any(|t| t == "One over the Other") && texts.iter().any(|t| t == "■ first.dst"), "{texts:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
