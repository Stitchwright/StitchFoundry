//! Spiro (HV-01): spirograph patterns sewn as lines, a colour a layer. Behaviour from mySewnet's
//! public pages on its Spiro wizard (intricate spiral designs in several colours, sewn as running,
//! double or triple stitch lines, with a size, the number of petals, the shape and the spacing,
//! or made at random; <https://softwarehelp.mysewnet.com/MSW/100/Embroidery/Topics/Welcome_Screen.htm>).
//! The curves are hypotrochoids, the circle-in-a-circle curves of the toy: code and wording are
//! our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::embroidery_lettering::ranged;
use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// Points a layer is drawn with at most.
const MAX_POINTS: usize = 6000;

/// Thread colours the layers take in turn, unless given.
const COLORS: [&str; 6] = ["#1f6fb2", "#c8352e", "#2e8b57", "#7b4fa3", "#d9822b", "#2a9d8f"];

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.spiro",
        "Spiro",
        ["Embroidery"],
        None,
        "{size?: mm across (80; 10–400), petals?: 3–36 (7), shape?: 1–petals−1 (how far round each loop reaches before it turns; 2), spacing?: 0.2–2 (how far the pen is from the small circle's middle; 1: the loops just touch the middle), layers?: 1–6 (copies turned between the petals, a colour each; 2), stitch?: run|double|triple (triple), colors?: [\"#rrggbb\"], random?: bool (petals, shape and spacing chosen at random), seed?: a number (with random: the same pattern each time)} a spirograph pattern in the middle of the hoop, each layer a line of its own colour sewn as a running, double or triple stitch, in a group, selected → {id, petals, shape, spacing, layers}",
        has_doc,
        spiro
    )]
}

/// The greatest common divisor.
fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// One layer of the pattern: a hypotrochoid with `petals` loops, the small circle `shape` /
/// `petals` of the large one (made coprime so it closes once round), the pen `spacing` of the
/// small radius from its middle, scaled to `radius` and turned by `turn`.
fn layer(petals: u32, shape: u32, spacing: f64, radius: f64, turn: f64) -> Vec<(f64, f64)> {
    let (p, q) = (f64::from(petals), f64::from(shape));
    let (big, small) = (1.0, q / p);
    let d = spacing * small;
    // the curve closes after `shape` turns of the small circle round the big one
    let turns = q;
    let n = ((petals as usize) * 80 * shape as usize).clamp(200, MAX_POINTS);
    let pts: Vec<(f64, f64)> = (0..=n)
        .map(|k| {
            let t = k as f64 / n as f64 * turns * std::f64::consts::TAU;
            let x = (big - small) * t.cos() + d * ((big - small) / small * t).cos();
            let y = (big - small) * t.sin() - d * ((big - small) / small * t).sin();
            (x, y)
        })
        .collect();
    let reach = pts.iter().map(|(x, y)| x.hypot(*y)).fold(0.0, f64::max).max(1e-9);
    let (sin, cos) = turn.sin_cos();
    pts.iter().map(|(x, y)| ((x * cos - y * sin) / reach * radius, (x * sin + y * cos) / reach * radius)).collect()
}

/// A small, steady pseudo-random sequence.
fn next(state: &mut u64) -> f64 {
    *state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    (*state >> 11) as f64 / (1u64 << 53) as f64
}

fn spiro(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.spiro";
    let size = ranged(p, "size", 10.0..=400.0, CMD, "`size` is 10 to 400 mm")?.unwrap_or(80.0);
    let int = |key: &str, lo: u64, hi: u64, def: u64, msg: &str| -> Result<u32> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(def as u32),
            Some(v) => v.as_u64().filter(|n| (lo..=hi).contains(n)).map(|n| n as u32).ok_or_else(|| bad(CMD, msg)),
        }
    };
    let mut petals = int("petals", 3, 36, 7, "`petals` is 3 to 36")?;
    let mut shape = int("shape", 1, 35, 2, "`shape` is 1 to petals − 1")?;
    let mut spacing = ranged(p, "spacing", 0.2..=2.0, CMD, "`spacing` is 0.2 to 2")?.unwrap_or(1.0);
    let layers = int("layers", 1, 6, 2, "`layers` is 1 to 6")?;
    let passes = match str_param(p, "stitch").unwrap_or("triple") {
        "run" => 1,
        "double" => 2,
        "triple" => 3,
        _ => return Err(bad(CMD, "`stitch` is run, double or triple")),
    };
    if bool_or(p, "random", false) {
        let mut st = p.get("seed").and_then(Value::as_u64).unwrap_or(1) ^ 0x9e37_79b9_7f4a_7c15;
        petals = 3 + (next(&mut st) * 14.0) as u32;
        shape = 1 + (next(&mut st) * f64::from(petals - 1)) as u32;
        spacing = 0.4 + next(&mut st) * 1.2;
    }
    if shape >= petals {
        return Err(bad(CMD, "`shape` is 1 to petals − 1"));
    }
    // coprime, so each layer closes in one line
    while gcd(petals, shape) != 1 {
        shape = if shape > 1 { shape - 1 } else { 1 };
    }
    let colors: Vec<String> = match p.get("colors").and_then(Value::as_array) {
        Some(list) => {
            let c: Vec<String> = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
            if c.is_empty() || c.iter().any(|h| sf_core::Rgb::from_hex(h).is_none()) {
                return Err(bad(CMD, "`colors` are \"#rrggbb\""));
            }
            c
        }
        None => COLORS.iter().map(|c| c.to_string()).collect(),
    };
    let c = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    let radius = size / 2.0 * PT_PER_MM;
    // its layers, their stitching and the group: one undo step
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Spiro".into()));
    }
    let done = make_layers(s, c, radius, (petals, shape, spacing, layers, passes), &colors);
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let id = done?;
    Ok(json!({ "id": id, "petals": petals, "shape": shape, "spacing": spacing, "layers": layers }))
}

/// The pattern's layers, each a line stitched in its colour, grouped → the group's id.
fn make_layers(
    s: &mut Session,
    c: kurbo::Point,
    radius: f64,
    (petals, shape, spacing, layers, passes): (u32, u32, f64, u32, u32),
    colors: &[String],
) -> Result<u64> {
    const CMD: &str = "embroidery.spiro";
    let mut ids = Vec::new();
    for k in 0..layers {
        // the layers turned evenly between the petals
        let turn = f64::from(k) * std::f64::consts::TAU / f64::from(petals * layers);
        let pts = layer(petals, shape, spacing, radius, turn);
        let mut d = String::with_capacity(pts.len() * 20);
        for (i, (x, y)) in pts.iter().enumerate() {
            d.push_str(&format!("{}{:.2} {:.2} ", if i == 0 { "M" } else { "L" }, c.x + x, c.y + y));
        }
        let id = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(CMD, "a layer wasn't made"))?;
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("embroidery.set", &json!({ "type": "run" }))?;
        s.execute("embroidery.setParams", &json!({ "params": { "passes": passes, "stitch_length": 2.5 } }))?;
        let color = colors.get(k as usize % colors.len()).cloned().unwrap_or_else(|| "#1f6fb2".into());
        s.execute("embroidery.setThread", &json!({ "color": color, "ids": [id], "paint": true }))?;
        ids.push(id);
    }
    s.execute("select.set", &json!({ "ids": ids }))?;
    let g = s.execute("object.group", &json!({}))?;
    Ok(g["id"].as_u64().or_else(|| ids.first().copied()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The curve closes, has `petals` loops round its middle and fills the size asked.
    #[test]
    fn a_layer_closes_with_its_petals_at_its_size() {
        let pts = layer(7, 2, 1.0, 100.0, 0.0);
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        assert!((first.0 - last.0).hypot(first.1 - last.1) < 1e-6, "closed");
        let reach = pts.iter().map(|(x, y)| x.hypot(*y)).fold(0.0, f64::max);
        assert!((reach - 100.0).abs() < 1e-6);
        // the outermost points: one a petal
        let far: Vec<f64> = pts.iter().map(|(x, y)| x.hypot(*y)).collect();
        // round the closed curve (its last point is its first)
        let n = far.len() - 1;
        let peaks = (0..n)
            .filter(|&i| {
                let (a, b, c) = (far[(i + n - 1) % n], far[i], far[(i + 1) % n]);
                b > a && b >= c && b > 99.0
            })
            .count();
        assert_eq!(peaks, 7, "seven petals");
        assert_eq!(gcd(12, 8), 4);
    }

    #[test]
    fn spiro_makes_a_layer_a_colour_in_the_hoop() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let r = s.execute("embroidery.spiro", &json!({"size": 90, "petals": 9, "shape": 4, "layers": 3})).unwrap();
        assert_eq!((r["petals"].as_u64(), r["shape"].as_u64(), r["layers"].as_u64()), (Some(9), Some(4), Some(3)));
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["colors"].as_array().unwrap().len(), 3, "a colour a layer: {info}");
        assert_eq!(info["fits"], true);
        assert!((info["widthMm"].as_f64().unwrap() - 90.0).abs() < 6.0, "{info}");
        // one undo takes it all back
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap_or(0), 0);
        s.execute("edit.redo", &json!({})).unwrap();
        // shape made coprime with the petals; random the same with the same seed
        let r = s.execute("embroidery.spiro", &json!({"petals": 8, "shape": 6})).unwrap();
        assert_eq!(r["shape"], 5);
        let a = s.execute("embroidery.spiro", &json!({"random": true, "seed": 42})).unwrap();
        let b = s.execute("embroidery.spiro", &json!({"random": true, "seed": 42})).unwrap();
        assert_eq!((a["petals"].clone(), a["shape"].clone()), (b["petals"].clone(), b["shape"].clone()));
        for bad in
            [json!({"petals": 2}), json!({"petals": 5, "shape": 5}), json!({"stitch": "zigzag"}), json!({"colors": ["red"]}), json!({"size": 1})]
        {
            assert!(s.execute("embroidery.spiro", &bad).is_err(), "{bad}");
        }
    }
}
