//! Monograms: one to three letters in an embroidery font, the middle one of three larger, and a
//! border round them. Each letter is lettering that can be set again.

use std::sync::Arc;

use serde_json::{Value, json};
use sf_core::attr::{Embroidery, LetterAlign, LetterSewing, Lettering, StitchType};
use sf_core::model::{Motif, RunKind, RunParams, SatinParams};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Node};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_geom::Rect;
use vectorcraft_geom::path::{PathData, SubPath};

use super::embroidery_lettering::{ensure_fonts, glyphs, lettering_name, loaded, not_loaded, own_size, ranged, size_warnings};
use super::{CommandSpec, Point, Result, Session, always, bad, cmd, has_doc, point_param, str_param};

const CMD: &str = "embroidery.monogram";
/// The middle letter of a classic monogram of three, × the others.
const BIG: f64 = 1.6;
/// How far the border keeps clear of the letters, and a satin border's width (mm).
const MARGIN: f64 = 3.0;
const BORDER_WIDTH: f64 = 2.5;
/// The borders there are.
pub const BORDERS: [&str; 9] = ["none", "circle", "diamond", "square", "scallop", "oval", "octagon", "heart", "shield"];
/// The ornaments either side of the letters.
pub const ORNAMENTS: [&str; 5] = ["none", "hearts", "stars", "dots", "swirls"];
/// Rings of a border at most, and how far apart (mm).
const MAX_RINGS: u64 = 4;
/// Monogram templates of the user's own kept at most.
const MAX_TEMPLATES: usize = 100;

/// A monogram's look, kept by name: style, border, rings, ornament and, when set, size.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MonogramTemplate {
    pub name: String,
    pub style: String,
    pub border: String,
    pub rings: u64,
    pub ornament: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
}

impl MonogramTemplate {
    fn json(&self, builtin: bool) -> Value {
        json!({ "name": self.name, "style": self.style, "border": self.border, "rings": self.rings,
            "ornament": self.ornament, "size": self.size, "builtin": builtin })
    }
}

/// The gallery that comes with StitchFoundry.
pub fn builtin_templates() -> Vec<MonogramTemplate> {
    let t = |name: &str, style: &str, border: &str, rings: u64, ornament: &str| MonogramTemplate {
        name: name.into(),
        style: style.into(),
        border: border.into(),
        rings,
        ornament: ornament.into(),
        size: None,
    };
    vec![
        t("Classic circle", "classic", "circle", 1, "none"),
        t("Double ring", "classic", "circle", 2, "none"),
        t("Wedding heart", "classic", "heart", 2, "hearts"),
        t("Royal shield", "classic", "shield", 1, "swirls"),
        t("Diamond trio", "classic", "diamond", 1, "dots"),
        t("Scalloped", "classic", "scallop", 1, "none"),
        t("Starry oval", "line", "oval", 1, "stars"),
        t("Square block", "line", "square", 2, "none"),
        t("Stacked crest", "stacked", "octagon", 3, "none"),
    ]
}

/// A template by name: the user's own first, then the gallery's.
fn template_named(s: &Session, name: &str) -> Option<MonogramTemplate> {
    s.prefs
        .embroidery_monogram_templates
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        .cloned()
        .or_else(|| builtin_templates().into_iter().find(|t| t.name.eq_ignore_ascii_case(name)))
}
const RING_GAP: f64 = 3.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.monogram.templates",
            "Monogram Templates",
            [],
            None,
            "{} the monogram templates: the gallery's and the user's own → {templates: [{name, style, border, rings, ornament, size, builtin}]}",
            always,
            list_templates
        ),
        cmd!(
            "embroidery.monogram.saveTemplate",
            "Save Monogram Template",
            [],
            None,
            "{name, style?, border?, rings?, ornament?, size?: mm} keep a monogram's look under a name (the user's own; a gallery name can't be taken), to make others with embroidery.monogram {template} → {name}",
            always,
            save_template
        ),
        cmd!(
            "embroidery.monogram.deleteTemplate",
            "Delete Monogram Template",
            [],
            None,
            "{name} delete one of the user's monogram templates → {name}",
            always,
            delete_template
        ),
        cmd!(
            "embroidery.monogram",
            "Monogram",
            [],
            None,
            "{letters: 1 to 3 characters, font: a loaded embroidery font, template?: a monogram template's name (its style, border, rings, ornament and size, unless given here), size?: mm (the letters' height, the middle one of three larger in a classic monogram; default: the font's own), style?: classic|line|stacked (classic: the middle of three letters larger, each centred on the monogram's middle; line: all one size on one baseline; stacked: one above the other), border?: none|circle|diamond|square|scallop|oval|octagon|heart|shield (none), rings?: 1–4 borders (3 mm apart; 1), ornament?: none|hearts|stars|dots|swirls (none: either side of the letters, mirrored), at?: [x, y] (its middle; default: the first artboard's centre)} a monogram: each letter lettering in the font (set again with embroidery.lettering.edit), round them and the ornaments a satin border (a scalloped one a scallop motif run), in a group → {id, letters, warnings}",
            has_doc,
            monogram
        ),
    ]
}

/// A border's outline round `b` (points), `margin` clear of it: a circle (scalloped too), a
/// diamond or a square.
fn border_points(kind: &str, b: Rect, margin: f64) -> Vec<Point> {
    let c = b.center();
    let (hw, hh) = (b.width() / 2.0 + margin, b.height() / 2.0 + margin);
    match kind {
        // through the corners' diagonals: |x| + |y| reaches hw + hh
        "diamond" => {
            let r = hw + hh;
            vec![Point::new(c.x, c.y - r), Point::new(c.x + r, c.y), Point::new(c.x, c.y + r), Point::new(c.x - r, c.y)]
        }
        "square" => {
            let r = hw.max(hh);
            vec![Point::new(c.x - r, c.y - r), Point::new(c.x + r, c.y - r), Point::new(c.x + r, c.y + r), Point::new(c.x - r, c.y + r)]
        }
        // an ellipse through the corners of the box grown by half again
        "oval" => (0..96)
            .map(|k| {
                let a = f64::from(k) / 96.0 * std::f64::consts::TAU;
                Point::new(c.x + hw * std::f64::consts::SQRT_2 * a.cos(), c.y + hh * std::f64::consts::SQRT_2 * a.sin())
            })
            .collect(),
        "octagon" => {
            // its sides clear of the box's corners
            let r = hw.hypot(hh) / (std::f64::consts::PI / 8.0).cos();
            (0..8)
                .map(|k| {
                    let a = (f64::from(k) + 0.5) / 8.0 * std::f64::consts::TAU;
                    Point::new(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect()
        }
        // the heart curve x = 16 sin³t, y = 13 cos t − 5 cos 2t − 2 cos 3t − cos 4t, round the box
        "heart" => {
            let k = hw.max(hh) * 1.9 / 16.0;
            (0..120)
                .map(|i| {
                    let t = f64::from(i) / 120.0 * std::f64::consts::TAU;
                    let x = 16.0 * t.sin().powi(3);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    Point::new(c.x + x * k, c.y - y * k + hh * 0.35)
                })
                .collect()
        }
        // flat top, straight sides, curving in to a point below
        "shield" => {
            let (w, top, side) = (hw * 1.25, c.y - hh * 1.35, c.y + hh * 0.35);
            let mut pts = vec![Point::new(c.x - w, top), Point::new(c.x + w, top), Point::new(c.x + w, side)];
            for i in 1..=12 {
                let t = f64::from(i) / 12.0;
                pts.push(Point::new(c.x + w * (1.0 - t) * (1.0 - t * 0.2), side + hh * 1.6 * (t * std::f64::consts::FRAC_PI_2).sin()));
            }
            for i in (1..12).rev() {
                let t = f64::from(i) / 12.0;
                pts.push(Point::new(c.x - w * (1.0 - t) * (1.0 - t * 0.2), side + hh * 1.6 * (t * std::f64::consts::FRAC_PI_2).sin()));
            }
            pts.push(Point::new(c.x - w, side));
            pts
        }
        _ => {
            let r = hw.hypot(hh);
            (0..96)
                .map(|k| {
                    let a = f64::from(k) / 96.0 * std::f64::consts::TAU;
                    Point::new(c.x + r * a.cos(), c.y + r * a.sin())
                })
                .collect()
        }
    }
}

/// An ornament `size` points across centred on `c`, `mirror`ed for the right side: its outlines
/// (closed shapes sewn as satin) or, for swirls, one open line (a triple run).
fn ornament(kind: &str, c: Point, size: f64, mirror: bool) -> (Vec<Vec<Point>>, bool) {
    let r = size / 2.0;
    let turn = |i: u32, n: u32| f64::from(i) / f64::from(n) * std::f64::consts::TAU;
    match kind {
        "hearts" => {
            let k = r / 16.0;
            let pts = (0..72)
                .map(|i| {
                    let t = turn(i, 72);
                    let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
                    Point::new(c.x + 16.0 * t.sin().powi(3) * k, c.y - y * k)
                })
                .collect();
            (vec![pts], true)
        }
        "stars" => {
            let pts = (0..10)
                .map(|i| {
                    let a = turn(i, 10) - std::f64::consts::FRAC_PI_2;
                    let rr = if i % 2 == 0 { r } else { r * 0.45 };
                    Point::new(c.x + rr * a.cos(), c.y + rr * a.sin())
                })
                .collect();
            (vec![pts], true)
        }
        "dots" => {
            // three dots in a column
            let d = r * 0.35;
            let dots = [-1.0, 0.0, 1.0]
                .iter()
                .map(|k| (0..24).map(|i| Point::new(c.x + d * turn(i, 24).cos(), c.y + k * r * 0.8 + d * turn(i, 24).sin())).collect())
                .collect();
            (dots, true)
        }
        _ => {
            // a spiral of two turns, opening away from the letters
            let side = if mirror { 1.0 } else { -1.0 };
            let pts = (0..=96)
                .map(|i| {
                    let t = f64::from(i) / 96.0 * 4.0 * std::f64::consts::PI;
                    let rr = r * t / (4.0 * std::f64::consts::PI);
                    Point::new(c.x + side * rr * t.cos(), c.y + rr * t.sin())
                })
                .collect();
            (vec![pts], false)
        }
    }
}

fn list_templates(s: &mut Session, _p: &Value) -> Result<Value> {
    let all: Vec<Value> =
        builtin_templates().iter().map(|t| t.json(true)).chain(s.prefs.embroidery_monogram_templates.iter().map(|t| t.json(false))).collect();
    Ok(json!({ "templates": all }))
}

fn save_template(s: &mut Session, p: &Value) -> Result<Value> {
    const SAVE: &str = "embroidery.monogram.saveTemplate";
    let name: String =
        str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(SAVE, "missing `name`"))?.chars().take(60).collect();
    if builtin_templates().iter().any(|t| t.name.eq_ignore_ascii_case(&name)) {
        return Err(bad(SAVE, format!("`{name}` is in the gallery: save it under another name")));
    }
    let pick = |key: &str, all: &[&str], default: &str| -> Result<String> {
        match str_param(p, key) {
            None => Ok(default.to_string()),
            Some(v) => all
                .iter()
                .find(|k| **k == v)
                .map(|k| (*k).to_string())
                .ok_or_else(|| bad(SAVE, format!("unknown {key} `{v}`: one of {}", all.join(", ")))),
        }
    };
    let t = MonogramTemplate {
        name: name.clone(),
        style: pick("style", &["classic", "line", "stacked"], "classic")?,
        border: pick("border", &BORDERS, "none")?,
        rings: match p.get("rings") {
            None | Some(Value::Null) => 1,
            Some(v) => v.as_u64().filter(|r| (1..=MAX_RINGS).contains(r)).ok_or_else(|| bad(SAVE, "`rings` is 1 to 4"))?,
        },
        ornament: pick("ornament", &ORNAMENTS, "none")?,
        size: ranged(p, "size", 1.0..=300.0, SAVE, "`size` is 1 to 300 mm")?,
    };
    let own = &mut s.prefs.embroidery_monogram_templates;
    let full = own.len() >= MAX_TEMPLATES;
    match own.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&name)) {
        Some(o) => *o = t,
        None if full => return Err(bad(SAVE, format!("{MAX_TEMPLATES} monogram templates at most: delete one first"))),
        None => own.push(t),
    }
    Ok(json!({ "name": name }))
}

fn delete_template(s: &mut Session, p: &Value) -> Result<Value> {
    const DELETE: &str = "embroidery.monogram.deleteTemplate";
    let name = str_param(p, "name").ok_or_else(|| bad(DELETE, "missing `name`"))?.to_string();
    let own = &mut s.prefs.embroidery_monogram_templates;
    let before = own.len();
    own.retain(|t| !t.name.eq_ignore_ascii_case(&name));
    if own.len() == before {
        return Err(bad(DELETE, format!("no monogram template of yours is named `{name}`")));
    }
    Ok(json!({ "name": name }))
}

fn monogram(s: &mut Session, p: &Value) -> Result<Value> {
    // a template's look, unless the request says otherwise
    let merged;
    let p = match str_param(p, "template") {
        None => p,
        Some(name) => {
            let t = template_named(s, name).ok_or_else(|| bad(CMD, format!("no monogram template `{name}`")))?;
            let mut m = json!({ "style": t.style, "border": t.border, "rings": t.rings, "ornament": t.ornament });
            if let Some(size) = t.size {
                m["size"] = json!(size);
            }
            if let (Some(o), Some(given)) = (m.as_object_mut(), p.as_object()) {
                for (k, v) in given {
                    o.insert(k.clone(), v.clone());
                }
            }
            merged = m;
            &merged
        }
    };
    let letters: Vec<char> = str_param(p, "letters").map(|t| t.chars().filter(|c| !c.is_whitespace()).collect()).unwrap_or_default();
    if !(1..=3).contains(&letters.len()) {
        return Err(bad(CMD, "`letters` are 1 to 3 characters"));
    }
    let name = str_param(p, "font").ok_or_else(|| bad(CMD, "missing `font`"))?;
    let mut warnings = ensure_fonts(s);
    let font = loaded(s, name).ok_or_else(|| not_loaded(CMD, name))?;
    let size = ranged(p, "size", 1.0..=300.0, CMD, "`size` is 1 to 300 mm")?.unwrap_or_else(|| own_size(&font));
    let (classic, stacked) = match str_param(p, "style") {
        None | Some("classic") => (true, false),
        Some("line") => (false, false),
        Some("stacked") => (false, true),
        Some(o) => return Err(bad(CMD, format!("unknown style `{o}`: classic, line or stacked"))),
    };
    let ornament_kind = match str_param(p, "ornament") {
        None => "none",
        Some(o) => {
            ORNAMENTS.iter().copied().find(|k| *k == o).ok_or_else(|| bad(CMD, format!("unknown ornament `{o}`: one of {}", ORNAMENTS.join(", "))))?
        }
    };
    let rings = match p.get("rings") {
        None | Some(Value::Null) => 1,
        Some(v) => v.as_u64().filter(|r| (1..=MAX_RINGS).contains(r)).ok_or_else(|| bad(CMD, "`rings` is 1 to 4"))?,
    };
    let border = match str_param(p, "border") {
        None => "none",
        Some(b) => {
            BORDERS.iter().copied().find(|k| *k == b).ok_or_else(|| bad(CMD, format!("unknown border `{b}`: one of {}", BORDERS.join(", "))))?
        }
    };
    let at = match p.get("at") {
        None | Some(Value::Null) => s.doc()?.doc.artboards.first().map_or(Point::new(0.0, 0.0), |a| a.rect.center()),
        Some(_) => point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y]"))?,
    };
    // each letter's height, and its width from the font's advances (points)
    let sizes: Vec<f64> = (0..letters.len()).map(|i| if classic && letters.len() == 3 && i == 1 { size * BIG } else { size }).collect();
    let width = |c: char, h: f64| {
        let k = h / own_size(&font);
        font.layout(&c.to_string()).iter().fold(0.0f64, |w, g| w.max((g.x + g.advance) * font.glyphs.get(&g.glyph).map_or(0.0, |gl| gl.unit) * k))
    };
    let widths: Vec<f64> = letters.iter().zip(&sizes).map(|(c, h)| width(*c, *h)).collect();
    let gap = size * PT_PER_MM * 0.25;
    let total = widths.iter().sum::<f64>() + gap * (letters.len() - 1) as f64;
    let tallest = sizes.iter().fold(0.0f64, |m, h| m.max(*h)) * PT_PER_MM;
    // left to right, centred on `at`: classic letters each centred on it up and down, a line on
    // one baseline
    let mut x = at.x - total / 2.0;
    // stacked: one above the other, centred on `at`
    let column = sizes.iter().map(|h| h * PT_PER_MM).sum::<f64>() + gap * (letters.len() - 1) as f64;
    let mut y = at.y - column / 2.0;
    let mut specs = Vec::new();
    for ((c, h), w) in letters.iter().zip(&sizes).zip(&widths) {
        let base = if stacked {
            y += h * PT_PER_MM;
            let b = y;
            y += gap;
            x = at.x - w / 2.0;
            b
        } else if classic {
            at.y + h * PT_PER_MM / 2.0
        } else {
            at.y + tallest / 2.0
        };
        specs.push(Lettering {
            text: c.to_string(),
            font: font.info.name.clone(),
            size: *h,
            leading: 1.0,
            arc: 0.0,
            spiral: 0.0,
            align: LetterAlign::Centre,
            sew: LetterSewing::default(),
            letters: Vec::new(),
            puff: None,
            applique: None,
            stitch: Default::default(),
            small: Default::default(),
            vertical: false,
            path: Vec::new(),
            frame: sf_core::Affine::translate(sf_core::Point::new(x + w / 2.0, base)),
        });
        x += w + gap;
        for w in size_warnings(&font, *h) {
            if !warnings.contains(&w) {
                warnings.push(w);
            }
        }
    }
    let paint = [&s.paint.stroke, &s.paint.fill]
        .into_iter()
        .find(|p| matches!(p, Paint::Solid { .. }))
        .cloned()
        .unwrap_or_else(|| Paint::solid(Color::rgb8(0x1a, 0x3e, 0x8c)));
    let label: String = letters.iter().collect();
    let parent = s.doc()?.insertion_parent();
    let (id, count) = s.edit("Monogram", |d, sel| {
        let mut parts = Vec::new();
        for spec in &specs {
            let glyphs = glyphs(d, &font, spec);
            if glyphs.is_empty() {
                return Err(bad(CMD, format!("{} has none of these characters", font.info.name)));
            }
            let mut g = Node::group(d.alloc_id(), glyphs);
            g.name = Some(lettering_name(spec));
            g.embroidery = Some(Box::new(Embroidery::new(StitchType::Lettering(Box::new(spec.clone())))));
            parts.push(Arc::new(g));
        }
        let count = parts.len();
        // the ornaments either side, mirrored
        let letters_box = parts.iter().filter_map(|n| n.geometric_bounds()).reduce(|a, b| a.union(b));
        if let Some(b) = letters_box.filter(|_| ornament_kind != "none") {
            let o = size * 0.6 * PT_PER_MM;
            let cy = b.center().y;
            for (mirror, cx) in [(false, b.x0 - MARGIN * PT_PER_MM - o / 2.0), (true, b.x1 + MARGIN * PT_PER_MM + o / 2.0)] {
                let (lines, closed) = ornament(ornament_kind, Point::new(cx, cy), o, mirror);
                let subs: Vec<SubPath> = lines.iter().filter(|l| l.len() >= 2).map(|l| SubPath::polyline(l, closed)).collect();
                let look =
                    if closed { Appearance::basic(paint.clone(), Paint::None, 0.0) } else { Appearance::basic(Paint::None, paint.clone(), 1.0) };
                let mut n = Node::path(d.alloc_id(), PathData::new(subs), look);
                n.name = Some("Ornament".into());
                n.embroidery = Some(Box::new(if closed {
                    Embroidery::new(StitchType::Satin(SatinParams::default()))
                } else {
                    Embroidery::new(StitchType::Run(RunParams { kind: RunKind::Triple, ..RunParams::default() }))
                }));
                parts.push(Arc::new(n));
            }
        }
        // the border round them all, in rings
        let bounds = parts.iter().filter_map(|n| n.geometric_bounds()).reduce(|a, b| a.union(b));
        for ring in 0..rings {
            let Some(b) = bounds.filter(|_| border != "none") else { break };
            let pts = border_points(border, b, (MARGIN + RING_GAP * ring as f64) * PT_PER_MM);
            let mut e = if border == "scallop" {
                Embroidery::new(StitchType::Run(RunParams {
                    kind: RunKind::Motif,
                    motif: Motif::Scallop,
                    width: 4.0,
                    spacing: 5.0,
                    ..RunParams::default()
                }))
            } else {
                Embroidery::new(StitchType::Satin(SatinParams::default()))
            };
            if border != "scallop" {
                e.width = Some(BORDER_WIDTH);
            }
            let mut n = Node::path(d.alloc_id(), PathData::single(SubPath::polyline(&pts, true)), Appearance::basic(Paint::None, paint.clone(), 1.0));
            n.name = Some(if ring == 0 { "Border".into() } else { format!("Border {}", ring + 1) });
            n.embroidery = Some(Box::new(e));
            parts.push(Arc::new(n));
        }
        let mut all = Node::group(d.alloc_id(), parts);
        all.name = Some(format!("Monogram {label}"));
        let id = all.id;
        d.insert(parent, usize::MAX, all)?;
        sel.set([id]);
        Ok((id, count))
    })?;
    Ok(json!({ "id": id.0, "letters": count, "warnings": warnings }))
}
