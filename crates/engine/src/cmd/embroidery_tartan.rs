//! Tartan (BEY-05): a shape filled as a woven check. The sett, a run of coloured stripes, is
//! laid both ways: down the shape (the warp, sewn first as open rows running down) and across
//! it (the weft, sewn on top as open rows running across), each repeated to fill the shape.
//! Where two stripes cross, both threads show, so their colours mix the way a weave's do.

use serde_json::{Value, json};
use sf_core::geom::{Point, Polygon, poly};
use vectorcraft_doc::NodeId;

use super::{CommandSpec, Result, Session, bad, cmd, color_value, has_selection};

const CMD: &str = "embroidery.tartan";
/// Most stripes in one sett.
const MAX_STRIPES: usize = 12;
/// Most stripe bands one shape is cut into.
const MAX_BANDS: usize = 2_000;
/// Rows of each layer this far apart (mm): open, so the other layer shows between them.
const OPEN_SPACING: f64 = 0.8;

/// The sett used when none is given: our own, navy, green, black and gold (mm wide).
const DEFAULT_SETT: [(&str, f64); 5] = [("#1d2f5c", 8.0), ("#2f6b3a", 6.0), ("#151515", 2.0), ("#2f6b3a", 6.0), ("#d4a72c", 1.5)];

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.tartan",
        "Tartan Fill",
        ["Embroidery"],
        None,
        "{ids?, sett?: [{color: \"#rrggbb\", width: mm (0.5 to 50)}, …] (1 to 12 stripes; default our own navy, green, black and gold), mirror?: bool (true: the sett runs there and back, as most tartans do), scale?: (1; 0.25 to 4)} fill each selected closed shape with a tartan: the sett's stripes down the shape (sewn first, as open rows running down) and across it (on top, as open rows running across), repeated to fill it, one fill per colour each way so the colours mix where stripes cross; they take the shape's place in a group → {id, colors, objects}",
        has_selection,
        tartan
    )]
}

/// The sett as (colour, width mm): the given or default stripes, scaled, run there and back
/// when mirrored (the end stripes not doubled).
fn sett(p: &Value) -> Result<Vec<(String, f64)>> {
    let scale = match p.get("scale") {
        None | Some(Value::Null) => 1.0,
        Some(v) => v.as_f64().filter(|v| (0.25..=4.0).contains(v)).ok_or_else(|| bad(CMD, "`scale` is 0.25 to 4"))?,
    };
    let mut stripes: Vec<(String, f64)> = match p.get("sett") {
        None | Some(Value::Null) => DEFAULT_SETT.iter().map(|(c, w)| (c.to_string(), *w)).collect(),
        Some(Value::Array(list)) if (1..=MAX_STRIPES).contains(&list.len()) => list
            .iter()
            .map(|st| {
                let c = st.get("color").and_then(color_value).ok_or_else(|| bad(CMD, format!("a stripe's `color` is \"#rrggbb\": {st}")))?;
                let w = st
                    .get("width")
                    .and_then(Value::as_f64)
                    .filter(|w| (0.5..=50.0).contains(w))
                    .ok_or_else(|| bad(CMD, format!("a stripe's `width` is 0.5 to 50 mm: {st}")))?;
                let v = c.to_rgba8(1.0);
                Ok((format!("#{:02x}{:02x}{:02x}", v[0], v[1], v[2]), w))
            })
            .collect::<Result<_>>()?,
        Some(_) => return Err(bad(CMD, format!("`sett` is a list of 1 to {MAX_STRIPES} stripes {{color, width}}"))),
    };
    let mirror = match p.get("mirror") {
        None | Some(Value::Null) => true,
        Some(v) => v.as_bool().ok_or_else(|| bad(CMD, "`mirror` is true or false"))?,
    };
    if mirror && stripes.len() > 2 {
        let back: Vec<(String, f64)> = stripes.iter().skip(1).rev().skip(1).cloned().collect();
        stripes.extend(back);
    }
    Ok(stripes.into_iter().map(|(c, w)| (c, w * scale)).collect())
}

/// The bands of one colour across `lo..hi`: the sett repeated from `lo`, each stripe of
/// `color` as its span.
fn bands(sett: &[(String, f64)], color: &str, lo: f64, hi: f64) -> Vec<(f64, f64)> {
    let total: f64 = sett.iter().map(|s| s.1).sum();
    let mut out = Vec::new();
    if !(total > 0.0 && lo.is_finite() && hi.is_finite()) {
        return out;
    }
    let mut x = lo;
    'outer: while x < hi {
        for (c, w) in sett {
            if x >= hi {
                break 'outer;
            }
            if c == color {
                out.push((x, x + w));
                if out.len() >= MAX_BANDS {
                    break 'outer;
                }
            }
            x += w;
        }
    }
    out
}

/// A path's `d` (points) for `polys` (mm): every ring, holes too.
fn path_d(polys: &[Polygon]) -> String {
    let mut d = String::new();
    for ring in polys.iter().flat_map(Polygon::rings) {
        for (i, p) in ring.iter().enumerate() {
            let q = vectorcraft_embroidery::pt(*p);
            d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, q.x, q.y));
        }
        d.push_str("Z ");
    }
    d
}

fn tartan(s: &mut Session, p: &Value) -> Result<Value> {
    let stripes = sett(p)?;
    let mut colors: Vec<String> = Vec::new();
    for (c, _) in &stripes {
        if !colors.contains(c) {
            colors.push(c.clone());
        }
    }
    let d = &s.doc()?.doc;
    let mut shapes: Vec<(NodeId, Vec<Polygon>)> = Vec::new();
    for id in super::embroidery::stitched_targets(s, p)? {
        let Some(node) = d.node(id) else { continue };
        let regions = vectorcraft_embroidery::regions_mm(node);
        if !regions.is_empty() {
            shapes.push((id, regions));
        }
    }
    if shapes.is_empty() {
        return Err(bad(CMD, "select closed shapes to fill with tartan"));
    }
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Tartan Fill".into()));
    }
    let done = (|| -> Result<Value> {
        let mut made = Vec::new();
        for (_, regions) in &shapes {
            let bb = regions.iter().map(Polygon::bbox).reduce(|a, b| a.union(b)).ok_or_else(|| bad(CMD, "an empty shape"))?;
            // the warp (down, sewn first), then the weft (across, on top)
            for warp in [true, false] {
                for color in &colors {
                    let strips: Vec<Polygon> = if warp {
                        bands(&stripes, color, bb.min.x, bb.max.x).into_iter().map(|(a, b)| rect(a, bb.min.y - 1.0, b, bb.max.y + 1.0)).collect()
                    } else {
                        bands(&stripes, color, bb.min.y, bb.max.y).into_iter().map(|(a, b)| rect(bb.min.x - 1.0, a, bb.max.x + 1.0, b)).collect()
                    };
                    let piece = poly::intersection(regions, &strips);
                    if piece.iter().all(|p| p.area().abs() < 0.5) {
                        continue;
                    }
                    let r = s.execute("path.create", &json!({ "d": path_d(&piece) }))?;
                    let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "a stripe wasn't made"))?;
                    s.execute("select.set", &json!({ "ids": [id] }))?;
                    s.execute("path.setFillRule", &json!({ "rule": "evenOdd" }))?;
                    s.execute("embroidery.set", &json!({ "type": "fill" }))?;
                    // rows down for the warp, across for the weft; open, so the other shows
                    let mut params = json!({ "angle": if warp { 90.0 } else { 0.0 }, "spacing": OPEN_SPACING, "auto_direction": false });
                    if !warp {
                        params["underlay"] = json!({ "edge_run": false, "tatami": false });
                    }
                    s.execute("embroidery.setParams", &json!({ "params": params }))?;
                    let name = format!("Tartan {}: {color}", if warp { "warp" } else { "weft" });
                    s.execute("embroidery.setThread", &json!({ "color": color, "name": name, "ids": [id] }))?;
                    made.push(id);
                }
            }
        }
        if made.is_empty() {
            return Err(bad(CMD, "the shapes are too small for the sett"));
        }
        let old: Vec<u64> = shapes.iter().map(|(id, _)| id.0).collect();
        s.execute("edit.clear", &json!({ "ids": old }))?;
        s.execute("select.set", &json!({ "ids": made }))?;
        let g = s.execute("object.group", &json!({}))?;
        Ok(json!({ "id": g["id"], "colors": colors, "objects": made.len() }))
    })();
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    done
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon {
    Polygon::from_ring(vec![Point::new(x0, y0), Point::new(x1, y0), Point::new(x1, y1), Point::new(x0, y1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sett_runs_there_and_back_and_repeats() {
        let s =
            sett(&json!({"sett": [{"color": "#ff0000", "width": 4}, {"color": "#00ff00", "width": 2}, {"color": "#0000ff", "width": 1}]})).unwrap();
        let names: Vec<&str> = s.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(names, vec!["#ff0000", "#00ff00", "#0000ff", "#00ff00"]);
        let plain =
            sett(&json!({"mirror": false, "scale": 2, "sett": [{"color": "#ff0000", "width": 4}, {"color": "#00ff00", "width": 2}]})).unwrap();
        assert_eq!(plain, vec![("#ff0000".to_string(), 8.0), ("#00ff00".to_string(), 4.0)]);
        // green's bands across 0..20 with the 9 mm mirrored sett: at 4, 7, 13 and 16
        let b = bands(&s, "#00ff00", 0.0, 20.0);
        assert_eq!(b, vec![(4.0, 6.0), (7.0, 9.0), (13.0, 15.0), (16.0, 18.0)]);
        for bad in [
            json!({"sett": []}),
            json!({"sett": [{"color": "plaid", "width": 2}]}),
            json!({"sett": [{"color": "#000", "width": 0}]}),
            json!({"scale": 9}),
        ] {
            assert!(sett(&bad).is_err(), "{bad}");
        }
    }
}
