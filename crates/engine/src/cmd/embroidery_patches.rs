//! Patches (PT-07, TM-04): embroidered patches with a satin edge, as many as fit in the hoop, one
//! for each name in a list when the words differ (a company's workers, a team). Every patch's
//! background is sewn first, then every patch's words, then every edge, so a hoopful changes
//! thread as few times as one patch does. A list longer than a hoopful goes into as many
//! hoopfuls as it needs; Export Patches writes a machine file for each. Fill Hoop With Copies
//! repeats a design of one's own across the hoop the same way. Original work: the shapes, sizes
//! and wording are our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, Result, Session, always, bad, cmd, has_doc, str_param, targets};

/// Room kept between the patches and the hoop's sewing field (mm).
const MARGIN: f64 = 5.0;
/// The most names (patches) one call makes.
const MAX_PATCHES: usize = 500;
/// The most letters a patch's words have.
const MAX_WORDS: usize = 60;
/// The most hoopfuls Export Patches writes.
const MAX_HOOPFULS: usize = 100;

const SHAPES: &[&str] = &["circle", "oval", "rectangle", "rounded", "shield"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.patches.make",
            "Make Patches",
            [],
            None,
            "{shape?: circle|oval|rectangle|rounded|shield (circle), width?: mm (75; 15–300), height?: mm (the width for a circle, else 0.6 of it; 15–300), border?: mm (the satin edge's width, 3.5; 0: none, else 1.5–8), borderColor?: \"#rrggbb\" (#1d3557), background?: \"#rrggbb\" | null (a tatami fill inside the edge; null: none, the fabric shows) (#f1faee), text?: words on every patch, names?: [text, …] (1–500: a patch each, its words the name; a line break sets two lines), textColor?: \"#rrggbb\" (#1d3557), textHeight?: mm (the capitals; made smaller to fit the patch), count?: n (copies of a patch with `text` or none, 1–500; default: as many as fit one hoop), hoop?: a hoop's name (default: My Machine's largest hoop when there are several patches, the smallest that fits one), gap?: mm between patches (5; 2–50), hoopful?: n (which hoopful to make, from 1: the patches after the first hoopfuls')} a new design: a hoopful of patches in rows, every background sewn first, then every patch's words, then every edge (three thread changes a hoopful); opens a new document → {hoop, perHoop, patches, hoopfuls, hoopful, made}",
            always,
            make
        ),
        cmd!(
            "embroidery.patches.export",
            "Export Patches",
            [],
            None,
            "{…embroidery.patches.make's, dir: the folder to write to (made if missing), format?: dst|exp|jef|pes|pec|vp3|xxx|u01 (My Machine's, else dst), prefix?: what the file names start with (patches)} every hoopful of patches as a machine file of its own (patches-1.vp3, patches-2.vp3…), and a list of which names are in which hoopful (patches-names.txt) when there are names. The open documents stay as they are → {files, hoopfuls, perHoop, patches, list?}",
            always,
            export
        ),
        cmd!(
            "embroidery.patches.fillHoop",
            "Fill Hoop With Copies",
            [],
            None,
            "{gap?: mm between copies (5; 2–50), count?: n (the most copies, the design included; default: as many as fit)} the design repeated in rows across its hoop's sewing field, as many copies as fit, then sorted by thread so each colour is sewn once for all the copies → {copies, columns, rows}",
            has_doc,
            fill_hoop
        ),
    ]
}

/// What makes one set of patches.
struct Spec {
    shape: &'static str,
    w: f64,
    h: f64,
    border: f64,
    border_color: String,
    background: Option<String>,
    text_color: String,
    text_height: Option<f64>,
    gap: f64,
    /// The words on each patch (`None`: none), one entry a patch.
    words: Vec<Option<String>>,
}

fn color(p: &Value, key: &str, default: &str, cmd: &str) -> Result<String> {
    let c = str_param(p, key).unwrap_or(default).trim().to_string();
    sf_core::Rgb::from_hex(&c).ok_or_else(|| bad(cmd, format!("`{key}` is \"#rrggbb\"")))?;
    Ok(c)
}

fn num(p: &Value, key: &str, range: std::ops::RangeInclusive<f64>, cmd: &str) -> Result<Option<f64>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite() && range.contains(v))
            .map(Some)
            .ok_or_else(|| bad(cmd, format!("`{key}` is {} to {}", range.start(), range.end()))),
    }
}

/// The words of a patch: trimmed, at most `MAX_WORDS` letters.
fn words(t: &str, cmd: &str) -> Result<Option<String>> {
    let t = t.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.chars().count() > MAX_WORDS {
        let start: String = t.chars().take(20).collect();
        return Err(bad(cmd, format!("a patch's words are {MAX_WORDS} letters at most: `{start}…`")));
    }
    Ok(Some(t.to_string()))
}

fn spec(p: &Value, cmd: &str) -> Result<Spec> {
    let shape = match str_param(p, "shape").map(str::trim).unwrap_or("circle").to_ascii_lowercase().as_str() {
        "circle" | "round" => "circle",
        "oval" => "oval",
        "rectangle" | "rect" => "rectangle",
        "rounded" => "rounded",
        "shield" => "shield",
        other => return Err(bad(cmd, format!("`shape` is one of {}, not `{other}`", SHAPES.join(", ")))),
    };
    let w = num(p, "width", 15.0..=300.0, cmd)?.unwrap_or(75.0);
    let h = match num(p, "height", 15.0..=300.0, cmd)? {
        Some(h) if shape != "circle" => h,
        _ if shape == "circle" => w,
        _ => (w * 0.6).max(15.0),
    };
    let border = match num(p, "border", 0.0..=8.0, cmd)? {
        None => 3.5,
        Some(b) if b == 0.0 || b >= 1.5 => b,
        Some(_) => return Err(bad(cmd, "`border` is 0 (none) or 1.5 to 8 mm")),
    };
    let background = match p.get("background") {
        Some(Value::Null) => None,
        _ => Some(color(p, "background", "#f1faee", cmd)?),
    };
    let names: Option<Vec<Option<String>>> = match p.get("names") {
        None | Some(Value::Null) => None,
        Some(Value::Array(a)) => {
            let mut v = Vec::with_capacity(a.len().min(MAX_PATCHES));
            for n in a.iter().filter_map(Value::as_str) {
                if let Some(w) = words(n, cmd)? {
                    v.push(Some(w));
                }
            }
            Some(v)
        }
        Some(Value::String(s)) => Some(s.lines().map(|l| words(l, cmd)).collect::<Result<Vec<_>>>()?.into_iter().flatten().map(Some).collect()),
        Some(_) => return Err(bad(cmd, "`names` is a list of names")),
    };
    let words_list = match names {
        Some(n) if n.is_empty() || n.len() > MAX_PATCHES => return Err(bad(cmd, format!("`names` are 1 to {MAX_PATCHES} names"))),
        Some(n) => n,
        None => {
            let one = str_param(p, "text").map(|t| words(t, cmd)).transpose()?.flatten();
            let count = match p.get("count") {
                None | Some(Value::Null) => 0,
                Some(v) => {
                    v.as_u64().filter(|c| (1..=MAX_PATCHES as u64).contains(c)).ok_or_else(|| bad(cmd, format!("`count` is 1 to {MAX_PATCHES}")))?
                        as usize
                }
            };
            // 0: as many as fit one hoop, decided once the hoop is known
            vec![one; count]
        }
    };
    Ok(Spec {
        shape,
        w,
        h,
        border,
        border_color: color(p, "borderColor", "#1d3557", cmd)?,
        background,
        text_color: color(p, "textColor", "#1d3557", cmd)?,
        text_height: num(p, "textHeight", 5.0..=100.0, cmd)?,
        gap: num(p, "gap", 2.0..=50.0, cmd)?.unwrap_or(5.0),
        words: words_list,
    })
}

/// The hoops to choose from: My Machine's, else the usual ones (name, field width, height).
fn hoops(s: &mut Session) -> Result<Vec<(String, f64, f64)>> {
    let mine = s.execute("embroidery.myMachine", &json!({}))?;
    let mut v: Vec<(String, f64, f64)> = mine["hoops"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|h| Some((h["name"].as_str()?.to_string(), h["width"].as_f64()?, h["height"].as_f64()?)))
        .collect();
    if v.is_empty() {
        v = ["Brother 100 × 100 (4×4)", "Brother 130 × 180 (5×7)", "Brother 160 × 260 (6×10)", "Brother 200 × 200 (8×8)"]
            .iter()
            .filter_map(|n| super::embroidery_hoops::find_hoop(s, n).map(|h| (h.name.clone(), h.width, h.height)))
            .collect();
    }
    v.sort_by(|a, b| (a.1 * a.2).total_cmp(&(b.1 * b.2)));
    Ok(v)
}

/// How many `w` × `h` cells fit `gap` apart in a `fw` × `fh` field less the margin → (columns, rows).
fn grid(fw: f64, fh: f64, w: f64, h: f64, gap: f64) -> (usize, usize) {
    let fit = |field: f64, size: f64| {
        let room = field - 2.0 * MARGIN;
        if room < size { 0 } else { (((room - size) / (size + gap)).floor() as usize).saturating_add(1) }
    };
    (fit(fw, w), fit(fh, h))
}

/// The hoop for these patches: `hoop`, else the largest of the hoops for several (more to a
/// hoopful), the smallest that holds one for one → (newDesign's `hoop`, field w, h).
fn hoop_for(s: &mut Session, p: &Value, sp: &Spec, cmd: &str) -> Result<(Value, f64, f64)> {
    if let Some(name) = str_param(p, "hoop") {
        let h = super::embroidery_hoops::find_hoop(s, name).ok_or_else(|| bad(cmd, format!("unknown hoop `{name}`")))?;
        return Ok((json!(h.name), h.width, h.height));
    }
    let all = hoops(s)?;
    let holds = |(_, w, h): &&(String, f64, f64)| grid(*w, *h, sp.w, sp.h, sp.gap).0 >= 1 && grid(*w, *h, sp.w, sp.h, sp.gap).1 >= 1;
    let one = sp.words.len() == 1;
    let pick = if one { all.iter().find(holds) } else { all.iter().rev().find(holds) };
    let (name, w, h) = pick.cloned().ok_or_else(|| bad(cmd, "the patch is bigger than every hoop: make it smaller or name a larger `hoop`"))?;
    Ok((json!(name), w, h))
}

/// The outline of a patch `w` × `h` mm round `c` (points), `inset` mm in from its edge → its id.
fn outline(s: &mut Session, shape: &str, c: kurbo::Point, w: f64, h: f64, inset: f64) -> Result<u64> {
    const CMD: &str = "embroidery.patches.make";
    let (w, h) = ((w - 2.0 * inset).max(1.0) * PT_PER_MM, (h - 2.0 * inset).max(1.0) * PT_PER_MM);
    let (x0, y0) = (c.x - w / 2.0, c.y - h / 2.0);
    let r = match shape {
        "circle" | "oval" => s.execute("shape.ellipse", &json!({ "x": x0, "y": y0, "width": w, "height": h }))?,
        "rounded" => s.execute("shape.rectangle", &json!({ "x": x0, "y": y0, "width": w, "height": h, "radius": w.min(h) * 0.2 }))?,
        "shield" => {
            // a flat top, straight sides down half its height, curving in to a point
            let (x1, y1) = (x0 + w, y0 + h);
            let d = format!(
                "M {x0} {y0} L {x1} {y0} L {x1} {m} Q {x1} {q} {cx} {y1} Q {x0} {q} {x0} {m} Z",
                m = y0 + h * 0.5,
                q = y0 + h * 0.85,
                cx = c.x
            );
            s.execute("path.create", &json!({ "d": d }))?
        }
        _ => s.execute("shape.rectangle", &json!({ "x": x0, "y": y0, "width": w, "height": h }))?,
    };
    r["id"].as_u64().ok_or_else(|| bad(CMD, "a patch's outline wasn't made"))
}

fn bounds(s: &Session, id: u64) -> Result<Option<kurbo::Rect>> {
    let d = &s.doc()?.doc;
    Ok(d.node(vectorcraft_doc::NodeId(id)).and_then(|n| n.bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false)))
}

/// Moves `id` so its middle is at `c` (points).
fn centre_on(s: &mut Session, id: u64, c: kurbo::Point) -> Result<()> {
    if let Some(b) = bounds(s, id)? {
        let (dx, dy) = (c.x - b.center().x, c.y - b.center().y);
        if dx.is_finite() && dy.is_finite() && (dx.abs() > 1e-3 || dy.abs() > 1e-3) {
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("object.move", &json!({ "dx": dx, "dy": dy }))?;
        }
    }
    Ok(())
}

/// The room inside a patch for its words (mm): inside the edge, and inside the circle or the
/// shield's narrowing.
fn room(sp: &Spec) -> (f64, f64) {
    let inside = sp.border + 2.5;
    let (w, h) = (sp.w - 2.0 * inside, sp.h - 2.0 * inside);
    match sp.shape {
        "circle" | "oval" => (w * 0.78, h * 0.6),
        "shield" => (w * 0.85, h * 0.55),
        _ => (w * 0.92, h * 0.8),
    }
}

/// One hoopful of patches, `words` one a patch, in a new design → (hoop, made).
fn build(s: &mut Session, sp: &Spec, words: &[Option<String>], hoop: &Value, field: (f64, f64)) -> Result<(Value, usize)> {
    let (cols, _) = grid(field.0, field.1, sp.w, sp.h, sp.gap);
    let made = s.execute("embroidery.newDesign", &json!({ "hoop": hoop }))?;
    let page = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    let n = words.len();
    let rows = n.div_ceil(cols.max(1));
    let used = (cols.min(n) as f64 * (sp.w + sp.gap) - sp.gap, rows as f64 * (sp.h + sp.gap) - sp.gap);
    // the patches' middles, the block of them centred in the hoop
    let mid = page.center();
    let centres: Vec<kurbo::Point> = (0..n)
        .map(|k| {
            let (col, row) = ((k % cols.max(1)) as f64, (k / cols.max(1)) as f64);
            kurbo::Point::new(
                mid.x + (-used.0 / 2.0 + sp.w / 2.0 + col * (sp.w + sp.gap)) * PT_PER_MM,
                mid.y + (-used.1 / 2.0 + sp.h / 2.0 + row * (sp.h + sp.gap)) * PT_PER_MM,
            )
        })
        .collect();
    // every background, then every patch's words, then every edge: a thread each for the hoopful
    if let Some(bg) = &sp.background {
        for c in &centres {
            let id = outline(s, sp.shape, *c, sp.w, sp.h, sp.border * 0.5)?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("paint.setFill", &json!({ "color": bg }))?;
            s.execute("paint.setStroke", &json!({ "none": true }))?;
            s.execute("embroidery.set", &json!({ "type": "fill" }))?;
            s.execute("embroidery.setThread", &json!({ "color": bg, "name": "Patch background" }))?;
        }
    }
    let (rw, rh) = room(sp);
    for (c, w) in centres.iter().zip(words) {
        let Some(text) = w else { continue };
        let lines = text.lines().count().max(1) as f64;
        let height = sp.text_height.unwrap_or((rh / lines * 0.75).min(rw * 0.3)).clamp(5.0, 100.0);
        let id = s.execute("embroidery.addWords", &json!({ "text": text, "height": height, "color": sp.text_color }))?["id"]
            .as_u64()
            .ok_or_else(|| bad("embroidery.patches.make", "a patch's words weren't made"))?;
        // too wide or tall for the patch: smaller until they fit
        if let Some(b) = bounds(s, id)? {
            let (bw, bh) = (b.width() / PT_PER_MM, b.height() / PT_PER_MM);
            let fit = (rw / bw.max(1e-6)).min(rh / bh.max(1e-6));
            if fit.is_finite() && fit < 1.0 {
                let size = height * PT_PER_MM / 0.7 * fit * 0.97;
                s.execute("text.setStyle", &json!({ "ids": [id], "size": size }))?;
            }
        }
        centre_on(s, id, *c)?;
    }
    if sp.border > 0.0 {
        for c in &centres {
            let id = outline(s, sp.shape, *c, sp.w, sp.h, sp.border * 0.5)?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("paint.setFill", &json!({ "none": true }))?;
            s.execute("paint.setStroke", &json!({ "color": sp.border_color }))?;
            s.execute("embroidery.set", &json!({ "type": "satin" }))?;
            s.execute("embroidery.setParams", &json!({ "params": { "width": sp.border } }))?;
            s.execute("embroidery.setThread", &json!({ "color": sp.border_color, "name": "Patch edge" }))?;
        }
    }
    s.execute("select.none", &json!({}))?;
    Ok((made["hoop"].clone(), n))
}

/// The spec, its hoop, and the patches a hoopful holds.
fn plan(s: &mut Session, p: &Value, cmd: &str) -> Result<(Spec, Value, (f64, f64), usize)> {
    let mut sp = spec(p, cmd)?;
    // "as many as fit": decided against the largest hoop
    let fill_one = sp.words.is_empty();
    if fill_one {
        sp.words = vec![None; 2];
    }
    let (hoop, fw, fh) = hoop_for(s, p, &sp, cmd)?;
    let (cols, rows) = grid(fw, fh, sp.w, sp.h, sp.gap);
    let per = cols * rows;
    if per == 0 {
        return Err(bad(cmd, "the patch is bigger than the hoop: make it smaller or choose a larger hoop"));
    }
    if fill_one {
        let one = str_param(p, "text").map(|t| words(t, cmd)).transpose()?.flatten();
        sp.words = vec![one; per];
    }
    Ok((sp, hoop, (fw, fh), per))
}

fn make(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.patches.make";
    let (sp, hoop, field, per) = plan(s, p, CMD)?;
    let hoopfuls = sp.words.len().div_ceil(per);
    let which = match p.get("hoopful") {
        None | Some(Value::Null) => 1,
        Some(v) => {
            v.as_u64().filter(|k| (1..=hoopfuls as u64).contains(k)).ok_or_else(|| bad(CMD, format!("`hoopful` is 1 to {hoopfuls}")))? as usize
        }
    };
    let chunk: Vec<Option<String>> = sp.words.iter().skip((which - 1) * per).take(per).cloned().collect();
    let (hoop, made) = super::embroidery_simple::one_step(s, |s| build(s, &sp, &chunk, &hoop, field))?;
    if let Ok(st) = s.doc_mut() {
        std::sync::Arc::make_mut(&mut st.doc).title = if hoopfuls > 1 { format!("Patches {which} of {hoopfuls}") } else { "Patches".into() };
    }
    Ok(json!({ "hoop": hoop, "perHoop": per, "patches": sp.words.len(), "hoopfuls": hoopfuls, "hoopful": which, "made": made }))
}

fn export(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.patches.export";
    let dir = str_param(p, "dir").filter(|d| !d.trim().is_empty()).ok_or_else(|| bad(CMD, "missing `dir`: the folder to write to"))?.to_string();
    let prefix = super::embroidery_lettering::safe_name(str_param(p, "prefix").unwrap_or("patches"));
    let (sp, hoop, field, per) = plan(s, p, CMD)?;
    let hoopfuls = sp.words.len().div_ceil(per);
    if hoopfuls > MAX_HOOPFULS {
        return Err(bad(CMD, format!("that is {hoopfuls} hoopfuls: {MAX_HOOPFULS} at most at a time")));
    }
    let format = match str_param(p, "format") {
        Some(f) => f.trim().trim_start_matches('.').to_ascii_lowercase(),
        None => super::embroidery_my_machine::mine(s).map(|m| m.format.to_string()).unwrap_or_else(|| "dst".into()),
    };
    if !super::embroidery::MACHINE_EXTS.contains(&format.as_str()) {
        return Err(bad(CMD, format!("unknown format `{format}`: one of {}", super::embroidery::MACHINE_EXTS.join(", "))));
    }
    crate::cmd::fileio::create_dir(&dir)?;
    let was = s.active_index();
    let mut files = Vec::with_capacity(hoopfuls);
    let mut list = String::new();
    for (k, chunk) in sp.words.chunks(per).enumerate() {
        build(s, &sp, chunk, &hoop, field)?;
        let made = s.active_index();
        let name = format!("{prefix}-{}", k + 1);
        let out = s.execute("embroidery.export", &json!({ "format": format, "name": name }));
        // the hoopful's document goes, written or not
        if let Some(i) = made {
            s.close_document(i);
        }
        let out = out?;
        let bytes =
            out["dataBase64"].as_str().and_then(vectorcraft_format::base64_decode).ok_or_else(|| bad(CMD, "a hoopful's machine file wasn't made"))?;
        let path = std::path::Path::new(&dir).join(format!("{name}.{format}")).to_string_lossy().to_string();
        crate::cmd::fileio::write_file(&path, &bytes)?;
        files.push(path);
        let names: Vec<&str> = chunk.iter().flatten().map(String::as_str).collect();
        if !names.is_empty() {
            list.push_str(&format!("{name}.{format}: {}\n", names.iter().map(|n| n.replace('\n', " / ")).collect::<Vec<_>>().join(", ")));
        }
    }
    if let Some(i) = was {
        s.set_active(i);
    }
    let mut r = json!({ "files": files, "hoopfuls": hoopfuls, "perHoop": per, "patches": sp.words.len() });
    if !list.is_empty() {
        let path = std::path::Path::new(&dir).join(format!("{prefix}-names.txt")).to_string_lossy().to_string();
        crate::cmd::fileio::write_file(&path, list.as_bytes())?;
        r["list"] = json!(path);
    }
    Ok(r)
}

fn fill_hoop(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.patches.fillHoop";
    let gap = num(p, "gap", 2.0..=50.0, CMD)?.unwrap_or(5.0);
    let most = match p.get("count") {
        None | Some(Value::Null) => usize::MAX,
        Some(v) => {
            v.as_u64().filter(|c| (1..=MAX_PATCHES as u64).contains(c)).ok_or_else(|| bad(CMD, format!("`count` is 1 to {MAX_PATCHES}")))? as usize
        }
    };
    let hoop = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.hoop.clone()).ok_or_else(|| bad(CMD, "choose a hoop for the design first"))?;
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let st = plan.stats.clone().ok_or_else(|| bad(CMD, "nothing is stitched yet: give objects a stitch type first"))?;
    let (w, h) = (st.width_mm, st.height_mm);
    let (cols, rows) = grid(hoop.width, hoop.height, w, h, gap);
    let copies = (cols * rows).min(most);
    if copies <= 1 {
        return Err(bad(CMD, "only one copy fits this hoop: choose a larger hoop or a smaller gap"));
    }
    // the whole design (the layers' objects), and its bounds (points): the copies are laid out from them
    s.execute("select.all", &json!({}))?;
    let ids = targets(s, &json!({}))?;
    let b = {
        let d = &s.doc()?.doc;
        ids.iter().filter_map(|id| d.node(*id).and_then(|n| n.bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false))).reduce(|a, b| a.union(b))
    };
    let b = b.ok_or_else(|| bad(CMD, "nothing is stitched yet: give objects a stitch type first"))?;
    let page = s.doc()?.doc.artboards.first().map(|a| a.rect).ok_or_else(|| EngineError::Other("the design has no page".into()))?;
    let (cw, ch) = (b.width() + gap * PT_PER_MM, b.height() + gap * PT_PER_MM);
    let used_rows = copies.div_ceil(cols);
    let used = (cols.min(copies) as f64 * cw - gap * PT_PER_MM, used_rows as f64 * ch - gap * PT_PER_MM);
    let origin = kurbo::Point::new(page.center().x - used.0 / 2.0, page.center().y - used.1 / 2.0);
    super::embroidery_simple::one_step(s, |s| {
        let id_vals: Vec<u64> = ids.iter().map(|i| i.0).collect();
        for k in (0..copies).rev() {
            let (col, row) = ((k % cols) as f64, (k / cols) as f64);
            let to = kurbo::Point::new(origin.x + col * cw, origin.y + row * ch);
            let (dx, dy) = (to.x - b.x0, to.y - b.y0);
            s.execute("select.set", &json!({ "ids": id_vals }))?;
            if k == 0 {
                if dx.abs() > 1e-3 || dy.abs() > 1e-3 {
                    s.execute("object.move", &json!({ "dx": dx, "dy": dy }))?;
                }
            } else {
                s.execute("object.move", &json!({ "dx": dx, "dy": dy, "copy": true }))?;
            }
        }
        s.execute("embroidery.sortByColor", &json!({}))?;
        s.execute("select.none", &json!({}))
    })?;
    Ok(json!({ "copies": copies, "columns": cols.min(copies), "rows": used_rows }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patches_fit_the_hoop_in_rows() {
        // 75 mm circles in a 200 × 200 field (190 usable), 5 mm apart: 2 × 2
        assert_eq!(grid(200.0, 200.0, 75.0, 75.0, 5.0), (2, 2));
        // a 360 × 200 hoop: 4 across, 2 down
        assert_eq!(grid(360.0, 200.0, 75.0, 75.0, 5.0), (4, 2));
        // name tapes 100 × 30 in a 5×7 field (120 × 170 usable): 1 across, 5 down
        assert_eq!(grid(130.0, 180.0, 100.0, 30.0, 5.0), (1, 5));
        // too big: none
        assert_eq!(grid(100.0, 100.0, 120.0, 50.0, 5.0).0, 0);
    }
}
