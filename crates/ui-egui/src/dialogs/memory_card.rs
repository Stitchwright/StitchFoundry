//! Save for a Memory Card: pick the card a card writer box writes (`embroidery.memoryCards`), and
//! the design is saved in the format the card's machines read, checked against what the card holds
//! (`embroidery.memoryCard.save`), for the box's own software to put on the card.

use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};

/// The Save for a Memory Card window.
#[derive(Clone, Debug, Default)]
pub struct MemoryCardState {
    pub open: bool,
    /// The card chosen (an id from `embroidery.memoryCards`).
    pub card: String,
    /// What the last save said: the file, or why it couldn't be saved.
    pub result: Option<Result<String, String>>,
}

/// `ui.saveForMemoryCard {card?}`: with a card, save for it at once; without, open the window.
pub fn run(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    match p.get("card").and_then(Value::as_str) {
        Some(card) => save(app, card, p.get("path").and_then(Value::as_str)),
        None => {
            app.ui.memory_card.open = true;
            Ok(json!({ "open": true }))
        }
    }
}

/// The design saved for `card`, to `path` or one picked.
fn save(app: &mut VectorcraftApp, card: &str, path: Option<&str>) -> Result<Value, String> {
    let cards = app.session.execute("embroidery.memoryCards", &json!({})).map_err(|e| e.to_string())?;
    let ext =
        cards["cards"].as_array().and_then(|l| l.iter().find(|c| c["id"] == card)).and_then(|c| c["format"].as_str()).unwrap_or("pes").to_string();
    let mut q = json!({ "card": card });
    if let Some(path) = path {
        q["path"] = json!(path);
    }
    let (path, r) = crate::io::run_to_file(app, "embroidery.memoryCard.save", &ext, q)?;
    Ok(json!({ "path": path, "format": r["format"], "stitches": r["stitches"], "share": r["share"], "warnings": r["warnings"] }))
}

pub(crate) fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.ui.memory_card.open {
        return;
    }
    let cards = app.session.execute("embroidery.memoryCards", &json!({})).unwrap_or_default();
    let cards = cards["cards"].as_array().cloned().unwrap_or_default();
    if app.ui.memory_card.card.is_empty()
        && let Some(id) = cards.first().and_then(|c| c["id"].as_str())
    {
        app.ui.memory_card.card = id.to_string();
    }
    let mut open = true;
    let mut go = false;
    egui::Window::new(tl!("Save for a Memory Card"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_pos(ctx.content_rect().center() - egui::vec2(230.0, 180.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.set_width(440.0);
            ui.label(egui::RichText::new(tl!("Which card does your machine take?")).font(theme::semibold(15.0)).color(t.text));
            ui.add_space(6.0);
            for c in &cards {
                let (Some(id), Some(name)) = (c["id"].as_str(), c["name"].as_str()) else { continue };
                let st = &mut app.ui.memory_card;
                ui.radio_value(&mut st.card, id.to_string(), name);
                let mut note = crate::i18n::fmt(tl!("For {machines}"), &[("machines", c["machines"].as_str().unwrap_or_default())]);
                if let Some(n) = c["stitches"].as_u64() {
                    note = format!("{note}; {}", crate::i18n::fmt(tl!("holds up to {n} stitches"), &[("n", &n.to_string())]));
                }
                ui.indent(id, |ui| ui.label(egui::RichText::new(note).size(12.0).color(t.text_dim)));
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(tl!(
                    "StitchFoundry saves the file; write it to the card with your card writer's own software (the Magic, Amazing or Ultimate Box's)."
                ))
                .size(12.0)
                .color(t.text_dim),
            );
            ui.add_space(8.0);
            if crate::widgets::primary_button(ui, tl!("Save…")).clicked() {
                go = true;
            }
            match &app.ui.memory_card.result {
                Some(Ok(msg)) => {
                    ui.label(egui::RichText::new(msg).size(13.0).color(t.text));
                }
                Some(Err(e)) => {
                    ui.label(egui::RichText::new(e).size(13.0).color(t.error));
                }
                None => {}
            }
        });
    if go {
        let card = app.ui.memory_card.card.clone();
        app.ui.memory_card.result = Some(save(app, &card, None).map(|r| saved_text(&r)));
    }
    if !open {
        app.ui.memory_card.open = false;
        app.ui.memory_card.result = None;
    }
}

/// What a save says: the file, how much of the card it takes, what the machine may do otherwise.
fn saved_text(r: &Value) -> String {
    let mut msg = crate::i18n::fmt(tl!("Saved {path}"), &[("path", r["path"].as_str().unwrap_or_default())]);
    if let Some(share) = r["share"].as_f64() {
        let pct = if share < 0.005 { "<1".to_string() } else { format!("{:.0}", share * 100.0) };
        msg = format!("{msg} ({})", crate::i18n::fmt(tl!("{share}% of the card"), &[("share", &pct)]));
    }
    for w in r["warnings"].as_array().map(Vec::as_slice).unwrap_or_default() {
        if let Some(w) = w.as_str() {
            msg = format!("{msg}\n• {}", crate::i18n::message(crate::i18n::current(), w));
        }
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_saves_the_design_in_its_format() {
        let services = crate::Services {
            write: Some(Box::new(|p: &str, b: &[u8]| std::fs::write(p, b).map_err(|e| e.to_string()))),
            ..Default::default()
        };
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), services);
        assert_eq!(app.run("ui.saveForMemoryCard", json!({})).unwrap()["open"], true);
        assert!(app.ui.memory_card.open);
        app.run("embroidery.sample", json!({"id": "heart-patch"})).unwrap();
        let dir = std::env::temp_dir().join(format!("sf-card-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("heart.pes");
        let r = app.run("ui.saveForMemoryCard", json!({"card": "brother-512k", "path": path.to_string_lossy()})).unwrap();
        assert_eq!(r["format"], "pes");
        assert!(std::fs::read(&path).unwrap().starts_with(b"#PES"));
        assert!(saved_text(&r).contains("% of the card"), "{}", saved_text(&r));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
