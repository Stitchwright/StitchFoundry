//! Cut files and templates (MDO-01, NEW-29): the rhinestones' template (a hole a little larger
//! than each stone, to cut in template material and brush the stones into) and the appliqué
//! pieces' outlines (to cut the fabric, by hand or on a cutter), as SVG or DXF in millimetres,
//! mirrored for fabric cut from the back if asked; or opened as a document to print life size.

use std::fmt::Write as _;

use serde_json::{Value, json};
use sf_core::decor::DecorKind;
use sf_core::geom::{FLATTEN_TOLERANCE, Point};
use sf_core::model::ObjectKind;
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// How much larger than its stone a template hole is (mm), the trade's usual allowance.
const HOLE_ALLOWANCE: f64 = 0.3;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.cutFile",
        "Cut File",
        ["Embroidery"],
        None,
        "{what?: stones|applique|all (all), format?: svg|dxf (svg), mirror?: bool (false: as seen from the front; true for fabric cut from its back), path?: a file to write (else the text is returned), open?: bool (open it as a new document, life size, to print)} a cut file: each rhinestone a hole 0.3 mm larger than the stone, each appliqué piece its outline, in millimetres → {text?, path?, holes, pieces, stones: [{size, color, count}], mirrored}",
        has_doc,
        cut_file
    )]
}

/// What a cut file holds (mm): stone holes (middle, diameter, colour) and piece outlines.
struct Cuts {
    holes: Vec<(Point, f64, String)>,
    pieces: Vec<Vec<Point>>,
}

fn gather(s: &Session, stones: bool, applique: bool) -> Result<Cuts> {
    let plan = s.embroidery_plan().ok_or_else(|| bad("embroidery.cutFile", "no document"))?;
    let mut c = Cuts { holes: Vec::new(), pieces: Vec::new() };
    for obj in &plan.design.objects {
        match &obj.kind {
            ObjectKind::Decor(d) if stones && d.params.kind == DecorKind::Stone => {
                let color = d.params.color.or_else(|| plan.design.palette.get(obj.color).map(|t| t.color)).map(|c| c.to_hex()).unwrap_or_default();
                for it in &d.items {
                    c.holes.push((it.at, it.size + HOLE_ALLOWANCE, color.clone()));
                }
            }
            ObjectKind::Applique(a) if applique => {
                let poly = a.shape.polygon(FLATTEN_TOLERANCE);
                for ring in poly.rings() {
                    if ring.len() >= 3 {
                        c.pieces.push(ring.clone());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(c)
}

fn svg(c: &Cuts, (x0, y0, w, h): (f64, f64, f64, f64), mirror: bool) -> String {
    let mut out = String::new();
    let _ = write!(out, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w:.2}mm\" height=\"{h:.2}mm\" viewBox=\"0 0 {w:.3} {h:.3}\">");
    let fx = |x: f64| if mirror { w - (x - x0) } else { x - x0 };
    for ring in &c.pieces {
        let mut d = String::new();
        for (i, p) in ring.iter().enumerate() {
            let _ = write!(d, "{}{:.3} {:.3} ", if i == 0 { "M " } else { "L " }, fx(p.x), p.y - y0);
        }
        let _ = write!(out, "<path d=\"{d}Z\" fill=\"none\" stroke=\"#000000\" stroke-width=\"0.2\"/>");
    }
    for (p, dia, _) in &c.holes {
        let _ = write!(
            out,
            "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"none\" stroke=\"#000000\" stroke-width=\"0.1\"/>",
            fx(p.x),
            p.y - y0,
            dia / 2.0
        );
    }
    out.push_str("</svg>");
    out
}

/// A DXF (AutoCAD R12 entities: circles and lines) in millimetres, y up.
fn dxf(c: &Cuts, (x0, y0, w, h): (f64, f64, f64, f64), mirror: bool) -> String {
    let fx = |x: f64| if mirror { w - (x - x0) } else { x - x0 };
    let fy = |y: f64| h - (y - y0);
    let mut out = String::from("0\nSECTION\n2\nENTITIES\n");
    for ring in &c.pieces {
        for (a, b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let _ = write!(out, "0\nLINE\n8\nPIECES\n10\n{:.3}\n20\n{:.3}\n30\n0\n11\n{:.3}\n21\n{:.3}\n31\n0\n", fx(a.x), fy(a.y), fx(b.x), fy(b.y));
        }
    }
    for (p, dia, _) in &c.holes {
        let _ = write!(out, "0\nCIRCLE\n8\nSTONES\n10\n{:.3}\n20\n{:.3}\n30\n0\n40\n{:.3}\n", fx(p.x), fy(p.y), dia / 2.0);
    }
    out.push_str("0\nENDSEC\n0\nEOF\n");
    out
}

fn cut_file(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.cutFile";
    let what = str_param(p, "what").unwrap_or("all");
    let (stones, applique) = match what {
        "stones" => (true, false),
        "applique" => (false, true),
        "all" => (true, true),
        _ => return Err(bad(CMD, "`what` is stones, applique or all")),
    };
    let format = str_param(p, "format").unwrap_or("svg");
    if !["svg", "dxf"].contains(&format) {
        return Err(bad(CMD, "`format` is svg or dxf"));
    }
    let mirror = bool_or(p, "mirror", false);
    let c = gather(s, stones, applique)?;
    if c.holes.is_empty() && c.pieces.is_empty() {
        return Err(bad(CMD, "nothing to cut: the design has no rhinestones or appliqué pieces"));
    }
    // the box round everything, 5 mm clear
    let pts = c.pieces.iter().flatten().copied().chain(c.holes.iter().flat_map(|(q, d, _)| [*q - Point::new(*d, *d), *q + Point::new(*d, *d)]));
    let (mut lo, mut hi) = (Point::new(f64::INFINITY, f64::INFINITY), Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for q in pts {
        lo = lo.min(q);
        hi = hi.max(q);
    }
    if !(lo.is_finite() && hi.is_finite()) {
        return Err(bad(CMD, "nothing to cut"));
    }
    let frame = (lo.x - 5.0, lo.y - 5.0, hi.x - lo.x + 10.0, hi.y - lo.y + 10.0);
    let text = if format == "svg" { svg(&c, frame, mirror) } else { dxf(&c, frame, mirror) };
    // stones by size and colour, to order and to set
    let mut stones_by: std::collections::BTreeMap<(String, String), usize> = std::collections::BTreeMap::new();
    for (_, dia, color) in &c.holes {
        *stones_by.entry((format!("{:.1}", dia - HOLE_ALLOWANCE), color.clone())).or_default() += 1;
    }
    let stones_json: Vec<Value> =
        stones_by.into_iter().map(|((size, color), n)| json!({ "size": size.parse::<f64>().unwrap_or(0.0), "color": color, "count": n })).collect();
    let mut out = json!({ "holes": c.holes.len(), "pieces": c.pieces.len(), "stones": stones_json, "mirrored": mirror });
    if bool_or(p, "open", false) {
        // a new document as large as the cuts, the template placed life size, to print
        let (_, _, w, h) = frame;
        let title = super::fileio::file_stem(&s.doc()?.doc.title);
        s.execute("file.new", &json!({ "width": w * PT_PER_MM, "height": h * PT_PER_MM }))?;
        let svg_text = svg(&c, frame, mirror);
        s.execute(
            "file.place",
            &json!({ "name": format!("{title} cut.svg"), "dataBase64": vectorcraft_format::base64_encode(svg_text.as_bytes()), "rect": [0.0, 0.0, w * PT_PER_MM, h * PT_PER_MM] }),
        )?;
        out["opened"] = json!(true);
    }
    match str_param(p, "path").map(str::trim).filter(|x| !x.is_empty()) {
        Some(path) => {
            super::fileio::write_file(path, text.as_bytes())?;
            out["path"] = json!(path);
        }
        None => out["text"] = json!(text),
    }
    Ok(out)
}
