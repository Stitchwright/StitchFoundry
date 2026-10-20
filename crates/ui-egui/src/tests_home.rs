//! The Home icon shows the Home screen over open documents; Cmd+N opens the New Document dialog.

use serde_json::json;

use crate::{VectorcraftApp, canvas};

fn app_with_doc() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({})).unwrap();
    app
}

/// Draw one canvas frame; whether it drew the document (else the Home screen).
fn shows_document(app: &mut VectorcraftApp) -> bool {
    app.canvas_rect = None;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    app.canvas_rect.is_some()
}

#[test]
fn home_shows_over_open_documents_until_a_document_is_chosen() {
    let mut app = app_with_doc();
    assert!(shows_document(&mut app));
    app.run("app.home", json!({})).unwrap();
    assert!(!shows_document(&mut app), "Home replaces the canvas");
    assert_eq!(app.session.documents().len(), 1, "the document stays open");
    assert!(app.ui.dialog.is_none(), "Home is not the New Document dialog");
    // Choosing the document's tab returns to it.
    app.ui.home = None;
    assert!(shows_document(&mut app));
    // A new document (from Home's presets, New… or Open) replaces Home too.
    app.run("app.home", json!({})).unwrap();
    app.run("file.new", json!({})).unwrap();
    assert!(shows_document(&mut app));
    assert!(app.ui.home.is_none());
}

/// Draw one canvas frame; whether it drew any text (the Home screen's labels; an empty window
/// draws none).
fn draws_text(app: &mut VectorcraftApp) -> bool {
    fn has_text(s: &egui::Shape) -> bool {
        match s {
            egui::Shape::Text(_) => true,
            egui::Shape::Vec(v) => v.iter().any(has_text),
            _ => false,
        }
    }
    app.canvas_rect = None;
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    out.shapes.iter().any(|c| has_text(&c.shape))
}

/// General › Show The Home Screen When No Documents Are Open (#394): off, an app with no document
/// shows an empty window instead of the Home screen, and the Home button still opens it.
#[test]
fn home_screen_without_documents_follows_its_preference() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    assert!(app.session.active().is_none());
    assert!(draws_text(&mut app), "Home with no document open (the default)");
    assert!(crate::menus::home_showing(&app));
    app.session.execute("prefs.set", &json!({"key": "showHomeScreen", "value": false})).unwrap();
    assert!(!draws_text(&mut app), "off: an empty window");
    assert!(!shows_document(&mut app));
    assert!(!crate::menus::home_showing(&app), "the Home button isn't lit");
    app.run("app.home", json!({})).unwrap();
    assert!(draws_text(&mut app), "the Home button still shows the Home screen");
    app.run("file.new", json!({})).unwrap();
    assert!(shows_document(&mut app));
    assert!(app.ui.home.is_none());
    app.run("file.close", json!({})).unwrap();
    assert!(app.session.active().is_none());
    assert!(!draws_text(&mut app), "closing the last document leaves the window empty");
    app.session.execute("prefs.set", &json!({"key": "showHomeScreen", "value": true})).unwrap();
    assert!(draws_text(&mut app), "on again: Home");
}

#[test]
fn cmd_n_opens_the_new_document_dialog() {
    assert_eq!(crate::shortcut_editor::command_for_key("Cmd+N"), Some("file.newDialog"));
    let mut app = app_with_doc();
    app.run("file.newDialog", json!({})).unwrap();
    assert_eq!(app.ui.dialog.as_ref().map(|d| d.kind.as_str()), Some("newDocument"));
    assert_eq!(app.session.documents().len(), 1, "no document is made until the dialog's OK");
}

/// One Home screen frame (no document open) with `events`: the texts drawn and where.
fn home_texts(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>) -> Vec<(String, egui::Rect)> {
    fn walk(s: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
        match s {
            egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.visual_bounding_rect())),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
            _ => {}
        }
    }
    let raw =
        egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 900.0))), events, ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    let mut texts = vec![];
    out.shapes.iter().for_each(|c| walk(&c.shape, &mut texts));
    texts
}

/// The Home screen lists the first recent files (#663), each by name and folder; a click opens it,
/// and one that can't be opened says so. With none, there is no Recent Files section.
#[test]
fn home_lists_recent_files_and_a_click_opens_one() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let has = |texts: &[(String, egui::Rect)], s: &str| texts.iter().any(|(t, _)| t == s);
    assert!(!has(&home_texts(&mut app, &ctx, vec![]), "Recent Files"), "none: no section");
    let dir = std::env::temp_dir().join("vectorcraft-home-recent");
    let missing = dir.join("Gone.svg").to_string_lossy().into_owned();
    app.ui.recent_files = (0..9).map(|i| dir.join(format!("Drawing {i}.svg")).to_string_lossy().into_owned()).collect();
    app.ui.recent_files.insert(0, missing);
    let texts = home_texts(&mut app, &ctx, vec![]);
    assert!(has(&texts, "Recent Files") && has(&texts, "Gone.svg") && has(&texts, "Drawing 4.svg"), "{texts:?}");
    assert!(!has(&texts, "Drawing 5.svg"), "the first six only");
    assert!(has(&texts, &dir.to_string_lossy()), "with its folder");
    let at = texts.iter().find(|(t, _)| t == "Gone.svg").map(|(_, r)| r.center()).unwrap();
    let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    home_texts(&mut app, &ctx, vec![egui::Event::PointerMoved(at), press(true)]);
    home_texts(&mut app, &ctx, vec![press(false)]);
    assert!(app.session.documents().is_empty());
    assert!(!app.ui.status.is_empty(), "a file that can't be opened says so");
}

/// Home's texts this frame.
fn home_strings(app: &mut VectorcraftApp) -> Vec<String> {
    fn texts(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 1400.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    let mut v = vec![];
    out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
    v
}

#[test]
fn home_starts_embroidery_designs_by_hoop_with_samples_steps_and_a_tip() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    let texts = home_strings(&mut app);
    for want in ["Start a design for your hoop", "130 × 180 mm (5×7″)", "Your first design", "Learn how", "Try a sample", "Heart patch", "Tip:"] {
        assert!(texts.iter().any(|t| t == want), "{want} in {texts:?}");
    }
    // the graphic-design presets are gone
    assert!(!texts.iter().any(|t| t.contains("Web 1920")), "{texts:?}");
    // a hoop card's command: a document the hoop's size, the hoop set
    app.run("embroidery.newDesign", json!({"hoop": "Brother 100 × 100 (4×4)"})).unwrap();
    assert_eq!(app.session.documents().len(), 1);
    assert!(shows_document(&mut app));
    // Learn how / F1 open Help on a topic
    app.run("help.show", json!({"topic": "machine-files"})).unwrap();
    assert!(app.ui.help.open && app.ui.help.topic == "machine-files");
}

/// The texts one frame of the Embroidery panel draws.
fn panel_texts(app: &mut VectorcraftApp) -> Vec<String> {
    fn collect(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|x| collect(x, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 4000.0))), ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| crate::embroidery::panel::show(app, ui));
    out.textures_delta.clear();
    let mut texts = Vec::new();
    out.shapes.iter().for_each(|c| collect(&c.shape, &mut texts));
    texts
}

/// NEW-34: an experience level switches to its workspace and sets which sections the Embroidery
/// panel shows: a beginner the everyday ones, a digitizer every digitizing tool, a business all.
#[test]
fn experience_levels_choose_the_workspace_and_the_panel_sections() {
    let mut app = app_with_doc();
    let has = |texts: &[String], s: &str| texts.iter().any(|t| t == s);
    let all = panel_texts(&mut app);
    assert!(has(&all, "Job Orders") && has(&all, "Font Creator") && has(&all, "Team Names"), "{all:?}");
    app.run("window.experience", json!({"level": "beginner"})).unwrap();
    assert_eq!(app.ui.workspace, crate::workspaces::EMBROIDERY_BASICS);
    assert!(!app.ui.toolbar_advanced && app.ui.open_panel.as_deref() == Some("embroidery"));
    assert_eq!(crate::menus::checked(&app, "window.experience", &json!({"level": "beginner"})), Some(true));
    let basic = panel_texts(&mut app);
    for gone in ["Job Orders", "Batch Output", "Font Creator", "Team Names", "Buttonholes", "Exports"] {
        assert!(!has(&basic, gone), "{gone} shown to a beginner");
    }
    assert!(has(&basic, "Design Library") && has(&basic, "Send to Machine") && has(&basic, "Showing the basics."), "{basic:?}");
    app.run("window.experience", json!({"level": "digitizer"})).unwrap();
    assert_eq!(app.ui.workspace, crate::workspaces::DIGITIZING);
    let dig = panel_texts(&mut app);
    assert!(has(&dig, "Font Creator") && has(&dig, "Buttonholes") && !has(&dig, "Job Orders") && !has(&dig, "Batch Output"));
    app.run("window.experience", json!({"level": "business"})).unwrap();
    assert_eq!(app.ui.workspace, crate::workspaces::EMBROIDERY_BUSINESS);
    assert!(has(&panel_texts(&mut app), "Job Orders"));
    app.run("window.experience", json!({"level": "all"})).unwrap();
    assert_eq!(app.ui.workspace, crate::workspaces::ESSENTIALS);
    assert!(app.run("window.experience", json!({"level": "expert"})).is_err());
}

/// E4: with My Machine chosen, Home names it and offers its hoops; Save for My Machine writes
/// its format to the folder, and with no machine it opens the My Machine window.
#[test]
fn my_machine_shapes_home_and_saves_in_its_format() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    let texts = home_strings(&mut app);
    assert!(texts.iter().any(|t| t == "Choose My Machine"), "{texts:?}");
    app.run("embroidery.myMachine.set", json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
    let texts = home_strings(&mut app);
    assert!(texts.iter().any(|t| t == "My machine: Husqvarna Viking Designer Diamond deLuxe"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "240 × 150 mm (9.4×5.9″)"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "130 × 180 mm (5×7″)"), "the general hoops make way");
    // a design for the machine's hoop, saved in its format
    app.run("embroidery.newDesign", json!({})).unwrap();
    app.run("shape.rectangle", json!({"x": 100, "y": 100, "width": 120, "height": 80})).unwrap();
    app.run("embroidery.set", json!({"type": "fill"})).unwrap();
    let dir = std::env::temp_dir().join(format!("sf-my-machine-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // OP-06: no fabric said yet: Which fabric? comes first, then the save goes on with it
    let r = app.run("ui.saveForMyMachine", json!({"folder": dir.to_string_lossy()})).unwrap();
    assert_eq!(r["askFabric"], true, "{r}");
    assert!(app.ui.my_machine.fabric_open);
    assert_eq!(app.ui.my_machine.fabric_then["folder"], dir.to_string_lossy().as_ref());
    let r = app.run("ui.saveForMyMachine", json!({"folder": dir.to_string_lossy(), "fabric": "Jersey knit (T-shirt)"})).unwrap();
    assert_eq!(r["format"], "vp3");
    assert_eq!(app.run("embroidery.info", json!({})).unwrap()["fabric"], "Jersey knit (T-shirt)");
    let path = r["path"].as_str().unwrap();
    assert!(path.ends_with(".vp3") && std::fs::metadata(path).unwrap().len() > 100, "{r}");
    std::fs::remove_dir_all(&dir).ok();
    // no machine: the window opens instead
    app.run("embroidery.myMachine.set", json!({"machine": null})).unwrap();
    assert_eq!(app.run("ui.saveForMyMachine", json!({})).unwrap()["open"], true);
    assert!(app.ui.my_machine.open);
}

/// What the Make a Design guide shows at its current step.
fn guide_strings(app: &mut VectorcraftApp) -> Vec<String> {
    fn texts(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 1000.0))), ..Default::default() };
    // a window is measured on its first frame and drawn on the next
    ctx.run_ui(raw.clone(), |ui| crate::guide::show(app, ui.ctx())).textures_delta.clear();
    let mut out = ctx.run_ui(raw, |ui| crate::guide::show(app, ui.ctx()));
    out.textures_delta.clear();
    let mut v = vec![];
    out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
    v
}

/// E3: Make a Design walks from what to make, through the hoop and the words, to the colours,
/// watching it sew and saving; a mistake shows on the step instead of moving on.
#[test]
fn make_a_design_guide_walks_from_words_to_saving() {
    use crate::guide::Step;
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    app.run("embroidery.myMachine.set", json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
    assert_eq!(app.run("ui.makeDesign", json!({})).unwrap()["open"], true);
    let has = |v: &[String], want: &str| assert!(v.iter().any(|t| t == want), "{want} in {v:?}");
    let s = guide_strings(&mut app);
    for want in ["Step 1 / 7", "What would you like to make?", "A name or some words", "An appliqué", "A sample to try"] {
        has(&s, want);
    }
    has(&s, "An in-the-hoop project");
    // an in-the-hoop project: its list, then made in a hoop it fits, its steps named
    (app.guide.what, app.guide.step) = (Some("ith"), Step::Details);
    let s = guide_strings(&mut app);
    has(&s, "Which project?");
    has(&s, "Little pillow front, 8″ square");
    crate::guide::make_it(&mut app, json!({"what": "ith", "project": "mug-rug", "text": "Hi"}));
    assert_eq!(app.guide.step, Step::Fabric, "{:?}", app.guide.error);
    // quilting for a tile: the size and pattern, then a block made
    (app.guide.what, app.guide.step) = (Some("quilt"), Step::Details);
    let s = guide_strings(&mut app);
    has(&s, "What size is the tile?");
    has(&s, "A border strip");
    has(&s, "A corner");
    has(&s, "In the middle (you can leave this empty)");
    has(&s, "Pebbles");
    has(&s, "Stipple");
    crate::guide::make_it(&mut app, json!({"what": "quilt", "width": 4, "height": 4, "pattern": "waves"}));
    assert_eq!(app.guide.step, Step::Fabric, "{:?}", app.guide.error);
    (app.guide.what, app.guide.step) = (Some("words"), Step::Hoop);
    let s = guide_strings(&mut app);
    has(&s, "Which hoop will you use?");
    has(&s, "240 × 150 mm (9.4×5.9″)");
    (app.guide.hoop, app.guide.step) = (Some("Husqvarna 240 × 150".into()), Step::Details);
    let s = guide_strings(&mut app);
    has(&s, "What should it say?");
    has(&s, "Make It");
    // nothing typed: the step says so and stays
    let hoop = app.guide.hoop.clone();
    crate::guide::make_it(&mut app, json!({"what": "words", "text": "", "hoop": hoop}));
    assert_eq!(app.guide.step, Step::Details);
    assert!(app.guide.error.is_some());
    crate::guide::make_it(&mut app, json!({"what": "words", "text": "Mia", "height": 25, "hoop": hoop}));
    // OP-06: the fabric next, every fabric to choose from and "not sure"
    assert_eq!(app.guide.step, Step::Fabric, "{:?}", app.guide.error);
    let s = guide_strings(&mut app);
    has(&s, "Step 4 / 7");
    has(&s, "What fabric will you sew it on?");
    has(&s, "Jersey knit (T-shirt)");
    has(&s, "Towel / terry");
    has(&s, "Another fabric, or not sure: leave it as it is");
    app.guide.step = Step::Colours;
    let s = guide_strings(&mut app);
    has(&s, "Your thread colours");
    has(&s, "1.");
    app.guide.step = Step::Watch;
    let s = guide_strings(&mut app);
    has(&s, "Watch It Sew");
    assert!(s.iter().any(|t| t.starts_with("Sewing time ")), "{s:?}");
    app.guide.step = Step::Save;
    has(&guide_strings(&mut app), "Save for My Machine");
    let info = app.run("embroidery.info", json!({})).unwrap();
    assert_eq!(info["machineFormat"], "vp3");
    assert!(info["stitches"].as_u64().unwrap() > 100);
}

/// E5 + E6: simple mode's column has Undo, Redo, Start Over and Text size; Start Over asks first;
/// Keep Safe saves the design in My Designs and Your Design says where.
/// The texts a frame of `f` draws (a window takes two frames: measured, then drawn).
fn strings(app: &mut VectorcraftApp, mut f: impl FnMut(&mut VectorcraftApp, &mut egui::Ui)) -> Vec<String> {
    fn texts(s: &egui::Shape, out: &mut Vec<String>) {
        match s {
            egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
            egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
            _ => {}
        }
    }
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let raw = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 900.0))), ..Default::default() };
    // a window is measured on its first frame and drawn on the next
    ctx.run_ui(raw.clone(), |ui| f(app, ui)).textures_delta.clear();
    let mut out = ctx.run_ui(raw, |ui| f(app, ui));
    out.textures_delta.clear();
    let mut v = vec![];
    out.shapes.iter().for_each(|c| texts(&c.shape, &mut v));
    v
}

#[test]
fn simple_mode_makes_mistakes_safe_and_text_larger() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    app.run("window.experience", json!({"level": "beginner"})).unwrap();
    assert!(crate::simple::active(&app));
    app.run("embroidery.newDesign", json!({"hoop": "Husqvarna 240 × 150"})).unwrap();
    app.run("embroidery.addWords", json!({"text": "Liv"})).unwrap();
    let bar = strings(&mut app, crate::simple::left_bar);
    for want in ["Make a Design…", "Undo", "Redo", "Start Over…", "Text size", "Save for My Machine"] {
        assert!(bar.iter().any(|t| t == want), "{want} in {bar:?}");
    }
    // Text size: the UI scaling preference
    app.run("prefs.set", json!({"key": "uiScaling", "value": 1.25})).unwrap();
    assert!((app.session.prefs.ui_scaling - 1.25).abs() < 1e-9);
    // Start Over asks first
    app.simple.start_over = true;
    let asked = strings(&mut app, |app, ui| crate::simple::windows(app, ui.ctx()));
    assert!(asked.iter().any(|t| t == "Clear everything in this design and begin again?"), "{asked:?}");
    // Keep Safe: a new design goes to My Designs, and Your Design says so
    let dir = std::env::temp_dir().join(format!("sf-my-designs-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    app.session.prefs.my_designs_folder = dir.to_string_lossy().to_string();
    crate::simple::keep_safe(&mut app);
    let kept = app.simple.kept.clone().expect("kept");
    assert!(kept.ends_with("My design 1.stitchfoundry") && std::path::Path::new(&kept).exists(), "{kept}");
    app.run("select.none", json!({})).unwrap();
    let panel = strings(&mut app, crate::simple::right_panel);
    assert!(panel.iter().any(|t| t == "Saved for you automatically in My Designs:"), "{panel:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// TB-01: the first start on a tablet turns on simple mode, unless a way of working was chosen.
#[test]
fn a_tablet_starts_in_simple_mode_unless_another_way_was_chosen() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    assert!(!crate::simple::active(&app));
    assert!(app.start_on_tablet());
    assert!(crate::simple::active(&app));
    // chosen: kept
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    app.run("window.experience", serde_json::json!({"level": "digitizer"})).unwrap();
    assert!(!app.start_on_tablet());
    assert!(!crate::simple::active(&app));
}

/// Add Words is in the Embroidery menu in every way of working; with a line selected it offers
/// to follow it, and the words it adds sew along the line.
#[test]
fn add_words_from_the_menu_can_follow_a_line() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    app.run("embroidery.newDesign", json!({"hoop": {"width": 130, "height": 180}})).unwrap();
    let r = app.run("ui.addWords", json!({})).unwrap();
    assert_eq!((r["open"].as_bool(), r["along"].as_u64()), (Some(true), None));
    let shown = strings(&mut app, |app, ui| crate::simple::words_window(app, ui.ctx()));
    assert!(shown.iter().any(|t| t.starts_with("To follow a line")) && shown.iter().any(|t| t == "Arc Up"), "{shown:?}");
    let arc = app.run("shape.arc", json!({"x1": 60, "y1": 220, "x2": 310, "y2": 100})).unwrap()["id"].as_u64().unwrap();
    app.run("select.set", json!({"ids": [arc]})).unwrap();
    let r = app.run("ui.addWords", json!({})).unwrap();
    assert_eq!(r["along"].as_u64(), Some(arc));
    let shown = strings(&mut app, |app, ui| crate::simple::words_window(app, ui.ctx()));
    assert!(shown.iter().any(|t| t == "Along the selected line"), "{shown:?}");
}

/// One Home screen frame with `events`: the filled rectangles drawn in `fill`, and the cursor.
fn home_fills(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>, fill: egui::Color32) -> (Vec<egui::Rect>, egui::CursorIcon) {
    fn walk(s: &egui::Shape, fill: egui::Color32, out: &mut Vec<egui::Rect>) {
        match s {
            egui::Shape::Rect(r) if r.fill == fill => out.push(r.rect),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, fill, out)),
            _ => {}
        }
    }
    let raw =
        egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 900.0))), events, ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    let mut rects = vec![];
    out.shapes.iter().for_each(|c| walk(&c.shape, fill, &mut rects));
    (rects, out.platform_output.cursor_icon)
}

/// A Recent Files row is one button: the pointer over its folder lights the whole row (icon, name
/// and folder) with a pointing hand, and a click there opens the file like a click on its name.
#[test]
fn a_recent_file_row_lights_up_and_opens_from_anywhere_on_it() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let dir = std::env::temp_dir().join("vectorcraft-home-recent-row");
    app.ui.recent_files = vec![dir.join("Gone.svg").to_string_lossy().into_owned()];
    let texts = home_texts(&mut app, &ctx, vec![]);
    let rect_of = |s: &str| texts.iter().find(|(t, _)| t == s).map(|(_, r)| *r).unwrap();
    let (name, folder) = (rect_of("Gone.svg"), rect_of(&dir.to_string_lossy()));
    let hover = crate::theme::Tokens::get(&ctx).hover;
    let lit = |rects: &[egui::Rect]| rects.iter().any(|r| r.contains(name.center()) && r.contains(folder.center()));
    let (rects, cursor) = home_fills(&mut app, &ctx, vec![], hover);
    assert!(!lit(&rects) && cursor != egui::CursorIcon::PointingHand, "no highlight without the pointer");
    let at = folder.center();
    home_fills(&mut app, &ctx, vec![egui::Event::PointerMoved(at)], hover);
    let (rects, cursor) = home_fills(&mut app, &ctx, vec![], hover);
    assert!(lit(&rects), "the pointer over the folder lights the whole row");
    assert_eq!(cursor, egui::CursorIcon::PointingHand);
    let press = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    home_texts(&mut app, &ctx, vec![press(true)]);
    home_texts(&mut app, &ctx, vec![press(false)]);
    assert!(app.session.documents().is_empty());
    assert!(!app.ui.status.is_empty(), "the click on the folder tried to open the file");
}

/// One Home screen frame in a `w`×`h` window at `time` with `events`: the texts visible (inside
/// their clip rect and the window) and where.
fn visible_home_texts(
    app: &mut VectorcraftApp,
    ctx: &egui::Context,
    (w, h): (f32, f32),
    time: f64,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    fn walk(s: &egui::Shape, clip: egui::Rect, out: &mut Vec<(String, egui::Rect)>) {
        match s {
            egui::Shape::Text(t) if clip.contains_rect(t.visual_bounding_rect()) => out.push((t.galley.text().to_string(), t.visual_bounding_rect())),
            egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, clip, out)),
            _ => {}
        }
    }
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
    let raw = egui::RawInput { screen_rect: Some(screen), time: Some(time), events, ..Default::default() };
    let mut out = ctx.run_ui(raw, |ui| canvas::show(app, ui));
    out.textures_delta.clear();
    let mut texts = vec![];
    out.shapes.iter().for_each(|c| walk(&c.shape, c.clip_rect.intersect(screen), &mut texts));
    texts
}

/// Home scrolls when it is taller than the window: in a short window the Community section and
/// the samples are below the fold, the wheel brings them into view, and a sample scrolled into
/// view still opens.
#[test]
fn home_scrolls_in_a_short_window() {
    let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    let dir = std::env::temp_dir().join("vectorcraft-home-scroll");
    app.ui.recent_files = (0..6).map(|i| dir.join(format!("Drawing {i}.svg")).to_string_lossy().into_owned()).collect();
    let size = (800.0, 300.0);
    let has = |texts: &[(String, egui::Rect)], s: &str| texts.iter().any(|(t, _)| t == s);
    let texts = visible_home_texts(&mut app, &ctx, size, 0.0, vec![]);
    assert!(has(&texts, "Welcome to StitchFoundry"), "{texts:?}");
    assert!(!has(&texts, "Community"), "below the fold at first");
    let at = egui::pos2(400.0, 150.0);
    let wheel = egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -5000.0),
        phase: egui::TouchPhase::Move,
        modifiers: Default::default(),
    };
    visible_home_texts(&mut app, &ctx, size, 0.1, vec![egui::Event::PointerMoved(at), wheel]);
    let mut texts = vec![];
    for i in 2..40 {
        texts = visible_home_texts(&mut app, &ctx, size, f64::from(i) * 0.1, vec![]);
    }
    assert!(has(&texts, "Community"), "the wheel reaches the bottom: {texts:?}");
    assert!(!has(&texts, "Welcome to StitchFoundry"), "the top scrolled away");
    // The last sample, scrolled into view, still takes a click.
    let card = texts.iter().find(|(t, _)| t == "Appliqué star").map(|(_, r)| r.center()).unwrap();
    let press = |pressed| egui::Event::PointerButton { pos: card, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
    visible_home_texts(&mut app, &ctx, size, 4.0, vec![egui::Event::PointerMoved(card), press(true)]);
    visible_home_texts(&mut app, &ctx, size, 4.1, vec![press(false)]);
    assert_eq!(app.session.documents().len(), 1, "the sample opened");
}
