//! Help › StitchFoundry Help… (F1): the user guide (compiled in; `help.topics`, `help.topic`)
//! with its topics, a search and the guide's own links, and Tips: short hints for the tool in
//! hand first. F1 opens the topic about the active tool (`help.forTool`).

use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};

/// What the Help window shows.
#[derive(Clone, Debug, Default)]
pub struct HelpState {
    pub open: bool,
    /// The topic shown (a guide anchor id).
    pub topic: String,
    pub query: String,
    /// The Tips tab instead of the guide.
    pub tips: bool,
}

/// Opens Help: on `topic`, else the active tool's topic; on Tips with `tips`.
pub fn open(app: &mut VectorcraftApp, p: &Value) -> Result<Value, String> {
    let topic = match p.get("topic").and_then(Value::as_str) {
        Some(t) => t.to_string(),
        None => app.session.execute("help.forTool", &json!({})).map_err(|e| e.to_string())?["id"].as_str().unwrap_or("quick-start").to_string(),
    };
    app.ui.help.topic = topic;
    app.ui.help.tips = p.get("tips").and_then(Value::as_bool).unwrap_or(false);
    app.ui.help.open = true;
    Ok(Value::Null)
}

/// Short hints: the tool they are about ("" for any), the hint.
pub fn tips() -> Vec<(&'static str, &'static str)> {
    vec![
        ("embroideryDirection", tl!("Draw an inclination line across a shape to set its stitch angle; two or more lines make the stitches turn.")),
        ("embroideryClick", tl!("Click a region of a placed image to stitch it; raise Tolerance to take in more of its shades.")),
        (
            "embroideryCutout",
            tl!("Paint over a cut-out photo to put back what Remove Background left out; hold Alt to take away background it kept."),
        ),
        ("embroideryStitches", tl!("Drag picked needle points to move them, Alt-click a stitch to add one, Delete removes them.")),
        ("embroideryColumn", tl!("Click on one side of the column, then the other, in turn; Enter or Escape ends it.")),
        ("embroideryBrush", tl!("The dial in the lower-left corner sets the brush, width, density, length and shape.")),
        ("type", tl!("Type your text, then choose an embroidery font in the Lettering panel so it sews cleanly.")),
        ("", tl!("Select art and choose Embroidery › Convert Artwork to Embroidery to digitize it in one step.")),
        ("", tl!("Satin suits shapes up to about 7 mm wide; wider areas sew better as tatami fill.")),
        ("", tl!("Choose a hoop in the Embroidery panel to see the sewing field round the design.")),
        ("", tl!("Check Design in the Embroidery menu finds dense layers, long stitches and text too small to sew.")),
        ("", tl!("Watch the design sew with the stitch player in Stitch Sequence before you send it.")),
        ("", tl!("Export with Embroidery › Export Machine File and pick your machine's format.")),
    ]
}

/// The Help window, while open.
pub(super) fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.ui.help.open {
        return;
    }
    let mut open = true;
    let mut go: Option<String> = None;
    egui::Window::new(tl!("StitchFoundry Help"))
        .open(&mut open)
        .default_size([860.0, 600.0])
        .default_pos(ctx.content_rect().center() - egui::vec2(430.0, 300.0))
        .show(ctx, |ui| {
            let t = Tokens::get(ui.ctx());
            ui.horizontal(|ui| {
                if ui.selectable_label(!app.ui.help.tips, tl!("Guide")).clicked() {
                    app.ui.help.tips = false;
                }
                if ui.selectable_label(app.ui.help.tips, tl!("Tips")).clicked() {
                    app.ui.help.tips = true;
                }
                if !app.ui.help.tips {
                    ui.add_space(16.0);
                    ui.add(egui::TextEdit::singleline(&mut app.ui.help.query).hint_text(tl!("Search help")).desired_width(240.0));
                }
            });
            ui.separator();
            if app.ui.help.tips {
                tips_tab(app, ui, &t);
                return;
            }
            let found = app.session.execute("help.topics", &json!({ "query": app.ui.help.query })).unwrap_or_default();
            let topics: Vec<(String, String)> = found["topics"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| (x["id"].as_str().unwrap_or_default().to_string(), x["title"].as_str().unwrap_or_default().to_string()))
                        .collect()
                })
                .unwrap_or_default();
            ui.horizontal_top(|ui| {
                // the topics down the left
                egui::ScrollArea::vertical().id_salt("help-topics").max_width(230.0).auto_shrink([false, false]).show(ui, |ui| {
                    // the window lays out side by side: the list goes down
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        if topics.is_empty() {
                            ui.label(egui::RichText::new(tl!("Nothing found")).color(t.text_dim));
                        }
                        for (id, title) in &topics {
                            if ui.selectable_label(*id == app.ui.help.topic, title).clicked() {
                                go = Some(id.clone());
                            }
                        }
                    });
                });
                ui.separator();
                // the topic
                let page = app.session.execute("help.topic", &json!({ "id": app.ui.help.topic })).unwrap_or_default();
                egui::ScrollArea::vertical().id_salt(("help-page", app.ui.help.topic.clone())).auto_shrink([false, false]).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width().min(680.0));
                        ui.label(egui::RichText::new(page["title"].as_str().unwrap_or_default()).font(theme::semibold(20.0)).color(t.text));
                        ui.add_space(6.0);
                        if let Some(link) = markdown(ui, page["text"].as_str().unwrap_or_default(), &t) {
                            go = Some(link);
                        }
                    });
                });
            });
        });
    if let Some(link) = go {
        if let Some(id) = link.strip_prefix('#') {
            app.ui.help.topic = id.to_string();
        } else if link.starts_with("https://") {
            ctx.open_url(egui::OpenUrl::new_tab(link));
        } else {
            app.ui.help.topic = link;
        }
    }
    app.ui.help.open = open;
}

/// The Tips tab: the active tool's hints first, then the general ones.
fn tips_tab(app: &mut VectorcraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let tool = app.session.tool_id();
    let all = tips();
    let mine = all.iter().filter(|(k, _)| *k == tool);
    let rest = all.iter().filter(|(k, _)| k.is_empty());
    egui::ScrollArea::vertical().id_salt("help-tips").auto_shrink([false, false]).show(ui, |ui| {
        ui.vertical(|ui| {
            for (_, tip) in mine.chain(rest) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new("•").color(t.accent));
                    ui.label(*tip);
                });
                ui.add_space(6.0);
            }
        });
    });
}

/// A run of inline text: plain, bold, code, or a link to its target.
#[derive(Debug, PartialEq)]
enum Span {
    Text(String),
    Bold(String),
    Code(String),
    Link(String, String),
}

/// The inline spans of a line of Markdown: `**bold**`, `` `code` `` and `[text](target)`.
fn spans(s: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = s;
    let flush = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.push(Span::Text(std::mem::take(plain)));
        }
    };
    while let Some(c) = rest.chars().next() {
        if let Some(r) = rest.strip_prefix("**")
            && let Some(end) = r.find("**")
        {
            flush(&mut plain, &mut out);
            out.push(Span::Bold(r.get(..end).unwrap_or_default().to_string()));
            rest = r.get(end + 2..).unwrap_or_default();
            continue;
        }
        if let Some(r) = rest.strip_prefix('`')
            && let Some(end) = r.find('`')
        {
            flush(&mut plain, &mut out);
            out.push(Span::Code(r.get(..end).unwrap_or_default().to_string()));
            rest = r.get(end + 1..).unwrap_or_default();
            continue;
        }
        if let Some(r) = rest.strip_prefix('[')
            && let Some(mid) = r.find("](")
            && let Some(end) = r.get(mid + 2..).and_then(|x| x.find(')'))
        {
            flush(&mut plain, &mut out);
            let text = r.get(..mid).unwrap_or_default().replace("**", "");
            let target = r.get(mid + 2..mid + 2 + end).unwrap_or_default().to_string();
            out.push(Span::Link(text, target));
            rest = r.get(mid + 3 + end..).unwrap_or_default();
            continue;
        }
        plain.push(c);
        rest = rest.get(c.len_utf8()..).unwrap_or_default();
    }
    flush(&mut plain, &mut out);
    out
}

/// One paragraph or list item; a link clicked → its target.
fn inline(ui: &mut egui::Ui, s: &str, t: &Tokens, size: f32) -> Option<String> {
    let parts = spans(s);
    let font = FontId::proportional(size);
    let fmt = |f: FontId, c: Color32| TextFormat { font_id: f, color: c, ..Default::default() };
    if !parts.iter().any(|p| matches!(p, Span::Link(..))) {
        // one wrapping label
        let mut job = LayoutJob::default();
        for p in &parts {
            match p {
                Span::Text(x) => job.append(x, 0.0, fmt(font.clone(), t.text)),
                Span::Bold(x) => job.append(x, 0.0, fmt(theme::semibold(size), t.text)),
                Span::Code(x) => job.append(x, 0.0, fmt(FontId::monospace(size - 1.0), t.text_dim)),
                Span::Link(..) => {}
            }
        }
        job.wrap.max_width = ui.available_width();
        ui.label(job);
        return None;
    }
    // with links: word by word, so the links can be clicked
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for p in &parts {
            match p {
                Span::Link(text, target) => {
                    if ui.link(egui::RichText::new(text).size(size)).clicked() {
                        clicked = Some(target.clone());
                    }
                }
                Span::Text(x) | Span::Bold(x) | Span::Code(x) => {
                    for w in x.split_inclusive(' ') {
                        let rt = match p {
                            Span::Bold(_) => egui::RichText::new(w).font(theme::semibold(size)),
                            Span::Code(_) => egui::RichText::new(w).monospace().color(t.text_dim),
                            _ => egui::RichText::new(w).size(size),
                        };
                        ui.label(rt);
                    }
                }
            }
        }
    });
    clicked
}

/// Draws a topic's Markdown: headings, paragraphs, lists, tables and code blocks; a link
/// clicked → its target.
fn markdown(ui: &mut egui::Ui, text: &str, t: &Tokens) -> Option<String> {
    let mut clicked = None;
    let mut para = String::new();
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut code: Option<String> = None;
    let mut keep = |c: Option<String>| {
        if clicked.is_none() {
            clicked = c;
        }
    };
    let flush_para = |ui: &mut egui::Ui, para: &mut String, keep: &mut dyn FnMut(Option<String>)| {
        let p = std::mem::take(para);
        let p = p.trim();
        if p.is_empty() {
            return;
        }
        if let Some(item) = p.strip_prefix("- ") {
            ui.horizontal_top(|ui| {
                ui.label("•");
                ui.vertical(|ui| keep(inline(ui, item, t, 14.0)));
            });
        } else if let Some((n, item)) = p.split_once(". ").filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())) {
            ui.horizontal_top(|ui| {
                ui.label(format!("{n}."));
                ui.vertical(|ui| keep(inline(ui, item, t, 14.0)));
            });
        } else {
            keep(inline(ui, p, t, 14.0));
            ui.add_space(6.0);
        }
    };
    let flush_table = |ui: &mut egui::Ui, table: &mut Vec<Vec<String>>, keep: &mut dyn FnMut(Option<String>)| {
        if table.is_empty() {
            return;
        }
        let rows = std::mem::take(table);
        egui::Grid::new(ui.next_auto_id()).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
            for (i, row) in rows.iter().enumerate() {
                for cell in row {
                    ui.vertical(|ui| {
                        ui.set_max_width(320.0);
                        let size = if i == 0 { 13.0 } else { 12.5 };
                        keep(inline(ui, cell, t, size));
                    });
                }
                ui.end_row();
            }
        });
        ui.add_space(8.0);
    };
    for line in text.lines() {
        if let Some(c) = code.as_mut() {
            if line.trim_start().starts_with("```") {
                ui.label(egui::RichText::new(std::mem::take(c)).monospace().color(t.text_dim));
                code = None;
            } else {
                c.push_str(line);
                c.push('\n');
            }
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush_para(ui, &mut para, &mut keep);
            code = Some(String::new());
            continue;
        }
        if trimmed.starts_with('|') {
            flush_para(ui, &mut para, &mut keep);
            let cells: Vec<String> = trimmed.trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
            // the line under the header row
            if !cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':'))) {
                table.push(cells);
            }
            continue;
        }
        flush_table(ui, &mut table, &mut keep);
        if let Some(h) = trimmed.strip_prefix("### ").or_else(|| trimmed.strip_prefix("#### ")) {
            flush_para(ui, &mut para, &mut keep);
            ui.add_space(6.0);
            ui.label(egui::RichText::new(h).font(theme::semibold(16.0)).color(t.text));
            continue;
        }
        if trimmed.starts_with("# ") {
            flush_para(ui, &mut para, &mut keep);
            continue;
        }
        if trimmed.is_empty() {
            flush_para(ui, &mut para, &mut keep);
            continue;
        }
        // a list item starts a new paragraph; other lines continue the one before
        let item = trimmed.starts_with("- ") || trimmed.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
        if item {
            flush_para(ui, &mut para, &mut keep);
        }
        if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(trimmed);
    }
    flush_para(ui, &mut para, &mut keep);
    flush_table(ui, &mut table, &mut keep);
    if let Some(c) = code {
        ui.label(egui::RichText::new(c).monospace().color(t.text_dim));
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_spans_read_bold_code_and_links() {
        assert_eq!(
            spans("Use **Cut File** (`embroidery.cutFile`), see [Hoops](#hoops)."),
            vec![
                Span::Text("Use ".into()),
                Span::Bold("Cut File".into()),
                Span::Text(" (".into()),
                Span::Code("embroidery.cutFile".into()),
                Span::Text("), see ".into()),
                Span::Link("Hoops".into(), "#hoops".into()),
                Span::Text(".".into()),
            ]
        );
        // unclosed markers stay text
        assert_eq!(spans("a ** b ` c [d"), vec![Span::Text("a ** b ` c [d".into())]);
    }

    #[test]
    fn every_topic_draws_and_f1_opens_the_tools_topic() {
        let mut app = crate::VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        let topics = app.session.execute("help.topics", &json!({})).unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for tp in topics["topics"].as_array().unwrap() {
            let text = app.session.execute("help.topic", &json!({"id": tp["id"]})).unwrap()["text"].as_str().unwrap().to_string();
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let t = Tokens::get(ui.ctx());
                let _ = markdown(ui, &text, &t);
            });
            out.textures_delta.clear();
        }
        app.select_tool("embroideryClick");
        open(&mut app, &json!({})).unwrap();
        assert!(app.ui.help.open);
        let want = app.session.execute("help.forTool", &json!({})).unwrap()["id"].as_str().unwrap().to_string();
        assert_eq!(app.ui.help.topic, want);
        open(&mut app, &json!({"topic": "hoops", "tips": true})).unwrap();
        assert_eq!((app.ui.help.topic.as_str(), app.ui.help.tips), ("hoops", true));
    }
}
