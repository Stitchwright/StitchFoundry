//! Stitches to objects (SE-06): the stitches of an opened machine file (manual stitches, one
//! path a thread) recognised as runs, satin columns and tatami fills from the way the needle
//! moves, and made into ordinary objects of those types in the same thread, sewn in the same
//! order, to edit, resize and sew again with their stitches regenerated.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::direction::Guide;
use sf_core::model::{FillParams, RunParams};
use vectorcraft_doc::{NodeId, NodeKind};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::path::{Anchor, PathData, SubPath};

use super::{CommandSpec, EngineError, Result, Session, bad, cmd, has_doc};

/// Stitches shorter than this (mm) are steps between rows or ties: they don't turn a stroke.
const STEP: f64 = 0.5;
pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.stitchesToObjects",
        "Convert Stitches to Objects",
        ["Embroidery"],
        None,
        "{ids?: manual-stitch objects (an opened machine file's threads; the selected ones, all of them with nothing selected)} recognise the stitches as runs (the needle following a line), satin columns (it swings from side to side every stitch) and tatami fills (rows of stitches back and forth), and make them objects of those types in the same thread, in the same order, in the manual stitches' place; one undo step → {ids, runs, satins, fills}",
        has_doc,
        convert
    )]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Run,
    Satin,
    Fill,
}

/// A stretch of stitches the needle sews without turning back: stitches `from..to` (stitch `i`
/// goes from point `i` to `i + 1`).
struct Stroke {
    from: usize,
    to: usize,
    /// Its stitches longer than [`STEP`].
    long: usize,
    length: f64,
    chord: sf_core::Point,
    start: sf_core::Point,
}

impl Stroke {
    fn straight(&self) -> f64 {
        if self.length > 1e-9 { self.chord.x.hypot(self.chord.y) / self.length } else { 0.0 }
    }
}

/// The run of stitches through `pts` (millimetres) cut where the needle turns back.
fn strokes(pts: &[sf_core::Point]) -> Vec<Stroke> {
    let mut out: Vec<Stroke> = Vec::new();
    let mut last: Option<sf_core::Point> = None;
    for i in 0..pts.len().saturating_sub(1) {
        let (Some(a), Some(b)) = (pts.get(i), pts.get(i + 1)) else { continue };
        let v = *b - *a;
        let l = v.x.hypot(v.y);
        let back = l >= STEP && last.is_some_and(|u| (u.x * v.x + u.y * v.y) / (u.x.hypot(u.y) * l).max(1e-12) < -0.7);
        if back || out.is_empty() {
            out.push(Stroke { from: i, to: i, long: 0, length: 0.0, chord: sf_core::Point::new(0.0, 0.0), start: *a });
        }
        if let Some(s) = out.last_mut() {
            s.to = i + 1;
            s.length += l;
            s.chord = sf_core::Point::new(s.chord.x + v.x, s.chord.y + v.y);
            if l >= STEP {
                s.long += 1;
            }
        }
        if l >= STEP {
            last = Some(v);
        }
    }
    out
}

/// What each stroke is: satin where the needle swings once side to side stroke after stroke (4 or
/// more), fill where straight strokes of several stitches lie side by side (3 or more rows), else run.
fn classify(strokes: &[Stroke]) -> Vec<Kind> {
    let n = strokes.len();
    let mut kinds = vec![Kind::Run; n];
    let swing = |s: &Stroke| s.long == 1 && (0.6..=15.0).contains(&s.length);
    let row = |s: &Stroke| s.long >= 2 && s.straight() > 0.95 && s.length >= 2.5;
    let angle = |s: &Stroke| s.chord.y.atan2(s.chord.x);
    let mut i = 0;
    while i < n {
        let Some(first) = strokes.get(i) else { break };
        let mut j = i;
        if swing(first) {
            while strokes.get(j).is_some_and(swing) {
                j += 1;
            }
            if j - i >= 4 {
                for k in kinds.get_mut(i..j).into_iter().flatten() {
                    *k = Kind::Satin;
                }
                i = j;
                continue;
            }
        } else if row(first) {
            // rows parallel to the first (either way) and close beside the one before
            let axis = angle(first);
            while let Some(s) = strokes.get(j) {
                let turn = (angle(s) - axis).rem_euclid(std::f64::consts::PI);
                let parallel = turn.min(std::f64::consts::PI - turn) < 12f64.to_radians();
                // the row's middle within 3 mm of the line of the row before
                let near = j == i
                    || strokes.get(j - 1).is_some_and(|p| {
                        let mid = sf_core::Point::new(s.start.x + s.chord.x / 2.0, s.start.y + s.chord.y / 2.0);
                        let l = p.chord.x.hypot(p.chord.y).max(1e-9);
                        ((mid.x - p.start.x) * p.chord.y - (mid.y - p.start.y) * p.chord.x).abs() / l < 3.0
                    });
                if !(row(s) && parallel && near) {
                    break;
                }
                j += 1;
            }
            if j - i >= 3 {
                for k in kinds.get_mut(i..j).into_iter().flatten() {
                    *k = Kind::Fill;
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    kinds
}

/// The pieces of a run of stitches: their kind and the needle points (millimetres) each holds.
fn pieces(pts: &[sf_core::Point]) -> Vec<(Kind, Vec<sf_core::Point>)> {
    let st = strokes(pts);
    let kinds = classify(&st);
    let mut out: Vec<(Kind, Vec<sf_core::Point>)> = Vec::new();
    for (s, k) in st.iter().zip(&kinds) {
        let span = pts.get(s.from..=s.to).unwrap_or_default();
        match out.last_mut() {
            Some((last, v)) if last == k => v.extend(span.iter().skip(1)),
            _ => out.push((*k, span.to_vec())),
        }
    }
    // a run of a stitch or two between two pieces of one kind joins them
    let mut merged: Vec<(Kind, Vec<sf_core::Point>)> = Vec::new();
    let mut it = out.into_iter().peekable();
    while let Some((k, v)) = it.next() {
        let tiny = k == Kind::Run && v.len() <= 3;
        let same = merged.last().map(|m| m.0) == it.peek().map(|p| p.0) && merged.last().is_some_and(|m| m.0 != Kind::Run);
        if tiny && same {
            if let (Some(last), Some((_, next))) = (merged.last_mut(), it.next()) {
                last.1.extend(v.into_iter().skip(1));
                last.1.extend(next.into_iter().skip(1));
            }
            continue;
        }
        match merged.last_mut() {
            Some(m) if m.0 == k => m.1.extend(v.into_iter().skip(1)),
            _ => merged.push((k, v)),
        }
    }
    merged
}

fn to_pt(p: sf_core::Point) -> kurbo::Point {
    kurbo::Point::new(p.x * PT_PER_MM, p.y * PT_PER_MM)
}

fn polyline(pts: &[sf_core::Point]) -> SubPath {
    SubPath::new(pts.iter().map(|p| Anchor::corner(to_pt(*p))).collect(), false)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite());
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

/// A recognised piece as an object: its path and how it is sewn.
fn object(kind: Kind, pts: &[sf_core::Point]) -> Option<(PathData, Embroidery)> {
    let lens: Vec<f64> = pts.windows(2).filter_map(|w| Some(w.first()?.distance(*w.get(1)?))).collect();
    match kind {
        Kind::Run => {
            let mut e = Embroidery::run();
            if let StitchType::Run(r) = &mut e.stitch {
                *r = RunParams { stitch_length: median(lens).clamp(1.0, 6.0), ..r.clone() };
            }
            Some((PathData::new(vec![polyline(pts)]), e))
        }
        Kind::Satin => {
            // the needle swings between the column's two sides
            let a: Vec<sf_core::Point> = pts.iter().step_by(2).copied().collect();
            let b: Vec<sf_core::Point> = pts.iter().skip(1).step_by(2).copied().collect();
            if a.len() < 2 || b.len() < 2 {
                return None;
            }
            let mut e = Embroidery::satin();
            e.guides = a.iter().zip(&b).step_by(6).map(|(p, q)| Guide::new(*p, *q)).collect();
            Some((PathData::new(vec![polyline(&a), polyline(&b)]), e))
        }
        Kind::Fill => {
            // the rows' axis, then each 1 mm band across the rows from its first to its last needle point
            let (c, s) = pts.windows(2).fold((0.0, 0.0), |(c, s), w| {
                let (Some(p), Some(q)) = (w.first(), w.get(1)) else { return (c, s) };
                let v = *q - *p;
                let l = v.x.hypot(v.y);
                let a = v.y.atan2(v.x) * 2.0;
                (c + a.cos() * l, s + a.sin() * l)
            });
            let axis = s.atan2(c) / 2.0;
            let (ux, uy) = (axis.cos(), axis.sin());
            let along = |p: &sf_core::Point| p.x * ux + p.y * uy;
            let across = |p: &sf_core::Point| -p.x * uy + p.y * ux;
            let mut bands: std::collections::BTreeMap<i64, (f64, f64)> = std::collections::BTreeMap::new();
            for p in pts {
                let (t, u) = (along(p), across(p));
                if !(t.is_finite() && u.is_finite()) {
                    continue;
                }
                let e = bands.entry(u.floor() as i64).or_insert((t, t));
                e.0 = e.0.min(t);
                e.1 = e.1.max(t);
            }
            if bands.len() < 2 {
                return None;
            }
            let back = |t: f64, u: f64| sf_core::Point::new(t * ux - u * uy, t * uy + u * ux);
            let mut ring: Vec<sf_core::Point> = bands.iter().map(|(u, (lo, _))| back(*lo, *u as f64 + 0.5)).collect();
            ring.extend(bands.iter().rev().map(|(u, (_, hi))| back(*hi, *u as f64 + 0.5)));
            let mut sub = polyline(&ring);
            sub.closed = true;
            let rows = pts.windows(3).filter(|w| {
                let (Some(a), Some(b), Some(c)) = (w.first(), w.get(1), w.get(2)) else { return false };
                let (v, x) = (*b - *a, *c - *b);
                v.x * x.x + v.y * x.y < 0.0
            });
            let reversals = rows.count().max(1) as f64;
            let width = bands.keys().next_back().zip(bands.keys().next()).map_or(1.0, |(hi, lo)| (hi - lo + 1) as f64);
            let fill = FillParams {
                angle: (-axis.to_degrees()).rem_euclid(180.0),
                spacing: (width / reversals).clamp(0.25, 2.0),
                stitch_length: median(lens).clamp(2.0, 7.0),
                ..FillParams::default()
            };
            Some((PathData::new(vec![sub]), Embroidery::new(StitchType::Fill(fill))))
        }
    }
}

fn convert(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.stitchesToObjects";
    let d = &s.doc()?.doc;
    let manual = |n: &vectorcraft_doc::Node| n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, StitchType::Manual) && e.frozen.is_none());
    let everything = p.get("ids").is_none() && s.doc()?.selection.objects.is_empty();
    let ids: Vec<NodeId> = if everything {
        vectorcraft_embroidery::stitched_nodes(d).iter().filter(|n| manual(n)).map(|n| n.id).collect()
    } else {
        super::embroidery::stitched_targets(s, p)?.into_iter().filter(|id| d.node(*id).is_some_and(&manual)).collect()
    };
    if ids.is_empty() {
        return Err(bad(CMD, "select manual stitches: an opened machine file's threads"));
    }
    // each manual object's runs of stitches, recognised piece by piece
    let mut plans: Vec<(NodeId, Vec<vectorcraft_doc::Node>)> = Vec::new();
    let (mut runs, mut satins, mut fills) = (0usize, 0usize, 0usize);
    for id in &ids {
        let Some(n) = d.node(*id) else { continue };
        let NodeKind::Path { path, .. } = &n.kind else { continue };
        let thread = n.embroidery.as_deref().and_then(|e| e.thread.clone());
        let mut made = Vec::new();
        for sub in &path.subpaths {
            let pts: Vec<sf_core::Point> = sub
                .anchors
                .iter()
                .map(|a| sf_core::Point::new(a.p.x / PT_PER_MM, a.p.y / PT_PER_MM))
                .filter(|q| q.x.is_finite() && q.y.is_finite())
                .collect();
            if pts.len() < 2 {
                continue;
            }
            for (kind, piece) in pieces(&pts) {
                let Some((path, mut e)) = object(kind, &piece) else { continue };
                e.thread = thread.clone();
                let mut node = n.clone();
                node.kind = NodeKind::Path { path, rule: vectorcraft_geom::FillRule::NonZero, live: None, clipping: false, guide: false };
                if kind == Kind::Fill {
                    let paint = n.appearance.stroke_paint();
                    node.appearance = vectorcraft_doc::Appearance::basic(paint, vectorcraft_color::Paint::None, 0.0).into();
                }
                node.embroidery = Some(Box::new(e));
                node.name = Some(
                    match kind {
                        Kind::Run => "Run",
                        Kind::Satin => "Satin",
                        Kind::Fill => "Fill",
                    }
                    .to_string(),
                );
                match kind {
                    Kind::Run => runs += 1,
                    Kind::Satin => satins += 1,
                    Kind::Fill => fills += 1,
                }
                made.push(node);
            }
        }
        if !made.is_empty() {
            plans.push((*id, made));
        }
    }
    if plans.is_empty() {
        return Err(bad(CMD, "no stitches to recognise"));
    }
    let made = s.edit("Convert Stitches to Objects", |d, sel| {
        let mut all = Vec::new();
        for (id, nodes) in plans {
            let (parent, idx, _) = d.position(id).ok_or(EngineError::NoNode(id))?;
            d.remove(id)?;
            for (k, mut n) in nodes.into_iter().enumerate() {
                n.id = d.alloc_id();
                all.push(n.id);
                d.insert(parent, idx + k, n)?;
            }
        }
        sel.set(all.iter().copied());
        Ok(all)
    })?;
    let ids: Vec<u64> = made.iter().map(|i| i.0).collect();
    Ok(json!({ "ids": ids, "runs": runs, "satins": satins, "fills": fills }))
}
