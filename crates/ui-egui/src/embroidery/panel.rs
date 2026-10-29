//! Embroidery panel: stitch type and parameters of the selected objects, thread,
//! stitch direction (automatic or edited inclination lines), start/stop beads,
//! design information, hoop, fabric and how stitches are shown.

use egui::{Color32, Ui};
use serde_json::{Value, json};
use vectorcraft_embroidery::sf_core::attr::{Embroidery, FontDesign, LetterAdjust, Lettering, StitchType};
use vectorcraft_embroidery::sf_core::brush::WidthProfile;
use vectorcraft_embroidery::sf_core::model::{
    AppliqueParams, Corners, CoverKind, CrossStyle, CustomMotif, FillKind, FillParams, JaggedSides, MAX_PASSES, Motif, MotifColumn, MotifWarp,
    RunKind, RunParams, SatinParams, TATAMI_LENGTH, TackdownKind, TexturePattern, TieStyle, WarpKind,
};
use vectorcraft_embroidery::sf_core::threads::Rgb;

use super::{ArtView, ColourPreview, CutoutPreview, StitchView};
use crate::VectorcraftApp;
use crate::theme::Tokens;
use crate::widgets::{dropdown, flat_button, label_row, plain_field, section_header};

/// A flat button sized to its (translated) label.
pub(crate) fn button(ui: &mut Ui, text: &str) -> egui::Response {
    let w = ui.painter().layout_no_wrap(tl!(text).to_string(), egui::FontId::proportional(12.5), egui::Color32::WHITE).size().x;
    flat_button(ui, text, (w + 20.0).min(ui.available_width().max(40.0)))
}

pub const ID: &str = "embroidery";

const LABEL_W: f32 = 112.0;
const FIELD_W: f32 = 74.0;

/// Stitch type menu: (label, `embroidery.set` type).
const TYPES: &[(&str, &str)] = &[
    ("No Stitches", "none"),
    ("Run", "run"),
    ("Bean Run", "bean"),
    ("Backstitch", "backstitch"),
    ("Stem Stitch", "stemstitch"),
    ("Blanket", "blanket"),
    ("Redwork", "redwork"),
    ("Satin", "satin"),
    ("Tatami Fill", "fill"),
    ("Satin Fill", "satinfill"),
    ("Contour Fill", "contour"),
    ("Spiral Fill", "spiral"),
    ("Radial Fill", "radial"),
    ("Cross Fill", "cross"),
    ("Meander Fill", "meander"),
    ("Motif Fill", "motiffill"),
    ("Appliqué", "applique"),
    ("Manual Stitches", "manual"),
    ("Sequins", "sequin"),
    ("Beads", "bead"),
    ("Rhinestones", "stone"),
    ("Cord", "cord"),
    ("Boring", "boring"),
    ("Print", "print"),
    ("Foil", "foil"),
];

fn type_index(e: Option<&Embroidery>) -> usize {
    let key = match e.map(|e| &e.stitch) {
        None | Some(StitchType::Branch | StitchType::Lettering(_)) => "none",
        Some(st) => st.type_name(),
    };
    TYPES.iter().position(|t| t.1 == key).unwrap_or(0)
}

/// The first selected object that can be stitched (groups: their first path; art in a redwork
/// group: the group), its id and embroidery record.
fn first_stitchable(app: &VectorcraftApp) -> Option<(u64, Option<Embroidery>, bool)> {
    let st = app.session.active()?;
    let d = &st.doc;
    fn find(d: &vectorcraft_doc::Document, id: vectorcraft_doc::NodeId) -> Option<vectorcraft_doc::NodeId> {
        let n = d.node(id)?;
        if vectorcraft_embroidery::stitchable(n) || vectorcraft_embroidery::sews_whole(n) {
            return Some(id);
        }
        n.children()?.iter().find_map(|c| find(d, c.id))
    }
    // the outermost redwork group holding `id`, else `id`
    let owner = |id: vectorcraft_doc::NodeId| {
        let mut out = id;
        let mut at = d.parent_of(id);
        while let Some(p) = at {
            if d.node(p).is_some_and(vectorcraft_embroidery::sews_whole) {
                out = p;
            }
            at = d.parent_of(p);
        }
        out
    };
    st.selection.objects.iter().find_map(|id| find(d, *id)).map(owner).and_then(|id| {
        let n = d.node(id)?;
        Some((id.0, n.embroidery.as_deref().cloned(), vectorcraft_embroidery::is_line_art(n)))
    })
}

fn num(ui: &mut Ui, label: &str, id: &str, value: f64, suffix: &str, decimals: usize) -> Option<f64> {
    let mut out = None;
    label_row(ui, label, LABEL_W, |ui| {
        out = plain_field(ui, ("emb", id), value, suffix, decimals, FIELD_W);
    });
    out
}

fn check(ui: &mut Ui, label: &str, value: bool) -> Option<bool> {
    let mut v = value;
    ui.checkbox(&mut v, label).changed().then_some(v)
}

fn pick(ui: &mut Ui, label: &str, id: &str, current: usize, options: &[&str]) -> Option<usize> {
    let mut out = None;
    label_row(ui, label, LABEL_W, |ui| {
        let cur = options.get(current).copied().unwrap_or_default();
        out = dropdown(ui, ("emb", id), cur, options, 130.0).filter(|i| *i != current);
    });
    out
}

/// [`pick`] for names that are data (styles, fabrics, hoops): shown as they are, never
/// translated.
pub(crate) fn pick_names(ui: &mut Ui, label: &str, id: &str, current: usize, options: &[&str]) -> Option<usize> {
    let mut out = None;
    label_row(ui, label, LABEL_W, |ui| {
        let cur = options.get(current).copied().unwrap_or_default();
        out = crate::widgets::dropdown_names(ui, ("emb", id), cur, options, 130.0).filter(|i| *i != current);
    });
    out
}

/// Whose stitch properties the rows edit: the selected objects (the first one's id), or
/// those new objects of a stitch type take.
#[derive(Clone, Copy)]
enum Target<'a> {
    Selection(u64),
    Current(&'a str),
}

/// Sends a parameter change for the target.
fn set(app: &mut VectorcraftApp, target: Target, params: Value) {
    match target {
        Target::Selection(_) => {
            app.run("embroidery.setParams", json!({ "params": params })).ok();
        }
        Target::Current(ty) => {
            // a run style change files the properties under that style's type
            if let Ok(r) = app.run("embroidery.current.setParams", json!({ "type": ty, "params": params }))
                && let Some(t) = r["type"].as_str()
            {
                app.embroidery.new_type = t.to_string();
            }
        }
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if app.session.active().is_none() {
        crate::panels::empty_state(ui, "waves", tl!("No document"), tl!("Open or create a document to digitize embroidery."));
        templates_body(app, ui);
        egui::CollapsingHeader::new(tl!("Patches")).id_salt("emb-patches").show(ui, |ui| patches_body(app, ui));
        return;
    }
    // the sections shown follow Window › Experience (a font document always has its Font Creator)
    let level = app.ui.experience;
    if !level.advanced() {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(tl!("Showing the basics.")).color(t.text_dim));
            if ui.link(tl!("Show everything")).clicked() {
                app.run("window.experience", json!({ "level": "all" })).ok();
            }
        });
    }
    digitize_row(app, ui);
    if level.advanced() || font_design(app).is_some() {
        font_rows(app, ui);
    }
    lettering_rows(app, ui);
    if level.advanced() {
        team_name_rows(app, ui);
    }
    monogram_rows(app, ui);
    egui::CollapsingHeader::new(tl!("Patches")).id_salt("emb-patches").show(ui, |ui| patches_body(app, ui));
    egui::CollapsingHeader::new(tl!("Design Templates")).id_salt("emb-templates").show(ui, |ui| templates_body(app, ui));
    egui::CollapsingHeader::new(tl!("Design Library")).id_salt("emb-library").show(ui, |ui| library_body(app, ui));
    egui::CollapsingHeader::new(tl!("Clipart")).id_salt("emb-clipart").show(ui, |ui| clipart_body(app, ui));
    if level.advanced() {
        buttonhole_rows(app, ui);
        quilting_rows(app, ui);
    }
    egui::CollapsingHeader::new(tl!("Send to Machine")).id_salt("emb-send").show(ui, |ui| send_body(app, ui));
    if level.business() {
        egui::CollapsingHeader::new(tl!("Batch Output")).id_salt("emb-output").show(ui, |ui| output_body(app, ui));
    }
    if level.advanced() {
        egui::CollapsingHeader::new(tl!("Exports")).id_salt("emb-exports").show(ui, |ui| exports_body(app, ui));
    }
    if level.business() {
        egui::CollapsingHeader::new(tl!("Job Orders")).id_salt("emb-jobs").show(ui, |ui| jobs_body(app, ui));
    }
    ui.add_space(6.0);
    object_section(app, ui);
    ui.add_space(6.0);
    design_section(app, ui, &t);
}

/// Lettering with embroidery fonts (Ink/Stitch's format): load a font folder, pick a font, type,
/// place the lettering in the middle of the first artboard.
fn lettering_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tl!("Embroidery Lettering")).id_salt("emb-lettering").show(ui, |ui| lettering_body(app, ui));
}

/// The appliqué fabric's look on the canvas: its colour, pattern and second colour.
fn applique_fabric_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, a: &vectorcraft_embroidery::sf_core::model::AppliqueParams) {
    use vectorcraft_embroidery::sf_core::model::FabricPattern;
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    ui.horizontal(|ui| {
        ui.label(tl!("Fabric colour"));
        let mut c = [a.fabric_color.r, a.fabric_color.g, a.fabric_color.b];
        if ui.color_edit_button_srgb(&mut c).changed() {
            set(app, target, json!({ "fabric_color": hex(c) }));
        }
    });
    let names = [tl!("Plain"), tl!("Stripes"), tl!("Check"), tl!("Dots"), tl!("Denim"), tl!("Felt")];
    let cur = FabricPattern::ALL.iter().position(|k| *k == a.fabric_pattern).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Fabric pattern"), "fabpat", cur, &names) {
        let k = FabricPattern::ALL.get(i).copied().unwrap_or_default();
        set(app, target, json!({ "fabric_pattern": k }));
    }
    if a.fabric_pattern != FabricPattern::Plain {
        ui.horizontal(|ui| {
            ui.label(tl!("Pattern colour"));
            let mut c = [a.fabric_color_2.r, a.fabric_color_2.g, a.fabric_color_2.b];
            if ui.color_edit_button_srgb(&mut c).changed() {
                set(app, target, json!({ "fabric_color_2": hex(c) }));
            }
        });
    }
}

/// A decoration's settings: where the pieces go, their size, shape and colour, how sequins are
/// held, and whether the machine stops first.
fn decoration_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, d: &vectorcraft_embroidery::sf_core::decor::DecorParams) {
    use vectorcraft_embroidery::sf_core::decor::{DecorKind, DecorLayout, DecorPacking, DecorShape, Fixing};
    let kind = d.kind;
    let pieces = matches!(kind, DecorKind::Sequin | DecorKind::Bead | DecorKind::Stone);
    if pieces {
        let layouts = [DecorLayout::Run, DecorLayout::Fill, DecorLayout::Border, DecorLayout::Manual];
        let names = [tl!("Along the line"), tl!("Fill the shape"), tl!("Round the edge"), tl!("At each point")];
        let cur = layouts.iter().position(|l| *l == d.layout).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Place"), "decor-layout", cur, &names)
            && let Some(l) = layouts.get(i)
        {
            set(app, target, json!({ "layout": l }));
        }
    }
    if matches!(kind, DecorKind::Sequin | DecorKind::Stone) {
        let shapes = [DecorShape::Round, DecorShape::Square, DecorShape::Star, DecorShape::Heart, DecorShape::Flower];
        let names = [tl!("Round"), tl!("Square"), tl!("Star"), tl!("Heart"), tl!("Flower")];
        let n = if kind == DecorKind::Stone { 2 } else { shapes.len() };
        let cur = shapes.iter().position(|x| *x == d.shape).unwrap_or(0).min(n - 1);
        if let Some(i) = pick(ui, tl!("Shape"), "decor-shape", cur, names.get(..n).unwrap_or(&names))
            && let Some(x) = shapes.get(i)
        {
            set(app, target, json!({ "shape": x }));
        }
    }
    let size_label = match kind {
        DecorKind::Bead => tl!("Bead length"),
        DecorKind::Cord => tl!("Cord width"),
        DecorKind::Boring => tl!("Edge width"),
        _ => tl!("Size"),
    };
    if !kind.is_virtual()
        && let Some(v) = num(ui, size_label, "decor-size", d.size, " mm", 1)
    {
        set(app, target, json!({ "size": v.clamp(0.5, 30.0) }));
    }
    if pieces {
        if let Some(v) = num(ui, tl!("Gap"), "decor-gap", d.spacing, " mm", 1) {
            set(app, target, json!({ "spacing": v.clamp(-15.0, 50.0) }));
        }
        if d.layout == DecorLayout::Fill {
            let packs = [DecorPacking::Hex, DecorPacking::Grid];
            let names = [tl!("Honeycomb"), tl!("Grid")];
            let cur = packs.iter().position(|x| *x == d.packing).unwrap_or(0);
            if let Some(i) = pick(ui, tl!("Packing"), "decor-pack", cur, &names)
                && let Some(x) = packs.get(i)
            {
                set(app, target, json!({ "packing": x }));
            }
            if let Some(v) = num(ui, tl!("Angle"), "decor-angle", d.angle, "°", 0) {
                set(app, target, json!({ "angle": v.clamp(-360.0, 360.0) }));
            }
        }
    }
    if kind == DecorKind::Sequin {
        let fixings = [Fixing::Single, Fixing::Double, Fixing::Tri, Fixing::Cross];
        let names = [tl!("Single"), tl!("Double"), tl!("Three"), tl!("Cross")];
        let cur = fixings.iter().position(|x| *x == d.fixing).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Fixing"), "decor-fix", cur, &names)
            && let Some(x) = fixings.get(i)
        {
            set(app, target, json!({ "fixing": x }));
        }
        if let Some(v) = check(ui, tl!("Flip every other"), d.flip) {
            set(app, target, json!({ "flip": v }));
        }
    }
    if kind != DecorKind::Boring {
        ui.horizontal(|ui| {
            ui.label(tl!("Colour"));
            let c = d.color.unwrap_or(vectorcraft_embroidery::sf_core::Rgb::new(0xc0, 0xc0, 0xc0));
            let mut rgb = [c.r, c.g, c.b];
            if ui.color_edit_button_srgb(&mut rgb).changed() {
                set(app, target, json!({ "color": format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]) }));
            }
            if d.color.is_some() && ui.small_button(tl!("Thread colour")).clicked() {
                set(app, target, json!({ "color": null }));
            }
        });
    }
    if kind.is_virtual() {
        ui.label(egui::RichText::new(tl!("Shown with the design, not sewn")).small());
    } else if kind != DecorKind::Stone {
        if let Some(v) = check(ui, tl!("Stop before"), d.stop) {
            set(app, target, json!({ "stop": v }));
        }
    } else {
        ui.label(egui::RichText::new(tl!("Rhinestones aren't sewn: export their template to cut and set them")).small());
    }
}

/// The selected lettering set in an embroidery font (or the one holding what is selected), and
/// what it says.
fn selected_lettering(app: &VectorcraftApp) -> Option<(u64, Lettering)> {
    let st = app.session.active()?;
    let d = &st.doc;
    st.selection.objects.iter().find_map(|id| {
        let mut at = Some(*id);
        while let Some(c) = at {
            if let Some(StitchType::Lettering(l)) = d.node(c)?.embroidery.as_deref().map(|e| &e.stitch) {
                return Some((c.0, (**l).clone()));
            }
            at = d.parent_of(c);
        }
        None
    })
}

/// Fills the lettering rows from the selected lettering `selected` the first time it is
/// selected, and forgets which lettering they were filled from when none is.
fn fill_from_lettering(app: &mut VectorcraftApp, selected: Option<&(u64, Lettering)>) {
    match selected {
        Some((id, l)) if app.embroidery.lettering_for != Some(*id) => {
            app.embroidery.lettering_for = Some(*id);
            app.embroidery.lettering_text = l.text.clone();
            app.embroidery.lettering_font = l.font.clone();
            app.embroidery.lettering_size = l.size;
            app.embroidery.lettering_arc = l.arc;
            app.embroidery.lettering_spiral = l.spiral;
            app.embroidery.lettering_sew = l.sew;
            app.embroidery.lettering_puff = l.puff.is_some();
            app.embroidery.lettering_applique = l.applique.map(|c| [c.r, c.g, c.b]);
            app.embroidery.lettering_stitch = l.stitch;
            app.embroidery.lettering_vertical = l.vertical;
            app.embroidery.lettering_small = l.small;
        }
        None => app.embroidery.lettering_for = None,
        _ => {}
    }
}

/// The buttons that load an embroidery font from a folder or get one from a font library.
fn font_buttons(app: &mut VectorcraftApp, ui: &mut Ui) {
    if app.services.pick_folder.is_some()
        && button(ui, tl!("Load Embroidery Font…")).on_hover_text(tl!("A font folder in Ink/Stitch's format: font.json and its glyphs")).clicked()
    {
        crate::picks::button(app, |app| {
            let Some(dir) = crate::picks::folder(app) else { return };
            match app.run("embroidery.font.load", json!({ "path": dir })) {
                Ok(r) => {
                    if let Some(name) = r["name"].as_str() {
                        app.embroidery.lettering_font = name.to_string();
                    }
                }
                Err(e) => app.status(e),
            }
        });
    }
    if crate::dialogs::font_library::available(app)
        && button(ui, tl!("Get Embroidery Fonts…"))
            .on_hover_text(tl!("Download fonts from an embroidery font library, ready for lettering"))
            .clicked()
    {
        app.run("ui.fontLibrary", json!({})).ok();
    }
}

/// The lettering's sew order, how its letters join and where the thread is cut.
fn sew_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    use vectorcraft_embroidery::sf_core::attr::{LetterJoin, LetterOrder, LetterTrim};
    let sew = &mut app.embroidery.lettering_sew;
    let orders = [LetterOrder::Left, LetterOrder::Right, LetterOrder::Centre];
    let cur = orders.iter().position(|o| *o == sew.order).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Sew order"), "let-order", cur, &[tl!("Left to right"), tl!("Right to left"), tl!("Centre out")])
        && let Some(o) = orders.get(i)
    {
        sew.order = *o;
    }
    let joins = [LetterJoin::Closest, LetterJoin::Baseline];
    let cur = joins.iter().position(|j| *j == sew.join).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Join"), "let-join", cur, &[tl!("Closest"), tl!("Along the baseline")])
        && let Some(j) = joins.get(i)
    {
        sew.join = *j;
    }
    let trims = [LetterTrim::Auto, LetterTrim::Letter, LetterTrim::Word, LetterTrim::Line];
    let cur = trims.iter().position(|t| *t == sew.trim).unwrap_or(0);
    if let Some(i) =
        pick(ui, tl!("Trims"), "let-trim", cur, &[tl!("Where needed"), tl!("After each letter"), tl!("After each word"), tl!("After each line")])
        && let Some(t) = trims.get(i)
    {
        sew.trim = *t;
    }
}

/// The Embroidery Lettering section's rows: new lettering, or the selected lettering to change.
pub(crate) fn lettering_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    // the font folders remembered from earlier sessions, loaded the first time
    if !std::mem::replace(&mut app.embroidery.fonts_listed, true)
        && let Ok(r) = app.run("embroidery.fonts", json!({}))
        && let Some(w) = r["warnings"].as_array().and_then(|w| w.first()).and_then(Value::as_str)
    {
        app.status(w);
    }
    // a lettering selected: what it says fills the rows
    let selected = selected_lettering(app);
    fill_from_lettering(app, selected.as_ref());
    font_buttons(app, ui);
    let fonts: Vec<(String, Option<f64>, Option<f64>, f64)> = app
        .session
        .embroidery_fonts
        .iter()
        .map(|f| {
            let i = &f.info;
            let size = |k: f64| (i.size > 0.0 && k > 0.0).then_some(i.size * k);
            (i.name.clone(), size(i.min_scale), size(i.max_scale), i.size)
        })
        .collect();
    if fonts.is_empty() {
        ui.label(tl!("Load an embroidery font to set text with it."));
        return;
    }
    if let Some((_, l)) = &selected
        && !fonts.iter().any(|f| f.0.eq_ignore_ascii_case(&l.font))
    {
        ui.label(format!("{} {}", tl!("Load this lettering's font to change it:"), l.font));
    }
    let names: Vec<&str> = fonts.iter().map(|f| f.0.as_str()).collect();
    let cur = names.iter().position(|n| n.eq_ignore_ascii_case(&app.embroidery.lettering_font)).unwrap_or(0);
    if let Some(i) = pick_names(ui, tl!("Font"), "let-font", cur, &names)
        && let Some(name) = names.get(i)
    {
        app.embroidery.lettering_font = name.to_string();
    }
    let Some((name, lo, hi, own)) = fonts.get(cur).cloned() else { return };
    if button(ui, tl!("Forget Font")).on_hover_text(tl!("Unload the font and stop loading its folder")).clicked() {
        app.run("embroidery.font.forget", json!({ "name": name })).ok();
        return;
    }
    ui.add(egui::TextEdit::multiline(&mut app.embroidery.lettering_text).desired_rows(2).hint_text(tl!("Text")));
    let size = if app.embroidery.lettering_size > 0.0 { app.embroidery.lettering_size } else { own.max(1.0) };
    if let Some(v) = num(ui, tl!("Size"), "let-size", size, " mm", 1) {
        app.embroidery.lettering_size = v.clamp(1.0, 500.0);
    }
    // the sizes the font sews well at: a warning as soon as the size is outside them
    if let (Some(lo), Some(hi)) = (lo, hi) {
        let said = format!("{} {lo:.1}–{hi:.1} mm", tl!("Sews well at"));
        let size = if app.embroidery.lettering_size > 0.0 { app.embroidery.lettering_size } else { size };
        if size < lo - 1e-6 || size > hi + 1e-6 {
            let t = Tokens::get(ui.ctx());
            ui.colored_label(t.warning, format!("▲ {said}"))
                .on_hover_text(tl!("Outside the sizes the font was made for: its stitches may come out too dense or too long"));
        } else {
            ui.label(said);
        }
    }
    // the baseline: straight, or round a circle, arched over its top or along its bottom
    let arc = app.embroidery.lettering_arc;
    let kinds = [tl!("Straight"), tl!("Arch"), tl!("Valley"), tl!("Vertical")];
    let cur = if arc > 0.0 {
        1
    } else if arc < 0.0 {
        2
    } else if app.embroidery.lettering_vertical {
        3
    } else {
        0
    };
    if let Some(i) = pick(ui, tl!("Baseline"), "let-arc", cur, &kinds) {
        let r = if arc != 0.0 { arc.abs() } else { 60.0 };
        app.embroidery.lettering_arc = [0.0, r, -r, 0.0].get(i).copied().unwrap_or(0.0);
        app.embroidery.lettering_vertical = i == 3;
    }
    if app.embroidery.lettering_arc != 0.0
        && let Some(v) = num(ui, tl!("Radius"), "let-radius", app.embroidery.lettering_arc.abs(), " mm", 0)
    {
        app.embroidery.lettering_arc = v.clamp(1.0, 10000.0).copysign(app.embroidery.lettering_arc);
    }
    // round the circle, winding in (or out) a turn at a time
    if app.embroidery.lettering_arc != 0.0
        && let Some(v) = num(ui, tl!("Spiral"), "let-spiral", app.embroidery.lettering_spiral, " mm", 0).map(|v| v.clamp(-1000.0, 1000.0))
    {
        app.embroidery.lettering_spiral = v;
    }
    let spiral = if app.embroidery.lettering_arc != 0.0 { app.embroidery.lettering_spiral } else { 0.0 };
    // the order the letters are sewn in, how they join, where the thread is cut
    sew_rows(app, ui);
    ui.checkbox(&mut app.embroidery.lettering_puff, tl!("3D puff"))
        .on_hover_text(tl!("Satin sewn over foam: dense, no underlay, its ends capped. Sew it in a colour of its own, to lay the foam at the stop"));
    // how the letters are stitched, and small text
    {
        use vectorcraft_embroidery::sf_core::attr::{LetterStitch, SmallText};
        let kinds = [LetterStitch::Font, LetterStitch::Satin, LetterStitch::Fill];
        let cur = kinds.iter().position(|k| *k == app.embroidery.lettering_stitch).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Letters sewn"), "let-stitch", cur, &[tl!("As the font has them"), tl!("Satin"), tl!("Fill")])
            && let Some(k) = kinds.get(i)
        {
            app.embroidery.lettering_stitch = *k;
        }
        let smalls = [SmallText::Auto, SmallText::On, SmallText::Off];
        let cur = smalls.iter().position(|k| *k == app.embroidery.lettering_small).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Small text"), "let-small", cur, &[tl!("Under 6 mm"), tl!("On"), tl!("Off")])
            && let Some(k) = smalls.get(i)
        {
            app.embroidery.lettering_small = *k;
        }
    }
    // appliqué letters, in a fabric colour
    ui.horizontal(|ui| {
        let mut on = app.embroidery.lettering_applique.is_some();
        if ui
            .checkbox(&mut on, tl!("Appliqué letters"))
            .on_hover_text(tl!("Each letter a patch of fabric: placement line, tack-down and cover"))
            .changed()
        {
            app.embroidery.lettering_applique = on.then_some([0xd9, 0xd9, 0xd9]);
        }
        if let Some(c) = app.embroidery.lettering_applique.as_mut() {
            egui::color_picker::color_edit_button_srgb(ui, c);
        }
    });
    let applique = app.embroidery.lettering_applique.map_or(Value::Bool(false), |[r, g, b]| json!(format!("#{r:02x}{g:02x}{b:02x}")));
    let sew = serde_json::to_value(app.embroidery.lettering_sew).unwrap_or_default();
    let arc = app.embroidery.lettering_arc;
    let text = app.embroidery.lettering_text.clone();
    let mut params = json!({ "text": text, "font": name, "size": size, "arc": arc, "spiral": spiral,
        "order": sew["order"], "lines": if sew["up"].as_bool() == Some(true) { "up" } else { "down" },
        "join": sew["join"], "trim": sew["trim"], "puff": app.embroidery.lettering_puff, "applique": applique,
        "stitch": serde_json::to_value(app.embroidery.lettering_stitch).unwrap_or_default(),
        "vertical": app.embroidery.lettering_vertical,
        "small": serde_json::to_value(app.embroidery.lettering_small).unwrap_or_default() });
    let (label, cmd) = match &selected {
        Some((id, _)) => {
            params["id"] = json!(id);
            (tl!("Update Lettering"), "embroidery.lettering.edit")
        }
        // new lettering is centred on the artboard
        None => {
            params["align"] = json!("centre");
            (tl!("Place Lettering"), "embroidery.lettering")
        }
    };
    if selected.is_none()
        && !text.trim().is_empty()
        && button(ui, tl!("Place Along Selected Line")).on_hover_text(tl!("The text along the selected line or shape, from its start")).clicked()
    {
        let mut along = params.clone();
        along["path"] = json!(true);
        if let Some(o) = along.as_object_mut() {
            o.remove("align");
        }
        if let Err(e) = app.run("embroidery.lettering", along) {
            app.status(e.to_string());
        }
    }
    if button(ui, label).clicked() && !text.trim().is_empty() {
        match app.run(cmd, params) {
            Ok(r) => {
                if let Some(w) = r["warnings"].as_array().and_then(|w| w.first()).and_then(Value::as_str) {
                    app.status(w);
                }
            }
            Err(e) => app.status(e),
        }
    }
    if let Some((id, l)) = &selected {
        letter_rows(app, ui, *id, l);
    }
}

/// One letter of the selected lettering moved, scaled or turned.
fn letter_rows(app: &mut VectorcraftApp, ui: &mut Ui, id: u64, l: &Lettering) {
    let count = l.text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    ui.separator();
    ui.label(tl!("One letter: moved, scaled or turned, kept when the lettering is set again"));
    let index = app.embroidery.letter_index.min(count - 1);
    if let Some(v) = num(ui, tl!("Letter"), "letter-index", (index + 1) as f64, "", 0)
        && v.is_finite()
    {
        app.embroidery.letter_index = (v.round().max(1.0) as usize - 1).min(count - 1);
    }
    let index = app.embroidery.letter_index.min(count - 1);
    let a = l.letters.iter().find(|a| a.index == index).copied().unwrap_or(LetterAdjust::none(index));
    let mut change = None;
    if let Some(v) = num(ui, tl!("Move along"), "letter-dx", a.dx, " mm", 1) {
        change = Some(json!({ "dx": v }));
    }
    if let Some(v) = num(ui, tl!("Move down"), "letter-dy", a.dy, " mm", 1) {
        change = Some(json!({ "dy": v }));
    }
    if let Some(v) = num(ui, tl!("Scale"), "letter-scale", a.scale * 100.0, " %", 0) {
        change = Some(json!({ "scale": (v / 100.0).clamp(0.1, 10.0) }));
    }
    if let Some(v) = num(ui, tl!("Turn"), "letter-angle", a.angle, "°", 0) {
        change = Some(json!({ "angle": v.clamp(-360.0, 360.0) }));
    }
    if !a.is_plain() && button(ui, tl!("Reset Letter")).clicked() {
        change = Some(json!({ "reset": true }));
    }
    if let Some(mut p) = change {
        p["id"] = json!(id);
        p["index"] = json!(index);
        if let Err(e) = app.run("embroidery.lettering.letter", p) {
            app.status(e);
        }
    }
}

/// Send to Machine: the machine folders remembered, one chosen, its file written there.
pub(crate) fn send_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    if app.services.pick_folder.is_some()
        && button(ui, tl!("Add Machine Folder…"))
            .on_hover_text(tl!("A USB stick, the machine's folder or a network share the machine reads"))
            .clicked()
    {
        crate::picks::button(app, |app| {
            let Some(dir) = crate::picks::folder(app) else { return };
            if app.run("embroidery.send.targets", json!({ "add": dir })).is_ok() {
                app.embroidery.send_folder = 0;
            }
        });
    }
    let dirs = app.session.prefs.embroidery_send_dirs.clone();
    if dirs.is_empty() {
        ui.label(tl!("Add the folder your machine reads: a USB stick, its folder or a network share."));
        return;
    }
    let names: Vec<&str> = dirs.iter().map(String::as_str).collect();
    let cur = app.embroidery.send_folder.min(names.len() - 1);
    if let Some(i) = pick_names(ui, tl!("Machine folder"), "send-folder", cur, &names) {
        app.embroidery.send_folder = i;
        app.embroidery.send_status.clear();
    }
    let folder = dirs.get(app.embroidery.send_folder.min(dirs.len() - 1)).cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        if button(ui, tl!("Send")).clicked() {
            app.embroidery.send_status = match app.run("embroidery.send", json!({ "folder": folder })) {
                Ok(r) => crate::dialogs::my_machine::with_notes(format!("{} {}", tl!("Sent:"), r["path"].as_str().unwrap_or_default()), &r),
                Err(e) => e.to_string(),
            };
            app.embroidery.send_folder = 0;
        }
        if button(ui, tl!("Forget Folder")).clicked() {
            app.run("embroidery.send.targets", json!({ "forget": folder })).ok();
            app.embroidery.send_folder = 0;
            app.embroidery.send_status.clear();
        }
    });
    queue_rows(app, ui, &folder);
    if !app.embroidery.send_status.is_empty() {
        ui.label(app.embroidery.send_status.clone());
    }
}

/// The worksheets Batch Output writes, as `embroidery.output` names them.
const OUTPUT_SHEETS: [&str; 5] = ["production", "approval", "pattern", "template", "none"];

/// `embroidery.output`'s params for the section's choices, written to `dir`.
fn output_params(app: &VectorcraftApp, dir: &str) -> Value {
    let o = &app.embroidery.output;
    let mut p = json!({
        "dir": dir,
        "worksheet": OUTPUT_SHEETS.get(o.worksheet).copied().unwrap_or("production"),
        "design": !o.no_design,
    });
    if !o.formats.is_empty() {
        p["formats"] = json!(o.formats);
    }
    p
}

/// Writes the batch to `dir`, saying what was written.
fn write_output(app: &mut VectorcraftApp, dir: &str) {
    let params = output_params(app, dir);
    app.embroidery.output.dir = dir.to_string();
    app.embroidery.output.status = match app.run("embroidery.output", params) {
        Ok(r) => format!("{} {} → {}", r["files"].as_array().map_or(0, Vec::len), tl!("files written"), r["dir"].as_str().unwrap_or(dir)),
        Err(e) => e.to_string(),
    };
}

/// Thread charts of one's own: one read from a CSV file, the chosen one written out.
fn chart_rows(app: &mut VectorcraftApp, ui: &mut Ui, chart: &str) {
    ui.horizontal(|ui| {
        if app.services.pick_open.is_some()
            && button(ui, tl!("Import Chart…"))
                .on_hover_text(tl!("A thread chart of your own from a CSV file (code, name and colour a thread) or a GIMP palette (.gpl)"))
                .clicked()
        {
            crate::picks::button(app, |app| {
                let pick =
                    crate::FilePick { filters: vec![crate::FilePick::filter("CSV, GIMP palette", &["csv", "txt", "gpl"])], ..Default::default() };
                let Some(path) = crate::picks::open(app, &pick) else { return };
                let name = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "My chart".into());
                match app.run("embroidery.chart.import", json!({ "name": name, "path": path })) {
                    Ok(_) => {
                        app.run("embroidery.settings", json!({ "chart": name })).ok();
                    }
                    Err(e) => app.status(e),
                }
            });
        }
        if !chart.is_empty() && button(ui, tl!("Export Chart…")).clicked() {
            let chart = chart.to_string();
            if let Err(e) = crate::io::save_command_output(app, "embroidery.chart.export", "csv", json!({ "name": chart })) {
                app.status(e);
            }
        }
    });
}

/// Recolouring every thread at once by a scheme, matched to the design's chart.
fn recolor_row(app: &mut VectorcraftApp, ui: &mut Ui) {
    let schemes = ["hue", "mono", "grayscale", "invert", "lighten", "darken", "tint"];
    let names = [tl!("Turn the hue"), tl!("One colour"), tl!("Grey"), tl!("Inverted"), tl!("Lighter"), tl!("Darker"), tl!("Tinted")];
    let cur = app.embroidery.recolor_scheme.min(schemes.len() - 1);
    if let Some(i) = pick(ui, tl!("Recolour"), "recolor", cur, &names) {
        app.embroidery.recolor_scheme = i;
    }
    ui.horizontal(|ui| {
        let scheme = schemes.get(app.embroidery.recolor_scheme).copied().unwrap_or("hue");
        let mut p = json!({ "scheme": scheme, "all": true });
        match scheme {
            "hue" => {
                ui.add(egui::DragValue::new(&mut app.embroidery.recolor_degrees).range(-180.0..=180.0).suffix("°"));
                p["degrees"] = json!(app.embroidery.recolor_degrees);
            }
            "mono" | "tint" => {
                let c = &mut app.embroidery.recolor_tone;
                ui.color_edit_button_srgb(c);
                p["color"] = json!(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                if scheme == "tint" {
                    ui.add(egui::DragValue::new(&mut app.embroidery.recolor_amount).range(1.0..=100.0).suffix("%"));
                    p["amount"] = json!(app.embroidery.recolor_amount);
                }
            }
            "lighten" | "darken" => {
                ui.add(egui::DragValue::new(&mut app.embroidery.recolor_amount).range(1.0..=100.0).suffix("%"));
                p["amount"] = json!(app.embroidery.recolor_amount);
            }
            _ => {}
        }
        if let Some(chart) = app.session.active().and_then(|s| s.doc.embroidery.as_ref().map(|c| c.chart.clone())).filter(|c| !c.is_empty()) {
            p["chart"] = json!(chart);
        }
        if button(ui, tl!("Recolour Threads"))
            .on_hover_text(tl!("Every thread changed by the scheme and matched to the design's chart; undo puts them back"))
            .clicked()
            && let Err(e) = app.run("embroidery.recolor", p)
        {
            app.status(e);
        }
        if button(ui, tl!("Merge Colours"))
            .on_hover_text(tl!("Threads of the same or a near colour made one, and their colour blocks joined; undo puts them back"))
            .clicked()
            && let Err(e) = app.run("embroidery.colors.merge", json!({}))
        {
            app.status(e);
        }
    });
}

/// Where the design sits in the hoop: centred, or at a fixed place (mm from the hoop's centre).
fn hoop_position_row(app: &mut VectorcraftApp, ui: &mut Ui, info: &Value) {
    let pos = info["settings"]["hoopPosition"]
        .as_array()
        .map(|a| (a.first().and_then(Value::as_f64).unwrap_or(0.0), a.get(1).and_then(Value::as_f64).unwrap_or(0.0)));
    let names = [tl!("Centred"), tl!("Fixed")];
    if let Some(i) = pick(ui, tl!("In the hoop"), "hoop-pos", usize::from(pos.is_some()), &names) {
        let v = if i == 0 { Value::Null } else { json!([0.0, 0.0]) };
        app.run("embroidery.settings", json!({ "hoopPosition": v })).ok();
    }
    if let Some((mut x, mut y)) = pos {
        ui.horizontal(|ui| {
            let a = ui.add(egui::DragValue::new(&mut x).range(-500.0..=500.0).speed(0.5).prefix("x ").suffix(" mm"));
            let b = ui.add(egui::DragValue::new(&mut y).range(-500.0..=500.0).speed(0.5).prefix("y ").suffix(" mm"));
            if a.changed() || b.changed() {
                app.run("embroidery.settings", json!({ "hoopPosition": [x, y] })).ok();
            }
        });
    }
}

/// A hoop of one's own: its name and field, kept with the standard ones.
fn hoop_custom_row(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tl!("Custom Hoops")).id_salt("emb-custom-hoops").show(ui, |ui| {
        let d = &mut app.embroidery.hoop_draft;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut d.0).desired_width(110.0).hint_text(tl!("Hoop name")));
            ui.add(egui::DragValue::new(&mut d.1).range(20.0..=2000.0).suffix(" mm"));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut d.2).range(20.0..=2000.0).suffix(" mm"));
        });
        let (name, w, h) = (d.0.trim().to_string(), d.1, d.2);
        ui.horizontal(|ui| {
            if !name.is_empty() && button(ui, tl!("Save Hoop")).clicked() {
                match app.run("embroidery.hoop.saveCustom", json!({ "name": name, "width": w, "height": h })) {
                    Ok(_) => {
                        app.run("embroidery.settings", json!({ "hoop": name })).ok();
                    }
                    Err(e) => app.status(e),
                }
            }
            if app.session.prefs.embroidery_custom_hoops.iter().any(|c| c.name.eq_ignore_ascii_case(&name))
                && button(ui, tl!("Delete Hoop")).clicked()
            {
                app.run("embroidery.hoop.deleteCustom", json!({ "name": name })).ok();
            }
        });
    });
}

/// Job orders: a new one for this design, and the open ones with their status, worksheet and files.
pub(crate) fn jobs_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    {
        let d = &mut app.embroidery.job_draft;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut d.0).desired_width(120.0).hint_text(tl!("Customer")));
            ui.add(egui::DragValue::new(&mut d.1).range(1..=1_000_000).suffix(" pcs"));
        });
        ui.add(egui::TextEdit::singleline(&mut d.2).desired_width(160.0).hint_text(tl!("Due (YYYY-MM-DD)")));
    }
    let (customer, quantity, due) = app.embroidery.job_draft.clone();
    if !customer.trim().is_empty() && button(ui, tl!("New Job for This Design")).clicked() {
        match app.run("embroidery.job.create", json!({ "customer": customer, "quantity": quantity, "due": due })) {
            Ok(_) => app.embroidery.job_draft = (String::new(), 1, String::new()),
            Err(e) => app.status(e),
        }
    }
    let jobs = app
        .session
        .prefs
        .embroidery_jobs
        .iter()
        .rev()
        .filter(|j| {
            !matches!(
                j.status,
                vectorcraft_engine::cmd::embroidery_jobs::JobStatus::Done | vectorcraft_engine::cmd::embroidery_jobs::JobStatus::Cancelled
            )
        })
        .take(15)
        .cloned()
        .collect::<Vec<_>>();
    let statuses = ["open", "production", "done", "cancelled"];
    let names = [tl!("Open"), tl!("In production"), tl!("Done"), tl!("Cancelled")];
    let mut act: Option<(&str, Value)> = None;
    for j in &jobs {
        ui.separator();
        ui.label(egui::RichText::new(format!("{}  {}  ×{}  {}", j.number, j.customer, j.quantity, j.due)).small());
        ui.horizontal(|ui| {
            let cur = statuses.iter().position(|s| *s == j.status.id()).unwrap_or(0);
            if let Some(i) =
                crate::widgets::dropdown(ui, ("emb-job", j.id), names.get(cur).copied().unwrap_or_default(), &names, 110.0).filter(|i| *i != cur)
                && let Some(st) = statuses.get(i)
            {
                act = Some(("embroidery.job.update", json!({ "id": j.id, "status": st })));
            }
            if button(ui, tl!("Worksheet")).clicked() {
                act = Some(("embroidery.job.worksheet", json!({ "id": j.id })));
            }
            if button(ui, tl!("Send to StitchShop")).clicked() {
                act = Some((crate::embroidery::stitchshop::COMMAND, json!({ "job": j.id })));
            }
        });
    }
    if let Some((id, p)) = act
        && let Err(e) = app.run(id, p)
    {
        app.status(e);
    }
}

/// The exports of this session that wrote files, newest first: where they went, or why they
/// failed, each to run again.
pub(crate) fn exports_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let list = app.session.export_history.iter().take(12).cloned().collect::<Vec<_>>();
    if list.is_empty() {
        ui.label(egui::RichText::new(tl!("Nothing exported yet in this session.")).small());
        return;
    }
    let now = vectorcraft_doc::metadata::now_unix();
    let t = Tokens::get(ui.ctx());
    let mut again = None;
    for (i, r) in list.iter().enumerate() {
        let paths: Vec<&str> = r["paths"].as_array().map(|p| p.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let what = match paths.as_slice() {
            [] => r["command"].as_str().unwrap_or_default().to_string(),
            [one] => vectorcraft_engine::cmd::fileio::file_name(one),
            [first, rest @ ..] => format!("{} +{}", vectorcraft_engine::cmd::fileio::file_name(first), rest.len()),
        };
        let ago = match (now, r["at"].as_i64()) {
            (Some(n), Some(a)) if n >= a => format!("{} min", (n - a) / 60),
            _ => String::new(),
        };
        ui.horizontal(|ui| {
            let ok = r["ok"].as_bool().unwrap_or(false);
            let mark = if ok { egui::RichText::new("✓").color(t.accent) } else { egui::RichText::new("✗").color(t.error) };
            ui.label(mark);
            let tip = if ok { paths.join("\n") } else { r["error"].as_str().unwrap_or_default().to_string() };
            ui.label(egui::RichText::new(format!("{what}  {ago}")).small()).on_hover_text(tip);
            if button(ui, tl!("Again")).on_hover_text(tl!("Export it again, as it was asked, from this document")).clicked() {
                again = Some(i);
            }
        });
    }
    if let Some(i) = again
        && let Err(e) = app.run("export.again", json!({ "index": i }))
    {
        app.status(e);
    }
    if button(ui, tl!("Clear")).clicked() {
        app.run("export.history.clear", json!({})).ok();
    }
}

/// Batch Output: the design's machine files in the formats ticked, its worksheet and the design,
/// written to a folder in one step; output profiles keep the choices by name.
pub(crate) fn output_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let profiles = app.session.prefs.embroidery_output_profiles.clone();
    if !profiles.is_empty() {
        let mut names: Vec<&str> = vec![tl!("None")];
        names.extend(profiles.iter().map(|p| p.name.as_str()));
        let cur = app.embroidery.output.profile.min(names.len() - 1);
        if let Some(i) = pick_names(ui, tl!("Profile"), "out-profile", cur, &names) {
            let o = &mut app.embroidery.output;
            o.profile = i;
            // a profile fills the rows in
            if let Some(p) = i.checked_sub(1).and_then(|k| profiles.get(k)) {
                o.formats = p.formats.clone();
                o.worksheet = OUTPUT_SHEETS.iter().position(|w| *w == p.worksheet).unwrap_or(0);
                o.no_design = !p.design;
                o.profile_name = p.name.clone();
                if let Some(d) = &p.dir {
                    o.dir = d.clone();
                }
            }
        }
    }
    ui.label(tl!("Machine formats (none ticked: the machine's)"));
    ui.horizontal_wrapped(|ui| {
        let o = &mut app.embroidery.output;
        for ext in vectorcraft_engine::cmd::embroidery::MACHINE_EXTS {
            let mut on = o.formats.iter().any(|f| f == *ext);
            if ui.checkbox(&mut on, ext.to_ascii_uppercase()).changed() {
                o.formats.retain(|f| f != *ext);
                if on {
                    o.formats.push((*ext).to_string());
                }
            }
        }
    });
    let sheets = [tl!("Production"), tl!("Approval"), tl!("Pattern"), tl!("Placement template"), tl!("None")];
    if let Some(i) = pick(ui, tl!("Worksheet"), "out-sheet", app.embroidery.output.worksheet, &sheets) {
        app.embroidery.output.worksheet = i;
    }
    let mut design = !app.embroidery.output.no_design;
    if ui.checkbox(&mut design, tl!("The design file too")).changed() {
        app.embroidery.output.no_design = !design;
    }
    ui.horizontal(|ui| {
        if app.services.pick_folder.is_some()
            && button(ui, tl!("Write to Folder…")).on_hover_text(tl!("Machine files, worksheet PDF and design, named alike")).clicked()
        {
            crate::picks::button(app, |app| {
                if let Some(dir) = crate::picks::folder(app) {
                    write_output(app, &dir);
                }
            });
        }
        let dir = app.embroidery.output.dir.clone();
        if !dir.is_empty() && button(ui, tl!("Write Again")).on_hover_text(dir.clone()).clicked() {
            write_output(app, &dir);
        }
    });
    // the choices kept by name
    ui.horizontal(|ui| {
        let o = &mut app.embroidery.output;
        ui.add(egui::TextEdit::singleline(&mut o.profile_name).desired_width(110.0).hint_text(tl!("Profile name")));
        let name = o.profile_name.trim().to_string();
        if !name.is_empty() && button(ui, tl!("Save Profile")).clicked() {
            let mut p = output_params(app, &app.embroidery.output.dir);
            p["name"] = json!(name);
            if app.embroidery.output.dir.is_empty() {
                p["dir"] = Value::Null;
            }
            match app.run("embroidery.output.saveProfile", p) {
                Ok(_) => {
                    let names = &app.session.prefs.embroidery_output_profiles;
                    app.embroidery.output.profile = names.iter().position(|o| o.name.eq_ignore_ascii_case(&name)).map_or(0, |i| i + 1);
                }
                Err(e) => app.status(e),
            }
        }
        let chosen = app.embroidery.output.profile.checked_sub(1).and_then(|k| profiles.get(k)).map(|p| p.name.clone());
        if let Some(n) = chosen
            && button(ui, tl!("Delete Profile")).clicked()
        {
            if let Err(e) = app.run("embroidery.output.deleteProfile", json!({ "name": n })) {
                app.status(e);
            }
            app.embroidery.output.profile = 0;
        }
    });
    if !app.embroidery.output.status.is_empty() {
        ui.label(egui::RichText::new(app.embroidery.output.status.clone()).small());
    }
}

/// The send queue: the design queued for the chosen folder, and what waits or was sent; waiting
/// designs go by themselves once their folder is there (looked at every few seconds).
fn queue_rows(app: &mut VectorcraftApp, ui: &mut Ui, folder: &str) {
    if button(ui, tl!("Send When Ready"))
        .on_hover_text(tl!("Queue the design for this folder: it is written now if the folder is there, else as soon as it is"))
        .clicked()
    {
        app.embroidery.send_status = match app.run("embroidery.queue.add", json!({ "folder": folder })) {
            Ok(r) => match r["path"].as_str() {
                Some(p) => format!("{} {p}", tl!("Sent:")),
                None => tl!("Waiting for the folder").to_string(),
            },
            Err(e) => e.to_string(),
        };
    }
    let waiting = app.session.prefs.embroidery_queue.iter().filter(|i| i.state == "waiting").count();
    if waiting > 0 {
        let now = ui.ctx().input(|i| i.time);
        if now - app.embroidery.queue_checked > 5.0 {
            app.embroidery.queue_checked = now;
            app.run("embroidery.queue.run", json!({})).ok();
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
    }
    let items = app.session.prefs.embroidery_queue.iter().rev().take(8).cloned().collect::<Vec<_>>();
    if items.is_empty() {
        return;
    }
    ui.label(egui::RichText::new(tl!("Send queue")).strong());
    for it in &items {
        let state = match it.state.as_str() {
            "waiting" => tl!("waiting"),
            "sent" => tl!("sent"),
            _ => tl!("failed"),
        };
        let tip = it.path.clone().or_else(|| it.error.clone()).unwrap_or_else(|| it.folder.clone());
        ui.label(egui::RichText::new(format!("{}.{}  {state}", it.name, it.format)).small()).on_hover_text(tip);
    }
    if items.iter().any(|i| i.state != "waiting") && button(ui, tl!("Clear Sent")).clicked() {
        app.run("embroidery.queue.remove", json!({ "done": true })).ok();
    }
}

/// The font creator: starting an embroidery font, and a font document's properties, glyphs,
/// kerning, trying and saving it. Open by itself in a font document.
fn font_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    let open = font_design(app).is_some();
    egui::CollapsingHeader::new(tl!("Font Creator")).id_salt("emb-font-creator").default_open(open).show(ui, |ui| font_body(app, ui));
}

/// The font the active document is being made into.
fn font_design(app: &VectorcraftApp) -> Option<FontDesign> {
    app.session.active()?.doc.embroidery.as_deref()?.font.clone()
}

/// Runs a font document command, its error in the status bar.
fn font_cmd(app: &mut VectorcraftApp, id: &str, params: Value) {
    if let Err(e) = app.run(id, params) {
        app.status(e);
    }
}

/// The Font Creator section's rows.
pub(crate) fn font_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(f) = font_design(app) else {
        font_start_rows(app, ui);
        return;
    };
    let t = Tokens::get(ui.ctx());
    // its name: typed, then set when the field is left
    let mut name = if app.embroidery.font_name_editing { app.embroidery.font_name.clone() } else { f.name.clone() };
    let mut left = false;
    label_row(ui, tl!("Name"), LABEL_W, |ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(FIELD_W + 40.0));
        app.embroidery.font_name_editing = r.has_focus();
        left = r.lost_focus();
    });
    app.embroidery.font_name = name.clone();
    if left && !name.trim().is_empty() && name.trim() != f.name {
        font_cmd(app, "embroidery.fontDoc.setProps", json!({ "name": name.trim() }));
    }
    ui.label(egui::RichText::new(format!("{}: {:.1} mm", tl!("Capitals' height"), f.size)).color(t.text_dim));
    if let Some(v) = num(ui, tl!("Line spacing"), "font-leading", f.leading, "×", 2) {
        font_cmd(app, "embroidery.fontDoc.setProps", json!({ "leading": v.clamp(0.5, 5.0) }));
    }
    if let Some(v) = num(ui, tl!("Space width"), "font-space", f.space, " mm", 1) {
        font_cmd(app, "embroidery.fontDoc.setProps", json!({ "space": v.clamp(0.0, 200.0) }));
    }
    let cases = [tl!("As typed"), tl!("Capitals"), tl!("Small letters")];
    let cur = match f.letter_case.as_str() {
        "upper" => 1,
        "lower" => 2,
        _ => 0,
    };
    if let Some(i) = pick(ui, tl!("Letter case"), "font-case", cur, &cases) {
        let case = ["none", "upper", "lower"].get(i).copied().unwrap_or("none");
        font_cmd(app, "embroidery.fontDoc.setProps", json!({ "letterCase": case }));
    }
    // the sizes it sews well at, in millimetres
    if let Some(v) = num(ui, tl!("Sews well from"), "font-min", f.min_scale * f.size, " mm", 1) {
        font_cmd(app, "embroidery.fontDoc.setProps", json!({ "minScale": (v / f.size).clamp(0.0, 10.0) }));
    }
    if let Some(v) = num(ui, tl!("Sews well up to"), "font-max", f.max_scale * f.size, " mm", 1) {
        font_cmd(app, "embroidery.fontDoc.setProps", json!({ "maxScale": (v / f.size).clamp(0.0, 10.0) }));
    }
    // the glyphs: those still without art dim; a click shows a glyph's artboard
    let info = app.run("embroidery.fontDoc.info", json!({})).unwrap_or(Value::Null);
    let glyphs: Vec<(String, bool)> = info["glyphs"]
        .as_array()
        .map(|a| a.iter().map(|g| (g["name"].as_str().unwrap_or_default().to_string(), g["done"].as_bool().unwrap_or(false))).collect())
        .unwrap_or_default();
    let missing = glyphs.iter().filter(|g| !g.1).count();
    ui.add_space(4.0);
    ui.label(format!("{}: {} · {}: {missing}", tl!("Glyphs"), glyphs.len(), tl!("Still to digitize")));
    ui.horizontal_wrapped(|ui| {
        for (g, done) in &glyphs {
            let text = egui::RichText::new(g.as_str()).monospace();
            let text = if *done { text } else { text.color(t.text_dim) };
            let tip = if *done { tl!("Show this glyph") } else { tl!("Not digitized yet: show its artboard") };
            if ui.add(egui::Button::new(text).min_size(egui::vec2(22.0, 20.0))).on_hover_text(tip).clicked()
                && let Some(i) = app.session.active().and_then(|st| st.doc.artboards.iter().position(|a| &a.name == g))
            {
                if let Some(v) = app.view_mut() {
                    v.artboard = i;
                }
                crate::canvas::fit(app, "view.fitArtboard");
            }
        }
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.font_add).desired_width(110.0).hint_text(tl!("Glyphs to add")));
        if button(ui, tl!("Add")).clicked() && !app.embroidery.font_add.trim().is_empty() {
            let chars = std::mem::take(&mut app.embroidery.font_add);
            font_cmd(app, "embroidery.fontDoc.addGlyphs", json!({ "chars": chars }));
        }
    });
    // designs on keys: the selected art becomes a glyph
    ui.horizontal(|ui| {
        ui.label(tl!("Selection to glyph"));
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.font_assign).desired_width(40.0).char_limit(8));
        if button(ui, tl!("Assign")).on_hover_text(tl!("The selected art becomes the glyph, moved onto its artboard")).clicked()
            && !app.embroidery.font_assign.trim().is_empty()
        {
            let glyph = app.embroidery.font_assign.trim().to_string();
            font_cmd(app, "embroidery.fontDoc.assign", json!({ "glyph": glyph }));
        }
    });
    // kerning: each pair how much closer, 0 to take it out; a pair to add
    ui.add_space(4.0);
    ui.label(tl!("Kerning (mm closer)"));
    for (pair, v) in &f.kerning {
        if let Some(nv) = num(ui, pair, &format!("kern-{pair}"), *v, " mm", 1) {
            font_cmd(app, "embroidery.fontDoc.kern", json!({ "pair": pair, "value": nv }));
        }
    }
    ui.horizontal(|ui| {
        ui.label(tl!("Pair"));
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.kern_pair).desired_width(60.0));
        if button(ui, tl!("Kern")).clicked() {
            // two glyphs, typed together or apart
            let p = app.embroidery.kern_pair.trim().to_string();
            let pair = if p.contains(' ') { p } else { p.chars().map(String::from).collect::<Vec<_>>().join(" ") };
            font_cmd(app, "embroidery.fontDoc.kern", json!({ "pair": pair, "value": 1.0 }));
        }
    });
    // kerning classes (PU-12): a named group of glyphs, then a glyph or class against another
    ui.add_space(4.0);
    ui.label(tl!("Kerning classes"));
    for (name, glyphs) in &f.classes {
        ui.label(egui::RichText::new(format!("@{name}: {}", glyphs.join(" "))).small());
    }
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.kern_class.0).desired_width(70.0).hint_text(tl!("Class name")));
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.kern_class.1).desired_width(80.0).hint_text(tl!("Its glyphs")));
        if button(ui, tl!("Save Class")).clicked() {
            let (name, glyphs) = app.embroidery.kern_class.clone();
            font_cmd(app, "embroidery.fontDoc.kernClass", json!({ "name": name.trim(), "glyphs": glyphs }));
        }
    });
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.kern_group.0).desired_width(70.0).hint_text(tl!("Left")));
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.kern_group.1).desired_width(70.0).hint_text(tl!("Right")));
        if button(ui, tl!("Kern All"))
            .on_hover_text(tl!("Every glyph on the left against every one on the right: letters, or a class as @name"))
            .clicked()
        {
            let (left, right) = app.embroidery.kern_group.clone();
            font_cmd(app, "embroidery.fontDoc.kern", json!({ "left": left, "right": right, "value": 1.0 }));
        }
    });
    // another font's glyphs this one hasn't yet
    let others: Vec<String> = app.session.embroidery_fonts.iter().map(|x| x.info.name.clone()).filter(|n| !n.eq_ignore_ascii_case(&f.name)).collect();
    if !others.is_empty() {
        let names: Vec<&str> = others.iter().map(String::as_str).collect();
        let cur = names.iter().position(|n| *n == app.embroidery.font_other).unwrap_or(0);
        ui.horizontal(|ui| {
            if let Some(i) = pick_names(ui, tl!("Merge glyphs from"), "font-merge", cur, &names)
                && let Some(n) = names.get(i)
            {
                app.embroidery.font_other = n.to_string();
            }
        });
        if button(ui, tl!("Merge")).clicked()
            && let Some(other) = names.get(cur)
        {
            let other = other.to_string();
            font_cmd(app, "embroidery.fontDoc.merge", json!({ "font": other }));
        }
    }
    // try it: the font loaded for this session, the text set in a new document
    ui.add_space(4.0);
    ui.add(egui::TextEdit::singleline(&mut app.embroidery.font_test).hint_text(tl!("Test text")));
    if button(ui, tl!("Try in a New Document")).clicked() {
        let text = if app.embroidery.font_test.trim().is_empty() { "ABC abc".to_string() } else { app.embroidery.font_test.clone() };
        match app.run("embroidery.fontDoc.save", json!({})) {
            Ok(r) => {
                let font = r["name"].as_str().unwrap_or_default().to_string();
                if app.run("file.new", json!({ "width": 800, "height": 400 })).is_ok() {
                    font_cmd(app, "embroidery.lettering", json!({ "text": text, "font": font, "align": "centre" }));
                }
            }
            Err(e) => app.status(e),
        }
    }
    if app.services.pick_folder.is_some() && button(ui, tl!("Save Embroidery Font…")).clicked() {
        crate::picks::button(app, |app| {
            let Some(dir) = crate::picks::folder(app) else { return };
            font_cmd(app, "embroidery.fontDoc.save", json!({ "folder": dir }));
        });
    }
}

/// Starting a font: from nothing, from type, or from a loaded font.
fn font_start_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    ui.label(tl!("Make an embroidery font: a document with an artboard for each glyph to digitize."));
    if button(ui, tl!("New Embroidery Font")).clicked() {
        font_cmd(app, "embroidery.fontDoc.new", json!({}));
    }
    ui.add_space(4.0);
    let families = vectorcraft_text::FontDb::global().family_list();
    let names: Vec<&str> = families.iter().map(String::as_str).collect();
    if !names.is_empty() {
        let cur = names.iter().position(|n| *n == app.embroidery.font_family).unwrap_or(0);
        if let Some(i) = pick_names(ui, tl!("Type family"), "font-family", cur, &names)
            && let Some(n) = names.get(i)
        {
            app.embroidery.font_family = n.to_string();
        }
        if button(ui, tl!("New Font from Type")).clicked()
            && let Some(family) = names.get(cur)
        {
            let family = family.to_string();
            font_cmd(app, "embroidery.fontDoc.fromType", json!({ "family": family }));
        }
    }
    let loaded: Vec<String> = app.session.embroidery_fonts.iter().map(|x| x.info.name.clone()).collect();
    if !loaded.is_empty() {
        ui.add_space(4.0);
        let names: Vec<&str> = loaded.iter().map(String::as_str).collect();
        let cur = names.iter().position(|n| *n == app.embroidery.font_other).unwrap_or(0);
        if let Some(i) = pick_names(ui, tl!("Font"), "font-edit", cur, &names)
            && let Some(n) = names.get(i)
        {
            app.embroidery.font_other = n.to_string();
        }
        if button(ui, tl!("Edit Font")).clicked()
            && let Some(name) = names.get(cur)
        {
            let name = name.to_string();
            font_cmd(app, "embroidery.font.edit", json!({ "name": name }));
        }
    }
}

/// Team names: one machine file per name, the selected lettering set to it.
fn team_name_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tl!("Team Names")).id_salt("emb-team-names").show(ui, |ui| team_names_body(app, ui));
}

/// The Team Names section's rows.
pub(crate) fn team_names_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    if selected_lettering(app).is_none() {
        ui.label(tl!("Select lettering set in an embroidery font: a machine file is written for each name, the lettering set to it."));
        return;
    }
    ui.add(egui::TextEdit::multiline(&mut app.embroidery.team_names).desired_rows(4).hint_text(tl!("One name a line")));
    if app.services.pick_open.is_some()
        && button(ui, tl!("Import Names…")).on_hover_text(tl!("The first column of a CSV table, or a text file of one name a line")).clicked()
    {
        crate::picks::button(app, |app| {
            let pick = crate::FilePick { filters: vec![crate::FilePick::filter("CSV, GIMP palette", &["csv", "txt", "gpl"])], ..Default::default() };
            let Some(path) = crate::picks::open(app, &pick) else { return };
            match app.run("embroidery.teamNames.read", json!({ "path": path })) {
                Ok(r) => {
                    let names: Vec<&str> = r["names"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
                    app.embroidery.team_names = names.join("\n");
                }
                Err(e) => app.status(e),
            }
        });
    }
    let names: Vec<String> = app.embroidery.team_names.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
    ui.label(format!("{}: {}", tl!("Names"), names.len()));
    if !names.is_empty() {
        ui.horizontal(|ui| {
            if button(ui, tl!("Lay Out Together"))
                .on_hover_text(tl!("Every name in one design, in rows and columns, for a large hoop or patches"))
                .clicked()
                && let Err(e) = app.run("embroidery.teamNames.layout", json!({ "names": names.clone() }))
            {
                app.status(e);
            }
            if button(ui, tl!("Names Worksheet")).on_hover_text(tl!("Each name with its size, stitches and sewing time, and the totals")).clicked()
                && let Err(e) = app.run("embroidery.teamNames.worksheet", json!({ "names": names.clone() }))
            {
                app.status(e);
            }
        });
    }
    if app.services.pick_folder.is_some() && !names.is_empty() && button(ui, tl!("Export One File per Name…")).clicked() {
        crate::picks::button(app, move |app| {
            let Some(dir) = crate::picks::folder(app) else { return };
            match app.run("embroidery.teamNames.export", json!({ "names": names.clone(), "dir": dir })) {
                Ok(r) => app.status(format!("{}: {}", tl!("Team name files written"), r["count"].as_u64().unwrap_or(0))),
                Err(e) => app.status(e),
            }
        });
    }
}

/// The Patches section: a patch's shape, size, edge and colours, the words on it or a list of
/// names (a patch each); Make Patches lays a hoopful out as a new design, Export Patches writes
/// every hoopful, and Fill Hoop repeats the open design across its hoop.
pub(crate) fn patches_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    use super::PATCH_SHAPES;
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    let shapes = [tl!("Circle"), tl!("Oval"), tl!("Rectangle"), tl!("Rounded Rectangle"), tl!("Shield")];
    let f = &mut app.embroidery.patches;
    if let Some(i) = pick(ui, tl!("Shape"), "patch-shape", f.shape, &shapes) {
        f.shape = i;
    }
    let circle = PATCH_SHAPES.get(f.shape) == Some(&"circle");
    if let Some(v) = num(ui, tl!("Width"), "patch-w", f.width, " mm", 1) {
        f.width = v.clamp(15.0, 300.0);
    }
    if !circle && let Some(v) = num(ui, tl!("Height"), "patch-h", f.height, " mm", 1) {
        f.height = v.clamp(15.0, 300.0);
    }
    if let Some(v) = num(ui, tl!("Edge width"), "patch-border", f.border, " mm", 1) {
        f.border = if v < 0.75 { 0.0 } else { v.clamp(1.5, 8.0) };
    }
    ui.horizontal(|ui| {
        ui.label(tl!("Border"));
        ui.color_edit_button_srgb(&mut f.border_color);
        let mut filled = f.background.is_some();
        if ui.checkbox(&mut filled, tl!("Fill")).changed() {
            f.background = filled.then_some([0xf1, 0xfa, 0xee]);
        }
        if let Some(c) = f.background.as_mut() {
            ui.color_edit_button_srgb(c);
        }
        ui.label(tl!("Words"));
        ui.color_edit_button_srgb(&mut f.text_color);
    });
    ui.add(egui::TextEdit::singleline(&mut f.text).hint_text(tl!("Words on every patch")));
    ui.add(egui::TextEdit::multiline(&mut f.names).desired_rows(4).hint_text(tl!("Or one name a line: a patch for each worker")));
    let names: Vec<String> = f.names.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
    let mut params = json!({
        "shape": PATCH_SHAPES.get(f.shape).copied().unwrap_or("circle"),
        "width": f.width,
        "border": f.border,
        "borderColor": hex(f.border_color),
        "background": f.background.map(hex),
        "textColor": hex(f.text_color),
    });
    if !circle {
        params["height"] = json!(f.height);
    }
    if !names.is_empty() {
        params["names"] = json!(names);
    } else if !f.text.trim().is_empty() {
        params["text"] = json!(f.text.trim());
    }
    if !names.is_empty() {
        ui.label(format!("{}: {}", tl!("Names"), names.len()));
    }
    ui.horizontal_wrapped(|ui| {
        if button(ui, tl!("Make Patches")).on_hover_text(tl!("A new design: a hoopful of patches, each colour sewn once for all of them")).clicked() {
            match app.run("embroidery.patches.make", params.clone()) {
                Ok(r) => {
                    let (made, all, hoops) =
                        (r["made"].as_u64().unwrap_or(0), r["patches"].as_u64().unwrap_or(0), r["hoopfuls"].as_u64().unwrap_or(0));
                    app.status(format!("{}: {} / {} · {}: {}", tl!("Patches"), made, all, tl!("Hoops"), hoops));
                }
                Err(e) => app.status(e),
            }
        }
        if app.services.pick_folder.is_some()
            && button(ui, tl!("Export Patches…")).on_hover_text(tl!("Every hoopful as a machine file, and a list of the names in each")).clicked()
        {
            let params = params.clone();
            crate::picks::button(app, move |app| {
                let Some(dir) = crate::picks::folder(app) else { return };
                let mut params = params.clone();
                params["dir"] = json!(dir);
                match app.run("embroidery.patches.export", params) {
                    Ok(r) => app.status(format!("{}: {}", tl!("Patch files written"), r["hoopfuls"].as_u64().unwrap_or(0))),
                    Err(e) => app.status(e),
                }
            });
        }
    });
    if app.session.active().is_some()
        && button(ui, tl!("Fill Hoop With Copies"))
            .on_hover_text(tl!("The open design repeated across its hoop, sorted so each colour is sewn once"))
            .clicked()
    {
        match app.run("embroidery.patches.fillHoop", json!({})) {
            Ok(r) => app.status(format!("{}: {}", tl!("Fill Hoop With Copies"), r["copies"].as_u64().unwrap_or(0))),
            Err(e) => app.status(e),
        }
    }
}

/// The design library: a folder's machine files as thumbnails, searched and sorted; a click opens
/// one, and the catalogue prints them all.
/// Clipart: a motif placed digitized with a click.
pub(crate) fn clipart_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let names: Vec<String> = app
        .run("embroidery.clipart", json!({}))
        .ok()
        .and_then(|r| r["clipart"].as_array().map(|a| a.iter().filter_map(|c| c["name"].as_str().map(str::to_string)).collect()))
        .unwrap_or_default();
    ui.label(egui::RichText::new(tl!("Click a motif to place it, digitized, in the middle of the page.")).small());
    let mut pick = None;
    ui.horizontal_wrapped(|ui| {
        for n in &names {
            if ui.button(n.as_str()).clicked() {
                pick = Some(n.clone());
            }
        }
    });
    if let Some(name) = pick
        && let Err(e) = app.run("embroidery.clipart.insert", json!({ "name": name }))
    {
        app.status(e);
    }
}

/// Design templates: a new design from one, and the active one kept as one's own.
pub(crate) fn templates_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let list = app.run("embroidery.templates", json!({})).ok();
    let items: Vec<(String, String, bool)> = list
        .as_ref()
        .and_then(|r| r["templates"].as_array())
        .map(|a| {
            a.iter()
                .map(|t| {
                    (
                        t["name"].as_str().unwrap_or_default().to_string(),
                        t["description"].as_str().unwrap_or_default().to_string(),
                        t["custom"] == true,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let mut open = None;
    let mut forget = None;
    for (name, description, custom) in &items {
        ui.horizontal(|ui| {
            let r = ui.button(name.as_str());
            let r = if description.is_empty() { r } else { r.on_hover_text(description.as_str()) };
            if r.clicked() {
                open = Some(name.clone());
            }
            if *custom && ui.small_button("×").on_hover_text(tl!("Delete Template")).clicked() {
                forget = Some(name.clone());
            }
        });
    }
    if let Some(name) = open
        && let Err(e) = app.run("embroidery.template.new", json!({ "name": name }))
    {
        app.status(e);
    }
    if let Some(name) = forget {
        app.run("embroidery.template.delete", json!({ "name": name })).ok();
    }
    if app.session.active().is_some() {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut app.embroidery.template_name).desired_width(120.0).hint_text(tl!("Template name")));
            let name = app.embroidery.template_name.trim().to_string();
            if !name.is_empty()
                && button(ui, tl!("Save as Template"))
                    .on_hover_text(tl!("Keep this design's page, art, hoop, fabric and settings to start new designs from"))
                    .clicked()
            {
                match app.run("embroidery.template.save", json!({ "name": name })) {
                    Ok(_) => app.embroidery.template_name.clear(),
                    Err(e) => app.status(e),
                }
            }
        });
    }
}

pub(crate) fn library_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let lib = &mut app.embroidery.library;
    ui.horizontal(|ui| {
        let shown = if lib.folder.is_empty() { tl!("No folder chosen").to_string() } else { lib.folder.clone() };
        ui.label(egui::RichText::new(shown).small());
    });
    let mut rescan = false;
    if app.services.pick_folder.is_some() && button(ui, tl!("Choose Folder…")).clicked() {
        crate::picks::button(app, |app| {
            if let Some(dir) = crate::picks::folder(app) {
                app.embroidery.library.folder = dir;
                app.embroidery.library.result = None;
            }
        });
    }
    let lib = &mut app.embroidery.library;
    if lib.folder.is_empty() {
        return;
    }
    ui.horizontal(|ui| {
        rescan |= ui.add(egui::TextEdit::singleline(&mut lib.query).desired_width(120.0).hint_text(tl!("Find"))).changed();
        let sorts = ["name", "date", "stitches", "size"];
        let names = [tl!("Name"), tl!("Newest"), tl!("Most stitches"), tl!("Largest")];
        let cur = sorts.iter().position(|s| *s == lib.sort).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Sort"), "lib-sort", cur, &names)
            && let Some(s) = sorts.get(i)
        {
            lib.sort = (*s).to_string();
            rescan = true;
        }
        rescan |= button(ui, tl!("Refresh")).clicked();
    });
    if rescan || lib.result.is_none() {
        let params = json!({ "folder": lib.folder, "query": lib.query, "sort": lib.sort });
        let folder = lib.folder.clone();
        match app.run("embroidery.library.scan", params) {
            Ok(r) => app.embroidery.library.result = Some(r),
            Err(e) => {
                app.embroidery.library.result = Some(json!({ "folder": folder, "designs": [] }));
                app.status(e);
            }
        }
    }
    let Some(result) = app.embroidery.library.result.clone() else { return };
    let designs = result["designs"].as_array().cloned().unwrap_or_default();
    ui.label(format!("{}: {}", tl!("Designs"), designs.len()));
    if let Some(failed) = result["failed"].as_array().filter(|f| !f.is_empty()) {
        ui.label(egui::RichText::new(format!("{}: {}", tl!("Couldn't be read"), failed.len())).small());
    }
    let mut open = None;
    let tile = 92.0;
    egui::ScrollArea::vertical().id_salt("lib-grid").max_height(360.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            for d in &designs {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(tile, tile + 16.0), egui::Sense::click());
                let t = Tokens::get(ui.ctx());
                let painter = ui.painter_at(rect);
                let thumb = egui::Rect::from_min_size(rect.min, egui::vec2(tile, tile));
                painter.rect_filled(thumb, 3.0, t.panel_darker);
                draw_thumbnail(&painter, thumb.shrink(5.0), d);
                let name: String = d["name"].as_str().unwrap_or_default().chars().take(14).collect();
                painter.text(egui::pos2(rect.min.x + 2.0, rect.max.y - 14.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(10.0), t.text);
                let resp = resp.on_hover_text(format!(
                    "{}\n{:.0} × {:.0} mm   {} {}   {} {}\n{}",
                    d["name"].as_str().unwrap_or_default(),
                    d["widthMm"].as_f64().unwrap_or(0.0),
                    d["heightMm"].as_f64().unwrap_or(0.0),
                    d["stitches"].as_u64().unwrap_or(0),
                    tl!("stitches"),
                    d["colors"].as_u64().unwrap_or(0),
                    tl!("colours"),
                    d["path"].as_str().unwrap_or_default()
                ));
                if resp.hovered() {
                    painter.rect_stroke(thumb, 3.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
                }
                if resp.clicked() {
                    open = d["path"].as_str().map(str::to_string);
                }
            }
        });
    });
    if let Some(path) = open
        && let Err(e) = crate::io::open_path(app, &path)
    {
        app.status(e);
    }
    if !designs.is_empty() && button(ui, tl!("Print Catalogue")).on_hover_text(tl!("A sheet of the designs found, to print or save as PDF")).clicked()
    {
        let lib = &app.embroidery.library;
        let params = json!({ "folder": lib.folder, "query": lib.query, "sort": lib.sort });
        if let Err(e) = app.run("embroidery.library.catalogue", params) {
            app.status(e);
        }
    }
    // batch convert: the designs found written again in another format, beside them
    if !designs.is_empty() {
        let exts = vectorcraft_engine::cmd::embroidery::MACHINE_EXTS;
        let names: Vec<String> = exts.iter().map(|e| e.to_ascii_uppercase()).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let cur = app.embroidery.library.convert_to.min(names.len().saturating_sub(1));
        if let Some(i) = pick_names(ui, tl!("Convert to"), "lib-convert", cur, &names) {
            app.embroidery.library.convert_to = i;
        }
        if button(ui, tl!("Convert All")).on_hover_text(tl!("Each design found written again in this format beside it")).clicked() {
            let lib = &app.embroidery.library;
            let format = exts.get(lib.convert_to).copied().unwrap_or("dst");
            let params = json!({ "folder": lib.folder, "query": lib.query, "format": format });
            match app.run("embroidery.library.convert", params) {
                Ok(r) => {
                    let n = r["converted"].as_array().map_or(0, Vec::len);
                    let failed = r["failed"].as_array().map_or(0, Vec::len);
                    app.status(format!("{} {}, {} {}", tl!("Converted:"), n, tl!("couldn't be read:"), failed));
                    app.embroidery.library.result = None;
                }
                Err(e) => app.status(e),
            }
        }
    }
}

/// A design's stitch outline (`embroidery.library.scan`'s preview) fitted in `r`.
fn draw_thumbnail(painter: &egui::Painter, r: egui::Rect, d: &Value) {
    let (w, h) = (d["widthMm"].as_f64().unwrap_or(0.0) as f32, d["heightMm"].as_f64().unwrap_or(0.0) as f32);
    let k = r.width().min(r.height()) / w.max(h).max(1.0);
    let o = egui::pos2(r.center().x - w * k / 2.0, r.center().y - h * k / 2.0);
    for block in d["preview"].as_array().into_iter().flatten() {
        let Some(c) = block["color"].as_str().and_then(Rgb::from_hex) else { continue };
        let stroke = egui::Stroke::new(0.8, egui::Color32::from_rgb(c.r, c.g, c.b));
        for run in block["runs"].as_array().into_iter().flatten().filter_map(Value::as_array) {
            let pts: Vec<egui::Pos2> =
                run.iter().filter_map(|q| Some(egui::pos2(o.x + q.get(0)?.as_f64()? as f32 * k, o.y + q.get(1)?.as_f64()? as f32 * k))).collect();
            if pts.len() >= 2 {
                painter.add(egui::Shape::line(pts, stroke));
            }
        }
    }
}

/// Monograms: one to three letters in the lettering font, the middle one of three larger, with a
/// border, in the middle of the first artboard.
fn monogram_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tl!("Monogram")).id_salt("emb-monogram").show(ui, |ui| monogram_body(app, ui));
}

/// The Monogram section's rows.
pub(crate) fn monogram_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let font = app
        .session
        .embroidery_fonts
        .iter()
        .find(|f| f.info.name.eq_ignore_ascii_case(&app.embroidery.lettering_font))
        .or_else(|| app.session.embroidery_fonts.first())
        .map(|f| (f.info.name.clone(), f.info.size));
    let Some((font, own)) = font else {
        ui.label(tl!("Load an embroidery font (Embroidery Lettering) to make monograms."));
        return;
    };
    ui.label(format!("{}: {font}", tl!("Font")));
    // a template's look, from the gallery or the user's own
    let templates = app.run("embroidery.monogram.templates", json!({})).ok().and_then(|r| r["templates"].as_array().cloned()).unwrap_or_default();
    let mut names = vec![tl!("Custom").to_string()];
    names.extend(templates.iter().filter_map(|t| t["name"].as_str()).map(str::to_string));
    let opts: Vec<&str> = names.iter().map(String::as_str).collect();
    if let Some(i) = pick_names(ui, tl!("Template"), "mono-template", 0, &opts)
        && let Some(t) = i.checked_sub(1).and_then(|k| templates.get(k))
    {
        use vectorcraft_engine::cmd::embroidery_monogram::{BORDERS, ORNAMENTS};
        let m = &mut app.embroidery;
        m.monogram_style = ["classic", "line", "stacked"].iter().position(|s| t["style"] == *s).unwrap_or(0);
        m.monogram_border = BORDERS.iter().position(|s| t["border"] == *s).unwrap_or(0);
        m.monogram_rings = t["rings"].as_u64().unwrap_or(1) as usize;
        m.monogram_ornament = ORNAMENTS.iter().position(|s| t["ornament"] == *s).unwrap_or(0);
        if let Some(size) = t["size"].as_f64() {
            m.monogram_size = size;
        }
        m.monogram_template = t["name"].as_str().unwrap_or_default().to_string();
    }
    ui.add(egui::TextEdit::singleline(&mut app.embroidery.monogram_letters).char_limit(3).hint_text(tl!("Initials")));
    let styles = [tl!("Classic"), tl!("In a line"), tl!("Stacked")];
    if let Some(i) = pick(ui, tl!("Style"), "mono-style", app.embroidery.monogram_style, &styles) {
        app.embroidery.monogram_style = i.min(2);
    }
    let borders =
        [tl!("None"), tl!("Circle"), tl!("Diamond"), tl!("Square"), tl!("Scallops"), tl!("Oval"), tl!("Octagon"), tl!("Heart"), tl!("Shield")];
    if let Some(i) = pick(ui, tl!("Border"), "mono-border", app.embroidery.monogram_border, &borders) {
        app.embroidery.monogram_border = i.min(borders.len() - 1);
    }
    if app.embroidery.monogram_border > 0 {
        let rings = ["1", "2", "3", "4"];
        if let Some(i) = pick(ui, tl!("Rings"), "mono-rings", app.embroidery.monogram_rings.saturating_sub(1), &rings) {
            app.embroidery.monogram_rings = i + 1;
        }
    }
    let ornaments = [tl!("None"), tl!("Hearts"), tl!("Stars"), tl!("Dots"), tl!("Swirls")];
    if let Some(i) = pick(ui, tl!("Ornament"), "mono-orn", app.embroidery.monogram_ornament, &ornaments) {
        app.embroidery.monogram_ornament = i.min(ornaments.len() - 1);
    }
    let size = if app.embroidery.monogram_size > 0.0 { app.embroidery.monogram_size } else { own.max(1.0) };
    if let Some(v) = num(ui, tl!("Size"), "mono-size", size, " mm", 1) {
        app.embroidery.monogram_size = v.clamp(1.0, 300.0);
    }
    let letters = app.embroidery.monogram_letters.trim().to_string();
    if button(ui, tl!("Place Monogram")).clicked() && !letters.is_empty() {
        let style = ["classic", "line", "stacked"].get(app.embroidery.monogram_style).copied().unwrap_or("classic");
        let border = vectorcraft_engine::cmd::embroidery_monogram::BORDERS.get(app.embroidery.monogram_border).copied().unwrap_or("none");
        let ornament = vectorcraft_engine::cmd::embroidery_monogram::ORNAMENTS.get(app.embroidery.monogram_ornament).copied().unwrap_or("none");
        let params = json!({ "letters": letters, "font": font, "size": size, "style": style, "border": border,
            "rings": app.embroidery.monogram_rings.clamp(1, 4), "ornament": ornament });
        match app.run("embroidery.monogram", params) {
            Ok(r) => {
                if let Some(w) = r["warnings"].as_array().and_then(|w| w.first()).and_then(Value::as_str) {
                    app.status(w);
                }
            }
            Err(e) => app.status(e),
        }
    }
    // this look kept as a template of the user's own
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut app.embroidery.monogram_template).desired_width(120.0).hint_text(tl!("Template name")));
        let name = app.embroidery.monogram_template.trim().to_string();
        if button(ui, tl!("Save as Template")).clicked() && !name.is_empty() {
            use vectorcraft_engine::cmd::embroidery_monogram::{BORDERS, ORNAMENTS};
            let m = &app.embroidery;
            let style = ["classic", "line", "stacked"].get(m.monogram_style).copied().unwrap_or("classic");
            let border = BORDERS.get(m.monogram_border).copied().unwrap_or("none");
            let ornament = ORNAMENTS.get(m.monogram_ornament).copied().unwrap_or("none");
            let params = json!({ "name": name, "style": style, "border": border, "rings": m.monogram_rings.clamp(1, 4),
                "ornament": ornament, "size": size });
            if let Err(e) = app.run("embroidery.monogram.saveTemplate", params) {
                app.status(e);
            }
        }
        let own = templates.iter().any(|t| t["name"] == name.as_str() && t["builtin"] == false);
        if own && button(ui, tl!("Delete Template")).clicked() {
            app.run("embroidery.monogram.deleteTemplate", json!({ "name": name })).ok();
        }
    });
}

/// Buttonholes: one, or a row of them evenly spaced, in the middle of the first artboard.
fn buttonhole_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tl!("Buttonholes")).id_salt("emb-buttonholes").show(ui, |ui| {
        let mut b = app.embroidery.buttonholes;
        let styles = [tl!("Straight"), tl!("Rounded"), tl!("Keyhole")];
        if let Some(i) = pick(ui, tl!("Style"), "bh-style", b.style, &styles) {
            b.style = i.min(2);
        }
        if let Some(v) = num(ui, tl!("Length"), "bh-length", b.length, " mm", 1) {
            b.length = v.clamp(3.0, 80.0);
        }
        if let Some(v) = num(ui, tl!("Width"), "bh-width", b.width, " mm", 1) {
            b.width = v.clamp(1.5, 12.0);
        }
        if let Some(v) = num(ui, tl!("Count"), "bh-count", f64::from(b.count), "", 0) {
            b.count = v.round().clamp(1.0, 50.0) as u32;
        }
        if b.count > 1
            && let Some(v) = num(ui, tl!("Spacing"), "bh-spacing", b.spacing, " mm", 1)
        {
            b.spacing = v.clamp(1.0, 500.0);
        }
        if let Some(v) = num(ui, tl!("Angle"), "bh-angle", b.angle, "°", 0) {
            b.angle = v.clamp(-360.0, 360.0);
        }
        app.embroidery.buttonholes = b;
        if button(ui, tl!("Add Buttonholes")).clicked() {
            let centre = app.session.active().and_then(|st| st.doc.artboards.first().map(|a| a.rect.center()));
            app.run("embroidery.addButtonholes", b.params(centre.unwrap_or_default())).ok();
        }
    });
}

/// Longarm Quilting: edge-to-edge rows and borders at the sizes typed (in inches or mm, as the
/// general units say), from the selected line; the rest is on Embroidery › Longarm Quilting.
fn quilting_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    egui::CollapsingHeader::new(tl!("Longarm Quilting")).id_salt("emb-quilting").show(ui, |ui| {
        let inches = app.session.prefs.units_general == "inches";
        let (k, unit) = if inches { (25.4, "″") } else { (1.0, " mm") };
        let mut q = app.embroidery.quilt;
        let size = |ui: &mut Ui, label: &str, id: &str, v: &mut f64, lo: f64, hi: f64| {
            if let Some(n) = num(ui, label, id, *v / k, unit, if inches { 1 } else { 0 }) {
                *v = (n * k).clamp(lo, hi);
            }
        };
        ui.label(tl!("Edge-to-edge rows of the selected line"));
        size(ui, tl!("Quilt width"), "q-w", &mut q.width, 10.0, 20_000.0);
        size(ui, tl!("Quilt height"), "q-h", &mut q.height, 10.0, 20_000.0);
        size(ui, tl!("Row spacing (0: the line's height)"), "q-row", &mut q.row, 0.0, 5_000.0);
        if let Some(v) = check(ui, tl!("Nest the rows"), q.nest) {
            q.nest = v;
        }
        if button(ui, tl!("Make Rows")).clicked() {
            let mut p = json!({ "width": q.width, "height": q.height, "nest": q.nest });
            if q.row >= 1.0 {
                p["row"] = json!(q.row);
            }
            if let Err(e) = app.run("quilt.pantograph", p) {
                app.status(e);
            }
        }
        ui.separator();
        ui.label(tl!("A border of the selected line"));
        size(ui, tl!("Border length"), "q-len", &mut q.length, 10.0, 20_000.0);
        size(ui, tl!("Border depth"), "q-depth", &mut q.depth, 2.0, 1_000.0);
        size(ui, tl!("Corner leg (0: no corner)"), "q-down", &mut q.down, 0.0, 20_000.0);
        if button(ui, tl!("Make Border")).clicked() {
            let mut p = json!({ "length": q.length, "depth": q.depth });
            if q.down >= 10.0 {
                p["down"] = json!(q.down);
            }
            if let Err(e) = app.run("quilt.border", p) {
                app.status(e);
            }
        }
        app.embroidery.quilt = q;
    });
}

/// Whether the selection holds an image (Auto-Digitize traces it).
fn selected_image(app: &VectorcraftApp) -> bool {
    app.session.active().is_some_and(|st| {
        st.selection.objects.iter().any(|id| st.doc.node(*id).is_some_and(|n| matches!(n.kind, vectorcraft_engine::doc::NodeKind::Image(_))))
    })
}

/// The selected image's id.
fn selected_image_id(app: &VectorcraftApp) -> Option<u64> {
    let st = app.session.active()?;
    st.selection
        .objects
        .iter()
        .find(|id| st.doc.node(**id).is_some_and(|n| matches!(n.kind, vectorcraft_engine::doc::NodeKind::Image(_))))
        .map(|id| id.0)
}

/// The image reduced to the chosen number of thread colours, made again when the image, the count
/// or the background choice changes.
fn colour_preview(app: &mut VectorcraftApp, ui: &Ui) {
    let Some(id) = selected_image_id(app) else { return };
    let key = (id, app.embroidery.digitize_colors, app.embroidery.digitize_background);
    if app.embroidery.colour_preview.as_ref().is_some_and(|p| p.key == key) {
        return;
    }
    let params = json!({ "id": id, "colors": key.1, "removeBackground": key.2, "size": 240 });
    let Ok(info) = app.session.execute("embroidery.image.preview", &params) else {
        app.embroidery.colour_preview = None;
        return;
    };
    let bytes = info["dataBase64"].as_str().and_then(vectorcraft_format::base64_decode).unwrap_or_default();
    let Ok(img) = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) else { return };
    let img = img.to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    let texture = ui.ctx().load_texture(format!("emb-colour-preview-{id}"), color, egui::TextureOptions::NEAREST);
    let mut info = info;
    if let Some(o) = info.as_object_mut() {
        o.remove("dataBase64");
    }
    app.embroidery.colour_preview = Some(ColourPreview { key, texture, info });
}

/// The border: on or off, its width and colour, a satin edge, and Add Border for stitched shapes
/// already there (a photo digitized before, lettering, a shape).
fn border_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.embroidery.border_on, tl!("Border round the subject"))
            .on_hover_text(tl!("A band of thread round the design's outline, like a sticker's or a patch's border, sewn first"));
        if app.embroidery.border_on {
            ui.color_edit_button_srgb(&mut app.embroidery.border_color);
        }
    });
    if !app.embroidery.border_on {
        return;
    }
    if let Some(v) = num(ui, tl!("Border width"), "border-width", app.embroidery.border_width, " mm", 1) {
        app.embroidery.border_width = v.clamp(1.0, 30.0);
    }
    ui.checkbox(&mut app.embroidery.border_edge, tl!("Satin edge"));
    if button(ui, tl!("Add Border")).on_hover_text(tl!("Put the border round the selected stitched shapes")).clicked() {
        let [r, g, b] = app.embroidery.border_color;
        let q = json!({
            "width": app.embroidery.border_width,
            "color": format!("#{r:02x}{g:02x}{b:02x}"),
            "edge": if app.embroidery.border_edge { "satin" } else { "none" },
        });
        if let Err(e) = app.run("embroidery.addBorder", q) {
            app.status(e);
        }
    }
}

/// Remove Background's sensitivity and its before-and-after preview: the photo on one side of a
/// line dragged across it, its cut-out (on a checkerboard) on the other.
fn cutout_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(id) = selected_image_id(app) else { return };
    let t = Tokens::get(ui.ctx());
    let mut settled = true;
    ui.horizontal(|ui| {
        ui.label(tl!("Sensitivity"));
        ui.visuals_mut().widgets.inactive.bg_fill = t.input_border;
        ui.visuals_mut().selection.bg_fill = t.accent;
        let mut v = app.embroidery.cutout_sensitivity.min(100);
        let r = ui
            .add(egui::Slider::new(&mut v, 0..=100).integer().trailing_fill(true))
            .on_hover_text(tl!("Higher takes more of the background away; lower keeps more of the subject"));
        if r.changed() {
            app.embroidery.cutout_sensitivity = v;
        }
        // the preview waits for the slider to be let go
        settled = !r.dragged();
    });
    let key = (id, app.embroidery.cutout_sensitivity);
    if settled && app.embroidery.cutout_preview.as_ref().is_none_or(|p| p.key != key) {
        let made =
            app.session.execute("embroidery.image.cutoutPreview", &json!({ "id": id, "sensitivity": key.1, "size": 240 })).ok().and_then(|r| {
                let texture = |field: &str, name: &str| -> Option<egui::TextureHandle> {
                    let bytes = r[field].as_str().and_then(vectorcraft_format::base64_decode)?;
                    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
                    let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                    Some(ui.ctx().load_texture(format!("emb-cutout-{name}-{id}"), color, egui::TextureOptions::LINEAR))
                };
                Some(CutoutPreview {
                    key,
                    before: texture("beforeBase64", "before")?,
                    after: texture("dataBase64", "after")?,
                    kept: r["kept"].as_f64().unwrap_or(0.0),
                })
            });
        app.embroidery.cutout_preview = made;
    }
    let Some(p) = &app.embroidery.cutout_preview else { return };
    let size = p.before.size_vec2();
    let k = (ui.available_width().min(260.0) / size.x.max(1.0)).min(2.0);
    let (rect, r) = ui.allocate_exact_size(size * k, egui::Sense::click_and_drag());
    if let Some(at) = r.interact_pointer_pos() {
        app.embroidery.cutout_split = ((at.x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0);
    }
    let split = rect.left() + rect.width() * app.embroidery.cutout_split;
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    let painter = ui.painter_at(rect);
    // before, left of the line
    let left = egui::Rect::from_min_max(rect.min, egui::pos2(split, rect.bottom()));
    ui.painter_at(left).image(p.before.id(), rect, uv, Color32::WHITE);
    // after, right of it, over a checkerboard where the background went
    let right = egui::Rect::from_min_max(egui::pos2(split, rect.top()), rect.max);
    let after = ui.painter_at(right);
    let cell = 8.0;
    let (cols, rows) = ((rect.width() / cell).ceil() as i32, (rect.height() / cell).ceil() as i32);
    for y in 0..rows {
        for x in 0..cols {
            let c = if (x + y) % 2 == 0 { Color32::from_gray(235) } else { Color32::from_gray(205) };
            let at = rect.min + egui::vec2(x as f32 * cell, y as f32 * cell);
            after.rect_filled(egui::Rect::from_min_size(at, egui::vec2(cell, cell)), 0.0, c);
        }
    }
    after.image(p.after.id(), rect, uv, Color32::WHITE);
    painter.line_segment([egui::pos2(split, rect.top()), egui::pos2(split, rect.bottom())], egui::Stroke::new(2.0, t.accent));
    painter.circle_filled(egui::pos2(split, rect.center().y), 6.0, t.accent);
    r.on_hover_text(tl!("Drag across the picture to compare the photo with its cut-out"));
    ui.label(
        egui::RichText::new(crate::i18n::fmt(tl!("Before | After: {n} % kept"), &[("n", &format!("{:.0}", p.kept * 100.0))]))
            .small()
            .color(t.text_dim),
    );
}

/// Auto-Digitize for a selected image: how many thread colours (seen at once in a preview with
/// the threads they become), then trace and stitch, prepare it, or sew it as a photo.
pub(crate) fn auto_digitize_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    ui.label(tl!("Turn the image into stitches: narrow parts become satin, the rest tatami fill."));
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(tl!("Thread colors"));
        // the theme's widget fill matches the panel, which would hide the rail
        let t = Tokens::get(ui.ctx());
        ui.visuals_mut().widgets.inactive.bg_fill = t.input_border;
        ui.visuals_mut().selection.bg_fill = t.accent;
        let mut n = app.embroidery.digitize_colors.clamp(2, 32);
        if ui.add(egui::Slider::new(&mut n, 2..=32).integer().trailing_fill(true)).changed() {
            app.embroidery.digitize_colors = n;
        }
    });
    if let Some(v) = check(ui, tl!("Remove background"), app.embroidery.digitize_background) {
        app.embroidery.digitize_background = v;
    }
    // what that many colours look like, and the threads they become
    colour_preview(app, ui);
    if let Some(p) = &app.embroidery.colour_preview {
        let size = p.texture.size_vec2();
        let k = (ui.available_width().min(260.0) / size.x.max(1.0)).min(2.0);
        ui.image((p.texture.id(), size * k));
        let t = Tokens::get(ui.ctx());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
            for c in p.info["colors"].as_array().into_iter().flatten() {
                let hex = c["thread"]["color"].as_str().or_else(|| c["color"].as_str()).unwrap_or("#000000");
                let rgb = Rgb::from_hex(hex).unwrap_or(Rgb::new(0, 0, 0));
                let (rect, r) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                let fill = Color32::from_rgb(rgb.r, rgb.g, rgb.b);
                let background = c["background"] == true;
                ui.painter().rect_filled(rect, egui::CornerRadius::same(2), if background { fill.gamma_multiply(0.25) } else { fill });
                ui.painter().rect_stroke(rect, egui::CornerRadius::same(2), egui::Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
                let name = c["thread"]["name"].as_str().unwrap_or_default();
                let code = c["thread"]["code"].as_str().unwrap_or_default();
                let share = c["share"].as_f64().unwrap_or(0.0) * 100.0;
                let mut tip = format!("{name} {code}  {share:.1} %");
                if background {
                    tip = format!("{tip}  ({})", tl!("background, not sewn"));
                }
                r.on_hover_text(tip);
            }
        });
        let sewn = p.info["sewn"].as_u64().unwrap_or(0);
        let changes = p.info["threadChanges"].as_u64().unwrap_or(0);
        ui.label(egui::RichText::new(format!("{sewn} {}, {changes} {}", tl!("threads sewn"), tl!("thread changes"))).small());
    }
    ui.add_space(4.0);
    let colors = app.embroidery.digitize_colors;
    // a border round the subject: a band like a sticker's or patch's, with a satin edge
    border_rows(app, ui);
    if button(ui, tl!("Auto-Digitize Image")).clicked() {
        let mut params = json!({ "colors": colors, "removeBackground": app.embroidery.digitize_background });
        if app.embroidery.border_on {
            let [r, g, b] = app.embroidery.border_color;
            params["border"] = json!(app.embroidery.border_width);
            params["borderColor"] = json!(format!("#{r:02x}{g:02x}{b:02x}"));
            params["borderEdge"] = json!(if app.embroidery.border_edge { "satin" } else { "none" });
        }
        if let Err(e) = app.run("embroidery.autoDigitize", params) {
            app.status(e);
        }
    }
    ui.horizontal(|ui| {
        if button(ui, tl!("Remove Background")).on_hover_text(tl!("Cut the subject out of the photo, so only it is digitized")).clicked()
            && let Err(e) = app.run("embroidery.image.removeBackground", json!({ "sensitivity": app.embroidery.cutout_sensitivity }))
        {
            app.status(e);
        }
        if button(ui, tl!("Prepare Image"))
            .on_hover_text(tl!("A smoothed copy of the image in these colours, its specks cleaned, to digitize or click to stitch"))
            .clicked()
            && let Err(e) = app.run("embroidery.image.prepare", json!({ "colors": colors }))
        {
            app.status(e);
        }
    });
    cutout_rows(app, ui);
    // photo stitch: lines, coral or these colours
    let modes = ["lines", "coral", "color", "sepia", "mono", "tile"];
    let names = [tl!("Lines"), tl!("Coral"), tl!("Colours"), tl!("Sepia"), tl!("Black and White"), tl!("Colour Tile")];
    let cur = app.embroidery.photo_mode.min(modes.len() - 1);
    if let Some(i) = pick(ui, tl!("Photo Stitch"), "photo-mode", cur, &names) {
        app.embroidery.photo_mode = i;
    }
    if button(ui, tl!("Photo Stitch")).on_hover_text(tl!("Sew the photo over itself as zigzag lines, one meandering run, or its colours")).clicked() {
        let mode = modes.get(app.embroidery.photo_mode).copied().unwrap_or("lines");
        let mut params = json!({ "mode": mode });
        if mode == "color" {
            params["colors"] = json!(colors);
        }
        if ["color", "sepia", "mono", "tile"].contains(&mode) {
            params["background"] = json!(!app.embroidery.digitize_background);
        }
        if let Err(e) = app.run("embroidery.photoStitch", params) {
            app.status(e);
        }
    }
    if crate::embroidery::prompt::available(app)
        && button(ui, tl!("Redraw with Claude…"))
            .on_hover_text(tl!("Claude redraws the image as clean flat-colour art, then StitchFoundry digitizes it"))
            .clicked()
    {
        app.run("ui.redrawImage", json!({ "colors": colors })).ok();
    }
}

/// Chenille (CH-01): how the selected object sews on a chenille machine (lockstitch, chain or
/// moss), its needle height and moss pattern, Compound Chenille for a fill, and the design's
/// stops as the operator sets them.
fn chenille_rows(app: &mut VectorcraftApp, ui: &mut Ui, id: u64, e: &Embroidery) {
    use vectorcraft_embroidery::sf_core::model::{ChenilleStitch, Motif};
    let kinds = ["none", "chain", "moss"];
    let names = [tl!("Lockstitch"), tl!("Chain"), tl!("Moss")];
    let cur = match e.chenille.map(|c| c.stitch) {
        None => 0,
        Some(ChenilleStitch::Chain) => 1,
        Some(ChenilleStitch::Moss) => 2,
    };
    let mut change: Option<Value> = None;
    if let Some(i) = pick(ui, tl!("Chenille"), "chenille", cur, &names)
        && let Some(k) = kinds.get(i)
    {
        change = Some(json!({ "stitch": k }));
    }
    if let Some(c) = e.chenille {
        if let Some(v) = num(ui, tl!("Needle height"), "chen-height", f64::from(c.height), "", 0) {
            change = Some(json!({ "stitch": c.stitch.id(), "height": v.clamp(1.0, 9.0).round() as u64 }));
        }
        if c.stitch == ChenilleStitch::Moss {
            let motif = match &e.stitch {
                StitchType::Fill(f) => f.motif,
                StitchType::Run(r) => r.motif,
                _ => Motif::Coil,
            };
            let pats = [tl!("Coil"), tl!("Square loop")];
            let cur = usize::from(motif == Motif::SquareLoop);
            if let Some(i) = pick(ui, tl!("Moss pattern"), "chen-pat", cur, &pats) {
                change = Some(json!({ "stitch": "moss", "height": c.height, "pattern": if i == 1 { "square" } else { "coil" } }));
            }
        }
    }
    if let Some(mut p) = change {
        p["ids"] = json!([id]);
        if let Err(err) = app.run("embroidery.chenille.set", p) {
            app.status(err);
        }
    }
    if matches!(e.stitch, StitchType::Fill(_))
        && button(ui, tl!("Compound Chenille…"))
            .on_hover_text(tl!("A moss fill with chain borders sewn before and after it, in a group in the shape's place"))
            .clicked()
        && let Err(err) = app.run("embroidery.chenille.compound", json!({ "ids": [id] }))
    {
        app.status(err);
    }
    // the stops the operator sets on the machine
    let stops = app.run("embroidery.chenille.stops", json!({})).unwrap_or_default();
    if let Some(start) = stops["start"].as_object() {
        let list = stops["stops"].as_array().cloned().unwrap_or_default();
        egui::CollapsingHeader::new(format!("{} ({})", tl!("Chenille stops"), list.len())).id_salt("chen-stops").show(ui, |ui| {
            ui.label(format!("{}: {} {}", tl!("Start"), tl!("Chain"), start.get("height").and_then(Value::as_u64).unwrap_or(0)));
            for st in list.iter().take(200) {
                let n = st["stop"].as_u64().unwrap_or(0);
                let what = match st["change"].as_str() {
                    Some("color") => format!("{} {}", tl!("Thread"), st["name"].as_str().or(st["color"].as_str()).unwrap_or_default()),
                    Some("stitch") => (if st["stitch"] == "moss" { tl!("Moss") } else { tl!("Chain") }).to_string(),
                    _ => format!("{} {}", tl!("Needle height"), st["height"].as_u64().unwrap_or(0)),
                };
                ui.label(format!("{} {n}: {what}", tl!("Stop")));
            }
        });
    }
}

/// The Motif tool's id.
const MOTIF_TOOL: &str = "embroideryMotif";
/// The Click-to-Stitch tool's id.
const CLICK_TOOL: &str = "embroideryClick";
const LASSO_TOOL: &str = "embroideryLasso";
const REGION_TOOL: &str = "embroideryRegion";
/// The Cross Stitch tool's id.
const CROSS_TOOL: &str = "embroideryCrossStitch";

/// The Cross Stitch tool's settings: the colour it paints (the selected pattern's colours one
/// click away), Erase and the brush size.
fn cross_stitch_tool_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    let opts = app.session.tool_options_of(CROSS_TOOL);
    let color = opts["color"].as_str().unwrap_or("#c0392b").to_string();
    let set_opt = |app: &mut VectorcraftApp, key: &str, v: Value| {
        app.run("tool.setOption", json!({ "tool": CROSS_TOOL, "values": { key: v } })).ok();
    };
    let hex = |c: [u8; 3]| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]);
    ui.horizontal(|ui| {
        ui.label(tl!("Thread colour"));
        let mut c = Rgb::from_hex(&color).map_or([192, 57, 43], |c| [c.r, c.g, c.b]);
        if ui.color_edit_button_srgb(&mut c).changed() {
            set_opt(app, "color", json!(hex(c)));
        }
    });
    // the selected pattern's colours, to paint with again
    let info = app.run("embroidery.crossStitch.info", json!({})).ok();
    let used: Vec<String> = info
        .as_ref()
        .and_then(|i| i["colors"].as_array())
        .map(|list| list.iter().filter_map(|c| c["color"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    if !used.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.label(tl!("Pattern colours"));
            for c in used.iter().take(32) {
                let Some(rgb) = Rgb::from_hex(c) else { continue };
                let fill = egui::Color32::from_rgb(rgb.r, rgb.g, rgb.b);
                let chosen = c.eq_ignore_ascii_case(&color);
                let size = egui::vec2(18.0, 18.0);
                let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
                ui.painter().rect_filled(rect, 3.0, fill);
                if chosen {
                    let t = Tokens::get(ui.ctx());
                    ui.painter().rect_stroke(rect.expand(1.5), 3.0, egui::Stroke::new(2.0, t.accent), egui::StrokeKind::Outside);
                }
                if resp.on_hover_text(c.as_str()).clicked() {
                    set_opt(app, "color", json!(c));
                }
            }
        });
    }
    if let Some(v) = check(ui, tl!("Erase"), opts["erase"].as_bool().unwrap_or(false)) {
        set_opt(app, "erase", json!(v));
    }
    let filling = opts["fill"].as_bool().unwrap_or(false);
    let lining = opts["line"].as_bool().unwrap_or(false);
    if let Some(v) = check(ui, tl!("Fill a patch"), filling) {
        set_opt(app, "fill", json!(v));
        if v {
            set_opt(app, "line", json!(false));
        }
    }
    if let Some(v) = check(ui, tl!("Backstitch lines"), lining) {
        set_opt(app, "line", json!(v));
        if v {
            set_opt(app, "fill", json!(false));
        }
    }
    if !filling && !lining {
        let size = opts["size"].as_f64().unwrap_or(1.0);
        if let Some(v) = num(ui, tl!("Brush (squares)"), "cross-size", size, "", 0) {
            set_opt(app, "size", json!(v.clamp(1.0, 9.0).round() as u64));
        }
    }
    ui.label(if lining {
        tl!("Drag from corner to corner of the squares to draw a backstitch line; Alt-click a line to take it away")
    } else if filling {
        tl!("Click a square to fill every touching square like it; hold Alt to clear them")
    } else {
        tl!("Click or drag on a cross-stitch grid to fill its squares; hold Alt to erase")
    });
    let Some(info) = info else {
        if button(ui, tl!("New Cross-Stitch Pattern…")).clicked() {
            app.run("embroidery.crossStitch.new", json!({})).ok();
        }
        return;
    };
    // the selected pattern: a colour swapped for the thread colour, its fabric, its chart
    if !used.is_empty() {
        let key = egui::Id::unique("cross-replace-from");
        let mut from: String = ui.ctx().data(|d| d.get_temp(key)).filter(|c| used.contains(c)).unwrap_or_else(|| used[0].clone());
        ui.horizontal_wrapped(|ui| {
            ui.label(tl!("Replace"));
            craft_ui::ComboBox::from_id_salt("cross-replace").selected_text(from.as_str()).show_ui(ui, |ui| {
                for c in &used {
                    ui.selectable_value(&mut from, c.clone(), c.as_str());
                }
            });
            if ui.button(tl!("With the Thread Colour")).clicked()
                && !from.eq_ignore_ascii_case(&color)
                && let Err(e) = app.run("embroidery.crossStitch.replace", json!({ "from": from, "to": color }))
            {
                app.status(e);
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(key, from));
    }
    let count = info["count"].as_f64().unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.label(tl!("Fabric"));
        let shown = [11.0, 14.0, 16.0, 18.0].into_iter().find(|c| (c - count).abs() < 0.05).map_or_else(
            || format!("{} mm", (info["cell"].as_f64().unwrap_or(0.0) * 100.0).round() / 100.0),
            |c| crate::i18n::fmt(tl!("{n}-count Aida"), &[("n", &format!("{c:.0}"))]),
        );
        craft_ui::ComboBox::from_id_salt("cross-count").selected_text(shown).show_ui(ui, |ui| {
            for c in [11u32, 14, 16, 18] {
                let label = crate::i18n::fmt(tl!("{n}-count Aida"), &[("n", &c.to_string())]);
                if ui.selectable_label((f64::from(c) - count).abs() < 0.05, label).clicked() {
                    app.run("embroidery.crossStitch.set", json!({ "count": c })).ok();
                }
            }
        });
    });
    if button(ui, tl!("Print Chart…"))
        .on_hover_text(tl!("The pattern as a counted chart to stitch from: a symbol a square, numbered every 10, and a key of the threads"))
        .clicked()
        && let Err(e) = app.run("embroidery.crossStitch.chart", json!({}))
    {
        app.status(e);
    }
}

/// Freehand digitizing: the radial widget and the embroidery brush.
fn digitize_row(app: &mut VectorcraftApp, ui: &mut Ui) {
    let open = app.embroidery.widget.open;
    let label = if open { tl!("Hide Digitizing Widget") } else { tl!("Digitizing Widget") };
    if button(ui, label).on_hover_text(tl!("Pick a brush, width, density and shape from a dial, then draw on the canvas")).clicked() {
        app.embroidery.widget.open = !open;
        if !open {
            crate::embroidery::widget::load(app);
        }
    }
    if crate::embroidery::prompt::available(app)
        && button(ui, tl!("Design from Prompt…"))
            .on_hover_text(tl!("Describe a design and Claude draws artwork for it, then StitchFoundry digitizes it"))
            .clicked()
    {
        app.run("ui.designFromPrompt", json!({})).ok();
    }
    // the Motif tool: the motif a click or a drag places
    if app.session.tool_id() == MOTIF_TOOL {
        let ours: Vec<String> = app
            .session
            .active()
            .and_then(|st| st.doc.embroidery.as_deref().map(|c| c.motifs.iter().map(|m| m.name.clone()).collect()))
            .unwrap_or_default();
        let library = with_kept(app, ours);
        let chosen = app.session.tool_options_of(MOTIF_TOOL)["motif"].as_str().unwrap_or("Wave").to_string();
        let built = Motif::ALL.into_iter().find(|m| m.label().eq_ignore_ascii_case(&chosen));
        let custom = built.is_none().then(|| CustomMotif { name: chosen.clone(), ..CustomMotif::default() });
        if let Some(choice) = pick_motif(ui, &library, built.unwrap_or_default(), custom.as_ref()) {
            let name = match choice {
                MotifChoice::BuiltIn(m) => m.label().to_string(),
                MotifChoice::Design(name) => name,
            };
            app.run("tool.setOption", json!({ "tool": MOTIF_TOOL, "values": { "motif": name } })).ok();
        }
    }
    // the Click-to-Stitch tool: how a clicked region is stitched
    if app.session.tool_id() == CLICK_TOOL {
        let opts = app.session.tool_options_of(CLICK_TOOL);
        let kinds = ["fill", "fillNoHoles", "satin", "outline", "centerline"];
        let labels = [tl!("Fill"), tl!("Fill without holes"), tl!("Satin"), tl!("Outline"), tl!("Centre line")];
        let cur = kinds.iter().position(|k| opts["as"].as_str() == Some(*k)).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Stitch as"), "click-as", cur, &labels)
            && let Some(k) = kinds.get(i)
        {
            app.run("tool.setOption", json!({ "tool": CLICK_TOOL, "values": { "as": k } })).ok();
        }
        let tol = opts["tolerance"].as_f64().unwrap_or(32.0);
        if let Some(v) = num(ui, tl!("Tolerance"), "click-tol", tol, "", 0) {
            app.run("tool.setOption", json!({ "tool": CLICK_TOOL, "values": { "tolerance": v.clamp(0.0, 255.0).round() as u64 } })).ok();
        }
        if let Some(v) = check(ui, tl!("Contiguous"), opts["contiguous"].as_bool().unwrap_or(true)) {
            app.run("tool.setOption", json!({ "tool": CLICK_TOOL, "values": { "contiguous": v } })).ok();
        }
        ui.label(tl!("Click a region of an image to stitch it; with Contiguous off, every patch of its colour"));
    }
    // the Magnetic Lasso: how the shape is stitched, and how edges are found
    if app.session.tool_id() == LASSO_TOOL {
        let opts = app.session.tool_options_of(LASSO_TOOL);
        let kinds = ["fill", "satin", "outline", "none"];
        let labels = [tl!("Fill"), tl!("Satin"), tl!("Outline"), tl!("None")];
        let cur = kinds.iter().position(|k| opts["as"].as_str() == Some(*k)).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Stitch as"), "lasso-as", cur, &labels)
            && let Some(k) = kinds.get(i)
        {
            app.run("tool.setOption", json!({ "tool": LASSO_TOOL, "values": { "as": k } })).ok();
        }
        if let Some(v) = num(ui, tl!("Width"), "lasso-width", opts["width"].as_f64().unwrap_or(10.0), " px", 0) {
            app.run("tool.setOption", json!({ "tool": LASSO_TOOL, "values": { "width": v.clamp(1.0, 256.0).round() } })).ok();
        }
        if let Some(v) = num(ui, tl!("Contrast"), "lasso-contrast", opts["contrast"].as_f64().unwrap_or(10.0), " %", 0) {
            app.run("tool.setOption", json!({ "tool": LASSO_TOOL, "values": { "contrast": v.clamp(0.0, 100.0).round() } })).ok();
        }
        ui.label(tl!(
            "Click round part of a photo: the border snaps to its edges; click the first point, double-click or press Enter to close and stitch it."
        ));
    }
    // the Fill Region tool: how the area is stitched, and the gaps it closes
    if app.session.tool_id() == REGION_TOOL {
        let opts = app.session.tool_options_of(REGION_TOOL);
        let kinds = ["fill", "satin"];
        let labels = [tl!("Fill"), tl!("Satin")];
        let cur = kinds.iter().position(|k| opts["as"].as_str() == Some(*k)).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Stitch as"), "region-as", cur, &labels)
            && let Some(k) = kinds.get(i)
        {
            app.run("tool.setOption", json!({ "tool": REGION_TOOL, "values": { "as": k } })).ok();
        }
        if let Some(v) = num(ui, tl!("Close gaps up to"), "region-gap", opts["gap"].as_f64().unwrap_or(1.0), " mm", 1) {
            app.run("tool.setOption", json!({ "tool": REGION_TOOL, "values": { "gap": v.clamp(0.0, 25.0) } })).ok();
        }
        ui.label(tl!("Click inside an area that lines enclose, as in a CAD drawing, to fill it; small gaps between the lines are closed."));
    }
    if app.session.tool_id() == CROSS_TOOL {
        cross_stitch_tool_rows(app, ui);
    }
    if !open {
        return;
    }
    // Digitize Before / After: where new strokes go in the sewing order.
    let orders = [("top", tl!("Sewn last")), ("before", tl!("Before the selection")), ("after", tl!("After the selection"))];
    let labels: Vec<&str> = orders.iter().map(|o| o.1).collect();
    let cur = orders.iter().position(|o| o.0 == app.embroidery.widget.order).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("New strokes"), "order", cur, &labels)
        && let Some(o) = orders.get(i)
    {
        app.embroidery.widget.order = o.0.to_string();
        crate::embroidery::widget::load(app);
    }
    // Widget preferences: the values each ring offers.
    egui::CollapsingHeader::new(tl!("Widget values")).id_salt("emb-widget-values").show(ui, |ui| {
        let w = &mut app.embroidery.widget;
        for (label, id, list, max) in [
            (tl!("Widths (mm)"), "widths", &mut w.widths, 50.0),
            (tl!("Densities (mm)"), "densities", &mut w.densities, 10.0),
            (tl!("Lengths (mm)"), "lengths", &mut w.lengths, 12.0),
        ] {
            label_row(ui, label, LABEL_W, |ui| {
                let key = egui::Id::unique(("emb-widget-list", id));
                let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| crate::embroidery::widget::format_values(list));
                let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(FIELD_W * 2.0));
                if resp.lost_focus() {
                    if let Some(v) = crate::embroidery::widget::parse_values(&text, max) {
                        *list = v;
                    }
                    ui.data_mut(|d| d.remove::<String>(key));
                } else if resp.has_focus() {
                    ui.data_mut(|d| d.insert_temp(key, text));
                }
            });
        }
    });
}

/// Whether a branch group (objects sewn as one path) is selected.
fn branch_selected(app: &VectorcraftApp) -> bool {
    app.session.active().is_some_and(|st| {
        st.selection.objects.iter().filter_map(|id| st.doc.node(*id)).any(|n| {
            matches!(n.kind, vectorcraft_doc::NodeKind::Group { .. })
                && n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, StitchType::Branch))
        })
    })
}

fn object_section(app: &mut VectorcraftApp, ui: &mut Ui) {
    section_header(ui, tl!("Stitches"));
    if branch_selected(app) {
        ui.label(tl!("These objects are sewn as one path (a branch)."));
        if button(ui, tl!("Break Apart")).on_hover_text(tl!("Sew the objects separately again")).clicked() {
            app.run("embroidery.breakApart", json!({})).ok();
        }
        ui.add_space(4.0);
    }
    if selected_image(app) {
        auto_digitize_rows(app, ui);
        return;
    }
    let Some((id, emb, open)) = first_stitchable(app) else {
        ui.label(tl!("Select a path, shape or compound path to give it a stitch type."));
        ui.add_space(4.0);
        if button(ui, tl!("Convert Artwork to Embroidery")).clicked() {
            app.run("embroidery.convert", json!({})).ok();
        }
        ui.add_space(6.0);
        current_section(app, ui);
        return;
    };
    let labels: Vec<&str> = TYPES.iter().map(|t| tl!(t.0)).collect();
    if let Some(i) = pick(ui, tl!("Stitch type"), "type", type_index(emb.as_ref()), &labels)
        && let Some((_, ty)) = TYPES.get(i)
    {
        app.run("embroidery.set", json!({ "type": ty })).ok();
    }
    let Some(e) = emb else {
        ui.add_space(4.0);
        if button(ui, tl!("Convert Artwork to Embroidery")).clicked() {
            app.run("embroidery.convert", json!({})).ok();
        }
        return;
    };
    thread_row(app, ui, &e);
    style_row(app, ui, &e);
    props_rows(app, ui, Target::Selection(id), &e, open);
}

/// The style the object follows (pick one to apply it), and making its properties the
/// ones new objects take.
fn style_row(app: &mut VectorcraftApp, ui: &mut Ui, e: &Embroidery) {
    let styles: Vec<(String, bool)> = app
        .session
        .active()
        .and_then(|st| st.doc.embroidery.as_deref().map(|c| c.styles.iter().map(|s| (s.name.clone(), s.properties != e.properties())).collect()))
        .unwrap_or_default();
    if !styles.is_empty() {
        let cur = e.style.as_ref().and_then(|n| styles.iter().position(|s| s.0 == *n)).map_or(0, |i| i + 1);
        let mut opts: Vec<String> = vec![tl!("None").to_string()];
        opts.extend(styles.iter().map(|(n, changed)| if *changed && Some(n) == e.style.as_ref() { format!("{n}+") } else { n.clone() }));
        let refs: Vec<&str> = opts.iter().map(String::as_str).collect();
        if let Some(i) = pick_names(ui, tl!("Style"), "style", cur, &refs) {
            if i == 0 {
                app.run("embroidery.style.unlink", json!({})).ok();
            } else if let Some((name, _)) = styles.get(i - 1) {
                app.run("embroidery.style.apply", json!({ "name": name })).ok();
            }
        }
    }
    if !matches!(e.stitch, StitchType::Manual | StitchType::Branch | StitchType::Lettering(_))
        && button(ui, tl!("Make Current")).on_hover_text(tl!("New objects of this stitch type take these properties")).clicked()
    {
        app.run("embroidery.makeCurrent", json!({})).ok();
    }
}

/// With nothing selected: the properties new objects of a stitch type take.
fn current_section(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(cfg) = app.session.active().map(|st| st.doc.embroidery.as_deref().cloned().unwrap_or_default()) else { return };
    egui::CollapsingHeader::new(tl!("Properties for New Objects")).id_salt("emb-current").default_open(true).show(ui, |ui| {
        ui.label(tl!("Objects you give a stitch type take these properties."));
        let types: Vec<(&str, &str)> = TYPES.iter().copied().filter(|t| !matches!(t.1, "none" | "manual")).collect();
        let labels: Vec<&str> = types.iter().map(|t| tl!(t.0)).collect();
        let cur = types.iter().position(|t| t.1 == app.embroidery.new_type).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Stitch type"), "newtype", cur, &labels)
            && let Some(t) = types.get(i)
        {
            app.embroidery.new_type = t.1.to_string();
        }
        let key = types.get(types.iter().position(|t| t.1 == app.embroidery.new_type).unwrap_or(0)).map_or("fill", |t| t.1);
        let saved = app.session.prefs.embroidery_default_properties.get(key).cloned();
        let Some(e) = cfg.current_for(key).cloned().or_else(|| saved.clone()).or_else(|| StitchType::from_name(key).map(Embroidery::new)) else {
            return;
        };
        props_rows(app, ui, Target::Current(key), &e, true);
        ui.horizontal_wrapped(|ui| {
            if cfg.current.contains_key(key) {
                if button(ui, tl!("Save as Default")).on_hover_text(tl!("Every new design starts with these properties")).clicked() {
                    app.run("embroidery.current.saveAsDefault", json!({ "type": key })).ok();
                }
                if button(ui, tl!("Reset to Defaults")).clicked() {
                    app.run("embroidery.current.reset", json!({ "type": key })).ok();
                }
            }
            if saved.is_some() && button(ui, tl!("Clear Default")).on_hover_text(tl!("New designs take the built-in properties again")).clicked() {
                app.run("embroidery.current.clearDefault", json!({ "type": key })).ok();
            }
        });
    });
}

/// The stitch properties of `e`: for the selected objects (`open`: a line, sewn as a satin
/// line) or for new objects.
fn props_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery, open: bool) {
    match &e.stitch {
        StitchType::Run(r) => run_rows(app, ui, target, r),
        StitchType::Satin(s) => satin_rows(app, ui, target, e, s, open),
        StitchType::Fill(f) => fill_rows(app, ui, target, e, f),
        StitchType::Applique(a) => applique_rows(app, ui, target, a),
        StitchType::Manual => {
            ui.label(tl!("Each anchor point is a needle penetration."));
        }
        StitchType::Decoration(d) => decoration_rows(app, ui, target, d),
        StitchType::Branch | StitchType::Lettering(_) => {}
        StitchType::Redwork(r) => redwork_rows(app, ui, target, r),
    }
    common_rows(app, ui, target, e);
}

/// A running stitch's rows: its style, stitch length, width and spacing, then what the style
/// adds.
fn run_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, r: &RunParams) {
    let kinds = [
        tl!("Single"),
        tl!("Triple"),
        tl!("Blanket"),
        tl!("Zigzag"),
        tl!("Motif"),
        tl!("Backstitch"),
        tl!("Stem stitch"),
        tl!("Hand stitch"),
        tl!("Sculpture run"),
        tl!("String stitch"),
    ];
    let cur = RunKind::ALL.iter().position(|k| *k == r.kind).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Run style"), "runkind", cur, &kinds)
        && let Some(k) = RunKind::ALL.get(i)
    {
        // string stitching starts with long strings close together
        let params = if *k == RunKind::String { json!({ "kind": k, "width": 8.0, "spacing": 1.0 }) } else { json!({ "kind": k }) };
        set(app, target, params);
    }
    // a sculpture run's stitch length is its triple stitches'
    let length = if r.kind == RunKind::Sculpture { tl!("Triple stitch length") } else { tl!("Stitch length") };
    if let Some(v) = num(ui, length, "len", r.stitch_length, " mm", 2) {
        set(app, target, json!({ "stitch_length": v }));
    }
    // a string stitch's width is how much of the line each string spans
    let width = if r.kind == RunKind::String { tl!("Chord length") } else { tl!("Width") };
    if matches!(r.kind, RunKind::Blanket | RunKind::Zigzag | RunKind::Motif | RunKind::Stem | RunKind::String)
        && let Some(v) = num(ui, width, "rw", r.width, " mm", 2)
    {
        set(app, target, json!({ "width": v }));
    }
    // a hand stitch's spacing is the gap between its stitches, a sculpture run's the length
    // of its single stitches
    let spacing = match r.kind {
        RunKind::Hand => tl!("Gap"),
        RunKind::Sculpture => tl!("Single stitch length"),
        _ => tl!("Spacing"),
    };
    if matches!(r.kind, RunKind::Blanket | RunKind::Zigzag | RunKind::Motif | RunKind::Hand | RunKind::Sculpture | RunKind::String)
        && let Some(v) = num(ui, spacing, "rs", r.spacing, " mm", 2)
    {
        set(app, target, json!({ "spacing": v }));
    }
    run_style_rows(app, ui, target, r);
}

/// The rows a run style adds: a sculpture run's stitch groups, the side to flip to, the
/// passes, and a motif run's motif and grading.
fn run_style_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, r: &RunParams) {
    if r.kind == RunKind::Sculpture {
        let [triple, single] = r.groups;
        if let Some(v) = num(ui, tl!("Triple stitches"), "rgt", f64::from(triple), "", 0) {
            set(app, target, json!({ "groups": [v.round().clamp(0.0, 20.0) as u32, single] }));
        }
        if let Some(v) = num(ui, tl!("Single stitches"), "rgs", f64::from(single), "", 0) {
            set(app, target, json!({ "groups": [triple, v.round().clamp(0.0, 20.0) as u32] }));
        }
    }
    if matches!(r.kind, RunKind::Blanket | RunKind::Motif | RunKind::Stem)
        && let Some(v) = check(ui, tl!("Flip to the other side"), r.flip)
    {
        set(app, target, json!({ "flip": v }));
    }
    let passes = num(ui, tl!("Passes"), "passes", f64::from(r.passes), "", 0);
    if let Some(v) = passes {
        set(app, target, json!({ "passes": v.round().clamp(1.0, f64::from(MAX_PASSES)) as u32 }));
    }
    if r.kind == RunKind::Motif {
        motif_rows(app, ui, target, r.motif, r.custom_motif.as_ref());
        // Graded: the motifs grow or shrink to another width along the line.
        if let Some(v) = check(ui, tl!("Graded"), r.width_end.is_some()) {
            set(app, target, json!({ "width_end": if v { json!(r.width * 2.0) } else { Value::Null } }));
        }
        if let Some(w) = r.width_end
            && let Some(v) = num(ui, tl!("Width at end"), "rwend", w, " mm", 2)
        {
            set(app, target, json!({ "width_end": v.clamp(0.5, 50.0) }));
        }
    }
}

/// A satin's rows: a satin line's width, profile and corners, what it is sewn as, its density
/// and compensation, its edges, its underlay and (selected, not a line) its direction.
fn satin_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery, s: &SatinParams, open: bool) {
    if open {
        satin_line_rows(app, ui, target, e, s);
    }
    satin_column_rows(app, ui, target, s);
    satin_comp_rows(app, ui, target, s);
    satin_edge_rows(app, ui, target, s);
    satin_underlay_rows(app, ui, target, s);
    if !open && let Target::Selection(id) = target {
        direction_rows(app, ui, id, e);
    }
}

/// A satin line's width, width profile and corners.
fn satin_line_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery, s: &SatinParams) {
    let w = e.width.unwrap_or(3.0);
    if let Some(v) = num(ui, tl!("Width"), "sw", w, " mm", 2) {
        set(app, target, json!({ "width": v }));
    }
    let profiles = [tl!("Constant"), tl!("Tapered"), tl!("Calligraphy")];
    let cur = match e.profile {
        WidthProfile::Constant | WidthProfile::Pressure(_) => 0,
        WidthProfile::Tapered => 1,
        WidthProfile::Calligraphy { .. } => 2,
    };
    if let Some(i) = pick(ui, tl!("Width profile"), "profile", cur, &profiles) {
        let p = match i {
            1 => json!("Tapered"),
            2 => json!({ "Calligraphy": { "nib_angle": 145.0 } }),
            _ => json!("Constant"),
        };
        set(app, target, json!({ "profile": p }));
    }
    let corners = [tl!("Automatic"), tl!("Turn"), tl!("Miter"), tl!("Capped")];
    let cur = Corners::ALL.iter().position(|c| *c == s.corners).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Corners"), "corners", cur, &corners)
        && let Some(c) = Corners::ALL.get(i)
    {
        set(app, target, json!({ "corners": c }));
    }
}

/// What a satin is sewn as: satin, or a motif column (motifs as wide as the column) with its
/// motif and length.
fn satin_column_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, s: &SatinParams) {
    let column = s.motif_column.as_deref();
    let sew_as = [tl!("Satin"), tl!("Motifs")];
    if let Some(i) = pick(ui, tl!("Sew as"), "sewas", usize::from(column.is_some()), &sew_as) {
        set(app, target, json!({ "motif_column": if i == 1 { json!(MotifColumn::default()) } else { Value::Null } }));
    }
    if let Some(m) = column {
        let names: Vec<&str> = Motif::ALL.iter().map(|x| motif_name(*x)).collect();
        let cur = Motif::ALL.iter().position(|x| *x == m.motif).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Motif"), "colmotif", cur, &names)
            && let Some(x) = Motif::ALL.get(i)
        {
            set(app, target, json!({ "motif_column": { "motif": x, "custom_motif": null } }));
        }
        if let Some(v) = num(ui, tl!("Motif length (× width)"), "collen", m.length, "", 2) {
            set(app, target, json!({ "motif_column": { "length": v.clamp(0.2, 10.0) } }));
        }
    }
}

/// A satin's density, pull and push compensation, tatami stitches, split and spacing.
fn satin_comp_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, s: &SatinParams) {
    if let Some(v) = num(ui, tl!("Density"), "sd", s.spacing, " mm", 2) {
        set(app, target, json!({ "spacing": v }));
    }
    let twin = s.pull_comp_2.is_some();
    let label = if twin { tl!("Pull comp., side 1") } else { tl!("Pull compensation") };
    if let Some(v) = num(ui, label, "spc", s.pull_comp, " mm", 2) {
        set(app, target, json!({ "pull_comp": v }));
    }
    if let Some(side2) = s.pull_comp_2
        && let Some(v) = num(ui, tl!("Pull comp., side 2"), "spc2", side2, " mm", 2)
    {
        set(app, target, json!({ "pull_comp_2": v }));
    }
    if let Some(v) = check(ui, tl!("Different on each side"), twin) {
        set(app, target, json!({ "pull_comp_2": if v { json!(s.pull_comp) } else { Value::Null } }));
    }
    if let Some(v) = num(ui, tl!("Push compensation"), "spush", s.push_comp, " mm", 2) {
        set(app, target, json!({ "push_comp": v.clamp(0.0, 5.0) }));
    }
    // a tatami border: rows across the column split at the stitch length
    if let Some(v) = check(ui, tl!("Tatami stitches"), s.tatami) {
        let len = if v { TATAMI_LENGTH } else { SatinParams::default().split_length };
        set(app, target, json!({ "tatami": v, "split_length": len }));
    }
    let split = if s.tatami { tl!("Stitch length") } else { tl!("Auto split") };
    if let Some(v) = num(ui, split, "split", s.split_length, " mm", 1) {
        set(app, target, json!({ "split_length": v }));
    }
    let preserve = check(ui, tl!("Preserve long stitches"), s.preserve_long);
    if let Some(v) = preserve {
        set(app, target, json!({ "preserve_long": v }));
    }
    if let Some(v) = check(ui, tl!("Auto spacing"), s.auto_spacing) {
        set(app, target, json!({ "auto_spacing": v }));
    }
    if let Some(v) = check(ui, tl!("Short stitches on curves"), s.short_stitches) {
        set(app, target, json!({ "short_stitches": v }));
    }
}

/// A satin's jagged edges: stitches end short of the edge at random (fur, grass, feathers).
fn satin_edge_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, s: &SatinParams) {
    let sides = [tl!("None"), tl!("Both sides"), tl!("Side 1"), tl!("Side 2")];
    let cur = s.jagged.and_then(|j| JaggedSides::ALL.iter().position(|x| *x == j.sides)).map_or(0, |i| i + 1);
    if let Some(i) = pick(ui, tl!("Jagged edges"), "jagged", cur, &sides) {
        let depth = s.jagged.unwrap_or_default().depth;
        let v = i.checked_sub(1).and_then(|k| JaggedSides::ALL.get(k)).map_or(Value::Null, |side| json!({ "sides": side, "depth": depth }));
        set(app, target, json!({ "jagged": v }));
    }
    if let Some(j) = s.jagged
        && let Some(v) = num(ui, tl!("Jag depth"), "jagdepth", j.depth, " mm", 1)
    {
        set(app, target, json!({ "jagged": { "depth": v.clamp(0.0, 20.0) } }));
    }
}

/// A satin's underlay: automatic, or the runs and zigzags chosen.
fn satin_underlay_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, s: &SatinParams) {
    let u = &s.underlay;
    if let Some(v) = check(ui, tl!("Automatic underlay"), u.auto) {
        set(app, target, json!({ "underlay": { "auto": v } }));
    }
    if u.auto {
        return;
    }
    ui.horizontal(|ui| {
        for (label, key, val) in
            [(tl!("Centre run"), "center_run", u.center_run), (tl!("Edge run"), "edge_run", u.edge_run), (tl!("Zigzag"), "zigzag", u.zigzag)]
        {
            if let Some(v) = check(ui, label, val) {
                set(app, target, json!({ "underlay": { key: v } }));
            }
        }
    });
    if u.zigzag
        && let Some(v) = check(ui, tl!("Double zigzag"), u.double_zigzag)
    {
        set(app, target, json!({ "underlay": { "double_zigzag": v } }));
    }
}

/// A fill's rows: its style and spacing, what the style adds, its compensation, underlay and
/// (selected) direction.
fn fill_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery, f: &FillParams) {
    fill_kind_rows(app, ui, target, f);
    if f.kind == FillKind::Motif {
        fill_motif_rows(app, ui, target, f);
    }
    fill_texture_rows(app, ui, target, e, f);
    fill_comp_rows(app, ui, target, e, f);
}

/// A fill's style, spacing, stitch length, cross style and rings.
fn fill_kind_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, f: &FillParams) {
    let kinds: Vec<&str> = FillKind::ALL.iter().map(|k| tl!(k.label())).collect();
    let cur = FillKind::ALL.iter().position(|k| *k == f.kind).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Fill style"), "fillkind", cur, &kinds)
        && let Some(k) = FillKind::ALL.get(i)
    {
        set(app, target, json!({ "kind": k }));
    }
    let (spacing_label, decimals) = match f.kind {
        FillKind::Meander => (tl!("Path spacing"), 1),
        FillKind::Motif => (tl!("Row spacing"), 1),
        _ => (tl!("Density"), 2),
    };
    if let Some(v) = num(ui, spacing_label, "fd", f.spacing, " mm", decimals) {
        set(app, target, json!({ "spacing": v }));
    }
    if !matches!(f.kind, FillKind::Satin | FillKind::Radial) {
        let label = if f.kind == FillKind::Cross { tl!("Cell size") } else { tl!("Stitch length") };
        if let Some(v) = num(ui, label, "fl", f.stitch_length, " mm", 2) {
            set(app, target, json!({ "stitch_length": v }));
        }
    }
    if f.kind == FillKind::Cross {
        let styles = [tl!("Full cross"), tl!("Half cross"), tl!("Upright cross"), tl!("Double cross")];
        let cur = CrossStyle::ALL.iter().position(|c| *c == f.cross).unwrap_or(0);
        if let Some(i) = pick(ui, tl!("Cross style"), "fcross", cur, &styles)
            && let Some(c) = CrossStyle::ALL.get(i)
        {
            set(app, target, json!({ "cross": c }));
        }
    }
    // an offset fill: only so many rings in from the outline
    if f.kind == FillKind::Contour
        && let Some(v) = num(ui, tl!("Rings (0: all)"), "frings", f64::from(f.rings), "", 0)
    {
        set(app, target, json!({ "rings": v.round().clamp(0.0, 500.0) as u32 }));
    }
}

/// A motif fill's motif, the motif paired with it, its size, row offset and 3D effect.
fn fill_motif_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, f: &FillParams) {
    motif_rows(app, ui, target, f.motif, f.custom_motif.as_ref());
    // a paired motif fill: a second motif in turn with the first along the rows
    let mut pairs = vec![tl!("None")];
    pairs.extend(Motif::ALL.iter().map(|m| motif_name(*m)));
    let cur = f.motif_pair.and_then(|m| Motif::ALL.iter().position(|x| *x == m)).map_or(0, |i| i + 1);
    if let Some(i) = pick(ui, tl!("Paired with"), "fpair", cur, &pairs) {
        let v = i.checked_sub(1).and_then(|k| Motif::ALL.get(k)).map_or(Value::Null, |m| json!(m));
        set(app, target, json!({ "motif_pair": v }));
    }
    if let Some(v) = num(ui, tl!("Motif size"), "fms", f.motif_size, " mm", 1) {
        set(app, target, json!({ "motif_size": v.clamp(1.0, 50.0) }));
    }
    if let Some(v) = num(ui, tl!("Row offset"), "fmo", f.offset, "", 2) {
        set(app, target, json!({ "offset": v }));
    }
    // a 3D motif fill: sculpted as a dome or a bowl, or receding
    let warps = [tl!("None"), tl!("Globe out"), tl!("Globe in"), tl!("Perspective")];
    let cur = f.warp.and_then(|w| WarpKind::ALL.iter().position(|k| *k == w.kind)).map_or(0, |i| i + 1);
    if let Some(i) = pick(ui, tl!("3D effect"), "fwarp", cur, &warps) {
        let v =
            i.checked_sub(1).and_then(|k| WarpKind::ALL.get(k)).map_or(Value::Null, |k| json!(MotifWarp { kind: *k, ..f.warp.unwrap_or_default() }));
        set(app, target, json!({ "warp": v }));
    }
    if let Some(w) = f.warp {
        if let Some(v) = num(ui, tl!("Strength"), "fwamt", w.amount, "", 2) {
            set(app, target, json!({ "warp": { "amount": v.clamp(0.0, 0.8) } }));
        }
        if w.kind == WarpKind::Perspective
            && let Some(v) = num(ui, tl!("Direction"), "fwang", w.angle, "°", 0)
        {
            set(app, target, json!({ "warp": { "angle": v } }));
        }
    }
}

/// A tatami fill's row offset, gradient, spacing profile, randomness, texture, trapunto and
/// ragged edge, and a satin fill's texture and split.
fn fill_texture_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery, f: &FillParams) {
    if f.kind == FillKind::Tatami {
        if let Some(v) = num(ui, tl!("Row offset"), "fo", f.offset, "", 2) {
            set(app, target, json!({ "offset": v }));
        }
        // a spacing profile replaces the gradient
        if f.spacing_profile.is_empty()
            && let Some(v) = num(ui, tl!("Gradient to"), "fg", f.spacing_end, " mm", 2)
        {
            set(app, target, json!({ "spacing_end": v }));
        }
        profile_rows(app, ui, target, f);
    }
    if matches!(f.kind, FillKind::Tatami | FillKind::Meander)
        && let Some(v) = num(ui, tl!("Randomness"), "fr", f.random, "", 2)
    {
        set(app, target, json!({ "random": v }));
    }
    // a satin that turns (two inclination lines or more) takes a pattern pressed in along it
    if f.kind == FillKind::Satin && e.guides.len() >= 2 {
        texture_rows(app, ui, target, f, e);
    }
    if f.kind == FillKind::Tatami {
        texture_rows(app, ui, target, f, e);
        if let Some(v) = check(ui, tl!("Trapunto (travel along the outline)"), f.trapunto) {
            set(app, target, json!({ "trapunto": v }));
        }
        if let Some(v) = num(ui, tl!("Ragged edge"), "frag", f.ragged, " mm", 1) {
            set(app, target, json!({ "ragged": v }));
        }
    }
    if f.kind == FillKind::Satin
        && let Some(v) = num(ui, tl!("Auto split"), "fsplit", f.split_length, " mm", 1)
    {
        set(app, target, json!({ "split_length": v }));
    }
}

/// A fill's pull and push compensation, underlay and (selected) direction.
fn fill_comp_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery, f: &FillParams) {
    if let Some(v) = num(ui, tl!("Pull compensation"), "fpc", f.pull_comp, " mm", 2) {
        set(app, target, json!({ "pull_comp": v }));
    }
    if let Some(v) = num(ui, tl!("Push compensation"), "fpush", f.push_comp, " mm", 2) {
        set(app, target, json!({ "push_comp": v.clamp(0.0, 5.0) }));
    }
    // cross, meander and motif fills are sewn without underlay
    let u = &f.underlay;
    if !matches!(f.kind, FillKind::Cross | FillKind::Meander | FillKind::Motif) {
        ui.horizontal(|ui| {
            if let Some(v) = check(ui, tl!("Edge run underlay"), u.edge_run) {
                set(app, target, json!({ "underlay": { "edge_run": v } }));
            }
            if let Some(v) = check(ui, tl!("Tatami underlay"), u.tatami) {
                set(app, target, json!({ "underlay": { "tatami": v } }));
            }
        });
    }
    if matches!(f.kind, FillKind::Tatami | FillKind::Satin | FillKind::Cross)
        && let Target::Selection(id) = target
    {
        direction_rows(app, ui, id, e);
    }
}

/// An appliqué's rows: placement, tack-down, trim stop, cover stitch and the fabric.
fn applique_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, a: &AppliqueParams) {
    if let Some(v) = check(ui, tl!("Placement line and stop"), a.placement) {
        set(app, target, json!({ "placement": v }));
    }
    let tack = [tl!("None"), tl!("Run"), tl!("Zigzag"), tl!("Blanket")];
    let cur = [TackdownKind::None, TackdownKind::Run, TackdownKind::Zigzag, TackdownKind::Blanket].iter().position(|k| *k == a.tackdown).unwrap_or(2);
    if let Some(i) = pick(ui, tl!("Tack-down"), "tack", cur, &tack) {
        let k = [TackdownKind::None, TackdownKind::Run, TackdownKind::Zigzag, TackdownKind::Blanket].get(i).copied().unwrap_or_default();
        set(app, target, json!({ "tackdown": k }));
    }
    if let Some(v) = check(ui, tl!("Stop to trim fabric"), a.trim_stop) {
        set(app, target, json!({ "trim_stop": v }));
    }
    let covers = [CoverKind::None, CoverKind::Satin, CoverKind::Blanket, CoverKind::Zigzag, CoverKind::Triple];
    let cover = [tl!("None"), tl!("Satin"), tl!("Blanket"), tl!("Zigzag"), tl!("Triple run")];
    let cur = covers.iter().position(|k| *k == a.cover).unwrap_or(1);
    if let Some(i) = pick(ui, tl!("Cover stitch"), "cover", cur, &cover) {
        let k = covers.get(i).copied().unwrap_or_default();
        set(app, target, json!({ "cover": k }));
    }
    if let Some(v) = check(ui, tl!("Partial cover"), a.partial) {
        set(app, target, json!({ "partial": v }));
    }
    if let Some(v) = num(ui, tl!("Cover width"), "cw", a.cover_width, " mm", 2) {
        set(app, target, json!({ "cover_width": v }));
    }
    if let Some(v) = num(ui, tl!("Density"), "cd", a.cover_spacing, " mm", 2) {
        set(app, target, json!({ "cover_spacing": v }));
    }
    if let Some(v) = check(ui, tl!("Sew the pieces together"), a.combine) {
        set(app, target, json!({ "combine": v }));
    }
    applique_fabric_rows(app, ui, target, a);
}

/// A redwork's rows: its run style and stitch length.
fn redwork_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, r: &RunParams) {
    // redwork lines are sewn in one of the outline stitches
    let kinds = [RunKind::Single, RunKind::Triple, RunKind::Back, RunKind::Stem];
    let names = [tl!("Single"), tl!("Triple"), tl!("Backstitch"), tl!("Stem stitch")];
    let cur = kinds.iter().position(|k| *k == r.kind).unwrap_or(0);
    if let Some(i) = pick(ui, tl!("Run style"), "rwkind", cur, &names)
        && let Some(k) = kinds.get(i)
    {
        set(app, target, json!({ "kind": k }));
    }
    if let Some(v) = num(ui, tl!("Stitch length"), "rwlen", r.stitch_length, " mm", 2) {
        set(app, target, json!({ "stitch_length": v }));
    }
    ui.label(tl!("Sewn as one line that ends where it starts; the way between lines is retraced along them."));
}

/// The rows every stitch type has: start and stop, trims and stops, ties, chenille (selected)
/// and removing the stitching underneath.
fn common_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, e: &Embroidery) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if let Target::Selection(id) = target
            && button(ui, tl!("Reset Start and Stop")).on_hover_text(tl!("Let sewing start and stop at the closest points again")).clicked()
        {
            app.run("embroidery.setBeads", json!({ "id": id, "entry": null, "exit": null })).ok();
        }
        if let Some(v) = check(ui, tl!("Trim after"), e.trim_after) {
            set(app, target, json!({ "trim_after": v }));
        }
        if let Some(v) = check(ui, tl!("Stop after"), e.stop_after) {
            set(app, target, json!({ "stop_after": v }));
        }
        if let Some(v) = check(ui, tl!("Own colour block"), e.own_block) {
            set(app, target, json!({ "own_block": v }));
        }
        if let Some(v) = check(ui, tl!("Omit last stitch"), e.omit_last) {
            set(app, target, json!({ "omit_last": v }));
        }
    });
    // Tie-in and tie-off: the design's setting unless this object says otherwise.
    let ties = [tl!("Design setting"), tl!("On"), tl!("Off")];
    let index = |v: Option<bool>| match v {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    };
    let value = |i: usize| match i {
        1 => json!(true),
        2 => json!(false),
        _ => Value::Null,
    };
    if let Some(i) = pick(ui, tl!("Tie in"), "tiein", index(e.tie_in), &ties) {
        set(app, target, json!({ "tie_in": value(i) }));
    }
    if let Some(i) = pick(ui, tl!("Tie off"), "tieoff", index(e.tie_off), &ties) {
        set(app, target, json!({ "tie_off": value(i) }));
    }
    if let Target::Selection(id) = target {
        chenille_rows(app, ui, id, e);
    }
    // Remove underlying stitching: the fills below leave out what this object covers.
    if let Some(v) = check(ui, tl!("Remove underlying stitching"), e.remove_underlying) {
        set(app, target, json!({ "remove_underlying": v }));
    }
    if e.remove_underlying
        && let Some(v) = num(ui, tl!("Overlap"), "uover", e.cut_overlap(), " mm", 2)
    {
        set(app, target, json!({ "underlying_overlap": v.clamp(0.0, 10.0) }));
    }
}

/// The spacing profile of a tatami fill: presets from its density (even, accordion, fading out
/// or in across the rows), and Make Blend, which turns it into layers blending its thread into
/// another colour.
fn profile_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, f: &FillParams) {
    let d = f.spacing.max(0.1);
    let presets: [Vec<[f64; 2]>; 4] =
        [Vec::new(), vec![[0.0, d], [0.25, d * 4.0], [0.5, d], [0.75, d * 4.0], [1.0, d]], vec![[0.0, d], [1.0, 0.0]], vec![[0.0, 0.0], [1.0, d]]];
    let mut names = vec![tl!("Even"), tl!("Accordion"), tl!("Fade out"), tl!("Fade in")];
    let cur = presets.iter().position(|p| *p == f.spacing_profile).unwrap_or_else(|| {
        names.push(tl!("Custom"));
        names.len() - 1
    });
    if let Some(i) = pick(ui, tl!("Spacing profile"), "fprofile", cur, &names)
        && let Some(p) = presets.get(i)
    {
        set(app, target, json!({ "spacing_profile": p }));
    }
    // the profile as a curve to reshape
    if let Some(p) = super::profile_curve::show(ui, egui::Id::unique("emb-profile-curve"), &f.spacing_profile, d) {
        set(app, target, json!({ "spacing_profile": p }));
    }
    // a Multi Blend's layers in their threads, and taking the blend out
    if !f.blend.is_empty() {
        ui.horizontal(|ui| {
            ui.label(tl!("Blend layers"));
            for l in &f.blend {
                let (r, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb(l.thread.color.r, l.thread.color.g, l.thread.color.b));
                resp.on_hover_text(l.thread.color.to_hex());
            }
            if button(ui, tl!("Remove Blend")).clicked() {
                set(app, target, json!({ "blend": [] }));
            }
        });
    }
    let Target::Selection(id) = target else { return };
    ui.horizontal(|ui| {
        ui.label(tl!("Blend into"));
        egui::color_picker::color_edit_button_srgb(ui, &mut app.embroidery.blend_to);
        if button(ui, tl!("Make Blend")).clicked() {
            let from = app.session.active().and_then(|st| {
                let n = st.doc.node(vectorcraft_doc::NodeId(id))?;
                Some(vectorcraft_embroidery::node_thread(n, n.embroidery.as_deref()?).color)
            });
            let [r, g, b] = app.embroidery.blend_to;
            if let Some(from) = from {
                let to = Rgb::new(r, g, b);
                app.run("embroidery.makeBlend", json!({ "ids": [id], "colors": [from.to_hex(), to.to_hex()] })).ok();
            }
        }
    });
    // art painted with a linear gradient: its colours blended the way it runs
    let linear = app.session.active().and_then(|st| st.doc.node(vectorcraft_doc::NodeId(id))).is_some_and(
        |n| matches!(n.appearance.fill_paint(), vectorcraft_color::Paint::Gradient(g) if g.gradient.kind == vectorcraft_color::GradientKind::Linear),
    );
    if linear && button(ui, tl!("Blend from Gradient")).on_hover_text(tl!("Sew the gradient's colours blended the way it runs")).clicked() {
        app.run("embroidery.gradientBlend", json!({ "ids": [id] })).ok();
    }
}

/// The motif of a motif run or fill: a dropdown of the built-in motifs, then the design's own
/// (Make Motif), and Delete Motif when it sews one of those. `custom` is the design motif it
/// sews, if any: shown even when the design no longer has it.
fn motif_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, current: Motif, custom: Option<&CustomMotif>) {
    let ours: Vec<String> = app
        .session
        .active()
        .and_then(|st| st.doc.embroidery.as_deref().map(|c| c.motifs.iter().map(|m| m.name.clone()).collect()))
        .unwrap_or_default();
    let library = with_kept(app, ours.clone());
    match pick_motif(ui, &library, current, custom) {
        Some(MotifChoice::BuiltIn(m)) => set(app, target, json!({ "motif": m, "custom_motif": null })),
        Some(MotifChoice::Design(name)) => use_motif(app, target, &name),
        None => {}
    }
    if let Some(c) = custom.filter(|c| ours.contains(&c.name)) {
        ui.horizontal(|ui| {
            if button(ui, tl!("Delete Motif")).on_hover_text(tl!("Remove the motif from the design; what sews it keeps it.")).clicked() {
                app.run("embroidery.motif.delete", json!({ "name": c.name })).ok();
            }
            if !app.session.prefs.embroidery_motifs.iter().any(|m| m.name == c.name)
                && button(ui, tl!("Keep Motif")).on_hover_text(tl!("Keep the motif in My Motifs, for any design")).clicked()
            {
                app.run("embroidery.motif.keep", json!({ "name": c.name })).ok();
            }
        });
    }
}

/// The design's motifs, then those of My Motifs it hasn't.
fn with_kept(app: &VectorcraftApp, mut names: Vec<String>) -> Vec<String> {
    for m in &app.session.prefs.embroidery_motifs {
        if !names.contains(&m.name) {
            names.push(m.name.clone());
        }
    }
    names
}

/// A built-in motif's name in the UI language.
fn motif_name(m: Motif) -> &'static str {
    match m {
        Motif::Wave => tl!("Wave"),
        Motif::Scallop => tl!("Scallop"),
        Motif::Diamond => tl!("Diamond"),
        Motif::Cross => tl!("Cross"),
        Motif::Feather => tl!("Feather"),
        Motif::Heart => tl!("Heart"),
        Motif::Star => tl!("Star"),
        Motif::Leaf => tl!("Leaf"),
        Motif::Loop => tl!("Loop"),
        Motif::Zigzag => tl!("Zigzag"),
        Motif::EStitch => tl!("E-stitch"),
        Motif::Coil => tl!("Coil"),
        Motif::SquareLoop => tl!("Square loop"),
        Motif::Blackwork => tl!("Blackwork"),
        Motif::Candlewick => tl!("Candlewick"),
    }
}

/// A motif picked from the motif dropdown.
enum MotifChoice {
    BuiltIn(Motif),
    /// One of the design's motifs, by name.
    Design(String),
}

/// The motif dropdown over the built-in motifs and the design's (`library`): the motif picked,
/// if it changed.
fn pick_motif(ui: &mut Ui, library: &[String], current: Motif, custom: Option<&CustomMotif>) -> Option<MotifChoice> {
    let mut names: Vec<&str> = Motif::ALL.iter().map(|m| motif_name(*m)).collect();
    let builtin = names.len();
    names.extend(library.iter().map(String::as_str));
    if let Some(c) = custom
        && !library.contains(&c.name)
    {
        names.push(&c.name);
    }
    let cur = match custom {
        Some(c) => names.iter().skip(builtin).position(|n| *n == c.name).map_or(0, |i| i + builtin),
        None => Motif::ALL.iter().position(|m| *m == current).unwrap_or(0),
    };
    let i = pick_names(ui, tl!("Motif"), "motif", cur, &names)?;
    match Motif::ALL.get(i) {
        Some(m) => Some(MotifChoice::BuiltIn(*m)),
        None => library.get(i.checked_sub(builtin)?).cloned().map(MotifChoice::Design),
    }
}

/// Sew one of the design's motifs: the selected objects through Apply Motif (which sizes them
/// for it), new objects through the properties they take.
fn use_motif(app: &mut VectorcraftApp, target: Target, name: &str) {
    match target {
        Target::Selection(_) => {
            app.run("embroidery.motif.apply", json!({ "name": name })).ok();
        }
        Target::Current(_) => {
            let m = app.session.active().and_then(|st| st.doc.embroidery.as_deref()?.motif(name).cloned());
            if let Some(m) = m {
                set(app, target, json!({ "custom_motif": m }));
            }
        }
    }
}

/// A texture pattern's name in the UI language.
fn texture_name(p: TexturePattern) -> &'static str {
    match p {
        TexturePattern::Diamonds => tl!("Diamonds"),
        TexturePattern::Waves => tl!("Waves"),
        TexturePattern::Herringbone => tl!("Herringbone"),
        TexturePattern::Bricks => tl!("Bricks"),
        TexturePattern::Circles => tl!("Circles"),
        TexturePattern::Scales => tl!("Scales"),
        TexturePattern::Hexagons => tl!("Hexagons"),
        TexturePattern::Chevrons => tl!("Chevrons"),
        TexturePattern::Zigzag => tl!("Zigzag"),
        TexturePattern::Ogee => tl!("Ogee"),
        TexturePattern::BasketWeave => tl!("Basket weave"),
        TexturePattern::Stars => tl!("Stars"),
        TexturePattern::Hearts => tl!("Hearts"),
        TexturePattern::Leaves => tl!("Leaves"),
        TexturePattern::Stripes => tl!("Stripes"),
        TexturePattern::Slants => tl!("Slants"),
        TexturePattern::Pinstripes => tl!("Pinstripes"),
        TexturePattern::Lattice => tl!("Lattice"),
        TexturePattern::Arches => tl!("Arches"),
        TexturePattern::Rings => tl!("Rings"),
        TexturePattern::Dots => tl!("Dots"),
        TexturePattern::Spirals => tl!("Spirals"),
        TexturePattern::Pyramids => tl!("Pyramids"),
        TexturePattern::Ripples => tl!("Ripples"),
        TexturePattern::GreekKey => tl!("Greek key"),
        TexturePattern::Crosses => tl!("Crosses"),
        TexturePattern::Quatrefoils => tl!("Quatrefoils"),
        TexturePattern::Flowers => tl!("Flowers"),
        TexturePattern::Trefoils => tl!("Trefoils"),
        TexturePattern::Moons => tl!("Moons"),
        TexturePattern::Arrows => tl!("Arrows"),
        TexturePattern::NestedDiamonds => tl!("Nested diamonds"),
        TexturePattern::Argyle => tl!("Argyle"),
        TexturePattern::Wicker => tl!("Wicker"),
        TexturePattern::Feathers => tl!("Feathers"),
        TexturePattern::Paisley => tl!("Paisley"),
        TexturePattern::Swirls => tl!("Swirls"),
        TexturePattern::Snowflakes => tl!("Snowflakes"),
        TexturePattern::Bubbles => tl!("Bubbles"),
        TexturePattern::Sunbursts => tl!("Sunbursts"),
    }
}

/// Tatami textures: programmed splits (a pattern over the fill), and the split lines carved
/// into the fill.
fn texture_rows(app: &mut VectorcraftApp, ui: &mut Ui, target: Target, f: &FillParams, e: &Embroidery) {
    let mut names = vec![tl!("None")];
    names.extend(TexturePattern::ALL.iter().map(|p| texture_name(*p)));
    let cur = f.texture.and_then(|t| TexturePattern::ALL.iter().position(|p| *p == t.pattern)).map_or(0, |i| i + 1);
    if let Some(i) = pick(ui, tl!("Texture"), "texture", cur, &names) {
        let v = match (i.checked_sub(1).and_then(|k| TexturePattern::ALL.get(k)), f.texture) {
            // another pattern keeps the size and turn already set
            (Some(p), Some(t)) => json!({ "pattern": p, "size": t.size, "angle": t.angle }),
            (Some(p), None) => json!({ "pattern": p, "size": p.default_size(f.stitch_length), "angle": 0.0 }),
            (None, _) => Value::Null,
        };
        set(app, target, json!({ "texture": v }));
    }
    if let Some(t) = f.texture {
        if let Some(v) = num(ui, tl!("Texture size"), "tsize", t.size, " mm", 1) {
            set(app, target, json!({ "texture": { "size": v.clamp(1.0, 100.0) } }));
        }
        if let Some(v) = num(ui, tl!("Texture angle"), "tangle", t.angle, "°", 1) {
            set(app, target, json!({ "texture": { "angle": v } }));
        }
    }
    if let Target::Selection(id) = target
        && !e.splits.is_empty()
    {
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", tl!("Split lines:"), e.splits.len()));
            if button(ui, tl!("Clear Split Lines")).clicked() {
                app.run("embroidery.clearSplits", json!({ "ids": [id] })).ok();
            }
        });
    }
    // rows following flow lines (a Florentine or liquid effect)
    if let Target::Selection(id) = target
        && !e.flow.is_empty()
    {
        ui.horizontal(|ui| {
            ui.label(format!("{} {}", tl!("Rows follow lines:"), e.flow.len()));
            if button(ui, tl!("Clear Flow Lines")).clicked() {
                app.run("embroidery.clearFlowLines", json!({ "ids": [id] })).ok();
            }
        });
    }
}

/// Stitch direction: automatic, or inclination lines the user can edit.
fn direction_rows(app: &mut VectorcraftApp, ui: &mut Ui, id: u64, e: &Embroidery) {
    ui.add_space(4.0);
    let n = e.guides.len();
    let status = match n {
        0 if e.auto_direction => tl!("Direction: automatic").to_string(),
        0 => tl!("Direction: fixed angle").to_string(),
        1 => tl!("Direction: one inclination line").to_string(),
        n => format!("{} {n} {}", tl!("Direction: turning along"), tl!("inclination lines")),
    };
    ui.label(status);
    if let StitchType::Fill(f) = &e.stitch
        && n == 0
        && let Some(v) = num(ui, tl!("Angle"), "angle", f.angle, "°", 1)
    {
        set(app, Target::Selection(id), json!({ "angle": v, "auto_direction": false }));
    }
    ui.horizontal_wrapped(|ui| {
        if button(ui, tl!("Auto Direction")).on_hover_text(tl!("Place editable inclination lines (or pick the angle) automatically")).clicked() {
            app.run("embroidery.autoDirection", json!({})).ok();
        }
        if button(ui, tl!("Edit Lines")).on_hover_text(tl!("Drag across the shape to add a line; drag a bead to move it")).clicked() {
            app.run("tool.select", json!({ "tool": "embroideryDirection" })).ok();
        }
        if n > 0 && button(ui, tl!("Clear Lines")).clicked() {
            app.run("embroidery.clearGuides", json!({ "ids": [id] })).ok();
        }
    });
    // Stitches edited by hand.
    ui.add_space(4.0);
    if e.frozen.is_some() {
        ui.label(tl!("Stitches edited by hand: the shape no longer changes them."));
    }
    ui.horizontal_wrapped(|ui| {
        if button(ui, tl!("Edit Stitches")).on_hover_text(tl!("Pick, move, add and delete single stitches")).clicked() {
            app.run("tool.select", json!({ "tool": "embroideryStitches" })).ok();
        }
        // one needle point picked with the Stitch Edit tool: split the object there
        let opts = if app.session.tool_id() == "embroideryStitches" { app.session.tool_options() } else { Value::Null };
        if opts["id"] == id
            && let Some([at]) = opts["picked"].as_array().map(Vec::as_slice)
            && button(ui, tl!("Split Here")).on_hover_text(tl!("Split the object in two at the picked stitch")).clicked()
        {
            app.run("embroidery.stitches.split", json!({ "id": id, "at": at })).ok();
        }
        if e.frozen.is_some() && button(ui, tl!("Regenerate Stitches")).on_hover_text(tl!("Drop the hand edits and stitch the shape again")).clicked()
        {
            app.run("embroidery.regenerate", json!({ "ids": [id] })).ok();
        }
    });
}

fn thread_row(app: &mut VectorcraftApp, ui: &mut Ui, e: &Embroidery) {
    label_row(ui, tl!("Thread"), LABEL_W, |ui| {
        let current = e.thread.as_ref().map(|t| t.color);
        let mut rgb = current.map(|c| [c.r, c.g, c.b]).unwrap_or([26, 62, 140]);
        if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
            let hex = Rgb::new(rgb[0], rgb[1], rgb[2]).to_hex();
            app.run("embroidery.setThread", json!({ "color": hex })).ok();
        }
        // pick a named thread from a chart
        let chart_name = app.session.active().and_then(|s| s.doc.embroidery.as_ref().map(|c| c.chart.clone())).unwrap_or_else(|| "Generic".into());
        let all = vectorcraft_engine::cmd::embroidery_charts::all_charts(&app.session);
        let chart = all.iter().find(|c| c.name == chart_name).or(all.first());
        if let Some(chart) = chart {
            let label = e.thread.as_ref().map(super::thread_name).unwrap_or_else(|| tl!("Object color").to_string());
            craft_ui::ComboBox::from_id_salt(("emb", "thread")).selected_text(label).width(150.0).height(320.0).show_ui(ui, |ui| {
                for th in &chart.threads {
                    let c = th.color;
                    let r = ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(c.r, c.g, c.b));
                        ui.selectable_label(false, th.label())
                    });
                    if r.inner.clicked() {
                        app.run("embroidery.setThread", json!({ "color": c.to_hex(), "name": th.name, "code": th.code, "chart": th.chart })).ok();
                    }
                }
            });
        }
    });
}

fn design_section(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    section_header(ui, tl!("Design"));
    let info = app.run("embroidery.info", json!({})).unwrap_or(Value::Null);
    let stitches = info["stitches"].as_u64().unwrap_or(0);
    if stitches > 0 {
        let w = info["widthMm"].as_f64().unwrap_or(0.0);
        let h = info["heightMm"].as_f64().unwrap_or(0.0);
        let row = |ui: &mut Ui, k: &str, v: String| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(k).color(t.text_dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(v));
            });
        };
        row(ui, tl!("Stitches"), stitches.to_string());
        row(ui, tl!("Size"), format!("{w:.1} × {h:.1} mm ({:.2} × {:.2} in)", w / 25.4, h / 25.4));
        row(ui, tl!("Colors"), info["colors"].as_array().map_or(0, Vec::len).to_string());
        row(ui, tl!("Trims"), info["trims"].as_u64().unwrap_or(0).to_string());
        row(ui, tl!("Sewing time"), info["time"].as_str().unwrap_or_default().to_string());
        row(ui, tl!("Thread"), format!("{:.1} m", info["threadM"].as_f64().unwrap_or(0.0)));
        // how editable it is, by where it came from
        let grade = match info["grade"].as_str() {
            Some("C") => tl!("C: from a machine file, made into objects"),
            Some("D") => tl!("D: from a machine file, stitches only"),
            _ => tl!("A: objects with their stitch settings"),
        };
        row(ui, tl!("Design grade"), grade.to_string());
        if let Some(c) = info["costEstimate"]["perPiece"].as_f64() {
            row(ui, tl!("Cost a piece"), format!("{}{c:.2}", info["costEstimate"]["currency"].as_str().unwrap_or_default()));
        }
        if let Some(fits) = info["fits"].as_bool() {
            let (txt, col) = if fits { (tl!("Fits the hoop"), t.accent) } else { (tl!("Larger than the hoop"), t.error) };
            ui.label(egui::RichText::new(txt).color(col));
        }
    } else {
        ui.label(egui::RichText::new(tl!("No stitches yet.")).color(t.text_dim));
    }
    checks(app, ui, t);
    // Hoop and fabric.
    let hoops: Vec<String> =
        info["hoops"].as_array().map(|a| a.iter().filter_map(|h| h["name"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    let mut hoop_opts: Vec<&str> = vec![tl!("No hoop")];
    hoop_opts.extend(hoops.iter().map(String::as_str));
    let cur_hoop = info["hoop"].as_str().and_then(|h| hoops.iter().position(|x| x == h)).map_or(0, |i| i + 1);
    if let Some(i) = pick_names(ui, tl!("Hoop"), "hoop", cur_hoop, &hoop_opts) {
        let v = if i == 0 { Value::Null } else { json!(hoop_opts.get(i).copied().unwrap_or_default()) };
        app.run("embroidery.settings", json!({ "hoop": v })).ok();
    }
    ui.horizontal(|ui| {
        if button(ui, tl!("Smallest That Fits")).on_hover_text(tl!("The smallest hoop that holds the design, turned if that fits better")).clicked()
            && let Err(e) = app.run("embroidery.hoop.auto", json!({}))
        {
            app.status(e);
        }
        if info["hoop"].is_string() && button(ui, tl!("Rotate Hoop")).clicked() {
            app.run("embroidery.hoop.rotate", json!({})).ok();
        }
    });
    if info["hoop"].is_string() {
        hoop_position_row(app, ui, &info);
    }
    hoop_custom_row(app, ui);
    let fabrics: Vec<String> =
        info["fabrics"].as_array().map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()).unwrap_or_default();
    let mut fabric_opts: Vec<&str> = vec![tl!("Not set")];
    fabric_opts.extend(fabrics.iter().map(String::as_str));
    let cur_fabric = info["fabric"].as_str().and_then(|f| fabrics.iter().position(|x| x == f)).map_or(0, |i| i + 1);
    if let Some(i) = pick_names(ui, tl!("Fabric"), "fabric", cur_fabric, &fabric_opts)
        && i > 0
        && let Some(name) = fabric_opts.get(i)
    {
        app.run("embroidery.applyFabric", json!({ "name": name })).ok();
    }
    fabric_rows(app, ui, &info);
    if app.services.pick_open.is_some()
        && button(ui, tl!("Garment Photo…"))
            .on_hover_text(tl!("A photo of the garment behind the design, life size, on a locked template layer that is never stitched"))
            .clicked()
    {
        crate::picks::button(app, |app| {
            let pick = crate::FilePick {
                filters: vec![crate::FilePick::filter("Images", &["png", "jpg", "jpeg", "webp", "tif", "tiff", "bmp"])],
                ..Default::default()
            };
            let Some(path) = crate::picks::open(app, &pick) else { return };
            if let Err(e) = app.run("embroidery.background", json!({ "path": path })) {
                app.status(e);
            }
        });
    }
    // the chart threads are picked from, and matching the design's threads to it
    let charts: Vec<&str> = info["charts"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let chart = app.session.active().and_then(|s| s.doc.embroidery.as_ref().map(|c| c.chart.clone())).unwrap_or_default();
    let cur = charts.iter().position(|c| *c == chart).unwrap_or(0);
    if let Some(i) = pick_names(ui, tl!("Thread chart"), "chart", cur, &charts)
        && let Some(name) = charts.get(i)
    {
        app.run("embroidery.settings", json!({ "chart": name })).ok();
    }
    if stitches > 0 && button(ui, tl!("Match Threads to Chart")).on_hover_text(tl!("Give every stitched object the chart's closest thread")).clicked()
    {
        app.run("embroidery.matchThreads", json!({ "all": true })).ok();
    }
    chart_rows(app, ui, &chart);
    if stitches > 0 {
        recolor_row(app, ui);
    }
    machine_rows(app, ui, &info);
    // How stitches are shown.
    ui.add_space(4.0);
    let views = [tl!("Hidden"), tl!("Stitch Lines"), tl!("TrueView")];
    let cur = match app.embroidery.stitches {
        StitchView::Off => 0,
        StitchView::Lines => 1,
        StitchView::TrueView => 2,
    };
    if let Some(i) = pick(ui, tl!("Show stitches"), "view", cur, &views) {
        app.embroidery.stitches = [StitchView::Off, StitchView::Lines, StitchView::TrueView].get(i).copied().unwrap_or_default();
    }
    let arts = [tl!("Normal"), tl!("Dimmed"), tl!("Hidden")];
    let cur = match app.embroidery.art {
        ArtView::Normal => 0,
        ArtView::Dimmed => 1,
        ArtView::Hidden => 2,
    };
    if let Some(i) = pick(ui, tl!("Artwork"), "art", cur, &arts) {
        app.embroidery.art = [ArtView::Normal, ArtView::Dimmed, ArtView::Hidden].get(i).copied().unwrap_or_default();
    }
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.embroidery.connectors, tl!("Connectors"));
        ui.checkbox(&mut app.embroidery.needle_points, tl!("Needle points"));
        ui.checkbox(&mut app.embroidery.beads, tl!("Start and stop"));
        ui.checkbox(&mut app.embroidery.guides, tl!("Inclination lines"));
        ui.checkbox(&mut app.embroidery.density_map, tl!("Density map"))
            .on_hover_text(tl!("Colour the design by how deep the thread lies: blue one layer, red as dense as Check Design flags"));
        let mut fabric = app.embroidery.fabric;
        if egui::color_picker::color_edit_button_srgba(ui, &mut fabric, egui::color_picker::Alpha::Opaque)
            .on_hover_text(tl!("Fabric color"))
            .changed()
        {
            app.embroidery.fabric = fabric;
        }
    });
    // the whole design repeated in a grid: a preview of a repeated layout, not sewn
    let [cols, rows] = app.embroidery.repeats;
    if let Some(v) = num(ui, tl!("Repeats across"), "rep-cols", f64::from(cols), "", 0) {
        app.run("ui.embroideryRepeats", json!({ "columns": v })).ok();
    }
    if let Some(v) = num(ui, tl!("Repeats down"), "rep-rows", f64::from(rows), "", 0) {
        app.run("ui.embroideryRepeats", json!({ "rows": v })).ok();
    }
    if cols * rows > 1
        && let Some(v) = num(ui, tl!("Gap between repeats"), "rep-gap", app.embroidery.repeat_gap, " mm", 1)
    {
        app.run("ui.embroideryRepeats", json!({ "gap": v })).ok();
    }
}

/// The design fabric's settings: editable when the fabric was made for the design; a
/// built-in one can be copied to change.
fn fabric_rows(app: &mut VectorcraftApp, ui: &mut Ui, info: &Value) {
    let Some(name) = info["fabric"].as_str().filter(|n| !n.is_empty()).map(str::to_string) else { return };
    let Some(f) = info["fabricDetails"].as_array().and_then(|a| a.iter().find(|f| f["name"] == name.as_str())).cloned() else { return };
    let custom = f["custom"].as_bool().unwrap_or(false);
    let names: Vec<&str> = info["fabrics"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    egui::CollapsingHeader::new(tl!("Fabric settings")).id_salt("emb-fabric").show(ui, |ui| {
        let mut change: Option<Value> = None;
        ui.add_enabled_ui(custom, |ui| {
            for (label, key, id, decimals) in [
                (tl!("Satin density"), "satinSpacing", "fab-ss", 2),
                (tl!("Fill density"), "fillSpacing", "fab-fs", 2),
                (tl!("Fill stitch length"), "fillStitchLength", "fab-fl", 1),
                (tl!("Pull compensation"), "pullComp", "fab-pc", 2),
                (tl!("Push compensation"), "pushComp", "fab-push", 2),
                (tl!("Underlay spacing"), "underlaySpacing", "fab-us", 1),
            ] {
                if let Some(v) = num(ui, label, id, f[key].as_f64().unwrap_or(0.0), " mm", decimals) {
                    change = Some(json!({ key: v }));
                }
            }
            ui.horizontal(|ui| {
                if let Some(v) = check(ui, tl!("Fill underlay"), f["fillUnderlay"].as_bool().unwrap_or(true)) {
                    change = Some(json!({ "fillUnderlay": v }));
                }
                if let Some(v) = check(ui, tl!("Topping"), f["topping"].as_bool().unwrap_or(false)) {
                    change = Some(json!({ "topping": v }));
                }
            });
            if let Some(v) = check(ui, tl!("Edge run under fills"), f["fillEdgeRun"].as_bool().unwrap_or(true)) {
                change = Some(json!({ "fillEdgeRun": v }));
            }
            use vectorcraft_embroidery::sf_core::fabrics::SatinUnderlayRecipe;
            // in the order of SatinUnderlayRecipe::ALL
            let unders = [tl!("Automatic"), tl!("Centre run"), tl!("Edge run"), tl!("Zigzag"), tl!("Edge run and zigzag")];
            let cur = SatinUnderlayRecipe::ALL.iter().position(|k| f["satinUnderlay"].as_str() == Some(k.id())).unwrap_or(0);
            if let Some(i) = pick(ui, tl!("Satin underlay"), "fab-su", cur, &unders)
                && let Some(k) = SatinUnderlayRecipe::ALL.get(i)
            {
                change = Some(json!({ "satinUnderlay": k.id() }));
            }
            if let Some(v) = check(ui, tl!("Sewn on clothing"), f["garment"].as_bool().unwrap_or(true)) {
                change = Some(json!({ "garment": v }));
            }
            use vectorcraft_embroidery::sf_core::fabrics::Stabilizer;
            // in the order of Stabilizer::ALL
            let kinds = [tl!("Tear-away"), tl!("Cut-away"), tl!("No-show mesh"), tl!("Wash-away")];
            let cur = Stabilizer::ALL.iter().position(|k| f["stabilizer"].as_str() == Some(k.id())).unwrap_or(0);
            if let Some(i) = pick(ui, tl!("Stabiliser"), "fab-stab", cur, &kinds)
                && let Some(k) = Stabilizer::ALL.get(i)
            {
                change = Some(json!({ "stabilizer": k.id() }));
            }
        });
        if let Some(mut c) = change {
            c["name"] = json!(name);
            if app.run("embroidery.fabric.save", c).is_ok() {
                app.run("embroidery.applyFabric", json!({ "name": name })).ok();
            }
        }
        ui.horizontal(|ui| {
            if button(ui, tl!("Save as New Fabric")).on_hover_text(tl!("A copy of this fabric to change")).clicked() {
                let base = name.trim_end_matches(|c: char| c.is_ascii_digit()).trim_end();
                let fresh = (2..).map(|k| format!("{base} {k}")).find(|n| !names.contains(&n.as_str())).unwrap_or_default();
                if app.run("embroidery.fabric.save", json!({ "name": fresh, "from": name })).is_ok() {
                    app.run("embroidery.applyFabric", json!({ "name": fresh })).ok();
                }
            }
            if custom && button(ui, tl!("Delete Fabric")).clicked() {
                app.run("embroidery.fabric.delete", json!({ "name": name })).ok();
            }
        });
    });
}

/// A machine profile's name as the Machine menu shows it: "PES: home machines" for a
/// standard one, the name of one of the user's.
fn machine_label(m: &Value) -> String {
    let name = m["name"].as_str().unwrap_or_default();
    if m["standard"].as_bool() != Some(true) {
        return name.to_string();
    }
    let kind = if m["kind"].as_str() == Some("commercial") { tl!("commercial machines") } else { tl!("home machines") };
    format!("{}: {kind}", name.to_uppercase())
}

/// The machine the design is for (its file format, stitch limits and speed), the design's
/// machine settings, and keeping them in a machine profile.
fn machine_rows(app: &mut VectorcraftApp, ui: &mut Ui, info: &Value) {
    let machines: Vec<Value> = info["machines"].as_array().cloned().unwrap_or_default();
    let current = info["machine"].as_str().unwrap_or_default().to_string();
    let mut rows: Vec<(String, String)> = machines.iter().filter_map(|m| Some((m["name"].as_str()?.to_string(), machine_label(m)))).collect();
    // a machine the design names that isn't kept here (made on another computer, or deleted)
    if !current.is_empty() && !rows.iter().any(|r| r.0.eq_ignore_ascii_case(&current)) {
        rows.push((current.clone(), current.clone()));
    }
    let mut opts: Vec<&str> = vec![tl!("Not set")];
    opts.extend(rows.iter().map(|r| r.1.as_str()));
    let cur = rows.iter().position(|r| r.0.eq_ignore_ascii_case(&current)).map_or(0, |i| i + 1);
    if let Some(i) = pick_names(ui, tl!("Machine"), "machine", cur, &opts) {
        let v = i.checked_sub(1).and_then(|k| rows.get(k)).map_or(Value::Null, |r| json!(r.0));
        app.run("embroidery.settings", json!({ "machine": v })).ok();
    }
    let s = &info["settings"];
    egui::CollapsingHeader::new(tl!("Machine settings")).id_salt("emb-machine-settings").show(ui, |ui| {
        let lengths = [
            (tl!("Longest stitch"), "maxStitch", "mxs"),
            (tl!("Longest jump"), "maxJump", "mxj"),
            (tl!("Shortest stitch"), "minStitch", "mns"),
            (tl!("Trim jumps longer than"), "trimThreshold", "trt"),
            (tl!("Overlap where shapes touch"), "overlapTouching", "ovt"),
            (tl!("Tie length"), "tieLength", "til"),
        ];
        for (label, key, id) in lengths {
            if let Some(v) = num(ui, label, id, s[key].as_f64().unwrap_or(0.0), " mm", 2) {
                app.run("embroidery.settings", json!({ key: v })).ok();
            }
        }
        for (label, key, id, decimals) in [
            (tl!("Stitches per minute"), "machineSpeed", "spm", 0),
            (tl!("Seconds per trim"), "trimSeconds", "tsec", 1),
            (tl!("Seconds per color change"), "colorChangeSeconds", "ccsec", 0),
        ] {
            if let Some(v) = num(ui, label, id, s[key].as_f64().unwrap_or(0.0), "", decimals) {
                app.run("embroidery.settings", json!({ key: v })).ok();
            }
        }
        let styles: Vec<&str> = TieStyle::ALL.iter().map(|t| tl!(t.label())).collect();
        let cur = TieStyle::ALL.iter().position(|t| s["tieStyle"].as_str() == Some(t.id())).unwrap_or(1);
        if let Some(i) = pick(ui, tl!("Tie style"), "tiestyle", cur, &styles)
            && let Some(t) = TieStyle::ALL.get(i)
        {
            app.run("embroidery.settings", json!({ "tieStyle": t.id() })).ok();
        }
        ui.horizontal(|ui| {
            if let Some(v) = check(ui, tl!("Tie in"), s["tieIn"].as_bool().unwrap_or(true)) {
                app.run("embroidery.settings", json!({ "tieIn": v })).ok();
            }
            if let Some(v) = check(ui, tl!("Tie off"), s["tieOff"].as_bool().unwrap_or(true)) {
                app.run("embroidery.settings", json!({ "tieOff": v })).ok();
            }
        });
        machine_profile_rows(app, ui, &machines, &current, info["machineFormat"].as_str().unwrap_or("dst"));
    });
    // a basting run sewn first, round the design or inside the hoop
    let b = &s["basting"];
    if let Some(v) = check(ui, tl!("Basting first"), b.is_object()) {
        app.run("embroidery.settings", json!({ "basting": v })).ok();
    }
    if b.is_object() {
        let cur = usize::from(b["hoop"].as_bool() == Some(true));
        if let Some(i) = pick(ui, tl!("Basting round"), "basting-round", cur, &[tl!("The design"), tl!("The hoop")]) {
            app.run("embroidery.settings", json!({ "basting": { "hoop": i == 1 } })).ok();
        }
        if let Some(v) = num(ui, tl!("Basting margin"), "basting-margin", b["margin"].as_f64().unwrap_or(5.0), " mm", 1) {
            app.run("embroidery.settings", json!({ "basting": { "margin": v.clamp(0.0, 50.0) } })).ok();
        }
    }
    // where sewing starts and where the frame ends
    let points = vectorcraft_embroidery::sf_core::model::DesignPoint::ALL;
    let mut names = vec![tl!("Where it falls")];
    names.extend([
        tl!("Centre"),
        tl!("Top left"),
        tl!("Top"),
        tl!("Top right"),
        tl!("Left"),
        tl!("Right"),
        tl!("Bottom left"),
        tl!("Bottom"),
        tl!("Bottom right"),
    ]);
    for (label, key, id) in [(tl!("Start at"), "start", "design-start"), (tl!("End at"), "end", "design-end")] {
        let cur = s[key].as_str().and_then(|v| points.iter().position(|q| q.id() == v)).map_or(0, |i| i + 1);
        if let Some(i) = pick(ui, label, id, cur, &names) {
            let v = i.checked_sub(1).and_then(|k| points.get(k)).map_or(Value::Null, |q| json!(q.id()));
            app.run("embroidery.settings", json!({ key: v })).ok();
        }
    }
    colorway_rows(app, ui);
    // the cost estimate's prices
    let cost = &s["cost"];
    if let Some(v) = check(ui, tl!("Estimate cost"), cost.is_object()) {
        app.run("embroidery.settings", json!({ "cost": v })).ok();
    }
    if cost.is_object() {
        egui::CollapsingHeader::new(tl!("Cost settings")).id_salt("emb-cost-settings").show(ui, |ui| {
            for (label, key, id, decimals) in [
                (tl!("Per 1000 stitches"), "perThousand", "cost-k", 2),
                (tl!("Machine time an hour"), "perHour", "cost-h", 2),
                (tl!("Thread per 1000 m"), "threadPerKm", "cost-t", 2),
                (tl!("Set-up an order"), "setup", "cost-s", 2),
                (tl!("Machine heads"), "heads", "cost-heads", 0),
            ] {
                if let Some(v) = num(ui, label, id, cost[key].as_f64().unwrap_or(0.0), "", decimals) {
                    let v = if key == "heads" { v.round().clamp(1.0, 100.0) } else { v.max(0.0) };
                    app.run("embroidery.settings", json!({ "cost": { key: v } })).ok();
                }
            }
            let symbols = ["$", "€", "£", "¥", "kr ", "CHF ", "R$", "zł "];
            let now = cost["currency"].as_str().unwrap_or("$");
            let cur = symbols.iter().position(|c| *c == now).unwrap_or(0);
            if let Some(i) = pick_names(ui, tl!("Currency"), "cost-currency", cur, &symbols)
                && let Some(c) = symbols.get(i)
            {
                app.run("embroidery.settings", json!({ "cost": { "currency": c } })).ok();
            }
        });
    }
    // notes kept with the design, set when the field is left
    let saved = info["notes"].as_str().unwrap_or_default().to_string();
    let mut text = app.embroidery.notes_draft.clone().unwrap_or_else(|| saved.clone());
    ui.label(tl!("Design notes"));
    let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2).hint_text(tl!("Printed on worksheets")));
    if r.lost_focus() {
        app.embroidery.notes_draft = None;
        if text.trim() != saved {
            app.run("embroidery.settings", json!({ "notes": text })).ok();
        }
    } else if r.has_focus() {
        app.embroidery.notes_draft = Some(text);
    }
}

/// The design's colorways: the one in use, a new one or deleting it, and its threads.
fn colorway_rows(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Ok(l) = app.run("embroidery.colorway.list", json!({})) else { return };
    let slots = l["slots"].as_array().cloned().unwrap_or_default();
    if slots.is_empty() {
        return;
    }
    let colorways = l["colorways"].as_array().cloned().unwrap_or_default();
    let active = l["active"].as_u64().unwrap_or(0) as usize;
    let mut names: Vec<String> = vec![tl!("The design's own").to_string()];
    names.extend(colorways.iter().map(|c| c["name"].as_str().unwrap_or_default().to_string()));
    let opts: Vec<&str> = names.iter().map(String::as_str).collect();
    if let Some(i) = pick_names(ui, tl!("Colorway"), "colorway", active, &opts) {
        app.run("embroidery.colorway.activate", json!({ "index": i })).ok();
    }
    ui.horizontal(|ui| {
        if button(ui, tl!("New Colorway")).clicked() {
            app.run("embroidery.colorway.add", json!({})).ok();
        }
        if active > 0 && button(ui, tl!("Delete Colorway")).clicked() {
            app.run("embroidery.colorway.remove", json!({})).ok();
        }
    });
    // the active colorway's thread for each of the design's colours
    let Some(cw) = active.checked_sub(1).and_then(|i| colorways.get(i)) else { return };
    for (slot, own) in slots.iter().enumerate() {
        let hex = cw["threads"][slot]["color"].as_str().or_else(|| own["color"].as_str()).unwrap_or("#000000");
        let Some(c) = Rgb::from_hex(hex) else { continue };
        let mut rgb = [c.r, c.g, c.b];
        ui.horizontal(|ui| {
            ui.label(own["name"].as_str().unwrap_or_default());
            ui.label("→");
            if egui::color_picker::color_edit_button_srgb(ui, &mut rgb).changed() {
                let hex = vectorcraft_embroidery::sf_core::Rgb::new(rgb[0], rgb[1], rgb[2]).to_hex();
                app.run("embroidery.colorway.setThread", json!({ "slot": slot, "color": hex })).ok();
            }
        });
    }
}

/// Keeping the design's machine settings in its machine's profile or a new one of the user's;
/// renaming the user's own, and putting a standard one back to how it ships.
fn machine_profile_rows(app: &mut VectorcraftApp, ui: &mut Ui, machines: &[Value], current: &str, format: &str) {
    let profile = machines.iter().find(|m| m["name"].as_str().is_some_and(|n| !current.is_empty() && n.eq_ignore_ascii_case(current)));
    let own = profile.filter(|m| m["standard"].as_bool() != Some(true));
    ui.add_space(4.0);
    if let Some(m) = own {
        let name = m["name"].as_str().unwrap_or_default().to_string();
        label_row(ui, tl!("Name"), LABEL_W, |ui| {
            let key = egui::Id::unique("emb-machine-name");
            let mut text = ui.data(|d| d.get_temp::<String>(key)).unwrap_or_else(|| name.clone());
            let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(130.0));
            if resp.lost_focus() {
                let new = text.trim();
                if !new.is_empty() && new != name {
                    app.run("embroidery.machine.save", json!({ "name": name, "newName": new })).ok();
                }
                ui.data_mut(|d| d.remove::<String>(key));
            } else if resp.has_focus() {
                ui.data_mut(|d| d.insert_temp(key, text));
            }
        });
        let formats: Vec<String> =
            machines.iter().filter(|m| m["standard"].as_bool() == Some(true)).filter_map(|m| m["format"].as_str().map(str::to_uppercase)).collect();
        let opts: Vec<&str> = formats.iter().map(String::as_str).collect();
        let cur = formats.iter().position(|f| f.eq_ignore_ascii_case(m["format"].as_str().unwrap_or_default())).unwrap_or(0);
        if let Some(i) = pick_names(ui, tl!("Format"), "machine-format", cur, &opts)
            && let Some(f) = formats.get(i)
        {
            app.run("embroidery.machine.save", json!({ "name": name, "format": f.to_lowercase() })).ok();
        }
    }
    ui.horizontal_wrapped(|ui| {
        if let Some(m) = profile {
            let name = m["name"].as_str().unwrap_or_default().to_string();
            if button(ui, tl!("Save to Machine")).on_hover_text(tl!("Keep these settings in the machine's profile, for new designs")).clicked() {
                app.run("embroidery.machine.save", json!({ "name": name, "fromDesign": true })).ok();
            }
            if m["edited"].as_bool() == Some(true)
                && button(ui, tl!("Reset Machine")).on_hover_text(tl!("Put the machine's profile back to how it ships, and use it")).clicked()
                && app.run("embroidery.machine.delete", json!({ "name": name })).is_ok()
            {
                app.run("embroidery.settings", json!({ "machine": name })).ok();
            }
            if own.is_some() && button(ui, tl!("Delete Machine")).clicked() {
                app.run("embroidery.machine.delete", json!({ "name": name })).ok();
            }
        }
        if button(ui, tl!("Save as New Machine")).on_hover_text(tl!("A machine profile of your own with these settings")).clicked() {
            // a copy of one of the user's is numbered; a machine the design names but that isn't
            // kept here comes back under its name
            let (base, first) = match (profile, own) {
                (_, Some(m)) => (m["name"].as_str().unwrap_or_default().trim_end_matches(|c: char| c.is_ascii_digit()).trim_end().to_string(), 2),
                (None, None) if !current.is_empty() => (current.to_string(), 1),
                _ => (tl!("My Machine").to_string(), 1),
            };
            let taken = |n: &str| machines.iter().any(|m| m["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n)));
            let fresh = (first..).map(|k| if k == 1 { base.clone() } else { format!("{base} {k}") }).find(|n| !taken(n)).unwrap_or_default();
            if app.run("embroidery.machine.save", json!({ "name": fresh, "format": format, "fromDesign": true })).is_ok() {
                app.run("embroidery.settings", json!({ "machine": fresh })).ok();
            }
        }
    });
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    let open = app.embroidery.widget.open;
    if ui.button(if open { tl!("Hide Digitizing Widget") } else { tl!("Digitizing Widget") }).clicked() {
        app.embroidery.widget.open = !open;
        if !open {
            crate::embroidery::widget::load(app);
        }
        ui.close();
    }
    if ui.button(tl!("Convert Artwork to Embroidery")).clicked() {
        app.run("embroidery.convert", json!({})).ok();
        ui.close();
    }
    if ui.button(tl!("Auto Direction")).clicked() {
        app.run("embroidery.autoDirection", json!({})).ok();
        ui.close();
    }
}

/// Check Design: runs `embroidery.check`, keeps the findings for the panel and
/// shows the panel → how many it found.
pub fn run_check(app: &mut VectorcraftApp) -> Result<usize, String> {
    let r = app.run("embroidery.check", json!({}))?;
    let findings = r["findings"].as_array().cloned().unwrap_or_default();
    let n = findings.len();
    app.embroidery.checks = Some(findings);
    // show the findings: open the panel (Window › Embroidery toggles, so not when it is open)
    if app.ui.open_panel.as_deref() != Some(ID) {
        app.run("window.panel", json!({ "panel": ID })).ok();
    }
    Ok(n)
}

/// A finding in the interface language (the engine's message is English).
fn finding_text(f: &Value) -> String {
    let num = |k: &str| f[k].as_f64().map(|v| format!("{v:.1}")).unwrap_or_default();
    match f["kind"].as_str().unwrap_or_default() {
        "dense" => crate::i18n::fmt(tl!("Very dense: about {n} layers of thread here"), &[("n", &num("value"))]),
        "longStitches" => crate::i18n::fmt(tl!("Stitches up to {n} mm long can snag"), &[("n", &num("value"))]),
        "shortStitches" => crate::i18n::fmt(
            tl!("{n} very short stitches can break the thread"),
            &[("n", &f["value"].as_f64().map(|v| format!("{v:.0}")).unwrap_or_default())],
        ),
        "narrowSatin" => crate::i18n::fmt(tl!("Satin only {n} mm wide sinks into the fabric"), &[("n", &num("value"))]),
        "shortFill" => crate::i18n::fmt(tl!("Fill stitches {n} mm long can ripple after washing"), &[("n", &num("value"))]),
        "tiny" => tl!("Tiny object: may not sew cleanly").to_string(),
        "smallLettering" => crate::i18n::fmt(tl!("Lettering only {n} mm tall is hard to sew clearly"), &[("n", &num("value"))]),
        "noStitches" => tl!("Makes no stitches").to_string(),
        "hoop" => tl!("The design is larger than the hoop").to_string(),
        _ => f["message"].as_str().unwrap_or_default().to_string(),
    }
}

/// The Check Design button and its findings: click one to select its objects and
/// bring the spot into view.
fn checks(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    ui.add_space(4.0);
    let label = if app.embroidery.checks.is_some() { tl!("Check Again") } else { tl!("Check Design") };
    if button(ui, label).on_hover_text(tl!("Look for dense spots, long stitches, tiny objects and small lettering")).clicked() {
        run_check(app).ok();
    }
    // what counts as a problem: the design's own limits
    egui::CollapsingHeader::new(tl!("Check settings")).id_salt("emb-check-limits").show(ui, |ui| {
        let lim = app.session.active().and_then(|st| st.doc.embroidery.as_deref().map(|e| e.checks.clone())).unwrap_or_default();
        if let Some(v) = check(ui, tl!("Sewn on clothing"), lim.garment) {
            app.run("embroidery.settings", json!({ "checks": { "garment": v } })).ok();
        }
        ui.label(egui::RichText::new(tl!("Long stitches catch from 7 mm on clothing, 12.1 mm on other items")).small());
        for (label, key, id, value, suffix) in [
            (tl!("Dense from (layers)"), "denseLayers", "chk-dense", lim.dense_layers, ""),
            (tl!("Long stitches from"), "longStitch", "chk-long", lim.long_stitch, " mm"),
            (tl!("Short stitches under"), "shortStitch", "chk-short", lim.short_stitch, " mm"),
            (tl!("Narrow satin under"), "narrowSatin", "chk-narrow", lim.narrow_satin, " mm"),
            (tl!("Short fill stitches under"), "shortFill", "chk-fill", lim.short_fill, " mm"),
            (tl!("Small lettering under"), "smallLettering", "chk-text", lim.small_lettering, " mm"),
            (tl!("Tiny objects under"), "tinyObject", "chk-tiny", lim.tiny_object, " mm"),
        ] {
            if let Some(v) = num(ui, label, id, value, suffix, 1) {
                app.run("embroidery.settings", json!({ "checks": { key: v } })).ok();
            }
        }
    });
    let Some(findings) = app.embroidery.checks.clone() else { return };
    if findings.is_empty() {
        ui.label(egui::RichText::new(tl!("No problems found.")).color(t.accent));
        return;
    }
    for f in &findings {
        let warning = f["severity"] == "warning";
        let (mark, color) = if warning { ("▲", t.warning) } else { ("●", t.text_dim) };
        let text = egui::RichText::new(format!("{mark} {}", finding_text(f))).color(color);
        let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(tl!("Click to select and show it"));
        if resp.clicked() {
            let ids: Vec<u64> = f["ids"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
            if !ids.is_empty() {
                app.run("select.set", json!({ "ids": ids })).ok();
            }
            if let Some(at) = f["at"].as_array()
                && let (Some(x), Some(y)) = (at.first().and_then(Value::as_f64), at.get(1).and_then(Value::as_f64))
                && let Some(v) = app.view_mut()
            {
                v.center = vectorcraft_geom::Point::new(x, y);
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn finding_text_for_test(f: &Value) -> String {
    finding_text(f)
}
