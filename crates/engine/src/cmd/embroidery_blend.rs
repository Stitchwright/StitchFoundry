//! Colour blends (`embroidery.makeBlend`, Multi Blend): a tatami fill sewn as stacked layers,
//! one per colour, each densest across its own band and fading to nothing at the next colour's,
//! so neighbouring colours blend across the rows. The layers live in the one object
//! (`FillParams::blend`), or with `separate` become objects of their own.

use serde_json::{Value, json};
use sf_core::attr::StitchType;
use sf_core::model::{FillKind, FillUnderlay};
use sf_core::threads::{Rgb, Thread};
use vectorcraft_color::Paint;
use vectorcraft_doc::{Document, NodeId};

use super::{CommandSpec, Result, Session, bad, cmd, color_value, has_selection};
use crate::EngineError;

const CMD: &str = "embroidery.makeBlend";
/// Most colours one blend takes.
const MAX_COLORS: usize = 6;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.gradientBlend",
            "Blend from Gradient",
            [],
            None,
            "{ids?, spacing?: mm (the densest rows; default the fill's)} sew each selected tatami fill painted with a linear gradient as a Multi Blend of the gradient's colours (up to six; more are sampled evenly): the rows turned across the gradient, each colour densest where its stop is and fading to nothing at the next stop's, so the stitches blend the way the gradient does; the art keeps its gradient → {ids}",
            has_selection,
            gradient_blend
        ),
        cmd!(
            "embroidery.makeBlend",
            "Blend Colors",
            [],
            None,
            "{colors: [\"#rrggbb\", …] (2 to 6, first to last across the rows), ids?, spacing?: mm (the densest rows; default the fill's), separate?: bool (false: the layers stay in the one object, its blend: setParams blend [] takes it out; true: one object per layer)} sew each selected tatami fill as stacked layers, one per colour, each densest across its own band and fading to nothing at the next colour's, so the colours blend across the rows (turn the fill's angle to turn the blend) → {ids}",
            has_selection,
            make_blend
        ),
    ]
}

/// Whether an object is stitched as a tatami fill.
fn tatami(d: &Document, id: NodeId) -> bool {
    d.node(id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e| matches!(&e.stitch, StitchType::Fill(f) if f.kind == FillKind::Tatami))
}

/// The spacing profile of layer `k` of `n`: densest (`dense` mm) at its own colour's place
/// across the rows, no rows from the next colours' places on.
fn layer_profile(k: usize, n: usize, dense: f64) -> Vec<[f64; 2]> {
    let t = |j: usize| j as f64 / (n - 1).max(1) as f64;
    let mut p = Vec::with_capacity(3);
    if k > 0 {
        p.push([t(k - 1), 0.0]);
    }
    p.push([t(k), dense]);
    if k + 1 < n {
        p.push([t(k + 1), 0.0]);
    }
    p
}

/// [`layer_profile`] with the colours at places `ts` (0 to 1, increasing) across the rows.
fn layer_profile_at(ts: &[f64], k: usize, dense: f64) -> Vec<[f64; 2]> {
    let mut p = Vec::with_capacity(3);
    if let Some(&t) = k.checked_sub(1).and_then(|j| ts.get(j)) {
        p.push([t, 0.0]);
    }
    if let Some(&t) = ts.get(k) {
        p.push([t, dense]);
    }
    if let Some(&t) = ts.get(k + 1) {
        p.push([t, 0.0]);
    }
    p
}

/// A linear gradient's colours as (place 0 to 1, colour): its stops in order, the same colour
/// twice running kept once, at most [`MAX_COLORS`] (more are sampled evenly).
fn gradient_colors(g: &vectorcraft_color::Gradient) -> Vec<(f64, vectorcraft_color::Color)> {
    let mut stops: Vec<(f64, vectorcraft_color::Color)> =
        g.stops.iter().filter(|st| st.offset.is_finite()).map(|st| (f64::from(st.offset.clamp(0.0, 1.0)), st.color)).collect();
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    let hex = |c: &vectorcraft_color::Color| c.to_rgba8(1.0);
    stops.dedup_by(|b, a| hex(&a.1) == hex(&b.1));
    if stops.len() > MAX_COLORS {
        let n = MAX_COLORS - 1;
        stops = (0..=n).map(|k| k as f64 / n as f64).map(|t| (t, g.sample(t as f32).0)).collect();
    }
    stops
}

fn gradient_blend(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.gradientBlend";
    let spacing = match p.get("spacing") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|v| (0.2..=5.0).contains(v)).ok_or_else(|| bad(CMD, "`spacing` is 0.2 to 5 mm"))?),
    };
    let d = &s.doc()?.doc;
    // each tatami fill painted with a linear gradient: its colours and the gradient's direction
    let mut jobs = Vec::new();
    for id in super::embroidery::stitched_targets(s, p)?.into_iter().filter(|id| tatami(d, *id)) {
        let Some(node) = d.node(id) else { continue };
        let Paint::Gradient(g) = node.appearance.fill_paint() else { continue };
        if g.gradient.kind != vectorcraft_color::GradientKind::Linear {
            continue;
        }
        let stops = gradient_colors(&g.gradient);
        if stops.len() < 2 {
            continue;
        }
        // the way the gradient runs, counter-clockwise on screen (y down)
        let angle = match &g.geom {
            Some(geom) if (geom.end - geom.start).hypot() > 1e-9 => {
                let v = geom.end - geom.start;
                (-v.y).atan2(v.x).to_degrees()
            }
            _ => g.angle,
        };
        if angle.is_finite() {
            jobs.push((id, stops, angle));
        }
    }
    if jobs.is_empty() {
        return Err(bad(CMD, "select tatami fills painted with a linear gradient"));
    }
    let ids: Vec<NodeId> = jobs.iter().map(|j| j.0).collect();
    s.edit("Blend from Gradient", |d, sel| {
        for (id, stops, angle) in &jobs {
            let node = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            let Some(e) = node.embroidery.as_deref_mut() else { continue };
            // rows across the gradient: they stack the way it runs (the angle set, not picked)
            e.auto_direction = false;
            let StitchType::Fill(f) = &mut e.stitch else { continue };
            let dense = spacing.unwrap_or(f.spacing).clamp(0.2, 5.0);
            f.angle = (angle + 90.0).rem_euclid(360.0);
            f.spacing_profile.clear();
            f.spacing_end = 0.0;
            let ts: Vec<f64> = stops.iter().map(|(t, _)| *t).collect();
            f.blend = stops
                .iter()
                .enumerate()
                .map(|(k, (_, c))| {
                    let v = c.to_rgba8(1.0);
                    let rgb = Rgb::new(v[0], v[1], v[2]);
                    sf_core::model::BlendLayer {
                        thread: Thread { color: rgb, name: rgb.to_hex(), code: String::new(), chart: String::new() },
                        spacing_profile: layer_profile_at(&ts, k, dense),
                    }
                })
                .collect();
            if let Some(first) = f.blend.first() {
                e.thread = Some(first.thread.clone());
            }
        }
        sel.set(ids.iter().copied());
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn make_blend(s: &mut Session, p: &Value) -> Result<Value> {
    let list = p.get("colors").and_then(Value::as_array).ok_or_else(|| bad(CMD, "missing `colors`: two to six \"#rrggbb\" colours"))?;
    if !(2..=MAX_COLORS).contains(&list.len()) {
        return Err(bad(CMD, format!("a blend takes 2 to {MAX_COLORS} colours, not {}", list.len())));
    }
    let colors = list.iter().map(|v| color_value(v).ok_or_else(|| bad(CMD, format!("not a colour: {v}")))).collect::<Result<Vec<_>>>()?;
    let spacing = match p.get("spacing") {
        None => None,
        Some(v) => Some(v.as_f64().filter(|v| (0.2..=5.0).contains(v)).ok_or_else(|| bad(CMD, "`spacing` is 0.2 to 5 mm"))?),
    };
    let separate = match p.get("separate") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| bad(CMD, "`separate` is true or false"))?,
    };
    let d = &s.doc()?.doc;
    let ids: Vec<NodeId> = super::embroidery::stitched_targets(s, p)?.into_iter().filter(|id| tatami(d, *id)).collect();
    if ids.is_empty() {
        return Err(bad(CMD, "select tatami fills to blend"));
    }
    let threads: Vec<(Thread, vectorcraft_color::Color)> = colors
        .iter()
        .map(|c| {
            let v = c.to_rgba8(1.0);
            let rgb = Rgb::new(v[0], v[1], v[2]);
            (Thread { color: rgb, name: rgb.to_hex(), code: String::new(), chart: String::new() }, *c)
        })
        .collect();
    let n = threads.len();
    if !separate {
        // one object: the layers are its blend, its thread and colour the first's
        s.edit("Blend Colors", |d, sel| {
            for id in &ids {
                let node = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
                let Some(e) = node.embroidery.as_deref_mut() else { continue };
                let StitchType::Fill(f) = &mut e.stitch else { continue };
                let dense = spacing.unwrap_or(f.spacing).clamp(0.2, 5.0);
                f.blend = threads
                    .iter()
                    .enumerate()
                    .map(|(k, (thread, _))| sf_core::model::BlendLayer { thread: thread.clone(), spacing_profile: layer_profile(k, n, dense) })
                    .collect();
                if let Some((thread, color)) = threads.first() {
                    e.thread = Some(thread.clone());
                    node.appearance.set_fill(Paint::solid(*color));
                }
            }
            sel.set(ids.iter().copied());
            Ok(())
        })?;
        return Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }));
    }
    let made = s.edit("Blend Colors", |d, sel| {
        let mut made = Vec::new();
        for id in &ids {
            let (parent, index, _) = d.position(*id).ok_or(EngineError::NoNode(*id))?;
            let base = d.node(*id).ok_or(EngineError::NoNode(*id))?.clone();
            for (k, (thread, color)) in threads.iter().enumerate() {
                // The fill itself is the first layer; the others are copies stacked above it.
                let mut layer = if k == 0 { base.clone() } else { d.reid(&base) };
                let Some(e) = layer.embroidery.as_deref_mut() else { continue };
                let StitchType::Fill(f) = &mut e.stitch else { continue };
                let dense = spacing.unwrap_or(f.spacing).clamp(0.2, 5.0);
                f.spacing_profile = layer_profile(k, n, dense);
                f.spacing_end = 0.0;
                if k > 0 {
                    // Underlay once, under the first layer.
                    f.underlay = FillUnderlay { edge_run: false, tatami: false, ..f.underlay.clone() };
                    e.entry = None;
                    e.exit = None;
                }
                e.thread = Some(thread.clone());
                layer.appearance.set_fill(Paint::solid(*color));
                let lid = layer.id;
                if k == 0 {
                    *d.node_mut(*id).ok_or(EngineError::NoNode(*id))? = layer;
                } else {
                    d.insert(parent, index + k, layer)?;
                }
                made.push(lid);
            }
        }
        sel.set(made.iter().copied());
        Ok(made)
    })?;
    Ok(json!({ "ids": made.iter().map(|i| i.0).collect::<Vec<_>>() }))
}
