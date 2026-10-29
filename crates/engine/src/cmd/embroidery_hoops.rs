//! Hoops (HO-01, HO-03, SYS-05): the smallest hoop that holds the design, chosen for it (turned
//! when that fits better), the hoop turned a quarter, and hoops of one's own kept by name beside
//! the standard ones.

use serde_json::{Value, json};
use sf_core::hoops::{Hoop, HoopShape};

use super::{CommandSpec, Result, Session, always, bad, bool_or, cmd, has_doc, str_param};

/// Hoops of one's own kept, at most.
const MAX_CUSTOM: usize = 100;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.hoops",
            "Hoops",
            [],
            None,
            "{} the standard hoops and one's own, each with whether the design fits it (either way round) → {hoops: [{name, width, height, shape, custom, fits}], hoop, rotated}",
            always,
            list
        ),
        cmd!(
            "embroidery.hoop.auto",
            "Choose Smallest Hoop",
            ["Embroidery"],
            None,
            "{brand?: text the hoop's name has (Brother, Janome, Magnetic…), turn?: bool (true: a hoop turned a quarter counts too), special?: bool (false: cap, sleeve and jacket-back frames left out)} the smallest hoop (by its field) that holds the design, chosen for it → {hoop, width, height, rotated}",
            has_doc,
            auto
        ),
        cmd!(
            "embroidery.hoop.rotate",
            "Rotate Hoop",
            ["Embroidery"],
            None,
            "{} turn the design's hoop a quarter (its width and height swapped) → {hoop, width, height, rotated}",
            has_doc,
            rotate
        ),
        cmd!(
            "embroidery.hoop.saveCustom",
            "Save Custom Hoop",
            [],
            None,
            "{name, width, height: mm (20–2000), shape?: rect|oval (rect)} a hoop of one's own, listed with the standard ones (a standard hoop's name can't be taken); 100 at most → {name}",
            always,
            save_custom
        ),
        cmd!("embroidery.hoop.deleteCustom", "Delete Custom Hoop", [], None, "{name} forget a hoop of one's own → {name}", always, delete_custom),
    ]
}

/// The standard hoops, then one's own.
pub(crate) fn all_hoops(s: &Session) -> Vec<Hoop> {
    let mut v = sf_core::hoops::standard_hoops();
    v.extend(s.prefs.embroidery_custom_hoops.iter().cloned());
    v
}

/// The hoop named `name` (any case), standard or one's own.
pub(crate) fn find_hoop(s: &Session, name: &str) -> Option<Hoop> {
    all_hoops(s).into_iter().find(|h| h.name.eq_ignore_ascii_case(name.trim()))
}

/// Whether the design's hoop is a named hoop turned a quarter.
fn turned(s: &Session, hoop: &Hoop) -> bool {
    find_hoop(s, &hoop.name).is_some_and(|h| (h.width - hoop.height).abs() < 1e-6 && (h.height - hoop.width).abs() < 1e-6 && h.width != h.height)
}

/// The design's size (mm) as it sews, if anything is stitched.
fn design_size(s: &Session) -> Option<(f64, f64)> {
    let plan = s.embroidery_plan()?;
    let st = plan.stats.as_ref()?;
    Some((st.width_mm, st.height_mm))
}

fn fits(h: &Hoop, (w, ht): (f64, f64)) -> bool {
    let b = |w: f64, h: f64| sf_core::BBox::new(sf_core::Point::new(-w / 2.0, -h / 2.0), sf_core::Point::new(w / 2.0, h / 2.0));
    h.fits(&b(w, ht))
}

fn turn(h: &Hoop) -> Hoop {
    Hoop { width: h.height, height: h.width, positions: h.positions.iter().map(|[x, y]| [*y, *x]).collect(), ..h.clone() }
}

fn set_hoop(s: &mut Session, h: &Hoop) -> Result<()> {
    // the hoop as it is (turned, one's own), set whole
    let shape = if h.shape == HoopShape::Oval { "oval" } else { "rect" };
    s.execute("embroidery.settings", &json!({ "hoop": { "name": h.name, "width": h.width, "height": h.height, "shape": shape } }))?;
    Ok(())
}

fn answer(s: &Session, h: &Hoop) -> Value {
    json!({ "hoop": h.name, "width": h.width, "height": h.height, "rotated": turned(s, h) })
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let size = design_size(s);
    let custom = &s.prefs.embroidery_custom_hoops;
    let hoops: Vec<Value> = all_hoops(s)
        .iter()
        .map(|h| {
            json!({
                "name": h.name,
                "width": h.width,
                "height": h.height,
                "shape": if h.shape == HoopShape::Oval { "oval" } else { "rect" },
                "custom": custom.iter().any(|c| c.name == h.name),
                "fits": size.is_some_and(|d| fits(h, d) || fits(&turn(h), d)),
            })
        })
        .collect();
    let cur = s.active().and_then(|d| d.doc.embroidery.as_deref()).and_then(|e| e.hoop.clone());
    Ok(json!({ "hoops": hoops, "hoop": cur.as_ref().map(|h| h.name.clone()), "rotated": cur.as_ref().is_some_and(|h| turned(s, h)) }))
}

fn auto(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.hoop.auto";
    let size = design_size(s).ok_or_else(|| bad(CMD, "nothing is stitched yet: give objects a stitch type first"))?;
    let brand = str_param(p, "brand").map(|b| b.trim().to_lowercase()).filter(|b| !b.is_empty());
    let may_turn = bool_or(p, "turn", true);
    let special = bool_or(p, "special", false);
    let is_special = |h: &Hoop| {
        let n = h.name.to_lowercase();
        ["cap", "sleeve", "jacket"].iter().any(|w| n.contains(w))
    };
    let mut best: Option<Hoop> = None;
    for h in all_hoops(s) {
        if brand.as_ref().is_some_and(|b| !h.name.to_lowercase().contains(b.as_str())) || (!special && is_special(&h)) {
            continue;
        }
        let way = if fits(&h, size) {
            h
        } else if may_turn && fits(&turn(&h), size) {
            turn(&h)
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|b| way.width * way.height < b.width * b.height - 1e-9) {
            best = Some(way);
        }
    }
    let h = best.ok_or_else(|| bad(CMD, format!("no hoop holds a design of {:.0} × {:.0} mm: try multi-hooping", size.0, size.1)))?;
    set_hoop(s, &h)?;
    Ok(answer(s, &h))
}

fn rotate(s: &mut Session, _: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.hoop.rotate";
    let cur = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.hoop.clone()).ok_or_else(|| bad(CMD, "the design has no hoop yet"))?;
    let h = turn(&cur);
    set_hoop(s, &h)?;
    Ok(answer(s, &h))
}

fn save_custom(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.hoop.saveCustom";
    let name: String =
        str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?.chars().take(80).collect();
    if sf_core::hoops::standard_hoops().iter().any(|h| h.name.eq_ignore_ascii_case(&name)) {
        return Err(bad(CMD, format!("`{name}` is a standard hoop: give yours another name")));
    }
    let mm =
        |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| (20.0..=2000.0).contains(v)).ok_or_else(|| bad(CMD, format!("`{k}` is 20 to 2000 mm")));
    let (w, h) = (mm("width")?, mm("height")?);
    let shape = match str_param(p, "shape").unwrap_or("rect") {
        "rect" => HoopShape::Rect,
        "oval" => HoopShape::Oval,
        _ => return Err(bad(CMD, "`shape` is rect or oval")),
    };
    let hoop = Hoop::new(&name, w, h, shape);
    let own = &mut s.prefs.embroidery_custom_hoops;
    let full = own.len() >= MAX_CUSTOM;
    match own.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&name)) {
        Some(o) => *o = hoop,
        None if full => return Err(bad(CMD, format!("{MAX_CUSTOM} hoops of your own at most: delete one first"))),
        None => own.push(hoop),
    }
    Ok(json!({ "name": name }))
}

fn delete_custom(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.hoop.deleteCustom";
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad(CMD, "missing `name`"))?.to_string();
    let own = &mut s.prefs.embroidery_custom_hoops;
    let before = own.len();
    own.retain(|h| !h.name.eq_ignore_ascii_case(&name));
    if own.len() == before {
        return Err(bad(CMD, format!("no hoop of yours is named `{name}`")));
    }
    Ok(json!({ "name": name }))
}
