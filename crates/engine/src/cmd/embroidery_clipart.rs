//! Embroidery clipart (DM-03): simple motifs drawn in code for StitchFoundry (hearts, stars,
//! crowns, paws, leaves, flowers…), placed as flat-colour art and digitized like Design from
//! Prompt's art, in the design chart's threads. Designs of one's own come from the Design Library.

use std::f64::consts::{PI, TAU};
use std::fmt::Write as _;

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, cmd, has_doc, str_param};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.clipart",
            "Embroidery Clipart",
            [],
            None,
            "{} the clipart motifs, each with its SVG (100 × 100) → {clipart: [{name, svg}]}",
            always,
            list
        ),
        cmd!(
            "embroidery.clipart.insert",
            "Insert Clipart",
            [],
            None,
            "{name, size?: mm (40), at?: [x, y] (the first artboard's centre), color?: \"#rrggbb\" (its own colours)} place a clipart motif digitized, its shapes given stitch types in the design chart's threads (as embroidery.placeArt) → {id, objects, colors}",
            has_doc,
            insert
        ),
    ]
}

/// A path of points joined by straight lines, closed.
fn poly(pts: &[(f64, f64)]) -> String {
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let _ = write!(d, "{}{x:.2} {y:.2} ", if i == 0 { "M " } else { "L " });
    }
    d.push('Z');
    d
}

/// A circle's path.
fn circle(cx: f64, cy: f64, r: f64) -> String {
    format!("M {:.2} {cy:.2} A {r:.2} {r:.2} 0 1 0 {:.2} {cy:.2} A {r:.2} {r:.2} 0 1 0 {:.2} {cy:.2} Z", cx - r, cx + r, cx - r)
}

/// An ellipse turned `deg` about its centre, as a path of points.
fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64, deg: f64) -> String {
    let a = deg.to_radians();
    let pts: Vec<(f64, f64)> = (0..36)
        .map(|k| {
            let t = TAU * f64::from(k) / 36.0;
            let (x, y) = (rx * t.cos(), ry * t.sin());
            (cx + x * a.cos() - y * a.sin(), cy + x * a.sin() + y * a.cos())
        })
        .collect();
    poly(&pts)
}

/// A star of `n` points.
fn star(n: u32, outer: f64, inner: f64) -> String {
    let pts: Vec<(f64, f64)> = (0..2 * n)
        .map(|k| {
            let r = if k % 2 == 0 { outer } else { inner };
            let a = -PI / 2.0 + PI * f64::from(k) / f64::from(n);
            (50.0 + r * a.cos(), 50.0 + r * a.sin())
        })
        .collect();
    poly(&pts)
}

fn svg(parts: &[(&str, String)]) -> String {
    let mut s = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" width=\"100\" height=\"100\">");
    for (fill, d) in parts {
        let _ = write!(s, "<path fill=\"{fill}\" d=\"{d}\"/>");
    }
    s.push_str("</svg>");
    s
}

/// The motifs: a name and its SVG.
fn motifs() -> Vec<(&'static str, String)> {
    let sun_rays: Vec<(f64, f64)> = (0..24)
        .map(|k| {
            let r = if k % 2 == 0 { 48.0 } else { 30.0 };
            let a = PI * f64::from(k) / 12.0;
            (50.0 + r * a.cos(), 50.0 + r * a.sin())
        })
        .collect();
    let petals: String = (0..5)
        .map(|k| {
            ellipse(
                50.0 + 22.0 * (TAU * f64::from(k) / 5.0 - PI / 2.0).cos(),
                50.0 + 22.0 * (TAU * f64::from(k) / 5.0 - PI / 2.0).sin(),
                18.0,
                11.0,
                72.0 * f64::from(k) + 90.0,
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let flake: String = (0..6)
        .map(|k| {
            let a = PI * f64::from(k) / 3.0;
            let (c, s) = (a.cos(), a.sin());
            let p = |x: f64, y: f64| (50.0 + x * c - y * s, 50.0 + x * s + y * c);
            poly(&[p(0.0, -3.0), p(44.0, -3.0), p(44.0, 3.0), p(0.0, 3.0)])
        })
        .collect::<Vec<_>>()
        .join(" ");
    vec![
        ("Heart", svg(&[("#d62828", "M 50 88 C 22 66 4 48 10 28 C 16 8 42 8 50 28 C 58 8 84 8 90 28 C 96 48 78 66 50 88 Z".into())])),
        ("Star", svg(&[("#f4a261", star(5, 48.0, 20.0))])),
        ("Sparkle", svg(&[("#e9c46a", star(4, 46.0, 12.0))])),
        (
            "Crown",
            svg(&[
                ("#e9c46a", poly(&[(8.0, 78.0), (12.0, 28.0), (32.0, 52.0), (50.0, 18.0), (68.0, 52.0), (88.0, 28.0), (92.0, 78.0)])),
                ("#d62828", circle(50.0, 64.0, 7.0)),
            ]),
        ),
        (
            "Paw",
            svg(&[(
                "#5a3e2b",
                format!(
                    "{} {} {} {} {}",
                    ellipse(50.0, 66.0, 22.0, 18.0, 0.0),
                    ellipse(24.0, 40.0, 8.0, 11.0, -20.0),
                    ellipse(40.0, 26.0, 8.0, 11.0, -8.0),
                    ellipse(60.0, 26.0, 8.0, 11.0, 8.0),
                    ellipse(76.0, 40.0, 8.0, 11.0, 20.0)
                ),
            )]),
        ),
        (
            "Leaf",
            svg(&[
                ("#2a9d8f", "M 50 95 C 14 70 10 30 50 5 C 90 30 86 70 50 95 Z".into()),
                ("#1b6b61", poly(&[(48.5, 92.0), (48.5, 14.0), (51.5, 14.0), (51.5, 92.0)])),
            ]),
        ),
        ("Flower", svg(&[("#e76f51", petals), ("#f4d35e", circle(50.0, 50.0, 12.0))])),
        ("Sun", svg(&[("#f4a261", poly(&sun_rays)), ("#f4d35e", circle(50.0, 50.0, 22.0))])),
        ("Moon", svg(&[("#e9c46a", "M 62 8 A 42 42 0 1 0 92 70 A 34 34 0 1 1 62 8 Z".into())])),
        ("Lightning", svg(&[("#f4d35e", poly(&[(58.0, 4.0), (22.0, 56.0), (46.0, 56.0), (36.0, 96.0), (80.0, 40.0), (56.0, 40.0), (72.0, 4.0)]))])),
        (
            "Shield",
            svg(&[
                ("#1d3557", "M 50 4 L 90 18 L 88 52 C 86 74 70 88 50 96 C 30 88 14 74 12 52 L 10 18 Z".into()),
                ("#a8dadc", "M 50 14 L 80 25 L 78 52 C 76 68 64 80 50 86 C 36 80 24 68 22 52 L 20 25 Z".into()),
            ]),
        ),
        ("Snowflake", svg(&[("#457b9d", flake)])),
        (
            "Music note",
            svg(&[(
                "#1a1a1a",
                format!(
                    "{} {}",
                    ellipse(36.0, 78.0, 16.0, 11.0, -20.0),
                    poly(&[(47.0, 76.0), (47.0, 8.0), (78.0, 18.0), (78.0, 30.0), (53.0, 22.0), (53.0, 76.0)])
                ),
            )]),
        ),
        (
            "Anchor",
            svg(&[
                (
                    "#1d3557",
                    format!(
                        "{} {} {}",
                        poly(&[(47.0, 22.0), (53.0, 22.0), (53.0, 84.0), (47.0, 84.0)]),
                        poly(&[(30.0, 30.0), (70.0, 30.0), (70.0, 36.0), (30.0, 36.0)]),
                        "M 14 60 L 22 56 C 26 74 38 82 50 84 C 62 82 74 74 78 56 L 86 60 C 82 82 66 94 50 94 C 34 94 18 82 14 60 Z"
                    ),
                ),
                ("#1d3557", circle(50.0, 14.0, 8.0)),
            ]),
        ),
        ("Check mark", svg(&[("#2a9d8f", poly(&[(8.0, 52.0), (20.0, 40.0), (40.0, 60.0), (80.0, 18.0), (92.0, 30.0), (40.0, 84.0)]))])),
        ("Arrow", svg(&[("#e76f51", poly(&[(6.0, 40.0), (58.0, 40.0), (58.0, 20.0), (94.0, 50.0), (58.0, 80.0), (58.0, 60.0), (6.0, 60.0)]))])),
    ]
}

fn list(_: &mut Session, _: &Value) -> Result<Value> {
    let v: Vec<Value> = motifs().into_iter().map(|(name, svg)| json!({ "name": name, "svg": svg })).collect();
    Ok(json!({ "clipart": v }))
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.clipart.insert";
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad(CMD, "missing `name`: a clipart motif"))?;
    let (title, mut art) =
        motifs().into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).ok_or_else(|| bad(CMD, format!("no clipart motif is named `{name}`")))?;
    if let Some(c) = str_param(p, "color") {
        let c = sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?.to_hex();
        // every part in that colour
        let mut out = String::new();
        let mut rest = art.as_str();
        while let Some(i) = rest.find("fill=\"#") {
            out.push_str(rest.get(..i).unwrap_or_default());
            out.push_str(&format!("fill=\"{c}\""));
            rest = rest.get(i + 14..).unwrap_or_default();
        }
        out.push_str(rest);
        art = out;
    }
    let size = match p.get("size") {
        None | Some(Value::Null) => 40.0,
        Some(v) => v.as_f64().filter(|v| (5.0..=500.0).contains(v)).ok_or_else(|| bad(CMD, "`size` is 5 to 500 mm"))?,
    };
    let mut q = json!({ "svg": art, "size": size, "name": title });
    if let Some(at) = p.get("at").filter(|v| !v.is_null()) {
        q["at"] = at.clone();
    }
    let r = s.execute("embroidery.placeArt", &q)?;
    Ok(json!({ "id": r["id"], "objects": r["objects"], "colors": r["colors"] }))
}
