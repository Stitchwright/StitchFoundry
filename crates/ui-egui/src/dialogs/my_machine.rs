//! My Machine (E4): choose the embroidery machine once — its hoops, its file format, inches or
//! millimetres and the folder or USB stick designs go to — and Save for My Machine writes the
//! design there in one step (`embroidery.myMachine`, `embroidery.send`), asking first which fabric
//! it goes on when nobody has said (`embroidery.applyFabric`).

use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};

/// The My Machine window.
#[derive(Clone, Debug, Default)]
pub struct MyMachineState {
    pub open: bool,
    /// The hoop of one's own being added: its name, width and height (in the units shown) and
    /// whether it is oval.
    pub hoop_name: String,
    pub hoop_width: String,
    pub hoop_height: String,
    pub hoop_oval: bool,
    /// Why the last hoop couldn't be added.
    pub hoop_error: Option<String>,
    /// Which fabric?, asked before saving: open, and the save's params to go on with.
    pub fabric_open: bool,
    pub fabric_then: Value,
    /// The designs (by title) the fabric question was answered "not sure" for this session.
    pub fabric_asked: Vec<String>,
}

/// A size typed in the units shown (`inches` or millimetres), in millimetres.
fn typed_mm(text: &str, inches: bool) -> Option<f64> {
    let v: f64 = text.trim().trim_end_matches(['"', '″']).trim().replace(',', ".").parse().ok()?;
    let mm = if inches { v * 25.4 } else { v };
    (mm.is_finite() && mm > 0.0).then_some(mm)
}

/// "My own hoops": the hoops of one's own, each with Remove, and a row to add one (its name,
/// width and height in the units chosen, square or oval).
fn own_hoops(app: &mut VectorcraftApp, ui: &mut egui::Ui, mine: &Value, t: &Tokens) {
    let inches = app.session.prefs.units_general == "inches";
    let show = |mm: f64| if inches { format!("{:.2}″", mm / 25.4) } else { format!("{mm:.0} mm") };
    ui.label(egui::RichText::new(tl!("My own hoops")).size(14.0));
    let own = mine["own"].as_array().cloned().unwrap_or_default();
    if own.is_empty() {
        ui.label(egui::RichText::new(tl!("Hoop not listed? Add its sewing field here.")).size(12.0).color(t.text_dim));
    }
    for h in &own {
        let (Some(name), Some(w), Some(ht)) = (h["name"].as_str(), h["width"].as_f64(), h["height"].as_f64()) else { continue };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{name}: {} × {}", show(w), show(ht))).size(13.0));
            if ui.small_button(tl!("Remove")).clicked() {
                app.run("embroidery.hoop.deleteCustom", json!({ "name": name })).ok();
            }
        });
    }
    let st = &mut app.ui.my_machine;
    let unit = if inches { tl!("in") } else { "mm" };
    ui.horizontal(|ui| {
        ui.label(tl!("Name"));
        ui.add(egui::TextEdit::singleline(&mut st.hoop_name).desired_width(120.0).hint_text(tl!("My hoop")));
        ui.label(tl!("Width"));
        ui.add(egui::TextEdit::singleline(&mut st.hoop_width).desired_width(44.0));
        ui.label(tl!("Height"));
        ui.add(egui::TextEdit::singleline(&mut st.hoop_height).desired_width(44.0));
        ui.label(unit);
    });
    ui.horizontal(|ui| {
        ui.radio_value(&mut st.hoop_oval, false, tl!("Square corners"));
        ui.radio_value(&mut st.hoop_oval, true, tl!("Oval"));
    });
    let mut add = None;
    if ui.button(tl!("Add My Hoop")).clicked() {
        let name = st.hoop_name.trim().to_string();
        add = match (typed_mm(&st.hoop_width, inches), typed_mm(&st.hoop_height, inches)) {
            _ if name.is_empty() => {
                st.hoop_error = Some(tl!("Give the hoop a name").into());
                None
            }
            (Some(w), Some(h)) => Some(json!({ "name": name, "width": w, "height": h, "shape": if st.hoop_oval { "oval" } else { "rect" } })),
            _ => {
                st.hoop_error = Some(tl!("Type the hoop's width and height as numbers").into());
                None
            }
        };
    }
    if let Some(p) = add {
        match app.run("embroidery.hoop.saveCustom", p) {
            Ok(_) => {
                let st = &mut app.ui.my_machine;
                (st.hoop_name, st.hoop_width, st.hoop_height, st.hoop_error) = (String::new(), String::new(), String::new(), None);
            }
            Err(e) => app.ui.my_machine.hoop_error = Some(e),
        }
    }
    if let Some(e) = &app.ui.my_machine.hoop_error {
        ui.label(egui::RichText::new(e.as_str()).size(12.0).color(t.error));
    }
}

/// `ui.myMachine`: opens the window.
pub fn open(app: &mut VectorcraftApp) -> Result<Value, String> {
    app.ui.my_machine.open = true;
    Ok(json!({ "open": true }))
}

/// The machine chosen, its format and folder (`embroidery.myMachine`).
fn info(app: &mut VectorcraftApp) -> Value {
    app.session.execute("embroidery.myMachine", &json!({})).unwrap_or_default()
}

/// `ui.saveForMyMachine`: the design written for My Machine in its folder. With no machine
/// chosen yet the My Machine window opens; with no folder, or the folder not there (the USB stick
/// out), a folder is picked first.
pub fn save(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    if app.session.active().is_none() {
        return Err("open or start a design first".into());
    }
    let mine = info(app);
    if mine["machine"].is_null() {
        app.ui.my_machine.open = true;
        return Ok(json!({ "open": true }));
    }
    // the stitches suit the fabric: asked once, when the design has none
    let title = app.session.active().map(|d| d.doc.title.clone()).unwrap_or_default();
    if let Some(name) = p.get("fabric").and_then(Value::as_str) {
        app.session.execute("embroidery.applyFabric", &json!({ "name": name })).map_err(|e| e.to_string())?;
    } else {
        let fabric = app.session.execute("embroidery.info", &json!({})).ok().and_then(|i| i["fabric"].as_str().map(str::to_string));
        if fabric.unwrap_or_default().is_empty() && !app.ui.my_machine.fabric_asked.contains(&title) {
            let st = &mut app.ui.my_machine;
            (st.fabric_open, st.fabric_then) = (true, p.clone());
            return Ok(json!({ "askFabric": true }));
        }
    }
    // the web app has no folders: the machine file downloads, to copy onto the USB stick
    if crate::io::is_web(app) && p.get("folder").is_none() {
        return download(app);
    }
    let ready = app.session.execute("embroidery.send.targets", &json!({})).ok().and_then(|t| t["targets"].get(0).and_then(|x| x["ready"].as_bool()));
    let folder = match p.get("folder").and_then(Value::as_str) {
        Some(f) => f.to_string(),
        None if ready == Some(true) => mine["folder"].as_str().unwrap_or_default().to_string(),
        // none yet, or not there: pick one (the answer runs this again)
        None => match crate::picks::folder(app) {
            Some(f) => f,
            None => return Ok(Value::Null),
        },
    };
    let r = app.session.execute("embroidery.send", &json!({ "folder": folder })).map_err(|e| e.to_string())?;
    let path = r["path"].as_str().unwrap_or_default();
    app.status(with_notes(format!("{} {}", tl!("Saved for your machine:"), path), &r));
    Ok(r)
}

/// `msg`, then what the machine may refuse or show otherwise (the file's warnings), if anything.
pub(crate) fn with_notes(msg: String, r: &Value) -> String {
    let notes: Vec<&str> = r["warnings"].as_array().map(|w| w.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    match notes.as_slice() {
        [] => msg,
        [one] => format!("{msg}. {} {one}", tl!("Note:")),
        [first, rest @ ..] => format!("{msg}. {} {first} (+{})", tl!("Note:"), rest.len()),
    }
}

/// Web: the design's machine file in My Machine's format, downloaded.
fn download(app: &mut VectorcraftApp) -> Result<Value, String> {
    let title = app.session.active().map(|d| d.doc.title.clone()).unwrap_or_default();
    let stem = vectorcraft_engine::cmd::fileio::file_stem(&title);
    let stem = if stem.trim().is_empty() { "Design".to_string() } else { stem };
    let r = app.session.execute("embroidery.export", &json!({ "center": true, "name": stem })).map_err(|e| e.to_string())?;
    let bytes = r["dataBase64"].as_str().and_then(vectorcraft_format::base64_decode).ok_or("the machine file didn't encode")?;
    let name = format!("{stem}.{}", r["format"].as_str().unwrap_or("dst"));
    crate::io::write_to(&mut app.services, &name, &bytes)?;
    app.status(with_notes(format!("{} {name}", tl!("Downloaded for your machine:")), &r));
    Ok(json!({ "downloaded": name, "format": r["format"], "bytes": bytes.len() }))
}

/// Which fabric?, before Save for My Machine goes on: a fabric chosen is applied, "not sure"
/// leaves the design as it is; either way the save goes on.
fn fabric_window(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.ui.my_machine.fabric_open {
        return;
    }
    let mut open = true;
    let mut chosen = None;
    egui::Window::new(tl!("Which fabric?"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(260.0, 240.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.set_width(520.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            chosen = crate::guide::fabric_choice(app, ui, &t, None);
        });
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.ui.my_machine.fabric_open = false;
        return;
    }
    let Some(choice) = chosen else { return };
    let st = &mut app.ui.my_machine;
    st.fabric_open = false;
    let mut then = std::mem::take(&mut st.fabric_then);
    if !then.is_object() {
        then = json!({});
    }
    match choice {
        Some(name) => then["fabric"] = json!(name),
        None => {
            let title = app.session.active().map(|d| d.doc.title.clone()).unwrap_or_default();
            app.ui.my_machine.fabric_asked.push(title);
        }
    }
    if let Err(e) = app.run("ui.saveForMyMachine", then) {
        app.status(e);
    }
}

/// The window, while open.
pub(super) fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    fabric_window(app, ctx);
    if !app.ui.my_machine.open {
        return;
    }
    let mut open = true;
    let mut done = false;
    let mine = info(app);
    egui::Window::new(tl!("My Machine"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(240.0, 220.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.set_width(460.0);
            ui.label(egui::RichText::new(tl!("Which embroidery machine do you have?")).font(theme::semibold(15.0)).color(t.text));
            ui.add_space(6.0);
            let current = mine["machine"].as_str().unwrap_or_default().to_string();
            for m in mine["machines"].as_array().map(Vec::as_slice).unwrap_or_default() {
                let Some(name) = m["name"].as_str() else { continue };
                let label = egui::RichText::new(machine_name(name)).size(15.0);
                if ui.selectable_label(name == current, label).clicked() {
                    app.run("embroidery.myMachine.set", json!({ "machine": name })).ok();
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tl!("Measure in")).size(14.0));
                let units = app.session.prefs.units_general.clone();
                for (id, label) in [("millimeters", tl!("Millimeters")), ("inches", tl!("Inches"))] {
                    if ui.selectable_label(units == id, egui::RichText::new(label).size(14.0)).clicked() {
                        app.session.execute("prefs.set", &json!({ "key": "unitsGeneral", "value": id })).ok();
                    }
                }
            });
            ui.add_space(10.0);
            if crate::io::is_web(app) {
                ui.label(
                    egui::RichText::new(tl!("Save for My Machine downloads the design: copy it to the USB stick you take to your machine."))
                        .size(12.0)
                        .color(t.text_dim),
                );
            } else {
                ui.label(egui::RichText::new(tl!("Save designs to")).size(14.0));
                ui.horizontal(|ui| {
                    let folder = mine["folder"].as_str().map(str::to_string);
                    ui.label(egui::RichText::new(folder.unwrap_or_else(|| tl!("No folder chosen yet").into())).color(t.text_dim));
                    if ui.button(tl!("Choose Folder…")).clicked() {
                        crate::picks::button(app, |app| {
                            if let Some(f) = crate::picks::folder(app) {
                                app.session.execute("embroidery.send.targets", &json!({ "add": f })).ok();
                            }
                        });
                    }
                });
                ui.label(
                    egui::RichText::new(tl!("Plug in the USB stick you take to your machine, then choose it here.")).size(12.0).color(t.text_dim),
                );
            }
            ui.add_space(10.0);
            own_hoops(app, ui, &mine, &t);
            ui.add_space(12.0);
            if crate::widgets::primary_button(ui, tl!("Done")).clicked() {
                done = true;
            }
        });
    if !open || done || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        app.ui.my_machine.open = false;
    }
}

/// A machine's name in the UI language (the brands stay as they are).
pub fn machine_name(name: &str) -> &str {
    match name {
        "Husqvarna Viking (other models)" => tl!("Husqvarna Viking (other models)"),
        "Brother or Baby Lock" => tl!("Brother or Baby Lock"),
        "Janome or Elna" => tl!("Janome or Elna"),
        "Another machine (DST)" => tl!("Another machine (DST)"),
        "Brother or Baby Lock multi-needle" => tl!("Brother or Baby Lock multi-needle"),
        "Commercial multi-needle (DST)" => tl!("Commercial multi-needle (DST)"),
        other => other,
    }
}
