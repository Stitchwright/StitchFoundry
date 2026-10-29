//! Lettering with embroidery fonts: fonts in Ink/Stitch's format, read from folders users have
//! (`embroidery.font.load`; none ship with StitchFoundry, and each keeps its own licence), and
//! text set with them (`embroidery.lettering`): each glyph's digitized art placed along the lines,
//! kerned and scaled, a group per glyph.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{Value, json};
use sf_core::attr::{
    Embroidery, LetterAdjust, LetterAlign, LetterJoin, LetterOrder, LetterSewing, LetterStitch, LetterTrim, Lettering, SmallText, StitchType,
};
use sf_core::model::{FillParams, Puff, SatinParams};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, Node, NodeId, NodeKind};
use vectorcraft_embroidery::fonts::{EmbFont, MAX_TEXT};
use vectorcraft_geom::Affine;

use super::{CommandSpec, EngineError, Point, Result, Session, always, bad, bool_or, cmd, has_doc, has_selection, point_param, str_param, targets};

const LOAD: &str = "embroidery.font.load";
const SET: &str = "embroidery.lettering";
const EDIT: &str = "embroidery.lettering.edit";
const LETTER: &str = "embroidery.lettering.letter";
/// Biggest font.json and glyph art read (bytes), most art files one font has, and most fonts
/// loaded at once.
const MAX_JSON: usize = 16 << 20;
const MAX_SVG: usize = 40 << 20;
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // fonts are read from folders on the desktop only
const MAX_FILES: usize = 64;
const MAX_FONTS: usize = 64;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.font.load",
            "Load Embroidery Font",
            [],
            None,
            "{path?: a font folder in Ink/Stitch's format (font.json, and ltr.svg or an ltr folder of SVGs), json?, svg?: the files' text instead} load an embroidery font for lettering; its glyphs sew the way their Ink/Stitch settings say. A folder is remembered: later sessions load it again the first time lettering needs a font (embroidery.font.forget forgets it); loading a font of the same name again replaces it → {name, glyphs, size, minSize, maxSize, license, folder}",
            always,
            load_font
        ),
        cmd!(
            "embroidery.fonts",
            "Embroidery Fonts",
            [],
            None,
            "{} the embroidery fonts: those built into the app (builtIn, with their licence's text), and those of the folders remembered from earlier sessions (loaded now, if they weren't) → {fonts: [{name, glyphs, size, minSize, maxSize, license, description, folder, builtIn, licenceText?}], warnings: the folders that failed to load}",
            always,
            list_fonts
        ),
        cmd!(
            "embroidery.teamNames.export",
            "Export Team Names",
            [],
            None,
            "{names: [text, …] (1–500; a line break sets two lines), dir: the folder to write to (made if missing), id?: the lettering (default: the selected one), format?: dst|exp|jef|pes|pec|vp3|xxx|u01 (the design's machine, else dst), prefix?: what the file names start with} team names: one machine file per name, the design with the lettering set to that name, named after it (characters files can't have replaced, the same name twice kept apart). The design itself stays as it is → {files, count}",
            has_selection,
            team_names_export
        ),
        cmd!(
            "embroidery.teamNames.layout",
            "Lay Out Team Names",
            [],
            None,
            "{names: [text, …] (1–500), id?: the lettering (default: the selected one), columns?: n (1–50; default: about as many as rows), gap?: mm between names (10; 0–200)} team names together in one design (a large hoop, patches cut apart after): a copy of the lettering for each name, each still lettering, in rows and columns `gap` apart starting where the lettering is, in a group, selected → {id, count, columns, rows}",
            has_selection,
            team_names_layout
        ),
        cmd!(
            "embroidery.teamNames.worksheet",
            "Team Names Worksheet",
            [],
            None,
            "{names: [text, …] (1–500), id?: the lettering (default: the selected one), page?: a4|letter (a4)} a worksheet listing each name with its size, stitches and sewing time, and their totals, as a new document → {count, stitches, seconds}",
            has_selection,
            team_names_worksheet
        ),
        cmd!(
            query "embroidery.teamNames.read",
            "Read Team Names",
            [],
            None,
            "{csv?: text, path?: a CSV or text file, column?: n (0: the first), header?: bool (the first row names the columns: left out)} the names in a column of a table (commas or semicolons, quoted fields), or one a line → {names}",
            always,
            team_names_read
        ),
        cmd!(
            "embroidery.font.forget",
            "Forget Embroidery Font",
            [],
            None,
            "{name} unload an embroidery font and forget its folder, so later sessions don't load it; lettering set in it stays as it is → {name}",
            always,
            forget_font
        ),
        cmd!(
            "embroidery.lettering",
            "Embroidery Lettering",
            [],
            None,
            "{text, font, size?: mm (its capitals' height; default: the font's own), at?: [x, y] (where the first line's baseline starts, or its middle or end by `align`; default: the first artboard's centre), leading?: × the font's line height (1), align?: left|centre|right (left), arc?: mm (the baseline round a circle that radius: positive arched over its top, negative along its bottom; 0 straight), spiral?: mm (with `arc`: its radius shrinks this much a turn, the line winding in; negative, out; 0), puff?: bool (3D puff: the glyphs' satin sewn over foam, see embroidery.puff), vertical?: bool (letters one under another, each centred; lines side by side; straight lettering only), path?: id|true (a line or shape to set the text along, from its start, upright to it: its id, or true for the selected one), stitch?: font|satin|fill (as the font digitizes them; satin: filled parts sewn as satin across; fill: each letter one tatami fill the shape of what it sews), small?: auto|on|off (small text: a centre-run underlay, half the pull compensation, short stitches; auto: under 6 mm), applique?: \"#rrggbb\"|true|false (appliqué letters: each glyph one patch of fabric that colour, the shape of what it sews, with a placement line, tack-down and cover; true: light grey), order?: left|right|centre (the letters sewn left to right, right to left or from the middle out, as for caps; left), lines?: down|up (the lines sewn from the first down or the last up; down), join?: closest|baseline (each letter started where closest to the last, or at its baseline's start and stopped at its end, the thread running on along the baseline; closest), trim?: auto|letter|word|line (the thread cut after each letter, word or line; auto: where the jumps need it)} set text in a loaded embroidery font: each glyph its digitized art, kerned and scaled, a group per glyph inside one for the lettering, which remembers what it says (embroidery.lettering.edit sets it again); a size outside what the font sews well at comes with a warning → {id, glyphs, warnings}",
            has_doc,
            lettering
        ),
        cmd!(
            "embroidery.lettering.edit",
            "Edit Lettering",
            [],
            None,
            "{id?, text?, font?, size?: mm, leading?, align?, arc?: mm, spiral?: mm, vertical?: bool, path?: id|true|null (along a line; null: straight again), puff?: bool, stitch?: font|satin|fill, small?: auto|on|off, applique?: \"#rrggbb\"|false, order?, lines?, join?, trim?} set the selected lettering (in an embroidery font) again with a new text, font, size or line spacing, where it is now, moved, turned and scaled as it is; its font must be loaded → {id, glyphs, warnings}",
            has_selection,
            edit_lettering
        ),
        cmd!(
            "embroidery.puff",
            "3D Puff",
            ["Embroidery"],
            None,
            "{ids?, on?: bool (default: on, or off when all the satin chosen is puff already), cap?: mm the end caps reach past a column's ends (0.5; 0–3)} 3D puff: the selected objects' satin (lettering's too, kept when it is set again) sewn over foam, a filled shape as satin across it (plain again, it fills again): dense (0.16 mm), no underlay, auto split or short stitches, the sides pushed out 0.35 mm, a cap across each end that cuts and covers the foam. Columns 3 to 7 mm wide puff best; sew the puff in a colour of its own so the machine stops to lay the foam → {satin: pieces set, other: pieces that aren't satin (left as they are), on}",
            has_selection,
            puff
        ),
        cmd!(
            "embroidery.puff.last",
            "Sew Puff Last",
            ["Embroidery"],
            None,
            "{} the 3D puff sewn last, the way it is laid: in each layer (and plain group), the objects that are all puff moved after the others, in the order they were in, and a stop after the last piece sewn before the first of them, so the foam is laid over what is already sewn (none when the design starts with puff) → {puff: objects moved last, stop}",
            has_doc,
            puff_last
        ),
        cmd!(
            "embroidery.lettering.letter",
            "Adjust Letter",
            [],
            None,
            "{id?, index?: the letter's place among the lettering's letters as the text sets them (from 0; default: the letter selected inside the lettering), dx?: mm along its line, dy?: mm down from it, scale?: × (0.1–10), angle?: degrees clockwise (-360–360), reset?: true puts it back} one letter of the selected lettering moved, scaled or turned about the middle of its baseline; what isn't given stays as it was; kept when the lettering is set again, moved, scaled and turned with it → {id, index, letter: {dx, dy, scale, angle}, glyphs}",
            has_selection,
            adjust_letter
        ),
    ]
}

/// What a font tells about itself.
pub(super) fn describe(f: &EmbFont) -> Value {
    let i = &f.info;
    let size = |k: f64| (i.size > 0.0 && k > 0.0).then_some(i.size * k);
    json!({
        "name": i.name,
        "glyphs": f.glyphs.len(),
        "size": (i.size > 0.0).then_some(i.size),
        "minSize": size(i.min_scale),
        "maxSize": size(i.max_scale),
        "license": i.font_license,
                "description": i.description,
        "folder": f.folder,
        "builtIn": f.built_in.is_some(),
    })
}

/// The `<sodipodi:guide>` labelled `label`: its height (`position="x,y"`, user units up from the
/// bottom of the page).
fn guide_y(svg: &str, label: &str) -> Option<f64> {
    let want = format!("inkscape:label=\"{label}\"");
    let mut rest = svg;
    while let Some(i) = rest.find("<sodipodi:guide") {
        let tail = rest.get(i..)?;
        let end = tail.find('>').unwrap_or(tail.len());
        let el = tail.get(..end)?;
        if el.contains(&want) {
            let pos = el.split("position=\"").nth(1)?.split('"').next()?;
            let y = pos.split([',', ' ']).filter(|v| !v.is_empty()).nth(1)?.parse::<f64>().ok()?;
            return y.is_finite().then_some(y);
        }
        rest = tail.get(end..)?;
    }
    None
}

/// The root element's `viewBox` height (user units).
fn view_box_height(svg: &str) -> Option<f64> {
    let tail = svg.get(svg.find("<svg")?..)?;
    let el = tail.get(..tail.find('>')?)?;
    let vb = el.split("viewBox=\"").nth(1)?.split('"').next()?;
    let h = vb.split([',', ' ']).filter(|v| !v.is_empty()).nth(3)?.parse::<f64>().ok()?;
    (h.is_finite() && h > 0.0).then_some(h)
}

/// One SVG of glyph art: imported, sewn the way its Ink/Stitch settings say, with its glyphs'
/// baseline (pt from the top of the page) and its font unit (pt).
fn glyph_art(svg: &str) -> Result<(Document, f64, f64)> {
    let (mut doc, _) = vectorcraft_svg::import_with_report(svg).map_err(|e| bad(LOAD, format!("the glyph art can't be read: {e}")))?;
    vectorcraft_embroidery::inkstitch::stitch_all(&mut doc);
    let page = doc.artboards.first().map(|a| a.rect.height()).filter(|h| h.is_finite() && *h > 0.0).unwrap_or(0.0);
    // a user unit: the page over its viewBox (no viewBox: CSS pixels)
    let unit = view_box_height(svg).filter(|_| page > 0.0).map_or(0.75, |h| page / h);
    let baseline = page - guide_y(svg, "baseline").unwrap_or(0.0) * unit;
    Ok((doc, baseline, unit))
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // fonts are read from folders on the desktop only
fn read_text(path: &str, max: usize, what: &str) -> Result<String> {
    let bytes = crate::cmd::fileio::read_file(path)?;
    if bytes.len() > max {
        return Err(bad(LOAD, format!("{what} is too big ({} MB at most)", max >> 20)));
    }
    String::from_utf8(bytes).map_err(|_| bad(LOAD, format!("{what} is not text")))
}

/// A font's glyph art: Ink/Stitch's SVG, or a StitchFoundry document of glyph layers (its glyphs
/// on their baselines at 0, in points).
pub(super) enum Art {
    Svg(String),
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))] // fonts are read from folders on the desktop only
    Native(Vec<u8>),
}

/// The file a font made in StitchFoundry keeps its glyphs in, next to its font.json.
pub(super) const GLYPHS_FILE: &str = "glyphs.stitchfoundry";

/// The font's files in folder `dir`: font.json and its art (glyphs.stitchfoundry; ltr.svg, →.svg
/// or rtl.svg; else the SVGs in an ltr folder).
#[cfg(not(target_arch = "wasm32"))]
fn read_folder(dir: &str) -> Result<(String, Vec<Art>)> {
    let at = |name: &str| std::path::Path::new(dir).join(name).to_string_lossy().to_string();
    let json = read_text(&at("font.json"), MAX_JSON, "font.json")?;
    let mut svgs = Vec::new();
    if std::path::Path::new(&at(GLYPHS_FILE)).is_file() {
        let bytes = crate::cmd::fileio::read_file(&at(GLYPHS_FILE))?;
        if bytes.len() > MAX_SVG {
            return Err(bad(LOAD, format!("the glyph art is too big ({} MB at most)", MAX_SVG >> 20)));
        }
        svgs.push(Art::Native(bytes));
    } else if let Some(file) = ["ltr.svg", "→.svg", "rtl.svg", "←.svg"].iter().map(|n| at(n)).find(|p| std::path::Path::new(p).is_file()) {
        svgs.push(Art::Svg(read_text(&file, MAX_SVG, "the glyph art")?));
    } else if let Ok(rd) = std::fs::read_dir(at("ltr")) {
        let mut files: Vec<String> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("svg")))
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        files.sort();
        for f in files.iter().take(MAX_FILES) {
            svgs.push(Art::Svg(read_text(f, MAX_SVG, "the glyph art")?));
        }
    }
    if svgs.is_empty() {
        return Err(bad(LOAD, format!("{dir}: no glyph art (ltr.svg, or SVGs in an ltr folder)")));
    }
    Ok((json, svgs))
}

#[cfg(target_arch = "wasm32")]
fn read_folder(dir: &str) -> Result<(String, Vec<Art>)> {
    Err(bad(LOAD, format!("{dir}: no file system here (pass json and svg)")))
}

/// A font from its files.
fn font_from(json: &str, arts: &[Art]) -> Result<EmbFont> {
    let art = arts
        .iter()
        .map(|a| match a {
            Art::Svg(svg) => glyph_art(svg),
            // our own: stitched already, its glyphs on their baselines at 0, in points
            Art::Native(bytes) => {
                vectorcraft_format::load(bytes).map(|doc| (doc, 0.0, 1.0)).map_err(|e| bad(LOAD, format!("the glyph art can't be read: {e}")))
            }
        })
        .collect::<Result<Vec<_>>>()?;
    EmbFont::new(json, art).map_err(|e| bad(LOAD, e))
}

/// Remembers font folder `dir` for later sessions.
pub(super) fn remember_folder(s: &mut Session, dir: &str) {
    let dirs = &mut s.prefs.embroidery_font_dirs;
    if !dirs.iter().any(|d| d == dir) {
        if dirs.len() >= MAX_FONTS {
            dirs.remove(0);
        }
        dirs.push(dir.to_string());
    }
}

/// Adds `font` to the loaded fonts, in place of one of the same name.
pub(super) fn add_font(s: &mut Session, font: EmbFont) {
    let fonts = &mut s.embroidery_fonts;
    fonts.retain(|f| !f.info.name.eq_ignore_ascii_case(&font.info.name));
    if fonts.len() >= MAX_FONTS {
        fonts.remove(0);
    }
    fonts.push(Arc::new(font));
}

/// Loads the font folders remembered from earlier sessions, the first time a font is needed.
/// → What failed to load (a folder moved or gone stays remembered: a drive may be away).
pub(super) fn ensure_fonts(s: &mut Session) -> Vec<String> {
    if std::mem::replace(&mut s.embroidery_fonts_ready, true) {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    // the fonts built into the app first: a folder loaded with the same name replaces one
    for b in vectorcraft_embroidery::fonts::BUNDLED_FONTS {
        match font_from(b.json, &[Art::Native(b.glyphs.to_vec())]) {
            Ok(mut font) => {
                font.built_in = Some(b.licence);
                add_font(s, font);
            }
            Err(e) => warnings.push(format!("the built-in embroidery font {} didn't load: {e}", b.folder)),
        }
    }
    for dir in s.prefs.embroidery_font_dirs.clone() {
        // one loaded already this session stays as it is
        if s.embroidery_fonts.iter().any(|f| f.folder.as_deref() == Some(dir.as_str())) {
            continue;
        }
        match read_folder(&dir).and_then(|(json, svgs)| font_from(&json, &svgs)) {
            Ok(mut font) => {
                font.folder = Some(dir);
                add_font(s, font);
            }
            Err(e) => warnings.push(format!("the embroidery font in {dir} didn't load: {e}")),
        }
    }
    warnings
}

fn load_font(s: &mut Session, p: &Value) -> Result<Value> {
    let (json, svgs, folder) = match (str_param(p, "json"), str_param(p, "svg"), str_param(p, "path")) {
        (Some(j), Some(v), _) => {
            if j.len() > MAX_JSON || v.len() > MAX_SVG {
                return Err(bad(LOAD, "the font is too big"));
            }
            (j.to_string(), vec![Art::Svg(v.to_string())], None)
        }
        (_, _, Some(path)) => {
            let (json, svgs) = read_folder(path)?;
            (json, svgs, Some(path.to_string()))
        }
        _ => return Err(bad(LOAD, "give the font's folder (`path`), or its `json` and `svg`")),
    };
    // the remembered folders first, so this one replaces a font of the same name among them
    let _ = ensure_fonts(s);
    let mut font = font_from(&json, &svgs)?;
    font.folder = folder.clone();
    let out = describe(&font);
    add_font(s, font);
    // remembered for later sessions
    if let Some(dir) = folder {
        remember_folder(s, &dir);
    }
    Ok(out)
}

fn list_fonts(s: &mut Session, _: &Value) -> Result<Value> {
    let warnings = ensure_fonts(s);
    let fonts: Vec<Value> = s
        .embroidery_fonts
        .iter()
        .map(|f| {
            let mut v = describe(f);
            if let Some(text) = f.built_in {
                v["licenceText"] = json!(text);
            }
            v
        })
        .collect();
    Ok(json!({ "fonts": fonts, "warnings": warnings }))
}

fn forget_font(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.font.forget";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?;
    // a font of a remembered folder is known by name once loaded; what fails says so when listed
    let _ = ensure_fonts(s);
    let i = s
        .embroidery_fonts
        .iter()
        .position(|f| f.info.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| bad(CMD, format!("no embroidery font `{name}` is loaded")))?;
    let font = s.embroidery_fonts.remove(i);
    if let Some(dir) = &font.folder {
        s.prefs.embroidery_font_dirs.retain(|d| d != dir);
    }
    Ok(json!({ "name": font.info.name }))
}

/// The size (mm) `font` was digitized at.
pub(super) fn own_size(font: &EmbFont) -> f64 {
    if font.info.size > 0.0 { font.info.size } else { 20.0 }
}

/// What `font` sews well at, said when `size` (mm) is outside it.
pub(super) fn size_warnings(font: &EmbFont, size: f64) -> Vec<String> {
    let (i, own) = (&font.info, own_size(font));
    let k = size / own;
    let mut warnings = Vec::new();
    if i.min_scale > 0.0 && k < i.min_scale - 1e-9 {
        warnings.push(format!("{} sews well from {:.1} mm", i.name, own * i.min_scale));
    }
    if i.max_scale > 0.0 && k > i.max_scale + 1e-9 {
        warnings.push(format!("{} sews well up to {:.1} mm", i.name, own * i.max_scale));
    }
    warnings
}

/// A loaded font, by name (any case).
pub(super) fn loaded(s: &Session, name: &str) -> Option<Arc<EmbFont>> {
    s.embroidery_fonts.iter().find(|f| f.info.name.eq_ignore_ascii_case(name)).cloned()
}

pub(super) fn not_loaded(cmd: &str, name: &str) -> EngineError {
    bad(cmd, format!("no embroidery font `{name}` is loaded (embroidery.font.load)"))
}

/// `text`, when given: 1 to [`MAX_TEXT`] characters, not all blank.
fn text_param<'a>(p: &'a Value, cmd: &str) -> Result<Option<&'a str>> {
    match p.get("text") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(t)) if !t.trim().is_empty() && t.chars().count() <= MAX_TEXT => Ok(Some(t.as_str())),
        Some(_) => Err(bad(cmd, format!("`text` is 1 to {MAX_TEXT} characters"))),
    }
}

/// A number param in `range`, when given.
pub(super) fn ranged(p: &Value, key: &str, range: std::ops::RangeInclusive<f64>, cmd: &str, what: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|v| range.contains(v)).map(Some).ok_or_else(|| bad(cmd, what)),
    }
}

/// The lettering among `ids`, or holding one of them (a glyph selected inside it), and what it
/// says.
fn lettering_of(d: &Document, ids: &[NodeId]) -> Option<(NodeId, Lettering)> {
    ids.iter().find_map(|id| {
        let mut at = Some(*id);
        while let Some(c) = at {
            if let Some(StitchType::Lettering(l)) = d.node(c)?.embroidery.as_deref().map(|e| &e.stitch) {
                return Some((c, (**l).clone()));
            }
            at = d.parent_of(c);
        }
        None
    })
}

/// The lettering group's name: its text (the start of it) and font.
pub(super) fn lettering_name(spec: &Lettering) -> String {
    let short: String = spec.text.chars().take(40).map(|c| if c.is_control() { ' ' } else { c }).collect();
    format!("{short} ({})", spec.font)
}

/// The glyphs `spec` sets in `font`, placed by its frame: a group per glyph, its art sewn as the
/// font says.
pub(super) fn glyphs(d: &mut Document, font: &EmbFont, spec: &Lettering) -> Vec<Arc<Node>> {
    glyphs_indexed(d, font, spec).into_iter().map(|g| g.1).collect()
}

/// [`glyphs`], each with its letter's place among those the text sets.
fn glyphs_indexed(d: &mut Document, font: &EmbFont, spec: &Lettering) -> Vec<(usize, Arc<Node>)> {
    let i = &font.info;
    let pt = vectorcraft_embroidery::PT_PER_MM;
    let k = spec.size / own_size(font);
    let frame = Affine::new(spec.frame.0);
    let placed = font.layout(&spec.text);
    // each line's width (font units), to align it
    let mut widths: Vec<f64> = Vec::new();
    for g in &placed {
        if widths.len() <= g.line {
            widths.resize(g.line + 1, 0.0);
        }
        if let Some(w) = widths.get_mut(g.line) {
            *w = w.max(g.x + g.advance);
        }
    }
    let r = spec.arc * vectorcraft_embroidery::PT_PER_MM;
    let turn = spec.spiral * vectorcraft_embroidery::PT_PER_MM;
    let line: Vec<vectorcraft_geom::Point> = spec.path.iter().map(|q| vectorcraft_geom::Point::new(q.x, q.y)).collect();
    // where a glyph's baseline middle goes: along a drawn line, or straight or round the arc
    let base = |a: f64, w: f64| if line.len() >= 2 { place_on(&line, a, w) } else { place(r, turn, a, w) };
    // stacked: each letter's row in its line
    let vertical = spec.vertical && r == 0.0 && line.len() < 2;
    let mut rows = Vec::with_capacity(placed.len());
    let mut in_line: Vec<usize> = Vec::new();
    for g in &placed {
        if in_line.len() <= g.line {
            in_line.resize(g.line + 1, 0);
        }
        let row = in_line.get(g.line).copied().unwrap_or(0);
        rows.push(row);
        if let Some(n) = in_line.get_mut(g.line) {
            *n += 1;
        }
    }
    let mut out = Vec::new();
    let origin = vectorcraft_geom::Point::new(0.0, 0.0);
    for (index, g) in placed.iter().enumerate() {
        let Some(glyph) = font.glyphs.get(&g.glyph) else { continue };
        let u = glyph.unit * k;
        // the letter moved, scaled or turned about its baseline's middle
        let adjust =
            spec.letters.iter().find(|a| a.index == index).map_or(Affine::IDENTITY, |a| {
                Affine::translate((a.dx * pt, a.dy * pt)) * Affine::rotate(a.angle.to_radians()) * Affine::scale(a.scale)
            });
        let width = widths.get(g.line).copied().unwrap_or(0.0);
        // the glyph's art from its page to its place: the middle of its baseline along the line
        // (from the origin, by the alignment), down to its line, round the arc
        let mut along = (g.x + g.advance * 0.5 - width * spec.align.share()) * u;
        let mut down = g.line as f64 * i.leading * spec.leading * u;
        if vertical {
            // one under another, 1.4 capitals apart; the lines side by side
            (along, down) = (down, rows.get(index).copied().unwrap_or(0) as f64 * spec.size * pt * 1.4 * spec.leading);
        }
        let m = frame
            * base(along, down)
            * adjust
            * Affine::translate((-g.advance * 0.5 * u, 0.0))
            * Affine::scale(k)
            * Affine::translate((0.0, -glyph.baseline));
        let children: Vec<Arc<Node>> = glyph
            .art
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
            .unwrap_or_default();
        if children.is_empty() {
            continue;
        }
        let mut grp = Node::group(d.alloc_id(), children);
        grp.name = Some(g.glyph.clone());
        match spec.stitch {
            LetterStitch::Font => {}
            LetterStitch::Satin => satin_all(&mut grp),
            LetterStitch::Fill => fill_glyph(d, &mut grp),
        }
        if let Some(fabric) = spec.applique {
            applique_glyph(d, &mut grp, fabric);
        } else if spec.puff.is_some() {
            puff_all(&mut grp, spec.puff, &mut (0, 0));
        }
        if spec.small.applies(spec.size) {
            small_all(&mut grp);
        }
        // its baseline's start and end
        let start = frame * base(along, down) * adjust * Affine::translate((-g.advance * 0.5 * u, 0.0)) * origin;
        let end = frame * base(along, down) * adjust * Affine::translate((g.advance * 0.5 * u, 0.0)) * origin;
        out.push(SetGlyph { index, line: g.line, word: g.word, along, start, end, art: grp });
    }
    sewn(out, &spec.sew)
}

/// A glyph set: its line, word and place along the line (points), its baseline's start and end
/// (document points) and its art.
struct SetGlyph {
    index: usize,
    line: usize,
    word: usize,
    along: f64,
    start: vectorcraft_geom::Point,
    end: vectorcraft_geom::Point,
    art: Node,
}

/// `glyphs` in the order `sew` says (the lines down or up, in each the letters left to right,
/// right to left or from the middle out), joined along the baseline if it says so, the thread
/// cut after the letters, words or lines it says.
fn sewn(glyphs: Vec<SetGlyph>, sew: &LetterSewing) -> Vec<(usize, Arc<Node>)> {
    let lines = glyphs.iter().map(|g| g.line + 1).max().unwrap_or(0);
    let mut line_order: Vec<usize> = (0..lines).collect();
    if sew.up {
        line_order.reverse();
    }
    // the glyphs' indices in sewing order, each with whether it is sewn leftwards
    let mut seq: Vec<(usize, bool)> = Vec::with_capacity(glyphs.len());
    for l in line_order {
        let mut on: Vec<(usize, f64)> = glyphs.iter().enumerate().filter(|(_, g)| g.line == l).map(|(i, g)| (i, g.along)).collect();
        on.sort_by(|a, b| a.1.total_cmp(&b.1));
        match sew.order {
            LetterOrder::Left => seq.extend(on.iter().map(|g| (g.0, false))),
            LetterOrder::Right => seq.extend(on.iter().rev().map(|g| (g.0, true))),
            LetterOrder::Centre => {
                let (to_first, to_last) = on.split_at((on.len().saturating_sub(1) / 2 + 1).min(on.len()));
                seq.extend(to_first.iter().rev().map(|g| (g.0, true)));
                seq.extend(to_last.iter().map(|g| (g.0, false)));
            }
        }
    }
    let mut slots: Vec<Option<SetGlyph>> = glyphs.into_iter().map(Some).collect();
    let mut list: Vec<(SetGlyph, bool)> = seq.into_iter().filter_map(|(i, left)| Some((slots.get_mut(i)?.take()?, left))).collect();
    // where the thread is cut: after a glyph whose next is another letter, word or line
    let cuts: Vec<bool> = list
        .windows(2)
        .map(|w| match w {
            [(a, _), (b, _)] => match sew.trim {
                LetterTrim::Auto => false,
                LetterTrim::Letter => true,
                LetterTrim::Word => a.word != b.word || a.line != b.line,
                LetterTrim::Line => a.line != b.line,
            },
            _ => false,
        })
        .collect();
    for (k, (g, left)) in list.iter_mut().enumerate() {
        if sew.join == LetterJoin::Baseline {
            let (from, to) = if *left { (g.end, g.start) } else { (g.start, g.end) };
            if let Some(e) = stitched_end(&mut g.art, false) {
                e.entry = Some(sf_core::Point::new(from.x, from.y));
            }
            if let Some(e) = stitched_end(&mut g.art, true) {
                e.exit = Some(sf_core::Point::new(to.x, to.y));
            }
        }
        if cuts.get(k).copied().unwrap_or(false)
            && let Some(e) = stitched_end(&mut g.art, true)
        {
            e.trim_after = true;
        }
    }
    list.into_iter().map(|(g, _)| (g.index, Arc::new(g.art))).collect()
}

/// Whether `n` is, or holds, art with stitches.
fn has_stitches(n: &Node) -> bool {
    match &n.kind {
        NodeKind::Group { children, .. } => children.iter().any(|c| has_stitches(c)),
        _ => n.embroidery.is_some(),
    }
}

/// The settings of the first (or `last`) art with stitches in `n`, sewing order.
fn stitched_end(n: &mut Node, last: bool) -> Option<&mut Embroidery> {
    match &mut n.kind {
        NodeKind::Group { children, .. } => {
            let i = if last { children.iter().rposition(|c| has_stitches(c)) } else { children.iter().position(|c| has_stitches(c)) }?;
            stitched_end(Arc::make_mut(children.get_mut(i)?), last)
        }
        _ => n.embroidery.as_deref_mut(),
    }
}

/// Every piece of satin in `n` sewn over foam (`puff`) or plain again (`None`); `counts`: the
/// satin pieces set and the other stitched pieces passed by.
/// A glyph as one appliqué patch in `fabric`: the shape of what its art sews, its thread the
/// cover's.
fn applique_glyph(d: &mut Document, grp: &mut Node, fabric: sf_core::Rgb) {
    let st = StitchType::Applique(sf_core::model::AppliqueParams { fabric_color: fabric, ..Default::default() });
    glyph_as_one(d, grp, st, Some(fabric), "Appliqué");
}

/// A glyph as one tatami fill, the shape of what its art sews.
fn fill_glyph(d: &mut Document, grp: &mut Node) {
    glyph_as_one(d, grp, StitchType::Fill(FillParams::default()), None, "Fill");
}

/// Filled parts of a glyph sewn as satin across them.
fn satin_all(n: &mut Node) {
    if let NodeKind::Group { children, .. } = &mut n.kind {
        for c in children.iter_mut() {
            satin_all(Arc::make_mut(c));
        }
        return;
    }
    if let Some(e) = n.embroidery.as_deref_mut()
        && matches!(e.stitch, StitchType::Fill(_))
    {
        e.stitch = StitchType::Satin(SatinParams::default());
    }
}

/// Small text: satin over a centre run only, with half the pull compensation and short
/// stitches round curves; fills with a light edge-run underlay, shorter stitches and half the
/// pull compensation; runs in short stitches.
fn small_all(n: &mut Node) {
    if let NodeKind::Group { children, .. } = &mut n.kind {
        for c in children.iter_mut() {
            small_all(Arc::make_mut(c));
        }
        return;
    }
    let Some(e) = n.embroidery.as_deref_mut() else { return };
    match &mut e.stitch {
        StitchType::Satin(s) => {
            // puff keeps its own (no underlay, dense)
            if s.puff.is_none() {
                s.underlay.auto = false;
                (s.underlay.center_run, s.underlay.edge_run, s.underlay.zigzag) = (true, false, false);
                s.underlay.stitch_length = s.underlay.stitch_length.min(1.5);
            }
            s.pull_comp *= 0.5;
            s.short_stitches = true;
        }
        StitchType::Fill(f) => {
            f.stitch_length = f.stitch_length.min(2.5);
            f.pull_comp *= 0.5;
            (f.underlay.edge_run, f.underlay.tatami) = (true, false);
            f.underlay.stitch_length = f.underlay.stitch_length.min(1.5);
        }
        StitchType::Run(r) => r.stitch_length = r.stitch_length.min(1.5),
        _ => {}
    }
}

/// A glyph as one object stitched `stitch`, the shape of what its art sews (the union of its
/// fills and satin columns), in its first thread; `fill` paints the shape (else no fill).
fn glyph_as_one(d: &mut Document, grp: &mut Node, stitch: StitchType, fill: Option<sf_core::Rgb>, name: &str) {
    use vectorcraft_geom::path::{PathData, SubPath};
    const PT: f64 = vectorcraft_embroidery::PT_PER_MM;
    let polys = sf_core::geom::poly::union(&vectorcraft_embroidery::covered_mm(grp));
    if polys.is_empty() {
        return;
    }
    // the glyph's first thread
    fn first_thread(n: &Node) -> Option<(sf_core::Thread, Color)> {
        if let Some(e) = n.embroidery.as_deref()
            && !matches!(n.kind, NodeKind::Group { .. })
        {
            let t = vectorcraft_embroidery::node_thread(n, e);
            return Some((t.clone(), Color::rgb8(t.color.r, t.color.g, t.color.b)));
        }
        n.children().into_iter().flatten().find_map(|c| first_thread(c))
    }
    let (thread, color) = first_thread(grp).unwrap_or((sf_core::Thread::new(sf_core::Rgb::new(0x1a, 0x1a, 0x1a), ""), Color::rgb8(0x1a, 0x1a, 0x1a)));
    let to_pt = |q: &sf_core::Point| kurbo::Point::new(q.x * PT, q.y * PT);
    let subpaths: Vec<SubPath> = polys
        .iter()
        .flat_map(|p| std::iter::once(&p.outer).chain(&p.holes))
        .filter(|ring| ring.len() >= 3)
        .map(|ring| SubPath::polyline(&ring.iter().map(to_pt).collect::<Vec<_>>(), true))
        .collect();
    let face = fill.map_or(color, |c| Color::rgb8(c.r, c.g, c.b));
    let paint = Appearance::basic(Paint::solid(face), Paint::solid(color), 1.0);
    let mut n = Node::path(d.alloc_id(), PathData::new(subpaths), paint);
    let mut e = Embroidery::new(stitch);
    e.thread = Some(thread);
    n.embroidery = Some(Box::new(e));
    n.name = Some(name.into());
    if let NodeKind::Group { children, .. } = &mut grp.kind {
        *children = vec![Arc::new(n)];
    }
}

fn puff_all(n: &mut Node, puff: Option<Puff>, counts: &mut (usize, usize)) {
    if let NodeKind::Group { children, .. } = &mut n.kind {
        for c in children.iter_mut() {
            puff_all(Arc::make_mut(c), puff, counts);
        }
        return;
    }
    let Some(e) = n.embroidery.as_deref_mut() else { return };
    // satin made puff or plain again; a fill made puff becomes satin across it, and fills again
    let next = match (&mut e.stitch, puff) {
        (StitchType::Satin(params), Some(p)) => {
            Puff { fill: params.puff.is_some_and(|q| q.fill), ..p }.apply(params);
            None
        }
        (StitchType::Satin(params), None) if params.puff.is_some_and(|q| q.fill) => Some(StitchType::Fill(FillParams::default())),
        (StitchType::Satin(params), None) => {
            Puff::remove(params);
            None
        }
        (StitchType::Fill(_), Some(p)) => {
            let mut params = SatinParams::default();
            Puff { fill: true, ..p }.apply(&mut params);
            Some(StitchType::Satin(params))
        }
        _ => {
            counts.1 += 1;
            return;
        }
    };
    counts.0 += 1;
    if let Some(st) = next {
        e.stitch = st;
    }
}

/// Whether all the stitched art in `n` is puff (and there is some).
fn is_puff(n: &Node) -> bool {
    fn walk(n: &Node, all: &mut bool, any: &mut bool) {
        if let NodeKind::Group { children, .. } = &n.kind {
            children.iter().for_each(|c| walk(c, all, any));
            return;
        }
        match n.embroidery.as_deref().map(|e| &e.stitch) {
            Some(StitchType::Satin(p)) if p.puff.is_some() => *any = true,
            Some(_) => *all = false,
            None => {}
        }
    }
    let (mut all, mut any) = (true, false);
    walk(n, &mut all, &mut any);
    all && any
}

/// In `children` and the plain layers and groups among them, the puff objects moved after the
/// others, in the order they were in → how many.
fn puff_last_in(children: &mut Vec<Arc<Node>>) -> usize {
    let mut moved = 0;
    for c in children.iter_mut() {
        if c.embroidery.is_none()
            && !is_puff(c)
            && c.children().is_some_and(|k| k.iter().any(|k| is_puff(k) || k.children().is_some()))
            && let Some(kids) = Arc::make_mut(c).children_mut()
        {
            moved += puff_last_in(kids);
        }
    }
    if children.iter().any(|c| is_puff(c)) {
        let (puff, rest): (Vec<_>, Vec<_>) = std::mem::take(children).into_iter().partition(|c| is_puff(c));
        moved += puff.len();
        *children = rest;
        children.extend(puff);
    }
    moved
}

fn puff_last(s: &mut Session, _: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.puff.last";
    if !vectorcraft_embroidery::stitched_nodes(&s.doc()?.doc).iter().any(|n| is_puff(n)) {
        return Err(bad(CMD, "nothing is 3D puff (Embroidery › 3D Puff)"));
    }
    let (moved, stop) = s.edit("Sew Puff Last", |d, _| {
        let mut moved = 0;
        for l in d.layers.iter_mut() {
            if let Some(kids) = Arc::make_mut(l).children_mut() {
                moved += puff_last_in(kids);
            }
        }
        // a stop after the last piece sewn before the first puff, to lay the foam
        let order = vectorcraft_embroidery::stitched_nodes(d);
        let first = order.iter().position(|n| is_puff(n)).unwrap_or(0);
        let before = order
            .get(..first)
            .unwrap_or_default()
            .iter()
            .rev()
            .find(|n| !matches!(n.kind, NodeKind::Group { .. } | NodeKind::Layer { .. }))
            .map(|n| n.id);
        if let Some(id) = before
            && let Some(e) = d.node_mut(id).and_then(|n| n.embroidery.as_deref_mut())
        {
            e.stop_after = true;
        }
        Ok((moved, before.is_some()))
    })?;
    Ok(json!({ "puff": moved, "stop": stop }))
}

/// Whether all the satin in `n` is puff already (and there is some).
fn all_puff(n: &Node) -> (usize, usize) {
    if let NodeKind::Group { children, .. } = &n.kind {
        return children.iter().map(|c| all_puff(c)).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
    }
    match n.embroidery.as_deref().map(|e| &e.stitch) {
        Some(StitchType::Satin(params)) => (1, usize::from(params.puff.is_some())),
        _ => (0, 0),
    }
}

/// `applique`, when given: the fabric's colour (appliqué letters), or false or null for none
/// (`Some(None)`).
fn applique_param(p: &Value, cmd: &str) -> Result<Option<Option<sf_core::Rgb>>> {
    match p.get("applique") {
        None => Ok(None),
        Some(Value::Null) | Some(Value::Bool(false)) => Ok(Some(None)),
        Some(Value::Bool(true)) => Ok(Some(Some(sf_core::Rgb::new(0xd9, 0xd9, 0xd9)))),
        Some(v) => v
            .as_str()
            .and_then(sf_core::Rgb::from_hex)
            .map(|c| Some(Some(c)))
            .ok_or_else(|| bad(cmd, "`applique` is the fabric's colour (\"#rrggbb\"), true or false")),
    }
}

/// `stitch` and `small`, when given, into `spec`.
fn stitch_params(p: &Value, cmd: &str, spec: &mut Lettering) -> Result<()> {
    if let Some(v) = p.get("stitch").filter(|v| !v.is_null()) {
        spec.stitch = v.as_str().and_then(LetterStitch::from_name).ok_or_else(|| bad(cmd, "`stitch` is font, satin or fill"))?;
    }
    if let Some(v) = p.get("small").filter(|v| !v.is_null()) {
        spec.small = v.as_str().and_then(SmallText::from_name).ok_or_else(|| bad(cmd, "`small` is auto, on or off"))?;
    }
    Ok(())
}

/// `puff`, when given: whether the lettering is 3D puff.
fn puff_param(p: &Value, cmd: &str) -> Result<Option<bool>> {
    match p.get("puff") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| bad(cmd, "`puff` is true or false")),
    }
}

fn puff(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.puff";
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    let cap = ranged(p, "cap", 0.0..=3.0, CMD, "`cap` is 0 to 3 mm")?;
    // on unless said, or unless all the satin chosen is puff already (the menu item turns it off)
    let on = match p.get("on") {
        None | Some(Value::Null) => {
            let d = &s.doc()?.doc;
            let (satin, puffed) = ids.iter().filter_map(|id| d.node(*id)).map(all_puff).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            let letterings = ids.iter().filter_map(|id| lettering_of(d, &[*id])).filter(|(l, _)| ids.contains(l)).collect::<Vec<_>>();
            !(satin > 0 && satin == puffed && letterings.iter().all(|(_, l)| l.puff.is_some()))
        }
        Some(v) => v.as_bool().ok_or_else(|| bad(CMD, "`on` is true or false"))?,
    };
    let setting = on.then(|| Puff { cap: cap.unwrap_or(Puff::default().cap), ..Puff::default() });
    // lettering is set again from its font, puff or not
    let _ = ensure_fonts(s);
    let mut letterings = Vec::new();
    for id in &ids {
        if let Some((l, mut spec)) = lettering_of(&s.doc()?.doc, &[*id]).filter(|(l, _)| l == id) {
            spec.puff = setting;
            let font = loaded(s, &spec.font).ok_or_else(|| not_loaded(CMD, &spec.font))?;
            letterings.push((l, spec, font));
        }
    }
    let counts = s.edit("3D Puff", |d, _| {
        let mut counts = (0, 0);
        for id in &ids {
            if let Some((_, spec, font)) = letterings.iter().find(|(l, _, _)| l == id) {
                let glyphs = glyphs(d, font, spec);
                let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
                counts.0 += glyphs.iter().map(|g| all_puff(g).0).sum::<usize>();
                if let NodeKind::Group { children, .. } = &mut n.kind {
                    *children = glyphs;
                }
                if let Some(e) = n.embroidery.as_deref_mut() {
                    e.stitch = StitchType::Lettering(Box::new(spec.clone()));
                }
            } else {
                puff_all(d.node_mut(*id).ok_or(EngineError::NoNode(*id))?, setting, &mut counts);
            }
        }
        Ok(counts)
    })?;
    Ok(json!({ "satin": counts.0, "other": counts.1, "on": on }))
}

/// A name-valued param `key`, when given.
fn name_param<'a>(p: &'a Value, key: &str, cmd: &str) -> Result<Option<&'a str>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_str().map(Some).ok_or_else(|| bad(cmd, format!("`{key}` is a name, not {v}"))),
    }
}

/// The sewing settings `p` gives, over `sew`.
fn sew_params(p: &Value, cmd: &str, sew: &mut LetterSewing) -> Result<()> {
    if let Some(v) = name_param(p, "order", cmd)? {
        sew.order = LetterOrder::from_name(v).ok_or_else(|| bad(cmd, "`order` is left, right or centre"))?;
    }
    if let Some(v) = name_param(p, "lines", cmd)? {
        sew.up = match v.to_ascii_lowercase().as_str() {
            "down" => false,
            "up" => true,
            _ => return Err(bad(cmd, "`lines` is down or up")),
        };
    }
    if let Some(v) = name_param(p, "join", cmd)? {
        sew.join = LetterJoin::from_name(v).ok_or_else(|| bad(cmd, "`join` is closest or baseline"))?;
    }
    if let Some(v) = name_param(p, "trim", cmd)? {
        sew.trim = LetterTrim::from_name(v).ok_or_else(|| bad(cmd, "`trim` is auto, letter, word or line"))?;
    }
    Ok(())
}

/// Where a glyph's baseline middle goes, `along` a line `down` from the first (points):
/// straight, or round a circle of radius `r` (points; positive, the lines arched over its top,
/// negative, along its bottom) whose edge the first line's baseline meets at the origin, the
/// glyph upright to it. A line the circle's centre doesn't reach is straight. With `turn`
/// (points), the radius shrinks that much a turn along the line (grows, negative), `along`
/// measured along the spiral; it stops winding a point from the centre.
fn place(r: f64, turn: f64, along: f64, down: f64) -> Affine {
    let rl = r - down;
    if r != 0.0 && rl * r > 0.0 && rl.abs() > 1e-3 && along.is_finite() && turn.is_finite() {
        let (a, side) = (rl.abs(), r.signum());
        // the radius shrinks `c` a radian turned
        let c = turn / std::f64::consts::TAU;
        // the angle turned to go `along` the curve, stepped half a point at a time
        let mut t = along / a;
        if c != 0.0 {
            let n = (along.abs() / 0.5).ceil().clamp(1.0, 20_000.0);
            let ds = along / n;
            t = 0.0;
            for _ in 0..n as usize {
                let rho = a - c * t;
                if rho <= 1.0 {
                    break;
                }
                t += ds / (rho * rho + c * c).sqrt();
            }
        }
        let rho = (a - c * t).max(1.0);
        // the baseline's direction: the spiral's tangent (round a circle, the angle turned)
        let dir = (side * (c * t.cos() + rho * t.sin())).atan2(rho * t.cos() - c * t.sin());
        if t.is_finite() && dir.is_finite() {
            return Affine::translate((rho * t.sin(), r - side * rho * t.cos())) * Affine::rotate(dir);
        }
    }
    Affine::translate((along, down))
}

/// Where a glyph's baseline middle goes `along` a baseline drawn as `line` (points, the
/// lettering's own space) and `down` from it (to the right of the way it runs), turned to follow
/// it; before its start and past its end, straight on.
fn place_on(line: &[vectorcraft_geom::Point], along: f64, down: f64) -> Affine {
    let segs: Vec<(vectorcraft_geom::Point, vectorcraft_geom::Point, f64)> =
        line.windows(2).filter_map(|w| Some((*w.first()?, *w.get(1)?, w.first()?.distance(*w.get(1)?)))).filter(|s| s.2 > 1e-9).collect();
    let Some(&first) = segs.first() else { return Affine::translate((along, down)) };
    let (mut seg, mut t) = (first, along);
    if along > 0.0 {
        let mut rest = along;
        for (n, s) in segs.iter().enumerate() {
            (seg, t) = (*s, rest);
            if rest <= s.2 || n + 1 == segs.len() {
                break;
            }
            rest -= s.2;
        }
    }
    let (a, b, len) = seg;
    let dir = (b - a) / len;
    let normal = vectorcraft_geom::Vec2::new(-dir.y, dir.x);
    let q = a + dir * t + normal * down;
    if !(q.x.is_finite() && q.y.is_finite()) {
        return Affine::translate((along, down));
    }
    Affine::translate(q.to_vec2()) * Affine::rotate(dir.y.atan2(dir.x))
}

/// The first line of a path node, flattened (points, the document's space), to set lettering
/// along: at most 5000 points.
fn baseline_of(d: &Document, id: NodeId) -> Option<Vec<vectorcraft_geom::Point>> {
    let NodeKind::Path { path, .. } = &d.node(id)?.kind else { return None };
    let first = path.subpaths.first()?;
    let bez = vectorcraft_geom::path::PathData::new(vec![first.clone()]).to_bezpath();
    let mut pts = Vec::new();
    kurbo::flatten(bez.iter(), 0.2, |el| match el {
        kurbo::PathEl::MoveTo(q) | kurbo::PathEl::LineTo(q) if pts.len() < 5000 => pts.push(q),
        _ => {}
    });
    if first.closed
        && let Some(&q) = pts.first()
    {
        pts.push(q);
    }
    (pts.len() >= 2).then_some(pts)
}

/// `path`, when given: a path node's id (or true: the selected path) to set the text along, as
/// a document-space line; `Some(None)` to set it straight again.
fn path_param(s: &Session, p: &Value, cmd: &str) -> Result<Option<Option<Vec<vectorcraft_geom::Point>>>> {
    let d = &s.doc()?.doc;
    let id = match p.get("path") {
        None => return Ok(None),
        Some(Value::Null) | Some(Value::Bool(false)) => return Ok(Some(None)),
        Some(Value::Bool(true)) => s
            .doc()?
            .selection
            .objects
            .iter()
            .copied()
            .find(|id| baseline_of(d, *id).is_some())
            .ok_or_else(|| bad(cmd, "select a line or shape to set the text along"))?,
        Some(v) => NodeId(v.as_u64().ok_or_else(|| bad(cmd, "`path` is a path's id, true (the selected one) or null"))?),
    };
    let line = baseline_of(d, id).ok_or_else(|| bad(cmd, "`path` is a line or shape to set the text along"))?;
    Ok(Some(Some(line)))
}

/// `arc`, when given: 0 (straight) or a radius of 1 to 10000 mm either way.
fn arc_param(p: &Value, cmd: &str) -> Result<Option<f64>> {
    match p.get("arc") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|a| *a == 0.0 || (1.0..=10000.0).contains(&a.abs()))
            .map(Some)
            .ok_or_else(|| bad(cmd, "`arc` is 0 (straight) or a radius of 1 to 10000 mm (negative: along a circle's bottom)")),
    }
}

/// `spiral`, when given: −1000 to 1000 mm a turn.
fn spiral_param(p: &Value, cmd: &str) -> Result<Option<f64>> {
    ranged(p, "spiral", -1000.0..=1000.0, cmd, "`spiral` is -1000 to 1000 mm a turn")
}

/// A spiral winds round an arc: refused without one.
fn spiral_needs_arc(spec: &Lettering, cmd: &str) -> Result<()> {
    if spec.spiral != 0.0 && spec.arc == 0.0 {
        return Err(bad(cmd, "a `spiral` winds round an `arc`: give the radius it starts at"));
    }
    Ok(())
}

/// `align`, when given.
fn align_param(p: &Value, cmd: &str) -> Result<Option<LetterAlign>> {
    match p.get("align") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_str().and_then(LetterAlign::from_name).map(Some).ok_or_else(|| bad(cmd, "`align` is left, centre or right")),
    }
}

fn lettering(s: &mut Session, p: &Value) -> Result<Value> {
    let text = text_param(p, SET)?.ok_or_else(|| bad(SET, "missing `text`"))?;
    let name = str_param(p, "font").ok_or_else(|| bad(SET, "missing `font`"))?;
    let mut warnings = ensure_fonts(s);
    let font = loaded(s, name).ok_or_else(|| not_loaded(SET, name))?;
    let size = ranged(p, "size", 1.0..=500.0, SET, "`size` is 1 to 500 mm")?.unwrap_or_else(|| own_size(&font));
    let leading = ranged(p, "leading", 0.2..=5.0, SET, "`leading` is 0.2 to 5")?.unwrap_or(1.0);
    let at = match p.get("at") {
        None | Some(Value::Null) => s.doc()?.doc.artboards.first().map_or(Point::new(0.0, 0.0), |a| a.rect.center()),
        Some(_) => point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(SET, "`at` is [x, y]"))?,
    };
    let mut sew = LetterSewing::default();
    sew_params(p, SET, &mut sew)?;
    let puff = puff_param(p, SET)?.unwrap_or(false).then(Puff::default);
    let applique = applique_param(p, SET)?.flatten();
    let mut spec = Lettering {
        text: text.to_string(),
        font: font.info.name.clone(),
        size,
        leading,
        arc: arc_param(p, SET)?.unwrap_or(0.0),
        spiral: spiral_param(p, SET)?.unwrap_or(0.0),
        align: align_param(p, SET)?.unwrap_or_default(),
        sew,
        letters: Vec::new(),
        puff,
        applique,
        stitch: LetterStitch::Font,
        small: SmallText::Auto,
        vertical: false,
        path: Vec::new(),
        frame: sf_core::Affine::translate(sf_core::Point::new(at.x, at.y)),
    };
    spiral_needs_arc(&spec, SET)?;
    stitch_params(p, SET, &mut spec)?;
    spec.vertical = bool_or(p, "vertical", false);
    // along a drawn line: from its start, in the lettering's own space
    if let Some(Some(line)) = path_param(s, p, SET)?
        && let Some(&start) = line.first()
    {
        spec.frame = sf_core::Affine::translate(sf_core::Point::new(start.x, start.y));
        spec.path = line.iter().map(|q| sf_core::Point::new(q.x - start.x, q.y - start.y)).collect();
        spec.arc = 0.0;
    }
    // scaled from the size the font was digitized at; outside what it sews well at, a warning
    warnings.extend(size_warnings(&font, size));
    let parent = s.doc()?.insertion_parent();
    let (id, count) = s.edit("Embroidery Lettering", |d, sel| {
        let glyphs = glyphs(d, &font, &spec);
        if glyphs.is_empty() {
            return Err(bad(SET, format!("{} has none of these characters", font.info.name)));
        }
        let count = glyphs.len();
        let mut all = Node::group(d.alloc_id(), glyphs);
        all.name = Some(lettering_name(&spec));
        all.embroidery = Some(Box::new(Embroidery::new(StitchType::Lettering(Box::new(spec.clone())))));
        let id = all.id;
        d.insert(parent, usize::MAX, all)?;
        sel.set([id]);
        Ok((id, count))
    })?;
    Ok(json!({ "id": id.0, "glyphs": count, "warnings": warnings }))
}

fn edit_lettering(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = targets(s, p)?;
    let (id, mut spec) = lettering_of(&s.doc()?.doc, &ids).ok_or_else(|| bad(EDIT, "select lettering set in an embroidery font"))?;
    if let Some(t) = text_param(p, EDIT)? {
        spec.text = t.to_string();
    }
    if let Some(f) = str_param(p, "font") {
        spec.font = f.to_string();
    }
    let mut warnings = ensure_fonts(s);
    let font = loaded(s, &spec.font).ok_or_else(|| not_loaded(EDIT, &spec.font))?;
    spec.font = font.info.name.clone();
    if let Some(v) = ranged(p, "size", 1.0..=500.0, EDIT, "`size` is 1 to 500 mm")? {
        spec.size = v;
    }
    if let Some(v) = ranged(p, "leading", 0.2..=5.0, EDIT, "`leading` is 0.2 to 5")? {
        spec.leading = v;
    }
    if let Some(v) = arc_param(p, EDIT)? {
        spec.arc = v;
    }
    if let Some(v) = spiral_param(p, EDIT)? {
        spec.spiral = v;
    }
    // straightened: no spiral either
    if spec.arc == 0.0 && p.get("spiral").is_none_or(Value::is_null) {
        spec.spiral = 0.0;
    }
    spiral_needs_arc(&spec, EDIT)?;
    if let Some(v) = align_param(p, EDIT)? {
        spec.align = v;
    }
    sew_params(p, EDIT, &mut spec.sew)?;
    if let Some(on) = puff_param(p, EDIT)? {
        spec.puff = if on { Some(spec.puff.unwrap_or_default()) } else { None };
    }
    if let Some(a) = applique_param(p, EDIT)? {
        spec.applique = a;
    }
    stitch_params(p, EDIT, &mut spec)?;
    if let Some(v) = p.get("vertical").and_then(Value::as_bool) {
        spec.vertical = v;
    }
    match path_param(s, p, EDIT)? {
        Some(Some(line)) => {
            // the line in the lettering's own space
            let inv = Affine::new(spec.frame.0).inverse();
            spec.path = line.iter().map(|q| inv * *q).map(|q| sf_core::Point::new(q.x, q.y)).collect();
            spec.arc = 0.0;
        }
        Some(None) => spec.path.clear(),
        None => {}
    }
    // round an arc again: off the line
    if spec.arc != 0.0 && p.get("arc").is_some() {
        spec.path.clear();
    }
    warnings.extend(size_warnings(&font, spec.size));
    let count = set_again(s, id, &spec, &font, "Edit Lettering", EDIT)?;
    Ok(json!({ "id": id.0, "glyphs": count, "warnings": warnings }))
}

/// Lettering `id` set again as `spec` says, selected → how many glyphs it has.
fn set_again(s: &mut Session, id: NodeId, spec: &Lettering, font: &EmbFont, label: &'static str, cmd: &str) -> Result<usize> {
    s.edit(label, |d, sel| {
        let glyphs = glyphs(d, font, spec);
        if glyphs.is_empty() {
            return Err(bad(cmd, format!("{} has none of these characters", font.info.name)));
        }
        let count = glyphs.len();
        let n = d.node_mut(id).ok_or(EngineError::NoNode(id))?;
        let NodeKind::Group { children, .. } = &mut n.kind else { return Err(EngineError::NoNode(id)) };
        *children = glyphs;
        n.name = Some(lettering_name(spec));
        if let Some(e) = n.embroidery.as_deref_mut() {
            e.stitch = StitchType::Lettering(Box::new(spec.clone()));
        }
        sel.set([id]);
        Ok(count)
    })
}

fn adjust_letter(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = targets(s, p)?;
    let (id, mut spec, pos) = {
        let d = &s.doc()?.doc;
        let (id, spec) = lettering_of(d, &ids).ok_or_else(|| bad(LETTER, "select lettering set in an embroidery font, or one of its letters"))?;
        // a letter selected inside it: its glyph's place among the lettering's (sewing order)
        let inside = ids.iter().find_map(|i| {
            let mut at = *i;
            loop {
                let up = d.parent_of(at)?;
                if up == id {
                    return Some(at);
                }
                at = up;
            }
        });
        let pos = inside.and_then(|c| d.node(id)?.children()?.iter().position(|n| n.id == c));
        (id, spec, pos)
    };
    // remembered font folders loaded if they weren't; a folder that fails shows when the font is missing
    let _ = ensure_fonts(s);
    let font = loaded(s, &spec.font).ok_or_else(|| not_loaded(LETTER, &spec.font))?;
    let count = font.layout(&spec.text).len();
    let index = match p.get("index") {
        None | Some(Value::Null) => {
            let pos = pos.ok_or_else(|| bad(LETTER, "give `index`, or select one of the lettering's letters"))?;
            let order = glyphs_indexed(&mut Document::new(10.0, 10.0), &font, &spec);
            order.get(pos).map(|g| g.0).ok_or_else(|| bad(LETTER, "the letter selected isn't one the lettering sets"))?
        }
        Some(v) => v
            .as_u64()
            .and_then(|i| usize::try_from(i).ok())
            .filter(|i| *i < count)
            .ok_or_else(|| bad(LETTER, format!("`index` is a letter's place, 0 to {}", count.saturating_sub(1))))?,
    };
    let mut a = spec.letters.iter().find(|a| a.index == index).copied().unwrap_or_else(|| LetterAdjust::none(index));
    if p.get("reset").and_then(Value::as_bool) == Some(true) {
        a = LetterAdjust::none(index);
    }
    if let Some(v) = ranged(p, "dx", -1000.0..=1000.0, LETTER, "`dx` is -1000 to 1000 mm")? {
        a.dx = v;
    }
    if let Some(v) = ranged(p, "dy", -1000.0..=1000.0, LETTER, "`dy` is -1000 to 1000 mm")? {
        a.dy = v;
    }
    if let Some(v) = ranged(p, "scale", 0.1..=10.0, LETTER, "`scale` is 0.1 to 10")? {
        a.scale = v;
    }
    if let Some(v) = ranged(p, "angle", -360.0..=360.0, LETTER, "`angle` is -360 to 360 degrees")? {
        a.angle = v;
    }
    spec.letters.retain(|b| b.index != index);
    if !a.is_plain() {
        spec.letters.push(a);
        spec.letters.sort_by_key(|b| b.index);
    }
    let glyphs = set_again(s, id, &spec, &font, "Adjust Letter", LETTER)?;
    Ok(json!({ "id": id.0, "index": index, "letter": { "dx": a.dx, "dy": a.dy, "scale": a.scale, "angle": a.angle }, "glyphs": glyphs }))
}

/// Most names one team names export writes.
const MAX_NAMES: usize = 500;

/// The rows of a CSV table (RFC 4180: quoted fields, `""` for a quote, line breaks inside
/// quotes), separated by commas, or semicolons when the first line has more of those (as
/// spreadsheets in many languages write them). At most [`MAX_NAMES`] + 1 rows.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.lines().next().unwrap_or("");
    let sep = if first.matches(';').count() > first.matches(',').count() { ';' } else { ',' };
    let (mut rows, mut row, mut field, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c != '"' {
                field.push(c);
            } else if chars.peek() == Some(&'"') {
                field.push('"');
                chars.next();
            } else {
                quoted = false;
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == sep {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            if rows.len() > MAX_NAMES {
                return rows;
            }
        } else {
            field.push(c);
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn team_names_read(_: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.teamNames.read";
    let text = match (str_param(p, "csv"), str_param(p, "path")) {
        (Some(t), _) => t.to_string(),
        (None, Some(path)) => {
            let bytes = crate::cmd::fileio::read_file(path)?;
            if bytes.len() > MAX_JSON {
                return Err(bad(CMD, format!("the table is too big ({} MB at most)", MAX_JSON >> 20)));
            }
            String::from_utf8(bytes).map_err(|_| bad(CMD, "the table is not text (UTF-8)"))?
        }
        _ => return Err(bad(CMD, "give the table's text (`csv`) or its file (`path`)")),
    };
    let column = match p.get("column") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_u64().filter(|c| *c < 100).ok_or_else(|| bad(CMD, "`column` is 0 to 99"))? as usize,
    };
    let skip = usize::from(bool_or(p, "header", false));
    let names: Vec<String> = csv_rows(&text)
        .into_iter()
        .skip(skip)
        .filter_map(|row| row.into_iter().nth(column))
        .map(|n| n.trim().chars().take(MAX_TEXT).collect::<String>())
        .filter(|n| !n.is_empty())
        .take(MAX_NAMES)
        .collect();
    Ok(json!({ "names": names }))
}

/// A name as a file name: no characters files can't have, not too long, not empty.
pub(super) fn safe_name(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).take(60).collect();
    let s = s.trim().trim_matches('.').trim();
    if s.is_empty() { "name".to_string() } else { s.to_string() }
}

/// `base`, or `base 2`, `base 3`… when one is taken already (in any case), now taken.
fn unique(taken: &mut HashSet<String>, base: &str) -> String {
    let mut name = base.to_string();
    let mut k = 2;
    while !taken.insert(name.to_lowercase()) {
        name = format!("{base} {k}");
        k += 1;
    }
    name
}

/// The `names` asked for: 1 to [`MAX_NAMES`], each [`MAX_TEXT`] characters at most.
fn names_param(p: &Value, cmd: &str) -> Result<Vec<String>> {
    let names: Vec<String> = p
        .get("names")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).collect())
        .unwrap_or_default();
    if names.is_empty() || names.len() > MAX_NAMES {
        return Err(bad(cmd, format!("`names` are 1 to {MAX_NAMES} names")));
    }
    if let Some(n) = names.iter().find(|n| n.chars().count() > MAX_TEXT) {
        let start: String = n.chars().take(20).collect();
        return Err(bad(cmd, format!("a name is {MAX_TEXT} characters at most: `{start}…`")));
    }
    Ok(names)
}

/// The team names' lettering (selected, or `id`) and its font.
fn team_lettering(s: &mut Session, p: &Value, cmd: &str) -> Result<(NodeId, Lettering, std::sync::Arc<EmbFont>)> {
    let ids = targets(s, p)?;
    let (id, spec) = lettering_of(&s.doc()?.doc, &ids).ok_or_else(|| bad(cmd, "select lettering set in an embroidery font"))?;
    let _ = ensure_fonts(s);
    let font = loaded(s, &spec.font).ok_or_else(|| not_loaded(cmd, &spec.font))?;
    Ok((id, spec, font))
}

/// The bounds (points) of glyphs set for `name` with `spec`, in a scratch copy of `d`.
fn name_bounds(d: &mut Document, font: &EmbFont, spec: &Lettering, name: &str) -> Option<vectorcraft_geom::Rect> {
    let one = Lettering { text: name.to_string(), ..spec.clone() };
    glyphs(d, font, &one).iter().filter_map(|g| g.geometric_bounds()).reduce(|a, b| a.union(b))
}

fn team_names_layout(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.teamNames.layout";
    let names = names_param(p, CMD)?;
    let gap = ranged(p, "gap", 0.0..=200.0, CMD, "`gap` is 0 to 200 mm")?.unwrap_or(10.0) * vectorcraft_embroidery::PT_PER_MM;
    let columns = match p.get("columns") {
        None | Some(Value::Null) => ((names.len() as f64).sqrt().ceil() as usize).max(1),
        Some(v) => v.as_u64().filter(|c| (1..=50).contains(c)).map(|c| c as usize).ok_or_else(|| bad(CMD, "`columns` is 1 to 50"))?,
    };
    let (id, spec, font) = team_lettering(s, p, CMD)?;
    // the largest name sets the cell; each name's lettering starts where the original is
    let mut scratch = (*s.doc()?.doc).clone();
    let mut cell = (0.0f64, 0.0f64);
    for name in &names {
        let b = name_bounds(&mut scratch, &font, &spec, name)
            .ok_or_else(|| bad(CMD, format!("{} has none of the characters of `{name}`", font.info.name)))?;
        cell = (cell.0.max(b.width()), cell.1.max(b.height()));
    }
    let rows = names.len().div_ceil(columns);
    let original = s.doc()?.doc.node(id).cloned().ok_or(EngineError::NoNode(id))?;
    let parent = s.doc()?.insertion_parent();
    let count = names.len();
    let group = s.edit("Lay Out Team Names", |d, sel| {
        let mut copies = Vec::with_capacity(names.len());
        for (k, name) in names.iter().enumerate() {
            let (col, row) = (k % columns, k / columns);
            let shift = sf_core::Affine::translate(sf_core::Point::new(col as f64 * (cell.0 + gap), row as f64 * (cell.1 + gap)));
            let one = Lettering { text: name.clone(), frame: spec.frame.then(&shift), ..spec.clone() };
            let mut n = d.reid(&original);
            n.name = Some(lettering_name(&one));
            if let NodeKind::Group { children, .. } = &mut n.kind {
                *children = glyphs(d, &font, &one);
            }
            if let Some(e) = n.embroidery.as_deref_mut() {
                e.stitch = StitchType::Lettering(Box::new(one));
            }
            copies.push(Arc::new(n));
        }
        let mut all = Node::group(d.alloc_id(), copies);
        all.name = Some("Team names".into());
        let gid = all.id;
        d.insert(parent, usize::MAX, all)?;
        sel.set([gid]);
        Ok(gid)
    })?;
    Ok(json!({ "id": group.0, "count": count, "columns": columns.min(count), "rows": rows }))
}

fn team_names_worksheet(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.teamNames.worksheet";
    let names = names_param(p, CMD)?;
    let (pw, ph) = match str_param(p, "page").unwrap_or("a4") {
        "a4" | "A4" => super::embroidery::SHEET,
        "letter" | "Letter" => (612.0, 792.0),
        _ => return Err(bad(CMD, "`page` is a4 or letter")),
    };
    let (id, spec, font) = team_lettering(s, p, CMD)?;
    // each name as it sews: the design with the lettering set to it
    let mut doc = (*s.doc()?.doc).clone();
    let planner = super::embroidery::EmbroideryState::default();
    let mut rows = Vec::with_capacity(names.len());
    for name in &names {
        let one = Lettering { text: name.clone(), ..spec.clone() };
        let glyphs = glyphs(&mut doc, &font, &one);
        if glyphs.is_empty() {
            return Err(bad(CMD, format!("{} has none of the characters of `{name}`", font.info.name)));
        }
        let n = doc.node_mut(id).ok_or(EngineError::NoNode(id))?;
        let NodeKind::Group { children, .. } = &mut n.kind else { return Err(EngineError::NoNode(id)) };
        *children = glyphs;
        let plan = planner.plan(&doc);
        let Some(st) = plan.stats.clone() else { continue };
        rows.push((name.replace(['\n', '\r'], " / "), st));
    }
    let stitches: usize = rows.iter().map(|r| r.1.stitches).sum();
    let seconds: f64 = rows.iter().map(|r| r.1.seconds).sum();
    let title = super::fileio::file_stem(&s.doc()?.doc.title);
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let grey = Color::rgb8(0x66, 0x66, 0x66);
    let mut sheet = Document::new(pw, ph);
    sheet.title = format!("{title} team names");
    use super::embroidery::sheet_text;
    sheet_text(&mut sheet, 40.0, 56.0, "Team names", 20.0, black)?;
    let t = sf_formats::Timestamp::now();
    sheet_text(
        &mut sheet,
        40.0,
        78.0,
        &format!("{title}   {:04}-{:02}-{:02}   font: {}, {:.0} mm", t.year, t.month, t.day, spec.font, spec.size),
        10.0,
        grey,
    )?;
    let hms = |secs: f64| {
        let s = secs.max(0.0).round() as u64;
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    };
    sheet_text(&mut sheet, 40.0, 104.0, "Name", 9.0, grey)?;
    sheet_text(&mut sheet, 300.0, 104.0, "Size (mm)", 9.0, grey)?;
    sheet_text(&mut sheet, 400.0, 104.0, "Stitches", 9.0, grey)?;
    sheet_text(&mut sheet, 480.0, 104.0, "Time", 9.0, grey)?;
    let room = ((ph - 170.0) / 14.0).max(1.0) as usize;
    let mut y = 122.0;
    for (k, (name, st)) in rows.iter().take(room).enumerate() {
        let shown: String = name.chars().take(40).collect();
        sheet_text(&mut sheet, 40.0, y, &format!("{}. {shown}", k + 1), 9.0, black)?;
        sheet_text(&mut sheet, 300.0, y, &format!("{:.0} × {:.0}", st.width_mm, st.height_mm), 9.0, black)?;
        sheet_text(&mut sheet, 400.0, y, &st.stitches.to_string(), 9.0, black)?;
        sheet_text(&mut sheet, 480.0, y, &hms(st.seconds), 9.0, black)?;
        y += 14.0;
    }
    if rows.len() > room {
        sheet_text(&mut sheet, 40.0, y, &format!("… and {} more", rows.len() - room), 9.0, grey)?;
        y += 14.0;
    }
    sheet_text(&mut sheet, 40.0, y + 10.0, &format!("{} names   {stitches} stitches   {} sewing", rows.len(), hms(seconds)), 10.0, black)?;
    s.add_document(sheet, None);
    Ok(json!({ "count": rows.len(), "stitches": stitches, "seconds": seconds.round() }))
}

fn team_names_export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.teamNames.export";
    let names = names_param(p, CMD)?;
    let dir = str_param(p, "dir").filter(|d| !d.trim().is_empty()).ok_or_else(|| bad(CMD, "missing `dir`: the folder to write to"))?;
    let prefix = str_param(p, "prefix").unwrap_or("");
    let ids = targets(s, p)?;
    let (id, spec) = lettering_of(&s.doc()?.doc, &ids).ok_or_else(|| bad(CMD, "select lettering set in an embroidery font"))?;
    // its font: one of the remembered folders' (what fails to load says so when the fonts are listed)
    let _ = ensure_fonts(s);
    let font = loaded(s, &spec.font).ok_or_else(|| not_loaded(CMD, &spec.font))?;
    // the format, as Export Machine File picks it
    let machine = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.machine_file_format(&s.prefs.embroidery_machines));
    let key = str_param(p, "format").map(str::to_string).or(machine).unwrap_or_else(|| "dst".to_string());
    let format = sf_formats::Format::from_extension(&key)
        .ok_or_else(|| bad(CMD, format!("unknown format `{key}`: one of {}", super::embroidery::MACHINE_EXTS.join(", "))))?;
    crate::cmd::fileio::create_dir(dir)?;
    // a copy of the design, the lettering set to each name in turn
    let mut doc = (*s.doc()?.doc).clone();
    let planner = super::embroidery::EmbroideryState::default();
    let mut taken = HashSet::new();
    let mut files = Vec::new();
    for name in &names {
        let one = Lettering { text: name.clone(), ..spec.clone() };
        let glyphs = glyphs(&mut doc, &font, &one);
        if glyphs.is_empty() {
            return Err(bad(CMD, format!("{} has none of the characters of `{name}`", font.info.name)));
        }
        let n = doc.node_mut(id).ok_or(EngineError::NoNode(id))?;
        let NodeKind::Group { children, .. } = &mut n.kind else { return Err(EngineError::NoNode(id)) };
        *children = glyphs;
        let plan = planner.plan(&doc);
        let opts = sf_formats::WriteOptions { name: name.replace(['\n', '\r'], " "), center: true, ..sf_formats::WriteOptions::default() };
        let bytes =
            sf_formats::write(format, plan.pattern(), &opts).map_err(|e| EngineError::Other(format!("{} export failed: {e}", format.name())))?;
        let stem = unique(&mut taken, &format!("{prefix}{}", safe_name(name)));
        let path = std::path::Path::new(dir).join(format!("{stem}.{}", format.extension())).to_string_lossy().to_string();
        crate::cmd::fileio::write_file(&path, &bytes)?;
        files.push(path);
    }
    Ok(json!({ "count": files.len(), "files": files }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_tables_give_their_columns() {
        assert_eq!(csv_rows("a,b\n\"Smith, J\",7\r\n\"say \"\"hi\"\"\",8"), vec![vec!["a", "b"], vec!["Smith, J", "7"], vec!["say \"hi\"", "8"]]);
        assert_eq!(csv_rows("\u{feff}Name;No\nLee;9\n"), vec![vec!["Name", "No"], vec!["Lee", "9"]]);
        assert_eq!(csv_rows("\"two\nlines\",x"), vec![vec!["two\nlines", "x"]]);
        assert_eq!(csv_rows("one\ntwo"), vec![vec!["one"], vec!["two"]]);
        assert!(csv_rows("").is_empty());
        assert_eq!(csv_rows(&"x\n".repeat(10_000)).len(), MAX_NAMES + 1);
        assert_eq!(safe_name(" A/B: c? "), "A_B_ c_");
        assert_eq!(safe_name("..."), "name");
        let mut taken = HashSet::new();
        assert_eq!([unique(&mut taken, "Lee"), unique(&mut taken, "lee"), unique(&mut taken, "Lee")], ["Lee", "lee 2", "Lee 3"]);
    }

    #[test]
    fn glyphs_go_round_arcs_upright_to_them() {
        let at = |m: Affine| m * vectorcraft_geom::Point::new(0.0, 0.0);
        let r = 100.0;
        let centre = vectorcraft_geom::Point::new(0.0, r);
        // arched over a circle: the origin on its top, the glyphs round it, their tops outward
        assert!(at(place(r, 0.0, 0.0, 0.0)).distance(vectorcraft_geom::Point::new(0.0, 0.0)) < 1e-9);
        for along in [-80.0, 30.0, 150.0] {
            let m = place(r, 0.0, along, 0.0);
            let p = at(m);
            assert!((p.distance(centre) - r).abs() < 1e-9, "{p:?}");
            let up = m * vectorcraft_geom::Point::new(0.0, -1.0) - p;
            let out = (p - centre) / r;
            assert!((up - out).hypot() < 1e-9, "{up:?} {out:?}");
        }
        // mirrored either side of the top
        let (a, b) = (at(place(r, 0.0, -40.0, 0.0)), at(place(r, 0.0, 40.0, 0.0)));
        assert!((a.x + b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9 && a.y > 0.0);
        // a second line 20 down: a circle 20 smaller
        assert!((at(place(r, 0.0, 50.0, 20.0)).distance(centre) - 80.0).abs() < 1e-9);
        // along a circle's bottom: the ends rise, tops inward
        let c = vectorcraft_geom::Point::new(0.0, -r);
        let p = at(place(-r, 0.0, 40.0, 0.0));
        assert!((p.distance(c) - r).abs() < 1e-9 && p.y < 0.0, "{p:?}");
        // a line past the centre, and a straight baseline, stay straight
        assert_eq!(at(place(r, 0.0, 5.0, 150.0)), vectorcraft_geom::Point::new(5.0, 150.0));
        assert_eq!(at(place(0.0, 0.0, 5.0, 7.0)), vectorcraft_geom::Point::new(5.0, 7.0));
    }

    #[test]
    fn glyphs_follow_a_drawn_line() {
        use vectorcraft_geom::Point as P;
        let at = |m: Affine| m * P::new(0.0, 0.0);
        let line = [P::new(0.0, 0.0), P::new(100.0, 0.0), P::new(100.0, 100.0)];
        // along the first leg, straight
        assert!(at(place_on(&line, 30.0, 0.0)).distance(P::new(30.0, 0.0)) < 1e-9);
        // round the corner: 50 down the second leg, turned a quarter (its up pointing away, +x)
        let m = place_on(&line, 150.0, 0.0);
        assert!(at(m).distance(P::new(100.0, 50.0)) < 1e-9);
        let up = m * P::new(0.0, -1.0) - at(m);
        assert!((up.x - 1.0).abs() < 1e-9 && up.y.abs() < 1e-9, "{up:?}");
        // a line below: to the right of the way it runs
        assert!(at(place_on(&line, 30.0, 10.0)).distance(P::new(30.0, 10.0)) < 1e-9);
        // before the start and past the end: straight on
        assert!(at(place_on(&line, -20.0, 0.0)).distance(P::new(-20.0, 0.0)) < 1e-9);
        assert!(at(place_on(&line, 250.0, 0.0)).distance(P::new(100.0, 150.0)) < 1e-9);
        // a line of one point: straight
        assert!(at(place_on(&line[..1], 5.0, 7.0)).distance(P::new(5.0, 7.0)) < 1e-9);
    }

    #[test]
    fn glyphs_go_round_spirals_along_them() {
        let at = |m: Affine| m * vectorcraft_geom::Point::new(0.0, 0.0);
        let (r, turn) = (100.0, 30.0);
        let centre = vectorcraft_geom::Point::new(0.0, r);
        // the start on the circle's top, then winding in by `turn` a turn
        assert!(at(place(r, turn, 0.0, 0.0)).distance(vectorcraft_geom::Point::new(0.0, 0.0)) < 1e-9);
        let mut last = r;
        let mut prev = at(place(r, turn, 0.0, 0.0));
        let mut walked = 0.0;
        for k in 1..=200 {
            let along = f64::from(k) * 5.0;
            let m = place(r, turn, along, 0.0);
            let p = at(m);
            let d = p.distance(centre);
            assert!(d < last + 1e-9, "{k}: {d} after {last}");
            last = d;
            walked += p.distance(prev);
            prev = p;
            // upright to it: the glyph's up nearly away from the centre (a spiral's tangent leans in a little more near it)
            let up = m * vectorcraft_geom::Point::new(0.0, -1.0) - p;
            let out = (p - centre) / d;
            assert!((up.x * out.x + up.y * out.y) > 0.97, "{k}: {up:?} {out:?}");
        }
        // `along` is the length along the spiral (the chords a little shorter)
        assert!(walked < 1000.5 && walked > 999.0, "{walked}");
        // a full turn in: 30 closer (to a step's precision)
        let one = 2.0 * std::f64::consts::PI * (r - turn / 2.0);
        assert!((at(place(r, turn, one, 0.0)).distance(centre) - (r - turn)).abs() < 0.5);
        // negative: winding out
        assert!(at(place(r, -turn, 300.0, 0.0)).distance(centre) > r + 10.0);
        // it stops winding near the centre rather than through it
        let p = at(place(r, turn, 1e6, 0.0));
        assert!(p.x.is_finite() && p.distance(centre) >= 1.0 - 1e-9);
        // round a circle's bottom too
        let c = vectorcraft_geom::Point::new(0.0, -r);
        assert!(at(place(-r, turn, 200.0, 0.0)).distance(c) < r - 5.0);
    }
}
