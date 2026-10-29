//! The lettering kiosk (KI-01): a simple mode for counter staff. The app shows one panel: pick a
//! template, type the name, pick a font and a thread, see it on the product, and send it to the
//! machine or save it. The shop chooses what the counter offers (templates, fonts, threads, the
//! machine's folder) and may lock the mode with a PIN so the full app stays out of reach.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, Result, Session, always, bad, cmd, str_param};

/// Most templates, fonts and threads offered, and the longest name typed.
const MAX_OFFERED: usize = 60;
const MAX_TEXT: usize = 200;
const MAX_LINES: usize = 4;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.kiosk",
            "Lettering Kiosk",
            [],
            None,
            "{on?: bool, pin?: digits (to lock it; \"\" for none), templates?: [name], fonts?: [name], colors?: [\"#rrggbb\"], folder?: the machine's folder, format?: as Export Machine File} the lettering kiosk: a simple mode for counter staff with only the templates, fonts and threads given (empty: all); while it is on and locked, changing or leaving it takes the PIN; without params, how it is set → {on, locked, templates, fonts, colors, folder, format}",
            always,
            kiosk
        ),
        cmd!(
            "embroidery.kiosk.order",
            "Kiosk Lettering",
            [],
            None,
            "{text: up to 4 lines, template?: a design template (the kiosk's first), font?: an embroidery font (the kiosk's first, else the first loaded), color?: \"#rrggbb\" (the kiosk's first thread), size?: mm (the capitals' height; as large as fits)} a new design from the template with the text set in the font, centred in the template's area and made to fit it, in the thread → {id, title, template, font, color, size, widthMm, heightMm}",
            always,
            order
        ),
    ]
}

/// What the counter offers, and whether the mode is on (`embroidery.kiosk`); kept across sessions.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct KioskSetup {
    pub on: bool,
    /// The PIN's hash (empty: not locked). A lock against leaving the mode by mistake at the
    /// counter, not a safe: the preferences file can be edited.
    pub pin: String,
    pub templates: Vec<String>,
    pub fonts: Vec<String>,
    pub colors: Vec<String>,
    pub folder: String,
    pub format: String,
}

impl KioskSetup {
    pub fn is_empty(&self) -> bool {
        *self == KioskSetup::default()
    }

    fn json(&self) -> Value {
        json!({
            "on": self.on, "locked": !self.pin.is_empty(), "templates": self.templates, "fonts": self.fonts,
            "colors": self.colors, "folder": self.folder, "format": self.format,
        })
    }
}

/// A PIN's hash (FNV-1a, 64 bits).
fn pin_hash(pin: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in "stitchfoundry-kiosk:".bytes().chain(pin.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// A list of names given as `key`, each trimmed, empty ones dropped.
fn names(p: &Value, key: &str, cmd: &str) -> Result<Option<Vec<String>>> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => {
            let mut out: Vec<String> = Vec::new();
            for v in a.iter().take(MAX_OFFERED) {
                let n: String = v.as_str().ok_or_else(|| bad(cmd, format!("`{key}` is a list of names")))?.trim().chars().take(120).collect();
                if !n.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&n)) {
                    out.push(n);
                }
            }
            Ok(Some(out))
        }
        Some(_) => Err(bad(cmd, format!("`{key}` is a list of names"))),
    }
}

fn kiosk(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.kiosk";
    let mut k = s.prefs.embroidery_kiosk.clone();
    let changes = ["on", "pin", "templates", "fonts", "colors", "folder", "format"].iter().any(|key| p.get(*key).is_some_and(|v| !v.is_null()));
    if !changes {
        return Ok(k.json());
    }
    let pin = p.get("pin").and_then(Value::as_str).map(str::trim);
    // on and locked: the PIN first
    if k.on && !k.pin.is_empty() && pin.map(pin_hash).as_deref() != Some(k.pin.as_str()) {
        return Err(bad(CMD, "the kiosk is locked: give its PIN"));
    }
    if let Some(on) = p.get("on").filter(|v| !v.is_null()) {
        k.on = on.as_bool().ok_or_else(|| bad(CMD, "`on` is true or false"))?;
    }
    // a new PIN (or none) is set while it is unlocked or turned on
    if let Some(pin) = pin
        && (k.pin.is_empty() || !k.on)
    {
        if !pin.is_empty() && (!(4..=12).contains(&pin.len()) || !pin.chars().all(|c| c.is_ascii_digit())) {
            return Err(bad(CMD, "the PIN is 4 to 12 digits"));
        }
        k.pin = if pin.is_empty() { String::new() } else { pin_hash(pin) };
    }
    if let Some(t) = names(p, "templates", CMD)? {
        let known: Vec<String> = s.execute("embroidery.templates", &json!({}))?["templates"]
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        // as the templates spell them
        let mut out = Vec::new();
        for n in &t {
            out.push(
                known.iter().find(|k| k.eq_ignore_ascii_case(n)).cloned().ok_or_else(|| bad(CMD, format!("no design template is named `{n}`")))?,
            );
        }
        k.templates = out;
    }
    if let Some(f) = names(p, "fonts", CMD)? {
        k.fonts = f;
    }
    if let Some(c) = names(p, "colors", CMD)? {
        let mut out = Vec::new();
        for c in c {
            out.push(sf_core::Rgb::from_hex(&c).ok_or_else(|| bad(CMD, "`colors` are \"#rrggbb\""))?.to_hex());
        }
        k.colors = out;
    }
    if let Some(f) = str_param(p, "folder") {
        k.folder = f.trim().chars().take(2000).collect();
    }
    if let Some(f) = str_param(p, "format") {
        let f = f.trim().trim_start_matches('.').to_ascii_lowercase();
        if !f.is_empty() && !super::embroidery::MACHINE_EXTS.contains(&f.as_str()) {
            return Err(bad(CMD, format!("`{f}` isn't a machine format")));
        }
        k.format = f;
    }
    s.prefs.embroidery_kiosk = k.clone();
    Ok(k.json())
}

/// `name` among `offered` (any when nothing is offered), as it is spelled there.
fn offered(offered: &[String], name: &str) -> Option<String> {
    if offered.is_empty() {
        return Some(name.to_string());
    }
    offered.iter().find(|o| o.eq_ignore_ascii_case(name)).cloned()
}

fn order(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.kiosk.order";
    let k = s.prefs.embroidery_kiosk.clone();
    let text = str_param(p, "text").map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| bad(CMD, "missing `text`: the name to sew"))?;
    let text: String = text.lines().map(str::trim).filter(|l| !l.is_empty()).take(MAX_LINES).collect::<Vec<_>>().join("\n");
    let text: String = text.chars().take(MAX_TEXT).collect();
    let template = match str_param(p, "template") {
        Some(t) => offered(&k.templates, t.trim()).ok_or_else(|| bad(CMD, format!("`{t}` isn't offered at this counter")))?,
        None => k.templates.first().cloned().unwrap_or_else(|| "Name tape 100 × 30".to_string()),
    };
    // the fonts remembered from earlier sessions, loaded the first time
    if s.embroidery_fonts.is_empty() {
        // no fonts is answered below
        let _ = s.execute("embroidery.fonts", &json!({}));
    }
    let loaded: Vec<String> = s.embroidery_fonts.iter().map(|f| f.info.name.clone()).collect();
    let font = match str_param(p, "font") {
        Some(f) => offered(&k.fonts, f.trim()).ok_or_else(|| bad(CMD, format!("`{f}` isn't offered at this counter")))?,
        None => k
            .fonts
            .iter()
            .find(|f| loaded.iter().any(|l| l.eq_ignore_ascii_case(f)))
            .or(loaded.first())
            .cloned()
            .ok_or_else(|| bad(CMD, "no embroidery font is loaded"))?,
    };
    let color = match str_param(p, "color") {
        Some(c) => {
            let c = sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?.to_hex();
            offered(&k.colors, &c).ok_or_else(|| bad(CMD, format!("`{c}` isn't offered at this counter")))?
        }
        None => k.colors.first().cloned().unwrap_or_else(|| "#1d3557".to_string()),
    };
    let asked = match p.get("size") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_f64().filter(|v| (3.0..=200.0).contains(v)).ok_or_else(|| bad(CMD, "`size` is 3 to 200 mm"))?),
    };
    lettered(s, &text, template, font, color, asked)
}

/// A new design from `template` with `text` (up to four lines) in `font` and thread `color`,
/// centred in the template's area and made to fit it unless `asked` gives the size (mm): Kiosk
/// Lettering's design, and an online order's ([`super::embroidery_orders`]).
pub(crate) fn lettered(s: &mut Session, text: &str, template: String, font: String, color: String, asked: Option<f64>) -> Result<Value> {
    const CMD: &str = "embroidery.kiosk.order";
    let lines = text.lines().count().max(1);
    // as the template spells its name
    let made = s.execute("embroidery.template.new", &json!({ "name": template }))?;
    let template = made["template"].as_str().map_or(template, str::to_string);
    // the area the text goes in (mm): the template's, inside a patch's border
    let (gx, gy, gw, gh) = match super::embroidery_templates::area(&template) {
        Some(a) => a,
        None => {
            let a = s.doc()?.doc.artboards.first().map(|a| a.rect).unwrap_or(kurbo::Rect::new(0.0, 0.0, 100.0 * PT_PER_MM, 50.0 * PT_PER_MM));
            let (w, h) = (a.width() / PT_PER_MM, a.height() / PT_PER_MM);
            (a.x0 / PT_PER_MM + w * 0.1, a.y0 / PT_PER_MM + h * 0.1, w * 0.8, h * 0.8)
        }
    };
    let size = asked.unwrap_or_else(|| (gh * 0.6 / lines as f64).clamp(3.0, 60.0));
    let (cx, cy) = ((gx + gw / 2.0) * PT_PER_MM, (gy + gh / 2.0) * PT_PER_MM);
    let r = s.execute("embroidery.lettering", &json!({ "text": text, "font": font, "size": size, "at": [cx, cy], "align": "centre" }))?;
    let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "the lettering wasn't made"))?;
    s.execute("select.set", &json!({ "ids": [id] }))?;
    let bounds = |s: &Session| -> Result<kurbo::Rect> {
        let d = &s.doc()?.doc;
        d.node(vectorcraft_doc::NodeId(id))
            .and_then(|n| n.bounds_in(&d.symbol_boxes(), kurbo::Affine::IDENTITY, false))
            .ok_or_else(|| bad(CMD, "the lettering has no size"))
    };
    // made to fit the area unless a size was asked for
    let mut size = size;
    let b = bounds(s)?;
    let k_fit = ((gw * PT_PER_MM) / b.width().max(1e-6)).min((gh * PT_PER_MM) / b.height().max(1e-6));
    if asked.is_none() && k_fit.is_finite() && k_fit < 1.0 {
        size = (size * k_fit * 0.97).max(1.0);
        s.execute("embroidery.lettering.edit", &json!({ "id": id, "size": size }))?;
    }
    // its middle in the area's
    let b = bounds(s)?;
    let (dx, dy) = (cx - b.center().x, cy - b.center().y);
    if dx.is_finite() && dy.is_finite() && (dx.abs() > 1e-3 || dy.abs() > 1e-3) {
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("object.move", &json!({ "dx": dx, "dy": dy }))?;
    }
    s.execute("embroidery.setThread", &json!({ "color": color, "ids": [id] }))?;
    s.execute("select.none", &json!({}))?;
    let title = text.lines().next().unwrap_or("Name").to_string();
    if let Ok(st) = s.doc_mut() {
        std::sync::Arc::make_mut(&mut st.doc).title = title.clone();
    }
    let b = bounds(s)?;
    Ok(json!({
        "id": id, "title": title, "template": template, "font": font, "color": color, "size": (size * 10.0).round() / 10.0,
        "widthMm": (b.width() / PT_PER_MM * 10.0).round() / 10.0, "heightMm": (b.height() / PT_PER_MM * 10.0).round() / 10.0,
    }))
}
