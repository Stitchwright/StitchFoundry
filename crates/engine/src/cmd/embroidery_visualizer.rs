//! The product visualizer (CW-05): the design's stitches, life size, on a garment or product drawn
//! in its colour (T-shirt, polo, hoodie, ball cap, snapback, trucker or dad cap, tote bag, towel),
//! at the usual place for it, or on a photo of the customer's own product; a new document to show,
//! print or export.

use serde_json::{Value, json};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document};
use vectorcraft_embroidery::PT_PER_MM;

use super::embroidery::sheet_text;
use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

/// Margin round the product (mm).
const MARGIN: f64 = 40.0;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.visualize",
            "Product Visualizer",
            ["Embroidery"],
            None,
            "{product?: tshirt|polo|hoodie|cap|snapback|trucker|dadcap|tote|towel (tshirt; cap: a structured ball cap, snapback: flat bill, trucker: mesh back, dadcap: low and unstructured), color?: \"#rrggbb\" (the product's colour; white), placement?: left-chest|right-chest|full-front|full-back|yoke|sleeve|pocket (garments; cap-front for caps of every style; full-front for totes and towels; default: the product's usual), image?: a photo of the customer's own product instead (PNG, JPEG…), imageWidth?: mm the photo shows across (500; 50–3000), at?: [x, y] mm from the photo's top left where the design's middle goes (its middle), areas?: bool (false) the usual work areas of the side shown drawn dashed and named} the design's stitches life size on the product drawn in its colour (front, or back for full-back and yoke) at the usual place for it, or on the photo; a size larger than usual there is warned of; on shirts and hoodies, how far below the neckline the design starts; opens a new document → {product, placement, view, widthMm, heightMm, belowNeckMm?, areas, warnings}",
            has_doc,
            visualize
        ),
        cmd!(
            "embroidery.background",
            "Garment Background",
            [],
            None,
            "{path: a photo of the garment or product (PNG, JPEG…), width?: mm the photo shows across (500; 50–3000), at?: [x, y] mm where its middle goes (the design's middle)} a photo of the garment behind the design while it is digitized, life size: on a locked template layer below the current one, never stitched, printed or exported with the design; delete the layer to take it away → {ids, widthMm, heightMm}",
            has_doc,
            background
        ),
    ]
}

/// Where the design goes: its middle across at `x`, and its middle, top or bottom at `y` (mm).
#[derive(Clone, Copy)]
enum Anchor {
    Centre(f64, f64),
    Top(f64, f64),
    Bottom(f64, f64),
}

/// A product as drawn: its box (mm), outline and seam lines (SVG path data in mm), and where each
/// placement goes with the widest a design usually is there.
struct Product {
    size: (f64, f64),
    /// Filled in the product's colour.
    body: &'static [&'static str],
    /// Seams, hems and details, drawn in a darker shade.
    lines: &'static [&'static str],
    placements: &'static [(&'static str, Anchor, f64)],
    default: &'static str,
    /// The neckline's lowest point at the middle (mm from the top), on shirts and hoodies.
    neck: Option<f64>,
}

// The T-shirt's front: neckline, shoulders, short sleeves, straight body; 820 × 740 mm box.
const TEE: &str =
    "M 320 20 C 340 100 480 100 500 20 L 640 50 L 800 250 L 690 330 L 660 300 L 660 730 L 160 730 L 160 300 L 130 330 L 20 250 L 180 50 Z";
const TEE_BACK: &str =
    "M 320 20 C 340 45 480 45 500 20 L 640 50 L 800 250 L 690 330 L 660 300 L 660 730 L 160 730 L 160 300 L 130 330 L 20 250 L 180 50 Z";
const TEE_LINES: &[&str] = &["M 322 34 C 342 112 478 112 498 34", "M 160 705 L 660 705", "M 784 230 L 674 310", "M 36 230 L 146 310"];
const GARMENT_PLACES: &[(&str, Anchor, f64)] = &[
    ("left-chest", Anchor::Centre(500.0, 220.0), 100.0),
    ("right-chest", Anchor::Centre(320.0, 220.0), 100.0),
    ("full-front", Anchor::Top(410.0, 170.0), 300.0),
    ("full-back", Anchor::Top(410.0, 150.0), 350.0),
    ("yoke", Anchor::Top(410.0, 70.0), 125.0),
    ("sleeve", Anchor::Centre(705.0, 215.0), 100.0),
    ("pocket", Anchor::Centre(500.0, 240.0), 110.0),
];

const HOODIE: &str =
    "M 310 120 L 180 150 L 60 410 L 20 700 L 100 710 L 160 460 L 160 840 L 660 840 L 660 460 L 720 710 L 800 700 L 760 410 L 640 150 L 510 120 Z";
const HOOD: &str = "M 300 125 C 270 0 550 0 520 125 C 470 175 350 175 300 125 Z";
const HOODIE_LINES: &[&str] = &[
    "M 322 128 C 300 40 520 40 498 128",
    "M 260 640 L 560 640 L 600 780 L 220 780 Z",
    "M 160 815 L 660 815",
    "M 24 680 L 98 688",
    "M 796 680 L 722 688",
];
const HOODIE_PLACES: &[(&str, Anchor, f64)] = &[
    ("left-chest", Anchor::Centre(500.0, 300.0), 100.0),
    ("right-chest", Anchor::Centre(320.0, 300.0), 100.0),
    ("full-front", Anchor::Top(410.0, 260.0), 300.0),
    ("full-back", Anchor::Top(410.0, 230.0), 350.0),
    ("yoke", Anchor::Top(410.0, 150.0), 125.0),
    ("sleeve", Anchor::Centre(705.0, 300.0), 100.0),
];

const POLO_LINES: &[&str] = &[
    "M 395 75 L 425 75 L 425 230 L 395 230 Z",
    "M 320 20 L 400 95 L 360 112 L 300 30 Z",
    "M 500 20 L 420 95 L 460 112 L 520 30 Z",
    "M 160 705 L 660 705",
    "M 784 230 L 674 310",
    "M 36 230 L 146 310",
];

// A structured ball cap from the front: the crown's six panels up to the button, the bill curved
// down at its sides in front of it (separate shapes, so their overlap is filled), the side seams,
// eyelets and the rows of stitching along the bill.
const CAP_CROWN: &str = "M 50 152 C 46 98 58 52 104 36 C 126 28 174 28 196 36 C 242 52 254 98 250 152 Z";
const CAP_BRIM: &str = "M 48 146 C 95 136 205 136 252 146 C 259 156 259 168 253 177 C 218 160 82 160 47 177 C 41 168 41 156 48 146 Z";
const CAP_BUTTON: &str = "M 143 31 A 7 4 0 1 0 157 31 A 7 4 0 1 0 143 31 Z";
const CAP_LINES: &[&str] = &[
    "M 150 31 C 120 40 92 78 85 141",
    "M 150 31 C 180 40 208 78 215 141",
    "M 93 70 A 3 3 0 1 0 99 70 A 3 3 0 1 0 93 70 Z",
    "M 201 70 A 3 3 0 1 0 207 70 A 3 3 0 1 0 201 70 Z",
    "M 54 168 C 92 154 208 154 246 168",
    "M 60 163 C 96 150 204 150 240 163",
];
const CAP_PLACES: &[(&str, Anchor, f64)] = &[("cap-front", Anchor::Bottom(150.0, 124.0), 130.0)];

// A snapback: a high, upright crown and a flat bill, seen a little from above.
const SNAP_CROWN: &str = "M 52 150 C 48 88 56 38 100 25 C 124 18 176 18 200 25 C 244 38 252 88 248 150 Z";
const SNAP_BILL: &str = "M 52 145 L 248 145 L 266 165 C 267 169 265 172 261 172 L 39 172 C 35 172 33 169 34 165 Z";
const HIGH_BUTTON: &str = "M 143 22 A 7 4 0 1 0 157 22 A 7 4 0 1 0 143 22 Z";
const SNAP_LINES: &[&str] = &[
    "M 150 22 C 120 31 92 70 85 144",
    "M 150 22 C 180 31 208 70 215 144",
    "M 93 60 A 3 3 0 1 0 99 60 A 3 3 0 1 0 93 60 Z",
    "M 201 60 A 3 3 0 1 0 207 60 A 3 3 0 1 0 201 60 Z",
    "M 40 167 L 260 167",
    "M 44 162 L 256 162",
];
const SNAP_PLACES: &[(&str, Anchor, f64)] = &[("cap-front", Anchor::Bottom(150.0, 124.0), 130.0)];

// A trucker cap: a high foam front panel, mesh sides (drawn by `mesh`) and a curved bill.
const TRUCKER_CROWN: &str = "M 50 152 C 46 90 56 38 102 25 C 125 18 175 18 198 25 C 244 38 254 90 250 152 Z";
const TRUCKER_PANEL: &str = "M 150 22 C 120 31 92 70 85 141 L 215 141 C 208 70 180 31 150 22 Z";
const TRUCKER_LINES: &[&str] =
    &["M 150 22 C 120 31 92 70 85 141", "M 150 22 C 180 31 208 70 215 141", "M 54 168 C 92 154 208 154 246 168", "M 60 163 C 96 150 204 150 240 163"];

// A dad cap: unstructured, a low rounded crown and a curved bill.
const DAD_CROWN: &str = "M 54 150 C 50 116 62 76 106 63 C 127 57 173 57 194 63 C 238 76 250 116 246 150 Z";
const DAD_BUTTON: &str = "M 143 60 A 7 4 0 1 0 157 60 A 7 4 0 1 0 143 60 Z";
const DAD_LINES: &[&str] = &[
    "M 150 60 C 122 67 96 96 88 141",
    "M 150 60 C 178 67 204 96 212 141",
    "M 97 94 A 3 3 0 1 0 103 94 A 3 3 0 1 0 97 94 Z",
    "M 197 94 A 3 3 0 1 0 203 94 A 3 3 0 1 0 197 94 Z",
    "M 54 168 C 92 154 208 154 246 168",
    "M 60 163 C 96 150 204 150 240 163",
];
const DAD_PLACES: &[(&str, Anchor, f64)] = &[("cap-front", Anchor::Bottom(150.0, 128.0), 100.0)];

/// The products the visualizer draws.
const PRODUCTS: &str = "tshirt, polo, hoodie, cap, snapback, trucker, dadcap, tote or towel";

const TOTE: &str = "M 20 160 L 400 160 L 400 550 L 20 550 Z";
const TOTE_LINES: &[&str] = &["M 100 160 C 100 10 200 10 200 160", "M 220 160 C 220 10 320 10 320 160", "M 20 185 L 400 185"];
const TOTE_PLACES: &[(&str, Anchor, f64)] = &[("full-front", Anchor::Centre(210.0, 370.0), 300.0)];

const TOWEL: &str = "M 20 20 L 520 20 L 520 920 L 20 920 Z";
const TOWEL_LINES: &[&str] = &["M 20 760 L 520 760", "M 20 820 L 520 820", "M 20 900 L 520 900", "M 20 40 L 520 40"];
const TOWEL_PLACES: &[(&str, Anchor, f64)] = &[("full-front", Anchor::Bottom(270.0, 740.0), 400.0)];

fn product(name: &str, back: bool) -> Option<Product> {
    // the necklines' lowest points: the curves' middles (front 80 mm, back 39, the polo's 50)
    let garment =
        |body, lines, neck| Product { size: (820.0, 740.0), body, lines, placements: GARMENT_PLACES, default: "left-chest", neck: Some(neck) };
    let cap = |body, lines, placements| Product { size: (300.0, 200.0), body, lines, placements, default: "cap-front", neck: None };
    Some(match name {
        "tshirt" if back => garment(&[TEE_BACK], &TEE_LINES[1..], 38.75),
        "tshirt" => garment(&[TEE], TEE_LINES, 80.0),
        "polo" if back => garment(&[TEE_BACK], &POLO_LINES[3..], 38.75),
        "polo" => garment(
            &["M 320 20 C 350 60 470 60 500 20 L 640 50 L 800 250 L 690 330 L 660 300 L 660 730 L 160 730 L 160 300 L 130 330 L 20 250 L 180 50 Z"],
            POLO_LINES,
            50.0,
        ),
        "hoodie" => Product {
            size: (820.0, 860.0),
            body: &[HOOD, HOODIE],
            lines: HOODIE_LINES,
            placements: HOODIE_PLACES,
            default: "full-front",
            neck: Some(162.5),
        },
        "cap" => cap(&[CAP_CROWN, CAP_BRIM, CAP_BUTTON], CAP_LINES, CAP_PLACES),
        "snapback" => cap(&[SNAP_CROWN, SNAP_BILL, HIGH_BUTTON], SNAP_LINES, SNAP_PLACES),
        "trucker" => cap(&[TRUCKER_CROWN, CAP_BRIM, HIGH_BUTTON], TRUCKER_LINES, SNAP_PLACES),
        "dadcap" => cap(&[DAD_CROWN, CAP_BRIM, DAD_BUTTON], DAD_LINES, DAD_PLACES),
        "tote" => Product { size: (420.0, 570.0), body: &[TOTE], lines: TOTE_LINES, placements: TOTE_PLACES, default: "full-front", neck: None },
        "towel" => Product { size: (540.0, 940.0), body: &[TOWEL], lines: TOWEL_LINES, placements: TOWEL_PLACES, default: "full-front", neck: None },
        _ => return None,
    })
}

/// The top left (mm) of a `w` × `h` box at `anchor`.
fn anchored(anchor: Anchor, w: f64, h: f64) -> (f64, f64) {
    match anchor {
        Anchor::Centre(x, y) => (x - w / 2.0, y - h / 2.0),
        Anchor::Top(x, y) => (x - w / 2.0, y),
        Anchor::Bottom(x, y) => (x - w / 2.0, y - h),
    }
}

/// How tall a placement's usual work area is (mm), given how wide.
fn area_height(product: &str, placement: &str, widest: f64) -> f64 {
    match (product, placement) {
        (_, "yoke") => 60.0,
        ("dadcap", _) => 40.0,
        (_, "cap-front") => 55.0,
        ("towel", _) => 120.0,
        _ => widest,
    }
}

/// A trucker cap's mesh: small rings on a staggered grid over the crown's sides, outside its front
/// panel and above the bill, moved by `(dx, dy)` mm.
fn mesh(dx: f64, dy: f64) -> Result<vectorcraft_geom::path::PathData> {
    use kurbo::Shape;
    let parse = |d: &str| kurbo::BezPath::from_svg(d).map_err(|e| EngineError::Other(format!("product outline: {e}")));
    let (crown, panel) = (parse(TRUCKER_CROWN)?, parse(TRUCKER_PANEL)?);
    let mut rings = kurbo::BezPath::new();
    let step = 5.0;
    for row in 0..26_u32 {
        let y = 24.0 + f64::from(row) * step * 0.87;
        for col in 0..46_u32 {
            let x = 40.0 + f64::from(col) * step + if row % 2 == 1 { step / 2.0 } else { 0.0 };
            let at = kurbo::Point::new(x, y);
            // inside the crown with room for the ring, off the panel and above the bill
            let inside = [(-1.6, 0.0), (1.6, 0.0), (0.0, -1.6)].iter().all(|(ox, oy)| crown.contains(at + kurbo::Vec2::new(*ox, *oy)));
            if y < 138.0 && inside && !panel.contains(at) {
                rings.extend(kurbo::Circle::new(at, 1.1).path_elements(0.05));
            }
        }
    }
    let rings = kurbo::Affine::scale(PT_PER_MM) * kurbo::Affine::translate((dx, dy)) * rings;
    Ok(vectorcraft_geom::path::PathData::from_bezpath(&rings))
}

/// What a product and a placement are called on the caption.
fn spoken(id: &str) -> &str {
    match id {
        "tshirt" => "T-shirt",
        "polo" => "polo shirt",
        "tote" => "tote bag",
        "snapback" => "snapback cap",
        "trucker" => "trucker cap",
        "dadcap" => "dad cap",
        "left-chest" => "left chest",
        "right-chest" => "right chest",
        "full-front" => "full front",
        "full-back" => "full back",
        "cap-front" => "cap front",
        "yoke" => "back yoke",
        other => other,
    }
}

/// `svg` (mm) as a path in page space: scaled to points and moved by `(dx, dy)` mm.
fn svg_path(svg: &str, dx: f64, dy: f64) -> Result<vectorcraft_geom::path::PathData> {
    let bp = kurbo::BezPath::from_svg(svg).map_err(|e| EngineError::Other(format!("product outline: {e}")))?;
    let bp = kurbo::Affine::scale(PT_PER_MM) * kurbo::Affine::translate((dx, dy)) * bp;
    Ok(vectorcraft_geom::path::PathData::from_bezpath(&bp))
}

/// A path drawn as `appearance` on the first layer.
fn add_path(doc: &mut Document, path: vectorcraft_geom::path::PathData, appearance: Appearance) -> Result<()> {
    let n = vectorcraft_doc::Node::path(doc.alloc_id(), path, appearance);
    let layer = doc.layers.first().map(|l| l.id);
    doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    Ok(())
}

/// A shade of `c` (`k` < 1 darker), for seams on the product's colour.
fn shade(c: sf_core::Rgb, k: f64) -> Color {
    let f = |v: u8| (f64::from(v) * k).round().clamp(0.0, 255.0) as u8;
    Color::rgb8(f(c.r), f(c.g), f(c.b))
}

fn visualize(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.visualize";
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    if plan.is_empty() {
        return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type first"));
    }
    let pattern = plan.pattern().clone();
    let title = super::fileio::file_stem(&s.doc()?.doc.title);
    let b = pattern.bounds();
    let (w, h) = (b.width(), b.height());
    let image = str_param(p, "image").map(str::trim).filter(|i| !i.is_empty()).map(str::to_string);
    let name = str_param(p, "product").unwrap_or("tshirt");
    let color = match str_param(p, "color") {
        None => sf_core::Rgb::new(0xff, 0xff, 0xff),
        Some(c) => sf_core::Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?,
    };
    let mut warnings = Vec::new();
    let (mut areas, mut below_neck) = (0, None);
    // where the design's top left goes (mm), the page's size (mm) and what is drawn under it
    let (doc, page, origin, placement, view) = if let Some(path) = &image {
        let width = match p.get("imageWidth") {
            None | Some(Value::Null) => 500.0,
            Some(v) => v.as_f64().filter(|v| (50.0..=3000.0).contains(v)).ok_or_else(|| bad(CMD, "`imageWidth` is 50 to 3000 mm"))?,
        };
        let info = s.execute("file.place.info", &json!({ "path": path }))?;
        let (iw, ih) = (info["width"].as_f64().unwrap_or(0.0), info["height"].as_f64().unwrap_or(0.0));
        if !(iw.is_finite() && ih.is_finite() && iw > 0.0 && ih > 0.0) {
            return Err(bad(CMD, "the image has no size"));
        }
        let height = width * ih / iw;
        let at = match p.get("at") {
            None | Some(Value::Null) => (width / 2.0, height / 2.0),
            Some(v) => {
                let pt = v.as_array().filter(|a| a.len() == 2).and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
                pt.filter(|(x, y)| x.is_finite() && y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y] in mm"))?
            }
        };
        let mut doc = Document::new(width * PT_PER_MM, height * PT_PER_MM);
        doc.title = format!("{title} mock-up");
        (doc, (width, height), (at.0 - w / 2.0, at.1 - h / 2.0), "photo".to_string(), "photo")
    } else {
        let placement = str_param(p, "placement");
        let back = matches!(placement, Some("full-back" | "yoke"));
        let prod = product(name, back).ok_or_else(|| bad(CMD, format!("`product` is {PRODUCTS}")))?;
        let placement = placement.unwrap_or(prod.default);
        let Some((_, anchor, widest)) = prod.placements.iter().find(|(id, ..)| *id == placement) else {
            let ids: Vec<&str> = prod.placements.iter().map(|(id, ..)| *id).collect();
            return Err(bad(CMD, format!("on a {name}, `placement` is {}", ids.join(", "))));
        };
        if w > *widest {
            warnings.push(format!("{w:.0} mm wide: designs there are usually at most {widest:.0} mm"));
        }
        let (pw, ph) = (prod.size.0 + 2.0 * MARGIN, prod.size.1 + 2.0 * MARGIN + 20.0);
        let mut doc = Document::new(pw * PT_PER_MM, ph * PT_PER_MM);
        doc.title = format!("{title} on {}", spoken(name));
        let fill = Color::rgb8(color.r, color.g, color.b);
        // a light product gets a darker outline, a dark one a lighter
        let light = u32::from(color.r) + u32::from(color.g) + u32::from(color.b) > 3 * 90;
        let ink =
            if light { shade(color, 0.62) } else { Color::rgb8(color.r.saturating_add(70), color.g.saturating_add(70), color.b.saturating_add(70)) };
        for body in prod.body {
            let path = svg_path(body, MARGIN, MARGIN)?;
            add_path(&mut doc, path, Appearance::basic(Paint::solid(fill), Paint::solid(ink), 1.2 * PT_PER_MM))?;
        }
        for line in prod.lines {
            let path = svg_path(line, MARGIN, MARGIN)?;
            add_path(&mut doc, path, Appearance::basic(Paint::None, Paint::solid(ink), 0.8 * PT_PER_MM))?;
        }
        if name == "trucker" {
            add_path(&mut doc, mesh(MARGIN, MARGIN)?, Appearance::basic(Paint::None, Paint::solid(ink), 0.4 * PT_PER_MM))?;
        }
        // a pocket drawn round the design
        if placement == "pocket" && matches!(name, "tshirt" | "polo") {
            let path = svg_path("M 440 165 L 560 165 L 560 305 C 520 320 480 320 440 305 Z", MARGIN, MARGIN)?;
            add_path(&mut doc, path, Appearance::basic(Paint::solid(fill), Paint::solid(ink), 0.8 * PT_PER_MM))?;
        }
        let (ax, ay) = anchored(*anchor, w, h);
        // the usual work areas of the side shown, dashed and named (a pocket shows only when chosen)
        if bool_or(p, "areas", false) {
            let blue = Color::rgb8(0x2a, 0x6f, 0xdb);
            for (id, at, widest) in prod.placements {
                if matches!(*id, "full-back" | "yoke") != back || *id == "pocket" {
                    continue;
                }
                let ah = area_height(name, id, *widest);
                let (x, y) = anchored(*at, *widest, ah);
                let (x, y) = (x + MARGIN, y + MARGIN);
                let rect = format!("M {x} {y} L {} {y} L {} {} L {x} {} Z", x + widest, x + widest, y + ah, y + ah);
                let mut look = Appearance::basic(Paint::None, Paint::solid(blue), 0.6 * PT_PER_MM);
                if let Some(st) = look.stroke_mut() {
                    st.dash = Some(vectorcraft_doc::appearance::Dash {
                        pattern: vec![4.0 * PT_PER_MM, 3.0 * PT_PER_MM],
                        offset: 0.0,
                        align_corners: false,
                    });
                }
                add_path(&mut doc, svg_path(&rect, 0.0, 0.0)?, look)?;
                let label = format!("{} {widest:.0} × {ah:.0} mm", spoken(id));
                // named inside its bottom left corner: areas that start level (a chest and the
                // full front) keep their names apart
                sheet_text(&mut doc, (x + 2.0) * PT_PER_MM, (y + ah - 3.0) * PT_PER_MM, &label, 11.0, blue)?;
                areas += 1;
            }
        }
        let mut caption = format!("{title} on a {}, {}: {w:.0} × {h:.0} mm, life size", spoken(name), spoken(placement));
        // how far below the neckline it starts, as placements are measured on shirts
        if let Some(neck) = prod.neck {
            let below = ((ay - neck) * 10.0).round() / 10.0;
            caption.push_str(&format!(", {below:.0} mm below the neckline"));
            below_neck = Some(below);
        }
        sheet_text(&mut doc, MARGIN * PT_PER_MM, (ph - 14.0) * PT_PER_MM, &caption, 24.0, Color::rgb8(0x44, 0x44, 0x44))?;
        (doc, (pw, ph), (ax + MARGIN, ay + MARGIN), placement.to_string(), if back { "back" } else { "front" })
    };
    // the stitches, life size, in their thread colours
    let mut doc = doc;
    for (i, runs) in super::embroidery::stitch_runs(&pattern).iter().enumerate() {
        use vectorcraft_geom::path::{PathData, SubPath};
        let subpaths: Vec<SubPath> = runs
            .iter()
            .filter(|r| r.len() >= 2)
            .map(|r| {
                let pts: Vec<kurbo::Point> =
                    r.iter().map(|q| kurbo::Point::new((q.x - b.min.x + origin.0) * PT_PER_MM, (q.y - b.min.y + origin.1) * PT_PER_MM)).collect();
                SubPath::polyline(&pts, false)
            })
            .collect();
        if subpaths.is_empty() {
            continue;
        }
        let c = pattern.thread(i).color;
        add_path(&mut doc, PathData::new(subpaths), Appearance::basic(Paint::None, Paint::solid(Color::rgb8(c.r, c.g, c.b)), 0.35 * PT_PER_MM))?;
    }
    let (wpt, hpt) = (page.0 * PT_PER_MM, page.1 * PT_PER_MM);
    s.add_document(doc, None);
    if let Some(path) = &image {
        // the photo under the design, filling the page
        s.execute("file.place", &json!({ "path": path, "rect": [0.0, 0.0, wpt, hpt] }))?;
        s.execute("object.arrange.sendToBack", &json!({}))?;
        s.execute("select.none", &json!({}))?;
    }
    Ok(json!({
        "product": if image.is_some() { "photo" } else { name },
        "placement": placement,
        "view": view,
        "widthMm": (w * 10.0).round() / 10.0,
        "heightMm": (h * 10.0).round() / 10.0,
        "belowNeckMm": below_neck,
        "areas": areas,
        "warnings": warnings,
    }))
}

fn background(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.background";
    let path =
        str_param(p, "path").map(str::trim).filter(|p| !p.is_empty()).ok_or_else(|| bad(CMD, "missing `path`: the garment's photo"))?.to_string();
    let width = match p.get("width") {
        None | Some(Value::Null) => 500.0,
        Some(v) => v.as_f64().filter(|v| (50.0..=3000.0).contains(v)).ok_or_else(|| bad(CMD, "`width` is 50 to 3000 mm"))?,
    };
    let info = s.execute("file.place.info", &json!({ "path": path }))?;
    let (iw, ih) = (info["width"].as_f64().unwrap_or(0.0), info["height"].as_f64().unwrap_or(0.0));
    if !(iw.is_finite() && ih.is_finite() && iw > 0.0 && ih > 0.0) {
        return Err(bad(CMD, "the image has no size"));
    }
    let height = width * ih / iw;
    // its middle on the design's (else the first artboard's)
    let at = match p.get("at") {
        None | Some(Value::Null) => {
            let b = s.embroidery_plan().map(|plan| plan.pattern().bounds()).filter(|b| !b.is_empty());
            match b {
                Some(b) => ((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0),
                None => {
                    let r = s.doc()?.doc.artboards.first().map(|a| a.rect).unwrap_or_default();
                    ((r.x0 + r.x1) / 2.0 / PT_PER_MM, (r.y0 + r.y1) / 2.0 / PT_PER_MM)
                }
            }
        }
        Some(v) => {
            let pt = v.as_array().filter(|a| a.len() == 2).and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
            pt.filter(|(x, y)| x.is_finite() && y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y] in mm"))?
        }
    };
    let rect = [(at.0 - width / 2.0) * PT_PER_MM, (at.1 - height / 2.0) * PT_PER_MM, width * PT_PER_MM, height * PT_PER_MM];
    let r = s.execute("file.place", &json!({ "path": path, "template": true, "rect": rect }))?;
    Ok(json!({ "ids": r["ids"], "widthMm": (width * 10.0).round() / 10.0, "heightMm": (height * 10.0).round() / 10.0 }))
}
