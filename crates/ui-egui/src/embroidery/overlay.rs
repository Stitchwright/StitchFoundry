//! The stitch overlay: every stitch of the plan drawn on the canvas as a GPU mesh,
//! either as flat lines or "TrueView" shaded thread, over a fabric-tinted artboard.

use std::sync::Arc;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Painter, Pos2, Shape, Stroke};
use vectorcraft_embroidery::sf_core::decor::{DecorKind, DecorShape};
use vectorcraft_embroidery::sf_core::density::LayerGrid;
use vectorcraft_embroidery::sf_core::geom::FLATTEN_TOLERANCE;
use vectorcraft_embroidery::sf_core::model::ObjectKind;
use vectorcraft_embroidery::sf_core::pattern::{Cmd, Pattern, Stitch};
use vectorcraft_embroidery::sf_core::threads::Rgb;
use vectorcraft_embroidery::{PT_PER_MM, StitchPlan, pt};

use super::{ArtView, StitchView};
use crate::canvas::Xf;

/// Thread thickness of 40 wt polyester lying on fabric, in mm.
const THREAD_WIDTH_MM: f64 = 0.36;

/// Most vertices the repeats of a design may add up to, so a big design repeated many times
/// keeps the canvas responsive (copies beyond it are left out).
const MAX_REPEAT_VERTICES: usize = 6_000_000;

/// What the base mesh is built from: the plan and how it is shown, not where the view is.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Key {
    plan: usize,
    mode: StitchView,
    upto: usize,
    points: bool,
    repeats: [u32; 2],
    gap: i64,
}

/// Zooming this far from the base mesh's zoom (either way) builds it again, so thread widths and
/// needle points, set in screen pixels, stay about the size they should be.
const REBUILD_ZOOM: f64 = 1.25;

/// The overlay mesh. Building it (every stitch a shaded quad) is the costly part, so it is built
/// once per plan and zoom range, for the whole design (`base`, laid out at `base_zoom` with no
/// rotation round the canvas centre `base_c`), and a pan, a rotation or a small zoom only moves
/// its vertices to where the view puts them (`mesh`).
#[derive(Default)]
pub struct MeshCache {
    key: Option<Key>,
    base: Mesh,
    base_jumps: Vec<[Pos2; 2]>,
    base_zoom: f64,
    base_c: Pos2,
    /// Centre, zoom, rotation and the canvas rect `mesh` was placed for: a window resize moves
    /// where the centre lands.
    view: Option<[i64; 8]>,
    mesh: Arc<Mesh>,
    jumps: Vec<[Pos2; 2]>,
    /// The base mesh's pieces as last built, kept to reuse.
    pieces: Vec<Piece>,
}

fn c32(c: Rgb, f: f32) -> Color32 {
    let s = |v: u8| ((v as f32) * f).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(s(c.r), s(c.g), s(c.b))
}

fn toward_white(c: Color32, t: f32) -> Color32 {
    let m = |v: u8| (v as f32 + (255.0 - v as f32) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

fn quad(mesh: &mut Mesh, a: Pos2, b: Pos2, half_w: f32, col: Color32) {
    let d = b - a;
    let len = d.length();
    if len < 0.01 {
        return;
    }
    let n = egui::vec2(-d.y, d.x) / len * half_w;
    let i = mesh.vertices.len() as u32;
    for p in [a + n, a - n, b - n, b + n] {
        mesh.vertices.push(Vertex { pos: p, uv: WHITE_UV, color: col });
    }
    mesh.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
}

/// One stitch as lit thread: four lanes across its width shaded as a cylinder with
/// a soft specular highlight, ends tucked into the fabric.
fn thread(mesh: &mut Mesh, a: Pos2, b: Pos2, half_w: f32, color: Rgb) {
    let d = b - a;
    let len = d.length();
    if len < 0.01 {
        return;
    }
    let t = d / len;
    let nrm = egui::vec2(-t.y, t.x);
    let inset = (half_w * 0.35).min(len * 0.2);
    let (a2, b2) = (a + t * inset, b - t * inset);
    // light from the top left, a little towards the viewer
    const L: [f32; 3] = [-0.45, -0.55, 0.70];
    const LANES: [f32; 4] = [-1.0, -0.4, 0.4, 1.0];
    let base = mesh.vertices.len() as u32;
    for &u in &LANES {
        let z = (1.0 - u * u).max(0.0).sqrt();
        let ndl = nrm.x * u * L[0] + nrm.y * u * L[1] + z * L[2];
        let diffuse = ndl.max(0.0);
        let spec = (2.0 * ndl * z - L[2]).max(0.0).powf(12.0);
        let mut col = c32(color, 0.38 + 0.75 * diffuse);
        if spec > 0.01 {
            col = toward_white(col, (spec * 0.55).min(0.6));
        }
        let off = nrm * (u * half_w);
        mesh.vertices.push(Vertex { pos: a2 + off, uv: WHITE_UV, color: col });
        mesh.vertices.push(Vertex { pos: b2 + off, uv: WHITE_UV, color: col });
    }
    for k in 0..3u32 {
        let i = base + k * 2;
        mesh.indices.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
    }
}

/// Where the drawing stands between records: the last needle position, whether the thread is
/// sewing (not after a jump or trim) and which thread is in.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pen {
    prev: Option<Pos2>,
    sewing: bool,
    thread: usize,
}

/// Below this half width (screen px) TrueView draws a stitch as one flat strip in the thread's
/// lit colour: its shading lanes would be thinner than a pixel and can't be seen.
const SHADE_MIN_HALF_W: f32 = 1.0;

/// The mesh of `records` (a stretch of `pattern`'s records, starting with the pen at `pen`), and
/// its jumps; returns where the pen is after them.
fn build_range(
    pattern: &Pattern,
    records: &[Stitch],
    xf: &Xf,
    mode: StitchView,
    points: bool,
    visible: egui::Rect,
    mut pen: Pen,
) -> (Mesh, Vec<[Pos2; 2]>, Pen) {
    let mut mesh = Mesh::default();
    let mut jumps = Vec::new();
    let true_view = mode == StitchView::TrueView;
    let half_w = if true_view { ((THREAD_WIDTH_MM * PT_PER_MM * xf.zoom) as f32 * 0.5).max(0.45) } else { 0.6 };
    let shaded = true_view && half_w >= SHADE_MIN_HALF_W;
    let mut color = pattern.thread(pen.thread).color;
    for s in records {
        let p = xf.to_screen(pt(s.pos()));
        match s.cmd {
            Cmd::Stitch => {
                if let Some(a) = pen.prev
                    && pen.sewing
                    && egui::Rect::from_two_pos(a, p).intersects(visible)
                {
                    if shaded {
                        thread(&mut mesh, a, p, half_w, color);
                    } else if true_view {
                        quad(&mut mesh, a, p, half_w, c32(color, 0.9));
                    } else {
                        quad(&mut mesh, a, p, half_w, c32(color, 1.0));
                    }
                }
                if points && visible.contains(p) {
                    quad(&mut mesh, p - egui::vec2(1.2, 0.0), p + egui::vec2(1.2, 0.0), 1.2, Color32::from_black_alpha(190));
                }
                pen.prev = Some(p);
                pen.sewing = true;
            }
            Cmd::Jump => {
                if let Some(a) = pen.prev {
                    jumps.push([a, p]);
                }
                pen.prev = Some(p);
                pen.sewing = false;
            }
            Cmd::Trim => pen.sewing = false,
            Cmd::ColorChange => {
                pen.thread += 1;
                color = pattern.thread(pen.thread).color;
                pen.sewing = false;
            }
            Cmd::Stop | Cmd::End => {}
        }
    }
    (mesh, jumps, pen)
}

/// The whole mesh in one go (the tests check the pieces against it).
#[cfg(test)]
fn build(pattern: &Pattern, xf: &Xf, mode: StitchView, upto: usize, points: bool, visible: egui::Rect) -> (Mesh, Vec<[Pos2; 2]>) {
    let n = upto.min(pattern.stitches.len());
    let records = pattern.stitches.get(..n).unwrap_or_default();
    let (mesh, jumps, _) = build_range(pattern, records, xf, mode, points, visible, Pen { prev: None, sewing: false, thread: 0 });
    (mesh, jumps)
}

/// Most records one cached piece of the mesh holds (a big object is cut into several).
const PIECE_RECORDS: usize = 8_000;

/// One piece of the base mesh: a stretch of records, the hash it was built for, its mesh and
/// jumps and where the pen ends.
struct Piece {
    hash: u64,
    mesh: Mesh,
    jumps: Vec<[Pos2; 2]>,
    end: Pen,
}

/// A hash of what a piece's mesh depends on: its records, how they are drawn and where the pen
/// starts. A fast multiply-rotate mix: it only tells pieces apart, nothing depends on it being
/// hard to forge.
fn piece_hash(records: &[Stitch], pattern: &Pattern, how: (StitchView, bool, &Xf), pen: Pen) -> u64 {
    struct Mix(u64);
    impl Mix {
        fn add(&mut self, v: u64) {
            self.0 = (self.0.rotate_left(5) ^ v).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    let mut h = Mix(0x9e37_79b9_7f4a_7c15);
    let (mode, points, xf) = how;
    h.add(mode as u64);
    h.add(u64::from(points));
    h.add(xf.zoom.to_bits());
    let c = xf.rect.center();
    h.add(u64::from(c.x.to_bits()) << 32 | u64::from(c.y.to_bits()));
    match pen.prev {
        Some(p) => h.add(u64::from(p.x.to_bits()) << 32 | u64::from(p.y.to_bits())),
        None => h.add(u64::MAX),
    }
    h.add(u64::from(pen.sewing));
    h.add(pen.thread as u64);
    // the threads the piece may be drawn in
    let changes = records.iter().filter(|r| r.cmd == Cmd::ColorChange).count();
    for k in pen.thread..=pen.thread + changes {
        let c = pattern.thread(k).color;
        h.add(u64::from(c.r) << 16 | u64::from(c.g) << 8 | u64::from(c.b));
    }
    h.add(records.len() as u64);
    for r in records {
        h.add(r.x.to_bits());
        h.add(r.y.to_bits() ^ r.cmd as u64);
    }
    h.0
}

/// Where the base mesh is cut into pieces: at every object's first and last record, and every
/// [`PIECE_RECORDS`] within, up to `n`.
fn piece_bounds(plan: &StitchPlan, n: usize) -> Vec<usize> {
    let mut cuts: Vec<usize> = vec![0, n];
    for span in &plan.built.spans {
        cuts.push(span.range.start.min(n));
        cuts.push(span.range.end.min(n));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = Vec::with_capacity(cuts.len());
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let mut at = a;
        while at < b {
            out.push(at);
            at = (at + PIECE_RECORDS).min(b);
        }
    }
    out.push(n);
    out
}

/// Moves the base mesh's vertices to where `place` puts them, into `out` (as long as `base`), on
/// several threads for a big design on the desktop.
fn place_all(base: &[Vertex], out: &mut [Vertex], place: &Placement) {
    let one = |src: &[Vertex], dst: &mut [Vertex]| {
        for (d, v) in dst.iter_mut().zip(src) {
            *d = Vertex { pos: place.at(v.pos), ..*v };
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    {
        const PER_THREAD: usize = 100_000;
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8);
        if base.len() > PER_THREAD * 2 && threads > 1 {
            let chunk = base.len().div_ceil(threads).max(1);
            std::thread::scope(|sc| {
                for (src, dst) in base.chunks(chunk).zip(out.chunks_mut(chunk)) {
                    sc.spawn(move || one(src, dst));
                }
            });
            return;
        }
    }
    one(base, out);
}

impl MeshCache {
    fn update(&mut self, plan: &Arc<StitchPlan>, xf: &Xf, (mode, upto, points): (StitchView, usize, bool), (repeats, gap): ([u32; 2], f64)) {
        let q = |v: f64| (v * 1000.0).round() as i64;
        let key = Key { plan: Arc::as_ptr(plan) as usize, mode, upto, points, repeats, gap: q(gap) };
        let view = [
            q(xf.center.x),
            q(xf.center.y),
            q(xf.zoom),
            q(xf.rot),
            q(xf.rect.min.x as f64),
            q(xf.rect.min.y as f64),
            q(xf.rect.max.x as f64),
            q(xf.rect.max.y as f64),
        ];
        let ratio = if self.base_zoom > 0.0 { xf.zoom / self.base_zoom } else { 0.0 };
        let rebuild = self.key != Some(key) || !(1.0 / REBUILD_ZOOM..=REBUILD_ZOOM).contains(&ratio);
        if !rebuild && self.view == Some(view) {
            return;
        }
        if rebuild {
            // the whole design, at this zoom, unrotated, centred on the document's origin
            let bx = Xf { rect: xf.rect, zoom: xf.zoom, center: vectorcraft_geom::Point::new(0.0, 0.0), rot: 0.0 };
            let pattern = plan.pattern();
            let step = repeat_step(pattern, &bx, gap);
            // piece by piece, reusing the pieces whose records and pen are as they were (an edit
            // to one object, or the player sewing on, builds little again)
            let mut old: std::collections::HashMap<u64, Piece> = std::mem::take(&mut self.pieces).into_iter().map(|p| (p.hash, p)).collect();
            let n = upto.min(pattern.stitches.len());
            let bounds = piece_bounds(plan, n);
            let mut pen = Pen { prev: None, sewing: false, thread: 0 };
            let mut mesh = Mesh::default();
            let mut jumps = Vec::new();
            for w in bounds.windows(2) {
                let records = pattern.stitches.get(w[0]..w[1]).unwrap_or_default();
                let hash = piece_hash(records, pattern, (mode, points, &bx), pen);
                let piece = old.remove(&hash).unwrap_or_else(|| {
                    let (mesh, jumps, end) = build_range(pattern, records, &bx, mode, points, egui::Rect::EVERYTHING, pen);
                    Piece { hash, mesh, jumps, end }
                });
                mesh.append_ref(&piece.mesh);
                jumps.extend_from_slice(&piece.jumps);
                pen = piece.end;
                self.pieces.push(piece);
            }
            self.base = tiled(mesh, repeats, step);
            self.base_jumps = jumps;
            self.base_zoom = xf.zoom;
            self.base_c = xf.rect.center();
            self.key = Some(key);
            // the placed mesh takes the base's triangles; its vertices are placed below
            let placed = Arc::make_mut(&mut self.mesh);
            placed.indices.clone_from(&self.base.indices);
            placed.texture_id = self.base.texture_id;
        }
        let place = Placement::new(self.base_c, self.base_zoom, xf);
        let placed = Arc::make_mut(&mut self.mesh);
        placed.vertices.resize(self.base.vertices.len(), Vertex::default());
        place_all(&self.base.vertices, &mut placed.vertices, &place);
        self.jumps = self.base_jumps.iter().map(|[a, b]| [place.at(*a), place.at(*b)]).collect();
        self.view = Some(view);
    }
}

/// Moves a point of the base mesh (laid out at zoom `z0`, unrotated, the document's origin at
/// `c0`) to where the view `xf` draws it: screen = c + R(rot) · ((v − c0) · z/z0 − centre · z).
struct Placement {
    c0: Pos2,
    k: f32,
    off: egui::Vec2,
    sin: f32,
    cos: f32,
    c: Pos2,
}

impl Placement {
    fn new(c0: Pos2, z0: f64, xf: &Xf) -> Self {
        let k = if z0 > 0.0 { (xf.zoom / z0) as f32 } else { 1.0 };
        let (sin, cos) = xf.rot.sin_cos();
        let off = egui::vec2(-(xf.center.x * xf.zoom) as f32, -(xf.center.y * xf.zoom) as f32);
        Placement { c0, k, off, sin: sin as f32, cos: cos as f32, c: xf.rect.center() }
    }

    fn at(&self, v: Pos2) -> Pos2 {
        let u = (v - self.c0) * self.k + self.off;
        self.c + egui::vec2(u.x * self.cos - u.y * self.sin, u.x * self.sin + u.y * self.cos)
    }
}

/// How far apart repeats of the design are drawn on screen, across and down: its stitches'
/// width and height plus `gap` (mm).
fn repeat_step(pattern: &Pattern, xf: &Xf, gap: f64) -> [egui::Vec2; 2] {
    let b = pattern.bounds();
    if b.is_empty() {
        return [egui::Vec2::ZERO; 2];
    }
    let gap = if gap.is_finite() { gap.clamp(0.0, 500.0) } else { 0.0 };
    let at = |x: f64, y: f64| xf.to_screen(pt(vectorcraft_embroidery::sf_core::Point::new(x, y)));
    let o = at(b.min.x, b.min.y);
    [at(b.max.x + gap, b.min.y) - o, at(b.min.x, b.max.y + gap) - o]
}

/// The design's mesh repeated `cols × rows` times, `step` apart across and down: the first
/// where the design is, the others beside it and below.
fn tiled(mesh: Mesh, [cols, rows]: [u32; 2], step: [egui::Vec2; 2]) -> Mesh {
    let (cols, rows) = (cols.clamp(1, 10), rows.clamp(1, 10));
    if cols * rows == 1 || mesh.vertices.is_empty() {
        return mesh;
    }
    let room = (MAX_REPEAT_VERTICES / mesh.vertices.len()).max(1);
    let mut out = mesh.clone();
    let offsets = (0..rows).flat_map(|j| (0..cols).map(move |i| (i, j))).skip(1).take(room.saturating_sub(1));
    for (i, j) in offsets {
        let mut copy = mesh.clone();
        copy.translate(step[0] * i as f32 + step[1] * j as f32);
        out.append(copy);
    }
    out
}

/// Paints the stitch plan over the art.
pub fn paint(app: &mut crate::VectorcraftApp, painter: &Painter, xf: &Xf) {
    let ui = &app.embroidery;
    if ui.stitches == StitchView::Off {
        return;
    }
    let Some(plan) = app.session.embroidery_plan() else { return };
    if plan.is_empty() {
        return;
    }
    let Some(st) = app.session.active() else { return };
    // Fabric under the stitches: the artboards, tinted.
    let fabric = ui.fabric;
    let cover = match ui.art {
        ArtView::Normal => None,
        ArtView::Dimmed => Some(Color32::from_rgba_unmultiplied(fabric.r(), fabric.g(), fabric.b(), 190)),
        ArtView::Hidden => Some(fabric),
    };
    if let Some(c) = cover {
        for ab in &st.doc.artboards {
            painter.add(Shape::convex_polygon(xf.quad(ab.rect), c, Stroke::NONE));
        }
    }
    let total = plan.pattern().stitches.len();
    let upto = if ui.player.active { (ui.player.pos as usize).min(total) } else { total };
    let mode = ui.stitches;
    let points = ui.needle_points;
    let connectors = ui.connectors;
    let (beads, guides) = (ui.beads, ui.guides);
    let selected: Vec<_> = st.selection.objects.clone();
    // The hoop's sewing field, centred on the design as the machine centres it (or where the
    // design is placed in it).
    if let Some(e) = st.doc.embroidery.as_deref()
        && let Some(hoop) = e.hoop.as_ref()
    {
        hoop_outline(painter, xf, hoop, plan.pattern(), e.settings.hoop_offset.unwrap_or([0.0, 0.0]));
    }
    let repeats = (app.embroidery.repeats, app.embroidery.repeat_gap);
    let cache = &mut app.embroidery.mesh;
    cache.update(&plan, xf, (mode, upto, points), repeats);
    // appliqué fabric, sequins, beads, cord and boring holes under their stitches; stones on top
    fabrics(&plan, painter, xf);
    decorations(&plan, painter, xf, false);
    painter.add(Shape::mesh(cache.mesh.clone()));
    decorations(&plan, painter, xf, true);
    if app.embroidery.density_map {
        let limit = st.doc.embroidery.as_deref().map_or(4.5, |e| e.checks.dense_layers);
        density_map(&mut app.embroidery.density, &plan, painter, xf, limit);
    }
    if connectors {
        for [a, b] in &cache.jumps {
            painter.add(Shape::dashed_line(&[*a, *b], Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 60, 200, 200)), 4.0, 3.0));
        }
    }
    // Inclination lines of the selected stitched objects.
    if guides {
        let orange = Color32::from_rgb(255, 120, 0);
        for id in &selected {
            let Some(e) = st.doc.node(*id).and_then(|n| n.embroidery.as_deref()) else { continue };
            for g in &e.guides {
                let a = xf.to_screen(vectorcraft_geom::Point::new(g.a.x, g.a.y));
                let b = xf.to_screen(vectorcraft_geom::Point::new(g.b.x, g.b.y));
                painter.line_segment([a, b], Stroke::new(1.5, orange));
                for p in [a, b] {
                    painter.circle_filled(p, 3.0, orange);
                }
            }
        }
    }
    // Split lines of the selected fills, dashed: each stitch row has a needle point where it
    // crosses one.
    let teal = Color32::from_rgb(0, 150, 170);
    for id in &selected {
        let Some(e) = st.doc.node(*id).and_then(|n| n.embroidery.as_deref()) else { continue };
        for line in &e.splits {
            let pts: Vec<Pos2> = line.iter().map(|q| xf.to_screen(vectorcraft_geom::Point::new(q.x, q.y))).collect();
            painter.add(Shape::dashed_line(&pts, Stroke::new(1.2, teal), 5.0, 3.0));
        }
    }
    // Flow lines of the selected fills, dashed: the rows follow them.
    let violet = Color32::from_rgb(150, 80, 200);
    for id in &selected {
        let Some(e) = st.doc.node(*id).and_then(|n| n.embroidery.as_deref()) else { continue };
        for line in &e.flow {
            let pts: Vec<Pos2> = line.iter().map(|q| xf.to_screen(vectorcraft_geom::Point::new(q.x, q.y))).collect();
            painter.add(Shape::dashed_line(&pts, Stroke::new(1.5, violet), 8.0, 4.0));
        }
    }
    // Start (green) and stop (red) beads of the selected stitched objects.
    for id in selected.iter().copied().filter(|_| beads) {
        let ranges = plan.records_of(id);
        let pat = plan.pattern();
        let first = ranges.first().and_then(|r| pat.stitches.get(r.start..r.end)).and_then(|s| s.iter().find(|s| s.cmd == Cmd::Stitch));
        let last = ranges.last().and_then(|r| pat.stitches.get(r.start..r.end)).and_then(|s| s.iter().rev().find(|s| s.cmd == Cmd::Stitch));
        for (s, col) in [(first, Color32::from_rgb(40, 190, 90)), (last, Color32::from_rgb(220, 50, 50))] {
            if let Some(s) = s {
                let p = xf.to_screen(pt(s.pos()));
                painter.circle_filled(p, 5.0, col);
                painter.circle_stroke(p, 5.0, Stroke::new(1.5, Color32::WHITE));
            }
        }
    }
    // The stitch player's needle.
    if app.embroidery.player.active
        && let Some(s) = plan.pattern().stitches.get(upto.saturating_sub(1))
    {
        let p = xf.to_screen(pt(s.pos()));
        painter.circle_stroke(p, 7.0, Stroke::new(2.0, Color32::from_rgb(255, 170, 40)));
        painter.line_segment([p - egui::vec2(0.0, 14.0), p + egui::vec2(0.0, 14.0)], Stroke::new(1.0, Color32::from_rgb(255, 170, 40)));
    }
}

/// The colour of a cell `layers` deep, against the depth Check Design calls dense: nothing
/// under half a layer, then blue (one layer), green, yellow and red (as deep as `limit`).
fn density_color(layers: f32, limit: f64) -> Option<Color32> {
    if layers < 0.5 || limit <= 0.0 {
        return None;
    }
    let t = (f64::from(layers) / limit).clamp(0.0, 1.0) as f32;
    let stops = [
        (0.0, Color32::from_rgb(40, 110, 230)),
        (0.4, Color32::from_rgb(40, 190, 90)),
        (0.7, Color32::from_rgb(240, 200, 30)),
        (1.0, Color32::from_rgb(230, 40, 40)),
    ];
    let c = stops.windows(2).find(|w| t <= w[1].0).map_or(Color32::from_rgb(230, 40, 40), |w| lerp(w[0].1, w[1].1, (t - w[0].0) / (w[1].0 - w[0].0)));
    Some(Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 120))
}

/// The density map over the stitches: each cell coloured by how deep the thread lies there,
/// measured once per plan.
fn density_map(cache: &mut Option<(usize, Arc<LayerGrid>)>, plan: &Arc<StitchPlan>, painter: &Painter, xf: &Xf, limit: f64) {
    let key = Arc::as_ptr(plan) as usize;
    let grid = match cache {
        Some((k, g)) if *k == key => g.clone(),
        _ => {
            let g = Arc::new(LayerGrid::of(plan.pattern(), 1.0));
            *cache = Some((key, g.clone()));
            g
        }
    };
    let mut mesh = Mesh::default();
    let screen = |x: f64, y: f64| xf.to_screen(vectorcraft_geom::Point::new(x * PT_PER_MM, y * PT_PER_MM));
    for row in 0..grid.rows {
        for col in 0..grid.columns {
            let Some(c) = density_color(grid.get(col, row), limit) else { continue };
            let (x, y) = (grid.origin.x + col as f64 * grid.cell, grid.origin.y + row as f64 * grid.cell);
            let corners = [screen(x, y), screen(x + grid.cell, y), screen(x + grid.cell, y + grid.cell), screen(x, y + grid.cell)];
            let base = mesh.vertices.len() as u32;
            mesh.vertices.extend(corners.map(|pos| Vertex { pos, uv: WHITE_UV, color: c }));
            mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Each appliqué's fabric under its stitches, in its colour and pattern.
fn fabrics(plan: &StitchPlan, painter: &Painter, xf: &Xf) {
    use vectorcraft_embroidery::sf_core::model::FabricPattern;
    for obj in &plan.design.objects {
        let ObjectKind::Applique(a) = &obj.kind else { continue };
        let p = &a.params;
        let rings: Vec<Vec<_>> = a.pieces().flat_map(|s| s.polygon(FLATTEN_TOLERANCE).rings().cloned().collect::<Vec<_>>()).collect();
        let (c1, c2) = (c32(p.fabric_color, 1.0), c32(p.fabric_color_2, 1.0));
        let cell = (p.fabric_pattern != FabricPattern::Plain).then_some(2.0);
        let pattern = p.fabric_pattern;
        fill_rings(painter, xf, &rings, cell, &|u, v| lerp(c1, c2, pattern.mix(u, v) as f32));
    }
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Most cells a filled area is cut into, so a big patterned piece stays quick to draw.
const MAX_FILL_CELLS: f32 = 120_000.0;

/// Fills the even-odd area of `rings` (mm; concave, with holes) by screen scanlines; with
/// `cell` (px), each line is cut into cells coloured by `color(u, v)` at their middles (mm).
fn fill_rings(
    painter: &Painter,
    xf: &Xf,
    rings: &[Vec<vectorcraft_embroidery::sf_core::geom::Point>],
    cell: Option<f32>,
    color: &dyn Fn(f64, f64) -> Color32,
) {
    let scr: Vec<Vec<Pos2>> = rings.iter().filter(|r| r.len() >= 3).map(|r| r.iter().map(|q| xf.to_screen(pt(*q))).collect()).collect();
    let mut bb = egui::Rect::NOTHING;
    for p in scr.iter().flatten() {
        bb.extend_with(*p);
    }
    let area = bb.intersect(painter.clip_rect());
    if !(area.is_positive() && area.width().is_finite() && area.height().is_finite()) {
        return;
    }
    // rows and cells about `cell` px, fewer when the area is large
    let base = cell.unwrap_or(1.5).max(1.0);
    let step = base.max((area.width() * area.height() / MAX_FILL_CELLS).sqrt());
    let rows = (area.height() / step).ceil().min(4000.0) as usize;
    let mm = |p: Pos2| {
        let d = xf.to_doc(p);
        (d.x / PT_PER_MM, d.y / PT_PER_MM)
    };
    let mut mesh = Mesh::default();
    let mut xs: Vec<f32> = Vec::new();
    for k in 0..rows {
        let y0 = area.top() + k as f32 * step;
        let y1 = (y0 + step).min(area.bottom());
        let y = (y0 + y1) / 2.0;
        xs.clear();
        for r in &scr {
            for (a, b) in r.iter().zip(r.iter().cycle().skip(1)) {
                if (a.y <= y) != (b.y <= y) && (b.y - a.y).abs() > f32::EPSILON {
                    xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        xs.sort_by(f32::total_cmp);
        for &[xa, xb] in xs.as_chunks::<2>().0 {
            let (xa, xb) = (xa.max(area.left()), xb.min(area.right()));
            if xb <= xa {
                continue;
            }
            match cell {
                None => {
                    let (u, v) = mm(Pos2::new((xa + xb) / 2.0, y));
                    mesh.add_colored_rect(egui::Rect::from_min_max(Pos2::new(xa, y0), Pos2::new(xb, y1)), color(u, v));
                }
                Some(_) => {
                    let mut x = xa;
                    while x < xb {
                        let x1 = (x + step).min(xb);
                        let (u, v) = mm(Pos2::new((x + x1) / 2.0, y));
                        mesh.add_colored_rect(egui::Rect::from_min_max(Pos2::new(x, y0), Pos2::new(x1, y1)), color(u, v));
                        x = x1;
                    }
                }
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// A piece's outline (screen) of `shape`, `r` across, turned `angle`, round `c`.
fn decor_outline(shape: DecorShape, c: Pos2, r: f32, angle: f32) -> Vec<Pos2> {
    let n = 40;
    (0..n)
        .map(|k| {
            let t = std::f32::consts::TAU * k as f32 / n as f32;
            let (x, y) = match shape {
                DecorShape::Round => (t.cos(), t.sin()),
                DecorShape::Square => {
                    let (cx, cy) = (t.cos(), t.sin());
                    let m = cx.abs().max(cy.abs()).max(1e-6);
                    (cx / m * 0.85, cy / m * 0.85)
                }
                DecorShape::Star => {
                    let k5 = (t * 5.0 / std::f32::consts::TAU).fract();
                    let rr = 0.45 + 0.55 * (1.0 - 2.0 * (k5 - 0.5).abs());
                    (rr * (t - std::f32::consts::FRAC_PI_2).cos(), rr * (t - std::f32::consts::FRAC_PI_2).sin())
                }
                DecorShape::Heart => {
                    let x = 16.0 * t.sin().powi(3) / 17.0;
                    let y = -(13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos()) / 17.0;
                    (x, y)
                }
                DecorShape::Flower => {
                    let rr = 0.78 + 0.22 * (6.0 * t).cos();
                    (rr * t.cos(), rr * t.sin())
                }
            };
            let (sa, ca) = angle.sin_cos();
            c + egui::vec2((x * ca - y * sa) * r, (x * sa + y * ca) * r)
        })
        .collect()
}

/// Sequins, beads, stones, cord and boring holes as they will look: stones (`top`) over the
/// stitches, the rest under them.
fn decorations(plan: &StitchPlan, painter: &Painter, xf: &Xf, top: bool) {
    let clip = painter.clip_rect().expand(40.0);
    let scale = (xf.zoom * PT_PER_MM) as f32;
    for obj in &plan.design.objects {
        let ObjectKind::Decor(d) = &obj.kind else { continue };
        let p = &d.params;
        if (p.kind == DecorKind::Stone) != top {
            continue;
        }
        let thread = plan.design.palette.get(obj.color).map(|t| t.color).unwrap_or(Rgb::new(0x80, 0x80, 0x80));
        let base = c32(p.color.unwrap_or(thread), 1.0);
        let dark = c32(p.color.unwrap_or(thread), 0.55);
        let light = toward_white(base, 0.6);
        let at = |q: vectorcraft_embroidery::sf_core::geom::Point| xf.to_screen(pt(q));
        match p.kind {
            DecorKind::Cord => {
                for (line, closed) in &d.lines {
                    let mut pts: Vec<Pos2> = line.iter().map(|q| at(*q)).collect();
                    if *closed && let Some(f) = pts.first().copied() {
                        pts.push(f);
                    }
                    let w = (p.size as f32 * scale).max(1.5);
                    painter.add(Shape::line(pts.clone(), Stroke::new(w, dark)));
                    painter.add(Shape::line(pts, Stroke::new(w * 0.45, base)));
                }
            }
            DecorKind::Print => {
                let rings: Vec<Vec<_>> = d.lines.iter().map(|(l, _)| l.clone()).collect();
                fill_rings(painter, xf, &rings, None, &|_, _| base);
            }
            DecorKind::Foil => {
                // a metallic sheen: light bands running diagonally across the foil
                let rings: Vec<Vec<_>> = d.lines.iter().map(|(l, _)| l.clone()).collect();
                let shine = |u: f64, v: f64| {
                    let t = ((u + v) * 0.35).sin() * 0.5 + ((u - 0.6 * v) * 0.9).sin() * 0.15;
                    if t >= 0.0 { toward_white(base, (t as f32 * 0.8).min(0.7)) } else { lerp(base, dark, (-t as f32 * 1.2).min(0.8)) }
                };
                fill_rings(painter, xf, &rings, Some(2.0), &shine);
            }
            DecorKind::Boring => {
                let rings: Vec<Vec<_>> = d.lines.iter().map(|(l, _)| l.clone()).collect();
                fill_rings(painter, xf, &rings, None, &|_, _| Color32::from_gray(38));
                for line in &rings {
                    let mut pts: Vec<Pos2> = line.iter().map(|q| at(*q)).collect();
                    if let Some(f) = pts.first().copied() {
                        pts.push(f);
                    }
                    painter.add(Shape::line(pts, Stroke::new(1.0, Color32::from_gray(20))));
                }
            }
            DecorKind::Sequin | DecorKind::Bead | DecorKind::Stone => {
                for it in &d.items {
                    let c = at(it.at);
                    let r = (it.size as f32 * scale / 2.0).max(1.0);
                    if !clip.contains(c) {
                        continue;
                    }
                    let angle = it.angle as f32 + xf.rot as f32;
                    match p.kind {
                        DecorKind::Bead => {
                            // a seed bead seen from above: a rounded barrel, its hole along the run
                            let dir = egui::vec2(angle.cos(), angle.sin());
                            let half = dir * r * 0.35;
                            let w = r * 1.5;
                            painter.line_segment([c - half, c + half], Stroke::new(w, dark));
                            painter.circle_filled(c - half, w / 2.0, dark);
                            painter.circle_filled(c + half, w / 2.0, dark);
                            painter.line_segment([c - half, c + half], Stroke::new(w * 0.75, base));
                            painter.circle_filled(c - egui::vec2(r * 0.3, r * 0.35), (r * 0.22).max(0.7), light);
                        }
                        DecorKind::Stone => {
                            let shape = if p.shape == DecorShape::Square { DecorShape::Square } else { DecorShape::Round };
                            let face = decor_outline(shape, c, r, angle);
                            painter.add(Shape::convex_polygon(face, toward_white(base, 0.25), Stroke::new(1.0, dark)));
                            // facets and a glint
                            for k in 0..8 {
                                let t = std::f32::consts::TAU * k as f32 / 8.0;
                                painter.line_segment(
                                    [c + egui::vec2(t.cos(), t.sin()) * r * 0.45, c + egui::vec2(t.cos(), t.sin()) * r * 0.95],
                                    Stroke::new(0.6, dark),
                                );
                            }
                            painter.circle_filled(c - egui::vec2(r * 0.3, r * 0.3), (r * 0.18).max(0.8), Color32::WHITE);
                        }
                        _ => {
                            let (face, rim) = if it.flip { (dark, base) } else { (base, dark) };
                            let outline = decor_outline(p.shape, c, r, angle);
                            if matches!(p.shape, DecorShape::Round | DecorShape::Square) {
                                painter.add(Shape::convex_polygon(outline, face, Stroke::new(1.0, rim)));
                            } else {
                                painter.add(Shape::closed_line(outline.clone(), Stroke::new(1.0, rim)));
                                painter.add(Shape::convex_polygon(decor_outline(DecorShape::Round, c, r * 0.6, angle), face, Stroke::NONE));
                                painter.add(Shape::closed_line(outline, Stroke::new((r * 0.25).max(1.0), face)));
                            }
                            // a sheen toward the light, and the hole
                            painter.circle_filled(c - egui::vec2(r * 0.35, r * 0.35), r * 0.28, light.gamma_multiply(0.8));
                            painter.circle_filled(c, (r * 0.14).max(0.7), Color32::from_gray(30));
                        }
                    }
                }
            }
        }
    }
}

/// The hoop's sewing field (dashed, red where the design doesn't fit) round the
/// design's centre, with centre marks and the hoop's name.
fn hoop_outline(painter: &Painter, xf: &Xf, hoop: &vectorcraft_embroidery::sf_core::hoops::Hoop, pattern: &Pattern, [ox, oy]: [f64; 2]) {
    let b = pattern.bounds();
    if b.is_empty() {
        return;
    }
    // the hoop's centre: the design's middle, less where the design sits in the hoop
    let c = b.center() - vectorcraft_embroidery::sf_core::Point::new(ox, oy);
    let fits = hoop.fits(&vectorcraft_embroidery::sf_core::geom::BBox::new(b.min - c, b.max - c));
    let color = if fits { Color32::from_rgb(70, 110, 170) } else { Color32::from_rgb(210, 50, 50) };
    let mut ring: Vec<Pos2> = hoop.outline().into_iter().map(|q| xf.to_screen(pt(q + c))).collect();
    if let Some(&first) = ring.first() {
        ring.push(first);
    }
    painter.add(Shape::dashed_line(&ring, Stroke::new(1.5, color), 8.0, 5.0));
    let centre = xf.to_screen(pt(c));
    for d in [egui::vec2(10.0, 0.0), egui::vec2(0.0, 10.0)] {
        painter.line_segment([centre - d, centre + d], Stroke::new(1.0, color));
    }
    let top_left = xf.to_screen(pt(c + vectorcraft_embroidery::sf_core::Point::new(-hoop.width / 2.0, -hoop.height / 2.0)));
    painter.text(top_left + egui::vec2(4.0, -4.0), egui::Align2::LEFT_BOTTOM, &hoop.name, egui::FontId::proportional(11.0), color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_tile_the_mesh_across_and_down() {
        let mut mesh = Mesh::default();
        quad(&mut mesh, Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0), 1.0, Color32::RED);
        let n = mesh.vertices.len();
        let step = [egui::vec2(20.0, 0.0), egui::vec2(0.0, 15.0)];
        // once: the mesh as it is
        assert_eq!(tiled(mesh.clone(), [1, 1], step).vertices.len(), n);
        // three across, two down: six copies, the last one along and down
        let t = tiled(mesh.clone(), [3, 2], step);
        assert_eq!(t.vertices.len(), n * 6);
        let last = &t.vertices[n * 5..];
        assert!(last.iter().zip(&mesh.vertices).all(|(a, b)| a.pos == b.pos + egui::vec2(40.0, 15.0)));
        // more than ten each way is ten
        assert_eq!(tiled(mesh, [50, 1], step).vertices.len(), n * 10);
    }

    #[test]
    fn panning_rotating_and_zooming_move_the_mesh_without_building_it_again() {
        let mut s = vectorcraft_engine::Session::new();
        s.execute("file.new", &serde_json::json!({"width": 200, "height": 200})).unwrap();
        s.execute("shape.rectangle", &serde_json::json!({"x": 20, "y": 20, "width": 60, "height": 40})).unwrap();
        s.execute("embroidery.set", &serde_json::json!({"type": "fill"})).unwrap();
        let plan = s.embroidery_plan().unwrap();
        let rect = egui::Rect::from_min_size(Pos2::new(10.0, 20.0), egui::vec2(800.0, 600.0));
        let view = |cx: f64, zoom: f64, rot: f64| Xf { rect, zoom, center: vectorcraft_geom::Point::new(cx, 60.0), rot };
        let opts = (StitchView::Lines, usize::MAX, false);
        let mut cache = MeshCache::default();
        cache.update(&plan, &view(50.0, 2.0, 0.0), opts, ([1, 1], 0.0));
        let base_len = cache.base.vertices.len();
        // pan, rotate and zoom a little: placed from the same base, where a fresh build puts it
        for (cx, zoom, rot) in [(80.0, 2.0, 0.0), (80.0, 2.2, 0.3), (10.0, 1.7, -1.0)] {
            cache.update(&plan, &view(cx, zoom, rot), opts, ([1, 1], 0.0));
            assert_eq!(cache.base_zoom, 2.0, "not built again");
            let (fresh, _) = build(plan.pattern(), &view(cx, zoom, rot), StitchView::Lines, usize::MAX, false, egui::Rect::EVERYTHING);
            // the same stitches; a stitch's quad keeps its width in pixels, so compare its middle
            let mid = |m: &Mesh, i: usize| {
                let v = &m.vertices[i * 4..i * 4 + 4];
                v.iter().fold(egui::Vec2::ZERO, |a, x| a + x.pos.to_vec2()) / 4.0
            };
            assert_eq!(cache.mesh.vertices.len(), base_len);
            for i in [0, base_len / 8, base_len / 4 - 1] {
                let (a, b) = (mid(&cache.mesh, i), mid(&fresh, i));
                assert!((a - b).length() < 0.05, "{cx} {zoom} {rot}: {a:?} {b:?}");
            }
        }
        // a big zoom builds it again
        cache.update(&plan, &view(10.0, 5.0, 0.0), opts, ([1, 1], 0.0));
        assert_eq!(cache.base_zoom, 5.0);
    }

    /// Timings of the overlay on a design of over 100,000 stitches (run by hand:
    /// `cargo test -p vectorcraft-ui-egui --lib big_design_overlay_timings -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn big_design_overlay_timings() {
        let mut s = vectorcraft_engine::Session::new();
        s.execute("file.new", &serde_json::json!({"width": 1400, "height": 1400})).unwrap();
        for i in 0..120 {
            let (x, y) = (20.0 + (i % 11) as f64 * 110.0, 20.0 + (i / 11) as f64 * 110.0);
            s.execute("shape.rectangle", &serde_json::json!({"x": x, "y": y, "width": 100, "height": 100})).unwrap();
            s.execute("embroidery.set", &serde_json::json!({"type": "fill"})).unwrap();
        }
        let plan = s.embroidery_plan().unwrap();
        let n = plan.pattern().stitch_count();
        let rect = egui::Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(1600.0, 1000.0));
        let view = |cx: f64| Xf { rect, zoom: 0.8, center: vectorcraft_geom::Point::new(cx, 700.0), rot: 0.0 };
        for mode in [StitchView::Lines, StitchView::TrueView] {
            let mut cache = MeshCache::default();
            let t = std::time::Instant::now();
            cache.update(&plan, &view(700.0), (mode, usize::MAX, false), ([1, 1], 0.0));
            let built = t.elapsed();
            let t = std::time::Instant::now();
            for k in 0..10 {
                cache.update(&plan, &view(700.0 + f64::from(k)), (mode, usize::MAX, false), ([1, 1], 0.0));
            }
            let pan = t.elapsed() / 10;
            // one object's density changed: its pieces built again, the rest reused
            let mid = s.doc().unwrap().doc.layers[0].children().unwrap()[60].id.0;
            s.execute("select.set", &serde_json::json!({"ids": [mid]})).unwrap();
            s.execute("embroidery.setParams", &serde_json::json!({"params": {"spacing": 0.45}})).unwrap();
            let edited = s.embroidery_plan().unwrap();
            let t = std::time::Instant::now();
            cache.update(&edited, &view(700.0), (mode, usize::MAX, false), ([1, 1], 0.0));
            let edit = t.elapsed();
            s.execute("edit.undo", &serde_json::json!({})).unwrap();
            eprintln!("{n} stitches, {mode:?}: build {built:?}, pan {pan:?}, after an edit {edit:?}, {} vertices", cache.mesh.vertices.len());
        }
    }

    /// The mesh built from reused pieces is the mesh a fresh build gives: after an edit to one
    /// object, and while the player sews on.
    #[test]
    fn reused_pieces_draw_what_a_fresh_build_draws() {
        let mut s = vectorcraft_engine::Session::new();
        s.execute("file.new", &serde_json::json!({"width": 400, "height": 400})).unwrap();
        let mut ids = Vec::new();
        for i in 0..4 {
            let r = s.execute("shape.rectangle", &serde_json::json!({"x": 20 + i * 90, "y": 40, "width": 80, "height": 60})).unwrap();
            s.execute("embroidery.set", &serde_json::json!({"type": "fill"})).unwrap();
            s.execute("embroidery.setThread", &serde_json::json!({"color": if i % 2 == 0 { "#aa2222" } else { "#2222aa" }})).unwrap();
            ids.push(r["id"].clone());
        }
        let rect = egui::Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(800.0, 600.0));
        let xf = Xf { rect, zoom: 3.0, center: vectorcraft_geom::Point::new(200.0, 70.0), rot: 0.0 };
        let same = |cache: &MeshCache, plan: &Arc<StitchPlan>, mode, upto| {
            let (fresh, _) = build(plan.pattern(), &xf, mode, upto, false, egui::Rect::EVERYTHING);
            assert_eq!(cache.mesh.vertices.len(), fresh.vertices.len());
            assert_eq!(cache.mesh.indices, fresh.indices);
            for (a, b) in cache.mesh.vertices.iter().zip(&fresh.vertices) {
                assert!((a.pos - b.pos).length() < 0.01 && a.color == b.color);
            }
        };
        for mode in [StitchView::Lines, StitchView::TrueView] {
            let mut cache = MeshCache::default();
            let plan = s.embroidery_plan().unwrap();
            cache.update(&plan, &xf, (mode, usize::MAX, false), ([1, 1], 0.0));
            same(&cache, &plan, mode, usize::MAX);
            // the third square made denser
            s.execute("select.set", &serde_json::json!({"ids": [ids[2]]})).unwrap();
            s.execute("embroidery.setParams", &serde_json::json!({"params": {"spacing": 0.6}})).unwrap();
            let plan = s.embroidery_plan().unwrap();
            let before: Vec<u64> = cache.pieces.iter().map(|p| p.hash).collect();
            cache.update(&plan, &xf, (mode, usize::MAX, false), ([1, 1], 0.0));
            same(&cache, &plan, mode, usize::MAX);
            let kept = cache.pieces.iter().filter(|p| before.contains(&p.hash)).count();
            assert!(kept >= 2, "the first squares' pieces are reused ({kept} of {})", cache.pieces.len());
            // the player sewing on
            let total = plan.pattern().stitches.len();
            for upto in [total / 3, total / 2, total - 1] {
                cache.update(&plan, &xf, (mode, upto, false), ([1, 1], 0.0));
                same(&cache, &plan, mode, upto);
            }
            s.execute("edit.undo", &serde_json::json!({})).unwrap();
        }
    }

    #[test]
    fn the_mesh_follows_a_resized_canvas() {
        // a window resize keeps the canvas's top left but moves its centre, where the view's
        // centre is drawn: the cached stitches must move with it
        let mut s = vectorcraft_engine::Session::new();
        s.execute("file.new", &serde_json::json!({"width": 200, "height": 200})).unwrap();
        s.execute("shape.rectangle", &serde_json::json!({"x": 20, "y": 20, "width": 60, "height": 40})).unwrap();
        s.execute("embroidery.set", &serde_json::json!({"type": "fill"})).unwrap();
        let plan = s.embroidery_plan().unwrap();
        let xf = |w: f32| Xf {
            rect: egui::Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(w, 600.0)),
            zoom: 1.0,
            center: vectorcraft_geom::Point::new(100.0, 100.0),
            rot: 0.0,
        };
        let mut cache = MeshCache::default();
        cache.update(&plan, &xf(800.0), (StitchView::Lines, usize::MAX, false), ([1, 1], 0.0));
        let before = cache.mesh.vertices.first().map(|v| v.pos).unwrap();
        cache.update(&plan, &xf(1000.0), (StitchView::Lines, usize::MAX, false), ([1, 1], 0.0));
        let after = cache.mesh.vertices.first().map(|v| v.pos).unwrap();
        assert!((after.x - before.x - 100.0).abs() < 0.5, "{before:?} → {after:?}");
    }
}
