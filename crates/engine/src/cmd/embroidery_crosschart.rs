//! Cross-stitch chart (CH-04): a cross-stitch pattern printed as a counted chart, the way hand
//! cross stitch is worked from paper: a symbol in every square (each thread colour its own
//! symbol, on the colour itself unless printed without colour), heavier lines every ten squares
//! with their numbers along the top and left, as many pages as the pattern takes, and a key of
//! the symbols with each thread's name, number and square count. It opens as a new document of
//! A4 or Letter pages, to print or export as PDF.

use kurbo::{BezPath, Circle, Point, Shape};
use serde_json::{Value, json};
use sf_core::Rgb;
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Artboard, Document, Node};
use vectorcraft_geom::{PathData, Rect};

use super::embroidery::{SHEET, sheet_lines, sheet_text};
use super::embroidery_crossstitch::{Pattern, chosen, style_name};
use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// The page margin (points).
const MARGIN: f64 = 36.0;
/// The room for the row numbers left of the grid and the column numbers above it (points).
const LABELS: f64 = 20.0;
/// The top of the grid below the page heading (points).
const TOP: f64 = 74.0;
/// Chart squares (points): the default and the range.
const SQUARE: (f64, f64, f64) = (10.0, 6.0, 24.0);
/// Gap between pages laid side by side in the document (points).
const PAGE_GAP: f64 = 40.0;
/// Most pages a chart may take.
const MAX_PAGES: usize = 200;
/// The base shapes symbols are made of; each comes in four fills, so 64 colours get their own.
const SHAPES: usize = 16;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.crossStitch.chart",
        "Print Cross-Stitch Chart",
        ["Embroidery", "Cross Stitch"],
        None,
        "{id?: the pattern (the selected one), page?: a4|letter (a4), square?: pt (10; 6 to 24, a chart square), colors?: bool (true: each square in its thread colour under its symbol; false: black symbols on white, to save ink)} the pattern as a counted chart to stitch from: a symbol a square, heavier lines every 10 squares, numbered, on as many pages as it takes, then a key of the symbols with each thread and its square count; opens as a new document to print or export as PDF; backstitch lines are drawn over the squares in their colours and listed in the key → {pages, colors, squares, backstitch}",
        has_doc,
        chart
    )]
}

/// Symbol `k`'s shapes, centred on `c`, `r` from its middle to its edge: the filled part and the
/// outlined part.
pub fn symbol(k: usize, c: Point, r: f64) -> (BezPath, BezPath) {
    let shape = k % SHAPES;
    let poly = |pts: &[(f64, f64)]| {
        let mut bp = BezPath::new();
        for (i, (x, y)) in pts.iter().enumerate() {
            let p = Point::new(c.x + x * r, c.y + y * r);
            if i == 0 {
                bp.move_to(p);
            } else {
                bp.line_to(p);
            }
        }
        bp.close_path();
        bp
    };
    let regular = |n: usize, turn: f64, scale: f64| {
        let pts: Vec<(f64, f64)> = (0..n)
            .map(|i| {
                let a = turn + std::f64::consts::TAU * i as f64 / n as f64;
                (scale * a.cos(), scale * a.sin())
            })
            .collect();
        poly(&pts)
    };
    let star = |n: usize, inner: f64| {
        let pts: Vec<(f64, f64)> = (0..2 * n)
            .map(|i| {
                let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * i as f64 / n as f64;
                let m = if i % 2 == 0 { 1.0 } else { inner };
                (m * a.cos(), m * a.sin())
            })
            .collect();
        poly(&pts)
    };
    let t = 0.28; // half a bar's width
    let up = -std::f64::consts::FRAC_PI_2;
    let outline = match shape {
        0 => Circle::new(c, r * 0.9).to_path(0.05),
        1 => poly(&[(-0.8, -0.8), (0.8, -0.8), (0.8, 0.8), (-0.8, 0.8)]),
        2 => regular(3, up, 1.0),
        3 => regular(3, -up, 1.0),
        4 => regular(4, up, 1.0),
        5 => poly(&[(-t, -1.0), (t, -1.0), (t, -t), (1.0, -t), (1.0, t), (t, t), (t, 1.0), (-t, 1.0), (-t, t), (-1.0, t), (-1.0, -t), (-t, -t)]),
        6 => {
            let s = std::f64::consts::FRAC_1_SQRT_2;
            let q = |x: f64, y: f64| ((x - y) * s, (x + y) * s);
            let pts: Vec<(f64, f64)> =
                [(-t, -1.0), (t, -1.0), (t, -t), (1.0, -t), (1.0, t), (t, t), (t, 1.0), (-t, 1.0), (-t, t), (-1.0, t), (-1.0, -t), (-t, -t)]
                    .iter()
                    .map(|(x, y)| q(*x, *y))
                    .collect();
            poly(&pts)
        }
        7 => star(5, 0.45),
        8 => regular(6, 0.0, 0.95),
        9 => regular(5, up, 0.95),
        10 => regular(3, std::f64::consts::PI, 1.0),
        11 => regular(3, 0.0, 1.0),
        12 => poly(&[(-0.9, -0.35), (0.9, -0.35), (0.9, 0.35), (-0.9, 0.35)]),
        13 => poly(&[(-0.35, -0.9), (0.35, -0.9), (0.35, 0.9), (-0.35, 0.9)]),
        14 => poly(&[(-0.8, -0.85), (0.8, -0.85), (0.0, 0.0), (0.8, 0.85), (-0.8, 0.85), (0.0, 0.0)]),
        _ => star(4, 0.4),
    };
    let mut filled = BezPath::new();
    let mut stroked = BezPath::new();
    match k / SHAPES % 4 {
        // solid
        0 => filled = outline,
        // outline
        1 => stroked = outline,
        // outline with a dot
        2 => {
            stroked = outline;
            filled = Circle::new(c, r * 0.22).to_path(0.05);
        }
        // outline round a small solid copy
        _ => {
            stroked = outline;
            let (inner, _) = symbol(shape, c, r * 0.45);
            filled = inner;
        }
    }
    (filled, stroked)
}

/// Adds a path to the first layer of `doc`.
fn put(doc: &mut Document, path: &BezPath, fill: Paint, stroke: Paint, width: f64) -> Result<()> {
    if path.elements().is_empty() {
        return Ok(());
    }
    let n = Node::path(doc.alloc_id(), PathData::from_bezpath(path), Appearance::basic(fill, stroke, width));
    let layer = doc.layers.first().map(|l| l.id);
    doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    Ok(())
}

fn color(c: Rgb) -> Color {
    Color::rgb8(c.r, c.g, c.b)
}

/// The colour a symbol is drawn in on `under`: black on light colours, white on dark ones.
fn ink_on(under: Option<Rgb>) -> Color {
    match under {
        Some(c) if c.luma() < 0.45 => Color::rgb8(255, 255, 255),
        _ => Color::rgb8(0x1a, 0x1a, 0x1a),
    }
}

fn rect_path(bp: &mut BezPath, x0: f64, y0: f64, x1: f64, y1: f64) {
    bp.move_to((x0, y0));
    bp.line_to((x1, y0));
    bp.line_to((x1, y1));
    bp.line_to((x0, y1));
    bp.close_path();
}

/// A row of the key: a colour of crosses, or of backstitch (indices into the pattern's lists).
#[derive(Clone, Copy)]
enum Row {
    Cross(usize),
    Back(usize),
}

/// A thread as the key names it: its chart, number and name, then its colour.
fn thread_name(rgb: Rgb, thread: Option<&sf_core::threads::Thread>) -> String {
    match thread {
        Some(t) if !t.name.is_empty() || !t.code.is_empty() => {
            let words: Vec<&str> = [t.chart.as_str(), t.code.as_str(), t.name.as_str()].into_iter().filter(|w| !w.is_empty()).collect();
            format!("{} ({})", words.join(" "), rgb.to_hex())
        }
        _ => rgb.to_hex(),
    }
}

/// Backstitch line `l` (grid corners) cut to the corners from `lo` to `hi` (Liang–Barsky), if any
/// of it is there.
fn clip(l: &[u32; 4], lo: (f64, f64), hi: (f64, f64)) -> Option<((f64, f64), (f64, f64))> {
    let (x0, y0, x1, y1) = (f64::from(l[0]), f64::from(l[1]), f64::from(l[2]), f64::from(l[3]));
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, x0 - lo.0), (dx, hi.0 - x0), (-dy, y0 - lo.1), (dy, hi.1 - y0)] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            t0 = t0.max(t);
        } else {
            t1 = t1.min(t);
        }
    }
    (t1 - t0 > 1e-9).then_some(((x0 + t0 * dx, y0 + t0 * dy), (x0 + t1 * dx, y0 + t1 * dy)))
}

fn chart(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.crossStitch.chart";
    let group = chosen(s, p).ok_or_else(|| bad(CMD, "select a cross-stitch pattern"))?;
    let (pattern, title) = {
        let d = &s.doc()?.doc;
        let pattern = Pattern::read(d, group).ok_or_else(|| bad(CMD, "that cross-stitch pattern can't be read"))?;
        (pattern, super::fileio::file_stem(&d.title))
    };
    let (pw, ph) = match str_param(p, "page").unwrap_or("a4") {
        "a4" | "A4" => SHEET,
        "letter" | "Letter" => (612.0, 792.0),
        _ => return Err(bad(CMD, "`page` is a4 or letter")),
    };
    let sq = match p.get("square") {
        None | Some(Value::Null) => SQUARE.0,
        Some(v) => v.as_f64().filter(|v| (SQUARE.1..=SQUARE.2).contains(v)).ok_or_else(|| bad(CMD, "`square` is 6 to 24 points"))?,
    };
    let in_color = bool_or(p, "colors", true);
    let counts = pattern.counts();
    // the colours used, in sewing order; each its symbol
    let used: Vec<usize> = (0..pattern.colors.len()).filter(|k| counts.get(*k).copied().unwrap_or(0) > 0).collect();
    // the backstitch colours with lines
    let backs: Vec<usize> = (0..pattern.backs.len()).filter(|b| pattern.backs.get(*b).is_some_and(|b| !b.3.is_empty())).collect();
    if used.is_empty() && backs.is_empty() {
        return Err(bad(CMD, "the pattern has no squares yet: paint some first"));
    }
    // the key's rows: the crosses' colours, then the backstitch
    let rows: Vec<Row> = used.iter().map(|k| Row::Cross(*k)).chain(backs.iter().map(|b| Row::Back(*b))).collect();
    let symbol_of = |k: usize| used.iter().position(|u| *u == k).unwrap_or(0);
    // squares a page holds, and the pages across and down
    let across = (((pw - MARGIN - (MARGIN + LABELS)) / sq).floor() as u32).max(1);
    let down = (((ph - MARGIN - TOP) / sq).floor() as u32).max(1);
    let (pages_x, pages_y) = (pattern.cols.div_ceil(across), pattern.rows.div_ceil(down));
    let chart_pages = pages_x as usize * pages_y as usize;
    // key rows a page holds
    let key_rows = (((ph - MARGIN - 110.0) / 18.0).floor() as usize).max(1);
    // a chart of one page has its key under it when there is room
    let under_chart = (chart_pages == 1)
        .then(|| TOP + f64::from(pattern.rows.min(down)) * sq + 44.0)
        .filter(|y| y + 12.0 + rows.len() as f64 * 18.0 <= ph - MARGIN);
    let key_pages = if under_chart.is_some() { 0 } else { rows.len().div_ceil(key_rows) };
    let pages = chart_pages + key_pages;
    if pages > MAX_PAGES {
        return Err(bad(CMD, format!("the chart would take {pages} pages: use larger chart squares or print a smaller pattern")));
    }
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let grey = Color::rgb8(0x66, 0x66, 0x66);
    let light = Color::rgb8(0xa0, 0xa0, 0xa0);
    let mut doc = Document::new(pw, ph);
    doc.title = format!("{title} cross-stitch chart");
    doc.artboards.clear();
    for i in 0..pages {
        let x = i as f64 * (pw + PAGE_GAP);
        doc.artboards.push(Artboard {
            id: i as u32 + 1,
            name: format!("Page {}", i + 1),
            rect: Rect::new(x, 0.0, x + pw, ph),
            show_center_mark: false,
            show_cross_hairs: false,
            locked: false,
            show_video_safe_areas: false,
            video_ruler_pixel_aspect_ratio: 1.0,
            color: None,
        });
    }
    let cm = |cell_mm: f64, n: u32| f64::from(n) * cell_mm / 10.0;
    let summary = format!(
        "{} × {} stitches, {} colours, {}; sewn {:.1} × {:.1} cm ({:.0}-count); on 14-count Aida {:.1} × {:.1} cm",
        pattern.cols,
        pattern.rows,
        used.len(),
        style_name(pattern.style),
        cm(pattern.cell_mm(), pattern.cols),
        cm(pattern.cell_mm(), pattern.rows),
        25.4 / pattern.cell_mm(),
        cm(25.4 / 14.0, pattern.cols),
        cm(25.4 / 14.0, pattern.rows),
    );
    let mut page = 0usize;
    for py in 0..pages_y {
        for px in 0..pages_x {
            let x0 = page as f64 * (pw + PAGE_GAP);
            let (c0, r0) = (px * across, py * down);
            let (c1, r1) = ((c0 + across).min(pattern.cols), (r0 + down).min(pattern.rows));
            sheet_text(&mut doc, x0 + MARGIN, 40.0, &format!("{title}: cross-stitch chart"), 13.0, black)?;
            let where_ = format!("Page {} of {}: columns {}–{}, rows {}–{}. {summary}", page + 1, pages, c0 + 1, c1, r0 + 1, r1);
            sheet_text(&mut doc, x0 + MARGIN, 56.0, &where_, 7.5, grey)?;
            let (gx, gy) = (x0 + MARGIN + LABELS, TOP);
            // the squares: colour blocks, then each colour's symbols
            for &k in &used {
                let under = pattern.colors.get(k).map(|c| c.1);
                let mut blocks = BezPath::new();
                let mut filled = BezPath::new();
                let mut stroked = BezPath::new();
                for r in r0..r1 {
                    for c in c0..c1 {
                        if pattern.cells.get((r * pattern.cols + c) as usize).copied().flatten() != Some(k) {
                            continue;
                        }
                        let (x, y) = (gx + f64::from(c - c0) * sq, gy + f64::from(r - r0) * sq);
                        rect_path(&mut blocks, x, y, x + sq, y + sq);
                        let (f, o) = symbol(symbol_of(k), Point::new(x + sq / 2.0, y + sq / 2.0), sq * 0.34);
                        filled.extend(f);
                        stroked.extend(o);
                    }
                }
                let ink = if in_color { ink_on(under) } else { black };
                if in_color && let Some(u) = under {
                    put(&mut doc, &blocks, Paint::solid(color(u)), Paint::None, 0.0)?;
                }
                put(&mut doc, &filled, Paint::solid(ink), Paint::None, 0.0)?;
                put(&mut doc, &stroked, Paint::None, Paint::solid(ink), (sq * 0.07).max(0.4))?;
            }
            // the grid: every square, heavier every ten, numbered every ten
            let (w, h) = (f64::from(c1 - c0) * sq, f64::from(r1 - r0) * sq);
            let mut thin = Vec::new();
            let mut bold = Vec::new();
            for c in c0..=c1 {
                let x = gx + f64::from(c - c0) * sq;
                let line = vec![(x, gy), (x, gy + h)];
                if c % 10 == 0 || c == c1 || c == c0 {
                    bold.push(line)
                } else {
                    thin.push(line)
                }
                if c % 10 == 0 && c > 0 && c > c0 {
                    sheet_text(&mut doc, x - 6.0, gy - 5.0, &c.to_string(), 7.0, grey)?;
                }
            }
            for r in r0..=r1 {
                let y = gy + f64::from(r - r0) * sq;
                let line = vec![(gx, y), (gx + w, y)];
                if r % 10 == 0 || r == r1 || r == r0 {
                    bold.push(line)
                } else {
                    thin.push(line)
                }
                if r % 10 == 0 && r > 0 && r > r0 {
                    sheet_text(&mut doc, gx - LABELS + 1.0, y + 2.5, &r.to_string(), 7.0, grey)?;
                }
            }
            sheet_lines(&mut doc, &thin, light, 0.5)?;
            sheet_lines(&mut doc, &bold, black, 0.9)?;
            // the backstitch over it, each line cut to the page
            for &b in &backs {
                let Some((_, rgb, _, lines)) = pattern.backs.get(b) else { continue };
                let on_page: Vec<Vec<(f64, f64)>> = lines
                    .iter()
                    .filter_map(|l| clip(l, (f64::from(c0), f64::from(r0)), (f64::from(c1), f64::from(r1))))
                    .map(|((ca, ra), (cb, rb))| {
                        let at = |c: f64, r: f64| (gx + (c - f64::from(c0)) * sq, gy + (r - f64::from(r0)) * sq);
                        vec![at(ca, ra), at(cb, rb)]
                    })
                    .collect();
                let ink = if in_color { color(*rgb) } else { black };
                sheet_lines(&mut doc, &on_page, ink, (sq * 0.2).max(1.2))?;
            }
            page += 1;
        }
    }
    // the key: symbol, colour, thread, squares
    let chunks: Vec<&[Row]> = if under_chart.is_some() { vec![rows.as_slice()] } else { rows.chunks(key_rows).collect() };
    for (i, chunk) in chunks.into_iter().enumerate() {
        let (x0, top) = match under_chart {
            Some(y) => {
                sheet_text(&mut doc, MARGIN, y - 14.0, "Key", 11.0, black)?;
                (0.0, y)
            }
            None => {
                let x0 = page as f64 * (pw + PAGE_GAP);
                let heading = if key_pages > 1 { format!("Key ({} of {key_pages})", i + 1) } else { "Key".to_string() };
                sheet_text(&mut doc, x0 + MARGIN, 40.0, &format!("{title}: {heading}"), 13.0, black)?;
                sheet_text(&mut doc, x0 + MARGIN, 56.0, &summary, 7.5, grey)?;
                (x0, 84.0)
            }
        };
        sheet_text(&mut doc, x0 + MARGIN, top, "Symbol", 8.0, grey)?;
        sheet_text(&mut doc, x0 + MARGIN + 60.0, top, "Thread", 8.0, grey)?;
        sheet_text(&mut doc, x0 + pw - MARGIN - 80.0, top, "Stitches", 8.0, grey)?;
        for (j, row) in chunk.iter().enumerate() {
            let y = top + 12.0 + j as f64 * 18.0;
            let (bx, by) = (x0 + MARGIN, y);
            let k = match *row {
                Row::Cross(k) => k,
                Row::Back(b) => {
                    let Some((_, rgb, thread, lines)) = pattern.backs.get(b) else { continue };
                    let ink = if in_color { color(*rgb) } else { black };
                    sheet_lines(&mut doc, &[vec![(bx, by + 7.0), (bx + 50.0, by + 7.0)]], ink, 2.0)?;
                    sheet_text(&mut doc, bx + 60.0, by + 10.5, &format!("Backstitch: {}", thread_name(*rgb, thread.as_ref())), 9.0, black)?;
                    sheet_text(&mut doc, x0 + pw - MARGIN - 80.0, by + 10.5, &format!("{} lines", lines.len()), 9.0, black)?;
                    continue;
                }
            };
            let (_, rgb, thread) = match pattern.colors.get(k) {
                Some(c) => c,
                None => continue,
            };
            let box_ = 14.0;
            let mut b = BezPath::new();
            rect_path(&mut b, bx, by, bx + box_, by + box_);
            let under = in_color.then_some(*rgb);
            put(&mut doc, &b, under.map_or(Paint::None, |u| Paint::solid(color(u))), Paint::solid(black), 0.5)?;
            let (f, o) = symbol(symbol_of(k), Point::new(bx + box_ / 2.0, by + box_ / 2.0), box_ * 0.34);
            let ink = if in_color { ink_on(under) } else { black };
            put(&mut doc, &f, Paint::solid(ink), Paint::None, 0.0)?;
            put(&mut doc, &o, Paint::None, Paint::solid(ink), 0.8)?;
            // the colour swatch beside it, so the colour shows even printed without colour blocks
            let mut sw = BezPath::new();
            rect_path(&mut sw, bx + 22.0, by, bx + 22.0 + 28.0, by + box_);
            put(&mut doc, &sw, Paint::solid(color(*rgb)), Paint::solid(light), 0.4)?;
            sheet_text(&mut doc, bx + 60.0, by + 10.5, &thread_name(*rgb, thread.as_ref()), 9.0, black)?;
            let n = counts.get(k).copied().unwrap_or(0);
            sheet_text(&mut doc, x0 + pw - MARGIN - 80.0, by + 10.5, &n.to_string(), 9.0, black)?;
        }
        if under_chart.is_none() {
            page += 1;
        }
    }
    s.add_document(doc, None);
    let lines: usize = backs.iter().filter_map(|b| pattern.backs.get(*b)).map(|b| b.3.len()).sum();
    Ok(json!({ "pages": pages, "colors": used.len(), "squares": counts.iter().sum::<usize>(), "backstitch": lines }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backstitch_is_cut_to_each_page() {
        assert_eq!(clip(&[0, 0, 10, 0], (0.0, 0.0), (5.0, 5.0)), Some(((0.0, 0.0), (5.0, 0.0))));
        assert_eq!(clip(&[0, 0, 10, 10], (5.0, 0.0), (10.0, 10.0)), Some(((5.0, 5.0), (10.0, 10.0))));
        assert_eq!(clip(&[0, 8, 4, 8], (5.0, 0.0), (10.0, 10.0)), None);
        assert_eq!(clip(&[5, 0, 5, 3], (5.0, 0.0), (10.0, 10.0)), Some(((5.0, 0.0), (5.0, 3.0))), "along a page's edge: on both pages");
    }

    #[test]
    fn every_colour_of_a_pattern_has_its_own_symbol() {
        let c = Point::new(0.0, 0.0);
        let mut seen = Vec::new();
        for k in 0..64 {
            let (f, o) = symbol(k, c, 5.0);
            assert!(!(f.elements().is_empty() && o.elements().is_empty()), "symbol {k} draws something");
            let b = f.bounding_box().union(o.bounding_box());
            assert!(b.x0 >= -5.01 && b.y0 >= -5.01 && b.x1 <= 5.01 && b.y1 <= 5.01, "symbol {k} stays in its square: {b:?}");
            let key = format!("{:?}|{:?}", f.elements(), o.elements());
            assert!(!seen.contains(&key), "symbol {k} is like another");
            seen.push(key);
        }
    }

    #[test]
    fn a_pattern_prints_as_chart_pages_and_a_key() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        // 100 × 30 squares: two pages across at 10 pt squares on A4 (50 a page), one down
        s.execute("embroidery.crossStitch.new", &json!({"cols": 100, "rows": 30, "cell": 2, "at": [10, 10]})).unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[0, 0], [1, 0], [60, 5]], "color": "#cc0000"})).unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[2, 0], [99, 29]], "color": "#002288"})).unwrap();
        // backstitch: drawn over the squares, listed in the key with its lines
        s.execute("embroidery.crossStitch.backstitch", &json!({"from": [0, 0], "to": [90, 20], "color": "#000000"})).unwrap();
        let docs = s.documents().len();
        let r = s.execute("embroidery.crossStitch.chart", &json!({})).unwrap();
        assert_eq!(r, json!({"pages": 3, "colors": 2, "squares": 5, "backstitch": 1}));
        assert_eq!(s.documents().len(), docs + 1, "the chart opens as a document of its own");
        let d = &s.doc().unwrap().doc;
        assert_eq!(d.artboards.len(), 3);
        assert!(d.artboards.iter().all(|a| (a.rect.width() - SHEET.0).abs() < 1e-9 && (a.rect.height() - SHEET.1).abs() < 1e-9));
        let texts: Vec<String> = d
            .layers
            .iter()
            .flat_map(|l| l.children().cloned().unwrap_or_default())
            .filter_map(|n| match &n.kind {
                vectorcraft_doc::NodeKind::Text(t) => Some(t.plain_text()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|t| t.starts_with("Page 1 of 3: columns 1–")), "{texts:?}");
        assert!(texts.iter().any(|t| t.ends_with("Key")), "{texts:?}");
        assert!(texts.iter().any(|t| t == "#cc0000") && texts.iter().any(|t| t == "3"), "the key: each thread and its squares: {texts:?}");
        assert!(texts.iter().any(|t| t == "Backstitch: #000000") && texts.iter().any(|t| t == "1 lines"), "{texts:?}");
        // a small pattern: one page, its key under the chart
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        s.execute("embroidery.crossStitch.new", &json!({"cols": 20, "rows": 20})).unwrap();
        s.execute("embroidery.crossStitch.paint", &json!({"cells": [[0, 0]], "color": "#cc0000"})).unwrap();
        assert_eq!(s.execute("embroidery.crossStitch.chart", &json!({})).unwrap()["pages"], 1);
        // an empty pattern has nothing to chart; a pattern is needed
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        assert!(s.execute("embroidery.crossStitch.chart", &json!({})).is_err());
        s.execute("embroidery.crossStitch.new", &json!({"cols": 5, "rows": 5})).unwrap();
        assert!(s.execute("embroidery.crossStitch.chart", &json!({})).is_err(), "no squares yet");
        assert!(s.execute("embroidery.crossStitch.chart", &json!({"square": 2})).is_err());
    }
}
