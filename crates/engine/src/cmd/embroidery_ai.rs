//! The AI features' Effort (AI-05): how hard the model works on Design from Prompt and Redraw
//! Image, kept in preferences. Higher effort thinks longer, which costs more; the dialog shows
//! the estimate (`sf_core::ai::effort`).

use serde_json::{Value, json};
use sf_core::ai::Effort;

use super::{CommandSpec, Result, Session, always, bad, cmd, f64_or, str_param};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.artPreview",
            "Art Stitch Preview",
            [],
            None,
            "{svg: flat-colour SVG art (Design from Prompt's), size?: mm 20–400 (100: its longest side), ppmm?: pixels a millimetre 1–10 (4)} the art digitized as embroidery.placeArt does, in a scratch document, and drawn as it stitches (the open document is left alone) → {dataBase64: PNG, width, height, objects, stitches}",
            always,
            art_preview
        ),
        cmd!(
        query "embroidery.aiEffort",
        "AI Effort",
        [],
        None,
        "{effort?: quick|normal|thorough} how hard the AI features work (normal: the model's own default; quick costs least; thorough thinks longest, at about twice normal's cost), kept for next time; without params, the effort set → {effort}",
        always,
        ai_effort
        ),
    ]
}

fn art_preview(_s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.artPreview";
    let svg = str_param(p, "svg").filter(|s| !s.trim().is_empty()).ok_or_else(|| bad(CMD, "missing `svg`: the art"))?;
    let size = f64_or(p, "size", 100.0);
    if !(size.is_finite() && (20.0..=400.0).contains(&size)) {
        return Err(bad(CMD, "`size` is 20 to 400 mm"));
    }
    let ppmm = f64_or(p, "ppmm", 4.0);
    if !(ppmm.is_finite() && (1.0..=10.0).contains(&ppmm)) {
        return Err(bad(CMD, "`ppmm` is 1 to 10"));
    }
    // a scratch session, so the user's document and its history are untouched
    let mut t = Session::new();
    t.execute("file.new", &json!({"width": 800, "height": 800}))?;
    let side = (size * 1.2).max(50.0);
    t.execute("embroidery.newDesign", &json!({"hoop": {"width": side, "height": side}}))?;
    let placed = t.execute("embroidery.placeArt", &json!({"svg": svg, "size": size}))?;
    t.execute("select.none", &json!({}))?;
    let info = t.execute("embroidery.info", &json!({}))?;
    let img = t.execute("embroidery.exportImage", &json!({"ppmm": ppmm, "background": "#f4f1ea"}))?;
    Ok(json!({
        "dataBase64": img["dataBase64"],
        "width": img["width"],
        "height": img["height"],
        "objects": placed["objects"],
        "stitches": info["stitches"],
    }))
}

/// The effort set in `prefs` (normal when none or unknown).
pub fn effort(prefs: &crate::Prefs) -> Effort {
    Effort::from_id(&prefs.embroidery_ai_effort).unwrap_or_default()
}

fn ai_effort(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.aiEffort";
    if let Some(v) = p.get("effort").filter(|v| !v.is_null()) {
        let e = v.as_str().and_then(Effort::from_id).ok_or_else(|| bad(CMD, "`effort` is quick, normal or thorough"))?;
        s.prefs.embroidery_ai_effort = if e == Effort::Normal { String::new() } else { e.id().to_string() };
    }
    Ok(json!({ "effort": effort(&s.prefs).id() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_is_previewed_as_it_stitches_without_touching_the_document() {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
        let before = s.execute("document.inspect", &json!({})).unwrap()["revision"].clone();
        let svg = r##"<svg viewBox="0 0 1000 1000"><circle cx="500" cy="500" r="400" fill="#cc3322"/><rect x="300" y="300" width="400" height="120" fill="#ffffff"/></svg>"##;
        let r = s.execute("embroidery.artPreview", &json!({"svg": svg, "size": 60})).unwrap();
        assert!(r["dataBase64"].as_str().unwrap().len() > 100, "{r}");
        assert!(r["stitches"].as_u64().unwrap() > 100 && r["objects"].as_u64().unwrap() >= 2, "{r}");
        assert_eq!(s.execute("document.inspect", &json!({})).unwrap()["revision"], before, "the open document is left alone");
        for bad in [json!({}), json!({"svg": svg, "size": 5}), json!({"svg": svg, "ppmm": 50})] {
            assert!(s.execute("embroidery.artPreview", &bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_effort_is_kept_and_checked() {
        let mut s = Session::new();
        assert_eq!(s.execute("embroidery.aiEffort", &json!({})).unwrap()["effort"], "normal");
        assert_eq!(s.execute("embroidery.aiEffort", &json!({"effort": "Thorough"})).unwrap()["effort"], "thorough");
        assert_eq!(effort(&s.prefs), Effort::Thorough);
        assert!(s.execute("embroidery.aiEffort", &json!({"effort": "max"})).is_err());
        assert_eq!(s.execute("embroidery.aiEffort", &json!({})).unwrap()["effort"], "thorough");
        s.execute("embroidery.aiEffort", &json!({"effort": "normal"})).unwrap();
        assert!(s.prefs.embroidery_ai_effort.is_empty(), "normal is the default: nothing kept");
    }
}
