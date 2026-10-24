//! Stitch Sequence panel: the sewing order (paint order of stitched objects),
//! grouped by thread, with the stitch player and its travel buttons. Rows show and
//! hide, lock and unlock their objects, and drag to sew them earlier or later.

use egui::{Color32, Sense, Ui, vec2};
use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_embroidery::sf_core::threads::Rgb;

use super::{TravelBy, object_at, travel};
use crate::VectorcraftApp;
use crate::icons;
use crate::theme::Tokens;

pub const ID: &str = "stitchSequence";

/// A row being dragged to a new place in the sewing order.
#[derive(Clone, Copy)]
struct SeqDrag(u64);

fn swatch(ui: &Ui, rect: egui::Rect, c: Rgb) {
    ui.painter().rect_filled(rect, 2.0, Color32::from_rgb(c.r, c.g, c.b));
    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_black_alpha(90)), egui::StrokeKind::Inside);
}

/// What a click in the list asks for.
enum RowAction {
    Select(u64, bool),
    Arrange(&'static str, u64),
    Visible(u64, bool),
    Locked(u64, bool),
    /// Sew the first object just before (`false`) or after (`true`) the second.
    Move(u64, u64, bool),
}

/// The eye and lock toggles at the right end of row `r`; returns the eye's rectangle.
fn toggles(ui: &mut Ui, t: &Tokens, r: egui::Rect, id: u64, visible: bool, locked: bool, out: &mut Option<RowAction>) -> egui::Rect {
    let lock = egui::Rect::from_min_size(egui::pos2(r.right() - 22.0, r.top()), vec2(20.0, r.height()));
    let eye = egui::Rect::from_min_size(egui::pos2(lock.left() - 22.0, r.top()), vec2(20.0, r.height()));
    let er = ui.interact(eye, ui.id().with(("seq-eye", id)), Sense::click()).on_hover_text(if visible { tl!("Hide") } else { tl!("Show") });
    let icon = if visible { "eye" } else { "eye-off" };
    icons::paint(ui, icon, egui::Rect::from_center_size(eye.center(), vec2(13.0, 13.0)), if er.hovered() { t.text } else { t.text_dim });
    if er.clicked() {
        *out = Some(RowAction::Visible(id, !visible));
    }
    let lr = ui.interact(lock, ui.id().with(("seq-lock", id)), Sense::click()).on_hover_text(if locked { tl!("Unlock") } else { tl!("Lock") });
    if locked || lr.hovered() {
        let tint = if locked { t.text } else { t.text_dim };
        icons::paint(ui, "lock", egui::Rect::from_center_size(lock.center(), vec2(11.0, 11.0)), tint);
    }
    if lr.clicked() {
        *out = Some(RowAction::Locked(id, !locked));
    }
    eye
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let Some(plan) = app.session.embroidery_plan() else {
        crate::panels::empty_state(ui, "dc-list-view", tl!("No document"), tl!("Open a document to see its stitch sequence."));
        return;
    };
    // stitched objects hidden on their own: listed at the end, to show them again
    let hidden: Vec<(u64, String, bool)> = app
        .session
        .active()
        .map(|st| {
            let mut v = Vec::new();
            st.doc.walk(|n| {
                if !n.visible && n.embroidery.is_some() && !n.is_layer() && !matches!(n.kind, vectorcraft_doc::NodeKind::Group { .. }) {
                    v.push((n.id.0, n.display_name(), n.locked));
                }
            });
            v
        })
        .unwrap_or_default();
    if plan.design.objects.is_empty() && hidden.is_empty() {
        crate::panels::empty_state(ui, "dc-list-view", tl!("Nothing stitched yet"), tl!("Give objects a stitch type in the Embroidery panel."));
        return;
    }
    // Player controls.
    let total = plan.pattern().stitches.len();
    ui.horizontal(|ui| {
        let p = &mut app.embroidery.player;
        if crate::widgets::icon_button(ui, "chevrons-left", tl!("Back to the start"), false, 22.0).clicked() {
            p.active = true;
            p.pos = 0.0;
            p.playing = false;
        }
        let playing = p.active && p.playing;
        let (icon, tip) = if playing { ("square", tl!("Pause")) } else { ("chevron-right", tl!("Play the stitches")) };
        if crate::widgets::icon_button(ui, icon, tip, playing, 22.0).clicked() {
            if !p.active || p.pos as usize >= total {
                p.pos = 0.0;
            }
            p.active = true;
            p.playing = !playing;
        }
        if crate::widgets::icon_button(ui, "chevrons-right", tl!("Show every stitch"), false, 22.0).clicked() {
            p.active = false;
            p.playing = false;
        }
        let mut pos = p.pos.min(total as f64);
        let slider = egui::Slider::new(&mut pos, 0.0..=total as f64).show_value(false);
        if ui.add_sized(vec2((ui.available_width() - 70.0).max(40.0), 18.0), slider).changed() {
            p.active = true;
            p.playing = false;
            p.pos = pos;
        }
        let mut speed = p.speed;
        if ui.add(egui::DragValue::new(&mut speed).range(20.0..=20000.0).speed(10.0).suffix("/s")).changed() {
            p.speed = speed;
        }
    });
    // Travel: step through the design by stitch, object, colour, trim or machine function.
    let mut stepped: Option<usize> = None;
    ui.horizontal(|ui| {
        let labels: Vec<&str> = TravelBy::ALL.iter().map(|b| super::machine_word(b.label())).collect();
        let cur = TravelBy::ALL.iter().position(|b| *b == app.embroidery.player.travel).unwrap_or(0);
        ui.label(egui::RichText::new(tl!("Travel by")).color(t.text_dim).size(11.5));
        if let Some(i) = crate::widgets::dropdown(ui, "seq-travel", labels.get(cur).copied().unwrap_or_default(), &labels, 96.0)
            && let Some(b) = TravelBy::ALL.get(i)
        {
            app.embroidery.player.travel = *b;
        }
        let p = &app.embroidery.player;
        let at = if p.active { (p.pos as usize).min(total) } else { total };
        if crate::widgets::icon_button(ui, "chevron-left", tl!("Previous"), false, 22.0).clicked() {
            stepped = Some(travel(&plan, at, p.travel, false));
        }
        if crate::widgets::icon_button(ui, "chevron-right", tl!("Next"), false, 22.0).clicked() {
            stepped = Some(travel(&plan, at, p.travel, true));
        }
    });
    // the player's sewing as a looping GIF or an MP4 video, at about the player's speed
    let seconds = (total as f64 / app.embroidery.player.speed.max(1.0)).clamp(2.0, 60.0);
    let mut save = None;
    ui.horizontal(|ui| {
        if ui
            .small_button(tl!("Save as Animation…"))
            .on_hover_text(tl!("A GIF of the design sewing itself, for a website, a listing or a message"))
            .clicked()
        {
            save = Some(json!({ "seconds": seconds }));
        }
        if ui
            .small_button(tl!("Save as Video…"))
            .on_hover_text(tl!("An MP4 video of the design sewing itself, for social posts and phones"))
            .clicked()
        {
            save = Some(json!({ "seconds": seconds, "format": "mp4" }));
        }
    });
    if let Some(q) = save
        && let Err(e) = app.run("file.export.sewAnimation", q)
    {
        app.status(e);
    }
    if let Some(pos) = stepped {
        let p = &mut app.embroidery.player;
        p.active = true;
        p.playing = false;
        p.pos = pos as f64;
    }
    if let Some(st) = &plan.stats {
        let counts = [
            crate::i18n::tn(st.stitches as u64, "{n} stitch", "{n} stitches"),
            crate::i18n::tn(st.colors.len() as u64, "{n} color", "{n} colors"),
            crate::i18n::tn(st.trims as u64, "{n} trim", "{n} trims"),
        ];
        ui.label(egui::RichText::new(counts.join(" · ")).color(t.text_dim).size(11.5));
    }
    ui.add_space(4.0);
    // the selection follows the travel buttons
    let follow = stepped.and_then(|pos| object_at(&plan, pos));
    let (selected, doc) = match app.session.active() {
        Some(st) => (st.selection.objects.clone(), Some(&st.doc)),
        None => (Vec::new(), None),
    };
    let locked_of = |id: NodeId| doc.and_then(|d| d.node(id)).is_some_and(|n| n.locked);
    let mut action: Option<RowAction> = None;
    crate::widgets::list_box(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("stitch-seq").max_height(420.0).show(ui, |ui| {
            let mut last_node = None;
            let mut last_color: Option<Rgb> = None;
            let mut step = 0usize;
            for (k, obj) in plan.design.objects.iter().enumerate() {
                let Some(node) = plan.nodes.get(k).copied() else { continue };
                if last_node == Some(node) {
                    continue; // one row per document object
                }
                last_node = Some(node);
                let color = plan.design.thread(obj.color).color;
                if last_color != Some(color) {
                    step += 1;
                    last_color = Some(color);
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
                    swatch(ui, egui::Rect::from_center_size(r.left_center() + vec2(12.0, 0.0), vec2(14.0, 14.0)), color);
                    let label = format!("{} {step} · {}", tl!("Color"), super::thread_name(&plan.design.thread(obj.color)));
                    ui.painter().text(
                        r.left_center() + vec2(26.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        egui::FontId::proportional(11.5),
                        t.text_dim,
                    );
                }
                let stitches: usize = plan
                    .records_of(node)
                    .iter()
                    .map(|r| plan.pattern().stitches[r.clone()].iter().filter(|s| s.cmd == vectorcraft_embroidery::sf_core::Cmd::Stitch).count())
                    .sum();
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
                let is_sel = selected.contains(&node);
                if is_sel {
                    ui.painter().rect_filled(r, 0.0, t.row_selected);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, t.hover);
                }
                swatch(ui, egui::Rect::from_center_size(r.left_center() + vec2(30.0, 0.0), vec2(10.0, 10.0)), color);
                ui.painter().text(r.left_center() + vec2(42.0, 0.0), egui::Align2::LEFT_CENTER, &obj.name, egui::FontId::proportional(12.5), t.text);
                let eye = toggles(ui, &t, r, node.0, true, locked_of(node), &mut action);
                ui.painter().text(
                    egui::pos2(eye.left() - 4.0, r.center().y),
                    egui::Align2::RIGHT_CENTER,
                    stitches.to_string(),
                    egui::FontId::proportional(11.5),
                    t.text_dim,
                );
                // drag a row onto another to sew it just before (upper half) or after it
                if resp.drag_started() {
                    egui::DragAndDrop::set_payload(ui.ctx(), SeqDrag(node.0));
                }
                if let Some(drag) = resp.dnd_hover_payload::<SeqDrag>()
                    && drag.0 != node.0
                    && let Some(p) = ui.ctx().pointer_hover_pos()
                {
                    let y = if p.y > r.center().y { r.bottom() - 1.0 } else { r.top() + 1.0 };
                    ui.painter().line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], egui::Stroke::new(2.0, t.accent));
                }
                if let Some(drag) = resp.dnd_release_payload::<SeqDrag>()
                    && drag.0 != node.0
                {
                    let after = ui.ctx().pointer_interact_pos().is_some_and(|p| p.y > r.center().y);
                    action = Some(RowAction::Move(drag.0, node.0, after));
                }
                if resp.clicked() && action.is_none() {
                    action = Some(RowAction::Select(node.0, ui.input(|i| i.modifiers.shift || i.modifiers.command)));
                }
                resp.context_menu(|ui| {
                    for (label, cmd) in [
                        (tl!("Sew earlier"), "object.arrange.sendBackward"),
                        (tl!("Sew later"), "object.arrange.bringForward"),
                        (tl!("Sew first"), "object.arrange.sendToBack"),
                        (tl!("Sew last"), "object.arrange.bringToFront"),
                    ] {
                        if ui.button(label).clicked() {
                            action = Some(RowAction::Arrange(cmd, node.0));
                            ui.close();
                        }
                    }
                });
            }
            if !hidden.is_empty() {
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
                ui.painter().text(
                    r.left_center() + vec2(8.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    tl!("Hidden (not sewn)"),
                    egui::FontId::proportional(11.5),
                    t.text_dim,
                );
                for (id, name, locked) in &hidden {
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, t.hover);
                    }
                    ui.painter().text(
                        r.left_center() + vec2(42.0, 0.0),
                        egui::Align2::LEFT_CENTER,
                        name,
                        egui::FontId::proportional(12.5),
                        t.text_dim,
                    );
                    toggles(ui, &t, r, *id, false, *locked, &mut action);
                }
            }
        });
    });
    if let Some(id) = follow {
        app.run("select.set", json!({"ids": [id.0]})).ok();
    }
    match action {
        Some(RowAction::Select(id, add)) => {
            let cmd = if add { "select.toggle" } else { "select.set" };
            app.run(cmd, json!({"ids": [id]})).ok();
        }
        Some(RowAction::Arrange(cmd, id)) => {
            app.run("select.set", json!({"ids": [id]})).ok();
            app.run(cmd, json!({})).ok();
        }
        Some(RowAction::Visible(id, visible)) => {
            app.run("layer.setProps", json!({"ids": [id], "visible": visible})).ok();
        }
        Some(RowAction::Locked(id, locked)) => {
            app.run("layer.setProps", json!({"ids": [id], "locked": locked})).ok();
        }
        // sewn later is higher in the paint order
        Some(RowAction::Move(id, target, after)) => {
            let place = if after { "above" } else { "below" };
            app.run("layer.move", json!({"ids": [id], "target": target, "place": place})).ok();
        }
        None => {}
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if crate::embroidery::panel::button(ui, tl!("Sort by Thread Color")).clicked() {
            app.run("embroidery.sortByColor", json!({})).ok();
        }
        if crate::embroidery::panel::button(ui, tl!("Auto Sequence")).clicked() {
            app.run("embroidery.autoSequence", json!({})).ok();
        }
    });
}

pub fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    if ui.button(tl!("Sort by Thread Color")).clicked() {
        app.run("embroidery.sortByColor", json!({})).ok();
        ui.close();
    }
    if ui.button(tl!("Auto Sequence")).clicked() {
        app.run("embroidery.autoSequence", json!({})).ok();
        ui.close();
    }
}
