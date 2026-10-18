//! Make a Design (E3): a step-by-step guide for someone new to embroidery. What to make (words,
//! a shape, an appliqué piece, a picture or a sample), which hoop, the words or shape or picture,
//! the fabric it goes on, then the thread colours, watching it sew and saving it for the machine.
//! Each step runs ordinary commands (`embroidery.makeDesign`, `embroidery.applyFabric`,
//! `embroidery.recolor`, `ui.saveForMyMachine`).

use egui::{RichText, Ui, vec2};
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};

/// The pictures the guide opens.
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "webp"];

/// The steps, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Step {
    #[default]
    What,
    Hoop,
    Details,
    Fabric,
    Colours,
    Watch,
    Save,
}

impl Step {
    const ALL: [Step; 7] = [Step::What, Step::Hoop, Step::Details, Step::Fabric, Step::Colours, Step::Watch, Step::Save];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// What the guide has been told so far.
#[derive(Clone, Debug)]
pub struct GuideState {
    pub open: bool,
    pub step: Step,
    /// words, shape, applique, picture or sample.
    pub what: Option<&'static str>,
    pub hoop: Option<String>,
    /// The fabric chosen (`embroidery.applyFabric`).
    pub fabric: Option<String>,
    pub text: String,
    /// How tall the words are (mm).
    pub height: f64,
    pub shape: Option<&'static str>,
    pub sample: Option<String>,
    pub picture: Option<String>,
    /// The in-the-hoop project chosen (`embroidery.ith.projects`).
    pub project: Option<String>,
    /// The in-the-hoop project's shape, if any.
    pub ith_shape: Option<&'static str>,
    /// Its extras: a ribbon (loop or band), clear vinyl on top, a button mark.
    pub ribbon: bool,
    pub vinyl: bool,
    pub button: bool,
    /// The last in-the-hoop project made, for its cutting templates.
    pub last_ith: Option<Value>,
    /// A quilt block: its size (inches), pattern and line spacing (mm).
    pub block: (f64, f64),
    pub pattern: &'static str,
    pub spacing: f64,
    /// A border strip rather than a block.
    pub border: bool,
    /// A corner square where two border strips meet.
    pub corner: bool,
    /// Words, a shape (as appliqué or not) in the middle for the quilting to go round.
    pub middle_text: String,
    pub middle_shape: Option<&'static str>,
    pub middle_applique: bool,
    /// The last thing that went wrong, shown under the step.
    pub error: Option<String>,
}

impl Default for GuideState {
    fn default() -> Self {
        GuideState {
            open: false,
            step: Step::What,
            what: None,
            hoop: None,
            fabric: None,
            text: String::new(),
            height: 25.0,
            shape: None,
            sample: None,
            picture: None,
            project: None,
            ith_shape: None,
            ribbon: false,
            vinyl: false,
            button: false,
            last_ith: None,
            block: (5.0, 7.0),
            pattern: "stipple",
            spacing: 6.0,
            border: false,
            corner: false,
            middle_text: String::new(),
            middle_shape: None,
            middle_applique: false,
            error: None,
        }
    }
}

/// `ui.makeDesign`: opens the guide at its first step.
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    app.guide = GuideState { open: true, ..GuideState::default() };
    Ok(json!({ "open": true }))
}

/// A large choice: a title and a line under it, the whole card clickable.
fn card(ui: &mut Ui, t: &Tokens, title: &str, line: &str, on: bool) -> egui::Response {
    let fill = if on { t.accent.gamma_multiply(0.25) } else { t.panel };
    let inner = egui::Frame::NONE
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, if on { t.accent } else { t.border }))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(title).font(theme::semibold(16.0)).color(t.text));
            if !line.is_empty() {
                ui.label(RichText::new(line).size(13.0).color(t.text_dim));
            }
        });
    let r = ui.interact(inner.response.rect, ui.id().with(("guide-card", title)), egui::Sense::click());
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The guide's window, while open.
pub fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.guide.open {
        return;
    }
    let mut open = true;
    let step = app.guide.step;
    egui::Window::new(tl!("Make a Design"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(260.0, 280.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.set_width(520.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            // sliders a beginner sees are sliders: wide, filled up to the handle
            ui.spacing_mut().slider_width = 220.0;
            ui.visuals_mut().slider_trailing_fill = true;
            // checkboxes and buttons with a visible edge
            ui.visuals_mut().widgets.inactive.bg_fill = t.input;
            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::new(1.0, t.input_border);
            ui.label(RichText::new(format!("{} {} / {}", tl!("Step"), step.index() + 1, Step::ALL.len())).size(12.0).color(t.text_dim));
            match step {
                Step::What => what(app, ui, &t),
                Step::Hoop => hoop(app, ui, &t),
                Step::Details => details(app, ui, &t),
                Step::Fabric => fabric(app, ui, &t),
                Step::Colours => colours(app, ui, &t),
                Step::Watch => watch(app, ui, &t),
                Step::Save => save(app, ui, &t),
            }
            if let Some(e) = &app.guide.error {
                ui.label(RichText::new(e).size(13.0).color(t.error));
            }
            ui.add_space(6.0);
            ui.separator();
            nav(app, ui);
        });
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.guide.open = false;
    }
}

/// Back, and Next where the step needs nothing more.
fn nav(app: &mut VectorcraftApp, ui: &mut Ui) {
    let g = &app.guide;
    let back = match g.step {
        Step::What => None,
        Step::Hoop => Some(Step::What),
        Step::Details if matches!(g.what, Some("sample" | "ith" | "quilt")) => Some(Step::What),
        Step::Details => Some(Step::Hoop),
        Step::Fabric => Some(Step::Details),
        Step::Colours => Some(Step::Fabric),
        Step::Watch => Some(Step::Colours),
        Step::Save => Some(Step::Watch),
    };
    let next = match g.step {
        Step::Colours => Some(Step::Watch),
        Step::Watch => Some(Step::Save),
        _ => None,
    };
    ui.horizontal(|ui| {
        if let Some(b) = back
            && ui.button(RichText::new(tl!("Back")).size(14.0)).clicked()
        {
            app.guide.step = b;
            app.guide.error = None;
        }
        if let Some(n) = next
            && crate::widgets::primary_button(ui, tl!("Next")).clicked()
        {
            app.guide.step = n;
        }
        if app.guide.step == Step::Save && crate::widgets::primary_button(ui, tl!("Done")).clicked() {
            app.guide.open = false;
        }
    });
}

/// Step 1: what to make.
fn what(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(tl!("What would you like to make?")).font(theme::semibold(18.0)).color(t.text));
    let choices = [
        ("words", tl!("A name or some words"), tl!("For towels, bags, pillows and gifts")),
        ("shape", tl!("A shape"), tl!("A heart, a star, a flower and more")),
        ("applique", tl!("An appliqué"), tl!("A shape cut from fabric and sewn down")),
        ("picture", tl!("A picture"), tl!("A photo or a drawing turned into stitches")),
        ("ith", tl!("An in-the-hoop project"), tl!("A little pillow front, a mug rug, a coaster or an ornament")),
        ("quilt", tl!("Quilting for a tile"), tl!("Background quilting the size of a quilting tile, block by block")),
        ("sample", tl!("A sample to try"), tl!("A finished design to sew and learn from")),
    ];
    for (id, title, line) in choices {
        if card(ui, t, title, line, app.guide.what == Some(id)).clicked() {
            app.guide.what = Some(id);
            // a sample brings its hoop, a project picks the one it fits
            app.guide.step = if matches!(id, "sample" | "ith" | "quilt") { Step::Details } else { Step::Hoop };
        }
    }
}

/// Step 2: the hoop, from My Machine's (else the usual ones).
fn hoop(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(tl!("Which hoop will you use?")).font(theme::semibold(18.0)).color(t.text));
    let mine = app.session.execute("embroidery.myMachine", &json!({})).unwrap_or_default();
    if mine["machine"].is_null() {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tl!("Tell us your machine to see its hoops.")).size(13.0).color(t.text_dim));
            if ui.link(tl!("Choose My Machine")).clicked() {
                app.run("ui.myMachine", json!({})).ok();
            }
        });
    }
    ui.label(RichText::new(tl!("Pick the hoop you will put the fabric in. The design is made to fit it.")).size(13.0).color(t.text_dim));
    let hoops = crate::canvas::home_hoops(&mine);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
        for (label, name, w, h) in &hoops {
            if crate::canvas::hoop_card(ui, t, label, (*w, *h)).clicked() {
                app.guide.hoop = Some(name.clone());
                app.guide.step = Step::Details;
            }
        }
    });
}

/// Step 3: the words, the shape, the picture or the sample; then the design is made.
fn details(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    let what = app.guide.what.unwrap_or("words");
    let mut make: Option<Value> = None;
    match what {
        "words" => {
            ui.label(RichText::new(tl!("What should it say?")).font(theme::semibold(18.0)).color(t.text));
            ui.add(egui::TextEdit::singleline(&mut app.guide.text).font(egui::FontId::proportional(20.0)).desired_width(500.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new(tl!("How tall")).size(15.0));
                ui.add(egui::Slider::new(&mut app.guide.height, 8.0..=80.0).suffix(" mm").fixed_decimals(0));
                ui.label(RichText::new(format!("≈ {:.1} in", app.guide.height / 25.4)).size(13.0).color(t.text_dim));
            });
            ui.label(
                RichText::new(tl!("Capital letters are this tall. 20 to 30 mm suits a towel; 10 to 15 mm a shirt.")).size(13.0).color(t.text_dim),
            );
            if crate::widgets::primary_button(ui, tl!("Make It")).clicked() {
                make = Some(json!({ "what": "words", "text": app.guide.text, "height": app.guide.height }));
            }
        }
        "shape" | "applique" => {
            let title = if what == "applique" { tl!("Which shape of fabric?") } else { tl!("Which shape?") };
            ui.label(RichText::new(title).font(theme::semibold(18.0)).color(t.text));
            if what == "applique" {
                ui.label(
                    RichText::new(tl!("The machine sews where the fabric goes and stops; you lay the fabric on, it tacks it down and stops again for you to trim; then it sews the edge.")).size(13.0).color(t.text_dim),
                );
            }
            ui.horizontal_wrapped(|ui| {
                for name in vectorcraft_engine::cmd::embroidery_simple::SIMPLE_SHAPES {
                    let label = crate::simple::shape_name(name);
                    if ui.add(egui::Button::new(RichText::new(label).size(15.0)).min_size(vec2(110.0, 40.0))).clicked() {
                        make = Some(json!({ "what": what, "shape": name }));
                    }
                }
            });
        }
        "picture" => {
            ui.label(RichText::new(tl!("Choose a picture")).font(theme::semibold(18.0)).color(t.text));
            ui.label(
                RichText::new(tl!("Simple pictures with a few flat colours and clear edges work best: a cartoon, a logo, clipart."))
                    .size(13.0)
                    .color(t.text_dim),
            );
            if crate::widgets::primary_button(ui, tl!("Choose a Picture…")).clicked() {
                crate::picks::button(app, |app| {
                    let pick = crate::FilePick { filters: vec![("Pictures", PICTURES)], ..Default::default() };
                    if let Some(path) = crate::picks::open(app, &pick) {
                        app.guide.picture = Some(path.clone());
                        let q = json!({ "what": "picture", "path": path, "hoop": app.guide.hoop });
                        make_it(app, q);
                    }
                });
            }
        }
        "quilt" => {
            ui.label(RichText::new(tl!("What size is the tile?")).font(theme::semibold(18.0)).color(t.text));
            ui.label(
                RichText::new(tl!("Read the size in the tile's window. The block is quilted edge to edge, the size of the tile."))
                    .size(13.0)
                    .color(t.text_dim),
            );
            ui.horizontal(|ui| {
                // (border, corner)
                for (kind, label) in [((false, false), tl!("A block")), ((true, false), tl!("A border strip")), ((false, true), tl!("A corner"))] {
                    let on = (app.guide.border, app.guide.corner) == kind;
                    if ui.selectable_label(on, RichText::new(label).size(15.0)).clicked() && !on {
                        (app.guide.border, app.guide.corner) = kind;
                        // a sensible size to start from
                        app.guide.block = match kind {
                            (true, _) => (7.0, 1.5),
                            (_, true) => (2.0, 2.0),
                            _ => (5.0, 7.0),
                        };
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tl!("Width")).size(15.0));
                ui.add(egui::DragValue::new(&mut app.guide.block.0).range(1.0..=14.0).speed(0.25).suffix(" in"));
                ui.label(RichText::new(tl!("Height")).size(15.0));
                ui.add(egui::DragValue::new(&mut app.guide.block.1).range(1.0..=14.0).speed(0.25).suffix(" in"));
            });
            ui.label(RichText::new(tl!("Pattern")).size(15.0));
            ui.horizontal_wrapped(|ui| {
                for (id, name) in vectorcraft_engine::cmd::embroidery_tiles::PATTERNS {
                    if ui.selectable_label(app.guide.pattern == *id, RichText::new(pattern_name(name)).size(15.0)).clicked() {
                        app.guide.pattern = id;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tl!("Lines apart")).size(15.0));
                ui.add(egui::Slider::new(&mut app.guide.spacing, 3.0..=12.0).suffix(" mm").fixed_decimals(0));
            });
            // something in the middle for the quilting to go round (blocks only)
            if !app.guide.border && !app.guide.corner {
                ui.label(RichText::new(tl!("In the middle (you can leave this empty)")).size(15.0));
                ui.add(egui::TextEdit::singleline(&mut app.guide.middle_text).hint_text(tl!("Words")).desired_width(300.0));
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(app.guide.middle_shape.is_none(), tl!("No shape")).clicked() {
                        app.guide.middle_shape = None;
                    }
                    for name in vectorcraft_engine::cmd::embroidery_simple::SIMPLE_SHAPES {
                        if ui.selectable_label(app.guide.middle_shape == Some(name), crate::simple::shape_name(name)).clicked() {
                            app.guide.middle_shape = Some(name);
                        }
                    }
                });
                if app.guide.middle_shape.is_some() {
                    ui.checkbox(&mut app.guide.middle_applique, tl!("Sew the shape as appliqué"));
                }
            }
            ui.label(
                RichText::new(tl!("Keep the same pattern and spacing for every block of a quilt, so the blocks flow into each other."))
                    .size(13.0)
                    .color(t.text_dim),
            );
            ui.horizontal(|ui| {
                if crate::widgets::primary_button(ui, tl!("Make It")).clicked() {
                    let (w, h) = app.guide.block;
                    let g = &app.guide;
                    let block = !g.border && !g.corner;
                    let text = g.middle_text.trim();
                    make = Some(json!({
                        "what": "quilt", "width": w, "height": h, "pattern": g.pattern, "spacing": g.spacing, "corner": g.corner,
                        "text": (block && !text.is_empty()).then_some(text),
                        "shape": if block { g.middle_shape } else { None },
                        "applique": block && g.middle_applique,
                    }));
                }
                if ui
                    .button(RichText::new(tl!("Make Every Size…")).size(15.0))
                    .on_hover_text(tl!("One file for each usual tile size, in a folder or on the USB stick"))
                    .clicked()
                {
                    crate::picks::button(app, |app| {
                        // the web app has no folders: the files download in one zip
                        let folder = if crate::io::is_web(app) { Some(None) } else { crate::picks::folder(app).map(Some) };
                        if let Some(folder) = folder {
                            let kind = match (app.guide.border, app.guide.corner) {
                                (true, _) => "borders",
                                (_, true) => "corners",
                                _ => "blocks",
                            };
                            let mut q = json!({ "pattern": app.guide.pattern, "spacing": app.guide.spacing, "kind": kind });
                            if let Some(f) = folder {
                                q["folder"] = json!(f);
                            }
                            match app.run("embroidery.quiltBlock.set", q) {
                                Ok(r) => {
                                    let n = r["files"].as_array().map_or(0, Vec::len);
                                    let zip = r["dataBase64"].as_str().and_then(vectorcraft_format::base64_decode);
                                    match (zip, r["name"].as_str()) {
                                        (Some(bytes), Some(name)) => match crate::io::write_to(&mut app.services, name, &bytes) {
                                            Ok(()) => app.status(format!("{} {name}", tl!("Downloaded:"))),
                                            Err(e) => app.guide.error = Some(e),
                                        },
                                        _ => app.status(format!("{} {n}", tl!("Quilt blocks saved:"))),
                                    }
                                }
                                Err(e) => app.guide.error = Some(e),
                            }
                        }
                    });
                }
            });
        }
        "ith" => {
            ui.label(RichText::new(tl!("Which project?")).font(theme::semibold(18.0)).color(t.text));
            let list = app.session.execute("embroidery.ith.projects", &json!({})).unwrap_or_default();
            for pr in list["projects"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let (Some(id), Some(name)) = (pr["id"].as_str(), pr["name"].as_str()) else { continue };
                let line = if pr["panel"] == true {
                    tl!("Sewn in a 5×7 hoop, borders and back sewn on after")
                } else {
                    tl!("Finished in the hoop: batting, backing and a satin edge")
                };
                if card(ui, t, project_name(name), line, app.guide.project.as_deref() == Some(id)).clicked() {
                    app.guide.project = Some(id.to_string());
                }
            }
            ui.label(RichText::new(tl!("Words (you can leave this empty)")).size(14.0));
            ui.add(egui::TextEdit::singleline(&mut app.guide.text).font(egui::FontId::proportional(18.0)).desired_width(500.0));
            ui.label(RichText::new(tl!("A shape")).size(14.0));
            ui.horizontal_wrapped(|ui| {
                if ui.selectable_label(app.guide.ith_shape.is_none(), tl!("None")).clicked() {
                    app.guide.ith_shape = None;
                }
                for name in vectorcraft_engine::cmd::embroidery_simple::SIMPLE_SHAPES {
                    if ui.selectable_label(app.guide.ith_shape == Some(name), crate::simple::shape_name(name)).clicked() {
                        app.guide.ith_shape = Some(name);
                    }
                }
            });
            // the extras (vinyl only on the pieces finished in the hoop)
            let panel = app.guide.project.as_deref().is_some_and(|p| p.starts_with("pillow"));
            ui.checkbox(&mut app.guide.ribbon, if panel { tl!("Add a ribbon band across it") } else { tl!("Add a ribbon loop to hang it") });
            if !panel {
                ui.checkbox(&mut app.guide.vinyl, tl!("Clear vinyl on top (wipe-clean)"));
            }
            ui.checkbox(&mut app.guide.button, tl!("Mark where a button goes"));
            if let Some(project) = app.guide.project.clone()
                && crate::widgets::primary_button(ui, tl!("Make It")).clicked()
            {
                let text = app.guide.text.trim().to_string();
                let g = &app.guide;
                let q = json!({
                    "what": "ith", "project": project, "text": (!text.is_empty()).then_some(text), "shape": g.ith_shape,
                    "ribbon": g.ribbon, "vinyl": g.vinyl && !panel, "button": g.button,
                });
                app.guide.last_ith = Some(q.clone());
                make = Some(q);
            }
        }
        _ => {
            ui.label(RichText::new(tl!("Which sample?")).font(theme::semibold(18.0)).color(t.text));
            let samples = app.session.execute("embroidery.samples", &json!({})).unwrap_or_default();
            for s in samples["samples"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let (Some(id), Some(name)) = (s["id"].as_str(), s["name"].as_str()) else { continue };
                if ui.add(egui::Button::new(RichText::new(crate::canvas::sample_name(name)).size(15.0)).min_size(vec2(220.0, 40.0))).clicked() {
                    make = Some(json!({ "what": "sample", "sample": id }));
                }
            }
        }
    }
    if let Some(mut q) = make {
        if !matches!(what, "sample" | "ith" | "quilt") {
            q["hoop"] = json!(app.guide.hoop);
        }
        make_it(app, q);
    }
}

/// A quilting pattern's name in the UI language.
fn pattern_name(name: &str) -> &str {
    match name {
        "Stipple" => tl!("Stipple"),
        "Swirls" => tl!("Swirls"),
        "Loops" => tl!("Loops"),
        "Feathers" => tl!("Feathers"),
        "Pebbles" => tl!("Pebbles"),
        "Waves" => tl!("Waves"),
        "Squares spiral" => tl!("Squares spiral"),
        "Crosshatch" => tl!("Crosshatch"),
        other => other,
    }
}

/// An in-the-hoop project's name in the UI language.
fn project_name(name: &str) -> &str {
    match name {
        "Little pillow front, 8″ square" => tl!("Little pillow front, 8″ square"),
        "Little pillow front, 9½ × 5½″" => tl!("Little pillow front, 9½ × 5½″"),
        "Mug rug, 6½ × 4½″" => tl!("Mug rug, 6½ × 4½″"),
        "Coaster, 3¾″ square" => tl!("Coaster, 3¾″ square"),
        "Round ornament, 3½″" => tl!("Round ornament, 3½″"),
        other => other,
    }
}

/// Makes the design and moves on to the fabric it goes on.
pub(crate) fn make_it(app: &mut VectorcraftApp, q: Value) {
    match app.run("embroidery.makeDesign", q) {
        Ok(_) => {
            app.run("select.none", json!({})).ok();
            app.run("view.fitArtboard", json!({})).ok();
            app.guide.error = None;
            app.guide.step = Step::Fabric;
        }
        Err(e) => app.guide.error = Some(e),
    }
}

/// A built-in fabric's name in the UI language (one made for the design keeps its own).
pub(crate) fn fabric_name(name: &str) -> &str {
    match name {
        "Woven (twill, poplin)" => tl!("Woven (twill, poplin)"),
        "Pique knit (polo shirt)" => tl!("Pique knit (polo shirt)"),
        "Jersey knit (T-shirt)" => tl!("Jersey knit (T-shirt)"),
        "Fleece / sweatshirt" => tl!("Fleece / sweatshirt"),
        "Towel / terry" => tl!("Towel / terry"),
        "Denim / canvas" => tl!("Denim / canvas"),
        "Leather / vinyl" => tl!("Leather / vinyl"),
        "Structured cap" => tl!("Structured cap"),
        "Silk / lightweight" => tl!("Silk / lightweight"),
        other => other,
    }
}

/// The fabrics to choose from, two to a row, then "not sure": `Some(Some(name))` for a fabric
/// clicked, `Some(None)` for not sure. Shared by the guide and Save for My Machine.
pub(crate) fn fabric_choice(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens, current: Option<&str>) -> Option<Option<String>> {
    ui.label(RichText::new(tl!("What fabric will you sew it on?")).font(theme::semibold(18.0)).color(t.text));
    ui.label(
        RichText::new(tl!("The stitches are made to suit it: a stretchy T-shirt needs more holding than a towel or denim."))
            .size(13.0)
            .color(t.text_dim),
    );
    let info = app.session.execute("embroidery.info", &json!({})).unwrap_or_default();
    let names: Vec<String> =
        info["fabrics"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let mut out = None;
    for pair in names.chunks(2) {
        ui.columns(2, |cols| {
            for (col, name) in cols.iter_mut().zip(pair) {
                if card(col, t, fabric_name(name), "", current == Some(name.as_str())).clicked() {
                    out = Some(Some(name.clone()));
                }
            }
        });
    }
    if card(ui, t, tl!("Another fabric, or not sure: leave it as it is"), "", false).clicked() {
        out = Some(None);
    }
    out
}

/// Step 4: the fabric, which sets the stitches up for it.
fn fabric(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    let current = app.guide.fabric.clone();
    match fabric_choice(app, ui, t, current.as_deref()) {
        Some(Some(name)) => match app.run("embroidery.applyFabric", json!({ "name": name })) {
            Ok(_) => {
                (app.guide.fabric, app.guide.error, app.guide.step) = (Some(name), None, Step::Colours);
            }
            Err(e) => app.guide.error = Some(e),
        },
        Some(None) => (app.guide.error, app.guide.step) = (None, Step::Colours),
        None => {}
    }
}

/// Step 5: the thread colours, each one changeable.
fn colours(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(tl!("Your thread colours")).font(theme::semibold(18.0)).color(t.text));
    ui.label(RichText::new(tl!("These are sewn in this order. Click a colour to match a thread you have.")).size(13.0).color(t.text_dim));
    let info = app.session.execute("embroidery.info", &json!({})).unwrap_or_default();
    let mut change: Option<(String, String)> = None;
    for (i, c) in info["colors"].as_array().map(Vec::as_slice).unwrap_or_default().iter().enumerate() {
        let hex = c["color"].as_str().unwrap_or("#000000").to_string();
        let rgb = vectorcraft_embroidery::sf_core::Rgb::from_hex(&hex).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}.", i + 1)).size(15.0));
            let mut c3 = [rgb.r, rgb.g, rgb.b];
            if ui.color_edit_button_srgb(&mut c3).changed() {
                change = Some((hex.clone(), format!("#{:02x}{:02x}{:02x}", c3[0], c3[1], c3[2])));
            }
            ui.label(RichText::new(c["name"].as_str().unwrap_or(&hex)).size(15.0));
        });
    }
    if let Some((from, to)) = change {
        app.run("embroidery.recolor", json!({ "map": [{ "from": from, "to": to }], "all": true })).ok();
    }
}

/// Step 6: watch it sew.
fn watch(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(tl!("Watch it sew")).font(theme::semibold(18.0)).color(t.text));
    ui.label(RichText::new(tl!("See the needle go stitch by stitch, in the order your machine will sew it.")).size(13.0).color(t.text_dim));
    let info = app.session.execute("embroidery.info", &json!({})).unwrap_or_default();
    if let Some(time) = info["time"].as_str() {
        ui.label(RichText::new(format!("{} {time}", tl!("Sewing time"))).size(15.0));
    }
    if info["fits"].as_bool() == Some(false) {
        ui.label(RichText::new(tl!("The design is bigger than the hoop: make it smaller before you sew.")).size(13.0).color(t.error));
    }
    if crate::widgets::primary_button(ui, tl!("Watch It Sew")).clicked() {
        let p = &mut app.embroidery.player;
        (p.active, p.playing, p.pos) = (true, true, 0.0);
    }
}

/// Step 7: save it for the machine, and as a design to change later.
fn save(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.label(RichText::new(tl!("Save it")).font(theme::semibold(18.0)).color(t.text));
    // an in-the-hoop project's pieces, life size to cut round
    if app.guide.what == Some("ith")
        && let Some(q) = app.guide.last_ith.clone()
        && ui.button(RichText::new(tl!("Print the Cutting Templates…")).size(15.0)).clicked()
    {
        match app.run("embroidery.ith.templates", q) {
            Ok(_) => {
                app.run("file.print", json!({})).ok();
            }
            Err(e) => app.guide.error = Some(e),
        }
    }
    let how = if crate::io::is_web(app) {
        tl!("Save for My Machine downloads the file your machine sews: copy it to its USB stick.")
    } else {
        tl!("Save for My Machine writes the file your machine sews, to its USB stick.")
    };
    ui.label(RichText::new(how).size(13.0).color(t.text_dim));
    if crate::widgets::primary_button(ui, tl!("Save for My Machine")).clicked() {
        app.run("ui.saveForMyMachine", json!({})).ok();
    }
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("Save Design keeps a copy you can open and change later.")).size(13.0).color(t.text_dim));
    if ui.button(RichText::new(tl!("Save Design…")).size(15.0)).clicked() {
        app.run("file.save", json!({})).ok();
    }
}
