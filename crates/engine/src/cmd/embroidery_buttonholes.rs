//! Buttonholes (`embroidery.addButtonholes`): satin columns bordering a slit that is cut open
//! after sewing, with bartacks or a rounded or keyhole end; one, or a string of them evenly
//! spaced. Each is a group of ordinary stitched paths sewn in order: a run round the buttonhole
//! to hold the fabric, one side, the far end, the other side and a bartack.

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, StitchType};
use sf_core::model::{RunParams, SatinParams};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node, NodeId};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::path::{PathData, SubPath};

use super::{CommandSpec, Point, Result, Session, bad, cmd, has_doc, point_param, str_param};

const CMD: &str = "embroidery.addButtonholes";
/// Most buttonholes in one string.
const MAX_BUTTONHOLES: usize = 50;
/// The open space left between the two sides for the cut (mm).
const CUT_SPACE: f64 = 0.6;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.addButtonholes",
        "Add Buttonholes",
        [],
        None,
        "{style?: straight|rounded|keyhole (straight), length?: mm (the slit, 15), width?: mm (both sides and the cut space, 4), angle?: degrees (0: the slit along x), at?: [x, y] (the first one's centre; default: the first artboard's centre), to?: [x, y] (the last one's centre), count?: 1–50 (1; with `to`, evenly spaced from `at` to it)} buttonholes sewn as satin columns bordering a slit, cut open after sewing: a run round the buttonhole, one side, the far end (a bartack, a rounded end or a keyhole eye), the other side and a bartack; each a group of stitched paths, a string of them grouped too → {id, ids}",
        has_doc,
        add_buttonholes
    )]
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Straight,
    Rounded,
    Keyhole,
}

/// One stitched path of a buttonhole (mm, slit along x, centred on the origin).
struct Part {
    name: &'static str,
    pts: Vec<(f64, f64)>,
    closed: bool,
    stitch: StitchType,
    /// Satin: the column's width.
    width: Option<f64>,
}

/// One buttonhole's paths, in sewing order.
fn buttonhole(style: Style, length: f64, width: f64) -> Vec<Part> {
    let half = length * 0.5;
    let column = ((width - CUT_SPACE) * 0.5).max(0.4);
    let y = CUT_SPACE * 0.5 + column * 0.5;
    let tack = (width * 0.35).clamp(0.8, 2.0).min(length / 3.0);
    let satin = |name: &'static str, pts: Vec<(f64, f64)>, w: f64| Part {
        name,
        pts,
        closed: false,
        stitch: StitchType::Satin(SatinParams { spacing: 0.3, pull_comp: 0.1, ..SatinParams::default() }),
        width: Some(w),
    };
    let holding_run = |pts: Vec<(f64, f64)>| Part {
        name: "Holding run",
        pts,
        closed: true,
        stitch: StitchType::Run(RunParams { stitch_length: 1.5, ..RunParams::default() }),
        width: None,
    };
    let arc = |c: (f64, f64), r: f64, from: f64, to: f64, steps: usize| -> Vec<(f64, f64)> {
        (0..=steps).map(|k| from + (to - from) * k as f64 / steps as f64).map(|a| (c.0 + r * a.cos(), c.1 + r * a.sin())).collect()
    };
    let near_tack = satin("Bartack", vec![(-half, 0.0), (-half - tack, 0.0)], width);
    match style {
        Style::Straight => vec![
            holding_run(vec![(-half - tack * 0.5, -y), (half + tack * 0.5, -y), (half + tack * 0.5, y), (-half - tack * 0.5, y)]),
            satin("Side", vec![(-half, -y), (half, -y)], column),
            satin("Bartack", vec![(half, 0.0), (half + tack, 0.0)], width),
            satin("Side", vec![(half, y), (-half, y)], column),
            near_tack,
        ],
        Style::Rounded => {
            // the sides joined round the far end
            let mut u = vec![(-half, -y)];
            u.extend(arc((half, 0.0), y, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2, 16));
            u.push((-half, y));
            vec![holding_run(holding(&u, -half - tack * 0.5)), satin("Sides", u, column), near_tack]
        }
        Style::Keyhole => {
            // the sides round an eye at the far end
            let eye = (width * 0.3).clamp(0.8, 3.0);
            let r = eye + column * 0.5;
            let centre = (half + eye, 0.0);
            let meet = centre.0 - (r * r - y * y).max(0.0).sqrt();
            let a = (-y).atan2(meet - centre.0);
            let mut u = vec![(-half, -y), (meet, -y)];
            // from the lower left of the eye round its far side to its upper left
            u.extend(arc(centre, r, a, -a, 24).into_iter().skip(1));
            u.extend([(meet, y), (-half, y)]);
            vec![holding_run(holding(&u, -half - tack * 0.5)), satin("Sides", u, column), near_tack]
        }
    }
}

/// The run holding the fabric round sides joined at the far end (`sides`, from the near end's
/// one side to its other), closed across the near end at `x`.
fn holding(sides: &[(f64, f64)], x: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(sides.len() + 2);
    out.extend(sides.first().map(|&(_, y)| (x, y)));
    out.extend_from_slice(sides);
    out.extend(sides.last().map(|&(_, y)| (x, y)));
    out
}

fn add_buttonholes(s: &mut Session, p: &Value) -> Result<Value> {
    let style = match str_param(p, "style").unwrap_or("straight") {
        "straight" => Style::Straight,
        "rounded" => Style::Rounded,
        "keyhole" => Style::Keyhole,
        other => return Err(bad(CMD, format!("`style` is straight, rounded or keyhole, not `{other}`"))),
    };
    let num = |key: &str, default: f64, range: std::ops::RangeInclusive<f64>, what: &str| -> Result<f64> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_f64().filter(|v| range.contains(v)).ok_or_else(|| bad(CMD, format!("`{key}` is {what}"))),
        }
    };
    let length = num("length", 15.0, 3.0..=80.0, "3 to 80 mm")?;
    let width = num("width", 4.0, 1.5..=12.0, "1.5 to 12 mm")?;
    let angle = num("angle", 0.0, -3600.0..=3600.0, "an angle in degrees")?;
    let count = match p.get("count") {
        None | Some(Value::Null) => 1,
        Some(v) => v.as_u64().filter(|n| (1..=MAX_BUTTONHOLES as u64).contains(n)).ok_or_else(|| bad(CMD, "`count` is 1 to 50"))? as usize,
    };
    let at = match p.get("at") {
        None | Some(Value::Null) => s.doc()?.doc.artboards.first().map_or(Point::new(0.0, 0.0), |a| a.rect.center()),
        Some(_) => point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y]"))?,
    };
    let to = match p.get("to") {
        None | Some(Value::Null) => None,
        Some(_) => Some(point_param(p, "to").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`to` is [x, y]"))?),
    };
    let centres: Vec<Point> = match to {
        Some(to) if count > 1 => (0..count).map(|k| at.lerp(to, k as f64 / (count - 1) as f64)).collect(),
        _ => vec![at; 1],
    };
    let paint = [&s.paint.stroke, &s.paint.fill]
        .into_iter()
        .find(|p| matches!(p, Paint::Solid { .. }))
        .cloned()
        .unwrap_or_else(|| Paint::solid(Color::rgb8(0x1a, 0x3e, 0x8c)));
    let (sin, cos) = angle.to_radians().sin_cos();
    let parts = buttonhole(style, length, width);
    let parent = s.doc()?.insertion_parent();
    let (id, ids) = s.edit("Add Buttonholes", |d, sel| {
        let mut ids = Vec::new();
        for (k, c) in centres.iter().enumerate() {
            let mut children = Vec::new();
            for part in &parts {
                let pts: Vec<Point> = part
                    .pts
                    .iter()
                    .map(|&(x, y)| {
                        let (x, y) = (x * PT_PER_MM, y * PT_PER_MM);
                        Point::new(c.x + x * cos - y * sin, c.y + x * sin + y * cos)
                    })
                    .collect();
                let id = d.alloc_id();
                let mut n =
                    Node::path(id, PathData::single(SubPath::polyline(&pts, part.closed)), Appearance::basic(Paint::None, paint.clone(), 0.5));
                n.name = Some(part.name.to_string());
                let mut e = Embroidery::new(part.stitch.clone());
                e.width = part.width;
                n.embroidery = Some(Box::new(e));
                children.push(std::sync::Arc::new(n));
            }
            let gid = d.alloc_id();
            let mut g = Node::group(gid, children);
            g.name = Some(if centres.len() > 1 { format!("Buttonhole {}", k + 1) } else { "Buttonhole".to_string() });
            d.insert(parent, usize::MAX, g)?;
            ids.push(gid);
        }
        let id = match ids.as_slice() {
            [one] => *one,
            _ => {
                let g = super::object::group_nodes(d, &ids)?;
                if let Some(n) = d.node_mut(g) {
                    n.name = Some("Buttonholes".into());
                }
                g
            }
        };
        sel.set([id]);
        Ok((id, ids))
    })?;
    Ok(json!({ "id": id.0, "ids": ids.iter().map(|i: &NodeId| i.0).collect::<Vec<_>>() }))
}
