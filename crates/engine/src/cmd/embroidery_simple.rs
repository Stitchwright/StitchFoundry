//! Simple mode's one-step actions (E2): words that sew, at a size a person can read, and a shape
//! or an appliqué piece, each in the middle of the design.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// The shapes Add a Shape offers (built-in clipart).
pub const SIMPLE_SHAPES: &[&str] = &["Heart", "Star", "Flower", "Leaf", "Sun", "Moon", "Paw", "Crown", "Snowflake"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.addWords",
            "Add Words",
            [],
            None,
            "{text, height?: mm the capital letters are tall (20; 5–150), color?: \"#rrggbb\" (dark blue), path?: id (a line the words follow, centred along it: a path, open or closed; words longer than the line are made smaller to fit), shape?: straight (default) | arcUp (a rainbow) | arcDown (a smile, read the right way up) | circle (round the middle, reading along the top; instead of a path)} words that sew: text in a clean built-in font, the given height, in the middle of the design or along the line, sewn as satin → {id, widthMm, heightMm}",
            has_doc,
            add_words
        ),
        cmd!(
            "embroidery.addShape",
            "Add a Shape",
            [],
            None,
            "{name: Heart|Star|Flower|Leaf|Sun|Moon|Paw|Crown|Snowflake, size?: mm (60% of the design's smaller side), applique?: bool (false: stitched; true: an appliqué piece, a placement line, tack-down and cover)} a shape in the middle of the design, ready to sew → {id}",
            has_doc,
            add_shape
        ),
    ]
}

/// New type in an embroidery design (one with a hoop) sews: satin from its letters' outlines, as
/// Add Words makes it, with the properties made current for satin. The Type tools and Type on a
/// Path make their type through here (`text.create`, `text.createInPath`); type in a document
/// that isn't an embroidery design is left as it is.
pub(crate) fn stitch_new_type(s: &mut Session, id: u64) -> Result<()> {
    if s.doc()?.doc.embroidery.as_deref().is_some_and(|e| e.hoop.is_some()) {
        s.execute("embroidery.set", &json!({ "type": "satin", "ids": [id] }))?;
    }
    Ok(())
}

/// `f`'s edits as one undo step (unless they are already part of an open one); undone if it fails.
pub(crate) fn one_step<T>(s: &mut Session, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_undo_group();
    }
    let res = f(s);
    if let Some(uid) = group {
        s.end_document_undo_group(uid, res.is_err());
    }
    res
}

/// The first artboard's middle and size (points).
fn page(s: &Session) -> Result<kurbo::Rect> {
    let d = &s.doc()?.doc;
    d.artboards.first().map(|a| a.rect).ok_or_else(|| EngineError::Other("the design has no page".into()))
}

/// Moves `id` so its middle is at `c` (points).
fn centre(s: &mut Session, id: u64, c: kurbo::Point) -> Result<()> {
    let b = {
        let d = &s.doc()?.doc;
        d.node(vectorcraft_doc::NodeId(id)).and_then(|n| n.bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false))
    };
    if let Some(b) = b {
        let (dx, dy) = (c.x - b.center().x, c.y - b.center().y);
        if dx.is_finite() && dy.is_finite() && (dx.abs() > 1e-3 || dy.abs() > 1e-3) {
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("object.move", &json!({ "dx": dx, "dy": dy }))?;
        }
    }
    Ok(())
}

fn add_words(s: &mut Session, p: &Value) -> Result<Value> {
    one_step(s, |s| make_words(s, p))
}

fn make_words(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.addWords";
    let text = str_param(p, "text").map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad(CMD, "missing `text`: the words to sew"))?.to_string();
    if text.chars().count() > 200 {
        return Err(bad(CMD, "keep the words to 200 letters"));
    }
    let height = match p.get("height") {
        None | Some(Value::Null) => 20.0,
        Some(v) => v.as_f64().filter(|v| (5.0..=150.0).contains(v)).ok_or_else(|| bad(CMD, "`height` is 5 to 150 mm"))?,
    };
    let color = str_param(p, "color").unwrap_or("#1a3e8c");
    sf_core::Rgb::from_hex(color).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?;
    // capital letters are about seven tenths of the type size
    let size = height * PT_PER_MM / 0.7;
    // a shape asked for: the line made for the words, about the page's middle
    let made_line = match str_param(p, "shape").unwrap_or("straight") {
        "straight" => None,
        shape @ ("arcUp" | "arcDown" | "circle") => {
            let r = page(s)?;
            let (c, w) = (r.center(), r.width().min(r.height()) * 0.75);
            let d = match shape {
                // a circular arc a quarter as deep as it is wide, over the top or under the bottom
                "arcUp" => format!("M {} {} A {} {} 0 0 1 {} {}", c.x - w / 2.0, c.y + w / 8.0, w * 0.625, w * 0.625, c.x + w / 2.0, c.y + w / 8.0),
                "arcDown" => format!("M {} {} A {} {} 0 0 0 {} {}", c.x - w / 2.0, c.y - w / 8.0, w * 0.625, w * 0.625, c.x + w / 2.0, c.y - w / 8.0),
                // from the bottom round by the left: the words centred on it read along the top
                _ => {
                    let rad = w / 2.0;
                    format!("M {cx} {b} A {rad} {rad} 0 0 1 {cx} {t} A {rad} {rad} 0 0 1 {cx} {b} Z", cx = c.x, b = c.y + rad, t = c.y - rad)
                }
            };
            let id = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(CMD, "the line for the words wasn't made"))?;
            Some(id)
        }
        other => return Err(bad(CMD, format!("`shape` is straight, arcUp, arcDown or circle, not `{other}`"))),
    };
    // along a line: type on that path, centred on it
    if let Some(v) = made_line.map(Value::from).or_else(|| p.get("path").filter(|v| !v.is_null()).cloned()) {
        let path = v.as_u64().ok_or_else(|| bad(CMD, "`path` is the id of the line the words follow"))?;
        let line = s.doc()?.doc.node(vectorcraft_doc::NodeId(path)).is_some_and(|n| matches!(n.kind, vectorcraft_doc::NodeKind::Path { .. }));
        if !line {
            return Err(bad(CMD, "the words can follow a line or a shape's outline: select one path"));
        }
        let made = s.execute(
            "text.createInPath",
            &json!({ "path": path, "mode": "onPath", "text": text, "size": size, "color": color, "justify": "center" }),
        )?;
        let id = made["id"].as_u64().ok_or_else(|| bad(CMD, "the words weren't made"))?;
        // words longer than the line are made smaller until they all fit on it (to a fifth of
        // the height asked at most)
        let mut fitted = size;
        let over = |s: &Session| super::typecmd::with_overflow(s, &[vectorcraft_doc::NodeId(id)], json!({})).get("overflow").is_some();
        while over(s) && fitted > size * 0.2 {
            fitted *= 0.9;
            s.execute("text.setStyle", &json!({ "ids": [id], "size": fitted }))?;
        }
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("embroidery.set", &json!({ "type": "satin" }))?;
        s.execute("embroidery.setThread", &json!({ "color": color, "name": "Words", "ids": [id] }))?;
        let round = |v: f64| (v * 10.0).round() / 10.0;
        let (w, h) = sewn_size(s, id).unwrap_or_default();
        return Ok(json!({ "id": id, "widthMm": round(w), "heightMm": round(h) }));
    }
    let r = page(s)?;
    let made = s.execute("text.create", &json!({ "x": r.center().x, "y": r.center().y, "text": text, "size": size, "color": color }))?;
    let id = made["id"].as_u64().ok_or_else(|| bad(CMD, "the words weren't made"))?;
    centre(s, id, r.center())?;
    s.execute("select.set", &json!({ "ids": [id] }))?;
    s.execute("embroidery.set", &json!({ "type": "satin" }))?;
    s.execute("embroidery.setThread", &json!({ "color": color, "name": "Words", "ids": [id] }))?;
    // the type's frame is taller than its letters: size by the stitches a capital sews at this
    // size, so a word with tails below the line (a y, a g) or without capitals is lettered at
    // the same size as any other
    if let Some(ink) = cap_ink(s, size, r.center(), color).or_else(|| sewn_size(s, id).map(|(_, h)| h)).filter(|h| *h > 0.5) {
        let f = height / ink;
        if f.is_finite() && (f - 1.0).abs() > 0.02 {
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("object.scale", &json!({ "sx": f * 100.0 }))?;
            centre(s, id, r.center())?;
        }
    }
    s.execute("select.set", &json!({ "ids": [id] }))?;
    let round = |v: f64| (v * 10.0).round() / 10.0;
    let (w, h) = sewn_size(s, id).unwrap_or_default();
    Ok(json!({ "id": id, "widthMm": round(w), "heightMm": round(h) }))
}

/// How tall (mm) a capital H sews in the words' font at type size `size`: lettered at `at`,
/// measured and taken away again.
fn cap_ink(s: &mut Session, size: f64, at: kurbo::Point, color: &str) -> Option<f64> {
    let made = s.execute("text.create", &json!({ "x": at.x, "y": at.y, "text": "H", "size": size, "color": color })).ok()?;
    let id = made["id"].as_u64()?;
    let ink = s
        .execute("select.set", &json!({ "ids": [id] }))
        .and_then(|_| s.execute("embroidery.set", &json!({ "type": "satin" })))
        .ok()
        .and_then(|_| sewn_size(s, id))
        .map(|(_, h)| h);
    // (the reference letter goes whatever happened; one left behind would sew)
    let _ = s.execute("edit.clear", &json!({ "ids": [id] }));
    ink
}

/// How wide and tall (mm) the stitches node `id` sews are.
fn sewn_size(s: &Session, id: u64) -> Option<(f64, f64)> {
    let plan = s.embroidery_plan()?;
    let node = vectorcraft_doc::NodeId(id);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (i, st) in plan.pattern().stitches.iter().enumerate() {
        if st.cmd == sf_core::Cmd::Stitch && plan.node_at_record(i) == Some(node) {
            (x0, y0, x1, y1) = (x0.min(st.x), y0.min(st.y), x1.max(st.x), y1.max(st.y));
        }
    }
    (x1 >= x0 && y1 >= y0).then_some((x1 - x0, y1 - y0))
}

fn add_shape(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.addShape";
    let name = str_param(p, "name").ok_or_else(|| bad(CMD, "missing `name`: Heart, Star, Flower, …"))?;
    let name = SIMPLE_SHAPES.iter().find(|n| n.eq_ignore_ascii_case(name.trim())).ok_or_else(|| bad(CMD, format!("no shape is named `{name}`")))?;
    let r = page(s)?;
    let side = r.width().min(r.height()) / PT_PER_MM;
    let size = match p.get("size") {
        None | Some(Value::Null) => (side * 0.6).clamp(10.0, 300.0),
        Some(v) => v.as_f64().filter(|v| (5.0..=500.0).contains(v)).ok_or_else(|| bad(CMD, "`size` is 5 to 500 mm"))?,
    };
    let made = s.execute("embroidery.clipart.insert", &json!({ "name": name, "size": size, "at": [r.center().x, r.center().y] }))?;
    let id = made["id"].as_u64().ok_or_else(|| bad(CMD, "the shape wasn't made"))?;
    s.execute("select.set", &json!({ "ids": [id] }))?;
    if bool_or(p, "applique", false) {
        s.execute("embroidery.set", &json!({ "type": "applique" }))?;
    }
    Ok(json!({ "id": id }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Husqvarna 240 × 150"})).unwrap();
        s
    }

    /// E2: Add Words makes words that sew, as tall as asked, in the middle of the design.
    #[test]
    fn words_sew_at_the_height_asked_in_the_middle() {
        let mut s = session();
        let r = s.execute("embroidery.addWords", &json!({"text": "Emma", "height": 25})).unwrap();
        let h = r["heightMm"].as_f64().unwrap();
        assert!((23.0..27.0).contains(&h), "{r}");
        let id = vectorcraft_doc::NodeId(r["id"].as_u64().unwrap());
        let d = &s.doc().unwrap().doc;
        let n = d.node(id).unwrap();
        assert!(matches!(n.embroidery.as_deref().map(|e| &e.stitch), Some(sf_core::attr::StitchType::Satin(_))));
        let (b, page) = (n.bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false).unwrap(), d.artboards[0].rect);
        assert!((b.center().x - page.center().x).abs() < 1.0 && (b.center().y - page.center().y).abs() < 1.0, "{b:?} {page:?}");
        assert!(s.embroidery_plan().unwrap().pattern().stitch_count() > 100);
        // a word with tails below the line is lettered at the same size: its capitals as tall
        let wide =
            |s: &mut Session, t: &str| s.execute("embroidery.addWords", &json!({"text": t, "height": 7})).unwrap()["widthMm"].as_f64().unwrap();
        let (john, mary) = (wide(&mut s, "John Green"), wide(&mut s, "Mary Green"));
        assert!(mary > john * 0.85, "Mary Green {mary} mm, John Green {john} mm");
        assert!(s.execute("embroidery.addWords", &json!({"text": "  "})).is_err());
        assert!(s.execute("embroidery.addWords", &json!({"text": "A", "height": 1})).is_err());
    }

    /// E2: Add a Shape stitches a shape, or makes it an appliqué piece.
    #[test]
    fn shapes_and_applique_pieces_land_in_the_middle_ready_to_sew() {
        let mut s = session();
        s.execute("embroidery.addShape", &json!({"name": "heart"})).unwrap();
        let before = s.embroidery_plan().unwrap().pattern().stitch_count();
        assert!(before > 100);
        s.execute("embroidery.addShape", &json!({"name": "Star", "applique": true, "size": 50})).unwrap();
        let plan = s.embroidery_plan().unwrap();
        assert!(plan.design.objects.iter().any(|o| matches!(o.kind, sf_core::model::ObjectKind::Applique(_))), "an appliqué piece");
        assert!(s.execute("embroidery.addShape", &json!({"name": "Dragon"})).is_err());
    }
}
