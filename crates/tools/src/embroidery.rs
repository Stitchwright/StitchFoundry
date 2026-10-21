//! Embroidery tools.
//!
//! * **Inclination** (`embroideryDirection`): edit the stitch direction of the
//!   selected stitched object. Drag across the shape to add an inclination line,
//!   drag a line's bead to move that end, click a line and press Delete to remove
//!   it. Drag the green (start) or red (stop) bead to choose where sewing starts
//!   and stops.
//! * **Stitch Edit** (`embroideryStitches`): pick, move, add and delete the
//!   selected stitched object's stitches one by one.
//! * **Embroidery brush** (`embroideryBrush`): draw stitched objects freehand or as
//!   preset shapes with the digitizing widget's settings (brush, width, density,
//!   stitch length, shape, thread colour).
//! * **Column A/B** (`embroideryColumn`): click a satin column's two sides in turn;
//!   picked with a column selected, it continues that column.
//! * **Cross Stitch** (`embroideryCrossStitch`): paint a cross-stitch pattern's cells.

use serde_json::{Value, json};
use vectorcraft_doc::NodeId;
use vectorcraft_geom::Point;

use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

/// Screen pixels within which a bead can be grabbed.
const GRAB_PX: f64 = 7.0;

pub fn create(id: &str) -> Option<Box<dyn Tool>> {
    match id {
        "embroideryDirection" => Some(Box::new(DirectionTool::default())),
        "embroideryBrush" => Some(Box::new(BrushTool::default())),
        "embroideryStitches" => Some(Box::new(StitchTool::default())),
        "embroideryColumn" => Some(Box::new(ColumnTool::default())),
        "embroideryMotif" => Some(Box::new(MotifTool::default())),
        "embroideryClick" => Some(Box::new(ClickStitchTool::default())),
        "embroideryCutout" => Some(Box::new(CutoutBrushTool::default())),
        "embroideryCrossStitch" => Some(Box::new(CrossStitchTool::default())),
        _ => None,
    }
}

/// The stitched object under `p` (the innermost one carrying embroidery), else
/// the path under it.
fn pick(cx: &ToolContext, p: Point) -> Option<NodeId> {
    let hit = vectorcraft_doc::hit::hit_test(cx.doc, p, cx.hit_options())?;
    std::iter::once(hit.leaf)
        .chain(hit.ancestry.iter().rev().copied())
        .find(|id| cx.doc.node(*id).is_some_and(|n| n.embroidery.is_some()))
        .or(Some(hit.leaf))
}

fn pt(p: sf_core::Point) -> Point {
    Point::new(p.x, p.y)
}

/// The selected object whose stitch direction the tool edits.
fn target<'a>(cx: &ToolContext<'a>) -> Option<(NodeId, &'a sf_core::attr::Embroidery)> {
    cx.selection.objects.iter().find_map(|id| {
        let e = cx.doc.node(*id)?.embroidery.as_deref()?;
        Some((*id, e))
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Dragging one end of guide `index` (`true` = end `a`).
    End { index: usize, a: bool },
    /// Drawing a new guide from `from`.
    New { from: Point },
    /// Dragging the start (`true`) or stop bead.
    Bead { start: bool },
}

#[derive(Default)]
pub struct DirectionTool {
    drag: Option<Drag>,
    /// The guide picked with a click (Delete removes it).
    picked: Option<usize>,
    hover: Option<Point>,
}

impl DirectionTool {
    fn grab(&self, cx: &ToolContext, id: NodeId, e: &sf_core::attr::Embroidery, p: Point) -> Drag {
        let tol = cx.tol(GRAB_PX);
        // beads of the start and stop first (they sit on the outline)
        if let Some((_, s, t)) = cx.embroidery_beads.iter().find(|b| b.0 == id) {
            if (*s - p).hypot() <= tol {
                return Drag::Bead { start: true };
            }
            if (*t - p).hypot() <= tol {
                return Drag::Bead { start: false };
            }
        }
        for (i, g) in e.guides.iter().enumerate() {
            if (pt(g.a) - p).hypot() <= tol {
                return Drag::End { index: i, a: true };
            }
            if (pt(g.b) - p).hypot() <= tol {
                return Drag::End { index: i, a: false };
            }
        }
        Drag::New { from: p }
    }

    fn guide_under(&self, cx: &ToolContext, e: &sf_core::attr::Embroidery, p: Point) -> Option<usize> {
        let tol = cx.tol(GRAB_PX * 0.7);
        e.guides.iter().position(|g| {
            let (a, b) = (pt(g.a), pt(g.b));
            let ab = b - a;
            let l2 = ab.hypot2();
            if l2 < 1e-12 {
                return false;
            }
            let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
            (a + ab * t - p).hypot() <= tol
        })
    }
}

impl Tool for DirectionTool {
    fn id(&self) -> &'static str {
        "embroideryDirection"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let p = ev.pos;
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(p);
                vec![]
            }
            PointerKind::Down => {
                let Some((id, e)) = target(cx) else {
                    // Nothing to edit yet: pick the stitched object under the pointer.
                    return match pick(cx, p) {
                        Some(hit) => vec![Action::Exec("select.set".into(), json!({ "ids": [hit.0] }))],
                        None => vec![],
                    };
                };
                let d = self.grab(cx, id, e, p);
                self.picked = match d {
                    Drag::End { index, .. } => Some(index),
                    _ => self.guide_under(cx, e, p),
                };
                self.drag = Some(d);
                match d {
                    Drag::End { .. } => vec![Action::Begin("Move Inclination Line".into())],
                    Drag::Bead { .. } => vec![Action::Begin("Set Start and Stop".into())],
                    Drag::New { .. } => vec![],
                }
            }
            PointerKind::Drag => {
                let Some((id, _)) = target(cx) else { return vec![] };
                match self.drag {
                    Some(Drag::End { index, a }) => {
                        let key = if a { "a" } else { "b" };
                        vec![Action::Preview("embroidery.moveGuide".into(), json!({ "id": id.0, "index": index, key: [p.x, p.y] }))]
                    }
                    Some(Drag::Bead { start }) => {
                        let key = if start { "entry" } else { "exit" };
                        vec![Action::Preview("embroidery.setBeads".into(), json!({ "id": id.0, key: [p.x, p.y] }))]
                    }
                    Some(Drag::New { from }) => {
                        if (p - from).hypot() < cx.tol(4.0) {
                            return vec![];
                        }
                        let mut out = Vec::new();
                        if self.picked != Some(usize::MAX) {
                            out.push(Action::Begin("Add Inclination Line".into()));
                            self.picked = Some(usize::MAX);
                        }
                        out.push(Action::Preview("embroidery.addGuide".into(), json!({ "id": id.0, "a": [from.x, from.y], "b": [p.x, p.y] })));
                        out
                    }
                    _ => vec![],
                }
            }
            PointerKind::Up => {
                let d = self.drag.take();
                match d {
                    Some(Drag::End { .. }) | Some(Drag::Bead { .. }) => vec![Action::Commit],
                    Some(Drag::New { .. }) if self.picked == Some(usize::MAX) => {
                        self.picked = None;
                        vec![Action::Commit]
                    }
                    _ => vec![],
                }
            }
            PointerKind::DoubleClick => vec![],
        }
    }

    fn key(&mut self, cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        let Some((id, e)) = target(cx) else { return vec![] };
        match key {
            ToolKey::Delete | ToolKey::Backspace => match self.picked.filter(|i| *i < e.guides.len()) {
                Some(i) => {
                    self.picked = None;
                    vec![Action::Exec("embroidery.removeGuide".into(), json!({ "id": id.0, "index": i }))]
                }
                None => vec![],
            },
            ToolKey::Escape => {
                self.picked = None;
                if self.drag.take().is_some() { vec![Action::Cancel] } else { vec![] }
            }
            _ => vec![],
        }
    }

    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let Some((id, e)) = target(cx) else { return vec![] };
        let mut out = Vec::new();
        for (i, g) in e.guides.iter().enumerate() {
            let col = if self.picked == Some(i) { [255, 120, 0] } else { [30, 30, 30] };
            out.push(Overlay::Line { a: pt(g.a), b: pt(g.b), color: col, dashed: false });
            out.push(Overlay::Handle { p: pt(g.a), color: col });
            out.push(Overlay::Handle { p: pt(g.b), color: col });
        }
        if let Some((_, s, t)) = cx.embroidery_beads.iter().find(|b| b.0 == id) {
            out.push(Overlay::Handle { p: *s, color: [40, 190, 90] });
            out.push(Overlay::Label { p: *s, text: "start".into(), color: [40, 150, 70] });
            out.push(Overlay::Handle { p: *t, color: [220, 50, 50] });
            out.push(Overlay::Label { p: *t, text: "stop".into(), color: [190, 40, 40] });
        }
        if let (Some(Drag::New { from }), Some(h)) = (self.drag, self.hover) {
            out.push(Overlay::Line { a: from, b: h, color: [255, 120, 0], dashed: true });
        }
        out
    }

    fn cursor(&self, cx: &ToolContext, p: Point, _mods: Mods) -> Cursor {
        match target(cx) {
            Some((id, e)) => match self.grab(cx, id, e, p) {
                Drag::New { .. } => Cursor::Crosshair,
                _ => Cursor::Move,
            },
            None => Cursor::Arrow,
        }
    }

    fn busy(&self) -> bool {
        self.drag.is_some()
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.picked = None;
        if self.drag.take().is_some() { vec![Action::Cancel] } else { vec![] }
    }
}

/// Screen pixels within which a needle point can be picked.
const NEEDLE_PX: f64 = 6.0;

/// What a Stitch Edit drag does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum StitchDrag {
    /// Moving the picked needle points by the pointer's offset from `from`.
    Move { from: Point, started: bool },
    /// Picking the needle points inside a rectangle.
    Marquee { from: Point, to: Point, add: bool },
}

/// **Stitch Edit** (`embroideryStitches`): edit the selected stitched object's
/// stitches one by one. Click a needle point to pick it (Shift adds or removes),
/// drag across empty canvas to pick the points inside, drag picked points to move
/// them, Alt-click between two stitches to add one, Delete removes the picked
/// ones. The first edit freezes the object's stitches (Regenerate Stitches undoes
/// that).
#[derive(Default)]
pub struct StitchTool {
    /// The object whose stitches are picked.
    node: Option<NodeId>,
    /// Picked record indices, sorted.
    picked: Vec<usize>,
    drag: Option<StitchDrag>,
}

/// The selected stitched object and its own stitches (document points).
fn stitch_target<'a>(cx: &ToolContext<'a>) -> Option<(NodeId, &'a [(Point, sf_core::Cmd)])> {
    cx.embroidery_stitches.iter().find(|(id, st)| cx.selection.objects.contains(id) && !st.is_empty()).map(|(id, st)| (*id, st.as_slice()))
}

impl StitchTool {
    /// The needle point nearest `p` within reach.
    fn needle(cx: &ToolContext, st: &[(Point, sf_core::Cmd)], p: Point) -> Option<usize> {
        let tol = cx.tol(NEEDLE_PX);
        st.iter()
            .enumerate()
            .filter(|(_, (_, c))| *c == sf_core::Cmd::Stitch || *c == sf_core::Cmd::Jump)
            .map(|(i, (q, _))| (i, (*q - p).hypot()))
            .filter(|(_, d)| *d <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The stitch (from record `i` to `i + 1`) passing nearest `p` within reach.
    fn segment(cx: &ToolContext, st: &[(Point, sf_core::Cmd)], p: Point) -> Option<usize> {
        let tol = cx.tol(NEEDLE_PX);
        st.windows(2)
            .enumerate()
            .filter(|(_, w)| w[1].1 == sf_core::Cmd::Stitch)
            .map(|(i, w)| {
                let (a, b) = (w[0].0, w[1].0);
                let ab = b - a;
                let l2 = ab.hypot2();
                let t = if l2 < 1e-12 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
                (i, (a + ab * t - p).hypot())
            })
            .filter(|(_, d)| *d <= tol)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn sync(&mut self, id: NodeId, len: usize) {
        if self.node != Some(id) {
            self.node = Some(id);
            self.picked.clear();
        }
        self.picked.retain(|i| *i < len);
    }
}

impl Tool for StitchTool {
    fn id(&self) -> &'static str {
        "embroideryStitches"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let p = ev.pos;
        match ev.kind {
            PointerKind::Down => {
                let Some((id, st)) = stitch_target(cx) else {
                    return match pick(cx, p) {
                        Some(hit) => vec![Action::Exec("select.set".into(), json!({ "ids": [hit.0] }))],
                        None => vec![],
                    };
                };
                self.sync(id, st.len());
                if ev.mods.alt {
                    // a stitch in the middle of the one under the pointer
                    let Some(i) = Self::segment(cx, st, p) else { return vec![] };
                    self.picked = vec![i + 1];
                    return vec![Action::Exec(
                        "embroidery.stitches.edit".into(),
                        json!({ "id": id.0, "ops": [{ "op": "insert", "after": i, "at": [p.x, p.y] }] }),
                    )];
                }
                match Self::needle(cx, st, p) {
                    Some(i) => {
                        if ev.mods.shift {
                            match self.picked.binary_search(&i) {
                                Ok(k) => {
                                    self.picked.remove(k);
                                }
                                Err(k) => self.picked.insert(k, i),
                            }
                        } else if !self.picked.contains(&i) {
                            self.picked = vec![i];
                        }
                        self.drag = (!self.picked.is_empty()).then_some(StitchDrag::Move { from: p, started: false });
                    }
                    None => {
                        if !ev.mods.shift {
                            self.picked.clear();
                        }
                        self.drag = Some(StitchDrag::Marquee { from: p, to: p, add: ev.mods.shift });
                    }
                }
                vec![]
            }
            PointerKind::Drag => {
                let Some((id, _)) = stitch_target(cx) else { return vec![] };
                match self.drag {
                    Some(StitchDrag::Move { from, started }) => {
                        if !started && (p - from).hypot() < cx.tol(3.0) {
                            return vec![];
                        }
                        let mut out = Vec::new();
                        if !started {
                            out.push(Action::Begin("Move Stitches".into()));
                            self.drag = Some(StitchDrag::Move { from, started: true });
                        }
                        let by = p - from;
                        out.push(Action::Preview(
                            "embroidery.stitches.edit".into(),
                            json!({ "id": id.0, "ops": [{ "op": "move", "indices": self.picked, "by": [by.x, by.y] }] }),
                        ));
                        out
                    }
                    Some(StitchDrag::Marquee { from, add, .. }) => {
                        self.drag = Some(StitchDrag::Marquee { from, to: p, add });
                        vec![]
                    }
                    None => vec![],
                }
            }
            PointerKind::Up => match self.drag.take() {
                Some(StitchDrag::Move { started: true, .. }) => vec![Action::Commit],
                Some(StitchDrag::Marquee { from, to, add }) => {
                    if let Some((id, st)) = stitch_target(cx) {
                        self.sync(id, st.len());
                        let (lo, hi) = (Point::new(from.x.min(to.x), from.y.min(to.y)), Point::new(from.x.max(to.x), from.y.max(to.y)));
                        let inside = st.iter().enumerate().filter(|(_, (q, c))| {
                            matches!(c, sf_core::Cmd::Stitch | sf_core::Cmd::Jump) && q.x >= lo.x && q.x <= hi.x && q.y >= lo.y && q.y <= hi.y
                        });
                        if !add {
                            self.picked.clear();
                        }
                        self.picked.extend(inside.map(|(i, _)| i));
                        self.picked.sort_unstable();
                        self.picked.dedup();
                    }
                    vec![]
                }
                _ => vec![],
            },
            PointerKind::Move | PointerKind::DoubleClick => vec![],
        }
    }

    fn key(&mut self, cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        let Some((id, st)) = stitch_target(cx) else { return vec![] };
        self.sync(id, st.len());
        match key {
            ToolKey::Delete | ToolKey::Backspace if !self.picked.is_empty() => {
                let indices = std::mem::take(&mut self.picked);
                vec![Action::Exec("embroidery.stitches.edit".into(), json!({ "id": id.0, "ops": [{ "op": "delete", "indices": indices }] }))]
            }
            ToolKey::Escape => {
                self.picked.clear();
                if self.drag.take().is_some_and(|d| matches!(d, StitchDrag::Move { started: true, .. })) { vec![Action::Cancel] } else { vec![] }
            }
            _ => vec![],
        }
    }

    /// The object whose stitches are picked and the picked indices (the panel splits an
    /// object at a single picked stitch).
    fn options(&self) -> Value {
        json!({ "id": self.node.map(|n| n.0), "picked": self.picked })
    }

    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let Some((id, st)) = stitch_target(cx) else { return vec![] };
        let mut out = Vec::with_capacity(st.len() + 2);
        let picked = |i: usize| self.node == Some(id) && self.picked.binary_search(&i).is_ok();
        for (i, (q, c)) in st.iter().enumerate() {
            match c {
                sf_core::Cmd::Stitch => out.push(Overlay::Anchor { p: *q, color: [30, 30, 30], filled: picked(i), size: 4.0 }),
                sf_core::Cmd::Jump => out.push(Overlay::Anchor { p: *q, color: [40, 110, 220], filled: picked(i), size: 4.0 }),
                sf_core::Cmd::Trim => out.push(Overlay::Label { p: *q, text: "trim".into(), color: [190, 40, 40] }),
                sf_core::Cmd::Stop => out.push(Overlay::Label { p: *q, text: "stop".into(), color: [190, 120, 0] }),
                _ => {}
            }
        }
        if let Some(StitchDrag::Marquee { from, to, .. }) = self.drag {
            out.push(Overlay::Marquee(vectorcraft_geom::Rect::from_points(from, to)));
        }
        out
    }

    fn cursor(&self, cx: &ToolContext, p: Point, mods: Mods) -> Cursor {
        match stitch_target(cx) {
            Some((_, st)) if mods.alt => {
                if Self::segment(cx, st, p).is_some() {
                    Cursor::Crosshair
                } else {
                    Cursor::Arrow
                }
            }
            Some((_, st)) if Self::needle(cx, st, p).is_some() => Cursor::Move,
            _ => Cursor::Arrow,
        }
    }

    fn busy(&self) -> bool {
        self.drag.is_some()
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.picked.clear();
        if self.drag.take().is_some_and(|d| matches!(d, StitchDrag::Move { started: true, .. })) { vec![Action::Cancel] } else { vec![] }
    }
}

/// The digitizing widget's brush settings.
#[derive(Clone, Debug, PartialEq)]
pub struct BrushSettings {
    /// run | bean | steil | tapered | calligraphy | pressure | applique | fill | satin
    pub brush: String,
    /// Satin width / appliqué cover width (mm).
    pub width: f64,
    /// Density: satin / fill spacing (mm).
    pub density: f64,
    /// Stitch length (mm).
    pub length: f64,
    /// Preset shape (free = freehand).
    pub stamp: String,
    /// Thread colour `#rrggbb`.
    pub color: String,
    /// Where a stroke goes in the sewing order: `top` (last), or `before` / `after`
    /// the object selected when the stroke starts (Digitize Before / After).
    pub order: String,
}

impl Default for BrushSettings {
    fn default() -> Self {
        BrushSettings {
            brush: "steil".into(),
            width: 3.0,
            density: 0.4,
            length: 2.5,
            stamp: "free".into(),
            color: "#1a3e8c".into(),
            order: "top".into(),
        }
    }
}

#[derive(Default)]
pub struct BrushTool {
    pub settings: BrushSettings,
    points: Vec<Point>,
    pressures: Vec<f32>,
    from: Option<Point>,
    drawing: bool,
    shift: bool,
    /// The stitched object selected when the stroke began (Digitize Before / After).
    anchor: Option<NodeId>,
}

impl BrushTool {
    fn params(&self) -> Value {
        let s = &self.settings;
        let mut p = json!({
            "brush": s.brush,
            "width": s.width,
            "density": s.density,
            "length": s.length,
            "color": s.color,
        });
        if s.stamp != "free" {
            if let (Some(from), Some(to)) = (self.from, self.points.last()) {
                p["stamp"] = json!(s.stamp);
                p["from"] = json!([from.x, from.y]);
                p["to"] = json!([to.x, to.y]);
                p["square"] = json!(self.shift);
            }
        } else {
            p["points"] = Value::Array(self.points.iter().map(|q| json!([q.x, q.y])).collect());
            if s.brush == "pressure" {
                p["pressures"] = json!(self.pressures);
            }
        }
        if let (Some(anchor), "before" | "after") = (self.anchor, s.order.as_str()) {
            p["order"] = json!(s.order);
            p["relativeTo"] = json!(anchor.0);
        }
        p
    }
}

impl Tool for BrushTool {
    fn id(&self) -> &'static str {
        "embroideryBrush"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let p = ev.pos;
        match ev.kind {
            PointerKind::Down => {
                // Strokes start and end on anchors and guides (Snap to Point, Smart Guides).
                let (p, _) = crate::guides::snap_draw(cx, p, &[]);
                self.anchor = cx.selection.objects.iter().copied().find(|id| cx.doc.node(*id).is_some_and(|n| n.embroidery.is_some()));
                self.points = vec![p];
                self.pressures = vec![ev.pressure];
                self.from = Some(p);
                self.drawing = true;
                self.shift = ev.mods.shift;
                vec![Action::Begin("Brush Stroke".into())]
            }
            PointerKind::Drag if self.drawing => {
                self.shift = ev.mods.shift;
                let far = self.points.last().is_none_or(|q| (*q - p).hypot() >= cx.tol(1.5));
                if !far && self.settings.stamp == "free" {
                    return vec![];
                }
                if self.settings.stamp == "free" {
                    self.points.push(p);
                    self.pressures.push(ev.pressure);
                } else {
                    self.points = vec![self.from.unwrap_or(p), p];
                }
                // Preview while drawing: every few samples keeps big fills responsive.
                if self.points.len() < 3 || self.points.len().is_multiple_of(4) || self.settings.stamp != "free" {
                    vec![Action::Preview("embroidery.draw".into(), self.params())]
                } else {
                    vec![]
                }
            }
            PointerKind::Up if self.drawing => {
                self.drawing = false;
                let (end, _) = crate::guides::snap_draw(cx, p, &[]);
                if self.settings.stamp == "free" {
                    if self.points.last().is_some_and(|q| (*q - p).hypot() < cx.tol(1.5)) {
                        self.points.pop();
                    }
                    self.points.push(end);
                } else {
                    self.points = vec![self.from.unwrap_or(end), end];
                }
                let long_enough = match (self.from, self.points.last()) {
                    (Some(a), Some(b)) => self.points.len() >= 2 && (*b - a).hypot() + self.points.len() as f64 * 0.01 > cx.tol(3.0),
                    _ => false,
                };
                if long_enough { vec![Action::Preview("embroidery.draw".into(), self.params()), Action::Commit] } else { vec![Action::Cancel] }
            }
            _ => vec![],
        }
    }

    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape && self.drawing {
            self.drawing = false;
            return vec![Action::Cancel];
        }
        vec![]
    }

    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        if !self.drawing || self.points.len() < 2 || self.settings.stamp != "free" {
            return vec![];
        }
        let mut path = vectorcraft_geom::BezPath::new();
        for (i, q) in self.points.iter().enumerate() {
            if i == 0 {
                path.move_to(*q);
            } else {
                path.line_to(*q);
            }
        }
        vec![Overlay::Path { path, color: [255, 120, 0], width: 1.0, dashed: true }]
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }

    fn options(&self) -> Value {
        let s = &self.settings;
        json!({ "brush": s.brush, "width": s.width, "density": s.density, "length": s.length, "stamp": s.stamp, "color": s.color, "order": s.order })
    }

    fn set_option(&mut self, key: &str, value: &Value) {
        let s = &mut self.settings;
        let num = |v: &Value, lo: f64, hi: f64| v.as_f64().filter(|x| x.is_finite()).map(|x| x.clamp(lo, hi));
        match key {
            "brush" => {
                if let Some(v) = value.as_str()
                    && matches!(v, "run" | "bean" | "steil" | "tapered" | "calligraphy" | "pressure" | "applique" | "fill" | "satin")
                {
                    s.brush = v.to_string();
                }
            }
            "width" => {
                if let Some(v) = num(value, 0.3, 50.0) {
                    s.width = v;
                }
            }
            "density" => {
                if let Some(v) = num(value, 0.1, 10.0) {
                    s.density = v;
                }
            }
            "length" => {
                if let Some(v) = num(value, 0.3, 12.0) {
                    s.length = v;
                }
            }
            "stamp" => {
                if let Some(v) = value.as_str() {
                    s.stamp = v.to_string();
                }
            }
            "color" => {
                if let Some(v) = value.as_str().filter(|v| v.starts_with('#') && v.len() == 7) {
                    s.color = v.to_string();
                }
            }
            "order" => {
                if let Some(v) = value.as_str().filter(|v| matches!(*v, "top" | "before" | "after")) {
                    s.order = v.to_string();
                }
            }
            _ => {}
        }
    }

    fn busy(&self) -> bool {
        self.drawing
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        if std::mem::take(&mut self.drawing) { vec![Action::Cancel] } else { vec![] }
    }
}

/// Motif tool: click to place one motif, 10 mm long, or drag across the canvas for one as long
/// as the drag and turned its way (`embroidery.placeMotif`). Which motif is the tool's own
/// setting (`motif`: a built-in motif's name or one of the design's motifs).
pub struct MotifTool {
    pub motif: String,
    from: Option<Point>,
}

impl Default for MotifTool {
    fn default() -> Self {
        MotifTool { motif: "Wave".into(), from: None }
    }
}

/// Points in a millimetre.
const PT_PER_MM: f64 = 72.0 / 25.4;

impl MotifTool {
    /// What a press at `a` released at `b` places: a click, the motif at its own size; a drag,
    /// one from `a` to `b`.
    fn params(&self, cx: &ToolContext, a: Point, b: Point) -> Value {
        let d = b - a;
        if d.hypot() < cx.tol(3.0) {
            return json!({ "motif": self.motif, "at": [a.x, a.y] });
        }
        let size = (d.hypot() / PT_PER_MM).clamp(1.0, 500.0);
        json!({ "motif": self.motif, "at": [(a.x + b.x) * 0.5, (a.y + b.y) * 0.5], "size": size, "angle": d.y.atan2(d.x).to_degrees() })
    }
}

impl Tool for MotifTool {
    fn id(&self) -> &'static str {
        "embroideryMotif"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Down => {
                let (p, _) = crate::guides::snap_draw(cx, ev.pos, &[]);
                self.from = Some(p);
                vec![Action::Begin("Place Motif".into()), Action::Preview("embroidery.placeMotif".into(), self.params(cx, p, p))]
            }
            PointerKind::Drag => match self.from {
                Some(a) => vec![Action::Preview("embroidery.placeMotif".into(), self.params(cx, a, ev.pos))],
                None => vec![],
            },
            PointerKind::Up => match self.from.take() {
                Some(a) => {
                    let (b, _) = crate::guides::snap_draw(cx, ev.pos, &[]);
                    vec![Action::Preview("embroidery.placeMotif".into(), self.params(cx, a, b)), Action::Commit]
                }
                None => vec![],
            },
            _ => vec![],
        }
    }

    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape && self.from.take().is_some() {
            return vec![Action::Cancel];
        }
        vec![]
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }

    fn options(&self) -> Value {
        json!({ "motif": self.motif })
    }

    fn set_option(&mut self, key: &str, value: &Value) {
        if key == "motif"
            && let Some(v) = value.as_str().filter(|v| !v.trim().is_empty() && v.len() <= 200)
        {
            self.motif = v.to_string();
        }
    }

    fn busy(&self) -> bool {
        self.from.is_some()
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        if self.from.take().is_some() { vec![Action::Cancel] } else { vec![] }
    }
}

/// Click-to-Stitch tool: a click on an image stitches the region round the spot
/// (`embroidery.clickStitch`), as the tool's settings say: `as` (fill, fillNoHoles, satin,
/// outline or centerline) and `tolerance`.
pub struct ClickStitchTool {
    pub stitch_as: String,
    pub tolerance: u8,
}

impl Default for ClickStitchTool {
    fn default() -> Self {
        ClickStitchTool { stitch_as: "fill".into(), tolerance: 32 }
    }
}

impl Tool for ClickStitchTool {
    fn id(&self) -> &'static str {
        "embroideryClick"
    }

    fn pointer(&mut self, _cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Up => vec![Action::Exec(
                "embroidery.clickStitch".into(),
                json!({ "at": [ev.pos.x, ev.pos.y], "as": self.stitch_as, "tolerance": self.tolerance }),
            )],
            _ => vec![],
        }
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }

    fn options(&self) -> Value {
        json!({ "as": self.stitch_as, "tolerance": self.tolerance })
    }

    fn set_option(&mut self, key: &str, value: &Value) {
        match key {
            "as" => {
                if let Some(v) = value.as_str().filter(|v| ["fill", "fillNoHoles", "satin", "outline", "centerline"].contains(v)) {
                    self.stitch_as = v.to_string();
                }
            }
            "tolerance" => {
                if let Some(v) = value.as_u64().and_then(|v| u8::try_from(v).ok()) {
                    self.tolerance = v;
                }
            }
            _ => {}
        }
    }
}

/// Cut-Out Brush tool: paint over a cut-out photo (made by Remove Background) to put back what it
/// left out, or with Alt held (or `mode: remove`) to take away background it kept; on release the
/// cut-out is made again with the stroke (`embroidery.image.refineCutout`), one step per stroke.
/// Option `size`: the brush's width (pt).
pub struct CutoutBrushTool {
    pub remove: bool,
    pub size: f64,
    points: Vec<Point>,
    erasing: bool,
}

impl Default for CutoutBrushTool {
    fn default() -> Self {
        CutoutBrushTool { remove: false, size: 12.0, points: Vec::new(), erasing: false }
    }
}

impl Tool for CutoutBrushTool {
    fn id(&self) -> &'static str {
        "embroideryCutout"
    }

    fn pointer(&mut self, _cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Down => {
                self.points = vec![ev.pos];
                self.erasing = self.remove != ev.mods.alt;
                vec![]
            }
            PointerKind::Drag => {
                // a point every third of the brush's radius is enough to follow the stroke
                if self.points.last().is_some_and(|q| q.distance(ev.pos) >= self.size / 6.0) {
                    self.points.push(ev.pos);
                }
                vec![]
            }
            PointerKind::Up => {
                if self.points.is_empty() {
                    return vec![];
                }
                self.points.push(ev.pos);
                let pts: Vec<[f64; 2]> = std::mem::take(&mut self.points).into_iter().map(|q| [q.x, q.y]).collect();
                let key = if self.erasing { "remove" } else { "keep" };
                vec![Action::Exec("embroidery.image.refineCutout".into(), json!({ key: pts, "radius": self.size / 2.0 }))]
            }
            _ => vec![],
        }
    }

    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        let Some((first, rest)) = self.points.split_first() else { return vec![] };
        let mut path = vectorcraft_geom::BezPath::new();
        path.move_to(*first);
        for q in rest {
            path.line_to(*q);
        }
        let color = if self.erasing { [220, 50, 50] } else { [40, 170, 80] };
        vec![Overlay::Path { path, color, width: 3.0, dashed: false }]
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }

    fn busy(&self) -> bool {
        !self.points.is_empty()
    }

    fn options(&self) -> Value {
        json!({ "mode": if self.remove { "remove" } else { "keep" }, "size": self.size })
    }

    fn set_option(&mut self, key: &str, value: &Value) {
        match key {
            "mode" => {
                if let Some(v) = value.as_str().filter(|v| ["keep", "remove"].contains(v)) {
                    self.remove = v == "remove";
                }
            }
            "size" => {
                if let Some(v) = value.as_f64().filter(|v| v.is_finite() && *v >= 1.0 && *v <= 500.0) {
                    self.size = v;
                }
            }
            _ => {}
        }
    }
}

/// Cross Stitch tool: paint a cross-stitch pattern's cells by clicking or dragging over its grid
/// (`embroidery.crossStitch.paint`), in the tool's colour; with `erase` on, or Alt held, the
/// cells are cleared instead. A whole stroke is one step (Undo takes it back). With `fill` on, a
/// click fills the patch of touching squares alike instead (`embroidery.crossStitch.fill`); with
/// `line` on, a drag draws a backstitch line from grid corner to grid corner, and an Alt-click
/// takes the lines there away (`embroidery.crossStitch.backstitch`). Options: `color`
/// (`#rrggbb`), `erase`, `fill`, `line` and `size` (a square brush, 1 to 9 cells).
pub struct CrossStitchTool {
    pub color: String,
    pub erase: bool,
    pub fill: bool,
    pub line: bool,
    line_start: Option<Point>,
    pub size: u8,
    stroke: Vec<Point>,
    erasing: bool,
}

impl Default for CrossStitchTool {
    fn default() -> Self {
        CrossStitchTool {
            color: "#c0392b".into(),
            erase: false,
            fill: false,
            line: false,
            line_start: None,
            size: 1,
            stroke: Vec::new(),
            erasing: false,
        }
    }
}

/// Most points one stroke keeps (the command's own limit).
const MAX_STROKE: usize = 20_000;

impl CrossStitchTool {
    fn params(&self) -> Value {
        let at: Vec<[f64; 2]> = self.stroke.iter().map(|p| [p.x, p.y]).collect();
        let mut v = json!({ "at": at, "size": self.size, "erase": self.erasing });
        if !self.erasing {
            v["color"] = json!(self.color);
        }
        v
    }
}

impl Tool for CrossStitchTool {
    fn id(&self) -> &'static str {
        "embroideryCrossStitch"
    }

    fn pointer(&mut self, _cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        const CMD: &str = "embroidery.crossStitch.paint";
        const LINE: &str = "embroidery.crossStitch.backstitch";
        let line = |start: Point, end: Point, color: &str| json!({ "at": [[start.x, start.y], [end.x, end.y]], "color": color });
        match ev.kind {
            PointerKind::Down if self.line && (self.erase || ev.mods.alt) => {
                let p = json!({ "at": [[ev.pos.x, ev.pos.y]], "erase": true });
                vec![Action::Begin("Erase Backstitch".into()), Action::Preview(LINE.into(), p), Action::Commit]
            }
            PointerKind::Down if self.line => {
                self.line_start = Some(ev.pos);
                vec![Action::Begin("Backstitch Line".into()), Action::Preview(LINE.into(), line(ev.pos, ev.pos, &self.color))]
            }
            PointerKind::Drag if self.line_start.is_some() => match self.line_start {
                Some(start) => vec![Action::Preview(LINE.into(), line(start, ev.pos, &self.color))],
                None => vec![],
            },
            PointerKind::Up if self.line_start.is_some() => match self.line_start.take() {
                Some(start) => vec![Action::Preview(LINE.into(), line(start, ev.pos, &self.color)), Action::Commit],
                None => vec![],
            },
            PointerKind::Down if self.fill => {
                let erasing = self.erase || ev.mods.alt;
                let mut p = json!({ "at": [ev.pos.x, ev.pos.y], "erase": erasing });
                if !erasing {
                    p["color"] = json!(self.color);
                }
                let label = if erasing { "Erase Cross Stitches" } else { "Fill Cross Stitches" };
                vec![Action::Begin(label.into()), Action::Preview("embroidery.crossStitch.fill".into(), p), Action::Commit]
            }
            PointerKind::Down => {
                self.erasing = self.erase || ev.mods.alt;
                self.stroke = vec![ev.pos];
                let label = if self.erasing { "Erase Cross Stitches" } else { "Paint Cross Stitches" };
                vec![Action::Begin(label.into()), Action::Preview(CMD.into(), self.params())]
            }
            PointerKind::Drag if !self.stroke.is_empty() => {
                if self.stroke.len() < MAX_STROKE {
                    self.stroke.push(ev.pos);
                }
                vec![Action::Preview(CMD.into(), self.params())]
            }
            PointerKind::Up if !self.stroke.is_empty() => {
                if self.stroke.len() < MAX_STROKE {
                    self.stroke.push(ev.pos);
                }
                let actions = vec![Action::Preview(CMD.into(), self.params()), Action::Commit];
                self.stroke.clear();
                actions
            }
            _ => vec![],
        }
    }

    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        if key == ToolKey::Escape && (!self.stroke.is_empty() || self.line_start.is_some()) {
            self.stroke.clear();
            self.line_start = None;
            return vec![Action::Cancel];
        }
        vec![]
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }

    fn options(&self) -> Value {
        json!({ "color": self.color, "erase": self.erase, "fill": self.fill, "line": self.line, "size": self.size })
    }

    fn set_option(&mut self, key: &str, value: &Value) {
        match key {
            "color" => {
                if let Some(v) =
                    value.as_str().filter(|v| v.len() == 7 && v.strip_prefix('#').is_some_and(|h| h.chars().all(|c| c.is_ascii_hexdigit())))
                {
                    self.color = v.to_ascii_lowercase();
                }
            }
            "erase" => {
                if let Some(v) = value.as_bool() {
                    self.erase = v;
                }
            }
            "fill" => {
                if let Some(v) = value.as_bool() {
                    self.fill = v;
                }
            }
            "line" => {
                if let Some(v) = value.as_bool() {
                    self.line = v;
                }
            }
            "size" => {
                if let Some(v) = value.as_u64().filter(|v| (1..=9).contains(v)) {
                    self.size = v as u8;
                }
            }
            _ => {}
        }
    }

    fn busy(&self) -> bool {
        !self.stroke.is_empty() || self.line_start.is_some()
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        if self.stroke.is_empty() && self.line_start.is_none() {
            return vec![];
        }
        self.stroke.clear();
        self.line_start = None;
        vec![Action::Cancel]
    }
}

/// Column A/B: click points on a column's two sides in turn (first side, second side,
/// first…), Alt-click for a smooth point; the stitches turn with each pair. Every click is a
/// step of its own (Undo takes it back). Enter or Escape ends the column; Backspace takes the
/// last point back. Picking the tool with a two-sided satin column selected continues that
/// column (continue digitizing).
#[derive(Default)]
pub struct ColumnTool {
    /// The column being drawn.
    id: Option<NodeId>,
    /// Its points in clicking order (sides A, B, A…) and whether each is smooth.
    points: Vec<(Point, bool)>,
    hover: Option<Point>,
    /// A click awaits its mouse-up (its interaction is open).
    pressed: bool,
    /// The first click since the tool was picked: it may continue the selected column.
    started: bool,
}

/// The points of a column object (its two sides' anchors in turn), when `n` is one: a
/// satin path of one open side or two open sides whose anchor counts allow taking turns.
pub fn column_points(n: &vectorcraft_doc::Node) -> Option<Vec<(Point, bool)>> {
    if !n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, sf_core::attr::StitchType::Satin(_))) {
        return None;
    }
    let subs = &n.path_data()?.subpaths;
    if subs.is_empty() || subs.len() > 2 || subs.iter().any(|s| s.closed) {
        return None;
    }
    let a = &subs[0].anchors;
    let b = subs.get(1).map(|s| s.anchors.as_slice()).unwrap_or_default();
    if b.len() > a.len() || a.len() > b.len() + 1 || (subs.len() == 1 && a.len() != 1) {
        return None;
    }
    let smooth = |x: &vectorcraft_geom::path::Anchor| x.kind == vectorcraft_geom::path::AnchorKind::Smooth;
    let mut out = Vec::with_capacity(a.len() + b.len());
    for (i, x) in a.iter().enumerate() {
        out.push((x.p, smooth(x)));
        if let Some(y) = b.get(i) {
            out.push((y.p, smooth(y)));
        }
    }
    Some(out)
}

impl ColumnTool {
    fn params(&self) -> Value {
        let mut p = json!({ "points": self.points.iter().map(|(q, c)| json!([q.x, q.y, c])).collect::<Vec<_>>() });
        if let Some(id) = self.id {
            p["id"] = json!(id.0);
        }
        p
    }

    fn reset(&mut self) {
        self.id = None;
        self.points.clear();
    }

    /// Before a click: the points of the column being drawn as the document has them now
    /// (after an undo, say), or of the selected column on the first click.
    fn sync(&mut self, cx: &ToolContext) {
        let selected = match cx.selection.objects.as_slice() {
            [one] => Some(*one),
            _ => None,
        };
        let column = |id: NodeId| cx.doc.node(id).and_then(column_points);
        match self.id {
            Some(id) if selected == Some(id) => match column(id) {
                Some(pts) => self.points = pts,
                None => self.reset(),
            },
            // the selection moved on: the next click starts a new column
            Some(_) => self.reset(),
            // the first click continues the selected column, when it is one
            None if !self.started => {
                if let Some((id, pts)) = selected.and_then(|id| column(id).map(|pts| (id, pts))) {
                    self.id = Some(id);
                    self.points = pts;
                }
            }
            // a column whose object is not known yet: keep its points
            None => {}
        }
    }
}

impl Tool for ColumnTool {
    fn id(&self) -> &'static str {
        "embroideryColumn"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(ev.pos);
                vec![]
            }
            PointerKind::Down => {
                self.sync(cx);
                self.started = true;
                let (p, _) = crate::guides::snap_draw(cx, ev.pos, &[]);
                self.points.push((p, ev.mods.alt));
                self.pressed = true;
                vec![Action::Begin("Column A/B".into()), Action::Preview("embroidery.column".into(), self.params()), Action::Notify("column".into())]
            }
            PointerKind::Drag if self.pressed => {
                // dragging places the point where the button is let go
                let (p, _) = crate::guides::snap_draw(cx, ev.pos, &[]);
                if let Some(last) = self.points.last_mut() {
                    last.0 = p;
                }
                self.hover = Some(p);
                vec![Action::Preview("embroidery.column".into(), self.params())]
            }
            PointerKind::Up if self.pressed => {
                self.pressed = false;
                vec![Action::Commit]
            }
            _ => vec![],
        }
    }

    fn notify(&mut self, cx: &ToolContext, what: &str) {
        if what == "column" && self.id.is_none() {
            self.id = cx.selection.objects.first().copied();
        }
    }

    fn key(&mut self, cx: &ToolContext, key: ToolKey, _mods: Mods) -> Vec<Action> {
        match key {
            ToolKey::Enter | ToolKey::Escape => {
                let open = std::mem::take(&mut self.pressed);
                self.reset();
                self.started = true;
                if open { vec![Action::Cancel] } else { vec![] }
            }
            ToolKey::Backspace | ToolKey::Delete if !self.pressed && self.id.is_some() => {
                self.sync(cx);
                let Some(id) = self.id else { return vec![] };
                self.points.pop();
                if self.points.is_empty() {
                    self.reset();
                    return vec![Action::Exec("edit.clear".into(), json!({ "ids": [id.0] }))];
                }
                vec![Action::Exec("embroidery.column".into(), self.params())]
            }
            _ => vec![],
        }
    }

    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        const ORANGE: [u8; 3] = [255, 120, 0];
        let mut out = Vec::new();
        let line = |pts: &[Point], dashed: bool| {
            let mut path = vectorcraft_geom::BezPath::new();
            for (i, q) in pts.iter().enumerate() {
                if i == 0 {
                    path.move_to(*q);
                } else {
                    path.line_to(*q);
                }
            }
            Overlay::Path { path, color: ORANGE, width: 1.0, dashed }
        };
        let a: Vec<Point> = self.points.iter().step_by(2).map(|q| q.0).collect();
        let b: Vec<Point> = self.points.iter().skip(1).step_by(2).map(|q| q.0).collect();
        for side in [&a, &b] {
            if side.len() >= 2 {
                out.push(line(side, false));
            }
        }
        for (p, q) in a.iter().zip(&b) {
            out.push(Overlay::Line { a: *p, b: *q, color: ORANGE, dashed: true });
        }
        // where the next click goes: on from its side's last point, across to the other side
        if let (Some(h), false) = (self.hover, self.pressed) {
            let n = self.points.len();
            if let Some(same) = n.checked_sub(2).and_then(|k| self.points.get(k)) {
                out.push(Overlay::Line { a: same.0, b: h, color: ORANGE, dashed: true });
            }
            if let Some(other) = self.points.last() {
                out.push(Overlay::Line { a: other.0, b: h, color: ORANGE, dashed: true });
            }
        }
        for (q, smooth) in &self.points {
            out.push(Overlay::Anchor { p: *q, color: ORANGE, filled: *smooth, size: 5.0 });
        }
        out
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _mods: Mods) -> Cursor {
        Cursor::Crosshair
    }

    fn busy(&self) -> bool {
        self.pressed
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.reset();
        self.started = false;
        if std::mem::take(&mut self.pressed) { vec![Action::Cancel] } else { vec![] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::cx;
    use vectorcraft_doc::{Document, Selection};

    /// PC-02: a stroke sends its points to `embroidery.image.refineCutout`, as `keep`, or as
    /// `remove` with Alt held.
    #[test]
    fn the_cutout_brush_sends_its_stroke() {
        let d = Document::new(400.0, 300.0);
        let s = Selection::default();
        let p = crate::PaintDefaults::default();
        let c = cx(&d, &s, &p);
        let mut t = CutoutBrushTool::default();
        let ev = |kind, x: f64, alt| {
            let mut e = PointerEvent::new(kind, x, 10.0);
            e.mods.alt = alt;
            e
        };
        assert!(t.pointer(&c, &ev(PointerKind::Down, 0.0, false)).is_empty());
        assert!(t.busy());
        t.pointer(&c, &ev(PointerKind::Drag, 5.0, false));
        assert_eq!(t.overlays(&c).len(), 1, "the stroke shows");
        let up = t.pointer(&c, &ev(PointerKind::Up, 10.0, false));
        let [Action::Exec(cmd, v)] = up.as_slice() else { panic!("{up:?}") };
        assert_eq!(cmd, "embroidery.image.refineCutout");
        assert_eq!(v["keep"], json!([[0.0, 10.0], [5.0, 10.0], [10.0, 10.0]]));
        assert_eq!(v["radius"], 6.0);
        assert!(!t.busy());
        t.pointer(&c, &ev(PointerKind::Down, 0.0, true));
        let up = t.pointer(&c, &ev(PointerKind::Up, 3.0, true));
        assert!(matches!(up.as_slice(), [Action::Exec(_, v)] if v.get("remove").is_some()), "Alt takes away");
        t.set_option("mode", &json!("remove"));
        t.set_option("size", &json!(20));
        assert_eq!(t.options(), json!({"mode": "remove", "size": 20.0}));
    }

    #[test]
    fn brush_draws_with_a_preview_and_commits() {
        let d = Document::new(400.0, 300.0);
        let s = Selection::default();
        let p = crate::PaintDefaults::default();
        let c = cx(&d, &s, &p);
        let mut t = BrushTool::default();
        assert_eq!(t.pointer(&c, &PointerEvent::new(PointerKind::Down, 10.0, 10.0)), vec![Action::Begin("Brush Stroke".into())]);
        for i in 1..20 {
            t.pointer(&c, &PointerEvent::new(PointerKind::Drag, 10.0 + i as f64 * 5.0, 10.0 + i as f64));
        }
        let up = t.pointer(&c, &PointerEvent::new(PointerKind::Up, 110.0, 30.0));
        assert!(matches!(up.as_slice(), [Action::Preview(cmd, _), Action::Commit] if cmd == "embroidery.draw"));
        t.set_option("brush", &json!("fill"));
        t.set_option("width", &json!(1000));
        assert_eq!(t.options()["brush"], "fill");
        assert_eq!(t.options()["width"], 50.0);
    }

    #[test]
    fn stitch_tool_picks_moves_inserts_and_deletes() {
        let d = Document::new(400.0, 300.0);
        let id = NodeId(7);
        let s = Selection { objects: vec![id], ..Selection::default() };
        let p = crate::PaintDefaults::default();
        let st = [(
            id,
            vec![
                (Point::new(10.0, 10.0), sf_core::Cmd::Stitch),
                (Point::new(30.0, 10.0), sf_core::Cmd::Stitch),
                (Point::new(50.0, 10.0), sf_core::Cmd::Stitch),
            ],
        )];
        let c = ToolContext { embroidery_stitches: &st, ..cx(&d, &s, &p) };
        let mut t = StitchTool::default();
        let ev = |k, x, y| PointerEvent::new(k, x, y);
        // drag a needle point: one move of the picked stitch, previewed, then committed
        assert!(t.pointer(&c, &ev(PointerKind::Down, 11.0, 10.0)).is_empty());
        let drag = t.pointer(&c, &ev(PointerKind::Drag, 16.0, 12.0));
        assert_eq!(drag.first(), Some(&Action::Begin("Move Stitches".into())));
        let Some(Action::Preview(cmd, params)) = drag.get(1) else { panic!("{drag:?}") };
        assert_eq!(cmd, "embroidery.stitches.edit");
        assert_eq!(params["ops"][0], json!({"op": "move", "indices": [0], "by": [5.0, 2.0]}));
        assert_eq!(t.pointer(&c, &ev(PointerKind::Up, 16.0, 12.0)), vec![Action::Commit]);
        // the panel reads the picked stitch (to split the object there)
        assert_eq!(t.options(), json!({"id": 7, "picked": [0]}));
        // shift adds a second point; Delete removes both
        let shift = Mods { shift: true, ..Mods::default() };
        t.pointer(&c, &ev(PointerKind::Down, 30.0, 11.0).with_mods(shift));
        t.pointer(&c, &ev(PointerKind::Up, 30.0, 11.0).with_mods(shift));
        let del = t.key(&c, ToolKey::Delete, Mods::default());
        assert!(
            matches!(del.as_slice(), [Action::Exec(cmd, p)] if cmd == "embroidery.stitches.edit" && p["ops"][0]["indices"] == json!([0, 1])),
            "{del:?}"
        );
        // a marquee picks everything inside it
        t.pointer(&c, &ev(PointerKind::Down, 100.0, 100.0));
        t.pointer(&c, &ev(PointerKind::Drag, 20.0, 0.0));
        t.pointer(&c, &ev(PointerKind::Up, 20.0, 0.0));
        let del = t.key(&c, ToolKey::Delete, Mods::default());
        assert!(matches!(del.as_slice(), [Action::Exec(_, p)] if p["ops"][0]["indices"] == json!([1, 2])), "{del:?}");
        // alt-click on a stitch adds a needle point there
        let alt = Mods { alt: true, ..Mods::default() };
        let ins = t.pointer(&c, &ev(PointerKind::Down, 20.0, 12.0).with_mods(alt));
        assert!(matches!(ins.as_slice(), [Action::Exec(_, p)] if p["ops"][0] == json!({"op": "insert", "after": 0, "at": [20.0, 12.0]})), "{ins:?}");
        // the picked points are drawn filled
        assert!(t.overlays(&c).iter().any(|o| matches!(o, Overlay::Anchor { filled: true, .. })));
    }

    #[test]
    fn column_tool_clicks_the_sides_in_turn_and_continues_a_column() {
        use vectorcraft_geom::path::{Anchor, PathData, SubPath};
        let mut d = Document::new(400.0, 300.0);
        let p = crate::PaintDefaults::default();
        let ev = |k, x, y| PointerEvent::new(k, x, y);
        let points = |a: &[Action]| match a {
            [Action::Begin(_), Action::Preview(cmd, v), Action::Notify(_)] if cmd == "embroidery.column" => v.clone(),
            _ => panic!("{a:?}"),
        };
        {
            let s = Selection::default();
            let c = cx(&d, &s, &p);
            let mut t = ColumnTool::default();
            assert_eq!(points(&t.pointer(&c, &ev(PointerKind::Down, 10.0, 10.0))), json!({"points": [[10.0, 10.0, false]]}));
            assert_eq!(t.pointer(&c, &ev(PointerKind::Up, 10.0, 10.0)), vec![Action::Commit]);
            // the next click goes on the other side; Alt makes a smooth point
            let alt = Mods { alt: true, ..Mods::default() };
            let v = points(&t.pointer(&c, &ev(PointerKind::Down, 10.0, 40.0).with_mods(alt)));
            assert_eq!(v["points"], json!([[10.0, 10.0, false], [10.0, 40.0, true]]));
            t.pointer(&c, &ev(PointerKind::Up, 10.0, 40.0));
            assert!(t.overlays(&c).iter().any(|o| matches!(o, Overlay::Line { dashed: true, .. })));
            // Enter ends the column: the next click starts another
            assert!(t.key(&c, ToolKey::Enter, Mods::default()).is_empty());
            assert_eq!(points(&t.pointer(&c, &ev(PointerKind::Down, 200.0, 10.0))), json!({"points": [[200.0, 10.0, false]]}));
            // Escape with the button down drops that click
            assert_eq!(t.key(&c, ToolKey::Escape, Mods::default()), vec![Action::Cancel]);
        }
        // picked with a column selected, the tool continues it
        let layer = d.layers[0].id;
        let id = d.alloc_id();
        let side = |x: f64| SubPath::new(vec![Anchor::corner(Point::new(x, 10.0)), Anchor::corner(Point::new(x, 60.0))], false);
        let mut n = vectorcraft_doc::Node::path(id, PathData::new(vec![side(10.0), side(30.0)]), Default::default());
        n.embroidery = Some(Box::new(sf_core::attr::Embroidery::satin()));
        d.insert(Some(layer), 0, n).unwrap();
        let s = Selection { objects: vec![id], ..Selection::default() };
        let c = cx(&d, &s, &p);
        let mut t = ColumnTool::default();
        let v = points(&t.pointer(&c, &ev(PointerKind::Down, 10.0, 90.0)));
        assert_eq!(v["id"], id.0);
        assert_eq!(v["points"], json!([[10.0, 10.0, false], [30.0, 10.0, false], [10.0, 60.0, false], [30.0, 60.0, false], [10.0, 90.0, false]]));
        t.pointer(&c, &ev(PointerKind::Up, 10.0, 90.0));
        // Backspace takes the last point back (the document here still has the four)
        let back = t.key(&c, ToolKey::Backspace, Mods::default());
        assert!(
            matches!(back.as_slice(), [Action::Exec(cmd, v)] if cmd == "embroidery.column" && v["points"].as_array().map(Vec::len) == Some(3)),
            "{back:?}"
        );
    }

    #[test]
    fn direction_tool_without_selection_picks() {
        let d = Document::new(400.0, 300.0);
        let s = Selection::default();
        let p = crate::PaintDefaults::default();
        let c = cx(&d, &s, &p);
        let mut t = DirectionTool::default();
        // nothing under the pointer: nothing to pick
        assert!(t.pointer(&c, &PointerEvent::new(PointerKind::Down, 10.0, 10.0)).is_empty());
    }
}
