//! The font creator: an embroidery font made in StitchFoundry. A font document has one artboard
//! per glyph, named after it and as wide as its advance, with guides at the baseline and the
//! capitals' height; each glyph is digitized on its artboard with the usual tools. Saving it
//! writes a font folder (font.json and glyphs.stitchfoundry: the glyphs in StitchFoundry's own
//! format, stitching and all) that lettering loads like any embroidery font.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use sf_core::attr::{DocEmbroidery, FontDesign};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::text::{CharStyle, TextObject};
use vectorcraft_doc::{Appearance, Artboard, Document, Guide, LayerColor, Node, Unit};
use vectorcraft_embroidery::PT_PER_MM;
use vectorcraft_embroidery::fonts::{EmbFont, Glyph};
use vectorcraft_geom::path::PathData;
use vectorcraft_geom::{Affine, Rect};

use super::embroidery_lettering::{GLYPHS_FILE, add_font, describe, ensure_fonts, loaded, not_loaded, ranged, remember_folder};
use super::{CommandSpec, EngineError, Result, Session, always, bad, cmd, has_doc, str_param, targets};

const NEW: &str = "embroidery.fontDoc.new";
const SAVE: &str = "embroidery.fontDoc.save";
/// The characters a new font starts with.
pub const DEFAULT_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.,!?'-&";
/// Most glyphs a font document holds, and characters a glyph name has (ligatures such as "fi").
const MAX_DOC_GLYPHS: usize = 500;
const MAX_NAME: usize = 8;
/// Glyph artboards in a row, and the gap between them (mm).
const PER_ROW: usize = 13;
const GAP: f64 = 10.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.fontDoc.new",
            "New Embroidery Font",
            ["Embroidery", "Embroidery Fonts"],
            None,
            "{name?: (My Font), chars?: the glyphs (one a character; A–Z, a–z, 0–9 and .,!?'-&), size?: mm (the capitals' height as digitized; 25; 5–200), descent?: mm below the baseline (0.4 × size)} the font creator: a new document for an embroidery font, one artboard per glyph, named after it and as wide as its advance (resize it to change that), with guides at the baseline and the capitals' height: digitize each glyph on its artboard, then embroidery.fontDoc.save; opens a new document → {glyphs}",
            always,
            new_font
        ),
        cmd!(
            "embroidery.fontDoc.addGlyphs",
            "Add Glyphs",
            [],
            None,
            "{chars: one glyph a character | names: [glyph, …] (ligatures such as \"fi\")} add artboards for glyphs the font document hasn't yet, below the others → {added}",
            has_doc,
            add_glyphs
        ),
        cmd!(
            "embroidery.fontDoc.assign",
            "Assign to Glyph",
            [],
            None,
            "{glyph: a character (or a glyph name of up to 8), ids?: the art (default: the selection)} designs on keys: the art becomes the glyph's, moved onto its artboard (made below the others if the font hasn't the glyph): its bottom on the baseline, its left a margin in, the artboard widened to hold it with a margin either side; art already there stays → {glyph, width}",
            has_doc,
            assign
        ),
        cmd!(
            "embroidery.fontDoc.setProps",
            "Font Properties",
            [],
            None,
            "{name?, leading?: × the size (0.5–5), space?: mm (0–200), letterCase?: upper|lower|none, defaultGlyph?: the glyph characters the font lacks show (\"\": none), minScale?, maxScale?: × the size it sews well down and up to (0–10; 0: not said), description?, license?} the font document's font properties → {font}",
            has_doc,
            set_props
        ),
        cmd!(
            "embroidery.fontDoc.kern",
            "Kern Glyph Pair",
            [],
            None,
            "{pair?: \"A V\" (two glyphs, a space between), left?: glyphs (\"AV\" or names with spaces between) or a kerning class's name after @ (\"@round\"; with `right` instead of `pair`: every left glyph against every right one), right?: as `left`, value: mm closer (negative: further apart; 0 removes it)} kerning for a pair of the font document's glyphs, or for whole classes of them at once → {pairs, set}",
            has_doc,
            kern
        ),
        cmd!(
            "embroidery.fontDoc.kernClass",
            "Kerning Class",
            [],
            None,
            "{name, glyphs?: the class's glyphs (\"COQ\", or names with spaces between; empty or missing deletes the class)} a named set of the font document's glyphs, to kern against a glyph or another class in one step (embroidery.fontDoc.kern with @name) → {classes}",
            has_doc,
            kern_class
        ),
        cmd!(
            "embroidery.fontDoc.fromType",
            "New Embroidery Font from Type",
            [],
            None,
            "{family: an installed font family, style?: (Regular), name?: the font's name (the family's), chars?: the glyphs (A–Z, a–z, 0–9 and .,!?'-&), size?: mm (the capitals' height; 25; 5–200)} the font creator from type: a font document whose glyphs are the family's outlines, each digitized as Convert Artwork would (satin where the strokes are thin enough, fill where they're wide) on its artboard at the family's own advance, to refine and save; opens a new document → {glyphs, family}",
            always,
            from_type
        ),
        cmd!(
            "embroidery.font.edit",
            "Edit Embroidery Font",
            [],
            None,
            "{name: a loaded embroidery font (Ink/Stitch's or one made here)} open the font as a font document to refine its glyphs, add missing ones and change its properties and kerning; saving it (embroidery.fontDoc.save) replaces the loaded font of that name; opens a new document → {glyphs}",
            always,
            edit_font
        ),
        cmd!(
            "embroidery.fontDoc.merge",
            "Merge Embroidery Font",
            [],
            None,
            "{font: a loaded embroidery font, chars?: only these glyphs} merge fonts: the font's glyphs the font document hasn't yet are added below the others, scaled to its size (their capitals the same height), with their advances → {added}",
            has_doc,
            merge_font
        ),
        cmd!(
            query "embroidery.fontDoc.info",
            "Font Info",
            [],
            None,
            "{} the font document's font: its properties, and its glyphs with their widths (mm) and whether they have art yet → {font, glyphs: [{name, width, done}], missing}",
            has_doc,
            info
        ),
        cmd!(
            "embroidery.fontDoc.save",
            "Save Embroidery Font",
            [],
            None,
            "{folder?: write the font folder there (font.json and glyphs.stitchfoundry; remembered like any font folder)} make the font document's glyphs an embroidery font, loaded for lettering at once (without a folder, only for this session): each glyph the art on its artboard (its middle inside it), its advance the artboard's width → {name, glyphs, missing, folder}",
            has_doc,
            save
        ),
    ]
}

/// The font the active document is being made into.
fn design(s: &Session, cmd: &str) -> Result<FontDesign> {
    s.doc()?
        .doc
        .embroidery
        .as_deref()
        .and_then(|e| e.font.clone())
        .ok_or_else(|| bad(cmd, "this document isn't an embroidery font (Embroidery › Embroidery Fonts › New Embroidery Font)"))
}

/// A new glyph's advance (mm): capitals wider than small letters and figures, the rest narrow.
fn advance(name: &str, size: f64) -> f64 {
    let c = name.chars().next().unwrap_or(' ');
    let k = if name.chars().count() > 1 {
        0.9
    } else if c.is_uppercase() {
        0.75
    } else if c.is_alphanumeric() {
        0.6
    } else {
        0.35
    };
    size * k
}

/// The glyph names in `p` (`chars`: one a character; `names`: as given), without blanks and
/// repeats.
fn glyph_names(p: &Value, cmd: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |n: String| {
        if !n.trim().is_empty() && n.chars().count() <= MAX_NAME && !out.contains(&n) {
            out.push(n);
        }
    };
    if let Some(chars) = str_param(p, "chars") {
        chars.chars().filter(|c| !c.is_whitespace()).for_each(|c| add(c.to_string()));
    }
    if let Some(names) = p.get("names").and_then(Value::as_array) {
        names.iter().filter_map(Value::as_str).for_each(|n| add(n.trim().to_string()));
    }
    if out.len() > MAX_DOC_GLYPHS {
        return Err(bad(cmd, format!("{MAX_DOC_GLYPHS} glyphs at most")));
    }
    Ok(out)
}

/// New glyphs `names` at their usual widths (points).
fn usual_widths(names: Vec<String>, f: &FontDesign) -> Vec<(String, f64)> {
    names.into_iter().map(|n| (advance(&n, f.size) * PT_PER_MM, n)).map(|(w, n)| (n, w)).collect()
}

/// The artboards (and their guides) of `glyphs` (names and widths, points), laid out in rows
/// below `top` (points), ids from `next`.
fn glyph_boards(glyphs: &[(String, f64)], f: &FontDesign, top: f64, next: u32) -> (Vec<Artboard>, Vec<Guide>) {
    let pt = PT_PER_MM;
    let height = (f.size * 1.4 + f.descent) * pt;
    let (mut boards, mut guides) = (Vec::new(), Vec::new());
    let (mut x, mut y) = (0.0, top);
    for (i, (name, w)) in glyphs.iter().enumerate() {
        if i > 0 && i % PER_ROW == 0 {
            (x, y) = (0.0, y + height + GAP * pt);
        }
        let w = w.clamp(1.0, 10_000.0);
        let id = next.saturating_add(i as u32);
        let rect = Rect::new(x, y, x + w, y + height);
        let base = rect.y1 - f.descent * pt;
        for pos in [base, base - f.size * pt] {
            guides.push(Guide { vertical: false, pos, artboard: Some(id), layer: None });
        }
        boards.push(Artboard {
            id,
            name: name.clone(),
            rect,
            show_center_mark: false,
            show_cross_hairs: false,
            locked: false,
            show_video_safe_areas: false,
            video_ruler_pixel_aspect_ratio: 1.0,
            color: None,
        });
        x += w + GAP * pt;
    }
    (boards, guides)
}

fn new_font(s: &mut Session, p: &Value) -> Result<Value> {
    let mut f = FontDesign::default();
    if let Some(n) = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        f.name = n.chars().take(100).collect();
    }
    f.size = ranged(p, "size", 5.0..=200.0, NEW, "`size` is 5 to 200 mm")?.unwrap_or(f.size);
    f.descent = ranged(p, "descent", 0.0..=200.0, NEW, "`descent` is 0 to 200 mm")?.unwrap_or(f.size * 0.4);
    f.space = f.size * 0.3;
    let mut names = glyph_names(p, NEW)?;
    if names.is_empty() {
        names = DEFAULT_CHARS.chars().map(String::from).collect();
    }
    let (boards, guides) = glyph_boards(&usual_widths(names, &f), &f, 0.0, 1);
    let mut doc = Document::new(100.0, 100.0);
    doc.title = f.name.clone();
    doc.units = Unit::Millimeters;
    doc.artboards = boards;
    doc.guides = guides;
    doc.embroidery = Some(Box::new(DocEmbroidery { font: Some(f), ..DocEmbroidery::default() }));
    let count = doc.artboards.len();
    s.add_document(doc, None);
    Ok(json!({ "glyphs": count }))
}

fn add_glyphs(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.addGlyphs";
    let f = design(s, CMD)?;
    let d = &s.doc()?.doc;
    let have: Vec<&str> = d.artboards.iter().map(|a| a.name.as_str()).collect();
    let names: Vec<String> = glyph_names(p, CMD)?.into_iter().filter(|n| !have.contains(&n.as_str())).collect();
    if names.is_empty() {
        return Err(bad(CMD, "no glyphs the font hasn't already: give `chars` or `names`"));
    }
    if d.artboards.len() + names.len() > MAX_DOC_GLYPHS {
        return Err(bad(CMD, format!("{MAX_DOC_GLYPHS} glyphs at most")));
    }
    let top = d.artboards.iter().map(|a| a.rect.y1).fold(0.0f64, f64::max) + GAP * PT_PER_MM;
    let next = d.artboards.iter().map(|a| a.id).max().unwrap_or(0).saturating_add(1);
    let (boards, guides) = glyph_boards(&usual_widths(names, &f), &f, top, next);
    let added = boards.len();
    s.edit("Add Glyphs", |d, _| {
        d.artboards.extend(boards);
        d.guides.extend(guides);
        Ok(())
    })?;
    Ok(json!({ "added": added }))
}

fn assign(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.assign";
    let f = design(s, CMD)?;
    let glyph = str_param(p, "glyph")
        .map(str::trim)
        .filter(|g| !g.is_empty() && g.chars().count() <= MAX_NAME)
        .ok_or_else(|| bad(CMD, "`glyph` is a character, or a glyph name of up to 8"))?
        .to_string();
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    let d = &s.doc()?.doc;
    let bounds = ids
        .iter()
        .filter_map(|id| d.node(*id)?.geometric_bounds())
        .reduce(|a, b| a.union(b))
        .ok_or_else(|| bad(CMD, "select the art for the glyph"))?;
    let pt = PT_PER_MM;
    let margin = f.size * 0.1 * pt;
    let width = bounds.width() + margin * 2.0;
    if !width.is_finite() || width > 10_000.0 {
        return Err(bad(CMD, "the art is too wide for a glyph"));
    }
    // the glyph's artboard, widened to hold the art; else a new one below the others
    let (board, guides) = match d.artboards.iter().find(|a| a.name == glyph) {
        Some(a) => {
            let mut a = a.clone();
            a.rect.x1 = a.rect.x0 + a.rect.width().max(width);
            (a, Vec::new())
        }
        None => {
            if d.artboards.len() >= MAX_DOC_GLYPHS {
                return Err(bad(CMD, format!("{MAX_DOC_GLYPHS} glyphs at most")));
            }
            let top = d.artboards.iter().map(|a| a.rect.y1).fold(0.0f64, f64::max) + GAP * pt;
            let next = d.artboards.iter().map(|a| a.id).max().unwrap_or(0).saturating_add(1);
            let (mut boards, guides) = glyph_boards(&[(glyph.clone(), width)], &f, top, next);
            (boards.pop().ok_or_else(|| bad(CMD, "no artboard for the glyph"))?, guides)
        }
    };
    let base = board.rect.y1 - f.descent * pt;
    let m = Affine::translate((board.rect.x0 + margin - bounds.x0, base - bounds.y1));
    let out = json!({ "glyph": glyph, "width": board.rect.width() / pt });
    s.edit("Assign to Glyph", |d, _| {
        for id in &ids {
            d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.transform(m, true);
        }
        match d.artboards.iter_mut().find(|a| a.id == board.id) {
            Some(a) => *a = board.clone(),
            None => {
                d.artboards.push(board.clone());
                d.guides.extend(guides.iter().cloned());
            }
        }
        Ok(())
    })?;
    Ok(out)
}

fn set_props(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.setProps";
    let mut f = design(s, CMD)?;
    if let Some(n) = str_param(p, "name").map(str::trim) {
        if n.is_empty() {
            return Err(bad(CMD, "`name` can't be blank"));
        }
        f.name = n.chars().take(100).collect();
    }
    f.leading = ranged(p, "leading", 0.5..=5.0, CMD, "`leading` is 0.5 to 5")?.unwrap_or(f.leading);
    f.space = ranged(p, "space", 0.0..=200.0, CMD, "`space` is 0 to 200 mm")?.unwrap_or(f.space);
    f.min_scale = ranged(p, "minScale", 0.0..=10.0, CMD, "`minScale` is 0 to 10")?.unwrap_or(f.min_scale);
    f.max_scale = ranged(p, "maxScale", 0.0..=10.0, CMD, "`maxScale` is 0 to 10")?.unwrap_or(f.max_scale);
    match str_param(p, "letterCase") {
        None => {}
        Some("upper" | "lower") => f.letter_case = str_param(p, "letterCase").unwrap_or_default().to_string(),
        Some("none" | "") => f.letter_case.clear(),
        Some(o) => return Err(bad(CMD, format!("unknown letterCase `{o}`: upper, lower or none"))),
    }
    if let Some(v) = str_param(p, "defaultGlyph") {
        f.default_glyph = v.trim().chars().take(MAX_NAME).collect();
    }
    if let Some(v) = str_param(p, "description") {
        f.description = v.chars().take(2000).collect();
    }
    if let Some(v) = str_param(p, "license") {
        f.license = v.chars().take(200).collect();
    }
    let out = json!(f);
    s.edit("Font Properties", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).font = Some(f);
        Ok(())
    })?;
    Ok(json!({ "font": out }))
}

fn kern(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.kern";
    let mut f = design(s, CMD)?;
    let pairs: Vec<String> = match (str_param(p, "pair"), str_param(p, "left"), str_param(p, "right")) {
        (Some(pair), _, _) => {
            let v = pair.split_whitespace().collect::<Vec<_>>();
            if v.len() != 2 {
                return Err(bad(CMD, "`pair` is two glyphs with a space between: \"A V\""));
            }
            vec![v.join(" ")]
        }
        (None, Some(l), Some(r)) => {
            let (l, r) = (glyph_set(&f, l, CMD)?, glyph_set(&f, r, CMD)?);
            l.iter().flat_map(|a| r.iter().map(move |b| format!("{a} {b}"))).take(MAX_CLASS * MAX_CLASS).collect()
        }
        _ => return Err(bad(CMD, "give a `pair` (\"A V\"), or `left` and `right` glyphs or @classes")),
    };
    let value = p
        .get("value")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && v.abs() <= f.size * 2.0)
        .ok_or_else(|| bad(CMD, format!("`value` is mm, at most {} either way", f.size * 2.0)))?;
    for key in &pairs {
        if value == 0.0 {
            f.kerning.remove(key);
        } else {
            f.kerning.insert(key.clone(), value);
        }
    }
    let all = json!(f.kerning);
    s.edit("Kern Glyph Pair", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).font = Some(f);
        Ok(())
    })?;
    Ok(json!({ "pairs": all, "set": pairs.len() }))
}

/// Glyphs in a kerning class at most, and classes in a font.
const MAX_CLASS: usize = 128;

/// The glyphs `text` names: a class (`@name`), glyph names with spaces between, or characters.
fn glyph_set(f: &FontDesign, text: &str, cmd: &str) -> Result<Vec<String>> {
    let text = text.trim();
    let set: Vec<String> = if let Some(name) = text.strip_prefix('@') {
        f.classes.get(name.trim()).cloned().ok_or_else(|| bad(cmd, format!("no kerning class `{}` (embroidery.fontDoc.kernClass)", name.trim())))?
    } else if text.contains(char::is_whitespace) {
        text.split_whitespace().map(str::to_string).collect()
    } else {
        text.chars().map(String::from).collect()
    };
    if set.is_empty() || set.len() > MAX_CLASS {
        return Err(bad(cmd, format!("a set of glyphs is 1 to {MAX_CLASS}")));
    }
    Ok(set)
}

/// Kerning Class (PU-12): a named set of glyphs kerned together.
fn kern_class(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.kernClass";
    let mut f = design(s, CMD)?;
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty() && n.chars().count() <= 40 && !n.starts_with('@'));
    let name = name.ok_or_else(|| bad(CMD, "`name` is 1 to 40 characters (without @)"))?.to_string();
    match str_param(p, "glyphs").map(str::trim).filter(|g| !g.is_empty()) {
        None => {
            f.classes.remove(&name);
        }
        Some(g) => {
            if g.starts_with('@') {
                return Err(bad(CMD, "a class holds glyphs, not other classes"));
            }
            let set = glyph_set(&f, g, CMD)?;
            if !f.classes.contains_key(&name) && f.classes.len() >= MAX_CLASS {
                return Err(bad(CMD, format!("a font keeps at most {MAX_CLASS} kerning classes")));
            }
            f.classes.insert(name, set);
        }
    }
    let classes = json!(f.classes);
    s.edit("Kerning Class", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).font = Some(f);
        Ok(())
    })?;
    Ok(json!({ "classes": classes }))
}

/// The glyphs of `font` (all, or those in `only`; by name), with their widths here (points, at
/// `k` times the font's size).
fn font_glyphs<'a>(font: &'a EmbFont, only: Option<&[String]>, k: f64) -> Vec<(String, &'a Glyph, f64)> {
    let mut out: Vec<(String, &Glyph, f64)> = font
        .glyphs
        .iter()
        .filter(|(n, _)| only.is_none_or(|o| o.contains(n)))
        .map(|(n, g)| (n.clone(), g, font.advance_of(n).unwrap_or(0.0) * g.unit * k))
        .collect();
    // in character order, ligatures after
    out.sort_by(|a, b| (a.0.chars().count(), a.0.as_str()).cmp(&(b.0.chars().count(), b.0.as_str())));
    out.truncate(MAX_DOC_GLYPHS);
    out
}

/// The art of glyph `g`, `k` times its size, placed on artboard `board` of a font document whose
/// baseline is `descent` (mm) up from its bottom, made visible (font glyph layers are hidden).
fn glyph_nodes(d: &mut Document, g: &Glyph, k: f64, board: Rect, descent: f64) -> Vec<Arc<Node>> {
    let m = Affine::translate((board.x0, board.y1 - descent * PT_PER_MM)) * Affine::scale(k) * Affine::translate((0.0, -g.baseline));
    g.art
        .children()
        .map(|c| {
            c.iter()
                .map(|n| {
                    let mut n = d.reid(n);
                    n.visible = true;
                    n.transform(m, true);
                    Arc::new(n)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// How far below the baseline `font`'s glyphs reach (mm), at `k` times its size.
fn depth(font: &EmbFont, k: f64) -> f64 {
    font.glyphs
        .values()
        .filter_map(|g| Some((g.art.geometric_bounds()?.y1 - g.baseline) * k / PT_PER_MM))
        .filter(|v| v.is_finite())
        .fold(0.0f64, f64::max)
}

/// Type `text` in `family` and `style` at `size` (points): its outline (points, its baseline at 0
/// and its start at 0) and how far the next character would start.
fn type_glyph(family: &str, style: &str, text: &str, size: f64) -> (PathData, f64) {
    let st = CharStyle { font_family: family.to_string(), font_style: style.to_string(), size, ..CharStyle::default() };
    let t = TextObject::point(vectorcraft_geom::Point::ZERO, text, st);
    let lay = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &t);
    let mut path = PathData::new(Vec::new());
    for g in &lay.glyphs {
        path.subpaths.extend(PathData::from_bezpath(&g.outline).transformed(t.xf).subpaths);
    }
    let advance = vectorcraft_text::caret_position(&lay, text.len()).0.x;
    (path, if advance.is_finite() { advance.max(0.0) } else { 0.0 })
}

fn from_type(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.fromType";
    let db = vectorcraft_text::FontDb::global();
    let want = str_param(p, "family").map(str::trim).filter(|f| !f.is_empty()).ok_or_else(|| bad(CMD, "missing `family`"))?;
    let family =
        db.families().into_iter().find(|f| f.eq_ignore_ascii_case(want)).ok_or_else(|| bad(CMD, format!("no font family `{want}` is installed")))?;
    // a style the family has (the font database stands the nearest in for one it hasn't)
    let styles = db.styles(&family);
    let key = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>();
    let style = match str_param(p, "style").map(str::trim).filter(|s| !s.is_empty()) {
        Some(want) => styles
            .iter()
            .find(|s| key(s) == key(want))
            .cloned()
            .ok_or_else(|| bad(CMD, format!("{family} has no {want} style: one of {}", styles.join(", "))))?,
        // its regular, or the style nearest it
        None => db.face(&family, "Regular").map_or_else(|| "Regular".to_string(), |f| f.style.clone()),
    };
    let mut f = FontDesign::default();
    f.name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).map_or_else(|| family.clone(), |n| n.chars().take(100).collect());
    f.size = ranged(p, "size", 5.0..=200.0, CMD, "`size` is 5 to 200 mm")?.unwrap_or(f.size);
    // the type size whose capitals are `size` high, measured on the first capital it has
    let cap = ["H", "I", "X", "E"]
        .iter()
        .find_map(|c| type_glyph(&family, &style, c, 100.0).0.bounds().map(|b| b.height()).filter(|h| *h > 1e-6))
        .ok_or_else(|| bad(CMD, format!("{family} has no capitals to measure its size by")))?;
    let points = f.size * PT_PER_MM * 100.0 / cap;
    let mut names = glyph_names(p, CMD)?;
    if names.is_empty() {
        names = DEFAULT_CHARS.chars().map(String::from).collect();
    }
    let glyphs: Vec<(String, PathData, f64)> = names
        .into_iter()
        .map(|n| {
            let (path, advance) = type_glyph(&family, &style, &n, points);
            (n, path, advance)
        })
        .collect();
    // room below the baseline for the deepest descender, a space as the family's
    let deepest = glyphs.iter().filter_map(|(_, path, _)| path.bounds()).map(|b| b.y1).fold(0.0f64, f64::max) / PT_PER_MM;
    f.descent = (deepest + 2.0).max(f.size * 0.2);
    // a space as wide as the family's: measured between two letters (a lone space lays out as
    // nothing), else three tenths of the size
    let space = type_glyph(&family, &style, "H H", points).1 - type_glyph(&family, &style, "HH", points).1;
    f.space = if space.is_finite() && space > 0.0 { space / PT_PER_MM } else { f.size * 0.3 };
    let widths: Vec<(String, f64)> = glyphs.iter().map(|(n, _, w)| (n.clone(), *w)).collect();
    let (boards, guides) = glyph_boards(&widths, &f, 0.0, 1);
    let mut doc = Document::new(100.0, 100.0);
    doc.title = f.name.clone();
    doc.units = Unit::Millimeters;
    let paint = Paint::solid(Color::rgb8(0x1a, 0x3e, 0x8c));
    let mut art = Vec::new();
    for ((name, path, _), board) in glyphs.iter().zip(&boards) {
        if path.is_empty() {
            continue;
        }
        let path = path.transformed(Affine::translate((board.rect.x0, board.rect.y1 - f.descent * PT_PER_MM)));
        let mut n = Node::path(doc.alloc_id(), path, Appearance::basic(paint.clone(), Paint::None, 0.0));
        n.name = Some(name.clone());
        // digitized as Convert Artwork does it: satin for thin strokes, fill for wide ones
        n.embroidery = Some(Box::new(vectorcraft_embroidery::suggest(&n)));
        art.push(Arc::new(n));
    }
    if let Some(layer) = doc.layers.first_mut()
        && let Some(ch) = Arc::make_mut(layer).children_mut()
    {
        ch.extend(art);
    }
    doc.artboards = boards;
    doc.guides = guides;
    doc.embroidery = Some(Box::new(DocEmbroidery { font: Some(f), ..DocEmbroidery::default() }));
    let count = doc.artboards.len();
    s.add_document(doc, None);
    Ok(json!({ "glyphs": count, "family": family }))
}

fn edit_font(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.font.edit";
    let name = str_param(p, "name").ok_or_else(|| bad(CMD, "missing `name`"))?;
    let _ = ensure_fonts(s);
    let font = loaded(s, name).ok_or_else(|| not_loaded(CMD, name))?;
    let i = &font.info;
    let unit = font.glyphs.values().next().map_or(1.0, |g| g.unit);
    let size = if i.size > 0.0 { i.size } else { 20.0 };
    // the font's properties, its font units in millimetres
    let mm = |v: f64| v * unit / PT_PER_MM;
    let f = FontDesign {
        name: i.name.clone(),
        size,
        descent: depth(&font, 1.0).max(size * 0.4) + 2.0,
        leading: if i.leading > 0.0 { mm(i.leading) / size } else { 1.4 },
        space: i.horiz_adv_x_space.map_or(size * 0.3, mm),
        letter_case: i.letter_case.clone(),
        default_glyph: i.default_glyph.clone(),
        min_scale: i.min_scale,
        max_scale: i.max_scale,
        kerning: i.kerning_pairs.iter().map(|(k, v)| (k.split_whitespace().collect::<Vec<_>>().join(" "), mm(*v))).collect(),
        classes: Default::default(),
        description: i.description.clone(),
        license: i.font_license.clone(),
    };
    let glyphs = font_glyphs(&font, None, 1.0);
    let widths: Vec<(String, f64)> = glyphs.iter().map(|(n, _, w)| (n.clone(), *w)).collect();
    let (boards, guides) = glyph_boards(&widths, &f, 0.0, 1);
    let mut doc = Document::new(100.0, 100.0);
    doc.title = f.name.clone();
    doc.units = Unit::Millimeters;
    let mut art = Vec::new();
    for ((_, g, _), board) in glyphs.iter().zip(&boards) {
        art.extend(glyph_nodes(&mut doc, g, 1.0, board.rect, f.descent));
    }
    if let Some(layer) = doc.layers.first_mut()
        && let Some(ch) = Arc::make_mut(layer).children_mut()
    {
        ch.extend(art);
    }
    doc.artboards = boards;
    doc.guides = guides;
    doc.embroidery = Some(Box::new(DocEmbroidery { font: Some(f), ..DocEmbroidery::default() }));
    let count = doc.artboards.len();
    s.add_document(doc, None);
    Ok(json!({ "glyphs": count }))
}

fn merge_font(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fontDoc.merge";
    let f = design(s, CMD)?;
    let name = str_param(p, "font").ok_or_else(|| bad(CMD, "missing `font`"))?;
    let _ = ensure_fonts(s);
    let font = loaded(s, name).ok_or_else(|| not_loaded(CMD, name))?;
    let only = glyph_names(p, CMD)?;
    let d = &s.doc()?.doc;
    let have: Vec<String> = d.artboards.iter().map(|a| a.name.clone()).collect();
    // their capitals as tall as this font's
    let k = f.size / if font.info.size > 0.0 { font.info.size } else { f.size };
    let glyphs: Vec<(String, &Glyph, f64)> =
        font_glyphs(&font, (!only.is_empty()).then_some(only.as_slice()), k).into_iter().filter(|(n, _, _)| !have.contains(n)).collect();
    if glyphs.is_empty() {
        return Err(bad(CMD, format!("{} has no glyphs this font hasn't already", font.info.name)));
    }
    if have.len() + glyphs.len() > MAX_DOC_GLYPHS {
        return Err(bad(CMD, format!("{MAX_DOC_GLYPHS} glyphs at most")));
    }
    let top = d.artboards.iter().map(|a| a.rect.y1).fold(0.0f64, f64::max) + GAP * PT_PER_MM;
    let next = d.artboards.iter().map(|a| a.id).max().unwrap_or(0).saturating_add(1);
    let widths: Vec<(String, f64)> = glyphs.iter().map(|(n, _, w)| (n.clone(), *w)).collect();
    let (boards, guides) = glyph_boards(&widths, &f, top, next);
    let parent = s.doc()?.insertion_parent();
    let added = boards.len();
    s.edit("Merge Embroidery Font", |d, _| {
        for ((_, g, _), board) in glyphs.iter().zip(&boards) {
            for n in glyph_nodes(d, g, k, board.rect, f.descent) {
                d.insert(parent, usize::MAX, (*n).clone())?;
            }
        }
        d.artboards.extend(boards.iter().cloned());
        d.guides.extend(guides.iter().cloned());
        Ok(())
    })?;
    Ok(json!({ "added": added }))
}

/// The art on each glyph's artboard: the top-level objects of the visible layers whose middle is
/// inside it, by artboard. An object whose middle is on no artboard (a script "j" whose tail
/// sweeps back under the letters before it, an apostrophe above the capitals) goes to the artboard
/// it overlaps most.
fn glyph_art(d: &Document) -> Vec<(&Artboard, Vec<Arc<Node>>)> {
    let boards: Vec<&Artboard> =
        d.artboards.iter().filter(|a| !a.name.trim().is_empty() && a.name.chars().count() <= MAX_NAME).take(MAX_DOC_GLYPHS).collect();
    let mut art: Vec<Vec<Arc<Node>>> = vec![Vec::new(); boards.len()];
    let tops = d.layers.iter().filter(|l| l.visible).flat_map(|l| l.children().into_iter().flatten());
    for n in tops {
        let Some(b) = n.geometric_bounds() else { continue };
        let home = boards.iter().position(|a| a.rect.contains(b.center())).or_else(|| {
            let overlap = |a: &Artboard| {
                let i = a.rect.intersect(b);
                i.width().max(0.0) * i.height().max(0.0)
            };
            boards.iter().enumerate().map(|(k, a)| (k, overlap(a))).filter(|(_, o)| *o > 0.0).max_by(|x, y| x.1.total_cmp(&y.1)).map(|(k, _)| k)
        });
        if let Some(list) = home.and_then(|k| art.get_mut(k)) {
            list.push(n.clone());
        }
    }
    boards.into_iter().zip(art).collect()
}

fn info(s: &mut Session, _: &Value) -> Result<Value> {
    let f = design(s, "embroidery.fontDoc.info")?;
    let d = &s.doc()?.doc;
    let glyphs: Vec<Value> =
        glyph_art(d).iter().map(|(a, art)| json!({ "name": a.name, "width": a.rect.width() / PT_PER_MM, "done": !art.is_empty() })).collect();
    let missing = glyphs.iter().filter(|g| g["done"] == false).count();
    Ok(json!({ "font": f, "glyphs": glyphs, "missing": missing }))
}

/// The font files of font document `d`: font.json and the glyphs' art (each glyph a layer, its
/// artboard's left edge at 0 and its baseline at 0, in points), and the glyphs without art.
fn font_files(d: &Document, f: &FontDesign) -> Result<(String, Document, Vec<String>)> {
    let pt = PT_PER_MM;
    let mut art = Document::new(100.0, 100.0);
    art.layers.clear();
    let (mut advances, mut missing) = (Map::new(), Vec::new());
    for (board, nodes) in glyph_art(d) {
        let name = board.name.trim();
        if nodes.is_empty() {
            missing.push(name.to_string());
            continue;
        }
        let r = board.rect;
        let m = Affine::translate((-r.x0, -(r.y1 - f.descent * pt)));
        let children: Vec<Arc<Node>> = nodes
            .iter()
            .map(|n| {
                let mut c = art.reid(n);
                c.transform(m, true);
                Arc::new(c)
            })
            .collect();
        let mut layer = Node::layer(art.alloc_id(), &format!("GlyphLayer-{name}"), LayerColor::Preset(0));
        if let Some(ch) = layer.children_mut() {
            *ch = children;
        }
        art.layers.push(Arc::new(layer));
        advances.insert(name.to_string(), json!(r.width()));
    }
    if art.layers.is_empty() {
        return Err(bad(SAVE, "nothing is digitized yet: draw each glyph on its artboard"));
    }
    let kerning: Map<String, Value> = f.kerning.iter().map(|(k, v)| (k.clone(), json!(v * pt))).collect();
    let json = json!({
        "name": f.name,
        "description": f.description,
        "font_license": f.license,
        "size": f.size,
        "leading": f.size * f.leading * pt,
        "min_scale": f.min_scale,
        "max_scale": f.max_scale,
        "letter_case": f.letter_case,
        "default_glyph": f.default_glyph,
        "horiz_adv_x_space": f.space * pt,
        "horiz_adv_x": advances,
        "kerning_pairs": kerning,
    });
    Ok((json.to_string(), art, missing))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let f = design(s, SAVE)?;
    let (json, art, missing) = font_files(&s.doc()?.doc, &f)?;
    let mut font = EmbFont::new(&json, vec![(art.clone(), 0.0, 1.0)]).map_err(|e| bad(SAVE, e))?;
    let folder = str_param(p, "folder").map(str::trim).filter(|f| !f.is_empty()).map(str::to_string);
    if let Some(dir) = &folder {
        crate::cmd::fileio::create_dir(dir)?;
        let at = |name: &str| std::path::Path::new(dir).join(name).to_string_lossy().to_string();
        crate::cmd::fileio::write_file(&at("font.json"), json.as_bytes())?;
        crate::cmd::fileio::write_file(&at(GLYPHS_FILE), &vectorcraft_format::save(&art, false))?;
        font.folder = Some(dir.clone());
    }
    let mut out = describe(&font);
    // the remembered folders first, so this font replaces one of the same name among them
    let _ = ensure_fonts(s);
    add_font(s, font);
    if let Some(dir) = &folder {
        remember_folder(s, dir);
    }
    if let Some(o) = out.as_object_mut() {
        o.insert("missing".into(), json!(missing));
    }
    Ok(out)
}
