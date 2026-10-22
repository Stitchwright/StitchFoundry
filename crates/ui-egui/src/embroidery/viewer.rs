//! The free viewer (VIEW-01): a design shared by link (`?open=…&view` on the web app) shown with
//! nothing to edit: the design, what it is (size, stitches, threads, sewing time) and two ways on:
//! Edit It Here (the full app, with the design open) and Get StitchFoundry Free (the website).

use egui::{Color32, RichText, Ui};
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::Tokens;

/// Where Get StitchFoundry Free goes.
pub const SITE: &str = "https://stitchfoundry.app";

/// The viewer's screen in place of the app's chrome.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::top("viewer-bar").frame(egui::Frame::NONE.fill(t.panel).inner_margin(12.0)).show(ui, |ui| bar(app, ui, &t));
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.pasteboard)).show(ui, |ui| {
        if app.session.active().is_some() {
            crate::canvas::show(app, ui);
        } else {
            // a link that fails to open says why here: the viewer has no status bar
            let note = crate::i18n::msg(&app.ui.status).to_string();
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.4);
                ui.heading(tl!("Opening the design…"));
                if !note.is_empty() {
                    ui.label(RichText::new(note).color(t.text_dim));
                }
            });
        }
    });
}

fn bar(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    let info: Value = if app.session.active().is_some() { app.run("embroidery.info", json!({})).unwrap_or_default() } else { Value::Null };
    let title = app.session.active().map(|d| d.title()).unwrap_or_default();
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(if title.is_empty() { tl!("A shared design") } else { title.as_str() })
                    .font(crate::theme::semibold(18.0))
                    .color(t.text),
            );
            if let (Some(w), Some(h)) = (info["widthMm"].as_f64(), info["heightMm"].as_f64()) {
                let facts = crate::i18n::fmt(
                    tl!("{w} × {h} mm · {stitches} stitches · {time} to sew"),
                    &[
                        ("w", &format!("{w:.0}")),
                        ("h", &format!("{h:.0}")),
                        ("stitches", &info["stitches"].as_u64().unwrap_or(0).to_string()),
                        ("time", info["time"].as_str().unwrap_or("–")),
                    ],
                );
                ui.label(RichText::new(facts).size(13.0).color(t.text_dim));
            }
            // the threads in sewing order
            ui.horizontal_wrapped(|ui| {
                for c in info["colors"].as_array().map(Vec::as_slice).unwrap_or_default() {
                    let hex = c["color"].as_str().unwrap_or("#000000");
                    let rgb = vectorcraft_embroidery::sf_core::threads::Rgb::from_hex(hex)
                        .unwrap_or(vectorcraft_embroidery::sf_core::threads::Rgb::new(0, 0, 0));
                    let (rect, r) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, egui::CornerRadius::same(3), Color32::from_rgb(rgb.r, rgb.g, rgb.b));
                    ui.painter().rect_stroke(rect, egui::CornerRadius::same(3), egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                    r.on_hover_text(c["name"].as_str().unwrap_or(hex));
                }
            });
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::widgets::primary_button(ui, tl!("Get StitchFoundry Free")).clicked() {
                app.open_url(SITE);
            }
            if ui.button(tl!("Edit It Here")).clicked() {
                app.run("ui.viewer", json!({ "on": false })).ok();
            }
        });
    });
}
