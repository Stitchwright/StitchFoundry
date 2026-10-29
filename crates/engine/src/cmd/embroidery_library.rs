//! The design library (DM-01): the machine embroidery files in a folder and its subfolders, each
//! with its size, stitches, colours and date and an outline of its stitches for a thumbnail,
//! searched by name; and a catalogue of them to print (NEW-30). What a file says is kept while
//! its size and date stay the same, so looking again is quick.

use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document};

use super::embroidery::{MACHINE_EXTS, MACHINE_READ_EXTS, sheet_text};
use super::{CommandSpec, EngineError, Result, Session, always, bad, bool_or, cmd, str_param};

/// Designs listed at most.
const MAX_DESIGNS: usize = 2000;
/// Largest file read (bytes).
const MAX_BYTES: u64 = 64 << 20;
/// Subfolders looked into, this deep at most.
const MAX_DEPTH: usize = 6;
/// Entries of one folder read at most.
const MAX_ENTRIES: usize = 5000;
/// Points kept for a design's thumbnail, at most.
const PREVIEW_POINTS: usize = 800;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.library.scan",
            "Design Library",
            [],
            None,
            "{folder, query?: text in the name or folder (any case), recursive?: bool (true: subfolders too, 6 deep), sort?: name|date|stitches|size (name)} the machine embroidery files in a folder (dst, exp, jef, pes, pec, vp3, xxx, u01, pcs, tap, dsb, dsz, hus; 2000 at most) → {folder, designs: [{path, name, folder, format, bytes, modified: ms since 1970, widthMm, heightMm, stitches, colors, threads: [\"#rrggbb\"], preview: [{color, runs: [[[x, y], …]]}] (mm from the design's top left, simplified)}], failed: [{path, error}], truncated}",
            always,
            scan
        ),
        cmd!(
            "embroidery.library.catalogue",
            "Design Catalogue",
            [],
            None,
            "{folder, query?, recursive?, sort?, columns?: 2–6 (4), page?: a4|letter (a4)} a catalogue of the designs in a folder, as embroidery.library.scan finds them, to print: each drawn in its colours in a grid with its name, size and stitches, a page (artboard) per sheetful; opens a new document → {designs, pages}",
            always,
            catalogue
        ),
        cmd!(
            "embroidery.library.convert",
            "Batch Convert",
            [],
            None,
            "{format: dst|exp|jef|pes|pec|vp3|xxx|u01|pcs|tap|dsb, folder?: the designs in it (as embroidery.library.scan finds them), query?, recursive? (as embroidery.library.scan), files?: [path…] (these designs instead), dir?: the folder to write to (each design's own), replace?: bool (false: a number is added when the name is taken)} batch convert: each design read and written again in `format`, named as it was; designs already in that format are skipped and the files read are left as they are → {format, converted: [{from, to, stitches, colors}], skipped: [path], failed: [{path, error}], truncated}",
            always,
            convert
        ),
    ]
}

/// One design as the library lists it, and the size and date it was read at.
#[derive(Clone)]
pub struct Entry {
    bytes: u64,
    modified: Option<u64>,
    info: Value,
}

/// The machine files under `folder` (and its subfolders, when `recursive`), paths sorted.
fn machine_files(folder: &str, recursive: bool) -> Result<(Vec<String>, bool)> {
    let mut out = Vec::new();
    let mut truncated = false;
    let mut stack = vec![(folder.trim_end_matches(['/', '\\']).to_string(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let (entries, more) = super::fileio::list_dir(&dir, MAX_ENTRIES)?;
        truncated |= more;
        for e in entries {
            let Some(name) = e["name"].as_str() else { continue };
            let path = std::path::Path::new(&dir).join(name).to_string_lossy().to_string();
            match e["kind"].as_str() {
                Some("folder") if recursive && depth < MAX_DEPTH && !name.starts_with('.') => stack.push((path, depth + 1)),
                Some("file") => {
                    let ext = name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
                    if MACHINE_READ_EXTS.contains(&ext.as_str()) {
                        if out.len() >= MAX_DESIGNS {
                            truncated = true;
                            continue;
                        }
                        out.push(path);
                    }
                }
                _ => {}
            }
        }
    }
    out.sort();
    Ok((out, truncated))
}

/// A design's stitches, read from its file in the format its name says: a file that isn't one
/// fails rather than being guessed at.
pub(super) fn read_pattern(path: &str, bytes: u64) -> std::result::Result<sf_core::Pattern, String> {
    if bytes > MAX_BYTES {
        return Err(format!("larger than {} MB", MAX_BYTES >> 20));
    }
    let p = std::path::Path::new(path);
    let ext = p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let format = sf_formats::Format::from_extension(&ext)
        .filter(|_| MACHINE_READ_EXTS.contains(&ext.as_str()))
        .ok_or_else(|| format!("not a machine file: .{ext}"))?;
    let data = super::fileio::read_file(path).map_err(|e| e.to_string())?;
    sf_formats::read(format, &data).map_err(|e| e.to_string())
}

/// A design's figures and thumbnail, read from its file.
fn read_design(path: &str, bytes: u64, modified: Option<u64>) -> std::result::Result<Value, String> {
    let pattern = read_pattern(path, bytes)?;
    let p = std::path::Path::new(path);
    let format = p.extension().and_then(|x| sf_formats::Format::from_extension(&x.to_string_lossy().to_ascii_lowercase()));
    let stats = sf_core::stats::pattern_stats(&pattern, &sf_core::model::DesignSettings::default());
    let preview = preview_runs(&pattern);
    let r = |v: f64| (v * 10.0).round() / 10.0;
    let threads: Vec<String> = stats.colors.iter().map(|c| c.thread.color.to_hex()).collect();
    Ok(json!({
        "path": path,
        "name": p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        "folder": p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default(),
        "format": format.map(sf_formats::Format::extension),
        "bytes": bytes,
        "modified": modified,
        "widthMm": r(stats.width_mm),
        "heightMm": r(stats.height_mm),
        "stitches": stats.stitches,
        "colors": stats.colors.len(),
        "threads": threads,
        "preview": preview,
    }))
}

/// A design's stitches as runs per colour (mm from its top left), thinned to a thumbnail's worth
/// of points: `[{color, runs: [[[x, y], …], …]}, …]`.
pub(super) fn preview_runs(pattern: &sf_core::Pattern) -> Vec<Value> {
    let b = pattern.bounds();
    // the stitches as runs per colour, thinned to a thumbnail's worth of points
    let blocks = super::embroidery::stitch_runs(pattern);
    let total: usize = blocks.iter().flatten().map(Vec::len).sum();
    let step = total.div_ceil(PREVIEW_POINTS).max(1);
    let r = |v: f64| (v * 10.0).round() / 10.0;
    blocks
        .iter()
        .enumerate()
        .filter(|(_, runs)| !runs.is_empty())
        .map(|(i, runs)| {
            let runs: Vec<Value> = runs
                .iter()
                .map(|run| {
                    let last = run.len().saturating_sub(1);
                    let pts: Vec<Value> = run
                        .iter()
                        .enumerate()
                        .filter(|(k, _)| k % step == 0 || *k == last)
                        .map(|(_, q)| json!([r(q.x - b.min.x), r(q.y - b.min.y)]))
                        .collect();
                    Value::Array(pts)
                })
                .filter(|run| run.as_array().is_some_and(|a| a.len() >= 2))
                .collect();
            json!({ "color": pattern.thread(i).color.to_hex(), "runs": runs })
        })
        .collect()
}

/// The designs `p` asks for, read or from the cache: (designs, failed, truncated).
fn designs(s: &mut Session, p: &Value, cmd: &str) -> Result<(String, Vec<Value>, Vec<Value>, bool)> {
    let folder = str_param(p, "folder").map(str::trim).filter(|f| !f.is_empty()).ok_or_else(|| bad(cmd, "missing `folder`"))?.to_string();
    let recursive = bool_or(p, "recursive", true);
    let query = str_param(p, "query").map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
    let sort = str_param(p, "sort").unwrap_or("name");
    if !["name", "date", "stitches", "size"].contains(&sort) {
        return Err(bad(cmd, "`sort` is name, date, stitches or size"));
    }
    let (files, truncated) = machine_files(&folder, recursive)?;
    let mut found = Vec::new();
    let mut failed = Vec::new();
    let mut fresh = std::collections::HashMap::new();
    for path in files {
        if let Some(q) = &query
            && !path.to_lowercase().contains(q.as_str())
        {
            continue;
        }
        let Some((bytes, modified)) = super::fileio::file_stamp(&path) else { continue };
        let info = match s.design_library.remove(&path) {
            Some(e) if e.bytes == bytes && e.modified == modified => e.info,
            _ => match read_design(&path, bytes, modified) {
                Ok(info) => info,
                Err(error) => {
                    failed.push(json!({ "path": path, "error": error }));
                    continue;
                }
            },
        };
        fresh.insert(path, Entry { bytes, modified, info: info.clone() });
        found.push(info);
    }
    // keep what this folder gave, and what others gave before
    s.design_library.extend(fresh);
    if s.design_library.len() > 4 * MAX_DESIGNS {
        s.design_library.clear();
    }
    let key = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0);
    match sort {
        "date" => found.sort_by(|a, b| key(b, "modified").total_cmp(&key(a, "modified"))),
        "stitches" => found.sort_by(|a, b| key(b, "stitches").total_cmp(&key(a, "stitches"))),
        "size" => found.sort_by(|a, b| (key(b, "widthMm") * key(b, "heightMm")).total_cmp(&(key(a, "widthMm") * key(a, "heightMm")))),
        _ => found.sort_by_key(|v| v["name"].as_str().unwrap_or_default().to_lowercase()),
    }
    Ok((super::fileio::absolute_path(&folder), found, failed, truncated))
}

fn convert(_: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.library.convert";
    let ext = str_param(p, "format").map(|f| f.trim().trim_start_matches('.').to_ascii_lowercase()).unwrap_or_default();
    let format = sf_formats::Format::from_extension(&ext)
        .filter(|_| MACHINE_EXTS.contains(&ext.as_str()))
        .ok_or_else(|| bad(CMD, format!("`format` is one of {}", MACHINE_EXTS.join(", "))))?;
    let dir = str_param(p, "dir").map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
    if let Some(d) = &dir
        && !super::embroidery::folder_ready(d)
    {
        return Err(bad(CMD, format!("{d} isn't there")));
    }
    let (files, truncated) = match p.get("files") {
        None | Some(Value::Null) => {
            let folder = str_param(p, "folder").map(str::trim).filter(|f| !f.is_empty()).ok_or_else(|| bad(CMD, "missing `folder` or `files`"))?;
            let query = str_param(p, "query").map(|q| q.trim().to_lowercase()).filter(|q| !q.is_empty());
            let (files, truncated) = machine_files(folder, bool_or(p, "recursive", true))?;
            let files = files.into_iter().filter(|f| query.as_ref().is_none_or(|q| f.to_lowercase().contains(q.as_str()))).collect();
            (files, truncated)
        }
        Some(Value::Array(list)) => {
            let files: Vec<String> = list.iter().take(MAX_DESIGNS).filter_map(Value::as_str).map(str::to_string).collect();
            if files.is_empty() {
                return Err(bad(CMD, "`files` lists no designs"));
            }
            (files, list.len() > MAX_DESIGNS)
        }
        Some(_) => return Err(bad(CMD, "`files` is a list of paths")),
    };
    let replace = bool_or(p, "replace", false);
    let (mut converted, mut skipped, mut failed) = (Vec::new(), Vec::new(), Vec::new());
    for path in files {
        let src = std::path::Path::new(&path);
        let own = src.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if own == ext {
            skipped.push(json!(path));
            continue;
        }
        let bytes = super::fileio::file_stamp(&path).map_or(0, |(b, _)| b);
        let read = read_pattern(&path, bytes).and_then(|pattern| {
            let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let opts = sf_formats::WriteOptions { name: stem.clone(), center: true, ..sf_formats::WriteOptions::default() };
            let data = sf_formats::write(format, &pattern, &opts).map_err(|e| e.to_string())?;
            Ok((pattern, stem, data))
        });
        let (pattern, stem, data) = match read {
            Ok(r) => r,
            Err(error) => {
                failed.push(json!({ "path": path, "error": error }));
                continue;
            }
        };
        let base = dir.as_deref().map(std::path::Path::new).or_else(|| src.parent()).unwrap_or(std::path::Path::new("."));
        let mut to = base.join(format!("{stem}.{ext}"));
        if !replace {
            let mut k = 2;
            while super::embroidery::file_there(&to) && k < 1000 {
                to = base.join(format!("{stem}-{k}.{ext}"));
                k += 1;
            }
        }
        let to = to.to_string_lossy().to_string();
        match super::fileio::write_file(&to, &data) {
            Ok(()) => converted.push(json!({ "from": path, "to": to, "stitches": pattern.stitch_count(), "colors": pattern.color_blocks().len() })),
            Err(e) => failed.push(json!({ "path": path, "error": e.to_string() })),
        }
    }
    Ok(json!({ "format": ext, "converted": converted, "skipped": skipped, "failed": failed, "truncated": truncated }))
}

fn scan(s: &mut Session, p: &Value) -> Result<Value> {
    let (folder, found, failed, truncated) = designs(s, p, "embroidery.library.scan")?;
    Ok(json!({ "folder": folder, "count": found.len(), "designs": found, "failed": failed, "truncated": truncated }))
}

fn catalogue(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.library.catalogue";
    let columns = match p.get("columns") {
        None | Some(Value::Null) => 4,
        Some(v) => v.as_u64().filter(|c| (2..=6).contains(c)).map(|c| c as usize).ok_or_else(|| bad(CMD, "`columns` is 2 to 6"))?,
    };
    let (pw, ph) = match str_param(p, "page").unwrap_or("a4") {
        "a4" | "A4" => super::embroidery::SHEET,
        "letter" | "Letter" => (612.0, 792.0),
        _ => return Err(bad(CMD, "`page` is a4 or letter")),
    };
    let (folder, found, _, _) = designs(s, p, CMD)?;
    if found.is_empty() {
        return Err(bad(CMD, "no machine embroidery files there"));
    }
    // the grid: square cells, a caption under each
    let (margin, top, caption) = (36.0, 80.0, 26.0);
    let cell = (pw - 2.0 * margin) / columns as f64;
    let rows = (((ph - top - margin) / (cell + caption)).floor() as usize).max(1);
    let per_page = columns * rows;
    let pages = found.len().div_ceil(per_page);
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let grey = Color::rgb8(0x66, 0x66, 0x66);
    let mut doc = Document::new(pw, ph);
    let name = std::path::Path::new(&folder).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| folder.clone());
    doc.title = format!("{name} catalogue");
    for page in 0..pages {
        // the pages side by side, a gap apart
        let ox = page as f64 * (pw + 40.0);
        if page > 0 {
            let id = doc.next_artboard_id();
            let rect = vectorcraft_geom::Rect::new(ox, 0.0, ox + pw, ph);
            doc.artboards.push(vectorcraft_doc::Artboard {
                id,
                name: format!("Page {}", page + 1),
                rect,
                show_center_mark: false,
                show_cross_hairs: false,
                locked: false,
                show_video_safe_areas: false,
                video_ruler_pixel_aspect_ratio: 1.0,
                color: None,
            });
        }
        sheet_text(&mut doc, ox + margin, 46.0, &name, 16.0, black)?;
        sheet_text(&mut doc, ox + margin, 64.0, &format!("{} designs   page {} of {pages}", found.len(), page + 1), 9.0, grey)?;
        for (k, d) in found.iter().skip(page * per_page).take(per_page).enumerate() {
            let (col, row) = (k % columns, k / columns);
            let (x0, y0) = (ox + margin + col as f64 * cell, top + row as f64 * (cell + caption));
            draw_preview(&mut doc, d, x0 + 4.0, y0 + 4.0, cell - 8.0)?;
            let title: String = d["name"].as_str().unwrap_or_default().chars().take(28).collect();
            sheet_text(&mut doc, x0 + 4.0, y0 + cell + 8.0, &title, 7.5, black)?;
            let figures = format!(
                "{:.0} × {:.0} mm   {} st   {} col   {}",
                d["widthMm"].as_f64().unwrap_or(0.0),
                d["heightMm"].as_f64().unwrap_or(0.0),
                d["stitches"].as_u64().unwrap_or(0),
                d["colors"].as_u64().unwrap_or(0),
                d["format"].as_str().unwrap_or_default().to_uppercase()
            );
            sheet_text(&mut doc, x0 + 4.0, y0 + cell + 18.0, &figures, 6.5, grey)?;
        }
    }
    s.add_document(doc, None);
    Ok(json!({ "designs": found.len(), "pages": pages }))
}

/// A design's thumbnail runs drawn in its colours, fitted in a `size` square at (`x`, `y`).
fn draw_preview(doc: &mut Document, d: &Value, x: f64, y: f64, size: f64) -> Result<()> {
    use vectorcraft_geom::path::{PathData, SubPath};
    let (w, h) = (d["widthMm"].as_f64().unwrap_or(0.0), d["heightMm"].as_f64().unwrap_or(0.0));
    let k = size / w.max(h).max(1.0);
    let (dx, dy) = (x + (size - w * k) / 2.0, y + (size - h * k) / 2.0);
    let layer = doc.layers.first().map(|l| l.id);
    for block in d["preview"].as_array().into_iter().flatten() {
        let Some(c) = block["color"].as_str().and_then(sf_core::Rgb::from_hex) else { continue };
        let subpaths: Vec<SubPath> = block["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_array)
            .map(|run| {
                run.iter().filter_map(|q| Some(kurbo::Point::new(dx + q.get(0)?.as_f64()? * k, dy + q.get(1)?.as_f64()? * k))).collect::<Vec<_>>()
            })
            .filter(|pts| pts.len() >= 2)
            .map(|pts| SubPath::polyline(&pts, false))
            .collect();
        if subpaths.is_empty() {
            continue;
        }
        let n = vectorcraft_doc::Node::path(
            doc.alloc_id(),
            PathData::new(subpaths),
            Appearance::basic(Paint::None, Paint::solid(Color::rgb8(c.r, c.g, c.b)), 0.4),
        );
        doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    Ok(())
}
