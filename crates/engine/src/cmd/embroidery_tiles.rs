//! Tile quilting (T-series): background quilting for a quilt sandwich hooped one block at a
//! time, as with clear marking tiles. A tile the size of the block is laid on the quilt, its
//! middle, crosshairs, up arrow, sides and corners marked; the block that size is hooped and
//! quilted. Every block is one running-stitch thread over the whole rectangle at the same scale,
//! its lines kept half a line-spacing in from the edge, so blocks of any size laid side by side
//! read as one continuous quilting with no lines to match. `embroidery.quiltBlock` makes one
//! block; `embroidery.quiltBlock.set` writes one machine file per size, named by its size.
//! Patterns and wording are our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, Result, Session, always, bad, bool_or, cmd, str_param};

/// mm in an inch.
const IN: f64 = 25.4;

/// The patterns: id, name.
pub const PATTERNS: &[(&str, &str)] = &[
    ("stipple", "Stipple"),
    ("swirls", "Swirls"),
    ("loops", "Loops"),
    ("feathers", "Feathers"),
    ("pebbles", "Pebbles"),
    ("waves", "Waves"),
    ("spiral", "Squares spiral"),
    ("crosshatch", "Crosshatch"),
];

/// Block sizes (inches) to start from: common tile and hoop sizes from 2″ to 9 × 14″.
pub const SIZES: &[(f64, f64)] = &[
    (2.0, 2.0),
    (3.0, 3.0),
    (4.0, 4.0),
    (4.0, 6.0),
    (5.0, 5.0),
    (5.0, 7.0),
    (6.0, 6.0),
    (6.0, 8.0),
    (6.0, 10.0),
    (7.0, 7.0),
    (8.0, 8.0),
    (8.0, 12.0),
    (9.0, 9.0),
    (9.0, 14.0),
];

/// Border strip sizes (inches, long side across): narrow strips from 1 × 4″ to 3 × 10″.
pub const BORDER_SIZES: &[(f64, f64)] = &[
    (4.0, 1.0),
    (6.0, 1.0),
    (5.0, 1.5),
    (7.0, 1.5),
    (4.0, 2.0),
    (5.0, 2.0),
    (6.0, 2.0),
    (7.0, 2.0),
    (8.0, 2.0),
    (10.0, 2.0),
    (7.0, 3.0),
    (10.0, 3.0),
];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.quiltBlock.patterns",
            "Quilt Block Patterns",
            [],
            None,
            "{} the background quilting patterns and the usual block and border strip sizes → {patterns: [{id, name}], sizes: [{width, height}], borders: [{width, height}] (inches)}",
            always,
            patterns
        ),
        cmd!(
            "embroidery.quiltBlock",
            "Quilt Block",
            [],
            None,
            "{width, height: the block (tile) size, unit?: in|mm (in), corner?: bool (a corner square where two border strips meet, the side the smaller of width and height: a row pattern bent round it, in from the right edge and out through the bottom), text?, shape?: Heart|Star|… , applique?: bool (words, a shape or an appliqué piece in the middle, sewn first, the quilting going round it `clearance` away), clearance?: mm (4; 1–30), pattern?: stipple|swirls|loops|feathers|pebbles|waves|spiral|crosshatch (stipple), spacing?: mm between lines (6; 2–25; keep it the same for every block of a quilt), stitch?: mm (3; 1.5–5), color?: \"#rrggbb\", hoop?: a hoop's name (else the smallest of My Machine's, or the usual ones, it fits)} a new design: the block's rectangle quilted edge to edge in one running-stitch thread, centred in the hoop, top up → {pattern, widthMm, heightMm, hoop, stitches}",
            always,
            quilt_block
        ),
        cmd!(
            "embroidery.quiltBlock.set",
            "Quilt Block Set",
            [],
            None,
            "{folder?: where the files go (none: they come back in one zip, for the web app), pattern?, kind?: blocks|borders|corners (blocks; corners: a square for each border strip width), sizes?: [{width, height}] (inches; the usual block or border strip sizes), spacing?, stitch?, color?, format?: dst|exp|jef|pes|vp3|xxx (My Machine's)} one machine file per block size in the folder, named \"<Pattern> <w>x<h> in.<ext>\" (borders: \"Border <Pattern> …\"); sizes no hoop fits are left out and said → {files: [path], skipped: [{width, height, reason}]} (no folder: files are their names, plus {name: \"<Pattern> quilt blocks.zip\", dataBase64})",
            always,
            quilt_set
        ),
    ]
}

fn patterns(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(json!({
        "patterns": PATTERNS.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "sizes": SIZES.iter().map(|(w, h)| json!({ "width": w, "height": h })).collect::<Vec<_>>(),
        "borders": BORDER_SIZES.iter().map(|(w, h)| json!({ "width": w, "height": h })).collect::<Vec<_>>(),
    }))
}

/// A length as the file names write it: `5`, `5.5`.
fn num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 { format!("{}", r.round() as i64) } else { format!("{r}") }
}

/// Rows across the block for a motif `amp` mm either side of its row line, about `pitch` apart:
/// the rows' middles, spread so the motifs reach `m` from the block's top and bottom.
fn rows(h: f64, pitch: f64, amp: f64, m: f64) -> Vec<f64> {
    let room = (h - 2.0 * m - 2.0 * amp).max(0.0);
    let n = ((room / pitch).round() as usize + 1).clamp(1, 2000);
    if n == 1 {
        return vec![h / 2.0];
    }
    // spread to reach from the top edge to the bottom one
    let step = room / (n as f64 - 1.0);
    (0..n).map(|i| m + amp + i as f64 * step).collect()
}

/// A row-by-row pattern: `row(x0, x1, y)` gives one row left to right; odd rows run right to
/// left (mirrored), and the line steps down the side to the next, all one line kept inside the
/// block `m` from its edges.
fn by_rows(w: f64, h: f64, m: f64, pitch: f64, amp: f64, row: impl Fn(f64, f64, f64) -> Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let (x0, x1) = (m, w - m);
    // a strip narrower than the motif: the motif squeezed to the strip's height
    let squeeze = if 2.0 * amp > h - 2.0 * m && amp > 0.0 { ((h - 2.0 * m) / (2.0 * amp)).max(0.05) } else { 1.0 };
    let mut pts = Vec::new();
    for (i, y) in rows(h, pitch, amp * squeeze, m).into_iter().enumerate() {
        let mut r: Vec<(f64, f64)> = row(x0, x1, y).into_iter().map(|(x, v)| (x, y + (v - y) * squeeze)).collect();
        // odd rows come back right to left: the row mirrored, so it starts at the right edge
        if i % 2 == 1 {
            r = r.into_iter().map(|(x, y)| (x0 + x1 - x, y)).collect();
        }
        pts.extend(r);
    }
    pts.into_iter().map(|(x, y)| (x.clamp(m, w - m), y.clamp(m, h - m))).collect()
}

/// One row of a motif from `x0` to `x1` along the line `y` (mm) → its points.
type Row = Box<dyn Fn(f64, f64, f64) -> Vec<(f64, f64)>>;

/// A row motif: how far it reaches either side of its row line, how far apart its rows go, and
/// how to draw one row.
struct Motif {
    amp: f64,
    pitch: f64,
    row: Row,
}

/// A cubic Bézier's points from `a` to `d`.
fn cubic(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64), n: usize) -> impl Iterator<Item = (f64, f64)> {
    (0..=n).map(move |i| {
        let t = i as f64 / n as f64;
        let u = 1.0 - t;
        let (k0, k1, k2, k3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        (k0 * a.0 + k1 * b.0 + k2 * c.0 + k3 * d.0, k0 * a.1 + k1 * b.1 + k2 * c.1 + k3 * d.1)
    })
}

/// The row motif of a row pattern (none for stipple, spiral and crosshatch, which fill the block
/// as a whole).
fn motif(pattern: &str, gap: f64) -> Option<Motif> {
    use std::f64::consts::{FRAC_PI_2, PI, TAU};
    Some(match pattern {
        // a row of round cursive loops (a looped cycloid)
        "loops" => {
            let d = gap * 0.9;
            let r = d * 0.42;
            Motif {
                amp: d,
                pitch: 2.0 * d + gap * 0.6,
                row: Box::new(move |x0, x1, y| {
                    // start and end on the row line (cos = 0), so rows join cleanly at the sides
                    let turns = ((x1 - x0 - 2.0 * d) / r).max(0.0);
                    let n = ((turns / 0.1).ceil() as usize).clamp(2, 200_000);
                    let mut pts = vec![(x0, y)];
                    pts.extend((0..=n).map(|i| {
                        let t = FRAC_PI_2 + turns * i as f64 / n as f64;
                        (x0 + 2.0 * d + r * (t - FRAC_PI_2) - d * t.sin(), y - d * t.cos())
                    }));
                    pts.push((x1, y));
                    pts
                }),
            }
        }
        // a spiral in, its echo back out beside it, and half a turn round the outside on to the
        // next; each swirl turns the other way from the last
        "swirls" => {
            let big = gap * 1.6;
            let echo = gap * 0.35;
            let outer = big + echo;
            Motif {
                amp: outer,
                pitch: 2.0 * outer + gap * 0.5,
                row: Box::new(move |x0, x1, y| {
                    let cell = 2.0 * outer;
                    let n = (((x1 - x0) / cell).floor() as usize).clamp(1, 5000);
                    let left = x0 + ((x1 - x0) - n as f64 * cell) / 2.0;
                    let mut pts = vec![(x0, y)];
                    let turn = 2.5 * PI;
                    for k in 0..n {
                        let cx = left + outer + k as f64 * cell;
                        let s = if k % 2 == 0 { 1.0 } else { -1.0 };
                        let at = |a: f64, rad: f64| (cx - rad * a.cos(), y - s * rad * a.sin());
                        let steps = 60;
                        for i in 0..=steps {
                            let t = i as f64 / steps as f64;
                            pts.push(at(t * turn, big * (1.0 - 0.8 * t)));
                        }
                        for i in (0..=steps).rev() {
                            let t = i as f64 / steps as f64;
                            pts.push(at(t * turn, big * (1.0 - 0.8 * t) + echo));
                        }
                        for i in 0..=steps / 2 {
                            pts.push(at(-PI * i as f64 / (steps / 2) as f64, outer));
                        }
                    }
                    pts.push((x1, y));
                    pts
                }),
            }
        }
        // teardrop plumes all growing one way, each swelling back over the one before and coming
        // down to the stem a plume further on
        "feathers" => {
            let ph = gap * 1.8;
            let pw = gap * 1.1;
            Motif {
                amp: ph / 2.0,
                pitch: ph + gap * 0.6,
                row: Box::new(move |x0, x1, y| {
                    let stem = y + ph / 2.0;
                    let n = (((x1 - x0) / pw).floor() as usize).clamp(1, 20_000);
                    let left = x0 + ((x1 - x0) - n as f64 * pw) / 2.0;
                    let mut pts = vec![(x0, stem)];
                    for k in 0..n {
                        let b = left + k as f64 * pw;
                        let tip = (b + pw * 0.3, stem - ph);
                        pts.extend(cubic((b, stem), (b - pw * 0.9, stem - ph * 0.4), (b - pw * 0.5, stem - ph * 1.05), tip, 16));
                        pts.extend(cubic(tip, (b + pw * 1.0, stem - ph * 0.95), (b + pw * 1.2, stem - ph * 0.35), (b + pw, stem), 16).skip(1));
                    }
                    pts.push((x1, stem));
                    pts
                }),
            }
        }
        // round pebbles of mixed sizes, each sewn once round and half round again to the next,
        // which it touches
        "pebbles" => {
            let big = gap * 1.1;
            Motif {
                amp: big,
                pitch: 2.0 * big + gap * 0.3,
                row: Box::new(move |x0, x1, y| {
                    // sizes from a fixed sequence, so the same block always sews the same
                    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15 ^ (y * 100.0) as u64;
                    let mut next = || {
                        seed ^= seed << 13;
                        seed ^= seed >> 7;
                        seed ^= seed << 17;
                        (seed >> 11) as f64 / (1u64 << 53) as f64
                    };
                    let mut pts = vec![(x0, y)];
                    let mut x = x0;
                    let mut up = true;
                    while x < x1 - gap * 0.6 {
                        let r = (gap * (0.55 + 0.55 * next())).min((x1 - x) / 2.0);
                        let cx = x + r;
                        let s = if up { -1.0 } else { 1.0 };
                        let steps = ((r * 1.5 * TAU / 0.8).ceil() as usize).clamp(24, 400);
                        for i in 0..=steps {
                            let a = PI + 1.5 * TAU * i as f64 / steps as f64;
                            pts.push((cx + r * a.cos(), y + s * r * a.sin()));
                        }
                        x = cx + r;
                        up = !up;
                    }
                    pts.push((x1, y));
                    pts
                }),
            }
        }
        // a gentle wave
        "waves" => {
            let (amp, period) = (gap * 0.3, gap * 4.0);
            Motif {
                amp,
                pitch: gap,
                row: Box::new(move |x0, x1, y| {
                    let steps = ((x1 - x0).ceil() as usize).clamp(2, 20_000);
                    (0..=steps)
                        .map(|i| {
                            let x = x0 + (x1 - x0) * i as f64 / steps as f64;
                            (x, y + (x / period * TAU).sin() * amp)
                        })
                        .collect()
                }),
            }
        }
        _ => return None,
    })
}

/// A row pattern over the block `w` × `h`.
fn rowed(w: f64, h: f64, gap: f64, m: &Motif) -> Vec<(f64, f64)> {
    by_rows(w, h, gap / 2.0, m.pitch, m.amp, |x0, x1, y| (m.row)(x0, x1, y))
}

/// A corner square `a` mm across where two border strips meet: rows of the motif bent round
/// quarter circles about its inner corner (bottom right), in from the right edge and out through
/// the bottom, nested from the inner corner out, each row coming back the other way along the edge
/// to the next (the top left corner of a quilt; turn the hoop for the others), squeezed to fit.
fn corner(a: f64, gap: f64, m: &Motif) -> Vec<(f64, f64)> {
    use std::f64::consts::FRAC_PI_2;
    let margin = gap / 2.0;
    let squeeze = if m.amp > a / 2.0 - margin && m.amp > 0.0 { ((a / 2.0 - margin) / m.amp).max(0.05) } else { 1.0 };
    let amp = m.amp * squeeze;
    let (lo, hi) = (margin + amp, a - margin - amp);
    let n = if hi > lo { ((hi - lo) / m.pitch).round() as usize + 1 } else { 1 }.clamp(1, 500);
    let radii: Vec<f64> = if n == 1 { vec![a / 2.0] } else { (0..n).map(|k| lo + (hi - lo) * k as f64 / (n - 1) as f64).collect() };
    let mut out = Vec::new();
    for (k, r0) in radii.into_iter().enumerate() {
        let mut row: Vec<(f64, f64)> = (m.row)(0.0, r0 * FRAC_PI_2, 0.0)
            .into_iter()
            .map(|(s, v)| {
                // along the arc by `s`, outwards (towards the quilt's edge) for a motif's up
                let phi = -FRAC_PI_2 - s / r0;
                let r = r0 - v * squeeze;
                ((a + r * phi.cos()).clamp(margin, a - margin), (a + r * phi.sin()).clamp(margin, a - margin))
            })
            .collect();
        if k % 2 == 1 {
            row.reverse();
        }
        out.extend(row);
    }
    out
}

/// The area round the stitches art nodes `ids` sew, `clearance` mm out, its holes (the counters of
/// letters) filled: its outer edges as rings in mm, traced on a half-millimetre grid and smoothed.
/// From the stitches themselves, so any art (words, shapes, appliqué, a picture) gives its true
/// outline.
fn sewn_rings(s: &Session, ids: &[u64], clearance: f64) -> Vec<Vec<(f64, f64)>> {
    let segs = sewn_segments(s, ids);
    if segs.is_empty() {
        return Vec::new();
    }
    let cell = 0.5;
    let pad = clearance + 2.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (a, b) in &segs {
        for p in [a, b] {
            (x0, y0, x1, y1) = (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1));
        }
    }
    let (ox, oy) = (x0 - pad, y0 - pad);
    let cols = (((x1 - x0 + 2.0 * pad) / cell).ceil() as usize).clamp(1, 3000);
    let rows = (((y1 - y0 + 2.0 * pad) / cell).ceil() as usize).clamp(1, 3000);
    let mut sewn = vec![false; cols * rows];
    let at = |x: f64, y: f64| -> Option<usize> {
        let (i, j) = (((x - ox) / cell).floor(), ((y - oy) / cell).floor());
        (i >= 0.0 && j >= 0.0 && (i as usize) < cols && (j as usize) < rows).then(|| j as usize * cols + i as usize)
    };
    for (a, b) in &segs {
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / (cell / 2.0)).ceil().clamp(1.0, 100_000.0) as usize;
        for k in 0..=n {
            let t = k as f64 / n as f64;
            if let Some(c) = at(a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t) {
                sewn[c] = true;
            }
        }
    }
    let keep = widened(&sewn, cols, rows, (clearance / cell).ceil() as i64);
    let outside = reached_from_edge(&keep, cols, rows);
    trace_rings(&outside, cols, rows, (ox, oy, cell))
}

/// The art nodes `ids` sew and everything in them, as their stitches as line pieces (mm).
fn sewn_segments(s: &Session, ids: &[u64]) -> Vec<((f64, f64), (f64, f64))> {
    let Some(plan) = s.embroidery_plan() else { return Vec::new() };
    // the art and everything in it
    let mut nodes = std::collections::HashSet::new();
    if let Ok(st) = s.doc() {
        let mut stack: Vec<vectorcraft_doc::NodeId> = ids.iter().map(|&i| vectorcraft_doc::NodeId(i)).collect();
        while let Some(id) = stack.pop() {
            if nodes.insert(id)
                && let Some(kids) = st.doc.node(id).and_then(|n| n.children())
            {
                stack.extend(kids.iter().map(|k| k.id));
            }
        }
    }
    // its stitches as line pieces
    let stitches = &plan.pattern().stitches;
    let mut segs: Vec<((f64, f64), (f64, f64))> = Vec::new();
    let mut last: Option<(f64, f64)> = None;
    for (i, st) in stitches.iter().enumerate() {
        let ours = st.cmd == sf_core::Cmd::Stitch && plan.node_at_record(i).is_some_and(|n| nodes.contains(&n));
        if !ours {
            last = None;
            continue;
        }
        let p = (st.x, st.y);
        segs.push((last.unwrap_or(p), p));
        last = Some(p);
    }
    segs
}

/// The `cols` × `rows` grid `sewn` widened by `r` cells (a disc round every sewn cell).
fn widened(sewn: &[bool], cols: usize, rows: usize, r: i64) -> Vec<bool> {
    let disc: Vec<(i64, i64)> = (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|(dx, dy)| dx * dx + dy * dy <= r * r).collect();
    let mut keep = vec![false; cols * rows];
    for (c, _) in sewn.iter().enumerate().filter(|(_, s)| **s) {
        let (ci, cj) = ((c % cols) as i64, (c / cols) as i64);
        for (dx, dy) in &disc {
            let (i, j) = (ci + dx, cj + dy);
            if i >= 0 && j >= 0 && (i as usize) < cols && (j as usize) < rows {
                keep[j as usize * cols + i as usize] = true;
            }
        }
    }
    keep
}

/// The cells of the `cols` × `rows` grid the outside reaches without crossing a `keep` cell:
/// whatever it can't reach (the holes) is kept clear.
fn reached_from_edge(keep: &[bool], cols: usize, rows: usize) -> Vec<bool> {
    let mut outside = vec![false; cols * rows];
    let mut stack: Vec<usize> = (0..cols)
        .chain((0..cols).map(|i| (rows - 1) * cols + i))
        .chain((0..rows).map(|j| j * cols))
        .chain((0..rows).map(|j| j * cols + cols - 1))
        .collect();
    while let Some(c) = stack.pop() {
        if c >= keep.len() || keep[c] || outside[c] {
            continue;
        }
        outside[c] = true;
        let (i, j) = (c % cols, c / cols);
        if i > 0 {
            stack.push(c - 1);
        }
        if i + 1 < cols {
            stack.push(c + 1);
        }
        if j > 0 {
            stack.push(c - cols);
        }
        if j + 1 < rows {
            stack.push(c + cols);
        }
    }
    outside
}

/// The edges between the cells the outside doesn't reach and those it does, chained into rings
/// in mm (the grid's origin and cell size in `grid`) and smoothed.
fn trace_rings(outside: &[bool], cols: usize, rows: usize, grid: (f64, f64, f64)) -> Vec<Vec<(f64, f64)>> {
    let (ox, oy, cell) = grid;
    let solid = |i: i64, j: i64| i >= 0 && j >= 0 && (i as usize) < cols && (j as usize) < rows && !outside[j as usize * cols + i as usize];
    // the edges between kept and outside cells, chained into rings
    let mut next_of: std::collections::HashMap<(i64, i64), (i64, i64)> = std::collections::HashMap::new();
    for j in 0..rows as i64 {
        for i in 0..cols as i64 {
            if !solid(i, j) {
                continue;
            }
            if !solid(i, j - 1) {
                next_of.insert((i, j), (i + 1, j));
            }
            if !solid(i + 1, j) {
                next_of.insert((i + 1, j), (i + 1, j + 1));
            }
            if !solid(i, j + 1) {
                next_of.insert((i + 1, j + 1), (i, j + 1));
            }
            if !solid(i - 1, j) {
                next_of.insert((i, j + 1), (i, j));
            }
        }
    }
    let mut keys: Vec<(i64, i64)> = next_of.keys().copied().collect();
    keys.sort_unstable();
    let mut used = std::collections::HashSet::new();
    let mut rings = Vec::new();
    for k in keys {
        if used.contains(&k) {
            continue;
        }
        let mut ring = Vec::new();
        let mut cur = k;
        while used.insert(cur) {
            ring.push((ox + cur.0 as f64 * cell, oy + cur.1 as f64 * cell));
            match next_of.get(&cur) {
                Some(&n) => cur = n,
                None => break,
            }
        }
        if ring.len() >= 8 {
            rings.push(smoothed(ring));
        }
    }
    rings
}

/// The ring rounded off (Chaikin), the grid's steps gone.
fn smoothed(mut ring: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    for _ in 0..3 {
        let n = ring.len();
        ring = (0..n)
            .flat_map(|i| {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                [(a.0 * 0.75 + b.0 * 0.25, a.1 * 0.75 + b.1 * 0.25), (a.0 * 0.25 + b.0 * 0.75, a.1 * 0.25 + b.1 * 0.75)]
            })
            .collect();
    }
    ring
}

/// Whether `p` is inside the ring.
fn inside(ring: &[(f64, f64)], p: (f64, f64)) -> bool {
    let mut c = false;
    let n = ring.len();
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + n - 1) % n]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            c = !c;
        }
    }
    c
}

/// The line `pts` kept out of the rings round the art: where it would cross one it goes round the
/// ring's edge instead (the shorter way), echoing the art's outline, and carries on.
fn detour(pts: &[(f64, f64)], rings: &[Vec<(f64, f64)>]) -> Vec<(f64, f64)> {
    // no long jumps across a ring between two points outside it
    let mut dense = Vec::with_capacity(pts.len() * 2);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let n = ((b.0 - a.0).hypot(b.1 - a.1) / 0.8).ceil().clamp(1.0, 10_000.0) as usize;
        dense.extend((0..n).map(|i| (a.0 + (b.0 - a.0) * i as f64 / n as f64, a.1 + (b.1 - a.1) * i as f64 / n as f64)));
    }
    if let Some(&l) = pts.last() {
        dense.push(l);
    }
    let nearest = |ring: &[(f64, f64)], p: (f64, f64)| {
        (0..ring.len()).min_by(|&i, &j| {
            let (a, b) = (ring[i], ring[j]);
            (a.0 - p.0).hypot(a.1 - p.1).total_cmp(&(b.0 - p.0).hypot(b.1 - p.1))
        })
    };
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(dense.len());
    let mut i = 0;
    while i < dense.len() {
        let p = dense[i];
        let Some(ring) = rings.iter().find(|r| r.len() >= 3 && inside(r, p)) else {
            out.push(p);
            i += 1;
            continue;
        };
        // through the ring from here to where the line comes out
        let mut j = i;
        while j < dense.len() && inside(ring, dense[j]) {
            j += 1;
        }
        let from = out.last().copied().unwrap_or(p);
        let to = dense.get(j).copied().unwrap_or(p);
        if let (Some(a), Some(b)) = (nearest(ring, from), nearest(ring, to)) {
            let n = ring.len();
            let fwd = (b + n - a) % n;
            if fwd <= n - fwd {
                out.extend((0..=fwd).map(|k| ring[(a + k) % n]));
            } else {
                out.extend((0..=(n - fwd)).map(|k| ring[(a + n - k) % n]));
            }
        }
        i = j;
    }
    out
}

/// Crosshatch: lines at 45° both ways, `gap` apart, sewn as one line that turns at the edges.
fn crosshatch(w: f64, h: f64, gap: f64) -> Vec<(f64, f64)> {
    let m = gap * 0.5;
    let (x0, y0, x1, y1) = (m, m, w - m, h - m);
    // a line x - y = c (one way) or x + y = c (the other) clipped to the inner box
    let clip = |c: f64, up: bool| -> Option<((f64, f64), (f64, f64))> {
        let mut hits: Vec<(f64, f64)> = Vec::new();
        let on = |x: f64, y: f64| x >= x0 - 1e-9 && x <= x1 + 1e-9 && y >= y0 - 1e-9 && y <= y1 + 1e-9;
        for &x in &[x0, x1] {
            let y = if up { c - x } else { x - c };
            if on(x, y) {
                hits.push((x, y));
            }
        }
        for &y in &[y0, y1] {
            let x = if up { c - y } else { c + y };
            if on(x, y) {
                hits.push((x, y));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        Some((*hits.first()?, *hits.last()?))
    };
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let step = gap * std::f64::consts::SQRT_2;
    for up in [false, true] {
        let (lo, hi) = if up { (x0 + y0, x1 + y1) } else { (x0 - y1, x1 - y0) };
        let n = (((hi - lo) / step).floor() as usize).min(5000);
        for i in 1..=n {
            let c = lo + i as f64 * step;
            if let Some((a, b)) = clip(c, up) {
                // alternate the direction so each line starts where the last one ended
                let (a, b) = if pts.len().is_multiple_of(4) { (a, b) } else { (b, a) };
                pts.push(a);
                pts.push(b);
            }
        }
    }
    pts
}

/// SVG path data for a polyline at the block's place (mm → points, `left`, `top` its corner).
fn path_d(pts: &[(f64, f64)], left: f64, top: f64) -> String {
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, left + x * PT_PER_MM, top + y * PT_PER_MM));
    }
    d
}

fn quilt_block(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.quiltBlock";
    let unit = match str_param(p, "unit").unwrap_or("in") {
        "in" => IN,
        "mm" => 1.0,
        _ => return Err(bad(CMD, "`unit` is in or mm")),
    };
    let dim = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).map(|v| v * unit).filter(|v| (10.0..=600.0).contains(v));
    let (Some(w), Some(h)) = (dim("width"), dim("height")) else {
        return Err(bad(CMD, "`width` and `height` are the block's size: 10 to 600 mm (0.4 to 23 in)"));
    };
    // a corner square where two border strips meet: the smaller side
    let is_corner = bool_or(p, "corner", false);
    let (w, h) = if is_corner { (w.min(h), w.min(h)) } else { (w, h) };
    let text = str_param(p, "text").map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let shape = str_param(p, "shape").map(str::to_string);
    let clearance = match p.get("clearance") {
        None | Some(Value::Null) => 4.0,
        Some(v) => v.as_f64().filter(|v| (1.0..=30.0).contains(v)).ok_or_else(|| bad(CMD, "`clearance` is 1 to 30 mm"))?,
    };
    let pattern = str_param(p, "pattern").unwrap_or("stipple");
    if !PATTERNS.iter().any(|(id, _)| *id == pattern) {
        return Err(bad(CMD, "`pattern` is stipple, swirls, loops, feathers, pebbles, waves, spiral or crosshatch"));
    }
    let gap = match p.get("spacing") {
        None | Some(Value::Null) => 6.0,
        Some(v) => v.as_f64().filter(|v| (2.0..=25.0).contains(v)).ok_or_else(|| bad(CMD, "`spacing` is 2 to 25 mm"))?,
    };
    let stitch = match p.get("stitch") {
        None | Some(Value::Null) => 3.0,
        Some(v) => v.as_f64().filter(|v| (1.5..=5.0).contains(v)).ok_or_else(|| bad(CMD, "`stitch` is 1.5 to 5 mm"))?,
    };
    let color = str_param(p, "color").unwrap_or("#5b6b7a").to_string();
    sf_core::Rgb::from_hex(&color).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?;
    // the hoop: the block plus a little room, turned the block's way round
    let (hoop, fw, fh) = super::embroidery_ith::pick_hoop(s, p, (w + 4.0, h + 4.0), CMD)?;
    let fits = (fw >= w && fh >= h) || (fw >= h && fh >= w);
    if !fits {
        return Err(bad(CMD, "the block is bigger than the hoop's sewing field"));
    }
    let hoop = if (w > h) != (fw > fh) && (w - h).abs() > 1.0 { json!({ "width": fh, "height": fw }) } else { hoop };
    let made = s.execute("embroidery.newDesign", &json!({ "hoop": hoop }))?;
    let c = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    let (left, top) = (c.x - w * PT_PER_MM / 2.0, c.y - h * PT_PER_MM / 2.0);
    // the art in the middle (words, a shape, an appliqué piece), sewn first; the quilting goes
    // round it, `clearance` away
    let mut art: Vec<u64> = Vec::new();
    let both = text.is_some() && shape.is_some();
    let inner = w.min(h);
    if let Some(name) = &shape {
        let size = (inner * if both { 0.45 } else { 0.55 }).max(10.0);
        let r = s.execute("embroidery.addShape", &json!({ "name": name, "size": size, "applique": bool_or(p, "applique", false) }))?;
        if let Some(id) = r["id"].as_u64() {
            if both {
                s.execute("select.set", &json!({ "ids": [id] }))?;
                s.execute("object.move", &json!({ "dx": 0.0, "dy": -inner * 0.15 * PT_PER_MM }))?;
            }
            art.push(id);
        }
    }
    if let Some(t) = &text {
        let height = (inner * if both { 0.12 } else { 0.16 }).clamp(6.0, 40.0);
        let r = s.execute("embroidery.addWords", &json!({ "text": t, "height": height }))?;
        if let Some(id) = r["id"].as_u64() {
            if both {
                s.execute("select.set", &json!({ "ids": [id] }))?;
                s.execute("object.move", &json!({ "dx": 0.0, "dy": inner * 0.28 * PT_PER_MM }))?;
            }
            art.push(id);
        }
    }
    let id = match pattern {
        // round the art: the block's rectangle with the art's outline left out, filled
        "stipple" | "spiral" if !art.is_empty() => {
            // the block with the art's outline left out (even-odd: the rings are holes)
            let mut d = format!(
                "M{:.3} {:.3} L{:.3} {:.3} L{:.3} {:.3} L{:.3} {:.3} Z ",
                left,
                top,
                left + w * PT_PER_MM,
                top,
                left + w * PT_PER_MM,
                top + h * PT_PER_MM,
                left,
                top + h * PT_PER_MM
            );
            for ring in sewn_rings(s, &art, clearance) {
                d.push_str(&path_d(&ring, 0.0, 0.0));
                d.push_str("Z ");
            }
            let r = s.execute("path.create", &json!({ "d": d }))?;
            let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "the block wasn't made"))?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("path.setFillRule", &json!({ "rule": "evenOdd" }))?;
            s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            let kind = if pattern == "stipple" { "Meander" } else { "Spiral" };
            let mut params = json!({ "kind": kind, "spacing": if pattern == "stipple" { gap / 2.0 } else { gap }, "stitch_length": stitch, "underlay": { "kinds": [] } });
            if pattern == "stipple" {
                params["random"] = json!(0.6);
            }
            s.execute("embroidery.setParams", &json!({ "params": params }))?;
            id
        }
        "stipple" | "spiral" => {
            let r = s.execute("shape.rectangle", &json!({ "x": left, "y": top, "width": w * PT_PER_MM, "height": h * PT_PER_MM }))?;
            let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "the block wasn't made"))?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            let kind = if pattern == "stipple" { "Meander" } else { "Spiral" };
            let mut params = json!({ "kind": kind, "spacing": if pattern == "stipple" { gap / 2.0 } else { gap }, "stitch_length": stitch, "underlay": { "kinds": [] } });
            if pattern == "stipple" {
                params["random"] = json!(0.6);
            }
            s.execute("embroidery.setParams", &json!({ "params": params }))?;
            id
        }
        _ => {
            let mut pts = match (pattern, motif(pattern, gap)) {
                ("crosshatch", _) => crosshatch(w, h, gap),
                (_, Some(m)) if is_corner => corner(w, gap, &m),
                (_, Some(m)) => rowed(w, h, gap, &m),
                _ => Vec::new(),
            };
            // round the art: the line goes round its outline instead of over it
            if !art.is_empty() {
                let (lx, ty) = (left / PT_PER_MM, top / PT_PER_MM);
                let rings: Vec<Vec<(f64, f64)>> =
                    sewn_rings(s, &art, clearance).into_iter().map(|r| r.into_iter().map(|(x, y)| (x - lx, y - ty)).collect()).collect();
                pts = detour(&pts, &rings);
            }
            let r = s.execute("path.create", &json!({ "d": path_d(&pts, left, top) }))?;
            let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "the block wasn't made"))?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("embroidery.set", &json!({ "type": "run" }))?;
            s.execute("embroidery.setParams", &json!({ "params": { "stitch_length": stitch } }))?;
            id
        }
    };
    let name = PATTERNS.iter().find(|(i, _)| *i == pattern).map_or("Quilting", |(_, n)| n);
    s.execute("embroidery.setThread", &json!({ "color": color, "name": format!("Quilting: {name}"), "ids": [id], "paint": false }))?;
    let notes = format!(
        "Quilt {} {} × {} in, {name}, lines {gap:.0} mm apart.\nLay the tile on the quilt and mark its middle, crosshairs, up arrow, sides and corners; hoop the quilt sandwich with the marks centred and the arrow up; quilt; spritz the marks away.",
        if is_corner { "corner" } else { "block" },
        num(w / IN),
        num(h / IN)
    );
    s.execute("embroidery.settings", &json!({ "notes": notes }))?;
    s.execute("select.none", &json!({}))?;
    let info = s.execute("embroidery.info", &json!({}))?;
    Ok(json!({ "pattern": pattern, "widthMm": w, "heightMm": h, "hoop": made["hoop"], "stitches": info["stitches"] }))
}

fn quilt_set(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.quiltBlock.set";
    // no folder (the web app): the files come back in one zip to download
    let folder = str_param(p, "folder").map(str::to_string);
    let pattern = str_param(p, "pattern").unwrap_or("stipple").to_string();
    let name = PATTERNS
        .iter()
        .find(|(i, _)| *i == pattern)
        .map(|(_, n)| *n)
        .ok_or_else(|| bad(CMD, "`pattern` is stipple, swirls, loops, feathers, pebbles, waves, spiral or crosshatch"))?;
    let kind = str_param(p, "kind").unwrap_or("blocks");
    let (borders, corners) = match kind {
        "blocks" => (false, false),
        "borders" => (true, false),
        "corners" => (false, true),
        _ => return Err(bad(CMD, "`kind` is blocks, borders or corners")),
    };
    let sizes: Vec<(f64, f64)> = match p.get("sizes").and_then(Value::as_array) {
        Some(a) => a.iter().take(60).filter_map(|v| Some((v["width"].as_f64()?, v["height"].as_f64()?))).collect(),
        None if borders => BORDER_SIZES.to_vec(),
        // a corner for each border strip's width
        None if corners => {
            let mut v: Vec<(f64, f64)> = BORDER_SIZES.iter().map(|&(_, h)| (h, h)).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v.dedup();
            v
        }
        None => SIZES.to_vec(),
    };
    if sizes.is_empty() {
        return Err(bad(CMD, "`sizes` is a list of {width, height} in inches"));
    }
    let mut files = Vec::new();
    let mut zipped: Vec<(String, Vec<u8>)> = Vec::new();
    let mut skipped = Vec::new();
    for (w, h) in sizes {
        let mut q = json!({ "width": w, "height": h, "pattern": pattern, "corner": corners });
        for k in ["spacing", "stitch", "color"] {
            if let Some(v) = p.get(k).filter(|v| !v.is_null()) {
                q[k] = v.clone();
            }
        }
        match s.execute("embroidery.quiltBlock", &q) {
            Ok(_) => {
                let file = format!(
                    "{}{name} {}x{} in",
                    if borders {
                        "Border "
                    } else if corners {
                        "Corner "
                    } else {
                        ""
                    },
                    num(w),
                    num(h)
                );
                let format = p.get("format").filter(|v| !v.is_null());
                if let Some(folder) = &folder {
                    let mut send = json!({ "folder": folder, "name": file });
                    if let Some(f) = format {
                        send["format"] = f.clone();
                    }
                    let r = s.execute("embroidery.send", &send)?;
                    files.push(r["path"].clone());
                } else {
                    let mut q = json!({ "center": true, "name": file });
                    if let Some(f) = format {
                        q["format"] = f.clone();
                    }
                    let r = s.execute("embroidery.export", &q)?;
                    let bytes = r["dataBase64"]
                        .as_str()
                        .and_then(vectorcraft_format::base64_decode)
                        .ok_or_else(|| bad(CMD, "the machine file didn't encode"))?;
                    let name = format!("{}.{}", super::embroidery_lettering::safe_name(&file), r["format"].as_str().unwrap_or("dst"));
                    files.push(json!(name));
                    zipped.push((name, bytes));
                }
                // the block's document is done with
                // (closing can only fail without a document, and then there is none to close)
                s.execute("file.close", &json!({})).ok();
            }
            Err(e) => skipped.push(json!({ "width": w, "height": h, "reason": e.to_string() })),
        }
    }
    if folder.is_some() || zipped.is_empty() {
        return Ok(json!({ "files": files, "skipped": skipped }));
    }
    let list: Vec<(String, &[u8])> = zipped.iter().map(|(n, b)| (n.clone(), b.as_slice())).collect();
    let zip = super::package::zip(&list)?;
    let what = if borders {
        "border strips"
    } else if corners {
        "corners"
    } else {
        "blocks"
    };
    Ok(
        json!({ "files": files, "skipped": skipped, "name": format!("{name} quilt {what}.zip"), "dataBase64": vectorcraft_format::base64_encode(&zip) }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T1: a block of each pattern fills its rectangle edge to edge with one thread, in a hoop it
    /// fits, its lines staying inside the block.
    #[test]
    fn each_pattern_quilts_the_whole_block_in_one_thread() {
        let mut s = Session::new();
        s.execute("embroidery.myMachine.set", &json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
        for (pattern, _) in PATTERNS {
            let r = s
                .execute("embroidery.quiltBlock", &json!({"width": 5, "height": 7, "pattern": pattern}))
                .unwrap_or_else(|e| panic!("{pattern}: {e}"));
            let info = s.execute("embroidery.info", &json!({})).unwrap();
            assert_eq!(info["fits"], true, "{pattern}: {info}");
            assert_eq!(info["colors"].as_array().unwrap().len(), 1, "{pattern}: one thread");
            // covers the block: within a line spacing of its size, never beyond it
            let (w, h) = (info["widthMm"].as_f64().unwrap(), info["heightMm"].as_f64().unwrap());
            assert!(w <= 5.0 * IN + 1.0 && h <= 7.0 * IN + 1.0, "{pattern}: {w} × {h}");
            assert!(w >= 5.0 * IN - 13.0 && h >= 7.0 * IN - 13.0, "{pattern}: {w} × {h}");
            assert!(r["stitches"].as_u64().unwrap() > 200, "{pattern}: {r}");
            // one continuous line: at most the trim that ends the design
            assert!(info["trims"].as_u64().unwrap_or(0) <= 1, "{pattern}: one continuous line, {info}");
        }
        // a wide block turns the hoop
        s.execute("embroidery.quiltBlock", &json!({"width": 8, "height": 4})).unwrap();
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert!(info["widthMm"].as_f64().unwrap() > info["heightMm"].as_f64().unwrap());
        for bad in [
            json!({}),
            json!({"width": 5}),
            json!({"width": 5, "height": 5, "pattern": "plaid"}),
            json!({"width": 50, "height": 50}),
            json!({"width": 5, "height": 5, "spacing": 0.1}),
        ] {
            assert!(s.execute("embroidery.quiltBlock", &bad).is_err(), "{bad}");
        }
    }

    /// T6: a corner bends a row pattern round a quarter circle inside its square; quilting round
    /// words or a shape keeps clear of them and is still one line.
    #[test]
    fn corners_turn_and_quilting_goes_round_the_art() {
        let mut s = Session::new();
        s.execute("embroidery.myMachine.set", &json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
        for pattern in ["swirls", "loops", "feathers", "pebbles", "waves", "stipple"] {
            s.execute("embroidery.quiltBlock", &json!({"width": 3, "height": 3, "corner": true, "pattern": pattern}))
                .unwrap_or_else(|e| panic!("{pattern}: {e}"));
            let info = s.execute("embroidery.info", &json!({})).unwrap();
            assert!(info["widthMm"].as_f64().unwrap() <= 3.0 * IN + 0.5 && info["heightMm"].as_f64().unwrap() <= 3.0 * IN + 0.5, "{pattern}: {info}");
            // a quarter circle half the corner across: a short line
            assert!(info["stitches"].as_u64().unwrap() > 20, "{pattern}: {info}");
        }
        // the bent rows start on the right edge and end on the bottom or right edge, nested
        let m = motif("waves", 6.0).unwrap();
        let pts = corner(76.2, 6.0, &m);
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        assert!((first.0 - 73.2).abs() < 1.0, "starts on the right edge: {first:?}");
        assert!((last.0 - 73.2).abs() < 1.0 || (last.1 - 73.2).abs() < 1.0, "ends on an edge: {last:?}");
        // reaching well into the square, not just round its inner corner
        assert!(pts.iter().any(|p| p.0 < 25.0 && p.1 < 40.0), "fills the corner");
        // round the art: every pattern keeps clear of the words and shape, in one line
        for (pattern, _) in PATTERNS {
            s.execute(
                "embroidery.quiltBlock",
                &json!({"width": 6, "height": 5, "pattern": pattern, "text": "Noel", "shape": "Star", "clearance": 4}),
            )
            .unwrap_or_else(|e| panic!("{pattern}: {e}"));
            let info = s.execute("embroidery.info", &json!({})).unwrap();
            let names: Vec<&str> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect();
            assert!(names.iter().any(|n| n.starts_with("Quilting")), "{pattern}: {names:?}");
            assert!(info["fits"] == true, "{pattern}: {info}");
        }
        // the detour itself: a straight line across a square ring goes round it
        let ring = vec![(40.0, 40.0), (60.0, 40.0), (60.0, 60.0), (40.0, 60.0)];
        let out = detour(&[(0.0, 50.0), (100.0, 50.0)], std::slice::from_ref(&ring));
        assert!(out.iter().all(|&p| !inside(&ring, (p.0 + 0.0, p.1)) || [40.0, 60.0].contains(&p.0) || [40.0, 60.0].contains(&p.1)), "{out:?}");
        assert!(out.iter().any(|&p| (p.1 - 40.0).abs() < 1e-9 || (p.1 - 60.0).abs() < 1e-9), "goes round the edge: {out:?}");
        assert!((out[out.len() - 1].0 - 100.0).abs() < 1e-9, "and carries on to the end");
    }

    /// T2: a set writes one file per size, named by its size; sizes no hoop takes are skipped.
    #[test]
    fn a_set_writes_a_file_per_block_size() {
        let dir = std::env::temp_dir().join(format!("sf-quilt-set-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::new();
        s.execute("embroidery.myMachine.set", &json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
        let r = s
            .execute(
                "embroidery.quiltBlock.set",
                &json!({"folder": dir.to_string_lossy(), "pattern": "waves", "sizes": [{"width": 4, "height": 4}, {"width": 5, "height": 7}, {"width": 30, "height": 30}]}),
            )
            .unwrap();
        let files: Vec<String> = r["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_string()).collect();
        assert_eq!(files.len(), 2, "{r}");
        assert!(files.iter().any(|f| f.ends_with("Waves 4x4 in.vp3")) && files.iter().any(|f| f.ends_with("Waves 5x7 in.vp3")), "{files:?}");
        assert!(files.iter().all(|f| std::fs::metadata(f).unwrap().len() > 100));
        assert_eq!(r["skipped"].as_array().unwrap().len(), 1, "{r}");
        // border strips: their own sizes and names, every pattern squeezed into a narrow strip
        let b = s.execute("embroidery.quiltBlock.set", &json!({"folder": dir.to_string_lossy(), "pattern": "swirls", "kind": "borders"})).unwrap();
        let files: Vec<&str> = b["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
        assert_eq!(files.len(), BORDER_SIZES.len(), "{b}");
        assert!(files.iter().any(|f| f.ends_with("Border Swirls 7x1.5 in.vp3")), "{files:?}");
        let c = s.execute("embroidery.quiltBlock.set", &json!({"folder": dir.to_string_lossy(), "pattern": "loops", "kind": "corners"})).unwrap();
        let corners: Vec<&str> = c["files"].as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
        assert!(corners.iter().any(|f| f.ends_with("Corner Loops 2x2 in.vp3")), "{corners:?}");
        // no folder (the web app): one zip holding a file per size, nothing written
        let z = s
            .execute("embroidery.quiltBlock.set", &json!({"pattern": "pebbles", "sizes": [{"width": 4, "height": 4}, {"width": 5, "height": 5}]}))
            .unwrap();
        assert_eq!(z["name"], "Pebbles quilt blocks.zip", "{z}");
        assert_eq!(z["files"], json!(["Pebbles 4x4 in.vp3", "Pebbles 5x5 in.vp3"]), "{z}");
        let zip = vectorcraft_format::base64_decode(z["dataBase64"].as_str().unwrap()).unwrap();
        assert!(zip.starts_with(b"PK") && zip.len() > 500, "{}", zip.len());
        for (pattern, _) in PATTERNS {
            s.execute("embroidery.quiltBlock", &json!({"width": 7, "height": 1, "pattern": pattern})).unwrap_or_else(|e| panic!("{pattern}: {e}"));
            let info = s.execute("embroidery.info", &json!({})).unwrap();
            assert!(info["heightMm"].as_f64().unwrap() <= IN + 0.5, "{pattern}: {info}");
            assert!(info["stitches"].as_u64().unwrap() > 50, "{pattern}: {info}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
