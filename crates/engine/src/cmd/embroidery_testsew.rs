//! Test sew-outs (U0): designs that sew the same thing several ways side by side, labelled in
//! running stitch, so a sew-out on the real machine and fabric shows which settings look best:
//! satin underlay by column width, lettering by size, fill underlay, and a towel's settings.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, Result, Session, always, bad, cmd, str_param};

const CMD: &str = "embroidery.testSew";
/// The word the lettering tests sew.
const WORD: &str = "Grace";
/// The tests: (id, name, what to look for).
pub const TESTS: [(&str, &str, &str); 4] = [
    (
        "satin",
        "Satin underlay ladder",
        "Columns: satin 1.5, 2, 3, 4, 5 and 6 mm wide. Rows: no underlay, a centre run, an edge run, an edge run and a zigzag. Look for: edges crisp or saw-toothed, fabric showing through, underlay poking out, columns sinking or puckering.",
    ),
    (
        "lettering",
        "Lettering sizes",
        "Left: the automatic underlay. Right: an edge run and a zigzag under every letter. Words 25, 15, 10 and 6 mm tall. Look for: letters crisp and raised, thin parts closed up or broken, underlay showing at the curves, the smallest size readable.",
    ),
    (
        "fill",
        "Fill underlay",
        "Squares, top row: no underlay, an edge run, an open tatami. Bottom row: edge run and tatami (the default), the same denser, a towel's (dense, close underlay). Look for: gaps or fabric showing, edges straight, puckering round the square, rows wavy.",
    ),
    (
        "towel",
        "Towel test",
        "Top: cotton settings. Middle: a towel's (denser, edge run and zigzag under every letter, more pull compensation). Bottom: the towel's with a knockdown under the word. Sew on a towel with a water-soluble topping. Look for: letters sinking into the loops, loops poking through, edges crisp.",
    ),
];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.testSew",
            "Test Sew-Out",
            [],
            None,
            "{test: satin|lettering|fill|towel, hoop?: a hoop's name (My Machine's first that fits 220 × 130 mm), color?: \"#rrggbb\"} a new design that sews the same thing several ways side by side, labelled in running stitch, to sew out on the real machine and fabric and compare: satin underlay by column width, lettering by size, fill underlay, a towel's settings; its notes say what each part is and what to look for → {test, name, look, stitches, hoop}",
            always,
            test_sew
        ),
        cmd!(
            "embroidery.testSew.list",
            "Test Sew-Outs",
            [],
            None,
            "{} the test sew-outs embroidery.testSew makes → {tests: [{id, name, look}]}",
            always,
            |_s, _p| {
                Ok(json!({ "tests": TESTS.iter().map(|(id, name, look)| json!({ "id": id, "name": name, "look": look })).collect::<Vec<_>>() }))
            }
        ),
    ]
}

/// Draws in millimetres from the page's middle (`c`, points).
struct Page {
    c: kurbo::Point,
}

impl Page {
    fn at(&self, x: f64, y: f64) -> (f64, f64) {
        (self.c.x + x * PT_PER_MM, self.c.y + y * PT_PER_MM)
    }

    /// A straight line from `a` to `b` (mm) → its id, selected.
    fn line(&self, s: &mut Session, a: (f64, f64), b: (f64, f64)) -> Result<u64> {
        let (p, q) = (self.at(a.0, a.1), self.at(b.0, b.1));
        let r = s.execute("path.create", &json!({ "d": format!("M{:.3} {:.3} L{:.3} {:.3}", p.0, p.1, q.0, q.1) }))?;
        r["id"].as_u64().ok_or_else(|| bad(CMD, "a line wasn't made"))
    }

    /// A `w` × `h` mm square centred at (`x`, `y`) → its id, selected.
    fn rect(&self, s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> Result<u64> {
        let p = self.at(x - w / 2.0, y - h / 2.0);
        let r = s.execute("shape.rectangle", &json!({ "x": p.0, "y": p.1, "width": w * PT_PER_MM, "height": h * PT_PER_MM }))?;
        r["id"].as_u64().ok_or_else(|| bad(CMD, "a square wasn't made"))
    }

    /// A label in running stitch, its capitals about 4 mm, starting at (`x`, `y`) on its baseline.
    fn label(&self, s: &mut Session, x: f64, y: f64, text: &str) -> Result<()> {
        let p = self.at(x, y);
        let r = s.execute("text.create", &json!({ "x": p.0, "y": p.1, "text": text, "size": 4.0 * PT_PER_MM / 0.7 }))?;
        let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "a label wasn't made"))?;
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("embroidery.set", &json!({ "type": "run" }))?;
        s.execute("embroidery.setParams", &json!({ "params": { "stitch_length": 1.5 } }))?;
        Ok(())
    }

    /// The word `height` mm tall, its middle at (`x`, `y`) → its id, selected.
    fn word(&self, s: &mut Session, x: f64, y: f64, height: f64) -> Result<u64> {
        let r = s.execute("embroidery.addWords", &json!({ "text": WORD, "height": height }))?;
        let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "the word wasn't made"))?;
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("object.move", &json!({ "dx": x * PT_PER_MM, "dy": y * PT_PER_MM }))?;
        Ok(id)
    }
}

/// Satin underlay settings by name: none, centre, edge, edge and zigzag.
fn satin_underlay(kind: &str) -> Value {
    let (center, edge, zz) = match kind {
        "centre" => (true, false, false),
        "edge" => (false, true, false),
        "edge+zz" => (false, true, true),
        _ => (false, false, false),
    };
    json!({ "auto": false, "center_run": center, "edge_run": edge, "zigzag": zz })
}

fn test_sew(s: &mut Session, p: &Value) -> Result<Value> {
    let test = str_param(p, "test").ok_or_else(|| bad(CMD, "missing `test`: satin, lettering, fill or towel"))?;
    let &(id, name, look) =
        TESTS.iter().find(|t| t.0 == test).ok_or_else(|| bad(CMD, format!("no test `{test}`: satin, lettering, fill or towel")))?;
    let color = str_param(p, "color").unwrap_or("#1a3e8c");
    sf_core::Rgb::from_hex(color).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?;
    let (hoop, _, _) = super::embroidery_ith::pick_hoop(s, p, (226.0, 136.0), CMD)?;
    let made = s.execute("embroidery.newDesign", &json!({ "hoop": hoop }))?;
    let c = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    let pg = Page { c };
    match id {
        "satin" => {
            let widths = [1.5, 2.0, 3.0, 4.0, 5.0, 6.0];
            let rows = [("None", "none"), ("Centre", "centre"), ("Edge", "edge"), ("Edge+ZZ", "edge+zz")];
            for (k, w) in widths.iter().enumerate() {
                pg.label(s, -72.0 + 26.0 * k as f64, -58.0, &format!("{w}"))?;
            }
            for (r, (title, kind)) in rows.iter().enumerate() {
                let y = -40.0 + 30.0 * r as f64;
                pg.label(s, -108.0, y + 2.0, title)?;
                for (k, w) in widths.iter().enumerate() {
                    let x = -70.0 + 26.0 * k as f64;
                    pg.line(s, (x, y - 11.0), (x, y + 11.0))?;
                    s.execute("embroidery.set", &json!({ "type": "satin" }))?;
                    s.execute("embroidery.setParams", &json!({ "params": { "width": w, "underlay": satin_underlay(kind) } }))?;
                }
            }
        }
        "lettering" => {
            pg.label(s, -70.0, -58.0, "Auto")?;
            pg.label(s, 38.0, -58.0, "Edge+ZZ")?;
            for (height, y) in [(25.0, -36.0), (15.0, -6.0), (10.0, 16.0), (6.0, 34.0)] {
                pg.word(s, -55.0, y, height)?;
                pg.word(s, 55.0, y, height)?;
                s.execute("embroidery.setParams", &json!({ "params": { "underlay": satin_underlay("edge+zz") } }))?;
                pg.label(s, -110.0, y + 2.0, &format!("{height}"))?;
            }
        }
        "fill" => {
            // (x, y, label, params)
            let squares = [
                (-70.0, -22.0, "None", json!({ "underlay": { "edge_run": false, "tatami": false } })),
                (0.0, -22.0, "Edge", json!({ "underlay": { "edge_run": true, "tatami": false } })),
                (70.0, -22.0, "Tatami", json!({ "underlay": { "edge_run": false, "tatami": true } })),
                (-70.0, 36.0, "Default", json!({})),
                (0.0, 36.0, "Denser", json!({ "spacing": 0.36 })),
                (
                    70.0,
                    36.0,
                    "Towel",
                    json!({ "spacing": 0.36, "pull_comp": 0.35, "underlay": { "edge_run": true, "tatami": true, "spacing": 1.5 } }),
                ),
            ];
            for (x, y, title, params) in squares {
                pg.rect(s, x, y, 30.0, 30.0)?;
                s.execute("embroidery.set", &json!({ "type": "fill" }))?;
                s.execute("embroidery.setParams", &json!({ "params": params }))?;
                pg.label(s, x - 15.0, y + 22.0, title)?;
            }
        }
        _ => {
            let towel = json!({ "spacing": 0.33, "pull_comp": 0.35, "underlay": satin_underlay("edge+zz") });
            pg.label(s, -110.0, -36.0, "Cotton")?;
            pg.word(s, 0.0, -38.0, 25.0)?;
            pg.label(s, -110.0, 2.0, "Towel")?;
            pg.word(s, 0.0, 0.0, 25.0)?;
            s.execute("embroidery.setParams", &json!({ "params": towel.clone() }))?;
            pg.label(s, -110.0, 40.0, "Knock-")?;
            pg.label(s, -110.0, 46.0, "down")?;
            pg.word(s, 0.0, 38.0, 25.0)?;
            s.execute("embroidery.setParams", &json!({ "params": towel }))?;
            s.execute("embroidery.outlineAround", &json!({ "kind": "knockdown", "gap": 2.0, "color": color }))?;
        }
    }
    // everything in the one thread
    s.execute("select.all", &json!({}))?;
    s.execute("embroidery.setThread", &json!({ "color": color, "name": name }))?;
    s.execute("select.none", &json!({}))?;
    s.execute("embroidery.settings", &json!({ "notes": format!("{name}. {look}") }))?;
    let info = s.execute("embroidery.info", &json!({}))?;
    Ok(json!({ "test": id, "name": name, "look": look, "stitches": info["stitches"], "fits": info["fits"], "hoop": made["hoop"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// U0: each test sew-out is made in one step, fits the hoop and sews the parts it describes.
    #[test]
    fn each_test_sew_out_fits_the_hoop() {
        let mut s = Session::new();
        s.execute("embroidery.myMachine.set", &json!({"machine": "Husqvarna Viking Designer Diamond deLuxe"})).unwrap();
        let mut counts = Vec::new();
        for (id, _, _) in TESTS {
            let r = s.execute("embroidery.testSew", &json!({ "test": id })).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert_eq!(r["fits"], true, "{id}: {r}");
            counts.push(r["stitches"].as_u64().unwrap());
        }
        assert!(counts.iter().all(|&n| n > 1500), "{counts:?}");
        assert!(s.execute("embroidery.testSew", &json!({"test": "plaid"})).is_err());
        assert_eq!(s.execute("embroidery.testSew.list", &json!({})).unwrap()["tests"].as_array().unwrap().len(), 4);
    }
}
