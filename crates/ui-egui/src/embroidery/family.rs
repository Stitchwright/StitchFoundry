//! Family Tree… (HV-01): the window that makes a family chart (`embroidery.familyTree`): the
//! names typed as an outline, one person a line, each generation indented under the one before,
//! and the chart's lettering, frames and connectors chosen. The outline is kept on this computer
//! for next time, as mySewnet's wizard keeps the family it was given.

use egui::RichText;
use serde_json::{Value, json};

use super::account;
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The command (Embroidery › Create › Family Tree…).
pub const COMMAND: &str = "ui.familyTree";
/// The name the outline is kept under.
const KEPT: &str = "family-tree";
const EXAMPLE: &str = "John Green + Mary Green\n  Anna Green + Tom Hill\n    Lucy Hill\n    Sam Hill\n  Peter Green\n";

/// The window between frames.
pub struct FamilyState {
    pub open: bool,
    loaded: bool,
    pub people: String,
    pub ancestors: bool,
    pub given_only: bool,
    pub height: f64,
    /// none, box, rounded or oval.
    pub frame: String,
    /// run, satin or none.
    pub connectors: String,
    pub double: bool,
    pub error: Option<String>,
    /// The last chart made: `embroidery.familyTree`'s answer.
    pub made: Option<Value>,
}

impl Default for FamilyState {
    fn default() -> Self {
        Self {
            open: false,
            loaded: false,
            people: String::new(),
            ancestors: false,
            given_only: false,
            height: 7.0,
            frame: "none".into(),
            connectors: "run".into(),
            double: false,
            error: None,
            made: None,
        }
    }
}

fn load(app: &mut VectorcraftApp) {
    if app.embroidery.family.loaded {
        return;
    }
    app.embroidery.family.loaded = true;
    let kept = app.services.secrets.as_ref().and_then(|s| s.load(KEPT));
    app.embroidery.family.people = kept.unwrap_or_else(|| EXAMPLE.to_string());
}

/// `ui.familyTree {people?, chart?, names?, height?, frame?, connectors?, double?, make?: bool}`:
/// opens the window with what is given; `make` makes the chart at once.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    load(app);
    let st = &mut app.embroidery.family;
    st.open = true;
    if let Some(t) = p.get("people").and_then(Value::as_str) {
        st.people = t.to_string();
    }
    if let Some(c) = p.get("chart").and_then(Value::as_str) {
        st.ancestors = match c {
            "ancestors" => true,
            "descendants" => false,
            _ => return Err("`chart` is descendants or ancestors".into()),
        };
    }
    if let Some(n) = p.get("names").and_then(Value::as_str) {
        st.given_only = n == "given";
    }
    if let Some(h) = p.get("height").and_then(Value::as_f64) {
        st.height = h;
    }
    if let Some(f) = p.get("frame").and_then(Value::as_str) {
        st.frame = f.to_string();
    }
    if let Some(c) = p.get("connectors").and_then(Value::as_str) {
        st.connectors = c.to_string();
    }
    if let Some(d) = p.get("double").and_then(Value::as_bool) {
        st.double = d;
    }
    if p.get("make").and_then(Value::as_bool) == Some(true) {
        make(app)?;
    }
    let st = &app.embroidery.family;
    Ok(json!({ "open": true, "people": st.people, "chart": if st.ancestors { "ancestors" } else { "descendants" }, "made": st.made, "error": st.error }))
}

/// Make the chart in the open design (a new one in the hoop of My Machine when none is open).
fn make(app: &mut VectorcraftApp) -> Result<(), String> {
    let st = &app.embroidery.family;
    let q = json!({
        "people": st.people,
        "chart": if st.ancestors { "ancestors" } else { "descendants" },
        "names": if st.given_only { "given" } else { "full" },
        "height": st.height,
        "frame": st.frame,
        "connectors": st.connectors,
        "double": st.double,
    });
    let kept = st.people.clone();
    account::keep(app, KEPT, Some(kept.as_str()));
    if app.session.active().is_none() {
        if let Err(e) = app.run("embroidery.newDesign", json!({})) {
            app.embroidery.family.error = Some(e.clone());
            return Err(e);
        }
    }
    match app.run("embroidery.familyTree", q) {
        Ok(r) => {
            let fits = r["fits"].as_bool();
            app.embroidery.family.error = None;
            app.embroidery.family.made = Some(r);
            if fits == Some(false) {
                app.status(tl!("The family tree is bigger than the hoop: choose a bigger hoop, or fewer generations."));
            }
            Ok(())
        }
        Err(e) => {
            app.embroidery.family.error = Some(e.clone());
            Err(e)
        }
    }
}

/// The Family Tree window, while open.
pub fn window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.embroidery.family.open {
        return;
    }
    let t = Tokens::get(ctx);
    let mut open = true;
    let mut go = false;
    egui::Window::new(tl!("Family Tree"))
        .id(egui::Id::new("family-tree"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(240.0, 240.0))
        .show(ctx, |ui| {
            ui.set_width(480.0);
            let st = &mut app.embroidery.family;
            ui.horizontal(|ui| {
                ui.radio_value(&mut st.ancestors, false, tl!("Descendants"));
                ui.radio_value(&mut st.ancestors, true, tl!("Ancestors"));
            });
            let help = if st.ancestors {
                tl!("One person a line. Start with the person, then their father and mother indented under them, and each one's parents under them.")
            } else {
                tl!("One person a line. Start with the first person (\"Name + Partner\" for a couple), then their children indented under them, and their children under each.")
            };
            ui.label(RichText::new(help).size(12.0).color(t.text_dim));
            ui.add(egui::TextEdit::multiline(&mut st.people).font(egui::TextStyle::Monospace).desired_rows(10).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(tl!("Names"));
                ui.radio_value(&mut st.given_only, false, tl!("Full names"));
                ui.radio_value(&mut st.given_only, true, tl!("Given names only"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Letter height"));
                ui.add(egui::DragValue::new(&mut st.height).range(4.0..=30.0).speed(0.2).suffix(" mm"));
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Frame"));
                for (id, name) in [("none", tl!("None")), ("box", tl!("Box")), ("rounded", tl!("Rounded")), ("oval", tl!("Oval"))] {
                    ui.radio_value(&mut st.frame, id.to_string(), name);
                }
            });
            ui.horizontal(|ui| {
                ui.label(tl!("Connectors"));
                for (id, name) in [("run", tl!("Running stitch")), ("satin", tl!("Satin")), ("none", tl!("None"))] {
                    ui.radio_value(&mut st.connectors, id.to_string(), name);
                }
                ui.add_enabled(st.connectors != "none", egui::Checkbox::new(&mut st.double, tl!("Double")));
            });
            ui.add_space(6.0);
            go = ui.button(tl!("Make the Family Tree")).clicked();
            if let Some(r) = &st.made {
                let size = format!("{} × {} mm", r["widthMm"], r["chartHeightMm"]);
                ui.label(crate::i18n::fmt(tl!("{n} people, {size}"), &[("n", &r["people"].to_string()), ("size", &size)]));
            }
            if let Some(e) = &st.error {
                ui.label(RichText::new(crate::i18n::msg(e)).color(t.error));
            }
        });
    if go {
        let _ = make(app); // (the error is shown in the window)
    }
    if !open {
        app.embroidery.family.open = false;
    }
}
