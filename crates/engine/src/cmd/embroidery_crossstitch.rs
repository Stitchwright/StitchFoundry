//! Cross-stitch designer (CH-03): a pattern of X stitches drawn cell by cell on a grid, the way
//! counted cross stitch is charted. A pattern is a group holding its grid (thin lines, not sewn)
//! and one object per thread colour, the union of that colour's cells, sewn as a cross fill on
//! the pattern's own grid, so every X lands in its cell. Cells are painted and erased with the
//! Cross Stitch tool (or `embroidery.crossStitch.paint`), and a picture can become a pattern
//! (`embroidery.crossStitch.fromImage`). Public description of the feature:
//! https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/CrossStitch/PART_CrossStitch/PART_CrossStitch-1.htm

use std::sync::Arc;

use kurbo::BezPath;
use serde_json::{Value, json};
use sf_core::Rgb;
use sf_core::attr::{Embroidery, StitchType};
use sf_core::model::{CrossStyle, FillKind, FillParams, FillUnderlay, RunKind, RunParams};
use sf_core::threads::Thread;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, Node, NodeId, ObjectAttributes};
use vectorcraft_embroidery::{MM_PER_PT, PT_PER_MM};
use vectorcraft_geom::PathData;

use super::embroidery_photo::{Picture, chart, main_colors, majority, quantise, sample};
use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// The data key marking a pattern's group: `"cols rows style"`.
pub const PATTERN_KEY: &str = "crossStitch";
/// The data key marking a pattern's grid lines.
const GRID_KEY: &str = "crossStitchGrid";
/// The data key marking one colour's object: its colour `#rrggbb`.
const COLOR_KEY: &str = "crossStitchColor";
/// The data key holding one colour's cells: runs `row,col,count` apart by spaces.
const CELLS_KEY: &str = "crossStitchCells";
/// The data key marking one colour's backstitch lines: its colour `#rrggbb`.
const BACK_KEY: &str = "crossStitchBack";
/// The data key holding one colour's backstitch lines: `col,row,col,row` from corner to corner
/// of the grid, apart by spaces.
const LINES_KEY: &str = "crossStitchLines";
/// Most backstitch lines in one pattern.
const MAX_LINES: usize = 20_000;
/// Most cells along a side.
const MAX_SIDE: u32 = 300;
/// Cell sizes (mm): the X stitches' side.
const CELL_MM: (f64, f64) = (1.0, 10.0);
/// Most colours in one pattern.
const MAX_COLORS: usize = 64;
/// Most points one paint stroke gives.
const MAX_STROKE: usize = 20_000;
/// The grid lines' colour.
const GRID_COLOR: [u8; 3] = [0xb4, 0xb4, 0xb4];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.crossStitch.new",
            "New Cross-Stitch Pattern…",
            ["Embroidery", "Cross Stitch"],
            None,
            "{cols?: (40; 1 to 300), rows?: (40; 1 to 300), cell?: mm (2.5; 1 to 10, the side of one X), count?: fabric count (3 to 25 stitches an inch, instead of cell: 14 is 14-count Aida, 1.81 mm), style?: full | half | upright | double (full), at?: [x, y] points (the grid's top left; default: centred on the page)} an empty cross-stitch grid, ready to paint with the Cross Stitch tool; it is selected → {id, cols, rows, cell}",
            has_doc,
            new_pattern
        ),
        cmd!(
            "embroidery.crossStitch.paint",
            "Paint Cross Stitches",
            [],
            None,
            "{id?: the pattern (or one of its objects; default: the selected pattern, else the one under the first point), at?: [[x, y], …] points along a stroke, cells?: [[col, row], …], color?: \"#rrggbb\" (needed unless erasing), erase?: bool, size?: cells (1; 1 to 9, a square brush)} paint (or erase) the cells the stroke passes through, one colour a cell → {id, cells, colors}",
            has_doc,
            paint
        ),
        cmd!(
            "embroidery.crossStitch.fromImage",
            "Cross Stitch from Image…",
            ["Embroidery", "Cross Stitch"],
            None,
            "{id?: an image (the selected one), cols?: (60; 4 to 300), colors?: (8; 2 to 32), cell?: mm (1 to 10; default the image's width over `cols`), count?: fabric count instead of cell (3 to 25), style?: full | half | upright | double, background?: bool (false: the colour round the edges is left unsewn), chart?: a thread chart's name} the image as a cross-stitch pattern over it: its main colours matched to threads, a colour a cell, specks cleaned → {id, cols, rows, cell, colors}",
            has_doc,
            from_image
        ),
        cmd!(
            "embroidery.crossStitch.set",
            "Cross-Stitch Pattern Settings",
            [],
            None,
            "{id?: the pattern (the selected one), cell?: mm (1 to 10; the grid grows or shrinks from its top left), count?: fabric count instead of cell (3 to 25), style?: full | half | upright | double} change a pattern's cell size or how its cells are sewn → {id, cell, style}",
            has_doc,
            set_pattern
        ),
        cmd!(
            "embroidery.crossStitch.fill",
            "Fill Cross Stitches",
            [],
            None,
            "{id?: the pattern (default: the selected one, else the one under `at`), at?: [x, y] points | cell?: [col, row], color?: \"#rrggbb\" (needed unless erasing), erase?: bool} fill the patch of touching squares the same as the one clicked (all empty, or all one colour, side by side) with `color`, or clear it → {id, cells}",
            has_doc,
            fill
        ),
        cmd!(
            "embroidery.crossStitch.replace",
            "Replace Cross-Stitch Colour",
            [],
            None,
            "{id?: the pattern (the selected one), from: \"#rrggbb\" (a colour of the pattern), to?: \"#rrggbb\" (needed unless erasing), erase?: bool} every square of one colour in another colour, or cleared → {id, cells}",
            has_doc,
            replace
        ),
        cmd!(
            "embroidery.crossStitch.backstitch",
            "Backstitch Lines",
            [],
            None,
            "{id?: the pattern (default: the selected one, else the one under `at`), at?: [[x, y], [x, y]] points (a line's two ends, each taken to the nearest grid corner) | from?: [col, row], to?: [col, row] (grid corners, 0 to cols and rows), color?: \"#rrggbb\" (needed unless erasing), erase?: bool (with `at` [[x, y]]: the lines passing within half a square of the point go)} a backstitch line from corner to corner of the grid, straight or slanted, sewn over the crosses as a running stitch in its thread, or lines taken away → {id, lines}",
            has_doc,
            backstitch
        ),
        cmd!(
            query "embroidery.crossStitch.info",
            "Cross-Stitch Pattern Info",
            [],
            None,
            "{id?: the pattern (the selected one)} → {id, cols, rows, cell (mm), count (stitches an inch), style, size: [w, h] mm, colors: [{color, name, code, cells}], backstitch: [{color, name, code, lines}]} (colours in sewing order)",
            has_doc,
            info
        ),
    ]
}

/// A pattern read from the document: its grid and each colour's cells.
pub(super) struct Pattern {
    pub(super) group: NodeId,
    pub(super) cols: u32,
    pub(super) rows: u32,
    pub(super) style: CrossStyle,
    /// The grid's top left (points).
    pub(super) origin: kurbo::Point,
    /// A cell's side (points).
    pub(super) cell: f64,
    /// The colour objects in sewing order, each with its colour and thread.
    pub(super) colors: Vec<(Option<NodeId>, Rgb, Option<Thread>)>,
    /// Each cell's colour (index into `colors`), row by row.
    pub(super) cells: Vec<Option<usize>>,
    /// The colours whose cells changed since the pattern was read (their objects are rebuilt).
    dirty: Vec<bool>,
    /// Backstitch: each colour's lines from grid corner to grid corner (`[col, row, col, row]`,
    /// the smaller corner first), sewn after the crosses, over them.
    pub(super) backs: Vec<Back>,
}

/// One colour's backstitch: its object (once written), colour, thread and lines.
pub(super) type Back = (Option<NodeId>, Rgb, Option<Thread>, Vec<[u32; 4]>);

pub(super) fn style_name(s: CrossStyle) -> &'static str {
    match s {
        CrossStyle::Full => "full",
        CrossStyle::Half => "half",
        CrossStyle::Upright => "upright",
        CrossStyle::Double => "double",
    }
}

fn parse_style(p: &Value, cmd: &str, default: CrossStyle) -> Result<CrossStyle> {
    match p.get("style") {
        None | Some(Value::Null) => Ok(default),
        Some(v) => CrossStyle::ALL
            .into_iter()
            .find(|s| v.as_str() == Some(style_name(*s)))
            .ok_or_else(|| bad(cmd, "`style` is full, half, upright or double")),
    }
}

fn side(p: &Value, key: &str, default: u32, lo: u32, cmd: &str) -> Result<u32> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| (lo..=MAX_SIDE).contains(v))
            .ok_or_else(|| bad(cmd, format!("`{key}` is {lo} to {MAX_SIDE}"))),
    }
}

/// Fabric counts (stitches an inch) a pattern may be sized by: 25.4 / count mm a cell.
const COUNT: (f64, f64) = (3.0, 25.0);

/// The cell size asked for (mm): `cell`, or a fabric `count` (14-count Aida: 25.4 / 14 mm).
fn cell_mm(p: &Value, cmd: &str) -> Result<Option<f64>> {
    if let Some(v) = p.get("count").filter(|v| !v.is_null()) {
        let count = v
            .as_f64()
            .filter(|c| (COUNT.0..=COUNT.1).contains(c))
            .ok_or_else(|| bad(cmd, format!("`count` is {} to {} stitches an inch", COUNT.0, COUNT.1)))?;
        return Ok(Some(25.4 / count));
    }
    match p.get("cell") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|v| (CELL_MM.0..=CELL_MM.1).contains(v))
            .map(Some)
            .ok_or_else(|| bad(cmd, format!("`cell` is {} to {} mm", CELL_MM.0, CELL_MM.1))),
    }
}

fn data<'a>(n: &'a Node, key: &str) -> Option<&'a str> {
    n.extras().attrs.as_deref()?.data.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn with_data(n: &mut Node, key: &str, value: String) {
    let mut extras = n.extras_mut();
    let attrs = extras.attrs.get_or_insert_with(|| Box::new(ObjectAttributes::default()));
    attrs.data.retain(|(k, _)| k != key);
    attrs.data.push((key.to_string(), value));
}

/// The pattern `id` belongs to (it, or the group holding it).
fn pattern_of(d: &Document, id: NodeId) -> Option<NodeId> {
    let mut at = Some(id);
    for _ in 0..64 {
        let here = at?;
        if d.node(here).is_some_and(|n| data(n, PATTERN_KEY).is_some()) {
            return Some(here);
        }
        at = d.parent_of(here);
    }
    None
}

/// The pattern named by `id`, else the selected one.
pub(super) fn chosen(s: &Session, p: &Value) -> Option<NodeId> {
    let st = s.doc().ok()?;
    match p.get("id").and_then(Value::as_u64) {
        Some(id) => pattern_of(&st.doc, NodeId(id)),
        None => st.selection.objects.iter().find_map(|id| pattern_of(&st.doc, *id)),
    }
}

/// Every pattern in the document.
fn all_patterns(d: &Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<Arc<Node>> = d.children(None).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        if data(&n, PATTERN_KEY).is_some() {
            out.push(n.id);
        } else if let Some(c) = n.children() {
            stack.extend(c.iter().cloned());
        }
    }
    out
}

/// The colour of a `#rrggbb` value.
fn rgb_of(v: &str) -> Option<Rgb> {
    Rgb::from_hex(v)
}

impl Pattern {
    /// Reads pattern `group`: its grid's place and size, and which cells each colour covers.
    pub(super) fn read(d: &Document, group: NodeId) -> Option<Pattern> {
        let g = d.node(group)?;
        let mut words = data(g, PATTERN_KEY)?.split_whitespace();
        let cols: u32 = words.next()?.parse().ok().filter(|c| (1..=MAX_SIDE).contains(c))?;
        let rows: u32 = words.next()?.parse().ok().filter(|r| (1..=MAX_SIDE).contains(r))?;
        let style = words.next().and_then(|w| CrossStyle::ALL.into_iter().find(|s| style_name(*s) == w)).unwrap_or(CrossStyle::Full);
        let children = g.children()?;
        // the grid's box (mm), from its lines
        let grid = children.iter().find(|c| data(c, GRID_KEY).is_some())?;
        let pts: Vec<sf_core::Point> = vectorcraft_embroidery::outlines_mm(grid).into_iter().flatten().collect();
        let (x0, y0) = pts.iter().fold((f64::INFINITY, f64::INFINITY), |(x, y), p| (x.min(p.x), y.min(p.y)));
        let x1 = pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let cell_mm = (x1 - x0) / f64::from(cols);
        if !(x0.is_finite() && y0.is_finite() && cell_mm.is_finite() && cell_mm > 0.01) {
            return None;
        }
        let mut pattern = Pattern {
            group,
            cols,
            rows,
            style,
            origin: kurbo::Point::new(x0 * PT_PER_MM, y0 * PT_PER_MM),
            cell: cell_mm * PT_PER_MM,
            colors: Vec::new(),
            cells: vec![None; cols as usize * rows as usize],
            dirty: Vec::new(),
            backs: Vec::new(),
        };
        for c in children.iter() {
            if let Some(color) = data(c, BACK_KEY).and_then(rgb_of) {
                let mut lines = Vec::new();
                for l in data(c, LINES_KEY).unwrap_or_default().split_whitespace() {
                    let v: Vec<u32> = l.split(',').filter_map(|v| v.parse().ok()).collect();
                    if let [c0, r0, c1, r1] = v[..]
                        && c0.max(c1) <= cols
                        && r0.max(r1) <= rows
                        && lines.len() < MAX_LINES
                    {
                        lines.push(line(c0, r0, c1, r1));
                    }
                }
                let thread = c.embroidery.as_deref().and_then(|e| e.thread.clone());
                pattern.backs.push((Some(c.id), color, thread, lines));
                continue;
            }
            let Some(color) = data(c, COLOR_KEY).and_then(rgb_of) else { continue };
            if pattern.colors.len() >= MAX_COLORS {
                break;
            }
            let k = pattern.colors.len();
            let thread = c.embroidery.as_deref().and_then(|e| e.thread.clone());
            pattern.colors.push((Some(c.id), color, thread));
            pattern.dirty.push(false);
            if let Some(runs) = data(c, CELLS_KEY) {
                for run in runs.split_whitespace() {
                    let mut n = run.split(',').map(|v| v.parse::<u32>().ok());
                    let (Some(Some(r)), Some(Some(col)), Some(Some(len))) = (n.next(), n.next(), n.next()) else { continue };
                    for cc in col..col.saturating_add(len).min(cols) {
                        if r < rows
                            && let Some(v) = pattern.cells.get_mut((r * cols + cc) as usize)
                        {
                            *v = Some(k);
                        }
                    }
                }
                continue;
            }
            // no cell list (an older or hand-edited object): the cells its shape covers
            if let Some(d) = pattern.dirty.get_mut(k) {
                *d = true;
            }
            for region in vectorcraft_embroidery::regions_mm(c) {
                let bb = region.bbox();
                let c0 = (((bb.min.x - x0) / cell_mm).floor().max(0.0)) as u32;
                let r0 = (((bb.min.y - y0) / cell_mm).floor().max(0.0)) as u32;
                let c1 = (((bb.max.x - x0) / cell_mm).ceil().clamp(0.0, f64::from(cols))) as u32;
                let r1 = (((bb.max.y - y0) / cell_mm).ceil().clamp(0.0, f64::from(rows))) as u32;
                for r in r0..r1 {
                    for col in c0..c1 {
                        let centre = sf_core::Point::new(x0 + (f64::from(col) + 0.5) * cell_mm, y0 + (f64::from(r) + 0.5) * cell_mm);
                        if region.contains(centre)
                            && let Some(v) = pattern.cells.get_mut((r * cols + col) as usize)
                        {
                            *v = Some(k);
                        }
                    }
                }
            }
        }
        Some(pattern)
    }

    pub(super) fn cell_mm(&self) -> f64 {
        self.cell * MM_PER_PT
    }

    /// The cell holding document point `q`, if it is on the grid.
    fn cell_at(&self, q: kurbo::Point) -> Option<(u32, u32)> {
        let c = ((q.x - self.origin.x) / self.cell).floor();
        let r = ((q.y - self.origin.y) / self.cell).floor();
        (c.is_finite() && r.is_finite() && c >= 0.0 && r >= 0.0 && c < f64::from(self.cols) && r < f64::from(self.rows))
            .then_some((c as u32, r as u32))
    }

    fn contains(&self, q: kurbo::Point) -> bool {
        self.cell_at(q).is_some()
    }

    fn set(&mut self, c: u32, r: u32, v: Option<usize>) -> bool {
        if c >= self.cols || r >= self.rows {
            return false;
        }
        match self.cells.get_mut((r * self.cols + c) as usize) {
            Some(x) if *x != v => {
                for k in [*x, v].into_iter().flatten() {
                    if let Some(d) = self.dirty.get_mut(k) {
                        *d = true;
                    }
                }
                *x = v;
                true
            }
            _ => false,
        }
    }

    /// The colour's index, added when new.
    fn color_index(&mut self, color: Rgb, thread: Option<Thread>) -> Result<usize> {
        if let Some(k) = self.colors.iter().position(|(_, c, _)| *c == color) {
            return Ok(k);
        }
        if self.colors.len() >= MAX_COLORS {
            return Err(bad("embroidery.crossStitch.paint", format!("a pattern has at most {MAX_COLORS} colours")));
        }
        self.colors.push((None, color, thread));
        self.dirty.push(true);
        Ok(self.colors.len() - 1)
    }

    /// Cells of each colour, counted.
    pub(super) fn counts(&self) -> Vec<usize> {
        let mut n = vec![0usize; self.colors.len()];
        for k in self.cells.iter().flatten() {
            if let Some(c) = n.get_mut(*k) {
                *c += 1;
            }
        }
        n
    }

    /// Colour `k`'s runs of cells along each row: `(row, first col, count)`.
    fn runs(&self, k: usize) -> Vec<(u32, u32, u32)> {
        let mut out = Vec::new();
        for r in 0..self.rows {
            let mut c = 0;
            while c < self.cols {
                if self.cells.get((r * self.cols + c) as usize).copied().flatten() != Some(k) {
                    c += 1;
                    continue;
                }
                let start = c;
                while c < self.cols && self.cells.get((r * self.cols + c) as usize).copied().flatten() == Some(k) {
                    c += 1;
                }
                out.push((r, start, c - start));
            }
        }
        out
    }

    /// Colour `k`'s cells as a path (points): rectangles, a run of cells along a row each, runs
    /// alike in the rows below taken in.
    fn color_path(&self, runs: &[(u32, u32, u32)]) -> BezPath {
        let mut used = vec![false; runs.len()];
        let mut bp = BezPath::new();
        for i in 0..runs.len() {
            let Some(&(r, c, n)) = runs.get(i) else { continue };
            if used.get(i).copied().unwrap_or(true) {
                continue;
            }
            // as many rows below with the same run
            let mut bottom = r + 1;
            for (j, run) in runs.iter().enumerate().skip(i + 1) {
                if run.0 > bottom {
                    break;
                }
                if *run == (bottom, c, n) && !used.get(j).copied().unwrap_or(true) {
                    if let Some(u) = used.get_mut(j) {
                        *u = true;
                    }
                    bottom += 1;
                }
            }
            let (x0, x1) = (self.origin.x + f64::from(c) * self.cell, self.origin.x + f64::from(c + n) * self.cell);
            let (y0, y1) = (self.origin.y + f64::from(r) * self.cell, self.origin.y + f64::from(bottom) * self.cell);
            bp.move_to((x0, y0));
            bp.line_to((x1, y0));
            bp.line_to((x1, y1));
            bp.line_to((x0, y1));
            bp.close_path();
        }
        bp
    }

    /// The grid's lines (points): every cell's sides.
    fn grid_path(&self) -> BezPath {
        let mut bp = BezPath::new();
        let (w, h) = (f64::from(self.cols) * self.cell, f64::from(self.rows) * self.cell);
        for c in 0..=self.cols {
            let x = self.origin.x + f64::from(c) * self.cell;
            bp.move_to((x, self.origin.y));
            bp.line_to((x, self.origin.y + h));
        }
        for r in 0..=self.rows {
            let y = self.origin.y + f64::from(r) * self.cell;
            bp.move_to((self.origin.x, y));
            bp.line_to((self.origin.x + w, y));
        }
        bp
    }

    /// Colour `k`'s object: its cells, sewn as cross stitch on the pattern's grid in its thread.
    fn color_node(&self, k: usize) -> Option<Node> {
        let (_, color, thread) = self.colors.get(k)?;
        let runs = self.runs(k);
        if runs.is_empty() {
            return None;
        }
        let path = self.color_path(&runs);
        let paint = Paint::solid(Color::rgb8(color.r, color.g, color.b));
        let mut node =
            Node::path(self.colors.get(k)?.0.unwrap_or(NodeId(0)), PathData::from_bezpath(&path), Appearance::basic(paint, Paint::None, 0.0));
        let fill = FillParams {
            kind: FillKind::Cross,
            angle: 0.0,
            stitch_length: self.cell_mm(),
            cross: self.style,
            // the grid's corner: the X stitches line up with the cells
            center: Some(sf_core::Point::new(self.origin.x, self.origin.y)),
            underlay: FillUnderlay { edge_run: false, tatami: false, ..FillUnderlay::default() },
            ..FillParams::default()
        };
        let mut e = Embroidery::new(StitchType::Fill(fill));
        e.auto_direction = false;
        e.thread = Some(thread.clone().unwrap_or_else(|| Thread::new(*color, "")));
        node.embroidery = Some(Box::new(e));
        node.name = Some(format!("Cross stitch {}", color.to_hex()));
        with_data(&mut node, COLOR_KEY, color.to_hex());
        let list: Vec<String> = runs.iter().map(|(r, c, n)| format!("{r},{c},{n}")).collect();
        with_data(&mut node, CELLS_KEY, list.join(" "));
        Some(node)
    }

    /// Grid corner `(col, row)` in document points.
    pub(super) fn corner(&self, c: u32, r: u32) -> kurbo::Point {
        kurbo::Point::new(self.origin.x + f64::from(c) * self.cell, self.origin.y + f64::from(r) * self.cell)
    }

    /// The grid corner nearest document point `q`, if it is on (or by) the grid.
    fn corner_at(&self, q: kurbo::Point) -> Option<(u32, u32)> {
        let c = ((q.x - self.origin.x) / self.cell).round();
        let r = ((q.y - self.origin.y) / self.cell).round();
        (c.is_finite() && r.is_finite() && c >= 0.0 && r >= 0.0 && c <= f64::from(self.cols) && r <= f64::from(self.rows))
            .then_some((c as u32, r as u32))
    }

    /// Backstitch colour `k`'s object: its lines joined where they meet, sewn as a running stitch
    /// a square long (its needle points on the grid's corners) in its thread.
    fn back_node(&self, k: usize) -> Option<Node> {
        let (_, color, thread, lines) = self.backs.get(k)?;
        if lines.is_empty() {
            return None;
        }
        let mut bp = BezPath::new();
        for chain in chains(lines) {
            for (i, (c, r)) in chain.iter().enumerate() {
                let p = self.corner(*c, *r);
                if i == 0 {
                    bp.move_to(p);
                } else {
                    bp.line_to(p);
                }
            }
        }
        let paint = Paint::solid(Color::rgb8(color.r, color.g, color.b));
        let mut node = Node::path(NodeId(0), PathData::from_bezpath(&bp), Appearance::basic(Paint::None, paint, 1.0));
        // a hair over a square, so a line of whole squares splits into exactly that many stitches
        let run = RunParams { kind: RunKind::Single, stitch_length: self.cell_mm() * 1.001, ..RunParams::default() };
        let mut e = Embroidery::new(StitchType::Run(run));
        e.auto_direction = false;
        e.thread = Some(thread.clone().unwrap_or_else(|| Thread::new(*color, "")));
        node.embroidery = Some(Box::new(e));
        node.name = Some(format!("Backstitch {}", color.to_hex()));
        with_data(&mut node, BACK_KEY, color.to_hex());
        let list: Vec<String> = lines.iter().map(|l| format!("{},{},{},{}", l[0], l[1], l[2], l[3])).collect();
        with_data(&mut node, LINES_KEY, list.join(" "));
        Some(node)
    }

    fn grid_node(&self) -> Node {
        let stroke = Paint::solid(Color::rgb8(GRID_COLOR[0], GRID_COLOR[1], GRID_COLOR[2]));
        let mut n = Node::path(NodeId(0), PathData::from_bezpath(&self.grid_path()), Appearance::basic(Paint::None, stroke, 0.25));
        n.name = Some("Cross stitch grid".into());
        with_data(&mut n, GRID_KEY, "1".into());
        n
    }

    fn header(&self) -> String {
        format!("{} {} {}", self.cols, self.rows, style_name(self.style))
    }

    /// Writes the pattern back: the group's grid and the colour objects whose cells changed
    /// rebuilt (all of them when `all`), colours with no cells left taken out. The group keeps
    /// its id, name and place.
    fn write(&self, d: &mut Document, all: bool) -> Result<()> {
        let mut kids: Vec<Arc<Node>> = Vec::new();
        let old: Vec<Arc<Node>> = d.node(self.group).and_then(Node::children).cloned().unwrap_or_default();
        // the grid first (it keeps its id), then anything else the user put in the group
        let mut grid = self.grid_node();
        grid.id = old.iter().find(|c| data(c, GRID_KEY).is_some()).map_or_else(|| d.alloc_id(), |c| c.id);
        kids.push(Arc::new(grid));
        for c in &old {
            if data(c, GRID_KEY).is_none() && data(c, COLOR_KEY).is_none() && data(c, BACK_KEY).is_none() {
                kids.push(c.clone());
            }
        }
        for k in 0..self.colors.len() {
            let before = self.colors.get(k).and_then(|c| c.0).and_then(|id| old.iter().find(|o| o.id == id));
            // unchanged colours stay as they are (and keep their stitches cached)
            if !all
                && !self.dirty.get(k).copied().unwrap_or(true)
                && let Some(o) = before
            {
                kids.push(o.clone());
                continue;
            }
            let Some(mut n) = self.color_node(k) else { continue };
            // a colour object keeps its own id, and what the user changed on it
            match before {
                Some(o) => {
                    n.id = o.id;
                    n.visible = o.visible;
                    n.locked = o.locked;
                    n.name = o.name.clone().or(n.name);
                }
                None => n.id = d.alloc_id(),
            }
            kids.push(Arc::new(n));
        }
        // backstitch last: sewn over the crosses
        for k in 0..self.backs.len() {
            let Some(mut n) = self.back_node(k) else { continue };
            let before = self.backs.get(k).and_then(|b| b.0).and_then(|id| old.iter().find(|o| o.id == id));
            match before {
                Some(o) => {
                    n.id = o.id;
                    n.visible = o.visible;
                    n.locked = o.locked;
                    n.name = o.name.clone().or(n.name);
                }
                None => n.id = d.alloc_id(),
            }
            kids.push(Arc::new(n));
        }
        let header = self.header();
        let g = d.node_mut(self.group).ok_or(EngineError::NoNode(self.group))?;
        with_data(g, PATTERN_KEY, header);
        if let Some(c) = g.children_mut() {
            *c = kids;
        }
        Ok(())
    }
}

/// A backstitch line from corner to corner, the smaller corner first (so a line drawn either way is
/// the same line).
fn line(c0: u32, r0: u32, c1: u32, r1: u32) -> [u32; 4] {
    if (r0, c0) <= (r1, c1) { [c0, r0, c1, r1] } else { [c1, r1, c0, r0] }
}

/// `lines` joined into chains of corners where they meet end to end, so each chain sews in one go.
fn chains(lines: &[[u32; 4]]) -> Vec<Vec<(u32, u32)>> {
    let mut left: Vec<[u32; 4]> = lines.to_vec();
    let mut out = Vec::new();
    while let Some(first) = left.pop() {
        let mut chain = vec![(first[0], first[1]), (first[2], first[3])];
        // grow the chain at its end while a line starts or ends there
        while let Some(&end) = chain.last() {
            let Some(i) = left.iter().position(|l| (l[0], l[1]) == end || (l[2], l[3]) == end) else { break };
            let l = left.swap_remove(i);
            chain.push(if (l[0], l[1]) == end { (l[2], l[3]) } else { (l[0], l[1]) });
        }
        out.push(chain);
    }
    out
}

/// The page's centre (points).
fn page_centre(d: &Document) -> kurbo::Point {
    d.artboards.first().map_or(kurbo::Point::new(300.0, 300.0), |a| a.rect.center())
}

fn new_group(
    d: &mut Document,
    sel: &mut vectorcraft_doc::Selection,
    pattern: &mut Pattern,
    name: &str,
    at: (Option<NodeId>, usize),
) -> Result<NodeId> {
    let mut g = Node::group(d.alloc_id(), Vec::new());
    g.name = Some(name.to_string());
    with_data(&mut g, PATTERN_KEY, pattern.header());
    let id = g.id;
    pattern.group = id;
    d.insert(at.0, at.1, g)?;
    pattern.write(d, true)?;
    sel.set([id]);
    Ok(id)
}

fn new_pattern(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.new";
    let cols = side(p, "cols", 40, 1, CMD)?;
    let rows = side(p, "rows", 40, 1, CMD)?;
    let cell = cell_mm(p, CMD)?.unwrap_or(2.5) * PT_PER_MM;
    let style = parse_style(p, CMD, CrossStyle::Full)?;
    let at = match p.get("at") {
        None | Some(Value::Null) => None,
        Some(v) => Some(point(v).ok_or_else(|| bad(CMD, "`at` is [x, y] in points"))?),
    };
    let parent = s.doc()?.target_parent()?;
    let id = s.edit("New Cross-Stitch Pattern", |d, sel| {
        let origin = at.unwrap_or_else(|| {
            let c = page_centre(d);
            kurbo::Point::new(c.x - f64::from(cols) * cell * 0.5, c.y - f64::from(rows) * cell * 0.5)
        });
        let mut pattern = Pattern {
            group: NodeId(0),
            cols,
            rows,
            style,
            origin,
            cell,
            colors: Vec::new(),
            cells: vec![None; cols as usize * rows as usize],
            dirty: Vec::new(),
            backs: Vec::new(),
        };
        new_group(d, sel, &mut pattern, "Cross Stitch", (parent, usize::MAX))
    })?;
    Ok(json!({ "id": id.0, "cols": cols, "rows": rows, "cell": cell * MM_PER_PT }))
}

fn point(v: &Value) -> Option<kurbo::Point> {
    let x = v.get(0)?.as_f64()?;
    let y = v.get(1)?.as_f64()?;
    (x.is_finite() && y.is_finite()).then(|| kurbo::Point::new(x, y))
}

fn paint(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.paint";
    let erase = bool_or(p, "erase", false);
    let size = match p.get("size") {
        None | Some(Value::Null) => 1,
        Some(v) => v.as_u64().filter(|v| (1..=9).contains(v)).ok_or_else(|| bad(CMD, "`size` is 1 to 9 cells"))? as i64,
    };
    let color = match str_param(p, "color") {
        Some(c) => Some(rgb_of(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?),
        None if erase => None,
        None => return Err(bad(CMD, "give the `color` to paint with")),
    };
    let stroke: Vec<kurbo::Point> = match p.get("at") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) if list.len() <= MAX_STROKE => {
            list.iter().map(|v| point(v).ok_or_else(|| bad(CMD, "`at` is a list of [x, y] points"))).collect::<Result<_>>()?
        }
        Some(_) => return Err(bad(CMD, format!("`at` is a list of at most {MAX_STROKE} [x, y] points"))),
    };
    let listed: Vec<(i64, i64)> = match p.get("cells") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) if list.len() <= MAX_STROKE => list
            .iter()
            .map(|v| {
                let c = v.get(0).and_then(Value::as_i64);
                let r = v.get(1).and_then(Value::as_i64);
                c.zip(r).ok_or_else(|| bad(CMD, "`cells` is a list of [col, row]"))
            })
            .collect::<Result<_>>()?,
        Some(_) => return Err(bad(CMD, format!("`cells` is a list of at most {MAX_STROKE} [col, row]"))),
    };
    if stroke.is_empty() && listed.is_empty() {
        return Err(bad(CMD, "give the cells to paint: `at` points or `cells`"));
    }
    let d = &s.doc()?.doc;
    let group = chosen(s, p)
        .or_else(|| stroke.first().and_then(|q| all_patterns(d).into_iter().find(|g| Pattern::read(d, *g).is_some_and(|pat| pat.contains(*q)))))
        .ok_or_else(|| bad(CMD, "paint on a cross-stitch pattern (Embroidery › Cross Stitch › New Cross-Stitch Pattern)"))?;
    let mut pattern = Pattern::read(d, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
    let thread = color.map(|c| nearest_thread(s, c));
    let value = match color {
        Some(c) => Some(pattern.color_index(c, thread)?),
        None => None,
    };
    // the cells under the stroke: its points, and between them every half cell; only those the
    // brush can reach the grid from, each once
    let (cols, rows) = (i64::from(pattern.cols), i64::from(pattern.rows));
    let near = |c: i64, r: i64| c > -size && r > -size && c < cols + size && r < rows + size;
    let mut seen = std::collections::HashSet::new();
    let mut hit: Vec<(i64, i64)> = listed.into_iter().filter(|(c, r)| near(*c, *r) && seen.insert((*c, *r))).collect();
    let mut prev: Option<kurbo::Point> = None;
    let mut budget = 2_000_000usize;
    for q in &stroke {
        let from = prev.unwrap_or(*q);
        let steps = ((from.distance(*q) / (pattern.cell * 0.5)).ceil() as usize).clamp(1, 1000).min(budget);
        budget -= steps;
        for k in 1..=steps {
            let at = from.lerp(*q, k as f64 / steps as f64);
            let c = ((at.x - pattern.origin.x) / pattern.cell).floor();
            let r = ((at.y - pattern.origin.y) / pattern.cell).floor();
            if c.is_finite() && r.is_finite() && c.abs() < 1e6 && r.abs() < 1e6 && near(c as i64, r as i64) && seen.insert((c as i64, r as i64)) {
                hit.push((c as i64, r as i64));
            }
        }
        prev = Some(*q);
    }
    let lo = -(size - 1) / 2;
    let mut changed = 0usize;
    for (c, r) in hit {
        for dy in lo..lo + size {
            for dx in lo..lo + size {
                let (cc, rr) = (c.saturating_add(dx), r.saturating_add(dy));
                if cc >= 0 && rr >= 0 && cc < i64::from(pattern.cols) && rr < i64::from(pattern.rows) && pattern.set(cc as u32, rr as u32, value) {
                    changed += 1;
                }
            }
        }
    }
    let label = if erase { "Erase Cross Stitches" } else { "Paint Cross Stitches" };
    if changed > 0 {
        s.edit(label, |d, _| pattern.write(d, false))?;
    }
    let colors = pattern.counts().iter().filter(|n| **n > 0).count();
    Ok(json!({ "id": group.0, "cells": changed, "colors": colors }))
}

/// A thread for `c`: the design's chart's thread of that colour, else one of its own.
fn nearest_thread(s: &Session, c: Rgb) -> Thread {
    match chart(s, &json!({}), "embroidery.crossStitch.paint") {
        Ok(chart) => chart.threads.iter().find(|t| t.color == c).cloned().unwrap_or_else(|| Thread::new(c, "")),
        Err(_) => Thread::new(c, ""),
    }
}

fn from_image(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.fromImage";
    let cols = side(p, "cols", 60, 4, CMD)?;
    let n = match p.get("colors") {
        None | Some(Value::Null) => 8,
        Some(v) => v.as_u64().filter(|v| (2..=32).contains(v)).ok_or_else(|| bad(CMD, "`colors` is 2 to 32"))? as usize,
    };
    let style = parse_style(p, CMD, CrossStyle::Full)?;
    let background = bool_or(p, "background", false);
    let pic = Picture::get(s, p, CMD)?;
    let b = pic.bounds();
    let cell = match cell_mm(p, CMD)? {
        Some(c) => c * PT_PER_MM,
        None => (b.width() / f64::from(cols)).clamp(CELL_MM.0 * PT_PER_MM, CELL_MM.1 * PT_PER_MM),
    };
    let cols = ((b.width() / cell).ceil() as u32).clamp(1, MAX_SIDE);
    let rows64 = (b.height() / cell).ceil();
    if !(rows64.is_finite() && rows64 >= 1.0 && rows64 <= f64::from(MAX_SIDE)) {
        return Err(bad(CMD, format!("the image would be more than {MAX_SIDE} cells tall: use larger cells or fewer columns")));
    }
    let rows = rows64 as u32;
    let chart = chart(s, p, CMD)?;
    // each cell the image's colour at its middle
    let inv = pic.to_doc.inverse();
    let origin = kurbo::Point::new(b.x0, b.y0);
    let grid = vectorcraft_trace::Raster::from_fn(cols, rows, |x, y| {
        let q = inv * kurbo::Point::new(origin.x + (f64::from(x) + 0.5) * cell, origin.y + (f64::from(y) + 0.5) * cell);
        if q.x >= 0.0 && q.y >= 0.0 && q.x < f64::from(pic.raster.width) && q.y < f64::from(pic.raster.height) {
            pic.raster.pixel(q.x as u32, q.y as u32)
        } else {
            [0, 0, 0, 0]
        }
    });
    let palette: Vec<Rgb> = main_colors(&sample(&grid, 40_000), n)
        .into_iter()
        .map(|(c, _)| chart.threads.get(chart.nearest(c)).map_or(c, |t| t.color))
        .fold(Vec::new(), |mut v, c| {
            if !v.contains(&c) {
                v.push(c);
            }
            v
        });
    let (w, h) = (cols as usize, rows as usize);
    let mut cells = quantise(&grid, &palette);
    majority(&mut cells, w, h, 1);
    // the background: the colour covering most of the edges, left unsewn unless asked for
    if !background {
        let mut edge = vec![0usize; palette.len()];
        let mut total = 0usize;
        for y in 0..h {
            for x in 0..w {
                if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
                    total += 1;
                    if let Some(Some(i)) = cells.get(y * w + x)
                        && let Some(e) = edge.get_mut(*i)
                    {
                        *e += 1;
                    }
                }
            }
        }
        if let Some((skip, _)) = edge.iter().enumerate().max_by_key(|(_, n)| **n).filter(|(_, n)| **n * 2 > total) {
            for c in cells.iter_mut() {
                if *c == Some(skip) {
                    *c = None;
                }
            }
        }
    }
    // the lightest colour sewn first
    let mut order: Vec<usize> = (0..palette.len()).collect();
    order.sort_by(|a, b| {
        let l = |i: &usize| palette.get(*i).map_or(0.0, |c| c.to_lab()[0]);
        l(b).total_cmp(&l(a))
    });
    let rank: Vec<usize> = (0..palette.len()).map(|i| order.iter().position(|o| *o == i).unwrap_or(0)).collect();
    let colors: Vec<(Option<NodeId>, Rgb, Option<Thread>)> = order
        .iter()
        .filter_map(|i| palette.get(*i))
        .map(|c| (None, *c, Some(chart.threads.iter().find(|t| t.color == *c).cloned().unwrap_or_else(|| Thread::new(*c, "")))))
        .collect();
    let cells: Vec<Option<usize>> = cells.into_iter().map(|c| c.and_then(|i| rank.get(i).copied())).collect();
    let dirty = vec![true; colors.len()];
    let mut pattern = Pattern { group: NodeId(0), cols, rows, style, origin, cell, colors, cells, dirty, backs: Vec::new() };
    if pattern.cells.iter().all(Option::is_none) {
        return Err(bad(CMD, "the image is all background: try `background: true`"));
    }
    let used = pattern.counts().iter().filter(|n| **n > 0).count();
    let source = pic.id;
    let id = s.edit("Cross Stitch from Image", |d, sel| {
        let (parent, idx, _) = d.position(source).ok_or(EngineError::NoNode(source))?;
        new_group(d, sel, &mut pattern, "Cross Stitch", (parent, idx + 1))
    })?;
    Ok(json!({ "id": id.0, "cols": cols, "rows": rows, "cell": cell * MM_PER_PT, "colors": used }))
}

fn set_pattern(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.set";
    let group = chosen(s, p).ok_or_else(|| bad(CMD, "select a cross-stitch pattern"))?;
    let mut pattern = Pattern::read(&s.doc()?.doc, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
    if let Some(c) = cell_mm(p, CMD)? {
        pattern.cell = c * PT_PER_MM;
    }
    pattern.style = parse_style(p, CMD, pattern.style)?;
    s.edit("Cross-Stitch Pattern Settings", |d, _| pattern.write(d, true))?;
    Ok(json!({ "id": group.0, "cell": pattern.cell_mm(), "style": style_name(pattern.style) }))
}

/// The colour asked for (`color`), or none when erasing.
fn paint_color(p: &Value, key: &str, cmd: &str) -> Result<Option<Rgb>> {
    let erase = bool_or(p, "erase", false);
    match str_param(p, key) {
        Some(_) if erase => Ok(None),
        Some(c) => Ok(Some(rgb_of(c).ok_or_else(|| bad(cmd, format!("`{key}` is \"#rrggbb\"")))?)),
        None if erase => Ok(None),
        None => Err(bad(cmd, format!("give the `{key}` to paint with, or `erase: true`"))),
    }
}

fn fill(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.fill";
    let color = paint_color(p, "color", CMD)?;
    let at = match p.get("at") {
        None | Some(Value::Null) => None,
        Some(v) => Some(point(v).ok_or_else(|| bad(CMD, "`at` is [x, y] in points"))?),
    };
    let listed = match p.get("cell") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let c = v.get(0).and_then(Value::as_u64).and_then(|c| u32::try_from(c).ok());
            let r = v.get(1).and_then(Value::as_u64).and_then(|r| u32::try_from(r).ok());
            Some(c.zip(r).ok_or_else(|| bad(CMD, "`cell` is [col, row]"))?)
        }
    };
    let d = &s.doc()?.doc;
    let group = chosen(s, p)
        .or_else(|| at.and_then(|q| all_patterns(d).into_iter().find(|g| Pattern::read(d, *g).is_some_and(|pat| pat.contains(q)))))
        .ok_or_else(|| bad(CMD, "fill on a cross-stitch pattern (Embroidery › Cross Stitch › New Cross-Stitch Pattern)"))?;
    let mut pattern = Pattern::read(d, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
    let (c0, r0) = match (listed, at) {
        (Some(cell), _) => cell,
        (None, Some(q)) => pattern.cell_at(q).ok_or_else(|| bad(CMD, "`at` is off the pattern's grid"))?,
        (None, None) => return Err(bad(CMD, "give the square to fill from: `at` or `cell`")),
    };
    let (cols, rows) = (pattern.cols, pattern.rows);
    let start = pattern.cells.get((r0 * cols + c0) as usize).copied().ok_or_else(|| bad(CMD, "`cell` is off the pattern's grid"))?;
    let thread = color.map(|c| nearest_thread(s, c));
    let value = match color {
        Some(c) => Some(pattern.color_index(c, thread)?),
        None => None,
    };
    // the squares side by side with the first that are as it is (4-connected)
    let mut patch = Vec::new();
    let mut seen = vec![false; pattern.cells.len()];
    let mut todo = vec![(c0, r0)];
    while let Some((c, r)) = todo.pop() {
        let i = (r * cols + c) as usize;
        if seen.get(i).copied().unwrap_or(true) || pattern.cells.get(i).copied() != Some(start) {
            continue;
        }
        if let Some(v) = seen.get_mut(i) {
            *v = true;
        }
        patch.push((c, r));
        if c > 0 {
            todo.push((c - 1, r));
        }
        if r > 0 {
            todo.push((c, r - 1));
        }
        if c + 1 < cols {
            todo.push((c + 1, r));
        }
        if r + 1 < rows {
            todo.push((c, r + 1));
        }
    }
    let mut changed = 0usize;
    for (c, r) in patch {
        if pattern.set(c, r, value) {
            changed += 1;
        }
    }
    if changed > 0 {
        s.edit(if value.is_some() { "Fill Cross Stitches" } else { "Erase Cross Stitches" }, |d, _| pattern.write(d, false))?;
    }
    Ok(json!({ "id": group.0, "cells": changed }))
}

fn replace(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.replace";
    let group = chosen(s, p).ok_or_else(|| bad(CMD, "select a cross-stitch pattern"))?;
    let from = str_param(p, "from").and_then(rgb_of).ok_or_else(|| bad(CMD, "`from` is the colour to replace, \"#rrggbb\""))?;
    let to = paint_color(p, "to", CMD)?;
    let mut pattern = Pattern::read(&s.doc()?.doc, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
    let k =
        pattern.colors.iter().position(|(_, c, _)| *c == from).ok_or_else(|| bad(CMD, format!("the pattern has no {} squares", from.to_hex())))?;
    let thread = to.map(|c| nearest_thread(s, c));
    let value = match to {
        Some(c) => Some(pattern.color_index(c, thread)?),
        None => None,
    };
    let mut changed = 0usize;
    for r in 0..pattern.rows {
        for c in 0..pattern.cols {
            if pattern.cells.get((r * pattern.cols + c) as usize).copied().flatten() == Some(k) && pattern.set(c, r, value) {
                changed += 1;
            }
        }
    }
    if changed > 0 {
        s.edit("Replace Cross-Stitch Colour", |d, _| pattern.write(d, false))?;
    }
    Ok(json!({ "id": group.0, "cells": changed }))
}

fn backstitch(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.backstitch";
    let erase = bool_or(p, "erase", false);
    let color = paint_color(p, "color", CMD)?;
    let at: Vec<kurbo::Point> = match p.get("at") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) if list.len() <= 2 => {
            list.iter().map(|v| point(v).ok_or_else(|| bad(CMD, "`at` is [[x, y], [x, y]] in points"))).collect::<Result<_>>()?
        }
        Some(_) => return Err(bad(CMD, "`at` is [[x, y], [x, y]] in points")),
    };
    let corner = |key: &str| -> Result<Option<(u32, u32)>> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => {
                let c = v.get(0).and_then(Value::as_u64).and_then(|c| u32::try_from(c).ok());
                let r = v.get(1).and_then(Value::as_u64).and_then(|r| u32::try_from(r).ok());
                Ok(Some(c.zip(r).ok_or_else(|| bad(CMD, format!("`{key}` is [col, row]")))?))
            }
        }
    };
    let (from, to) = (corner("from")?, corner("to")?);
    let d = &s.doc()?.doc;
    let group = chosen(s, p)
        .or_else(|| {
            at.first().and_then(|q| all_patterns(d).into_iter().find(|g| Pattern::read(d, *g).is_some_and(|pat| pat.corner_at(*q).is_some())))
        })
        .ok_or_else(|| bad(CMD, "draw on a cross-stitch pattern (Embroidery › Cross Stitch › New Cross-Stitch Pattern)"))?;
    let mut pattern = Pattern::read(d, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
    let off = || bad(CMD, "the line's ends are off the pattern's grid");
    if erase {
        let q = at.first().copied().ok_or_else(|| bad(CMD, "give the point to erase at: `at` [[x, y]]"))?;
        let mut gone = 0usize;
        let reach = pattern.cell * 0.5;
        for (_, _, _, lines) in &mut pattern.backs {
            let before = lines.len();
            let origin = pattern.origin;
            let cell = pattern.cell;
            let pt = |c: u32, r: u32| kurbo::Point::new(origin.x + f64::from(c) * cell, origin.y + f64::from(r) * cell);
            lines
                .retain(|l| kurbo::ParamCurveNearest::nearest(&kurbo::Line::new(pt(l[0], l[1]), pt(l[2], l[3])), q, 1e-9).distance_sq.sqrt() > reach);
            gone += before - lines.len();
        }
        if gone > 0 {
            s.edit("Erase Backstitch", |d, _| pattern.write(d, false))?;
        }
        return Ok(json!({ "id": group.0, "lines": gone }));
    }
    let (a, b) = match (from, to, at.as_slice()) {
        (Some(a), Some(b), _) => (a, b),
        (_, _, [qa, qb]) => (pattern.corner_at(*qa).ok_or_else(off)?, pattern.corner_at(*qb).ok_or_else(off)?),
        _ => return Err(bad(CMD, "give the line's ends: `from` and `to`, or `at` [[x, y], [x, y]]")),
    };
    if a.0 > pattern.cols || b.0 > pattern.cols || a.1 > pattern.rows || b.1 > pattern.rows {
        return Err(off());
    }
    if a == b {
        return Ok(json!({ "id": group.0, "lines": 0 }));
    }
    let Some(color) = color else { return Err(bad(CMD, "give the `color` to sew the line in")) };
    let total: usize = pattern.backs.iter().map(|b| b.3.len()).sum();
    if total >= MAX_LINES {
        return Err(bad(CMD, format!("a pattern has at most {MAX_LINES} backstitch lines")));
    }
    let l = line(a.0, a.1, b.0, b.1);
    // a line drawn again in another colour changes colour
    for b in &mut pattern.backs {
        b.3.retain(|x| *x != l);
    }
    let k = match pattern.backs.iter().position(|b| b.1 == color) {
        Some(k) => k,
        None => {
            let thread = nearest_thread(s, color);
            pattern.backs.push((None, color, Some(thread), Vec::new()));
            pattern.backs.len() - 1
        }
    };
    if let Some(b) = pattern.backs.get_mut(k) {
        b.3.push(l);
    }
    s.edit("Backstitch Line", |d, _| pattern.write(d, false))?;
    Ok(json!({ "id": group.0, "lines": 1 }))
}

fn info(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.info";
    let group = chosen(s, p).ok_or_else(|| bad(CMD, "select a cross-stitch pattern"))?;
    let pattern = Pattern::read(&s.doc()?.doc, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
    let counts = pattern.counts();
    let colors: Vec<Value> = pattern
        .colors
        .iter()
        .zip(&counts)
        .filter(|(_, n)| **n > 0)
        .map(|((_, c, t), n)| {
            json!({ "color": c.to_hex(), "name": t.as_ref().map_or("", |t| t.name.as_str()), "code": t.as_ref().map_or("", |t| t.code.as_str()), "cells": n })
        })
        .collect();
    let back: Vec<Value> = pattern
        .backs
        .iter()
        .filter(|b| !b.3.is_empty())
        .map(|(_, c, t, lines)| {
            json!({ "color": c.to_hex(), "name": t.as_ref().map_or("", |t| t.name.as_str()), "code": t.as_ref().map_or("", |t| t.code.as_str()), "lines": lines.len() })
        })
        .collect();
    let cell = pattern.cell_mm();
    Ok(json!({
        "id": group.0,
        "backstitch": back,
        "cols": pattern.cols,
        "rows": pattern.rows,
        "cell": cell,
        "count": (254.0 / cell).round() / 10.0,
        "style": style_name(pattern.style),
        "size": [f64::from(pattern.cols) * cell, f64::from(pattern.rows) * cell],
        "colors": colors,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::pattern::Cmd;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        s
    }

    /// The pattern's stitches (mm), needle points only.
    fn needle_points(s: &Session) -> Vec<sf_core::Point> {
        let plan = s.embroidery_plan().unwrap();
        plan.pattern().stitches.iter().filter(|r| r.cmd == Cmd::Stitch).map(|r| r.pos()).collect()
    }

    /// CH-03: an empty grid, cells painted in two colours, the X stitches on the grid's corners,
    /// a cell repainted changes colour, erased cells go, and Undo takes a stroke back.
    #[test]
    fn cells_painted_on_the_grid_sew_as_crosses_in_their_cells() {
        let mut s = session();
        // 10 × 6 cells of 3 mm at (72, 72) points (one inch in)
        let r = s.execute("embroidery.crossStitch.new", &json!({"cols": 10, "rows": 6, "cell": 3, "at": [72, 72]})).unwrap();
        let id = r["id"].as_u64().unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        assert_eq!((info["cols"].as_u64(), info["rows"].as_u64(), info["colors"].as_array().map(Vec::len)), (Some(10), Some(6), Some(0)));
        assert!((info["cell"].as_f64().unwrap() - 3.0).abs() < 1e-9);
        assert!(s.embroidery_plan().is_none_or(|p| p.pattern().stitch_count() == 0), "an empty grid sews nothing");
        // a row of four red cells by listed cells, then a blue stroke across row 3
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[0, 0], [1, 0], [2, 0], [3, 0]], "color": "#cc0000"})).unwrap();
        let cell = 3.0 * PT_PER_MM;
        let y = 72.0 + 3.5 * cell;
        let r = s
            .execute("embroidery.crossStitch.paint", &json!({"id": id, "at": [[72.0 + 0.5 * cell, y], [72.0 + 5.5 * cell, y]], "color": "#0000cc"}))
            .unwrap();
        assert_eq!(r["cells"], 6, "the stroke's cells, every one between its ends");
        let info = s.execute("embroidery.crossStitch.info", &json!({"id": id})).unwrap();
        let counts: Vec<(String, u64)> =
            info["colors"].as_array().unwrap().iter().map(|c| (c["color"].as_str().unwrap().to_string(), c["cells"].as_u64().unwrap())).collect();
        assert_eq!(counts, vec![("#cc0000".to_string(), 4), ("#0000cc".to_string(), 6)]);
        // every painted cell's four corners are needle points, and nearly every needle point is
        // on a grid line (corners, or travel along a line; the ties aren't)
        let origin = 72.0 * MM_PER_PT;
        let on = |v: f64| ((v - origin) / 3.0 - ((v - origin) / 3.0).round()).abs() < 1e-6;
        let pts = needle_points(&s);
        let cells = [(0, 0), (1, 0), (2, 0), (3, 0), (0, 3), (1, 3), (2, 3), (3, 3), (4, 3), (5, 3)];
        for (c, r) in cells {
            for (dc, dr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let corner = sf_core::Point::new(origin + f64::from(c + dc) * 3.0, origin + f64::from(r + dr) * 3.0);
                assert!(pts.iter().any(|p| p.distance(corner) < 1e-6), "cell ({c}, {r}) has no stitch at {corner:?}");
            }
        }
        let off = pts.iter().filter(|p| !(on(p.x) || on(p.y))).count();
        assert!(off * 10 < pts.len(), "{off} of {} needle points off the grid", pts.len());
        // two threads, red sewn first
        let plan = s.embroidery_plan().unwrap();
        let threads: Vec<String> = plan.pattern().threads.iter().map(|t| t.color.to_hex()).collect();
        assert_eq!(threads, vec!["#cc0000", "#0000cc"]);
        // repainting a red cell blue moves it; erasing takes cells away
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[0, 0]], "color": "#0000cc"})).unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[1, 0], [2, 0], [3, 0]], "erase": true})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        assert_eq!(info["colors"].as_array().unwrap().len(), 1, "red has no cells left: {info}");
        assert_eq!(info["colors"][0]["cells"], 7);
        s.execute("edit.undo", &json!({})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        assert_eq!(info["colors"].as_array().unwrap().len(), 2, "Undo brings the erased cells back");
    }

    /// Moving a pattern keeps its cells, the grid moves with it and new cells land on it; a bigger
    /// brush paints a square; the pattern survives a save (the document as JSON) unchanged.
    #[test]
    fn a_moved_pattern_keeps_its_cells_and_survives_saving() {
        let mut s = session();
        let id = s.execute("embroidery.crossStitch.new", &json!({"cols": 20, "rows": 20, "cell": 2.5})).unwrap()["id"].as_u64().unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[10, 10]], "color": "#228822", "size": 3})).unwrap();
        assert_eq!(s.execute("embroidery.crossStitch.info", &json!({})).unwrap()["colors"][0]["cells"], 9, "a 3 × 3 brush");
        let before = needle_points(&s);
        s.execute("select.set", &json!({"ids": [id]})).unwrap();
        s.execute("object.move", &json!({"dx": 36.0, "dy": 18.0})).unwrap();
        let after = needle_points(&s);
        assert_eq!(before.len(), after.len());
        let (dx, dy) = (36.0 * MM_PER_PT, 18.0 * MM_PER_PT);
        for (a, b) in before.iter().zip(&after) {
            assert!((b.x - a.x - dx).abs() < 1e-6 && (b.y - a.y - dy).abs() < 1e-6, "{a:?} → {b:?}");
        }
        // painting by point after the move finds the moved grid (no id, nothing selected)
        s.execute("select.set", &json!({"ids": []})).unwrap();
        let grid = Pattern::read(&s.doc().unwrap().doc, NodeId(id)).unwrap();
        let at = grid.origin + kurbo::Vec2::new(0.5 * grid.cell, 0.5 * grid.cell);
        s.execute("embroidery.crossStitch.paint", &json!({"at": [[at.x, at.y]], "color": "#228822"})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({"id": id})).unwrap();
        assert_eq!(info["colors"][0]["cells"], 10);
        // the cells are kept with the document
        let doc = s.doc().unwrap().doc.clone();
        let text = serde_json::to_string(&*doc).unwrap();
        let back: Document = serde_json::from_str(&text).unwrap();
        let p = Pattern::read(&back, NodeId(id)).unwrap();
        assert_eq!(p.counts(), vec![10]);
        // half crosses and a larger cell: the grid grows from its corner
        let r = s.execute("embroidery.crossStitch.set", &json!({"id": id, "cell": 4, "style": "half"})).unwrap();
        assert_eq!(r["style"], "half");
        let info = s.execute("embroidery.crossStitch.info", &json!({"id": id})).unwrap();
        assert!((info["size"][0].as_f64().unwrap() - 80.0).abs() < 1e-6, "{info}");
        assert_eq!(info["colors"][0]["cells"], 10);
    }

    /// A picture becomes a pattern of its main colours, the background left unsewn.
    #[test]
    fn an_image_becomes_a_pattern_of_its_colours() {
        let mut s = session();
        // a red disc and a blue bar on white
        let img = vectorcraft_trace::Raster::from_fn(120, 80, |x, y| {
            let (dx, dy) = (x as f64 - 40.0, y as f64 - 40.0);
            if dx * dx + dy * dy <= 25.0 * 25.0 {
                [200, 30, 30, 255]
            } else if (75..115).contains(&x) && (30..50).contains(&y) {
                [30, 30, 200, 255]
            } else {
                [255, 255, 255, 255]
            }
        });
        s.execute("file.place", &json!({"name": "art.png", "dataBase64": vectorcraft_format::base64_encode(&img.encode_png())})).unwrap();
        let r = s.execute("embroidery.crossStitch.fromImage", &json!({"cols": 30, "colors": 3})).unwrap();
        assert_eq!(r["cols"], 30, "{r}");
        assert_eq!(r["colors"], 2, "red and blue; white is the background: {r}");
        let info = s.execute("embroidery.crossStitch.info", &json!({"id": r["id"]})).unwrap();
        let total: u64 = info["colors"].as_array().unwrap().iter().map(|c| c["cells"].as_u64().unwrap()).sum();
        assert!(total > 50 && total < 30 * 20, "{info}");
        assert!(!needle_points(&s).is_empty());
        // the background sewn too when asked
        s.execute("edit.undo", &json!({})).unwrap();
        let r = s.execute("embroidery.crossStitch.fromImage", &json!({"cols": 30, "colors": 3, "background": true})).unwrap();
        assert_eq!(r["colors"], 3, "{r}");
    }

    /// The Cross Stitch tool: a drag paints a row of squares as one Undo step, Alt-drag erases.
    #[test]
    fn the_cross_stitch_tool_paints_and_erases_by_dragging() {
        use vectorcraft_tools::{Mods, PointerEvent, PointerKind};
        let mut s = session();
        let id = s.execute("embroidery.crossStitch.new", &json!({"cols": 12, "rows": 4, "cell": 4, "at": [100, 100]})).unwrap()["id"].clone();
        let v = crate::ViewInfo::default();
        s.select_tool("embroideryCrossStitch", v).unwrap();
        s.set_tool_options(Some("embroideryCrossStitch"), json!({"color": "#3366aa"}).as_object().unwrap());
        let cell = 4.0 * PT_PER_MM;
        let y = 100.0 + 1.5 * cell;
        let at = |c: f64| 100.0 + (c + 0.5) * cell;
        let ev = |kind, x: f64, alt: bool| PointerEvent::new(kind, x, y).with_mods(Mods { alt, ..Mods::default() });
        s.pointer(&ev(PointerKind::Down, at(1.0), false), v).unwrap();
        s.pointer(&ev(PointerKind::Drag, at(4.0), false), v).unwrap();
        s.pointer(&ev(PointerKind::Up, at(8.0), false), v).unwrap();
        let cells = |s: &mut Session| s.execute("embroidery.crossStitch.info", &json!({"id": id})).unwrap()["colors"][0]["cells"].as_u64();
        assert_eq!(cells(&mut s), Some(8), "squares 1 to 8 of row 1");
        // Alt-drag erases three
        s.pointer(&ev(PointerKind::Down, at(1.0), true), v).unwrap();
        s.pointer(&ev(PointerKind::Up, at(3.0), true), v).unwrap();
        assert_eq!(cells(&mut s), Some(5));
        // one stroke, one Undo step
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(cells(&mut s), Some(8));
        // with Fill on, a click on an empty square fills the empty patch round it
        s.set_tool_options(Some("embroideryCrossStitch"), json!({"fill": true, "color": "#aa3300"}).as_object().unwrap());
        s.pointer(&ev(PointerKind::Down, at(10.0), false), v).unwrap();
        s.pointer(&ev(PointerKind::Up, at(10.0), false), v).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({"id": id})).unwrap();
        assert_eq!(info["colors"][1]["cells"], 48 - 8, "every empty square: they all touch");
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.crossStitch.info", &json!({"id": id})).unwrap()["colors"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn bad_params_are_refused() {
        let mut s = session();
        for p in [json!({"cols": 0}), json!({"rows": 301}), json!({"cell": 0.2}), json!({"style": "zigzag"}), json!({"at": "here"})] {
            assert!(s.execute("embroidery.crossStitch.new", &p).is_err(), "{p}");
        }
        // painting needs a pattern, cells and a colour
        assert!(s.execute("embroidery.crossStitch.paint", &json!({"cells": [[0, 0]], "color": "#ff0000"})).is_err());
        s.execute("embroidery.crossStitch.new", &json!({"cols": 5, "rows": 5})).unwrap();
        for p in [
            json!({"cells": [[0, 0]]}),
            json!({"cells": [[0, 0]], "color": "red"}),
            json!({"color": "#ff0000"}),
            json!({"cells": "all", "color": "#ff0000"}),
            json!({"cells": [[0, 0]], "color": "#ff0000", "size": 12}),
            json!({"at": [[f64::NAN, 0.0]], "color": "#ff0000"}),
        ] {
            assert!(s.execute("embroidery.crossStitch.paint", &p).is_err(), "{p}");
        }
        // cells off the grid are left alone
        let r = s.execute("embroidery.crossStitch.paint", &json!({"cells": [[-1, 0], [5, 5], [99, 2]], "color": "#ff0000"})).unwrap();
        assert_eq!(r["cells"], 0);
        assert!(s.execute("embroidery.crossStitch.fromImage", &json!({})).is_err(), "no image");
    }

    /// CH-04: the fill bucket fills the patch of touching squares alike, empty or of one colour.
    #[test]
    fn the_fill_bucket_fills_a_patch_of_touching_squares() {
        let mut s = session();
        s.execute("embroidery.crossStitch.new", &json!({"cols": 6, "rows": 4, "cell": 3, "at": [72, 72]})).unwrap();
        // a red wall down column 2 splits the grid: the empty left side fills, the right doesn't
        let wall: Vec<Value> = (0..4).map(|r| json!([2, r])).collect();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": wall, "color": "#cc0000"})).unwrap();
        let r = s.execute("embroidery.crossStitch.fill", &json!({"cell": [0, 0], "color": "#0000cc"})).unwrap();
        assert_eq!(r["cells"], 8, "columns 0 and 1, every row");
        // by a point on the canvas: the wall turns green, as one patch
        let cell = 3.0 * PT_PER_MM;
        let r = s.execute("embroidery.crossStitch.fill", &json!({"at": [72.0 + 2.5 * cell, 72.0 + 0.5 * cell], "color": "#00aa00"})).unwrap();
        assert_eq!(r["cells"], 4);
        // erasing a patch, and one Undo for it
        let r = s.execute("embroidery.crossStitch.fill", &json!({"cell": [1, 3], "erase": true})).unwrap();
        assert_eq!(r["cells"], 8);
        s.execute("edit.undo", &json!({})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        let counts: Vec<u64> = info["colors"].as_array().unwrap().iter().map(|c| c["cells"].as_u64().unwrap()).collect();
        assert_eq!(counts, [8, 4], "the blue left side, then the green wall (in sewing order)");
        assert!(s.execute("embroidery.crossStitch.fill", &json!({"cell": [9, 9], "color": "#000000"})).is_err(), "off the grid");
        assert!(s.execute("embroidery.crossStitch.fill", &json!({"cell": [0, 0]})).is_err(), "no colour");
    }

    /// CH-04: Replace Colour turns every square of one colour into another (merging with it when the
    /// pattern has it), or clears them.
    #[test]
    fn a_colour_is_replaced_everywhere() {
        let mut s = session();
        s.execute("embroidery.crossStitch.new", &json!({"cols": 5, "rows": 5})).unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[0, 0], [1, 1], [4, 4]], "color": "#cc0000"})).unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[2, 2]], "color": "#0000cc"})).unwrap();
        let r = s.execute("embroidery.crossStitch.replace", &json!({"from": "#cc0000", "to": "#0000cc"})).unwrap();
        assert_eq!(r["cells"], 3);
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        let colors = info["colors"].as_array().unwrap();
        assert_eq!((colors.len(), colors[0]["color"].as_str(), colors[0]["cells"].as_u64()), (1, Some("#0000cc"), Some(4)));
        let r = s.execute("embroidery.crossStitch.replace", &json!({"from": "#0000cc", "erase": true})).unwrap();
        assert_eq!(r["cells"], 4);
        assert!(s.execute("embroidery.crossStitch.replace", &json!({"from": "#0000cc", "to": "#ffffff"})).is_err(), "no such colour left");
    }

    /// CH-04: a pattern sized by its fabric count: 14-count Aida is 25.4 / 14 mm a square.
    #[test]
    fn patterns_are_sized_by_fabric_count() {
        let mut s = session();
        s.execute("embroidery.crossStitch.new", &json!({"cols": 10, "rows": 10, "count": 14})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        assert!((info["cell"].as_f64().unwrap() - 25.4 / 14.0).abs() < 1e-9);
        assert_eq!(info["count"], 14.0);
        s.execute("embroidery.crossStitch.set", &json!({"count": 11})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        assert_eq!(info["count"], 11.0);
        assert!(s.execute("embroidery.crossStitch.set", &json!({"count": 40})).is_err());
    }

    /// CH-05: backstitch lines from grid corner to corner, joined where they meet, sewn as a run
    /// over the crosses with its needle on the corners; drawn again in another colour they change
    /// colour, and an erase near a line takes it away.
    #[test]
    fn backstitch_lines_run_from_corner_to_corner_over_the_crosses() {
        let mut s = session();
        let id = s.execute("embroidery.crossStitch.new", &json!({"cols": 10, "rows": 10, "cell": 3, "at": [72, 72]})).unwrap()["id"].clone();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[1, 1], [2, 1]], "color": "#cccc00"})).unwrap();
        s.execute("embroidery.crossStitch.backstitch", &json!({"from": [0, 0], "to": [3, 0], "color": "#202020"})).unwrap();
        s.execute("embroidery.crossStitch.backstitch", &json!({"from": [3, 3], "to": [3, 0], "color": "#202020"})).unwrap();
        // by points near the corners
        let cell = 3.0 * PT_PER_MM;
        let near = |c: f64, r: f64| json!([72.0 + c * cell + 1.0, 72.0 + r * cell - 1.0]);
        s.execute("embroidery.crossStitch.backstitch", &json!({"id": id, "at": [near(5.0, 5.0), near(7.0, 6.0)], "color": "#0000aa"})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        let lines: Vec<(String, u64)> =
            info["backstitch"].as_array().unwrap().iter().map(|b| (b["color"].as_str().unwrap().to_string(), b["lines"].as_u64().unwrap())).collect();
        assert_eq!(lines, [("#202020".to_string(), 2), ("#0000aa".to_string(), 1)]);
        // the backstitch sews last, its needle on the corners: (0,0) (1,0) (2,0) (3,0) … (3,3)
        let plan = s.embroidery_plan().unwrap();
        let st = s.doc().unwrap();
        let last = plan.nodes.last().and_then(|n| st.doc.node(*n)).unwrap();
        assert!(last.name.as_deref().unwrap_or_default().starts_with("Backstitch"), "{:?}", last.name);
        let pts = needle_points(&s);
        let mm = 72.0 * MM_PER_PT;
        for (c, r) in [(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0), (3.0, 1.0), (3.0, 2.0), (3.0, 3.0)] {
            let want = sf_core::Point::new(mm + c * 3.0, mm + r * 3.0);
            assert!(pts.iter().any(|p| p.distance(want) < 0.05), "a needle point on corner ({c}, {r})");
        }
        // the same line in another colour moves to it
        s.execute("embroidery.crossStitch.backstitch", &json!({"from": [3, 0], "to": [0, 0], "color": "#0000aa"})).unwrap();
        let info = s.execute("embroidery.crossStitch.info", &json!({})).unwrap();
        assert_eq!(info["backstitch"][0]["lines"], 1);
        assert_eq!(info["backstitch"][1]["lines"], 2);
        // erasing near the slanted line takes only it away; Undo brings it back
        let mid = json!([72.0 + 6.0 * cell, 72.0 + 5.5 * cell]);
        let r = s.execute("embroidery.crossStitch.backstitch", &json!({"at": [mid], "erase": true})).unwrap();
        assert_eq!(r["lines"], 1);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.crossStitch.info", &json!({})).unwrap()["backstitch"][1]["lines"], 2);
        // off the grid, and nothing to draw with
        assert!(s.execute("embroidery.crossStitch.backstitch", &json!({"from": [0, 0], "to": [11, 0], "color": "#000000"})).is_err());
        assert!(s.execute("embroidery.crossStitch.backstitch", &json!({"from": [0, 0], "to": [1, 0]})).is_err());
        // the cell size changes: the lines stay on their corners
        s.execute("embroidery.crossStitch.set", &json!({"cell": 4})).unwrap();
        let pts = needle_points(&s);
        assert!(pts.iter().any(|p| p.distance(sf_core::Point::new(mm + 12.0, mm + 12.0)) < 0.05), "corner (3, 3) at 4 mm squares");
    }

    /// The Cross Stitch tool's Backstitch: a drag draws a line between the corners nearest its ends,
    /// an Alt-click takes it away.
    #[test]
    fn the_tool_draws_backstitch_by_dragging() {
        use vectorcraft_tools::{Mods, PointerEvent, PointerKind};
        let mut s = session();
        s.execute("embroidery.crossStitch.new", &json!({"cols": 8, "rows": 8, "cell": 4, "at": [100, 100]})).unwrap();
        let v = crate::ViewInfo::default();
        s.select_tool("embroideryCrossStitch", v).unwrap();
        s.set_tool_options(Some("embroideryCrossStitch"), json!({"line": true, "color": "#112233"}).as_object().unwrap());
        let cell = 4.0 * PT_PER_MM;
        let at = |c: f64, r: f64, alt: bool| {
            PointerEvent::new(PointerKind::Down, 100.0 + c * cell, 100.0 + r * cell).with_mods(Mods { alt, ..Mods::default() })
        };
        let with = |e: PointerEvent, kind| PointerEvent { kind, ..e };
        s.pointer(&at(1.1, 1.0, false), v).unwrap();
        s.pointer(&with(at(3.0, 2.0, false), PointerKind::Drag), v).unwrap();
        s.pointer(&with(at(4.9, 4.1, false), PointerKind::Up), v).unwrap();
        let lines = |s: &mut Session| s.execute("embroidery.crossStitch.info", &json!({})).unwrap()["backstitch"].as_array().map_or(0, Vec::len);
        assert_eq!(lines(&mut s), 1, "one line, (1,1) to (5,4)");
        s.pointer(&at(3.0, 2.5, true), v).unwrap();
        s.pointer(&with(at(3.0, 2.5, true), PointerKind::Up), v).unwrap();
        assert_eq!(lines(&mut s), 0);
    }
}
