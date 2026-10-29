//! Design templates (BAS-06, OP-05): a new design started from a template: its page, hoop and
//! fabric set, a guide round the area designs usually fill, and for patches and name tapes the
//! border already digitized. Built-in ones are made in code; one's own keep a design's
//! properties and layout as it is.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, Result, Session, always, bad, cmd, has_doc, str_param};

/// Templates of one's own kept, at most, and the largest (bytes of the saved template).
const MAX_OWN: usize = 30;
const MAX_BYTES: usize = 4 << 20;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.templates",
            "Design Templates",
            [],
            None,
            "{} the design templates, built-in and one's own → {templates: [{name, description, custom}]}",
            always,
            list
        ),
        cmd!(
            "embroidery.template.new",
            "New from Design Template",
            [],
            None,
            "{name} a new design from a template: built-in ones set the page, hoop and fabric, draw a guide round the area such designs fill and, for patches and name tapes, the border already digitized; one's own open as they were saved → {title, template}",
            always,
            new_from
        ),
        cmd!(
            "embroidery.template.save",
            "Save as Design Template",
            [],
            None,
            "{name} keep the active design as a template of one's own (its page, layers, art, hoop, fabric, machine and settings), replacing one of that name; 30 at most, 4 MB each → {name, bytes}",
            has_doc,
            save
        ),
        cmd!("embroidery.template.delete", "Delete Design Template", [], None, "{name} forget a template of one's own → {name}", always, delete),
    ]
}

/// A design template of one's own: the design saved as a VectorCraft template.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OwnTemplate {
    pub name: String,
    /// The template file, base64.
    pub data: String,
}

/// A built-in template: its page (mm), hoop, the guide (x, y, width, height in mm; round when
/// `round`) and a border to digitize (`satin` mm wide) when it is a patch.
struct Builtin {
    name: &'static str,
    description: &'static str,
    page: (f64, f64),
    hoop: &'static str,
    guide: (f64, f64, f64, f64),
    round: bool,
    border: Option<f64>,
}

const BUILTIN: &[Builtin] = &[
    Builtin {
        name: "Left chest logo",
        description: "A 4×4 hoop, the usual left-chest area: up to 100 mm wide",
        page: (130.0, 130.0),
        hoop: "Brother 100 × 100 (4×4)",
        guide: (20.0, 35.0, 90.0, 60.0),
        round: false,
        border: None,
    },
    Builtin {
        name: "Cap front",
        description: "A cap frame, the front panel's area: about 110 × 55 mm, above the brim",
        page: (160.0, 90.0),
        hoop: "Cap frame 130 × 60",
        guide: (25.0, 17.5, 110.0, 55.0),
        round: false,
        border: None,
    },
    Builtin {
        name: "Full back",
        description: "A 30 × 30 cm frame, a jacket or shirt back: up to 300 mm wide",
        page: (330.0, 330.0),
        hoop: "Tajima Square 30 × 30 cm",
        guide: (15.0, 25.0, 300.0, 280.0),
        round: false,
        border: None,
    },
    Builtin {
        name: "Sleeve",
        description: "A sleeve frame: a narrow design down the sleeve",
        page: (110.0, 330.0),
        hoop: "Sleeve 80 × 300",
        guide: (20.0, 40.0, 70.0, 250.0),
        round: false,
        border: None,
    },
    Builtin {
        name: "Round patch 75 mm",
        description: "A 75 mm round patch, its 4 mm satin border digitized: fill the middle",
        page: (110.0, 110.0),
        hoop: "Brother 100 × 100 (4×4)",
        guide: (17.5, 17.5, 75.0, 75.0),
        round: true,
        border: Some(4.0),
    },
    Builtin {
        name: "Name tape 100 × 30",
        description: "A rectangular name tape with a 3 mm satin border: type the name inside",
        page: (130.0, 60.0),
        hoop: "Brother 130 × 180 (5×7)",
        guide: (15.0, 15.0, 100.0, 30.0),
        round: false,
        border: Some(3.0),
    },
    Builtin {
        name: "Towel border",
        description: "A wide hoop for towel names and monograms above the border",
        page: (300.0, 160.0),
        hoop: "Husqvarna 360 × 200",
        guide: (25.0, 30.0, 250.0, 100.0),
        round: false,
        border: None,
    },
];

/// The area a built-in template's designs go in (x, y, width, height in mm): its guide, inside a
/// patch's border (in a round patch, the square inside its circle).
pub(crate) fn area(name: &str) -> Option<(f64, f64, f64, f64)> {
    let b = BUILTIN.iter().find(|b| b.name.eq_ignore_ascii_case(name))?;
    let (x, y, w, h) = b.guide;
    let mut inset = b.border.map_or(0.0, |width| width + 2.0);
    if b.round {
        inset += (w.min(h) / 2.0 - inset) * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
    }
    Some((x + inset, y + inset, w - 2.0 * inset, h - 2.0 * inset))
}

fn list(s: &mut Session, _: &Value) -> Result<Value> {
    let mut v: Vec<Value> = BUILTIN.iter().map(|b| json!({ "name": b.name, "description": b.description, "custom": false })).collect();
    v.extend(s.prefs.embroidery_templates.iter().map(|t| json!({ "name": t.name, "description": "", "custom": true })));
    Ok(json!({ "templates": v }))
}

fn new_from(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.template.new";
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad(CMD, "missing `name`"))?.to_string();
    if let Some(own) = s.prefs.embroidery_templates.iter().find(|t| t.name.eq_ignore_ascii_case(&name)).cloned() {
        s.execute("file.newFromTemplate", &json!({ "name": format!("{}.{}", own.name, "vectorcraft"), "dataBase64": own.data }))?;
        return Ok(json!({ "title": s.doc()?.doc.title, "template": own.name }));
    }
    let b = BUILTIN.iter().find(|b| b.name.eq_ignore_ascii_case(&name)).ok_or_else(|| bad(CMD, format!("no design template is named `{name}`")))?;
    let pt = |mm: f64| mm * PT_PER_MM;
    s.execute("file.new", &json!({ "width": pt(b.page.0), "height": pt(b.page.1) }))?;
    // (a template's page is its own size, a margin round the hoop's)
    s.execute("embroidery.settings", &json!({ "hoop": b.hoop, "page": false }))?;
    let (x, y, w, h) = b.guide;
    let shape = |s: &mut Session, inset: f64| -> Result<u64> {
        let r = json!({ "x": pt(x + inset), "y": pt(y + inset), "width": pt(w - 2.0 * inset), "height": pt(h - 2.0 * inset) });
        let id = s.execute(if b.round { "shape.ellipse" } else { "shape.rectangle" }, &r)?["id"].as_u64();
        id.ok_or_else(|| bad(CMD, "the template's shape wasn't made"))
    };
    // the area such designs fill, as a guide (never stitched or printed)
    let guide = shape(s, 0.0)?;
    s.execute("select.set", &json!({ "ids": [guide] }))?;
    s.execute("view.guides.make", &json!({}))?;
    // a patch's border, digitized as satin along its edge
    if let Some(width) = b.border {
        let id = shape(s, width / 2.0)?;
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("paint.setFill", &json!({ "none": true }))?;
        s.execute("paint.setStroke", &json!({ "color": "#1d3557" }))?;
        s.execute("embroidery.set", &json!({ "type": "satin" }))?;
        s.execute("embroidery.setParams", &json!({ "params": { "width": width } }))?;
        s.execute("embroidery.setThread", &json!({ "color": "#1d3557" }))?;
    }
    s.execute("select.none", &json!({}))?;
    if let Ok(st) = s.doc_mut() {
        std::sync::Arc::make_mut(&mut st.doc).title = b.name.to_string();
    }
    Ok(json!({ "title": b.name, "template": b.name }))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.template.save";
    let name: String =
        str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?.chars().take(80).collect();
    if BUILTIN.iter().any(|b| b.name.eq_ignore_ascii_case(&name)) {
        return Err(bad(CMD, format!("`{name}` is a built-in template: give yours another name")));
    }
    let r = s.execute("document.serialize", &json!({ "format": "template" }))?;
    let data = r["dataBase64"].as_str().ok_or_else(|| bad(CMD, "the design couldn't be kept as a template"))?.to_string();
    let bytes = data.len() / 4 * 3;
    if bytes > MAX_BYTES {
        return Err(bad(CMD, "the design is larger than 4 MB as a template: leave large images out"));
    }
    let own = &mut s.prefs.embroidery_templates;
    let full = own.len() >= MAX_OWN;
    let t = OwnTemplate { name: name.clone(), data };
    match own.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&name)) {
        Some(o) => *o = t,
        None if full => return Err(bad(CMD, format!("{MAX_OWN} templates of your own at most: delete one first"))),
        None => own.push(t),
    }
    Ok(json!({ "name": name, "bytes": bytes }))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.template.delete";
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad(CMD, "missing `name`"))?.to_string();
    let own = &mut s.prefs.embroidery_templates;
    let before = own.len();
    own.retain(|t| !t.name.eq_ignore_ascii_case(&name));
    if own.len() == before {
        return Err(bad(CMD, format!("no template of yours is named `{name}`")));
    }
    Ok(json!({ "name": name }))
}
