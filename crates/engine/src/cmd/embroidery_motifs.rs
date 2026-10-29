//! Motifs: a single motif placed as art (`embroidery.placeMotif`), and the design's own motifs,
//! made from lines drawn in it (`embroidery.motif.*`), which runs, motif fills and borders sew
//! like the built-in ones.

use serde_json::{Value, json};
use sf_core::attr::{DocEmbroidery, Embroidery, StitchType};
use sf_core::model::{CustomMotif, FillKind, Motif, RunKind, RunParams};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, Node, NodeId};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery::{ids_json, stitched_targets};
use super::{CommandSpec, Point, Result, Session, always, bad, cmd, f64_or, has_doc, has_selection, point_param, str_param, unique_name};
use crate::EngineError;

const CMD: &str = "embroidery.placeMotif";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.placeMotif",
            "Place Motif",
            [],
            None,
            "{motif: wave|scallop|diamond|cross|feather|heart|star|leaf|loop|zigzag|e-stitch or the name of one of the design's motifs or My Motifs, size?: mm (its length, 10), at?: [x, y] (its centre; default: the first artboard's centre), angle?: degrees} one motif as a path sewn in a single run, ready to move, scale and reshape → {id}",
            has_doc,
            place_motif
        ),
        cmd!(
            "embroidery.placeRing",
            "Place Ring",
            [],
            None,
            "{stitch?: satin|fill|bean (satin), diameter?: mm (outside, 20), width?: mm (the ring's, 3), at?: [x, y] (its centre; default: the first artboard's centre)} a stitched ring, ready to move, scale and reshape: a satin border round a circle, a fill between two circles, or a bean run round one → {id}",
            has_doc,
            place_ring
        ),
        cmd!(
            "embroidery.motif.make",
            "Make Motif",
            [],
            None,
            "{name?, ids?} a motif of the design made from the first selected path as drawn, left to right: an open path from its first point to its last, a closed one sewn round from its left side to its right. Runs, motif fills, borders and Place Motif sew it like the built-in motifs; a name the design has already redefines that motif, in the objects sewing it too → {name, points, updated}",
            has_selection,
            motif_make
        ),
        cmd!(
            "embroidery.motif.apply",
            "Apply Motif",
            [],
            None,
            "{name, ids?} the selected runs sew one of the design's motifs along their lines, as high as its drawn proportions make it for their repeat length; the selected fills become motif fills of it, their rows far enough apart for it; paths not stitched yet become motif runs (lines) or motif fills (areas) → {ids}",
            has_selection,
            motif_apply
        ),
        cmd!(query "embroidery.motif.list", "Embroidery Motifs", [], None, "{} the design's motifs → {motifs: [{name, points, aspect, uses}]}", has_doc, motif_list),
        cmd!(
            "embroidery.motif.keep",
            "Keep Motif",
            [],
            None,
            "{name} keep one of the design's motifs in My Motifs, for any design: Apply Motif, Place Motif and the motif lists find it by name (a kept motif of the same name is replaced) → {name, kept}",
            has_doc,
            motif_keep
        ),
        cmd!(query "embroidery.motif.library", "My Motifs", [], None, "{} the motifs kept for any design → {motifs: [{name, points, aspect}]}", always, motif_library),
        cmd!(
            "embroidery.motif.forget",
            "Forget Motif",
            [],
            None,
            "{name} take a motif out of My Motifs; designs using it keep it → {name}",
            always,
            motif_forget
        ),
        cmd!(
            "embroidery.motif.delete",
            "Delete Motif",
            [],
            None,
            "{name} remove one of the design's motifs; objects sewing it keep sewing it → {name}",
            has_doc,
            motif_delete
        ),
    ]
}

fn config(s: &Session) -> Result<DocEmbroidery> {
    Ok(s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default())
}

/// A motif by name: a built-in one (any case), else one of the design's (its name, else any
/// case), with that one's line.
pub(crate) fn motif_by_name(s: &Session, name: &str, cmd: &str) -> Result<(Motif, Option<CustomMotif>)> {
    if let Some(m) = Motif::ALL.into_iter().find(|m| m.label().eq_ignore_ascii_case(name)) {
        return Ok((m, None));
    }
    let cfg = config(s)?;
    let custom = cfg.motif(name).or_else(|| cfg.motifs.iter().find(|m| m.name.eq_ignore_ascii_case(name))).or_else(|| kept(s, name));
    custom.map(|c| (Motif::default(), Some(c.clone()))).ok_or_else(|| bad(cmd, format!("unknown motif `{name}`")))
}

/// One of My Motifs by name (any case).
fn kept<'a>(s: &'a Session, name: &str) -> Option<&'a CustomMotif> {
    s.prefs.embroidery_motifs.iter().find(|m| m.name.eq_ignore_ascii_case(name))
}

/// Most motifs My Motifs keeps.
const MAX_KEPT: usize = 500;

fn motif_keep(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.motif.keep";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?;
    let motif = config(s)?.motif(name).cloned().ok_or_else(|| bad(CMD, format!("the design has no motif named `{name}`")))?;
    let lib = &mut s.prefs.embroidery_motifs;
    lib.retain(|m| !m.name.eq_ignore_ascii_case(&motif.name));
    if lib.len() >= MAX_KEPT {
        return Err(bad(CMD, format!("My Motifs keeps {MAX_KEPT} motifs at most: forget one first")));
    }
    lib.push(motif.clone());
    Ok(json!({ "name": motif.name, "kept": lib.len() }))
}

fn motif_library(s: &mut Session, _: &Value) -> Result<Value> {
    let motifs: Vec<Value> =
        s.prefs.embroidery_motifs.iter().map(|m| json!({ "name": m.name, "points": m.points.len(), "aspect": m.aspect() })).collect();
    Ok(json!({ "motifs": motifs }))
}

fn motif_forget(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.motif.forget";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?;
    let lib = &mut s.prefs.embroidery_motifs;
    let before = lib.len();
    lib.retain(|m| !m.name.eq_ignore_ascii_case(name));
    if lib.len() == before {
        return Err(bad(CMD, format!("My Motifs has no motif named `{name}`")));
    }
    Ok(json!({ "name": name }))
}

/// The custom motif an object sews, if any.
fn sewn_motif(n: &Node) -> Option<&CustomMotif> {
    match &n.embroidery.as_deref()?.stitch {
        StitchType::Run(r) => r.custom_motif.as_ref(),
        StitchType::Fill(f) => f.custom_motif.as_ref(),
        _ => None,
    }
}

/// Every node in the document sewing the motif `name`.
fn sewing(d: &Document, name: &str) -> Vec<NodeId> {
    let mut out = Vec::new();
    d.walk(|n| {
        if sewn_motif(n).is_some_and(|m| m.name == name) {
            out.push(n.id);
        }
    });
    out
}

fn motif_make(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.motif.make";
    let line: Vec<sf_core::Point> = {
        let d = &s.doc()?.doc;
        stitched_targets(s, p)?
            .iter()
            .filter_map(|id| d.node(*id))
            .map(|n| vectorcraft_embroidery::outlines_mm(n).concat())
            .find(|l| l.len() >= 2)
            .ok_or_else(|| bad(CMD, "select the path to make the motif from"))?
    };
    let cfg = config(s)?;
    let name = match str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => unique_name(&format!("Motif {}", cfg.motifs.len() + 1), |n| cfg.motif(n).is_some()),
    };
    let motif = sf_core::engine::make_motif(&name, &line).ok_or_else(|| bad(CMD, "the path is too small or too detailed to make a motif from"))?;
    let users = sewing(&s.doc()?.doc, &name);
    s.edit("Make Motif", |d, _| {
        let lib = &mut d.embroidery.get_or_insert_with(Box::default).motifs;
        match lib.iter_mut().find(|m| m.name == motif.name) {
            Some(m) => *m = motif.clone(),
            None => lib.push(motif.clone()),
        }
        for id in &users {
            let e = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut();
            match e.map(|e| &mut e.stitch) {
                Some(StitchType::Run(r)) => r.custom_motif = Some(motif.clone()),
                Some(StitchType::Fill(f)) => f.custom_motif = Some(motif.clone()),
                _ => {}
            }
        }
        Ok(())
    })?;
    Ok(json!({ "name": name, "points": motif.points.len(), "updated": users.len() }))
}

/// `e` sewing `motif`: a run as a motif run, as high as the motif's proportions make it for its
/// repeat length; a fill as a motif fill with rows far enough apart for it. None for other
/// stitch types.
fn with_motif(mut e: Embroidery, motif: &CustomMotif) -> Option<Embroidery> {
    match &mut e.stitch {
        StitchType::Run(r) => {
            if r.kind != RunKind::Motif {
                let StitchType::Run(d) = StitchType::from_name("motif")? else { return None };
                (r.kind, r.spacing) = (RunKind::Motif, d.spacing);
            }
            r.width = (r.spacing.max(0.5) * motif.aspect()).clamp(0.3, 50.0);
            r.custom_motif = Some(motif.clone());
        }
        StitchType::Fill(f) => {
            if f.kind != FillKind::Motif {
                let StitchType::Fill(d) = StitchType::from_name("motiffill")? else { return None };
                (f.kind, f.stitch_length) = (FillKind::Motif, d.stitch_length);
            }
            let size = if f.motif_size.is_finite() { f.motif_size.clamp(1.0, 50.0) } else { 4.0 };
            f.spacing = (size * motif.aspect() * 1.25).clamp(0.5, 80.0);
            f.custom_motif = Some(motif.clone());
        }
        _ => return None,
    }
    Some(e)
}

fn motif_apply(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.motif.apply";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?;
    let ours = config(s)?.motif(name).cloned();
    let joins = ours.is_none();
    let motif =
        ours.or_else(|| kept(s, name).cloned()).ok_or_else(|| bad(CMD, format!("the design has no motif named `{name}`, nor has My Motifs")))?;
    let mut updated = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in stitched_targets(s, p)? {
            let Some(n) = d.node(id) else { continue };
            let e = match n.embroidery.as_deref() {
                Some(e) => e.clone(),
                None if vectorcraft_embroidery::is_line_art(n) => Embroidery::run(),
                None => Embroidery::new(StitchType::Fill(sf_core::model::FillParams { kind: FillKind::Motif, ..Default::default() })),
            };
            if let Some(e) = with_motif(e, &motif) {
                updated.push((id, e));
            }
        }
    }
    if updated.is_empty() {
        return Err(bad(CMD, "select runs, fills or paths to sew the motif"));
    }
    s.edit("Apply Motif", |d, _| {
        for (id, e) in &updated {
            d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery = Some(Box::new(e.clone()));
        }
        // one of My Motifs becomes one of the design's
        if joins {
            d.embroidery.get_or_insert_with(Box::default).motifs.push(motif.clone());
        }
        Ok(())
    })?;
    Ok(ids_json(&updated.iter().map(|u| u.0).collect::<Vec<_>>()))
}

fn motif_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let motifs: Vec<Value> = d
        .embroidery
        .as_deref()
        .map(|c| c.motifs.as_slice())
        .unwrap_or_default()
        .iter()
        .map(|m| json!({ "name": m.name, "points": m.points.len(), "aspect": m.aspect(), "uses": sewing(d, &m.name).len() }))
        .collect();
    Ok(json!({ "motifs": motifs }))
}

fn motif_delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.motif.delete";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?;
    if config(s)?.motif(name).is_none() {
        return Err(bad(CMD, format!("the design has no motif named `{name}`")));
    }
    s.edit("Delete Motif", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).motifs.retain(|m| m.name != name);
        Ok(())
    })?;
    Ok(json!({ "name": name }))
}

fn place_ring(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.placeRing";
    let stitch = str_param(p, "stitch").unwrap_or("satin");
    if !matches!(stitch, "satin" | "fill" | "bean") {
        return Err(bad(CMD, format!("`stitch` is satin, fill or bean, not `{stitch}`")));
    }
    let diameter = match p.get("diameter") {
        None | Some(Value::Null) => 20.0,
        Some(v) => v.as_f64().filter(|v| (2.0..=300.0).contains(v)).ok_or_else(|| bad(CMD, "`diameter` is 2 to 300 mm"))?,
    };
    let width = match p.get("width") {
        None | Some(Value::Null) => 3.0_f64.min(diameter * 0.25),
        Some(v) => v
            .as_f64()
            .filter(|v| *v >= 0.5 && *v <= (diameter * 0.5).min(30.0))
            .ok_or_else(|| bad(CMD, "`width` is 0.5 mm to half the diameter (at most 30 mm)"))?,
    };
    let at = match p.get("at") {
        None | Some(Value::Null) => s.doc()?.doc.artboards.first().map_or(Point::new(0.0, 0.0), |a| a.rect.center()),
        Some(_) => point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y]"))?,
    };
    let circle = |radius_mm: f64| {
        let r = radius_mm * PT_PER_MM;
        vectorcraft_geom::shapes::ellipse(vectorcraft_geom::Rect::new(at.x - r, at.y - r, at.x + r, at.y + r))
    };
    let outer = diameter * 0.5;
    let (path, embroidery) = match stitch {
        "fill" => {
            let mut subs = circle(outer).subpaths;
            subs.extend(circle(outer - width).subpaths);
            (PathData::new(subs), Embroidery::fill())
        }
        "bean" => (circle(outer - width * 0.5), Embroidery::new(StitchType::Run(RunParams { kind: RunKind::Triple, ..RunParams::default() }))),
        _ => {
            let mut e = Embroidery::satin();
            e.width = Some(width);
            (circle(outer - width * 0.5), e)
        }
    };
    let paint = [&s.paint.stroke, &s.paint.fill]
        .into_iter()
        .find(|p| matches!(p, Paint::Solid { .. }))
        .cloned()
        .unwrap_or_else(|| Paint::solid(Color::rgb8(0x1a, 0x3e, 0x8c)));
    // a filled ring shows its paint as a fill, the others as a stroke as wide as they sew
    let appearance = match stitch {
        "fill" => Appearance::basic(paint, Paint::None, 0.0),
        "bean" => Appearance::basic(Paint::None, paint, 1.0),
        _ => Appearance::basic(Paint::None, paint, width * PT_PER_MM),
    };
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Place Ring", |d, sel| {
        let id = d.alloc_id();
        let mut n = Node::path(id, path.clone(), appearance.clone());
        if let vectorcraft_doc::NodeKind::Path { rule, .. } = &mut n.kind {
            *rule = vectorcraft_geom::FillRule::EvenOdd;
        }
        n.name = Some("Ring".to_string());
        n.embroidery = Some(Box::new(embroidery.clone()));
        d.insert(parent, usize::MAX, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}

fn place_motif(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "motif").ok_or_else(|| bad(CMD, "missing `motif`"))?;
    let (motif, custom) = motif_by_name(s, name, CMD)?;
    let size = match p.get("size") {
        None => 10.0,
        Some(v) => v.as_f64().filter(|v| (1.0..=500.0).contains(v)).ok_or_else(|| bad(CMD, "`size` is 1 to 500 mm"))?,
    };
    let angle = f64_or(p, "angle", 0.0);
    if !angle.is_finite() {
        return Err(bad(CMD, "`angle` must be a number of degrees"));
    }
    let at = match p.get("at") {
        None => s.doc()?.doc.artboards.first().map_or(Point::new(0.0, 0.0), |a| a.rect.center()),
        Some(_) => point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y]"))?,
    };
    // The motif `size` long (as high as a custom one's proportions make it), centred on `at` and
    // turned by `angle`, the way up it sews along a left-to-right run.
    let k = size * PT_PER_MM;
    let across = k * custom.as_ref().map_or(1.0, CustomMotif::aspect);
    let (sin, cos) = angle.to_radians().sin_cos();
    let pts: Vec<Point> = sf_core::engine::motif_line(motif, custom.as_ref())
        .iter()
        .map(|&(a, c)| {
            let (x, y) = ((a - 0.5) * k, c * across);
            Point::new(at.x + x * cos - y * sin, at.y + x * sin + y * cos)
        })
        .collect();
    let path = PathData::single(SubPath::polyline(&pts, false));
    // Short stitches, so a small motif keeps its shape.
    let run = RunParams { kind: RunKind::Single, stitch_length: (size / 8.0).clamp(1.0, 2.5), ..RunParams::default() };
    let paint = [&s.paint.stroke, &s.paint.fill]
        .into_iter()
        .find(|p| matches!(p, Paint::Solid { .. }))
        .cloned()
        .unwrap_or_else(|| Paint::solid(Color::rgb8(0x1a, 0x3e, 0x8c)));
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Place Motif", |d, sel| {
        let id = d.alloc_id();
        let mut n = Node::path(id, path.clone(), Appearance::basic(Paint::None, paint.clone(), 0.75));
        n.name = Some(custom.as_ref().map_or_else(|| motif.label().to_string(), |c| c.name.clone()));
        n.embroidery = Some(Box::new(Embroidery::new(StitchType::Run(run.clone()))));
        d.insert(parent, usize::MAX, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}
