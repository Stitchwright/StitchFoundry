//! # Embroidery mode
//!
//! Bridges VectorCraft documents and the StitchFoundry embroidery engine
//! ([`sf_core`]). Any path, compound path or (soon) type object can carry an
//! [`Embroidery`] record ([`vectorcraft_doc::Node::embroidery`]); the stitch plan
//! is generated from the artwork in paint order (bottom first = sewn first),
//! like Ink/Stitch does inside an SVG editor.
//!
//! Units: documents are in points (y down); the embroidery engine works in
//! millimetres (y down). [`mm`] and [`pt`] convert between them.

#![forbid(unsafe_code)]
mod convert;
pub mod font_library;
pub mod fonts;
pub mod inkstitch;

pub use convert::{holes, parts};

use sf_core::attr::Embroidery;
use sf_core::engine::{Built, StitchCache, build_cached, object_polygons};
use sf_core::geom::{BBox, FLATTEN_TOLERANCE, Polygon, Shape, poly};
use sf_core::model::{Design, EmbObject, ObjectId, ObjectKind};
use sf_core::pattern::Pattern;
use sf_core::stats::{PatternStats, pattern_stats};
use sf_core::threads::{Rgb, Thread};
use std::collections::HashMap;
use std::sync::Arc;
use vectorcraft_doc::{Document, Node, NodeId, NodeKind};

pub use sf_core;

/// Millimetres per point.
pub const MM_PER_PT: f64 = 25.4 / 72.0;
/// Points per millimetre.
pub const PT_PER_MM: f64 = 72.0 / 25.4;

/// Document point (pt) → embroidery point (mm).
#[inline]
pub fn mm(p: kurbo::Point) -> sf_core::Point {
    sf_core::Point::new(p.x * MM_PER_PT, p.y * MM_PER_PT)
}

/// Embroidery point (mm) → document point (pt).
#[inline]
pub fn pt(p: sf_core::Point) -> kurbo::Point {
    kurbo::Point::new(p.x * PT_PER_MM, p.y * PT_PER_MM)
}

/// The stitch plan of a document.
#[derive(Clone, Debug, Default)]
pub struct StitchPlan {
    /// Embroidery objects in sewing order (millimetres).
    pub design: Design,
    /// The document node each design object came from.
    pub nodes: Vec<NodeId>,
    /// The machine pattern (millimetres) and where each object's stitches are.
    pub built: Built,
    pub stats: Option<PatternStats>,
    /// The design's own threads in the order first used (its colorways' slots); `design`'s
    /// palette holds the active colorway's.
    pub own_threads: Vec<sf_core::Thread>,
}

impl StitchPlan {
    pub fn pattern(&self) -> &Pattern {
        &self.built.pattern
    }

    /// The document node that sews pattern record `i`.
    pub fn node_at_record(&self, i: usize) -> Option<NodeId> {
        let span = self.built.object_at(i)?;
        self.nodes.get(span.index).copied()
    }

    /// Pattern record ranges sewn by a node.
    pub fn records_of(&self, node: NodeId) -> Vec<std::ops::Range<usize>> {
        self.built.spans.iter().filter(|s| self.nodes.get(s.index) == Some(&node)).map(|s| s.range.clone()).collect()
    }

    /// A node's own stitches (millimetres) as its objects generate them, without the
    /// lock stitches and connections the plan adds round them; parts too far apart
    /// to sew across are separated by a trim.
    pub fn own_stitches(&self, node: NodeId) -> Vec<sf_core::Stitch> {
        let trim = self.design.settings.trim_threshold;
        let mut out: Vec<sf_core::Stitch> = Vec::new();
        for span in self.built.spans.iter().filter(|s| self.nodes.get(s.index) == Some(&node)) {
            let last = out.iter().rev().find(|s| s.is_move()).copied();
            let first = span.own.iter().find(|s| s.cmd == sf_core::Cmd::Stitch).copied();
            if let (Some(a), Some(b)) = (last, first)
                && a.pos().distance(b.pos()) > trim
            {
                out.push(sf_core::Stitch::new(a.x, a.y, sf_core::Cmd::Trim));
            }
            out.extend(span.own.iter().copied());
        }
        out
    }

    /// Whether the plan has any stitches.
    pub fn is_empty(&self) -> bool {
        self.built.pattern.stitches.is_empty()
    }
}

/// Every stitched node in sewing order (paint order: bottom layer first, back to
/// front), skipping hidden ones.
pub fn stitched_nodes(doc: &Document) -> Vec<Arc<Node>> {
    fn walk(n: &Arc<Node>, visible: bool, out: &mut Vec<Arc<Node>>) {
        let visible = visible && n.visible;
        if !visible {
            return;
        }
        if n.embroidery.is_some() {
            out.push(n.clone());
            // a stitched group or compound is one object: don't descend
            if !matches!(n.kind, NodeKind::Layer { .. } | NodeKind::Group { .. }) || sews_whole(n) {
                return;
            }
        }
        if let Some(children) = n.children() {
            for c in children {
                walk(c, visible, out);
            }
        }
    }
    let mut out = Vec::new();
    for l in &doc.layers {
        walk(l, true, &mut out);
    }
    out
}

/// Whether a group is sewn as one object made of all its art (redwork), its members' own
/// stitching left out.
pub fn sews_whole(n: &Node) -> bool {
    n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, sf_core::attr::StitchType::Redwork(_)))
}

/// Most instances of one repeat that are sewn.
const MAX_REPEAT_INSTANCES: usize = 1000;

/// Repeats and envelopes evaluated for sewing, kept while the object doesn't change (the same
/// `Arc`), so the plan of an unchanged document stays the same.
#[derive(Default)]
struct LiveCache(HashMap<NodeId, (Arc<Node>, Vec<Arc<Node>>)>);

impl LiveCache {
    /// What a repeat or envelope sews: its source moved to each instance, or its contents bent
    /// through it. The objects keep their ids, so selecting one shows the stitches of every copy.
    fn expand(&mut self, n: &Arc<Node>, fresh: &mut HashMap<NodeId, (Arc<Node>, Vec<Arc<Node>>)>) -> Vec<Arc<Node>> {
        let out = match self.0.remove(&n.id) {
            Some((key, out)) if Arc::ptr_eq(&key, n) => out,
            _ => match &n.kind {
                NodeKind::Repeat(spec) => spec
                    .transforms(None)
                    .into_iter()
                    .take(MAX_REPEAT_INSTANCES)
                    .flat_map(|m| {
                        spec.source.iter().map(move |c| {
                            let mut copy = (**c).clone();
                            copy.transform(m, false);
                            Arc::new(copy)
                        })
                    })
                    .collect(),
                NodeKind::Envelope { .. } => vectorcraft_doc::live::expand_live(n).into_iter().map(Arc::new).collect(),
                _ => Vec::new(),
            },
        };
        fresh.insert(n.id, (n.clone(), out.clone()));
        out
    }

    /// A stitched path or compound path with live geometry effects (Effect › Warp, Distort &
    /// Transform) as it shows: its geometry reshaped by them, the effects taken off. Anything else
    /// as it is.
    fn reshaped(&mut self, n: &Arc<Node>, fresh: &mut HashMap<NodeId, (Arc<Node>, Vec<Arc<Node>>)>) -> Arc<Node> {
        if !matches!(n.kind, NodeKind::Path { .. } | NodeKind::Compound { .. }) || !vectorcraft_effects::has_geometry(&n.appearance.effects) {
            return n.clone();
        }
        let out = match self.0.remove(&n.id) {
            Some((key, out)) if Arc::ptr_eq(&key, n) => out,
            _ => vec![Arc::new(with_geometry_effects(n))],
        };
        let shown = out.first().cloned().unwrap_or_else(|| n.clone());
        fresh.insert(n.id, (n.clone(), out));
        shown
    }
}

/// `n` (a path or compound path) with its visible geometry effects applied to its paths, each
/// taking the object's bounds as its reference box as the art does, and then taken off.
fn with_geometry_effects(n: &Node) -> Node {
    let mut c = n.clone();
    let Some(bounds) = n.geometric_bounds() else { return c };
    let fx = &n.appearance.effects;
    match &mut c.kind {
        NodeKind::Path { path, live, .. } => {
            *path = vectorcraft_effects::apply_geometry(fx, path, bounds);
            *live = None;
        }
        NodeKind::Compound { children, .. } => {
            for ch in children.iter_mut() {
                if let NodeKind::Path { path, live, .. } = &mut Arc::make_mut(ch).kind {
                    *path = vectorcraft_effects::apply_geometry(fx, path, bounds);
                    *live = None;
                }
            }
        }
        _ => {}
    }
    c.appearance.effects.retain(|e| !vectorcraft_effects::is_geometry(&e.id));
    c
}

/// The stitched nodes in sewing order as [`stitched_nodes`] lists them, with repeats sewn once
/// per instance and envelopes' contents bent through them.
fn sewing_order(doc: &Document, cache: &mut LiveCache) -> Vec<Arc<Node>> {
    fn walk(n: &Arc<Node>, cache: &mut LiveCache, fresh: &mut HashMap<NodeId, (Arc<Node>, Vec<Arc<Node>>)>, out: &mut Vec<Arc<Node>>) {
        if !n.visible {
            return;
        }
        if n.embroidery.is_some() {
            out.push(cache.reshaped(n, fresh));
            if !matches!(n.kind, NodeKind::Layer { .. } | NodeKind::Group { .. }) || sews_whole(n) {
                return;
            }
        }
        if matches!(n.kind, NodeKind::Repeat(_) | NodeKind::Envelope { .. }) {
            for c in cache.expand(n, fresh) {
                walk(&c, cache, fresh, out);
            }
        } else if let Some(children) = n.children() {
            for c in children {
                walk(c, cache, fresh, out);
            }
        }
    }
    let mut fresh = HashMap::new();
    let mut out = Vec::new();
    for l in &doc.layers {
        walk(l, cache, &mut fresh, &mut out);
    }
    // Forget repeats and envelopes that are gone.
    cache.0 = fresh;
    out
}

/// Thread of a stitched node: its own thread, else its paint colour.
pub fn node_thread(node: &Node, e: &Embroidery) -> Thread {
    if let Some(t) = &e.thread {
        return t.clone();
    }
    let prefer_stroke = matches!(e.stitch, sf_core::attr::StitchType::Run(_) | sf_core::attr::StitchType::Manual)
        || (matches!(e.stitch, sf_core::attr::StitchType::Satin(_)) && node.appearance.fill_paint().is_none());
    let c = convert::paint_color(node, prefer_stroke).unwrap_or(Rgb::BLACK);
    Thread::new(c, c.to_hex())
}

/// An editable vector path for a StitchFoundry contour (corner nodes stay corners,
/// curve nodes get smooth Bézier handles). Coordinates are passed through as is.
pub fn contour_to_path(c: &sf_core::geom::Contour) -> vectorcraft_geom::path::PathData {
    use vectorcraft_geom::path::{Anchor, AnchorKind, PathData, SubPath};
    let handles = sf_core::geom::shape::bezier_handles(&c.nodes, c.closed);
    let anchors = c
        .nodes
        .iter()
        .zip(handles)
        .map(|(n, (hin, hout))| {
            let p = kurbo::Point::new(n.p.x, n.p.y);
            Anchor {
                p,
                h_in: kurbo::Point::new(n.p.x + hin.x, n.p.y + hin.y),
                h_out: kurbo::Point::new(n.p.x + hout.x, n.p.y + hout.y),
                kind: if n.kind == sf_core::geom::NodeKind::Curve { AnchorKind::Smooth } else { AnchorKind::Corner },
            }
        })
        .collect();
    PathData::single(SubPath::new(anchors, c.closed))
}

/// What the stitched art in `node` (and inside it) covers, in millimetres: fills and appliqué
/// their shapes, satin columns their outlines; art sewn only as lines, its closed paths' areas.
pub fn covered_mm(node: &Node) -> Vec<sf_core::geom::Polygon> {
    fn walk(n: &Node, out: &mut Vec<sf_core::geom::Polygon>) {
        if let Some(e) = n.embroidery.as_deref()
            && !matches!(n.kind, NodeKind::Group { .. })
        {
            let before = out.len();
            for kind in convert::convert(n, e) {
                out.extend(sf_core::engine::object_polygons(&EmbObject::new(0, kind, 0)));
            }
            if out.len() == before {
                out.extend(regions_mm(n));
            }
        }
        for c in n.children().into_iter().flatten() {
            walk(c, out);
        }
    }
    let mut out = Vec::new();
    walk(node, &mut out);
    out.retain(|p| p.outer.len() >= 3);
    out
}

/// The filled regions of a path or compound path, in millimetres.
pub fn regions_mm(node: &Node) -> Vec<sf_core::geom::Polygon> {
    match convert::node_lines(node) {
        Some((lines, rule)) => convert::regions(&lines, rule),
        None => Vec::new(),
    }
}

/// A node's paths (type: its glyphs' outlines) as polylines in millimetres, each closed one
/// ending where it starts.
pub fn outlines_mm(node: &Node) -> Vec<Vec<sf_core::Point>> {
    let Some((lines, _)) = convert::node_lines(node) else { return Vec::new() };
    lines
        .into_iter()
        .map(|l| {
            let mut pts = l.pts;
            if l.closed
                && let Some(&first) = pts.first()
            {
                pts.push(first);
            }
            pts
        })
        .collect()
}

/// Whether a node's geometry is all open paths (a stroke rather than an area).
pub fn is_open(node: &Node) -> bool {
    convert::node_lines(node).is_some_and(|(lines, _)| !lines.is_empty() && lines.iter().all(|l| !l.closed))
}

/// Whether satin sews a node along its paths rather than across its area: open
/// paths, and closed ones painted with a stroke but no fill (a satin border round a
/// shape).
pub fn is_line_art(node: &Node) -> bool {
    is_open(node) || (!has_fill(node) && !matches!(node.kind, NodeKind::Text(_)) && convert::node_lines(node).is_some_and(|(l, _)| !l.is_empty()))
}

/// Whether a node can carry embroidery (paths, compound paths and type).
pub fn stitchable(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Path { .. } | NodeKind::Compound { .. } | NodeKind::Text(_))
}

/// Satin stitches stay this short at most (mm): thicker shapes are filled.
pub const MAX_SATIN_WIDTH: f64 = 7.0;

/// The stitch type an object reads as, the way a digitizer would pick it: areas
/// (type included) are satin when every part is at most [`MAX_SATIN_WIDTH`] thick
/// (thin columns, small lettering) and tatami otherwise; strokes are satin lines
/// when wide, bean (triple) runs when medium and runs when hairlines.
pub fn suggest(node: &Node) -> Embroidery {
    let area = matches!(node.kind, NodeKind::Text(_)) || (convert::paint_color(node, false).is_some() && !is_open(node) && has_fill(node));
    if area {
        let regions = regions_mm(node);
        let thin = !regions.is_empty() && regions.iter().all(|r| r.thickness() <= MAX_SATIN_WIDTH);
        return if thin { Embroidery::satin() } else { Embroidery::fill() };
    }
    let stroke_w = node.appearance.stroke_width() * MM_PER_PT;
    if stroke_w >= 1.2 {
        let mut e = Embroidery::satin();
        e.width = Some(stroke_w);
        e
    } else if stroke_w >= 0.6 {
        Embroidery::new(sf_core::attr::StitchType::Run(sf_core::model::RunParams { kind: sf_core::model::RunKind::Triple, ..Default::default() }))
    } else {
        Embroidery::run()
    }
}

/// Whether a node paints a fill (type always does).
pub(crate) fn has_fill(node: &Node) -> bool {
    !matches!(node.appearance.fill_paint(), vectorcraft_color::Paint::None)
}

/// Builds the embroidery design of a document (no caching).
pub fn design_from_doc(doc: &Document) -> (Design, Vec<NodeId>) {
    let mut p = Planner::default();
    let plan = p.plan(doc);
    (plan.design.clone(), plan.nodes.clone())
}

struct NodeEntry {
    node: Arc<Node>,
    objects: Vec<EmbObject>,
}

/// Incremental stitch planner. Unchanged nodes (same shared `Arc`) reuse their
/// converted objects, and unchanged objects reuse their stitches.
///
/// The planner keeps a reference to every stitched node it has seen, so the
/// document's copy-on-write edits always give a changed node a new `Arc`: pointer
/// equality is a reliable "unchanged" test.
#[derive(Default)]
pub struct Planner {
    /// Converted objects by node and occurrence (the copies of a repeat share their source's id).
    nodes: HashMap<(NodeId, usize), NodeEntry>,
    cache: StitchCache,
    live: LiveCache,
    last: Option<Snapshot>,
}

struct Snapshot {
    order: Vec<Arc<Node>>,
    settings: Option<sf_core::attr::DocEmbroidery>,
    plan: Arc<StitchPlan>,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }

    /// The stitch plan for `doc`, reusing everything that didn't change.
    pub fn plan(&mut self, doc: &Document) -> Arc<StitchPlan> {
        let order = sewing_order(doc, &mut self.live);
        let settings = doc.embroidery.as_deref().cloned();
        if let Some(s) = &self.last
            && s.settings == settings
            && s.order.len() == order.len()
            && s.order.iter().zip(&order).all(|(a, b)| Arc::ptr_eq(a, b))
        {
            return s.plan.clone();
        }
        let plan = Arc::new(self.build(doc, &order));
        self.last = Some(Snapshot { order, settings, plan: plan.clone() });
        plan
    }

    /// The embroidery objects of each node in `stitched` that isn't cached as it is (by index;
    /// `None` for the cached ones). On the desktop many are converted on several threads.
    fn convert_changed(&self, stitched: &[Arc<Node>]) -> Vec<Option<Vec<ObjectKind>>> {
        let mut seen: HashMap<NodeId, usize> = HashMap::new();
        let todo: Vec<usize> = stitched
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                let occurrence = seen.entry(n.id).or_insert(0);
                let key = (n.id, *occurrence);
                *occurrence += 1;
                n.embroidery.is_some() && !self.nodes.get(&key).is_some_and(|entry| Arc::ptr_eq(&entry.node, n))
            })
            .map(|(i, _)| i)
            .collect();
        let mut out: Vec<Option<Vec<ObjectKind>>> = vec![None; stitched.len()];
        let one = |i: usize| stitched.get(i).and_then(|n| n.embroidery.as_deref().map(|e| convert::convert(n, e)));
        #[cfg(not(target_arch = "wasm32"))]
        {
            const MIN_PARALLEL: usize = 8;
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 8);
            if todo.len() >= MIN_PARALLEL && threads > 1 {
                let chunk = todo.len().div_ceil(threads).max(1);
                let results: Vec<Vec<(usize, Option<Vec<ObjectKind>>)>> = std::thread::scope(|sc| {
                    let handles: Vec<_> =
                        todo.chunks(chunk).map(|part| sc.spawn(move || part.iter().map(|&i| (i, one(i))).collect::<Vec<_>>())).collect();
                    // a thread that failed leaves its nodes to be converted one by one below
                    handles.into_iter().filter_map(|h| h.join().ok()).collect()
                });
                for (i, kinds) in results.into_iter().flatten() {
                    if let Some(slot) = out.get_mut(i) {
                        *slot = kinds;
                    }
                }
                return out;
            }
        }
        for i in todo {
            if let Some(slot) = out.get_mut(i) {
                *slot = one(i);
            }
        }
        out
    }

    fn build(&mut self, doc: &Document, stitched: &[Arc<Node>]) -> StitchPlan {
        let settings = doc.embroidery.as_ref().map(|e| e.settings.clone()).unwrap_or_default();
        let mut design = Design::new();
        design.palette.clear();
        design.settings = settings;
        design.hoop = doc.embroidery.as_ref().and_then(|e| e.hoop.clone());
        let mut node_ids = Vec::new();
        let branches = branch_groups(doc);
        // objects that remove the stitching below them: (first object index, what they cover)
        let mut cuts: Vec<(usize, Vec<Polygon>)> = Vec::new();
        let mut fresh: HashMap<(NodeId, usize), NodeEntry> = HashMap::with_capacity(stitched.len());
        let mut seen: HashMap<NodeId, usize> = HashMap::new();
        // the nodes not cached (new or changed) converted first, side by side where there are
        // many: each node's conversion stands alone
        let mut converted = self.convert_changed(stitched);
        for (index, n) in stitched.iter().enumerate() {
            let Some(e) = n.embroidery.as_deref() else { continue };
            // the copies of a repeat share their source's id: tell them apart
            let occurrence = seen.entry(n.id).or_insert(0);
            let key = (n.id, *occurrence);
            *occurrence += 1;
            let objects = match self.nodes.remove(&key) {
                Some(entry) if Arc::ptr_eq(&entry.node, n) => entry.objects,
                _ => {
                    let thread = node_thread(n, e);
                    let name = n.name.clone().unwrap_or_else(|| e.stitch.label().to_string());
                    let kinds = converted.get_mut(index).and_then(Option::take).unwrap_or_else(|| convert::convert(n, e));
                    // a Multi Blend fill: one object per layer, each in its own thread
                    let mut parts: Vec<(ObjectKind, Rgb, bool)> = Vec::new();
                    for kind in kinds {
                        match &kind {
                            ObjectKind::Fill(f) if !f.params.blend.is_empty() => {
                                for (k, (params, layer)) in f.params.blend_layers().into_iter().enumerate() {
                                    let color = layer.map_or(thread.color, |t| t.color);
                                    parts.push((ObjectKind::Fill(sf_core::model::FillObject { params, ..f.clone() }), color, k == 0));
                                }
                            }
                            _ => parts.push((kind, thread.color, true)),
                        }
                    }
                    let count = parts.len();
                    parts
                        .into_iter()
                        .enumerate()
                        .map(|(k, (kind, color, lead))| {
                            // stable ids per node part (and copy) keep the stitch cache warm
                            let copy = (key.1 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                            let id: ObjectId = n.id.0.wrapping_mul(64).wrapping_add(k as u64) ^ copy;
                            let mut o = EmbObject::new(id, kind, 0);
                            o.name = name.clone();
                            // a blend's later layers find their own way in and out
                            o.entry = e.entry.filter(|_| lead).map(|p| p * MM_PER_PT);
                            o.exit = e.exit.filter(|_| lead).map(|p| p * MM_PER_PT);
                            o.trim_after = e.trim_after;
                            o.tie_in = e.tie_in;
                            o.tie_off = e.tie_off;
                            o.stop_after = e.stop_after && k + 1 == count;
                            o.own_block = e.own_block && k == 0;
                            o.omit_last = e.omit_last && k + 1 == count;
                            o.chenille = e.chenille;
                            // the palette index is resolved below; keep the thread colour
                            o.color = usize::from(color.r) << 16 | usize::from(color.g) << 8 | usize::from(color.b);
                            o
                        })
                        .collect()
                }
            };
            let start = design.objects.len();
            let branch = branches.get(&n.id).map(|g| g.0);
            // the columns a satin shape was cut into (a letter's strokes) sewn as one path
            let (order, pieces) = if branch.is_none() && design.settings.hide_travel {
                piece_branches(&objects, n.id.0, key.1)
            } else {
                ((0..objects.len()).collect(), vec![None; objects.len()])
            };
            for (o, auto) in order.iter().filter_map(|&k| objects.get(k)).zip(pieces) {
                let mut o = o.clone();
                o.branch = branch.or(auto);
                let rgb = Rgb::new((o.color >> 16) as u8, (o.color >> 8) as u8, o.color as u8);
                let blended = match &e.stitch {
                    sf_core::attr::StitchType::Fill(f) => f.blend.iter().map(|l| &l.thread).find(|t| t.color == rgb).cloned(),
                    _ => None,
                };
                let thread = e.thread.clone().filter(|t| t.color == rgb).or(blended).unwrap_or_else(|| Thread::new(rgb, rgb.to_hex()));
                o.color = design.thread_index(&thread);
                design.objects.push(o);
                node_ids.push(n.id);
            }
            if e.remove_underlying && start > 0 {
                let cover = cover_of(&design.objects[start..], e.cut_overlap());
                if !cover.is_empty() {
                    cuts.push((start, cover));
                }
            }
            fresh.insert(key, NodeEntry { node: n.clone(), objects });
        }
        self.nodes = fresh;
        // shapes that touch overlap a little (before the cuts, which take back what they cover)
        underlap(&mut design.objects, &node_ids, design.settings.overlap_touching);
        cut_underlying(&mut design.objects, &mut node_ids, cuts);
        cover_partial_applique(&mut design.objects);
        order_branches(&mut design.objects, &mut node_ids);
        design.next_id = design.objects.iter().map(|o| o.id).max().unwrap_or(0) + 1;
        // the active colorway's threads in place of the design's own
        let own_threads = design.palette.clone();
        if let Some(cfg) = doc.embroidery.as_deref().filter(|c| c.colorway > 0) {
            design.palette = cfg.threads_of(cfg.colorway, &own_threads);
        }
        let built = build_cached(&design, &mut self.cache);
        let stats = (!built.pattern.stitches.is_empty()).then(|| pattern_stats(&built.pattern, &design.settings));
        StitchPlan { design, nodes: node_ids, built, stats, own_threads }
    }
}

/// The branch group (a group whose stitch type is Branch) each node belongs to.
fn branch_groups(doc: &Document) -> HashMap<NodeId, NodeId> {
    fn walk(n: &Arc<Node>, branch: Option<NodeId>, out: &mut HashMap<NodeId, NodeId>) {
        let here = match n.embroidery.as_deref().map(|e| &e.stitch) {
            Some(sf_core::attr::StitchType::Branch) if matches!(n.kind, NodeKind::Group { .. }) => Some(n.id),
            _ => branch,
        };
        if let Some(g) = here
            && g != n.id
        {
            out.insert(n.id, g);
        }
        for c in n.children().into_iter().flatten() {
            walk(c, here, out);
        }
    }
    let mut out = HashMap::new();
    for l in &doc.layers {
        walk(l, None, &mut out);
    }
    out
}

/// The bit that marks a branch made of a shape's pieces (see [`piece_branches`]); document ids
/// never reach it.
const PIECE_BRANCH: u64 = 1 << 63;
/// Objects of one node apart from this (mm) are pieces of different shapes.
const PIECE_TOUCH: f64 = 0.3;

/// The satin pieces a node's shape was cut into (an "E" into its strokes, an "O" into two
/// columns) that touch one another, each group sewn as a branch of its own: the thread runs on
/// under the pieces still to come instead of being cut between them, and the pieces are sewn in
/// the order that keeps it short. Returns the order to add the node's objects in (each group
/// together) and the branch of each, in that order; `None` for objects on their own, and for
/// ones with their own entry, exit, trim or stop, which keep sewing as they are.
fn piece_branches(objects: &[EmbObject], node: u64, copy: usize) -> (Vec<usize>, Vec<Option<u64>>) {
    let candidate = |o: &EmbObject| {
        matches!(&o.kind, ObjectKind::Fill(f) if f.params.kind == sf_core::model::FillKind::Satin)
            && o.entry.is_none()
            && o.exit.is_none()
            && !o.trim_after
            && !o.stop_after
            && o.chenille.is_none()
    };
    let n = objects.len();
    let mut group: Vec<usize> = (0..n).collect();
    fn root(group: &mut [usize], mut i: usize) -> usize {
        while let Some(&up) = group.get(i) {
            if up == i {
                break;
            }
            // halve the path on the way
            let grand = group.get(up).copied().unwrap_or(up);
            if let Some(g) = group.get_mut(i) {
                *g = grand;
            }
            i = up;
        }
        i
    }
    let areas: Vec<Option<(Vec<Polygon>, BBox)>> =
        objects.iter().map(|o| candidate(o).then(|| (object_polygons(o), o.bbox())).filter(|(p, b)| !p.is_empty() && !b.is_empty())).collect();
    let touch = |a: &[Polygon], b: &[Polygon]| {
        let near = |from: &[Polygon], to: &[Polygon]| {
            from.iter().flat_map(|p| p.rings()).flatten().any(|q| to.iter().any(|t| t.contains(*q) || t.distance_to_boundary(*q) <= PIECE_TOUCH))
        };
        near(a, b) || near(b, a)
    };
    let mut links: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        let Some(Some((pa, ba))) = areas.get(i) else { continue };
        for j in i + 1..n {
            let Some(Some((pb, bb))) = areas.get(j) else { continue };
            let apart = ba.min.x > bb.max.x + PIECE_TOUCH
                || bb.min.x > ba.max.x + PIECE_TOUCH
                || ba.min.y > bb.max.y + PIECE_TOUCH
                || bb.min.y > ba.max.y + PIECE_TOUCH;
            if !apart && touch(pa, pb) {
                links.push((i, j));
                let (ra, rb) = (root(&mut group, i), root(&mut group, j));
                if let Some(g) = group.get_mut(ra.max(rb)) {
                    *g = ra.min(rb);
                }
            }
        }
    }
    let roots: Vec<usize> = (0..n).map(|i| root(&mut group, i)).collect();
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for r in &roots {
        *sizes.entry(*r).or_insert(0) += 1;
    }
    // each group where its first piece was (a group's root is its first piece), its pieces in
    // the order that leaves the rest in one piece (see [`peel`])
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, r) in roots.iter().enumerate() {
        if let Some(m) = members.get_mut(*r) {
            m.push(i);
        }
    }
    let centers: Vec<sf_core::Point> = objects.iter().map(|o| o.bbox().center()).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut at: Option<sf_core::Point> = None;
    for m in members.iter().filter(|m| !m.is_empty()) {
        let peeled = peel(m, &links, &centers, at);
        at = peeled.last().and_then(|&k| centers.get(k)).copied();
        order.extend(peeled);
    }
    let salt = (copy as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let branches = order
        .iter()
        .map(|&i| {
            let r = roots.get(i).copied().unwrap_or(i);
            // ids of their own, apart from the document's group ids
            (sizes.get(&r).is_some_and(|&s| s > 1)).then(|| PIECE_BRANCH | ((node.wrapping_mul(1 << 16).wrapping_add(r as u64) ^ salt) >> 1))
        })
        .collect();
    (order, branches)
}

/// The pieces `group` (connected by `links`) in an order where each, once sewn, leaves the
/// pieces still to come touching it and in one piece: the thread then always reaches the next
/// one hidden under them. Each is the nearest such piece to the one before (the first, the
/// nearest to `from`, else the leftmost). Large groups keep their order.
fn peel(group: &[usize], links: &[(usize, usize)], centers: &[sf_core::Point], from: Option<sf_core::Point>) -> Vec<usize> {
    const MAX_PEEL: usize = 200;
    if group.len() < 3 || group.len() > MAX_PEEL {
        return group.to_vec();
    }
    let center = |k: usize| centers.get(k).copied().unwrap_or_default();
    let linked = |a: usize, b: usize| links.iter().any(|&(x, y)| (x, y) == (a, b) || (x, y) == (b, a));
    // whether `rest` holds together
    let whole = |rest: &[usize]| {
        let Some(&first) = rest.first() else { return true };
        let mut seen = vec![first];
        let mut todo = vec![first];
        while let Some(k) = todo.pop() {
            for &o in rest {
                if !seen.contains(&o) && linked(k, o) {
                    seen.push(o);
                    todo.push(o);
                }
            }
        }
        seen.len() == rest.len()
    };
    let mut rest = group.to_vec();
    let mut out = Vec::with_capacity(group.len());
    let mut at = from;
    while !rest.is_empty() {
        let free: Vec<usize> = rest
            .iter()
            .copied()
            .filter(|&k| {
                let others: Vec<usize> = rest.iter().copied().filter(|&o| o != k).collect();
                whole(&others)
            })
            .collect();
        let pool = if free.is_empty() { rest.clone() } else { free };
        let key = |k: &usize| match at {
            Some(p) => center(*k).distance(p),
            None => center(*k).x,
        };
        let Some(next) = pool.iter().copied().min_by(|a, b| key(a).total_cmp(&key(b))) else { break };
        rest.retain(|&k| k != next);
        out.push(next);
        at = Some(center(next));
    }
    out
}

/// Sews each branch's objects in the order with the fewest colour changes and the
/// shortest travel, starting where the design is before it, keeping objects that
/// overlap in paint order.
fn order_branches(objects: &mut [EmbObject], nodes: &mut [NodeId]) {
    let mut i = 0;
    while i < objects.len() {
        let Some(b) = objects[i].branch else {
            i += 1;
            continue;
        };
        let end = (i..objects.len()).find(|&k| objects[k].branch != Some(b)).unwrap_or(objects.len());
        // a shape's pieces come in their order already (see [`piece_branches`])
        if b & PIECE_BRANCH != 0 {
            i = end;
            continue;
        }
        let items: Vec<sf_core::sequence::Item> =
            objects[i..end].iter().map(|o| sf_core::sequence::Item { color: o.color, area: object_polygons(o), bbox: o.bbox() }).collect();
        let start = i.checked_sub(1).map(|k| objects[k].bbox().center());
        let order = sf_core::sequence::sequence(&items, start);
        let objs: Vec<EmbObject> = order.iter().map(|&k| objects[i + k].clone()).collect();
        let ids: Vec<NodeId> = order.iter().map(|&k| nodes[i + k]).collect();
        objects[i..end].clone_from_slice(&objs);
        nodes[i..end].copy_from_slice(&ids);
        i = end;
    }
}

/// Fills sewn before a shape they touch grow `overlap` mm in under it (from the first sew-out: a
/// sliver of fabric showed between a truck's body and its bumper, the knit pulled in where they
/// only met edge to edge). Only where the later shape covers the growth, so it stays hidden; not
/// into a shape lying over it already, nor into the same node's own pieces (a letter's strokes),
/// nor for appliqué, whose lines are placed.
fn underlap(objects: &mut [EmbObject], nodes: &[NodeId], overlap: f64) {
    if !(overlap.is_finite() && overlap > 0.0) {
        return;
    }
    let overlap = overlap.min(3.0);
    let areas: Vec<Option<(Vec<Polygon>, BBox)>> = objects
        .iter()
        .map(|o| {
            let covers =
                o.visible && matches!(o.kind, ObjectKind::Fill(_) | ObjectKind::Satin(_) | ObjectKind::Lettering(_) | ObjectKind::Applique(_));
            covers
                .then(|| (object_polygons(o).into_iter().filter(|p| p.outer.len() >= 3).collect::<Vec<_>>(), o.bbox()))
                .filter(|(p, b)| !p.is_empty() && !b.is_empty())
        })
        .collect();
    for i in 0..objects.len() {
        let Some(ObjectKind::Fill(f)) = objects.get(i).map(|o| &o.kind) else { continue };
        if !objects[i].visible {
            continue;
        }
        let Some(Some((mine, bb))) = areas.get(i) else { continue };
        let reach = bb.expanded(overlap + PIECE_TOUCH);
        // what is sewn after it, touching it, of another node; not what lies over it already
        // (a bar across a branch's squares, a shape tucked under by hand)
        let deep = poly::offset(mine, -0.25);
        let later: Vec<Polygon> = (i + 1..objects.len())
            .filter(|&j| nodes.get(j) != nodes.get(i))
            .filter_map(|j| areas.get(j).and_then(Option::as_ref))
            .filter(|(_, b)| b.intersects(&reach))
            .filter(|(p, _)| poly::intersection(&deep, p).iter().map(Polygon::area).sum::<f64>() < 0.01)
            .flat_map(|(p, _)| p.iter().cloned())
            .collect();
        if later.is_empty() {
            continue;
        }
        let grown = poly::offset(mine, overlap);
        let under = poly::intersection(&grown, &poly::union(&later));
        let added: f64 = under.iter().map(Polygon::area).sum::<f64>();
        if added < 0.05 {
            continue;
        }
        let region = poly::union2(mine, &under);
        // a shape that became several pieces keeps its own (it touched others only at points)
        let [one] = region.as_slice() else { continue };
        let f = f.clone();
        if let Some(o) = objects.get_mut(i) {
            o.kind = ObjectKind::Fill(sf_core::model::FillObject { shape: Shape::from_polygon(one), ..f });
        }
    }
}

/// Fill pieces smaller than this (mm²) left over by a cut are dropped.
const MIN_PIECE: f64 = 1.0;

/// What objects cover, less `overlap` round the edge: the part the fills sewn before
/// them leave out when they remove the stitching below them.
/// Each appliqué with partial cover learns the outlines of what is sewn after it.
fn cover_partial_applique(objects: &mut [EmbObject]) {
    for i in 0..objects.len() {
        let partial = matches!(objects.get(i).map(|o| &o.kind), Some(ObjectKind::Applique(a)) if a.params.partial);
        if !partial {
            continue;
        }
        let later = cover_of(objects.get(i + 1..).unwrap_or_default(), 0.0);
        if let Some(ObjectKind::Applique(a)) = objects.get_mut(i).map(|o| &mut o.kind) {
            a.covered = later.into_iter().map(|p| p.outer).collect();
        }
    }
}

fn cover_of(objects: &[EmbObject], overlap: f64) -> Vec<Polygon> {
    let polys: Vec<Polygon> = objects.iter().flat_map(object_polygons).filter(|p| p.outer.len() >= 3).collect();
    if polys.is_empty() {
        return Vec::new();
    }
    poly::offset(&poly::union(&polys), -overlap)
}

/// Removes underlying stitching: every fill sewn before a cutting object loses the
/// part the cutter covers. A fill cut into pieces becomes one object per piece (the
/// extra ones with ids of their own); one covered completely makes no stitches.
fn cut_underlying(objects: &mut Vec<EmbObject>, nodes: &mut Vec<NodeId>, mut cuts: Vec<(usize, Vec<Polygon>)>) {
    for c in 0..cuts.len() {
        let (end, cover) = (cuts[c].0, std::mem::take(&mut cuts[c].1));
        let Some(bb) = cover.iter().map(Polygon::bbox).reduce(BBox::union) else { continue };
        let (mut i, mut end) = (0, end);
        while i < end {
            let ObjectKind::Fill(f) = &objects[i].kind else {
                i += 1;
                continue;
            };
            let region = f.shape.polygon(FLATTEN_TOLERANCE);
            if !region.bbox().intersects(&bb) {
                i += 1;
                continue;
            }
            let pieces: Vec<Polygon> =
                poly::difference(std::slice::from_ref(&region), &cover).into_iter().filter(|p| p.area() >= MIN_PIECE).collect();
            if region.area() - pieces.iter().map(Polygon::area).sum::<f64>() < 1e-3 {
                i += 1;
                continue;
            }
            let (obj, node) = (objects[i].clone(), nodes[i]);
            let made: Vec<EmbObject> = if pieces.is_empty() {
                vec![EmbObject { visible: false, ..obj }]
            } else {
                pieces
                    .iter()
                    .enumerate()
                    .map(|(k, p)| {
                        let mut o = obj.clone();
                        if let ObjectKind::Fill(f) = &mut o.kind {
                            f.shape = Shape::from_polygon(p);
                        }
                        o.id = obj.id.wrapping_add((k as u64) << 48);
                        o
                    })
                    .collect()
            };
            let added = made.len() - 1;
            objects.splice(i..=i, made);
            nodes.splice(i..=i, std::iter::repeat_n(node, added + 1));
            for later in cuts.iter_mut().skip(c + 1) {
                later.0 += added;
            }
            end += added;
            i += added + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::attr::StitchType;
    use sf_core::pattern::Cmd;
    use vectorcraft_color::{Color, Paint};
    use vectorcraft_doc::Appearance;
    use vectorcraft_geom::{Rect, shapes};

    fn doc_with(e: Embroidery, rect: Rect, color: Color) -> (Document, NodeId) {
        let mut d = Document::new(612.0, 792.0);
        let layer = d.layers[0].id;
        let id = d.alloc_id();
        let mut n = Node::path(id, shapes::rectangle(rect), Appearance::basic(Paint::solid(color), Paint::None, 0.0));
        n.embroidery = Some(Box::new(e));
        d.insert(Some(layer), usize::MAX, n).unwrap();
        (d, id)
    }

    #[test]
    fn filled_rectangle_becomes_stitches_in_its_colour() {
        // 72 × 36 pt = 25.4 × 12.7 mm
        let (d, id) = doc_with(Embroidery::fill(), Rect::new(0.0, 0.0, 72.0, 36.0), Color::rgb8(200, 20, 30));
        let mut p = Planner::new();
        let plan = p.plan(&d);
        assert_eq!(plan.nodes, vec![id]);
        let pat = plan.pattern();
        assert!(pat.stitch_count() > 300, "{}", pat.stitch_count());
        assert_eq!(pat.threads[0].color, Rgb::new(200, 20, 30));
        let b = pat.bounds();
        assert!((b.width() - 25.4).abs() < 1.0 && (b.height() - 12.7).abs() < 1.0, "{b:?}");
        assert_eq!(plan.node_at_record(pat.stitches.len() / 2), Some(id));
        // the same document gives the same (cached) plan
        assert!(Arc::ptr_eq(&plan, &p.plan(&d)));
    }

    /// A letter in satin is cut into its strokes; they are sewn as one path, the thread
    /// carried under the strokes still to come, not trimmed between them.
    #[test]
    fn a_letters_strokes_are_sewn_without_trims_between_them() {
        let red = Color::rgb8(200, 20, 30);
        let (bar, _) = doc_with(Embroidery::satin(), Rect::new(0.0, 0.0, 30.0, 8.0), red);
        let trims = |d: &Document| Planner::new().plan(d).pattern().count(Cmd::Trim);
        let e: Vec<kurbo::Point> = [(0, 0), (30, 0), (30, 8), (8, 8), (8, 21), (24, 21), (24, 29), (8, 29), (8, 42), (30, 42), (30, 50), (0, 50)]
            .iter()
            .map(|&(x, y)| kurbo::Point::new(f64::from(x), f64::from(y)))
            .collect();
        let mut d = Document::new(612.0, 792.0);
        let layer = d.layers[0].id;
        let id = d.alloc_id();
        let mut n = Node::path(
            id,
            vectorcraft_geom::path::PathData::single(vectorcraft_geom::path::SubPath::polyline(&e, true)),
            Appearance::basic(Paint::solid(red), Paint::None, 0.0),
        );
        n.embroidery = Some(Box::new(Embroidery::satin()));
        d.insert(Some(layer), usize::MAX, n).unwrap();
        let plan = Planner::new().plan(&d);
        let objects = &plan.design.objects;
        assert!(objects.len() >= 4, "the E is cut into its strokes: {}", objects.len());
        assert!(objects.iter().all(|o| o.branch.is_some() && o.branch == objects[0].branch));
        assert_eq!(trims(&d), trims(&bar), "no more trims than a single bar");
    }

    /// From the first sew-out: a body and the bumper sewn after it met edge to edge and fabric
    /// showed between them. The body now reaches in under the bumper (0.5 mm, and 1 mm when set so);
    /// the bumper, sewn on top, stays as drawn; with no overlap set, both stay as drawn.
    #[test]
    fn a_fill_reaches_in_under_a_shape_sewn_after_it_that_it_touches() {
        let mut d = Document::new(612.0, 792.0);
        let layer = d.layers[0].id;
        for (rect, color) in
            [(Rect::new(0.0, 0.0, 100.0, 60.0), Color::rgb8(180, 0, 27)), (Rect::new(0.0, 60.0, 100.0, 80.0), Color::rgb8(196, 201, 206))]
        {
            let id = d.alloc_id();
            let mut n = Node::path(id, shapes::rectangle(rect), Appearance::basic(Paint::solid(color), Paint::None, 0.0));
            n.embroidery = Some(Box::new(Embroidery::fill()));
            d.insert(Some(layer), usize::MAX, n).unwrap();
        }
        let bottom = |d: &Document, i: usize| {
            let plan = Planner::new().plan(d);
            plan.design.objects[i].bbox().max.y
        };
        let edge = 60.0 * MM_PER_PT;
        assert!((bottom(&d, 0) - (edge + 0.5)).abs() < 0.05, "the body reaches under the bumper: {}", bottom(&d, 0));
        assert!((bottom(&d, 1) - 80.0 * MM_PER_PT).abs() < 0.05, "the bumper stays as drawn");
        d.embroidery.get_or_insert_with(Box::default).settings.overlap_touching = 1.0;
        assert!((bottom(&d, 0) - (edge + 1.0)).abs() < 0.05, "{}", bottom(&d, 0));
        d.embroidery.get_or_insert_with(Box::default).settings.overlap_touching = 0.0;
        assert!((bottom(&d, 0) - edge).abs() < 0.05, "no overlap: as drawn");
    }

    #[test]
    fn every_stitch_type_converts() {
        let types =
            [Embroidery::run(), Embroidery::satin(), Embroidery::fill(), Embroidery::satin_fill(), Embroidery::applique(), Embroidery::manual()];
        for e in types {
            let label = e.stitch.id();
            let (d, _) = doc_with(e, Rect::new(0.0, 0.0, 60.0, 20.0), Color::rgb8(10, 120, 40));
            let plan = Planner::new().plan(&d);
            assert!(plan.pattern().stitch_count() > 3, "{label}: {}", plan.pattern().stitch_count());
            assert_eq!(plan.pattern().stitches.last().map(|s| s.cmd), Some(Cmd::End));
        }
        let _ = StitchType::Manual;
    }

    #[test]
    fn units_convert() {
        let p = pt(mm(kurbo::Point::new(72.0, 36.0)));
        assert!((p.x - 72.0).abs() < 1e-9 && (p.y - 36.0).abs() < 1e-9);
        assert!((mm(kurbo::Point::new(72.0, 0.0)).x - 25.4).abs() < 1e-9);
    }
}
