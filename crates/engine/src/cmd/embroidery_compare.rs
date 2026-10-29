//! Compare Designs (CMP-01): two designs side by side, each a machine file or the design open
//! now, with the same figures for both (stitches, trims, jumps, colour changes, stops, size,
//! thread, time, stitch lengths) and how much each differs. For checking a design against
//! another made from the same art, in this app or elsewhere, before sewing both.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, bad, cmd};

const CMD: &str = "embroidery.compare";

/// Shorter stitches than this (mm) are counted as short: they can break threads and needles.
const SHORT_MM: f64 = 1.0;
/// Longer stitches than this (mm) are counted as long: they can snag.
const LONG_MM: f64 = 7.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        query "embroidery.compare",
        "Compare Designs",
        [],
        None,
        "{a, b: each \"design\" (the design open now; {path}: a machine file; or {dataBase64, name}: its bytes and file name)} two designs side by side → {a, b: {name, format, stitches, jumps, trims, colorChanges, stops, widthMm, heightMm, threadM, bobbinM, seconds, time, maxStitchMm, averageStitchMm, shortStitches (under 1 mm), longStitches (over 7 mm), threads: [\"#rrggbb\"], preview}, differences: [{key, a, b, percent}] (b against a)}",
        has_doc_or_none,
        compare
    )]
}

/// Enabled with or without a document: two files need none.
fn has_doc_or_none(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

/// One side's pattern and name.
fn side(s: &Session, v: Option<&Value>, which: &str) -> Result<(sf_core::Pattern, String, Option<String>)> {
    match v {
        Some(Value::String(k)) if k == "design" => {
            let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no design is open"))?;
            if plan.is_empty() {
                return Err(bad(CMD, "the design open now has no stitches yet"));
            }
            let title = s.doc().map(|d| d.doc.title.clone()).unwrap_or_default();
            Ok((plan.pattern().clone(), if title.is_empty() { "This design".into() } else { title }, None))
        }
        Some(Value::Object(o)) => {
            if let Some(path) = o.get("path").and_then(Value::as_str) {
                let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                let p = super::embroidery_library::read_pattern(path, bytes).map_err(|e| bad(CMD, format!("{path}: {e}")))?;
                let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let ext = std::path::Path::new(path).extension().map(|x| x.to_string_lossy().to_ascii_lowercase());
                return Ok((p, name, ext));
            }
            let name = o.get("name").and_then(Value::as_str).unwrap_or_default().to_string();
            let data = o
                .get("dataBase64")
                .and_then(Value::as_str)
                .ok_or_else(|| bad(CMD, format!("`{which}` needs a `path` or `dataBase64` and `name`")))?;
            let bytes = vectorcraft_format::base64_decode(data).ok_or_else(|| bad(CMD, format!("`{which}.dataBase64` isn't base64")))?;
            if bytes.len() as u64 > 64 << 20 {
                return Err(bad(CMD, format!("`{which}` is larger than 64 MB")));
            }
            let ext = std::path::Path::new(&name).extension().map(|x| x.to_string_lossy().to_ascii_lowercase());
            let (format, p) = sf_formats::read_any(&bytes, ext.as_deref()).map_err(|e| bad(CMD, format!("{name}: {e}")))?;
            Ok((p, name, Some(format.extension().to_string())))
        }
        _ => Err(bad(CMD, format!("`{which}` is \"design\", {{path}} or {{dataBase64, name}}"))),
    }
}

/// A side's figures.
fn figures(p: &sf_core::Pattern, name: &str, format: Option<&str>) -> Value {
    let st = sf_core::stats::pattern_stats(p, &sf_core::model::DesignSettings::default());
    // stitch lengths: from one needle point to the next while sewing
    let (mut n, mut total, mut short, mut long) = (0usize, 0.0f64, 0usize, 0usize);
    let mut prev: Option<sf_core::Point> = None;
    for r in &p.stitches {
        match r.cmd {
            sf_core::pattern::Cmd::Stitch => {
                if let Some(a) = prev {
                    let d = a.distance(r.pos());
                    if d > 1e-6 {
                        n += 1;
                        total += d;
                        short += usize::from(d < SHORT_MM);
                        long += usize::from(d > LONG_MM);
                    }
                }
                prev = Some(r.pos());
            }
            sf_core::pattern::Cmd::Jump => prev = Some(r.pos()),
            _ => prev = None,
        }
    }
    let r1 = |v: f64| (v * 10.0).round() / 10.0;
    let r2 = |v: f64| (v * 100.0).round() / 100.0;
    json!({
        "name": name,
        "format": format,
        "stitches": st.stitches,
        "jumps": st.jumps,
        "trims": st.trims,
        "colorChanges": st.color_changes,
        "stops": st.stops,
        "widthMm": r1(st.width_mm),
        "heightMm": r1(st.height_mm),
        "threadM": r1(st.thread_m),
        "bobbinM": r1(st.bobbin_m),
        "seconds": st.seconds.round(),
        "time": st.time_string(),
        "maxStitchMm": r2(st.max_stitch_mm),
        "averageStitchMm": r2(if n > 0 { total / n as f64 } else { 0.0 }),
        "shortStitches": short,
        "longStitches": long,
        "threads": st.colors.iter().map(|c| c.thread.color.to_hex()).collect::<Vec<_>>(),
        "preview": super::embroidery_library::preview_runs(p),
    })
}

/// The figures compared, in the order shown.
pub const KEYS: [&str; 14] = [
    "stitches",
    "trims",
    "jumps",
    "colorChanges",
    "stops",
    "widthMm",
    "heightMm",
    "threadM",
    "bobbinM",
    "seconds",
    "averageStitchMm",
    "maxStitchMm",
    "shortStitches",
    "longStitches",
];

fn compare(s: &mut Session, p: &Value) -> Result<Value> {
    let (pa, na, fa) = side(s, p.get("a"), "a")?;
    let (pb, nb, fb) = side(s, p.get("b"), "b")?;
    let a = figures(&pa, &na, fa.as_deref());
    let b = figures(&pb, &nb, fb.as_deref());
    let differences: Vec<Value> = KEYS
        .iter()
        .map(|k| {
            let (x, y) = (a[*k].as_f64().unwrap_or(0.0), b[*k].as_f64().unwrap_or(0.0));
            let percent = if x.abs() > 1e-9 { Some(((y - x) / x * 1000.0).round() / 10.0) } else { None };
            json!({ "key": k, "a": a[*k], "b": b[*k], "percent": percent })
        })
        .collect();
    Ok(json!({ "a": a, "b": b, "differences": differences }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A design compared with itself, with a file of it, and with a denser one.
    #[test]
    fn designs_compare_side_by_side() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 150, "height": 100})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        let out = s.execute("embroidery.export", &json!({"format": "dst"})).unwrap();
        let file = json!({"dataBase64": out["dataBase64"], "name": "same.dst"});
        let r = s.execute("embroidery.compare", &json!({"a": "design", "b": file})).unwrap();
        let (a, b) = (&r["a"], &r["b"]);
        assert_eq!(b["format"], "dst");
        // the file is the design: about the same stitches (the file splits nothing here)
        let (sa, sb) = (a["stitches"].as_f64().unwrap(), b["stitches"].as_f64().unwrap());
        assert!((sa - sb).abs() <= sa * 0.02 + 2.0, "{sa} {sb}");
        assert!(a["preview"].as_array().is_some_and(|p| !p.is_empty()));
        // denser: more stitches, shown as a positive difference
        s.execute("embroidery.setParams", &json!({"params": {"spacing": 0.25}})).unwrap();
        let r = s.execute("embroidery.compare", &json!({"a": file, "b": "design"})).unwrap();
        let d = r["differences"].as_array().unwrap().iter().find(|d| d["key"] == "stitches").unwrap().clone();
        assert!(d["percent"].as_f64().unwrap() > 20.0, "{d}");
        for bad in [json!({"a": "design"}), json!({"a": 3, "b": "design"}), json!({"a": {"dataBase64": "!!", "name": "x.dst"}, "b": "design"})] {
            assert!(s.execute("embroidery.compare", &bad).is_err(), "{bad}");
        }
    }
}
