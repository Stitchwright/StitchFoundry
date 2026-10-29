use super::{CornerRadius, Tokens, Ui, VectorcraftApp, json, theme, vec2, widgets};

/// The Home screen (`app.home`; with no document open, unless its preference is off). It
/// scrolls when it is taller than the window (a small window, a tablet, a bigger text size).
pub(super) fn home(app: &mut VectorcraftApp, ui: &mut Ui, rect: egui::Rect) {
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, 0.0, t.panel_darker);
    // Taller than the window (a small window, many recent files): it scrolls. Widgets scrolled out
    // of view are clipped, so they take no pointer.
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min)));
    egui::ScrollArea::vertical().id_salt("home-scroll").auto_shrink([false, false]).show(&mut child, |ui| {
        let side = (ui.available_width() - 820.0).max(40.0) / 2.0;
        let width = (ui.available_width() - 2.0 * side).max(0.0);
        ui.add_space(60.0);
        ui.horizontal_top(|ui| {
            ui.add_space(side);
            ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(width);
                home_body(app, ui);
            });
        });
        ui.add_space(60.0);
    });
}

/// The Home screen's column: welcome, New / Open, Recent Files, presets and community links.
fn home_body(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(tl!("Welcome to StitchFoundry")).font(theme::semibold(26.0)).color(t.text));
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tl!("Turn your words, shapes and pictures into designs for your embroidery machine.")).size(14.0).color(t.text_dim));
    ui.add_space(22.0);
    ui.horizontal(|ui| {
        // the step-by-step guide first: the easiest way in
        if widgets::primary_button(ui, tl!("Make a Design, Step by Step")).clicked() {
            app.run("ui.makeDesign", json!({})).ok();
        }
        ui.add_space(8.0);
        if widgets::secondary_button(ui, tl!("New file")).clicked() {
            app.run("file.newDialog", json!({})).ok();
        }
        ui.add_space(8.0);
        if widgets::secondary_button(ui, tl!("Open")).clicked() {
            app.run("file.open", json!({})).ok();
        }
    });
    ui.add_space(18.0);
    crate::updates::home_card(app, ui, &t);
    crate::embroidery::account::home_card(app, ui, &t);
    ui.add_space(14.0);
    experience_row(app, ui, &t);
    recent(app, ui);
    ui.add_space(24.0);
    ui.label(egui::RichText::new(tl!("Start a design for your hoop")).font(theme::semibold(14.0)).color(t.text));
    ui.add_space(6.0);
    // My Machine: its name, and its hoops in place of the general ones
    let mine = app.session.execute("embroidery.myMachine", &json!({})).unwrap_or_default();
    ui.horizontal_wrapped(|ui| match mine["machine"].as_str() {
        Some(m) => {
            ui.label(egui::RichText::new(format!("{} {}", tl!("My machine:"), crate::dialogs::my_machine::machine_name(m))).color(t.text_dim));
            if ui.link(tl!("Change")).clicked() {
                app.run("ui.myMachine", json!({})).ok();
            }
        }
        None => {
            ui.label(egui::RichText::new(tl!("Tell us your machine to see its hoops and save in its format.")).color(t.text_dim));
            if ui.link(tl!("Choose My Machine")).clicked() {
                app.run("ui.myMachine", json!({})).ok();
            }
        }
    });
    ui.add_space(8.0);
    let hoops = home_hoops(&mine);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
        for (label, hoop, w, h) in &hoops {
            if hoop_card(ui, &t, label, (*w, *h)).clicked() {
                app.run("embroidery.newDesign", json!({ "hoop": hoop })).ok();
            }
        }
    });
    ui.add_space(24.0);
    ui.label(egui::RichText::new(tl!("Your first design")).font(theme::semibold(14.0)).color(t.text));
    ui.add_space(8.0);
    let steps = [
        (tl!("Draw shapes, place a picture or type text."), "quick-start"),
        (tl!("Choose how each part is stitched: satin, fill, run and more."), "stitch-types"),
        (tl!("Watch it sew with the stitch player and check it."), "stitch-sequence-and-the-stitch-player"),
        (tl!("Export the file for your machine."), "machine-files"),
    ];
    if ui.link(tl!("New to digitizing? Read the basics")).clicked() {
        app.run("help.show", json!({ "topic": "digitizing-basics" })).ok();
    }
    ui.add_space(4.0);
    for (i, (step, topic)) in steps.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{}", i + 1)).font(theme::semibold(14.0)).color(t.accent));
            ui.label(egui::RichText::new(*step).color(t.text));
            if ui.link(tl!("Learn how")).clicked() {
                app.run("help.show", json!({ "topic": topic })).ok();
            }
        });
    }
    ui.add_space(24.0);
    ui.label(egui::RichText::new(tl!("Try a sample")).font(theme::semibold(14.0)).color(t.text));
    ui.add_space(8.0);
    let samples = app.session.execute("embroidery.samples", &json!({})).unwrap_or_default();
    let samples = samples["samples"].as_array().cloned().unwrap_or_default();
    // one picture made a frame, so Home opens at once and fills in
    if let Some(id) = samples.iter().filter_map(|sm| sm["id"].as_str()).find(|id| !app.embroidery.sample_pictures.contains_key(*id)) {
        let picture = sample_picture(ui.ctx(), id);
        app.embroidery.sample_pictures.insert(id.to_string(), picture);
        ui.ctx().request_repaint();
    }
    ui.horizontal_wrapped(|ui| {
        for sm in &samples {
            let (Some(id), Some(name)) = (sm["id"].as_str(), sm["name"].as_str()) else { continue };
            let picture = app.embroidery.sample_pictures.get(id).cloned().flatten();
            let r = sample_card(ui, &t, sample_name(name), picture.as_ref()).on_hover_text(sample_about(sm["about"].as_str().unwrap_or_default()));
            if r.clicked() {
                app.run("embroidery.sample", json!({ "id": id })).ok();
            }
        }
    });
    // a tip: a different one as recent files come and go
    let tips = crate::dialogs::help::tips();
    let general: Vec<&str> = tips.iter().filter(|(k, _)| k.is_empty()).map(|(_, x)| *x).collect();
    let n = crate::io::recent_files(app).len();
    if let Some(tip) = general.get(n % general.len().max(1)) {
        ui.add_space(20.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(tl!("Tip:")).font(theme::semibold(13.0)).color(t.accent));
            ui.label(egui::RichText::new(*tip).color(t.text_dim));
            if ui.link(tl!("More tips")).clicked() {
                app.run("help.show", json!({ "tips": true })).ok();
            }
        });
    }
    ui.add_space(28.0);
    ui.label(egui::RichText::new(tl!("Community")).font(theme::semibold(14.0)).color(t.text));
    ui.add_space(10.0);
    crate::community::links(app, ui);
}

/// How do you work? (Window › Experience): each level a button, the chosen one lit; it sets how
/// much the embroidery tools show and the workspace for it.
fn experience_row(app: &mut VectorcraftApp, ui: &mut Ui, t: &Tokens) {
    use crate::state::Experience;
    let levels = [
        (Experience::Beginner, tl!("New to embroidery"), tl!("The basic tools and everyday sections, nothing more")),
        (Experience::Digitizer, tl!("I digitize designs"), tl!("Every digitizing tool, rulers and the control bar")),
        (Experience::Business, tl!("I run an embroidery business"), tl!("Everything, with job orders and batch output")),
    ];
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tl!("How do you work?")).font(theme::semibold(13.0)).color(t.text));
        ui.add_space(6.0);
        for (level, label, hint) in levels {
            if ui.selectable_label(app.ui.experience == level, label).on_hover_text(hint).clicked() {
                app.run("window.experience", json!({ "level": level.id() })).ok();
            }
        }
        if app.ui.experience != Experience::All && ui.link(tl!("Show everything")).clicked() {
            app.run("window.experience", json!({ "level": "all" })).ok();
        }
    });
}

/// The hoops a new design is offered in: My Machine's (`mine`, as `embroidery.myMachine` gives
/// it), else the usual ones; each label, hoop name, width × height (mm).
pub(crate) fn home_hoops(mine: &serde_json::Value) -> Vec<(String, String, f64, f64)> {
    let theirs: Vec<(String, String, f64, f64)> = mine["hoops"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|h| {
            let (name, w, ht) = (h["name"].as_str()?, h["width"].as_f64()?, h["height"].as_f64()?);
            Some((hoop_label(w, ht), name.to_string(), w, ht))
        })
        .collect();
    if !theirs.is_empty() {
        // the machine's hoops, one's own among them
        return theirs;
    }
    // no machine chosen: the usual sizes, then one's own hoops
    let own = mine["own"].as_array().map(Vec::as_slice).unwrap_or_default().iter().filter_map(|h| {
        let (name, w, ht) = (h["name"].as_str()?, h["width"].as_f64()?, h["height"].as_f64()?);
        Some((hoop_label(w, ht), name.to_string(), w, ht))
    });
    HOME_HOOPS.iter().map(|(label, hoop, w, h)| ((*label).to_string(), (*hoop).to_string(), *w, *h)).chain(own).collect()
}

/// A hoop card's label: its sewing field in millimetres and inches (`240 × 150 mm (9.4 × 5.9″)`).
fn hoop_label(w: f64, h: f64) -> String {
    let inch = |mm: f64| {
        let i = mm / 25.4;
        if (i - i.round()).abs() < 0.05 { format!("{}", i.round()) } else { format!("{i:.1}") }
    };
    format!("{w:.0} × {h:.0} mm ({}×{}″)", inch(w), inch(h))
}

/// The hoops the Home screen offers a new design for: label, hoop name, width × height (mm).
const HOME_HOOPS: [(&str, &str, f64, f64); 5] = [
    ("100 × 100 mm (4×4″)", "Brother 100 × 100 (4×4)", 100.0, 100.0),
    ("130 × 180 mm (5×7″)", "Brother 130 × 180 (5×7)", 130.0, 180.0),
    ("160 × 260 mm (6×10″)", "Brother 160 × 260 (6×10)", 160.0, 260.0),
    ("200 × 200 mm (8×8″)", "Brother 200 × 200 (8×8)", 200.0, 200.0),
    ("200 × 300 mm (8×12″)", "Brother 200 × 300 (8×12)", 200.0, 300.0),
];

/// A sample design's name in the UI language.
pub(crate) fn sample_name(name: &str) -> &str {
    match name {
        "Heart patch" => tl!("Heart patch"),
        "Sun and moon" => tl!("Sun and moon"),
        "Appliqué star" => tl!("Appliqué star"),
        "Spiro flower" => tl!("Spiro flower"),
        "Tartan heart" => tl!("Tartan heart"),
        "Lace star" => tl!("Lace star"),
        "Wavy leaf" => tl!("Wavy leaf"),
        "Mug rug" => tl!("Mug rug"),
        "Quilt block" => tl!("Quilt block"),
        other => other,
    }
}

/// What a sample design shows, in the UI language.
pub(crate) fn sample_about(about: &str) -> &str {
    match about {
        "A fill with a satin border round it, like a patch" => tl!("A fill with a satin border round it, like a patch"),
        "Clipart turned into fills, one colour after another" => tl!("Clipart turned into fills, one colour after another"),
        "Appliqué: stop, lay the fabric, tack it down, trim, satin edge" => tl!("Appliqué: stop, lay the fabric, tack it down, trim, satin edge"),
        "Spirograph loops in three colours, triple running stitch" => tl!("Spirograph loops in three colours, triple running stitch"),
        "A heart filled with a woven-looking tartan" => tl!("A heart filled with a woven-looking tartan"),
        "Free-standing lace: sew it on water-soluble stabilizer" => tl!("Free-standing lace: sew it on water-soluble stabilizer"),
        "A fill whose rows follow a wave, blending two greens" => tl!("A fill whose rows follow a wave, blending two greens"),
        "An in-the-hoop project, made start to finish in the hoop" => tl!("An in-the-hoop project, made start to finish in the hoop"),
        "A 5-inch quilt block, a heart in the middle, stippled round it" => tl!("A 5-inch quilt block, a heart in the middle, stippled round it"),
        other => other,
    }
}

/// A sample design's stitches as a small picture, made in a session of its own so the open
/// documents are left alone.
fn sample_picture(ctx: &egui::Context, id: &str) -> Option<egui::TextureHandle> {
    let mut s = vectorcraft_engine::Session::new();
    s.execute("embroidery.sample", &json!({ "id": id })).ok()?;
    let r = s.execute("embroidery.exportImage", &json!({ "ppmm": 1.5, "margin": 2, "view": "trueview" })).ok()?;
    let bytes = r["dataBase64"].as_str().and_then(vectorcraft_format::base64_decode)?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
    Some(ctx.load_texture(format!("sample-{id}"), color, egui::TextureOptions::LINEAR))
}

/// A Home screen card for a sample design: its stitches, its name under them.
pub(crate) fn sample_card(ui: &mut Ui, t: &Tokens, name: &str, picture: Option<&egui::TextureHandle>) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(128.0, 128.0), egui::Sense::click());
    let hovered = resp.hovered();
    ui.painter().rect_filled(rect, 6.0, t.panel);
    ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, if hovered { t.accent } else { t.border }), egui::StrokeKind::Inside);
    let field = egui::Rect::from_min_max(rect.min + vec2(10.0, 8.0), egui::pos2(rect.max.x - 10.0, rect.max.y - 28.0));
    match picture {
        Some(tex) => {
            let [w, h] = tex.size().map(|v| v.max(1) as f32);
            let scale = (field.width() / w).min(field.height() / h);
            let shown = egui::Rect::from_center_size(field.center(), vec2(w * scale, h * scale));
            ui.painter().image(tex.id(), shown, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(field.shrink(16.0), 4.0, t.border);
        }
    }
    ui.painter().text(
        egui::pos2(rect.center().x, rect.max.y - 14.0),
        egui::Align2::CENTER_CENTER,
        name,
        egui::FontId::proportional(12.0),
        if hovered { t.accent } else { t.text },
    );
    resp
}

/// A Home screen card for a hoop: its field drawn to scale, its size under it.
pub(crate) fn hoop_card(ui: &mut Ui, t: &Tokens, label: &str, (w, h): (f64, f64)) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(128.0, 128.0), egui::Sense::click());
    let hovered = resp.hovered();
    ui.painter().rect_filled(rect, 6.0, t.panel);
    ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, if hovered { t.accent } else { t.border }), egui::StrokeKind::Inside);
    // the field, 72 px at most each way, inside the hoop's rounded frame
    let k = (72.0 / w.max(h).max(1.0)) as f32;
    let field = egui::Rect::from_center_size(rect.center() - vec2(0.0, 12.0), vec2(w as f32 * k, h as f32 * k));
    ui.painter().rect_stroke(field.expand(5.0), 8.0, egui::Stroke::new(3.0, t.text_dim), egui::StrokeKind::Middle);
    ui.painter().rect_filled(field, 2.0, t.panel_darker);
    ui.painter().text(
        egui::pos2(rect.center().x, rect.bottom() - 12.0),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        t.text,
    );
    resp.on_hover_text(tl!("A new design the size of this hoop's sewing field"))
}

/// The Home screen's Recent Files (#663): the first of File › Open Recent Files, each its name and
/// folder; a click opens it. Nothing when there are none.
fn recent(app: &mut VectorcraftApp, ui: &mut Ui) {
    const SHOWN: usize = 6;
    let files: Vec<String> = crate::io::recent_files(app).iter().take(SHOWN).cloned().collect();
    if files.is_empty() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    ui.add_space(28.0);
    ui.label(egui::RichText::new(tl!("Recent Files")).font(theme::semibold(14.0)).color(t.text));
    ui.add_space(8.0);
    for path in files {
        if recent_row(ui, &path).clicked() {
            // A failure is reported to the user there.
            let _ = crate::io::open_recent_reporting(app, &path);
        }
    }
}

/// One Recent Files row across the Home screen: history icon, name (accent) and folder (dim,
/// truncated). The whole row is one button that lights up under the pointer; its tooltip is the
/// full path.
fn recent_row(ui: &mut Ui, path: &str) -> egui::Response {
    const PAD: f32 = 6.0;
    const ICON: f32 = 16.0;
    let t = Tokens::get(ui.ctx());
    let p = std::path::Path::new(path);
    let name = p.file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned());
    let folder = p.parent().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(4), t.hover);
        }
        let icon = egui::Rect::from_center_size(egui::pos2(rect.left() + PAD + ICON / 2.0, rect.center().y), vec2(ICON, ICON));
        crate::icons::paint(ui, "history", icon, t.icon);
        let x = icon.right() + 8.0;
        let name_galley = egui::WidgetText::from(egui::RichText::new(&name).size(13.0).color(t.accent)).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.right() - PAD - x).max(0.0),
            egui::TextStyle::Body,
        );
        let folder_x = x + name_galley.size().x + 10.0;
        ui.painter().galley(egui::pos2(x, rect.center().y - name_galley.size().y / 2.0), name_galley, t.accent);
        let room = rect.right() - PAD - folder_x;
        if room > 0.0 && !folder.is_empty() {
            let folder_galley = egui::WidgetText::from(egui::RichText::new(&folder).size(12.0).color(t.text_dim)).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                room,
                egui::TextStyle::Body,
            );
            ui.painter().galley(egui::pos2(folder_x, rect.center().y - folder_galley.size().y / 2.0), folder_galley, t.text_dim);
        }
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    resp.on_hover_text(path).on_hover_cursor(egui::CursorIcon::PointingHand)
}
