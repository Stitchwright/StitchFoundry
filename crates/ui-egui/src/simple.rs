//! Simple mode (E2): what someone new to embroidery sees once they choose "New to embroidery".
//! In place of the toolbar, a column of large labelled buttons (add words, a shape, an appliqué
//! piece or a picture, watch it sew, save for my machine); in place of the panels, Your Design:
//! how the chosen part sews, its thread and size, or the whole design's figures. Every button
//! runs an ordinary command.

use egui::{Color32, RichText, Ui, vec2};
use serde_json::{Value, json};
use vectorcraft_doc::NodeKind;

use crate::VectorcraftApp;
use crate::state::Experience;
use crate::theme::{self, Tokens};

/// The left column's width.
const BAR: f32 = 176.0;
/// The height kept at the column's foot for Save for My Machine, Home, Show everything, the screen
/// and Text size until they have been drawn once (then their measured height is kept).
const BOTTOM: f32 = 220.0;
/// Your Design's width.
const PANEL: f32 = 290.0;

/// Whether simple mode is on (Window › Experience › New to Embroidery).
pub fn active(app: &VectorcraftApp) -> bool {
    app.ui.experience == Experience::Beginner
}

/// What simple mode is asking for: words to add, or a shape to pick.
#[derive(Clone, Debug, Default)]
pub struct SimpleState {
    /// Add Words is open, with what has been typed and how tall (mm).
    pub words: Option<(String, f64)>,
    /// The line selected when Add Words opened (a path), and whether the words follow it.
    pub words_along: Option<(u64, bool)>,
    /// The shape Add Words sets the words in without a line: straight, arcUp, arcDown or circle.
    pub words_shape: &'static str,
    /// The shape picker is open, for appliqué or not.
    pub shapes: Option<bool>,
    /// Start Over is asking to be sure.
    pub start_over: bool,
    /// Where Keep Safe last saved the design.
    pub kept: Option<String>,
    /// Smaller or Bigger (percent) waiting for a yes: the design has stops for fabric.
    pub resize: Option<f64>,
}

/// The text sizes simple mode offers (UI scaling).
const TEXT_SIZES: [f64; 3] = [1.0, 1.25, 1.5];

/// A large button filling the column's width.
fn big(ui: &mut Ui, text: &str, primary: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let label = RichText::new(text).size(15.0).color(if primary { Color32::WHITE } else { t.text });
    let b = egui::Button::new(label).min_size(vec2(ui.available_width(), 40.0)).corner_radius(6.0);
    ui.add(if primary { b.fill(t.accent) } else { b })
}

/// The left column of actions.
pub fn left_bar(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(10)).stroke(egui::Stroke::new(1.0, t.border));
    egui::Panel::left("simple-bar").resizable(false).exact_size(BAR).frame(frame).show(ui, |ui| {
        let doc = app.session.active().is_some();
        ui.spacing_mut().item_spacing.y = 8.0;
        // the buttons scroll when the window is short (a small screen, or the biggest text size),
        // so they never run under Text size and Home at the bottom
        let foot_id = egui::Id::unique("simple-bar-foot");
        let foot = ui.ctx().data(|d| d.get_temp::<f32>(foot_id)).unwrap_or(BOTTOM);
        let room = (ui.available_height() - foot - 12.0).max(120.0);
        // a scroll bar that shows whenever the buttons don't all fit, not only under the pointer
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
            // the guide works with or without a design open
            if big(ui, tl!("Make a Design…"), false).on_hover_text(tl!("Step by step: words, a shape, appliqué, a picture or a sample")).clicked()
            {
                app.run("ui.makeDesign", json!({})).ok();
            }
            ui.add_space(6.0);
            ui.add_enabled_ui(doc, |ui| {
                if big(ui, tl!("Select"), false).on_hover_text(tl!("Click a part of the design to choose it, drag it to move it")).clicked() {
                    app.run("tool.select", json!({ "tool": "selection" })).ok();
                }
                if big(ui, tl!("Add Words…"), false).on_hover_text(tl!("A name or a few words, ready to sew")).clicked() {
                    open_words(app);
                }
                if big(ui, tl!("Add a Shape…"), false).on_hover_text(tl!("A heart, a star, a flower and more, ready to sew")).clicked() {
                    app.simple.shapes = Some(false);
                }
                if big(ui, tl!("Add Appliqué…"), false)
                    .on_hover_text(tl!("A fabric shape: the machine stops for you to lay the fabric, then sews it down"))
                    .clicked()
                {
                    app.simple.shapes = Some(true);
                }
                if big(ui, tl!("Add a Picture…"), false).on_hover_text(tl!("A photo or drawing to turn into stitches")).clicked() {
                    add_picture(app);
                }
                ui.add_space(6.0);
                if big(ui, tl!("Watch It Sew"), false).on_hover_text(tl!("See the design sew on screen, stitch by stitch")).clicked() {
                    let p = &mut app.embroidery.player;
                    (p.active, p.playing, p.pos) = (true, true, 0.0);
                }
                ui.horizontal(|ui| {
                    let half = vec2((ui.available_width() - 8.0) / 2.0, 40.0);
                    let undo = egui::Button::new(RichText::new(tl!("Undo")).size(15.0)).min_size(half).corner_radius(6.0);
                    if ui.add(undo).on_hover_text(tl!("Take back the last change")).clicked() {
                        app.run("edit.undo", json!({})).ok();
                    }
                    let redo = egui::Button::new(RichText::new(tl!("Redo")).size(15.0)).min_size(half).corner_radius(6.0);
                    if ui.add(redo).on_hover_text(tl!("Put back what Undo took back")).clicked() {
                        app.run("edit.redo", json!({})).ok();
                    }
                });
                if big(ui, tl!("Start Over…"), false).on_hover_text(tl!("Clear the design and begin again")).clicked() {
                    app.simple.start_over = true;
                }
            });
        });
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            text_size(app, ui);
            screen(app, ui);
            if ui.link(tl!("Show everything")).clicked() {
                app.run("window.experience", json!({ "level": "all" })).ok();
            }
            if ui.link(tl!("Home")).clicked() {
                app.run("app.home", json!({})).ok();
            }
            // the one thing every design ends with: always in view, above the column's foot
            ui.add_space(6.0);
            ui.add_enabled_ui(doc, |ui| {
                if big(ui, tl!("Save for My Machine"), true)
                    .on_hover_text(tl!("Write the design for your machine to its USB stick or folder"))
                    .clicked()
                {
                    app.run("ui.saveForMyMachine", json!({})).ok();
                }
            });
            let used = ui.min_rect().height();
            ui.ctx().data_mut(|d| d.insert_temp(foot_id, used));
        });
    });
}

/// Text size: three buttons, the letter bigger on each, for the whole window's text.
fn text_size(app: &mut VectorcraftApp, ui: &mut Ui) {
    let now = app.session.prefs.ui_scaling;
    ui.horizontal(|ui| {
        for (i, scale) in TEXT_SIZES.into_iter().enumerate() {
            let label = RichText::new("A").size(16.0 + 5.0 * i as f32);
            let on = (now - scale).abs() < 0.01;
            if ui.selectable_label(on, label).on_hover_text(tl!("Text size")).clicked() {
                app.run("prefs.set", json!({ "key": "uiScaling", "value": scale })).ok();
            }
        }
    });
    ui.label(RichText::new(tl!("Text size")).size(14.0));
}

/// Add a Picture: the file dialog opens in Pictures, and the picture chosen goes straight in, with
/// none of the Place dialog's options (the web picks its own way).
fn add_picture(app: &mut VectorcraftApp) {
    if app.services.place_async.is_some() || app.services.pick_open.is_none() {
        app.run("file.place", json!({})).ok();
        return;
    }
    crate::picks::button(app, |app| {
        if let Some(path) = crate::picks::open(app, &crate::guide::picture_pick())
            && let Err(e) = app.run("file.place", json!({ "path": path }))
        {
            app.status(e);
        }
    });
}

/// A sewing time as people say it: "4 h 31 min", "12 min" (from h:mm:ss), else as given.
pub(crate) fn plain_time(hms: &str) -> String {
    let parts: Vec<u64> = hms.split(':').filter_map(|p| p.trim().parse().ok()).collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (*h, *m, *s),
        [m, s] => (0, *m, *s),
        _ => return hms.to_string(),
    };
    // to the nearest minute, and never "0 min" for something that sews
    let total = (h * 3600 + m * 60 + sec + 30) / 60;
    let (h, m) = (total / 60, (total % 60).max(u64::from(total == 0)));
    if h > 0 {
        crate::i18n::fmt(tl!("{h} h {m} min"), &[("h", &h.to_string()), ("m", &m.to_string())])
    } else {
        crate::i18n::fmt(tl!("{m} min"), &[("m", &m.to_string())])
    }
}

/// Screen: Light or Dark, for the whole window (Light reads more easily for many eyes).
fn screen(app: &mut VectorcraftApp, ui: &mut Ui) {
    use crate::theme::Brightness;
    let now = app.ui.brightness;
    ui.horizontal(|ui| {
        for (b, label) in [(Brightness::Light, tl!("Light")), (Brightness::Dark, tl!("Dark"))] {
            let on =
                now == b || (b == Brightness::Dark && now == Brightness::MediumDark) || (b == Brightness::Light && now == Brightness::MediumLight);
            if ui.selectable_label(on, RichText::new(label).size(15.0)).clicked() {
                app.run("window.brightness", json!({ "brightness": b.id() })).ok();
            }
        }
    });
    ui.label(RichText::new(tl!("Light or dark screen")).size(14.0));
}

/// Keep Safe (E6): in simple mode, a design with changes is saved on its own every few minutes
/// (with Data Recovery's copies): in place, or as a new design in My Designs. Desktop only.
pub fn keep_safe(app: &mut VectorcraftApp) {
    if cfg!(target_arch = "wasm32") || !active(app) || app.session.active().is_none() {
        return;
    }
    // My Designs: the preference, else Documents/StitchFoundry Designs
    let Some(folder) = vectorcraft_engine::cmd::embroidery_safe::designs_folder(&app.session) else { return };
    match app.session.execute("file.keepSafe", &json!({ "folder": folder.to_string_lossy() })) {
        Ok(r) => {
            if let Some(path) = r["saved"].as_str() {
                app.simple.kept = Some(path.to_string());
            }
        }
        Err(e) => app.status(e.to_string()),
    }
}

/// How a part sews, as simple mode names it: label, stitch type id.
const SEW_AS: [(&str, &str); 5] =
    [("Filled", "fill"), ("Smooth edge (satin)", "satin"), ("Outline", "run"), ("Appliqué", "applique"), ("Not sewn", "none")];

/// The right column: the chosen part, else the whole design.
pub fn right_panel(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(12)).stroke(egui::Stroke::new(1.0, t.border));
    egui::Panel::right("simple-panel").resizable(false).exact_size(PANEL).frame(frame).show(ui, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let Some(st) = app.session.active() else {
                    ui.label(RichText::new(tl!("Start a design from the Home screen.")).size(14.0).color(t.text_dim));
                    return;
                };
                let selected = st.selection.objects.clone();
                let image = selected.first().and_then(|id| st.doc.node(*id)).is_some_and(|n| matches!(n.kind, NodeKind::Image(_)));
                if selected.is_empty() {
                    design_summary(app, ui, &t);
                } else if image {
                    ui.label(RichText::new(tl!("A picture")).font(theme::semibold(16.0)).color(t.text));
                    ui.label(RichText::new(tl!("Turn it into stitches in your thread colours.")).size(13.0).color(t.text_dim));
                    if big(ui, tl!("Turn Into Stitches"), true).clicked() {
                        app.run("embroidery.autoDigitize", json!({})).ok();
                    }
                    part_actions(app, ui);
                } else {
                    part(app, ui, &t, selected.len());
                }
            });
        });
    });
}

/// The chosen part: how it sews, its thread, its size.
fn part(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens, count: usize) {
    let title = if count == 1 { tl!("The chosen part") } else { tl!("The chosen parts") };
    ui.label(RichText::new(title).font(theme::semibold(16.0)).color(t.text));
    // how the first chosen part sews, and in what thread
    let (ty, hex) = app
        .session
        .active()
        .and_then(|st| {
            // the part itself, or the first stitched part inside it (a shape is a group)
            let top = st.doc.node(*st.selection.objects.first()?)?;
            let mut found = None;
            top.walk(&mut |m| {
                if found.is_none() {
                    found = m.embroidery.as_deref().map(|e| (m, e));
                }
            });
            let (n, e) = found?;
            use vectorcraft_embroidery::sf_core::attr::StitchType;
            let ty = match &e.stitch {
                StitchType::Fill(_) => "fill",
                StitchType::Satin(_) => "satin",
                StitchType::Run(_) | StitchType::Redwork(_) => "run",
                StitchType::Applique(_) => "applique",
                _ => "other",
            };
            Some((ty.to_string(), vectorcraft_embroidery::node_thread(n, e).color.to_hex()))
        })
        .unwrap_or_else(|| ("none".into(), "#1a3e8c".into()));
    ui.label(RichText::new(tl!("Sew it as")).size(14.0).color(t.text));
    for (label, id) in SEW_AS {
        let on = ty == id;
        if ui.selectable_label(on, RichText::new(sew_label(label)).size(14.0)).clicked() {
            app.run("embroidery.set", json!({ "type": id })).ok();
        }
    }
    if ty != "none" {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("Thread colour")).size(14.0));
            let mut c = vectorcraft_embroidery::sf_core::Rgb::from_hex(&hex).map_or([26, 62, 140], |c| [c.r, c.g, c.b]);
            if ui.color_edit_button_srgb(&mut c).changed() {
                let hex = format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
                app.run("embroidery.setThread", json!({ "color": hex })).ok();
            }
        });
    }
    part_actions(app, ui);
}

/// Size and delete, for whatever is chosen.
fn part_actions(app: &mut VectorcraftApp, ui: &mut Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(tl!("Size")).size(14.0));
        if ui.button(RichText::new(tl!("Smaller")).size(14.0)).clicked() {
            resize(app, 90.0);
        }
        if ui.button(RichText::new(tl!("Bigger")).size(14.0)).clicked() {
            resize(app, 110.0);
        }
    });
    if ui.button(RichText::new(tl!("Delete")).size(14.0)).clicked() {
        app.run("edit.clear", json!({})).ok();
    }
}

/// Smaller or Bigger: at once, unless the design stops for fabric (appliqué, in-the-hoop pieces)
/// or carries a project's cut sizes, which a new size would no longer match: then it asks first.
fn resize(app: &mut VectorcraftApp, percent: f64) {
    let info = app.session.execute("embroidery.info", &json!({})).unwrap_or_default();
    let careful = info["stops"].as_u64().unwrap_or(0) > 0 || !info["notes"].as_str().unwrap_or("").trim().is_empty();
    if careful {
        app.simple.resize = Some(percent);
    } else {
        app.run("object.scale", json!({ "sx": percent })).ok();
    }
}

/// The whole design: its hoop, size, stitches, colours and sewing time.
fn design_summary(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(tl!("Your design")).font(theme::semibold(16.0)).color(t.text));
    let info = app.session.execute("embroidery.info", &json!({})).unwrap_or_default();
    let stitches = info["stitches"].as_u64().unwrap_or(0);
    if stitches == 0 {
        ui.label(
            RichText::new(tl!("Nothing to sew yet. Add words, a shape or a picture with the buttons on the left.")).size(14.0).color(t.text_dim),
        );
    } else {
        let row = |ui: &mut Ui, k: &str, v: String| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(k).size(14.0).color(t.text_dim));
                ui.label(RichText::new(v).size(14.0).color(t.text));
            });
        };
        let (w, h) = (info["widthMm"].as_f64().unwrap_or(0.0), info["heightMm"].as_f64().unwrap_or(0.0));
        let inches = app.session.prefs.units_general == "inches";
        let size = if inches { format!("{:.1} × {:.1} in", w / 25.4, h / 25.4) } else { format!("{w:.0} × {h:.0} mm") };
        row(ui, tl!("Size"), size);
        row(ui, tl!("Stitches"), stitches.to_string());
        row(ui, tl!("Thread colours"), info["colors"].as_array().map_or(0, Vec::len).to_string());
        if let Some(time) = info["time"].as_str() {
            row(ui, tl!("Sewing time"), plain_time(time));
        }
        if info["fits"].as_bool() == Some(false) {
            ui.label(RichText::new(tl!("The design is bigger than the hoop: make it smaller before you sew.")).size(13.0).color(t.error));
        }
        // the steps: each thread in sewing order, named as the design names it (an in-the-hoop
        // design's threads say what to do)
        ui.add_space(6.0);
        ui.label(RichText::new(tl!("Steps")).font(theme::semibold(14.0)).color(t.text));
        for (i, c) in info["colors"].as_array().map(Vec::as_slice).unwrap_or_default().iter().enumerate() {
            let hex = c["color"].as_str().unwrap_or("#000000");
            let rgb = vectorcraft_embroidery::sf_core::Rgb::from_hex(hex).unwrap_or_default();
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 4.0, Color32::from_rgb(rgb.r, rgb.g, rgb.b));
                let name =
                    crate::embroidery::thread_name(&vectorcraft_embroidery::sf_core::Thread::new(rgb, c["name"].as_str().unwrap_or("").trim()));
                ui.add(egui::Label::new(RichText::new(format!("{}. {name}", i + 1)).size(14.0).color(t.text)).wrap());
            });
        }
        if info["stops"].as_u64().unwrap_or(0) > 0 {
            ui.label(RichText::new(tl!("The machine also stops where you lay or trim fabric.")).size(12.0).color(t.text_dim));
        }
        // a project's supplies and steps, printed with its picture
        if !info["notes"].as_str().unwrap_or("").trim().is_empty() && ui.button(RichText::new(tl!("Print the Steps…")).size(14.0)).clicked() {
            if let Err(e) = app.run("embroidery.worksheet", json!({ "kind": "production" })) {
                app.status(e);
            } else {
                app.run("file.print", json!({})).ok();
            }
        }
    }
    ui.add_space(8.0);
    ui.label(RichText::new(tl!("Click a part of the design to change how it sews.")).size(13.0).color(t.text_dim));
    if let Some(path) = &app.simple.kept {
        ui.add_space(8.0);
        ui.label(RichText::new(tl!("Saved for you automatically in My Designs:")).size(12.0).color(t.text_dim));
        ui.label(RichText::new(path.as_str()).size(12.0).color(t.text_dim));
    }
}

/// A Sew It As label in the UI language.
fn sew_label(label: &str) -> &str {
    match label {
        "Filled" => tl!("Filled"),
        "Smooth edge (satin)" => tl!("Smooth edge (satin)"),
        "Outline" => tl!("Just the outline"),
        "Appliqué" => tl!("Appliqué"),
        _ => tl!("Not sewn"),
    }
}

/// Add Words, the shape picker and the questions (Start Over, resizing), while open.
pub fn windows(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if let Some(percent) = app.simple.resize {
        let mut answer: Option<bool> = None;
        egui::Window::new(tl!("Change the Size?")).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(
                RichText::new(tl!("This design stops for fabric you cut to size. Made bigger or smaller, the pieces may no longer fit.")).size(14.0),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, tl!("Keep the Size")).clicked() {
                    answer = Some(false);
                }
                if ui.button(tl!("Change It Anyway")).clicked() {
                    answer = Some(true);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            answer = Some(false);
        }
        if let Some(yes) = answer {
            if yes {
                app.run("object.scale", json!({ "sx": percent })).ok();
            }
            app.simple.resize = None;
        }
    }
    if app.simple.start_over {
        let mut answer: Option<bool> = None;
        egui::Window::new(tl!("Start Over")).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(tl!("Clear everything in this design and begin again?")).size(15.0));
            ui.label(RichText::new(tl!("You can still get it back with Undo.")).size(13.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, tl!("Start Over")).clicked() {
                    answer = Some(true);
                }
                if ui.button(tl!("Keep It")).clicked() {
                    answer = Some(false);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            answer = Some(false);
        }
        if let Some(yes) = answer {
            if yes && let Err(e) = app.run("embroidery.startOver", json!({})) {
                app.status(e);
            }
            app.simple.start_over = false;
        }
    }
    if let Some(applique) = app.simple.shapes {
        let mut open = true;
        let mut chosen: Option<&str> = None;
        let title = if applique { tl!("Add Appliqué") } else { tl!("Add a Shape") };
        egui::Window::new(title).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(RichText::new(tl!("Choose a shape")).size(15.0));
            ui.horizontal_wrapped(|ui| {
                for name in vectorcraft_engine::cmd::embroidery_simple::SIMPLE_SHAPES {
                    if ui.add(egui::Button::new(RichText::new(shape_name(name)).size(15.0)).min_size(vec2(100.0, 36.0))).clicked() {
                        chosen = Some(name);
                    }
                }
            });
        });
        if let Some(name) = chosen {
            if let Err(e) = app.run("embroidery.addShape", json!({ "name": name, "applique": applique })) {
                app.status(e);
            }
            app.simple.shapes = None;
        } else if !open {
            app.simple.shapes = None;
        }
    }
}

/// A shape's name in the UI language.
pub(crate) fn shape_name(name: &str) -> &str {
    match name {
        "Heart" => tl!("Heart"),
        "Star" => tl!("Star"),
        "Flower" => tl!("Flower"),
        "Leaf" => tl!("Leaf"),
        "Sun" => tl!("Sun"),
        "Moon" => tl!("Moon"),
        "Paw" => tl!("Paw"),
        "Crown" => tl!("Crown"),
        _ => tl!("Snowflake"),
    }
}

/// The menus simple mode keeps: the everyday file, edit, view and help commands.
pub fn keeps(id: &str, _p: &Value) -> bool {
    const KEEP: &[&str] = &[
        "file.newDialog",
        "ui.makeDesign",
        "file.open",
        "file.save",
        "file.saveAs",
        "file.close",
        "file.print",
        "file.quit",
        "file.place",
        "ui.saveForMyMachine",
        "ui.saveForMemoryCard",
        "ui.myMachine",
        "edit.undo",
        "edit.redo",
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.clear",
        "select.all",
        "view.zoomIn",
        "view.zoomOut",
        "view.fitArtboard",
        "view.actualSize",
        "ui.calibrateLifeSize",
        "help.show",
        "ui.askAboutEmbroidery",
        "ui.checkSewout",
        "ui.account",
        "help.about",
        "window.experience",
        "app.home",
    ];
    KEEP.contains(&id)
}

/// Opens Add Words (`ui.addWords`; simple mode's Add Words button): with one line selected (a
/// path), the words can follow it.
pub fn open_words(app: &mut VectorcraftApp) -> Value {
    let line = app.session.active().and_then(|st| match st.selection.objects.as_slice() {
        [id] => st.doc.node(*id).filter(|n| matches!(n.kind, NodeKind::Path { .. })).map(|_| id.0),
        _ => None,
    });
    app.simple.words = Some((String::new(), 20.0));
    app.simple.words_along = line.map(|id| (id, true));
    app.simple.words_shape = "straight";
    json!({ "open": true, "along": line })
}

/// Add Words, while open (in every way of working): what to say, how tall, and with a line
/// selected, whether the words follow it.
pub fn words_window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    let Some((mut text, mut height)) = app.simple.words.clone() else { return };
    let mut along = app.simple.words_along;
    let mut open = true;
    let mut action: Option<bool> = None;
    egui::Window::new(tl!("Add Words")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(RichText::new(tl!("What should it say?")).size(15.0));
        let r = ui.add(egui::TextEdit::singleline(&mut text).font(egui::FontId::proportional(18.0)).desired_width(360.0));
        r.request_focus();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(tl!("How tall")).size(15.0));
            // a long slider with its filled part shown, so it reads as one in every theme
            ui.spacing_mut().slider_width = 200.0;
            ui.add(egui::Slider::new(&mut height, 8.0..=80.0).suffix(" mm").fixed_decimals(0).trailing_fill(true));
        });
        ui.label(RichText::new(format!("≈ {:.1} in", height / 25.4)).size(12.0));
        if let Some((_, follow)) = &mut along {
            ui.add_space(4.0);
            ui.checkbox(follow, RichText::new(tl!("Along the selected line")).size(15.0));
        }
        // without a line to follow: straight, or round a curve made for the words
        if !matches!(along, Some((_, true))) {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                for (id, label) in [("straight", tl!("Straight")), ("arcUp", tl!("Arc Up")), ("arcDown", tl!("Arc Down")), ("circle", tl!("Circle"))]
                {
                    if ui.selectable_label(app.simple.words_shape == id, RichText::new(label).size(15.0)).clicked() {
                        app.simple.words_shape = id;
                    }
                }
            });
            if along.is_none() {
                ui.label(RichText::new(tl!("To follow a line of your own, draw it first, select it, then Add Words.")).size(12.0));
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            // (Enter adds: the text field keeps the keyboard, so its losing focus can't say so)
            if crate::widgets::primary_button(ui, tl!("Add")).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                action = Some(true);
            }
            if ui.button(tl!("Cancel")).clicked() {
                action = Some(false);
            }
        });
    });
    app.simple.words_along = along;
    match action {
        Some(true) if !text.trim().is_empty() => {
            let mut p = json!({ "text": text, "height": height });
            if let Some((line, true)) = along {
                p["path"] = json!(line);
            } else if app.simple.words_shape != "straight" {
                p["shape"] = json!(app.simple.words_shape);
            }
            if let Err(e) = app.run("embroidery.addWords", p) {
                app.status(e);
            }
            app.simple.words = None;
        }
        Some(false) => app.simple.words = None,
        _ if !open => app.simple.words = None,
        _ => app.simple.words = Some((text, height)),
    }
}
