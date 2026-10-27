//! The digitizing widget: a radial palette in the canvas's lower-left corner.
//! The inner ring picks what to set (brush, width, density, stitch length,
//! shape); the outer ring shows that setting's values; the centre is the
//! thread colour. Choices are loaded onto the embroidery brush tool.

use egui::{Color32, Pos2, Sense, Shape, Stroke, Ui, pos2, vec2};
use serde_json::json;

use crate::VectorcraftApp;
use crate::theme::Tokens;

pub const TOOL: &str = "embroideryBrush";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ring {
    #[default]
    Brush,
    Width,
    Density,
    Length,
    Shape,
}

impl Ring {
    const ALL: [Ring; 5] = [Ring::Brush, Ring::Width, Ring::Density, Ring::Length, Ring::Shape];

    fn label(self) -> &'static str {
        match self {
            Ring::Brush => "Brush",
            Ring::Width => "Width",
            Ring::Density => "Density",
            Ring::Length => "Length",
            Ring::Shape => "Shape",
        }
    }
}

/// Brushes: (tool value, label).
pub const BRUSHES: &[(&str, &str)] = &[
    ("run", "Run"),
    ("bean", "Bean"),
    ("steil", "Steil"),
    ("tapered", "Tapered"),
    ("calligraphy", "Calligraphy"),
    ("pressure", "Pressure"),
    ("applique", "Appliqué"),
    ("fill", "Tatami"),
    ("satin", "Satin"),
];

/// Shapes: (tool value, label).
pub const SHAPES: &[(&str, &str)] = &[
    ("free", "Free"),
    ("line", "Line"),
    ("ellipse", "Circle"),
    ("rectangle", "Square"),
    ("roundedRectangle", "Rounded"),
    ("triangle", "Triangle"),
    ("diamond", "Diamond"),
    ("hexagon", "Hexagon"),
    ("star", "Star"),
    ("heart", "Heart"),
    ("arrow", "Arrow"),
];

/// Widget state; the value lists are editable (Widget preferences).
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetState {
    pub open: bool,
    /// The tool active last frame (picking the brush opens the widget).
    pub last_tool: &'static str,
    pub ring: Ring,
    pub brush: String,
    pub width: f64,
    pub density: f64,
    pub length: f64,
    pub stamp: String,
    pub color: [u8; 3],
    /// Sewing order of new strokes: top, before or after the selection.
    pub order: String,
    pub widths: Vec<f64>,
    pub densities: Vec<f64>,
    pub lengths: Vec<f64>,
}

impl Default for WidgetState {
    fn default() -> Self {
        WidgetState {
            open: false,
            last_tool: "",
            ring: Ring::Brush,
            brush: "steil".into(),
            width: 3.0,
            density: 0.4,
            length: 2.5,
            stamp: "free".into(),
            color: [0x1a, 0x3e, 0x8c],
            order: "top".into(),
            widths: vec![1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0],
            densities: vec![0.3, 0.35, 0.4, 0.45, 0.5, 0.6],
            lengths: vec![1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 5.0],
        }
    }
}

impl WidgetState {
    fn hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2])
    }

    /// Every brush setting, as `tool.setOption` values.
    pub fn values(&self) -> serde_json::Value {
        json!({
            "brush": self.brush,
            "width": self.width,
            "density": self.density,
            "length": self.length,
            "stamp": self.stamp,
            "color": self.hex(),
            "order": self.order,
        })
    }
}

/// Parses a widget value list ("1, 1.5, 2"): the positive numbers in it, sorted,
/// at most 12; `None` when it holds none.
pub fn parse_values(text: &str, max: f64) -> Option<Vec<f64>> {
    let mut v: Vec<f64> = text
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .filter(|x| x.is_finite() && *x > 0.0 && *x <= max)
        .collect();
    v.sort_by(f64::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    v.truncate(12);
    (!v.is_empty()).then_some(v)
}

/// A value list as the preferences field shows it.
pub fn format_values(v: &[f64]) -> String {
    v.iter().map(|x| format!("{x}")).collect::<Vec<_>>().join(", ")
}

/// Loads the widget's settings onto the brush tool (selecting it).
pub fn load(app: &mut VectorcraftApp) {
    let values = app.embroidery.widget.values();
    if app.session.tool_id() != TOOL {
        app.run("tool.select", json!({ "tool": TOOL })).ok();
    }
    app.run("tool.setOption", json!({ "tool": TOOL, "values": values })).ok();
}

const R_CENTER: f32 = 30.0;
const R_INNER: f32 = 66.0;
const R_OUTER: f32 = 112.0;

fn ring_segment(center: Pos2, r0: f32, r1: f32, a0: f32, a1: f32, fill: Color32, stroke: Stroke) -> Vec<Shape> {
    let steps = ((a1 - a0).abs() * 12.0).ceil().max(3.0) as usize;
    let mut pts = Vec::with_capacity(steps * 2 + 2);
    for k in 0..=steps {
        let a = a0 + (a1 - a0) * k as f32 / steps as f32;
        pts.push(center + vec2(a.cos(), a.sin()) * r1);
    }
    for k in (0..=steps).rev() {
        let a = a0 + (a1 - a0) * k as f32 / steps as f32;
        pts.push(center + vec2(a.cos(), a.sin()) * r0);
    }
    // A ring segment is not convex: fan it from its inner arc midpoint.
    let mut mesh = egui::epaint::Mesh::default();
    let n = steps + 1;
    for p in &pts {
        mesh.colored_vertex(*p, fill);
    }
    for k in 0..steps {
        let (o0, o1) = (k as u32, (k + 1) as u32);
        let (i0, i1) = ((2 * n - 1 - k) as u32, (2 * n - 2 - k) as u32);
        mesh.add_triangle(o0, o1, i1);
        mesh.add_triangle(o0, i1, i0);
    }
    let outline = Shape::closed_line(pts, stroke);
    vec![Shape::mesh(mesh), outline]
}

/// Where the widget sits: the canvas's lower-left corner.
fn placement(canvas: egui::Rect) -> (egui::Rect, Pos2) {
    let size = vec2(R_OUTER * 2.0 + 16.0, R_OUTER * 2.0 + 16.0);
    let rect = egui::Rect::from_min_size(pos2(canvas.left() + 12.0, canvas.bottom() - size.y - 12.0), size);
    (rect, rect.center())
}

/// Whether `p` is over the open widget (the canvas then leaves the pointer to it).
pub fn contains(app: &VectorcraftApp, canvas: egui::Rect, p: Option<Pos2>) -> bool {
    let (_, center) = placement(canvas);
    app.embroidery.widget.open && p.is_some_and(|p| p.distance(center) <= R_OUTER + 4.0)
}

/// A shape's outline, about 22 px across, centred on `at` (free drawing: a squiggle).
fn shape_icon(painter: &egui::Painter, at: Pos2, stamp: &str, stroke: Stroke) {
    use vectorcraft_embroidery::sf_core::Point;
    use vectorcraft_embroidery::sf_core::brush::{Stamp, stamp as make};
    const R: f64 = 11.0;
    let kind = match stamp {
        "line" => Stamp::Line,
        "ellipse" => Stamp::Ellipse,
        "rectangle" => Stamp::Rectangle,
        "roundedRectangle" => Stamp::RoundedRectangle,
        "triangle" => Stamp::Triangle,
        "diamond" => Stamp::Diamond,
        "hexagon" => Stamp::Hexagon,
        "star" => Stamp::Star,
        "heart" => Stamp::Heart,
        "arrow" => Stamp::Arrow,
        _ => {
            let pts: Vec<Pos2> = (0..=16).map(|k| at + vec2(-10.0 + k as f32 * 1.25, 5.0 * (k as f32 * 0.8).sin())).collect();
            painter.add(Shape::line(pts, stroke));
            return;
        }
    };
    let c = Point::new(f64::from(at.x), f64::from(at.y));
    let Some(contour) = make(kind, c - Point::new(R, R * 0.8), c + Point::new(R, R * 0.8), false) else { return };
    let mut pts: Vec<Pos2> = contour.polyline(0.3).into_iter().map(|q| pos2(q.x as f32, q.y as f32)).collect();
    if contour.closed
        && let Some(&first) = pts.first()
    {
        pts.push(first);
    }
    painter.add(Shape::line(pts, stroke));
}

/// Draws the widget in the canvas's lower-left corner and handles its clicks.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui, canvas: egui::Rect) {
    if !app.embroidery.widget.open {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let (rect, center) = placement(canvas);
    let resp = ui.interact(rect, ui.scope_id().with("embroidery-widget"), Sense::click());
    let painter = ui.painter_at(rect);
    let w = &app.embroidery.widget;
    let panel = Color32::from_rgba_unmultiplied(t.panel.r(), t.panel.g(), t.panel.b(), 236);
    painter.circle_filled(center, R_OUTER + 4.0, panel);
    painter.circle_stroke(center, R_OUTER + 4.0, Stroke::new(1.0, t.border));
    let font = egui::FontId::proportional(10.5);
    // Inner ring: categories.
    let n = Ring::ALL.len() as f32;
    let start = -std::f32::consts::FRAC_PI_2 - std::f32::consts::PI / n;
    for (i, ring) in Ring::ALL.iter().enumerate() {
        let a0 = start + i as f32 * std::f32::consts::TAU / n;
        let a1 = a0 + std::f32::consts::TAU / n;
        let fill = if *ring == w.ring { t.accent } else { t.button };
        for s in ring_segment(center, R_CENTER + 3.0, R_INNER, a0 + 0.02, a1 - 0.02, fill, Stroke::new(1.0, t.border)) {
            painter.add(s);
        }
        let mid = (a0 + a1) * 0.5;
        let p = center + vec2(mid.cos(), mid.sin()) * ((R_CENTER + R_INNER) * 0.5 + 1.5);
        let col = if *ring == w.ring { Color32::WHITE } else { t.text };
        painter.text(p, egui::Align2::CENTER_CENTER, tl!(ring.label()), font.clone(), col);
    }
    // Outer ring: the values of the chosen setting (shapes as small drawings).
    let (labels, current): (Vec<String>, usize) = match w.ring {
        Ring::Brush => (BRUSHES.iter().map(|b| tl!(b.1).to_string()).collect(), BRUSHES.iter().position(|b| b.0 == w.brush).unwrap_or(0)),
        Ring::Shape => (SHAPES.iter().map(|s| tl!(s.1).to_string()).collect(), SHAPES.iter().position(|s| s.0 == w.stamp).unwrap_or(0)),
        Ring::Width => {
            (w.widths.iter().map(|v| format!("{v}")).collect(), w.widths.iter().position(|v| (*v - w.width).abs() < 1e-9).unwrap_or(usize::MAX))
        }
        Ring::Density => (
            w.densities.iter().map(|v| format!("{v}")).collect(),
            w.densities.iter().position(|v| (*v - w.density).abs() < 1e-9).unwrap_or(usize::MAX),
        ),
        Ring::Length => {
            (w.lengths.iter().map(|v| format!("{v}")).collect(), w.lengths.iter().position(|v| (*v - w.length).abs() < 1e-9).unwrap_or(usize::MAX))
        }
    };
    let m = labels.len().max(1) as f32;
    let ostart = -std::f32::consts::FRAC_PI_2 - std::f32::consts::PI / m;
    for (i, label) in labels.iter().enumerate() {
        let a0 = ostart + i as f32 * std::f32::consts::TAU / m;
        let a1 = a0 + std::f32::consts::TAU / m;
        let fill = if i == current { t.accent_strong } else { t.input };
        for s in ring_segment(center, R_INNER + 4.0, R_OUTER, a0 + 0.015, a1 - 0.015, fill, Stroke::new(1.0, t.border)) {
            painter.add(s);
        }
        let mid = (a0 + a1) * 0.5;
        let p = center + vec2(mid.cos(), mid.sin()) * ((R_INNER + R_OUTER) * 0.5 + 2.0);
        let col = if i == current { Color32::WHITE } else { t.text };
        match (w.ring, SHAPES.get(i)) {
            (Ring::Shape, Some(shape)) => shape_icon(&painter, p, shape.0, Stroke::new(1.5, col)),
            _ => {
                painter.text(p, egui::Align2::CENTER_CENTER, label, font.clone(), col);
            }
        }
    }
    // Centre: thread colour.
    let c = Color32::from_rgb(w.color[0], w.color[1], w.color[2]);
    painter.circle_filled(center, R_CENTER, c);
    painter.circle_stroke(center, R_CENTER, Stroke::new(2.0, Color32::WHITE));
    // Clicks.
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let d = p - center;
        let r = d.length();
        let ang = d.y.atan2(d.x);
        let index = |start: f32, count: f32| (((ang - start).rem_euclid(std::f32::consts::TAU)) / (std::f32::consts::TAU / count)).floor() as usize;
        let w = &mut app.embroidery.widget;
        if r <= R_CENTER {
            // cycle through a few classic thread colours; the Embroidery panel has a full picker
            const COLORS: [[u8; 3]; 8] = [
                [0x1a, 0x3e, 0x8c],
                [0xd6, 0x28, 0x28],
                [0x11, 0x11, 0x11],
                [0xf6, 0xc9, 0x0e],
                [0x2a, 0x9d, 0x4b],
                [0xfa, 0xfa, 0xfa],
                [0xf7, 0x7f, 0x00],
                [0x6a, 0x2c, 0x91],
            ];
            let next = COLORS.iter().position(|k| *k == w.color).map_or(0, |i| (i + 1) % COLORS.len());
            w.color = COLORS[next];
        } else if r <= R_INNER {
            if let Some(ring) = Ring::ALL.get(index(start, n)) {
                w.ring = *ring;
            }
        } else if r <= R_OUTER {
            let i = index(ostart, m);
            match w.ring {
                Ring::Brush => {
                    if let Some(b) = BRUSHES.get(i) {
                        w.brush = b.0.into();
                    }
                }
                Ring::Shape => {
                    if let Some(s) = SHAPES.get(i) {
                        w.stamp = s.0.into();
                    }
                }
                Ring::Width => {
                    if let Some(v) = w.widths.get(i) {
                        w.width = *v;
                    }
                }
                Ring::Density => {
                    if let Some(v) = w.densities.get(i) {
                        w.density = *v;
                    }
                }
                Ring::Length => {
                    if let Some(v) = w.lengths.get(i) {
                        w.length = *v;
                    }
                }
            }
        }
        load(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_lists_parse_sorted_and_bounded() {
        assert_eq!(parse_values("3, 1.5 2;2", 50.0), Some(vec![1.5, 2.0, 3.0]));
        assert_eq!(parse_values("abc, -1, 0, 900", 50.0), None);
        assert_eq!(format_values(&[1.0, 2.5]), "1, 2.5");
        assert_eq!(parse_values(&(1..40).map(|i| i.to_string()).collect::<Vec<_>>().join(","), 50.0).map(|v| v.len()), Some(12));
    }
}
