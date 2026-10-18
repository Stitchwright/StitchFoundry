//! Design from Prompt: a description, the stitched size and how many thread colours, and the
//! Anthropic API key the request goes out with. Generate sends it and keeps the dialog open,
//! showing that Claude is drawing; the art is placed and digitized when it arrives
//! ([`crate::embroidery::prompt`]). With an `image`, it is Redraw Image with Claude: the
//! description becomes optional notes on what to change, and a line reminds the user to redraw
//! only artwork they may use.
//!
//! Fields: `prompt`, `size` (mm, longest side), `colors`, `remember` (keep the key); a redrawing's
//! `image` (its id). The key itself is not a field: agents can read the fields.

use serde_json::{Value, json};

use super::{DialogSpec, form};
use crate::embroidery::prompt;
use crate::state::Dialog;
use crate::theme::Tokens;
use crate::{VectorcraftApp, widgets};
use vectorcraft_embroidery::sf_core::ai::picture::Provider;

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |d| if redraw(d) { tl!("Redraw Image with Claude").into() } else { tl!("Design from Prompt").into() },
    body,
    confirm,
    ok: Some("Generate"),
    min_width: 420.0,
    max_width: Some(460.0),
    ..DialogSpec::FORM
};

/// Whether the dialog redraws an image (rather than drawing a description).
fn redraw(d: &Dialog) -> bool {
    d.fields.contains_key("image")
}

/// A field's label, `text` already in the UI language.
fn label(ui: &mut egui::Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(text).color(t.text_dim));
}

/// A whole-number field bound to `d.fields[key]`, kept within `range`, with `suffix` (a unit).
fn number(ui: &mut egui::Ui, d: &mut Dialog, key: &str, range: (f64, f64), suffix: &str) {
    let v = d.f64(key, range.0);
    if let Some(n) = widgets::plain_field(ui, ("design-prompt", key), v, suffix, 0, 72.0) {
        d.fields.insert(key.into(), json!(n.round().clamp(range.0, range.1)));
    }
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let t = Tokens::get(ui.ctx());
    let now = ui.input(|i| i.time);
    app.embroidery.prompt.now = now;
    crate::embroidery::account::load(app);
    let credits = crate::embroidery::account::paying_with_credits(app);
    let drawing = app.embroidery.prompt.drawing_for(now);
    let redraw = redraw(d);
    label(ui, if redraw { tl!("What to change (optional):") } else { tl!("Describe the design:") });
    form::text_area(ui, d, "prompt", 400.0, if redraw { 2 } else { 4 });
    ui.add_space(8.0);
    egui::Grid::new("design-prompt-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
        label(ui, tl!("Size (longest side):"));
        number(ui, d, "size", (20.0, 400.0), " mm");
        ui.end_row();
        label(ui, tl!("Thread colors"));
        number(ui, d, "colors", (1.0, 12.0), "");
        ui.end_row();
        if !redraw {
            // who makes the picture: Claude draws the shapes itself; another service paints a
            // picture, which Claude then redraws as shapes
            label(ui, tl!("Picture made by"));
            let mut options: Vec<&str> = vec![tl!("Claude (draws the shapes itself)")];
            options.extend(Provider::ALL.iter().map(|p| p.name()));
            let cur = Provider::from_id(&d.str("maker")).and_then(|p| Provider::ALL.iter().position(|x| *x == p)).map_or(0, |i| i + 1);
            if let Some(i) = widgets::dropdown_names(ui, "design-prompt-maker", options.get(cur).copied().unwrap_or_default(), &options, 260.0) {
                let id = i.checked_sub(1).and_then(|k| Provider::ALL.get(k)).map_or("claude", |p| p.id());
                d.fields.insert("maker".into(), json!(id));
            }
            ui.end_row();
            if let Some(p) = Provider::from_id(&d.str("maker")) {
                label(ui, &crate::i18n::fmt(tl!("{name} API key:"), &[("name", p.name())]));
                let mut get_key = false;
                ui.horizontal(|ui| {
                    let key = app.embroidery.prompt.picture_keys.entry(p.id()).or_default();
                    ui.add(egui::TextEdit::singleline(key).password(true).desired_width(200.0));
                    get_key = ui.link(egui::RichText::new(tl!("Get a key")).size(11.0)).clicked();
                });
                if get_key {
                    app.open_url(p.key_url());
                }
                ui.end_row();
                label(ui, tl!("Model"));
                let mut model = d.str("model");
                if ui.add(egui::TextEdit::singleline(&mut model).desired_width(260.0).hint_text(p.default_model())).changed() {
                    d.fields.insert("model".into(), json!(model));
                }
                ui.end_row();
            }
        }
        if !credits {
            label(ui, tl!("Anthropic API key:"));
            ui.add(egui::TextEdit::singleline(&mut app.embroidery.prompt.key).password(true).desired_width(260.0).hint_text("sk-ant-…"));
            ui.end_row();
            ui.label("");
            form::check(ui, d, "remember", tl!("Remember the key on this computer"));
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    if redraw {
        ui.label(egui::RichText::new(tl!("Only redraw artwork you own or have permission to use.")).size(11.0).color(t.text));
    } else if Provider::from_id(&d.str("maker")).is_some() {
        ui.label(
            egui::RichText::new(tl!("The picture is billed to your account with that service. Claude then redraws it as flat shapes in your thread colors, so it sews with few thread changes."))
                .size(11.0)
                .color(t.text_dim),
        );
    }
    // paid with StitchFoundry credits, or with the user's own key
    if !crate::embroidery::account::paid_line(app, ui, &t) {
        ui.horizontal_wrapped(|ui| {
        let about = if redraw {
            tl!("Claude redraws the image as flat-colour art with your own Anthropic API key, billed to your Anthropic account; StitchFoundry then digitizes it. Check the stitches before you sew.")
        } else {
            tl!("Claude draws the artwork with your own Anthropic API key, billed to your Anthropic account; StitchFoundry then digitizes it. Check the stitches before you sew.")
        };
        ui.label(egui::RichText::new(about).size(11.0).color(t.text_dim));
        if ui.link(egui::RichText::new(tl!("Get a key")).size(11.0)).clicked() {
            app.open_url(prompt::KEY_URL);
        }
    });
    }
    ui.add_space(8.0);
    if let Some(secs) = drawing {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            let what = if app.embroidery.prompt.picturing() { tl!("Making the picture…") } else { tl!("Drawing the design…") };
            ui.label(format!("{} {:.0} s", what, secs));
        });
    } else if let Some(e) = app.embroidery.prompt.error.clone() {
        ui.label(egui::RichText::new(crate::i18n::msg(&e)).color(t.error));
    }
    false
}

fn confirm(app: &mut VectorcraftApp, d: &Dialog) -> Result<Value, String> {
    prompt::start(app, d)
}
