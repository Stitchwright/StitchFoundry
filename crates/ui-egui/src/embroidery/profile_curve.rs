//! The spacing profile's curve in the Embroidery panel (NEW-02): how dense a tatami's rows are
//! across the shape, drawn as a line to reshape. Drag a point; double-click to add one;
//! right-click one to take it out. Up is denser; along the bottom, no rows. Between points the
//! density changes evenly, as the fill sews it, so the line is what is sewn.

use egui::{Pos2, Sense, Stroke, Ui, vec2};

use crate::theme::Tokens;

/// The densest the curve reaches, over the fill's own density.
const TOP: f64 = 1.5;
/// Most points the curve takes.
pub(crate) const MAX_POINTS: usize = 16;
/// How near (points) the pointer has to be to a point to take it.
const GRAB: f32 = 8.0;
/// Below this share of the height a point sews no rows.
const NONE: f64 = 0.03;
/// Nearest two points come across the rows.
const GAP: f64 = 0.02;

/// The curve of `profile` (`[t, spacing mm]`; empty: even at `spacing`): its points as
/// `[t, density]`, both 0 to 1 (density over the densest the curve shows), in order across the
/// rows from one edge to the other.
pub(crate) fn to_curve(profile: &[[f64; 2]], spacing: f64) -> Vec<[f64; 2]> {
    let top = TOP / spacing.max(0.05);
    let density = |s: f64| if s > 0.0 && s.is_finite() { (1.0 / s / top).min(1.0) } else { 0.0 };
    let mut pts: Vec<[f64; 2]> =
        profile.iter().filter(|p| p[0].is_finite() && p[1].is_finite()).map(|&[t, s]| [t.clamp(0.0, 1.0), density(s)]).collect();
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let (Some(&first), Some(&last)) = (pts.first(), pts.last()) else {
        let y = density(spacing);
        return vec![[0.0, y], [1.0, y]];
    };
    // the ends at the edges: the fill keeps the first and last densities out to them
    if first[0] > 0.0 {
        pts.insert(0, [0.0, first[1]]);
    }
    if last[0] < 1.0 || pts.len() < 2 {
        pts.push([1.0, last[1]]);
    }
    pts
}

/// Back to a spacing profile (`[t, spacing mm]`), rounded.
pub(crate) fn to_profile(curve: &[[f64; 2]], spacing: f64) -> Vec<[f64; 2]> {
    let top = TOP / spacing.max(0.05);
    curve
        .iter()
        .map(|&[t, y]| {
            let s = if y < NONE { 0.0 } else { 1.0 / (y.min(1.0) * top) };
            [(t * 1000.0).round() / 1000.0, (s * 100.0).round() / 100.0]
        })
        .collect()
}

/// `curve` with point `i` moved to `(t, y)`: the ends keep to the edges, the others between
/// their neighbours.
pub(crate) fn moved(curve: &[[f64; 2]], i: usize, t: f64, y: f64) -> Vec<[f64; 2]> {
    let mut c = curve.to_vec();
    let n = c.len();
    let lo = if i == 0 { 0.0 } else { c.get(i - 1).map_or(0.0, |p| p[0] + GAP) };
    let hi = if i + 1 >= n { 1.0 } else { c.get(i + 1).map_or(1.0, |p| p[0] - GAP) };
    if let Some(p) = c.get_mut(i) {
        let t = if i == 0 {
            0.0
        } else if i + 1 == n {
            1.0
        } else if lo <= hi {
            t.clamp(lo, hi)
        } else {
            p[0]
        };
        *p = [t, y.clamp(0.0, 1.0)];
    }
    c
}

/// `curve` with a point added at `(t, y)` in its place across the rows (none too near another or
/// past [`MAX_POINTS`]).
pub(crate) fn added(curve: &[[f64; 2]], t: f64, y: f64) -> Vec<[f64; 2]> {
    let mut c = curve.to_vec();
    if c.len() >= MAX_POINTS || !(GAP..=1.0 - GAP).contains(&t) || c.iter().any(|p| (p[0] - t).abs() < GAP) {
        return c;
    }
    let at = c.iter().position(|p| p[0] > t).unwrap_or(c.len());
    c.insert(at, [t, y.clamp(0.0, 1.0)]);
    c
}

/// `curve` without point `i` (the ends stay).
pub(crate) fn removed(curve: &[[f64; 2]], i: usize) -> Vec<[f64; 2]> {
    let mut c = curve.to_vec();
    if i > 0 && i + 1 < c.len() {
        c.remove(i);
    }
    c
}

/// A drag in progress: the point taken and the curve as it is so far.
type Drag = (usize, Vec<[f64; 2]>);

/// Draws `profile` (a tatami's spacing profile; empty: even at `spacing`, the fill's spacing in
/// mm) as its curve and lets it be reshaped. → The reshaped profile when a change ends (a drag
/// released, a point added or taken out).
pub(crate) fn show(ui: &mut Ui, id: egui::Id, profile: &[[f64; 2]], spacing: f64) -> Option<Vec<[f64; 2]>> {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().clamp(120.0, 260.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 72.0), Sense::click_and_drag());
    let resp = resp.on_hover_text(tl!("Rows across the shape: up is denser. Drag a point; double-click to add one; right-click one to take it out."));
    let inner = rect.shrink(5.0);
    let to_screen = |p: [f64; 2]| Pos2::new(inner.left() + p[0] as f32 * inner.width(), inner.bottom() - p[1] as f32 * inner.height());
    let from_screen = |q: Pos2| {
        let x = ((q.x - inner.left()) / inner.width().max(1.0)).clamp(0.0, 1.0);
        let y = ((inner.bottom() - q.y) / inner.height().max(1.0)).clamp(0.0, 1.0);
        (f64::from(x), f64::from(y))
    };
    let nearest = |c: &[[f64; 2]], q: Pos2| {
        c.iter()
            .enumerate()
            .map(|(i, p)| (i, to_screen(*p).distance(q)))
            .filter(|(_, d)| *d <= GRAB)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let drag_id = id.with("drag");
    let mut drag: Option<Drag> = ui.data(|d| d.get_temp(drag_id));
    let mut curve = drag.as_ref().map_or_else(|| to_curve(profile, spacing), |(_, c)| c.clone());
    let mut out = None;
    // where the button went down: the drag starts a little way from it
    if resp.drag_started()
        && let Some(q) = ui.input(|i| i.pointer.press_origin())
        && let Some(i) = nearest(&curve, q)
    {
        drag = Some((i, curve.clone()));
    }
    if let Some((i, _)) = drag {
        if let Some(q) = resp.interact_pointer_pos().filter(|_| resp.dragged()) {
            let (x, y) = from_screen(q);
            curve = moved(&curve, i, x, y);
        }
        if resp.dragged() {
            ui.data_mut(|d| d.insert_temp::<Drag>(drag_id, (i, curve.clone())));
        } else {
            ui.data_mut(|d| d.remove::<Drag>(drag_id));
            out = Some(to_profile(&curve, spacing));
        }
    } else if let Some(q) = resp.interact_pointer_pos().or(resp.hover_pos()).filter(|_| resp.double_clicked()) {
        let (x, y) = from_screen(q);
        let c = added(&curve, x, y);
        if c.len() != curve.len() {
            out = Some(to_profile(&c, spacing));
            curve = c;
        }
    } else if let Some(i) = resp.interact_pointer_pos().or(resp.hover_pos()).filter(|_| resp.secondary_clicked()).and_then(|q| nearest(&curve, q)) {
        let c = removed(&curve, i);
        if c.len() != curve.len() {
            out = Some(to_profile(&c, spacing));
            curve = c;
        }
    }
    let p = ui.painter_at(rect);
    p.rect_filled(rect, f32::from(t.radius), t.input);
    p.rect_stroke(rect, f32::from(t.radius), Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    // the fill's own density, for scale
    p.hline(inner.x_range(), to_screen([0.0, 1.0 / TOP]).y, Stroke::new(1.0, t.divider));
    let pts: Vec<Pos2> = curve.iter().map(|c| to_screen(*c)).collect();
    p.add(egui::Shape::line(pts.clone(), Stroke::new(1.5, t.accent)));
    let hover = resp.hover_pos().and_then(|q| nearest(&curve, q));
    for (i, q) in pts.iter().enumerate() {
        let r = if hover == Some(i) || drag.as_ref().is_some_and(|d| d.0 == i) { 4.5 } else { 3.0 };
        p.circle_filled(*q, r, t.accent);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(p, q)| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 0.011)
    }

    #[test]
    fn profiles_come_back_from_their_curves() {
        let d = 0.4;
        for p in [vec![[0.0, d], [0.25, d * 4.0], [0.5, d], [0.75, d * 4.0], [1.0, d]], vec![[0.0, d], [1.0, 0.0]], vec![[0.0, 0.0], [1.0, d]]] {
            let c = to_curve(&p, d);
            assert!(c.iter().all(|q| (0.0..=1.0).contains(&q[0]) && (0.0..=1.0).contains(&q[1])), "{c:?}");
            assert!(close(&to_profile(&c, d), &p), "{p:?} → {c:?} → {:?}", to_profile(&c, d));
        }
        // even: a flat line at the fill's own density, two thirds up
        let even = to_curve(&[], d);
        assert!(close(&even, &[[0.0, 1.0 / TOP], [1.0, 1.0 / TOP]]), "{even:?}");
        // points inside, out of order or junk: in order, reaching both edges
        let c = to_curve(&[[0.6, d], [0.2, 0.0], [f64::NAN, 1.0]], d);
        assert_eq!(c.iter().map(|p| p[0]).collect::<Vec<_>>(), vec![0.0, 0.2, 0.6, 1.0]);
        assert_eq!(to_curve(&[[0.5, d]], d).len(), 3);
    }

    #[test]
    fn points_move_add_and_go_within_the_rules() {
        let c = vec![[0.0, 0.5], [0.5, 0.5], [1.0, 0.5]];
        // the ends keep to the edges; the middle stays between them, the height within 0 to 1
        assert_eq!(moved(&c, 0, 0.3, 0.9)[0], [0.0, 0.9]);
        assert_eq!(moved(&c, 2, 0.2, -1.0)[2], [1.0, 0.0]);
        assert_eq!(moved(&c, 1, 5.0, 0.25)[1], [1.0 - GAP, 0.25]);
        assert_eq!(moved(&c, 7, 0.5, 0.5), c);
        // added in order, not onto another point or past the most
        let a = added(&c, 0.25, 0.8);
        assert_eq!(a[1], [0.25, 0.8]);
        assert_eq!(added(&c, 0.505, 0.8), c);
        let mut full = c.clone();
        for k in 1..40 {
            full = added(&full, k as f64 / 40.0, 0.3);
        }
        assert_eq!(full.len(), MAX_POINTS);
        // taken out, but never an end
        assert_eq!(removed(&a, 1), c);
        assert_eq!(removed(&c, 0), c);
        assert_eq!(removed(&c, 2), c);
    }

    #[test]
    fn dragging_a_point_reshapes_the_profile_once_released() {
        use egui::{Event, Modifiers, PointerButton, Rect};
        let d = 0.4;
        let accordion = vec![[0.0, d], [0.25, d * 4.0], [0.5, d], [0.75, d * 4.0], [1.0, d]];
        let ctx = egui::Context::default();
        let rect = std::cell::Cell::new(Rect::NOTHING);
        let mut time = 0.0;
        let mut frame = |events: Vec<Event>| {
            time += 0.05;
            let screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 120.0)));
            let input = egui::RawInput { events, time: Some(time), screen_rect, focused: true, ..Default::default() };
            let mut got = None;
            let mut out = ctx.run_ui(input, |ui| {
                let before = ui.cursor().min;
                got = show(ui, egui::Id::unique("curve"), &accordion, d);
                rect.set(Rect::from_min_max(before, ui.min_rect().max));
            });
            out.textures_delta.clear();
            got
        };
        assert_eq!(frame(vec![]), None);
        let inner = rect.get().shrink(5.0);
        let at = |t: f32, y: f32| Pos2::new(inner.left() + t * inner.width(), inner.bottom() - y * inner.height());
        let button = |pos, button, pressed| Event::PointerButton { pos, button, pressed, modifiers: Modifiers::NONE };
        // the second point (a quarter across, sparse) dragged to the top: dense there
        let from = at(0.25, 1.0 / 6.0);
        assert_eq!(frame(vec![Event::PointerMoved(from)]), None);
        assert_eq!(frame(vec![button(from, PointerButton::Primary, true)]), None);
        let mut q = from;
        for _ in 0..6 {
            q.y -= 12.0;
            assert_eq!(frame(vec![Event::PointerMoved(q)]), None, "nothing changes until it is let go");
        }
        let top = at(0.25, 1.0);
        assert_eq!(frame(vec![Event::PointerMoved(top)]), None);
        let p = frame(vec![button(top, PointerButton::Primary, false)]).expect("a profile on release");
        assert_eq!(p.len(), 5);
        assert!((p[1][0] - 0.25).abs() < 0.01 && p[1][1] < d, "{p:?}");
        assert_eq!((p[0], p[2], p[4]), (accordion[0], accordion[2], accordion[4]));
        // a right-click on the middle point takes it out
        let mid = at(0.5, 2.0 / 3.0);
        assert_eq!(frame(vec![Event::PointerMoved(mid)]), None);
        frame(vec![button(mid, PointerButton::Secondary, true)]);
        let p = frame(vec![button(mid, PointerButton::Secondary, false)]).expect("one point fewer");
        assert_eq!(p.len(), 4, "{p:?}");
        assert!(!p.iter().any(|q| q[0] == 0.5));
    }
}
