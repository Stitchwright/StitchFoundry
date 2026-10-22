//! In-the-hoop projects (K2, K3): small sewn pieces made in the hoop step by step, each step its
//! own thread named for what to do, so the machine stops and says it. Pillow fronts for the little
//! seasonal bench pillows (an 8″ square or 5½ × 9½″ form, the embroidered panel from a 5×7 hoop
//! with borders sewn on after), and pieces finished in the hoop: a mug rug, a coaster, an
//! ornament, a key fob, a luggage tag and a bookmark (placement, batting and top fabric tacked
//! down, the design, the backing, a satin edge; the key fob and the tag with a loop). The supplies with their cut sizes and the steps are kept as the design's notes, so its
//! worksheet is the step sheet. Original work: shapes, sizes and wording are our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::*;

/// mm in an inch.
const IN: f64 = 25.4;
/// Seam allowance (mm): ¼″.
const SEAM: f64 = IN / 4.0;
/// Room kept between the project and the hoop's sewing field (mm).
const MARGIN: f64 = 6.0;

/// One project: id, name, finished width × height (mm), sewn round (an ornament), and whether the
/// hoop makes a panel that borders finish (else the piece is finished in the hoop).
struct Project {
    id: &'static str,
    name: &'static str,
    finished: (f64, f64),
    round: bool,
    panel: bool,
    /// The loop at the top unless asked otherwise (the key fob's and the tag's).
    looped: bool,
    /// What else it needs, for the supplies.
    extra: &'static str,
}

const PROJECTS: &[Project] = &[
    Project {
        id: "pillow-square",
        name: "Little pillow front, 8″ square",
        finished: (8.0 * IN, 8.0 * IN),
        round: false,
        panel: true,
        looped: false,
        extra: "",
    },
    Project {
        id: "pillow-wide",
        name: "Little pillow front, 9½ × 5½″",
        finished: (9.5 * IN, 5.5 * IN),
        round: false,
        panel: true,
        looped: false,
        extra: "",
    },
    Project { id: "mug-rug", name: "Mug rug, 6½ × 4½″", finished: (6.5 * IN, 4.5 * IN), round: false, panel: false, looped: false, extra: "" },
    Project { id: "coaster", name: "Coaster, 3¾″ square", finished: (3.75 * IN, 3.75 * IN), round: false, panel: false, looped: false, extra: "" },
    Project { id: "ornament", name: "Round ornament, 3½″", finished: (3.5 * IN, 3.5 * IN), round: true, panel: false, looped: false, extra: "" },
    Project {
        id: "key-fob",
        name: "Key fob, 1¼ × 5″",
        finished: (1.25 * IN, 5.0 * IN),
        round: false,
        panel: false,
        looped: true,
        extra: "A 1″ split key ring: thread it onto the ribbon before laying the loop, so it sits in the fold",
    },
    Project {
        id: "luggage-tag",
        name: "Luggage tag, 2¾ × 4½″",
        finished: (2.75 * IN, 4.5 * IN),
        round: false,
        panel: false,
        looped: true,
        extra: "Write your name and phone number on the back, or stitch them as the words; the loop goes round the bag's handle",
    },
    Project {
        id: "bookmark",
        name: "Bookmark, 2 × 6½″",
        finished: (2.0 * IN, 6.5 * IN),
        round: false,
        panel: false,
        looped: false,
        extra: "Thin batting (or none) keeps it flat enough to close the book on",
    },
];

/// Threads for the steps that aren't the design: a colour each, named for what to do.
const PLACEMENT: (&str, &str) = ("#9aa5b1", "Placement: lay batting, then top fabric over the line");
const TRIM_LINE: (&str, &str) = ("#9aa5b1", "Trim line for the panel");
const BACKING: (&str, &str) = ("#7b8794", "Backing: tape the backing fabric under the hoop");
const EDGE: (&str, &str) = ("#1f2933", "Edge: satin all round");
const FRAME: (&str, &str) = ("#1f2933", "Frame");
const VINYL: (&str, &str) = ("#b9c4cc", "Vinyl: lay clear vinyl over the piece and tape it");
const RIBBON_MARK: (&str, &str) = ("#c9a227", "Ribbon: lay it over the mark and tape it");
const RIBBON_TACK: (&str, &str) = ("#a8862a", "Ribbon: tacked down");
const BUTTON: (&str, &str) = ("#6b4f3a", "Button: the spot to sew it on by hand");

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.ith.projects",
            "In-the-Hoop Projects",
            [],
            None,
            "{} the in-the-hoop projects → {projects: [{id, name, widthMm, heightMm, panel (borders finish it) }]}",
            always,
            projects
        ),
        cmd!(
            "embroidery.ith.make",
            "Make In-the-Hoop Project",
            [],
            None,
            "{project: pillow-square|pillow-wide|mug-rug|coaster|ornament|key-fob|luggage-tag|bookmark, hoop?: a hoop's name (else the smallest of My Machine's, or the usual ones, that it fits), text?: words, height?: mm (the words), shape?: Heart|Star|… (embroidery.addShape's), color?: \"#rrggbb\" (the design's thread), ribbon?: bool (a hanging loop at the top of a piece finished in the hoop, on by default for the key fob and the luggage tag; on a pillow front, a ribbon band across the panel below the design, sewn along both its edges), vinyl?: bool (pieces finished in the hoop: clear vinyl over the top, tacked with long stitches, wipe-clean), button?: bool (a small cross marking where to sew a button by hand)} a new design: the project's steps in sewing order, each its own thread named for what to do (the machine stops between them), the words and shape in the middle; the supplies with cut sizes and the steps become the design's notes (its worksheet is the step sheet) → {project, hoop, steps: [{step, thread, what}], supplies: [text], pieces: [{name, widthMm, heightMm}], notes}",
            always,
            make
        ),
        cmd!(
            "embroidery.ith.templates",
            "In-the-Hoop Cutting Templates",
            [],
            None,
            "{…the same as embroidery.ith.make} cutting templates for the project's pieces (fabric, batting, backing, vinyl, the trimmed panel, borders, the envelope back): a new Letter document, each piece life size with its name and size, as many pages as they need (a piece bigger than a page drawn smaller, its size written); print at 100% → {pages, pieces}",
            always,
            templates
        ),
    ]
}

fn projects(_s: &mut Session, _p: &Value) -> Result<Value> {
    Ok(json!({
        "projects": PROJECTS.iter().map(|p| json!({
            "id": p.id, "name": p.name, "widthMm": p.finished.0, "heightMm": p.finished.1, "panel": p.panel,
        })).collect::<Vec<_>>()
    }))
}

/// A length in inches to the nearest eighth, written with a fraction (`4⅝″`).
fn inches(mm: f64) -> String {
    let eighths = (mm / IN * 8.0).round().max(0.0) as u64;
    let (whole, part) = (eighths / 8, eighths % 8);
    let frac = ["", "⅛", "¼", "⅜", "½", "⅝", "¾", "⅞"].get(part as usize).copied().unwrap_or("");
    match (whole, frac) {
        (0, "") => "0″".into(),
        (0, f) => format!("{f}″"),
        (w, f) => format!("{w}{f}″"),
    }
}

/// `w × h` in inches and millimetres.
fn size(w: f64, h: f64) -> String {
    format!("{} × {} ({w:.0} × {h:.0} mm)", inches(w), inches(h))
}

/// The hoop to make the project in: `hoop` if given, else the smallest of My Machine's hoops (or
/// the usual ones) whose field holds `need` either way round. → (newDesign's `hoop`, field w, h).
pub(super) fn pick_hoop(s: &mut Session, p: &Value, need: (f64, f64), cmd: &str) -> Result<(Value, f64, f64)> {
    let fits = |w: f64, h: f64| (w >= need.0 && h >= need.1) || (w >= need.1 && h >= need.0);
    if let Some(name) = str_param(p, "hoop") {
        let hoop = super::embroidery_hoops::find_hoop(s, name).ok_or_else(|| bad(cmd, format!("unknown hoop `{name}`")))?;
        return Ok((json!(hoop.name), hoop.width, hoop.height));
    }
    let mine = s.execute("embroidery.myMachine", &json!({}))?;
    let mut hoops: Vec<(String, f64, f64)> = mine["hoops"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|h| Some((h["name"].as_str()?.to_string(), h["width"].as_f64()?, h["height"].as_f64()?)))
        .collect();
    if hoops.is_empty() {
        hoops = ["Brother 100 × 100 (4×4)", "Brother 130 × 180 (5×7)", "Brother 160 × 260 (6×10)", "Brother 200 × 200 (8×8)"]
            .iter()
            .filter_map(|n| super::embroidery_hoops::find_hoop(s, n).map(|h| (h.name.clone(), h.width, h.height)))
            .collect();
    }
    hoops.sort_by(|a, b| (a.1 * a.2).total_cmp(&(b.1 * b.2)));
    let (name, w, h) =
        hoops.into_iter().find(|(_, w, h)| fits(*w, *h)).ok_or_else(|| bad(cmd, "none of the hoops is big enough: name a larger `hoop`"))?;
    Ok((json!(name), w, h))
}

/// Selects `id` and runs `cmd` with `q` on it.
fn on(s: &mut Session, id: u64, cmd: &str, q: Value) -> Result<Value> {
    s.execute("select.set", &json!({ "ids": [id] }))?;
    s.execute(cmd, &q)
}

/// A rectangle (or circle) `w` × `h` mm round the page's middle → its id.
fn outline(s: &mut Session, c: kurbo::Point, w: f64, h: f64, round: bool) -> Result<u64> {
    let (wp, hp) = (w * PT_PER_MM, h * PT_PER_MM);
    let q = json!({ "x": c.x - wp / 2.0, "y": c.y - hp / 2.0, "width": wp, "height": hp });
    let r = s.execute(if round { "shape.ellipse" } else { "shape.rectangle" }, &q)?;
    r["id"].as_u64().ok_or_else(|| bad("embroidery.ith.make", "the outline wasn't made"))
}

/// A step: its object gets stitch type `ty`, `params`, and the step's thread (its own colour block).
fn step(s: &mut Session, id: u64, ty: &str, params: Value, thread: (&str, &str)) -> Result<()> {
    on(s, id, "embroidery.set", json!({ "type": ty }))?;
    let mut all = params;
    all["own_block"] = json!(true);
    s.execute("embroidery.setParams", &json!({ "params": all }))?;
    s.execute("embroidery.setThread", &json!({ "color": thread.0, "name": thread.1, "ids": [id], "paint": false }))?;
    Ok(())
}

fn make(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.ith.make";
    let id = str_param(p, "project").ok_or_else(|| bad(CMD, "missing `project`: see embroidery.ith.projects"))?;
    let pr = PROJECTS.iter().find(|x| x.id == id).ok_or_else(|| bad(CMD, format!("no project `{id}`: see embroidery.ith.projects")))?;
    let text = str_param(p, "text").map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let shape = str_param(p, "shape").map(str::to_string);
    let color = str_param(p, "color").unwrap_or("#8a2432").to_string();
    sf_core::Rgb::from_hex(&color).ok_or_else(|| bad(CMD, "`color` is \"#rrggbb\""))?;
    let (ribbon, vinyl, button) = (bool_or(p, "ribbon", pr.looped), bool_or(p, "vinyl", false), bool_or(p, "button", false));
    if vinyl && pr.panel {
        return Err(bad(CMD, "vinyl is for the pieces finished in the hoop, not the pillow fronts"));
    }
    // a panel needs a 5×7-class hoop; a finished piece its own size
    let need = if pr.panel { (118.0 + 2.0 * MARGIN, 160.0 + 2.0 * MARGIN) } else { (pr.finished.0 + 2.0 * MARGIN, pr.finished.1 + 2.0 * MARGIN) };
    let (hoop, fw, fh) = pick_hoop(s, p, need, CMD)?;
    let (long, short) = (fw.max(fh) - 2.0 * MARGIN, fw.min(fh) - 2.0 * MARGIN);
    // the piece sewn in the hoop: a panel as big as the field allows (no bigger than the front),
    // or the finished piece itself
    let landscape = pr.finished.0 >= pr.finished.1;
    let (pw, ph) = if pr.panel {
        if (pr.finished.0 - pr.finished.1).abs() < 1.0 {
            let side = short.min(pr.finished.0);
            (side, side)
        } else {
            let (a, b) = (long.min(pr.finished.0.max(pr.finished.1)), short.min(pr.finished.0.min(pr.finished.1)));
            if landscape { (a, b) } else { (b, a) }
        }
    } else {
        pr.finished
    };
    // the hoop turned to the piece's way round
    let hoop = if (pw > ph) != (fw > fh) && (pw - ph).abs() > 1.0 { json!({ "width": fh, "height": fw }) } else { hoop };
    let made = s.execute("embroidery.newDesign", &json!({ "hoop": hoop }))?;
    let c = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    let mut steps: Vec<Value> = Vec::new();
    let mut add_step = |thread: &str, what: &str| {
        steps.push(json!({ "step": steps.len() + 1, "thread": thread, "what": what }));
    };
    // 1: where the piece goes
    if pr.panel {
        let o = outline(s, c, pw, ph, pr.round)?;
        step(s, o, "run", json!({}), TRIM_LINE)?;
        add_step(TRIM_LINE.1, "Hoop the top fabric with tear-away stabilizer. This line marks the panel; trim on it after.");
    } else {
        let o = outline(s, c, pw, ph, pr.round)?;
        step(s, o, "applique", json!({ "placement": true, "tackdown": "Run", "trim_stop": false, "cover": "None" }), PLACEMENT)?;
        add_step(
            PLACEMENT.1,
            "Hoop cut-away stabilizer and sew the line. Lay the batting, then the top fabric right side up over it; the machine tacks them down.",
        );
    }
    // 2: the design in the middle (above the words when both). A narrow piece (a key fob, a
    // bookmark) is sized by its width, its words turned to run along it.
    let narrow = pw < ph * 0.5;
    let inner = (pw.min(ph) - 24.0).max(10.0);
    let both = text.is_some() && shape.is_some();
    if let Some(name) = &shape {
        let size = match (narrow, both) {
            (true, _) => (pw - 14.0).max(8.0),
            (false, true) => inner * 0.5,
            (false, false) => inner * 0.7,
        };
        let r = s.execute("embroidery.addShape", &json!({ "name": name, "size": size }))?;
        if both && let Some(sid) = r["id"].as_u64() {
            let dy = if narrow { -(ph * 0.25) } else { -inner * 0.18 };
            on(s, sid, "object.move", json!({ "dx": 0.0, "dy": dy * PT_PER_MM }))?;
        }
        add_step("The design", "Sew the shape in its colours.");
    }
    if let Some(t) = &text {
        let height = match p.get("height").and_then(Value::as_f64) {
            Some(h) => h.clamp(5.0, 80.0),
            None if narrow => ((pw - 14.0) * 0.55).clamp(6.0, 40.0),
            None => (inner * if both { 0.18 } else { 0.28 }).clamp(6.0, 40.0),
        };
        let r = s.execute("embroidery.addWords", &json!({ "text": t, "height": height, "color": color }))?;
        if let Some(wid) = r["id"].as_u64() {
            if narrow {
                // reading upwards, the way a strip is held; kept within the piece's length
                on(s, wid, "object.rotate", json!({ "angle": 90.0 }))?;
                let room = ph * if both { 0.45 } else { 0.7 };
                let long = {
                    let d = &s.doc()?.doc;
                    d.node(NodeId(wid)).and_then(|n| n.geometric_bounds()).map_or(0.0, |b| b.height() / PT_PER_MM)
                };
                if long > room && long > 0.0 {
                    let k = room / long * 100.0;
                    on(s, wid, "object.scale", json!({ "sx": k, "sy": k }))?;
                }
                if both {
                    on(s, wid, "object.move", json!({ "dx": 0.0, "dy": ph * 0.15 * PT_PER_MM }))?;
                }
            } else if both {
                on(s, wid, "object.move", json!({ "dx": 0.0, "dy": inner * 0.3 * PT_PER_MM }))?;
            }
        }
        add_step("The words", "Sew the words.");
    }
    // the extras: a ribbon band across a panel, vinyl over a finished piece, a button mark
    let line = |s: &mut Session, pts: &[(f64, f64)]| -> Result<u64> {
        let mut d = String::new();
        for (i, (x, y)) in pts.iter().enumerate() {
            d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, c.x + x * PT_PER_MM, c.y + y * PT_PER_MM));
        }
        s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(CMD, "a step wasn't made"))
    };
    const RIBBON_W: f64 = 16.0;
    if ribbon && pr.panel {
        // a band below the design: where its top edge goes, then both its edges sewn down
        let (x0, x1, y) = (-pw / 2.0, pw / 2.0, ph / 2.0 - 16.0 - RIBBON_W);
        let mark = line(s, &[(x0, y), (x1, y)])?;
        step(s, mark, "run", json!({}), RIBBON_MARK)?;
        add_step(RIBBON_MARK.1, "Lay the ribbon across the panel with its top edge on the line, ends past the trim line; tape the ends.");
        let tack = line(s, &[(x1, y + 1.5), (x0, y + 1.5), (x0, y + RIBBON_W - 1.5), (x1, y + RIBBON_W - 1.5)])?;
        step(s, tack, "run", json!({ "passes": 2 }), RIBBON_TACK)?;
        add_step(RIBBON_TACK.1, "The machine sews the ribbon down along both its edges.");
    }
    if vinyl {
        let v = outline(s, c, pw - 4.0, ph - 4.0, pr.round)?;
        step(s, v, "run", json!({ "stitch_length": 4.0 }), VINYL)?;
        add_step(
            VINYL.1,
            "Lay clear vinyl over the whole piece and tape it outside the stitching; the machine tacks it down with long stitches, which don't tear it.",
        );
    }
    if ribbon && !pr.panel {
        // a hanging loop at the top: its ends inside the edge, the loop out beyond it
        let top = -ph / 2.0;
        let mark = line(s, &[(-6.0, top + 14.0), (-6.0, top), (6.0, top), (6.0, top + 14.0)])?;
        step(s, mark, "run", json!({}), RIBBON_MARK)?;
        add_step(
            RIBBON_MARK.1,
            "Fold a 6″ ribbon in half; lay its ends inside the mark and the loop out beyond the top edge, and tape the loop out of the way of the edge.",
        );
        let tack = line(s, &[(-8.0, top + 6.0), (8.0, top + 6.0), (8.0, top + 9.0), (-8.0, top + 9.0)])?;
        step(s, tack, "run", json!({ "passes": 2 }), RIBBON_TACK)?;
        add_step(RIBBON_TACK.1, "The machine tacks the ribbon's ends down; the backing covers them.");
    }
    if button {
        let (bx, by) = (pw / 2.0 - 16.0, ph / 2.0 - 16.0);
        let cross = line(s, &[(bx - 3.0, by - 3.0), (bx + 3.0, by + 3.0), (bx + 3.0, by - 3.0), (bx - 3.0, by + 3.0)])?;
        step(s, cross, "run", json!({ "stitch_length": 1.5 }), BUTTON)?;
        add_step(BUTTON.1, "The machine marks the spot with a small cross; sew a button over it by hand when the piece is done.");
    }
    // 3: finishing in the hoop
    if pr.panel {
        let o = outline(s, c, pw - 16.0, ph - 16.0, pr.round)?;
        step(s, o, "run", json!({ "passes": 3 }), FRAME)?;
        add_step(FRAME.1, "A triple-stitch frame round the design. Take the panel out of the hoop and tear the stabilizer away.");
    } else {
        let back = outline(s, c, pw, ph, pr.round)?;
        step(s, back, "applique", json!({ "placement": false, "tackdown": "Run", "trim_stop": true, "cover": "None" }), BACKING)?;
        add_step(
            BACKING.1,
            "Take the hoop off the machine without unhooping. Tape the backing fabric right side out under the hoop over the piece, put the hoop back; the machine tacks it on, then stops: trim all the layers close to the stitching, front and back.",
        );
        let edge = outline(s, c, pw, ph, pr.round)?;
        step(s, edge, "applique", json!({ "placement": false, "tackdown": "None", "trim_stop": false, "cover": "Satin", "cover_width": 4.0 }), EDGE)?;
        add_step(EDGE.1, "A satin edge all round covers the cut edges. Take it out of the hoop and cut the stabilizer away close to the satin.");
    }
    // the supplies and cut sizes
    let mut supplies = vec![format!("Hoop: {} (field {})", made["hoop"].as_str().unwrap_or("custom"), size(fw, fh))];
    // the pieces to cut, for the cutting templates
    let mut pieces: Vec<(String, f64, f64)> = Vec::new();
    if pr.panel {
        let (w, h) = pr.finished;
        let (side, topb) = (((w - pw) / 2.0).max(0.0), ((h - ph) / 2.0).max(0.0));
        supplies.push(format!("Top fabric for hooping: {}", size(fw + 2.0 * IN, fh + 2.0 * IN)));
        supplies.push("Tear-away stabilizer, the size of the hoop".into());
        supplies.push(format!("Trim the embroidered panel to {} (¼″ seams)", size(pw + 2.0 * SEAM, ph + 2.0 * SEAM)));
        pieces.push(("Embroidered panel, trimmed".into(), pw + 2.0 * SEAM, ph + 2.0 * SEAM));
        if side > 1.0 {
            supplies.push(format!("Side borders: 2 strips {}", size(side + 2.0 * SEAM, ph + 2.0 * SEAM)));
            pieces.push(("Side border (cut 2)".into(), side + 2.0 * SEAM, ph + 2.0 * SEAM));
        }
        if topb > 1.0 {
            supplies.push(format!("Top and bottom borders: 2 strips {}", size(w + 2.0 * SEAM, topb + 2.0 * SEAM)));
            pieces.push(("Top and bottom border (cut 2)".into(), w + 2.0 * SEAM, topb + 2.0 * SEAM));
        }
        let back_h = (h * 0.65 + IN).round();
        supplies.push(format!("Envelope back: 2 pieces {} (hem one long edge of each, overlap them)", size(w + 2.0 * SEAM, back_h)));
        pieces.push(("Envelope back (cut 2)".into(), w + 2.0 * SEAM, back_h));
        if ribbon {
            supplies.push(format!("Ribbon ⅝″ wide, {} long", inches(pw + 2.0 * IN)));
        }
        supplies.push(format!("Pillow form {} finished", size(w, h)));
        add_step(
            "Sew the front",
            "Sew the side borders on, then the top and bottom ones, ¼″ seams; press. The front measures the pillow's size plus ¼″ all round.",
        );
        add_step(
            "Sew the cover",
            "Lay the two hemmed back pieces right side down on the front, overlapping; sew all round with a ¼″ seam, clip the corners, turn out and put the form in.",
        );
    } else {
        supplies.push(format!("Cut-away stabilizer, the size of the hoop; batting {}", size(pw + IN, ph + IN)));
        supplies.push(format!("Top fabric {}; backing fabric {}", size(pw + IN, ph + IN), size(pw + IN, ph + IN)));
        supplies.push(format!("Finished size {}", size(pw, ph)));
        let cut = (pw + IN, ph + IN);
        for name in ["Top fabric", "Batting", "Backing"] {
            pieces.push((name.into(), cut.0, cut.1));
        }
        if vinyl {
            supplies.push(format!("Clear vinyl {}", size(cut.0, cut.1)));
            pieces.push(("Clear vinyl".into(), cut.0, cut.1));
        }
        if ribbon {
            supplies.push("Ribbon ⅜″ wide, 6″ long, for the hanging loop".into());
        }
        if !pr.extra.is_empty() {
            supplies.push(pr.extra.into());
        }
    }
    let notes: String = std::iter::once(pr.name.to_string())
        .chain(std::iter::once(String::new()))
        .chain(std::iter::once("You need:".to_string()))
        .chain(supplies.iter().map(|x| format!("- {x}")))
        .chain(std::iter::once(String::new()))
        .chain(std::iter::once("Steps:".to_string()))
        .chain(
            steps
                .iter()
                .map(|st| format!("{}. {}: {}", st["step"], st["thread"].as_str().unwrap_or_default(), st["what"].as_str().unwrap_or_default())),
        )
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(2000)
        .collect();
    s.execute("embroidery.settings", &json!({ "notes": notes }))?;
    s.execute("select.none", &json!({}))?;
    if button {
        supplies.push("A button, ½″ to ¾″".into());
    }
    let pieces: Vec<Value> = pieces.into_iter().map(|(name, w, h)| json!({ "name": name, "widthMm": w, "heightMm": h })).collect();
    Ok(json!({ "project": pr.id, "hoop": made["hoop"], "steps": steps, "supplies": supplies, "pieces": pieces, "notes": notes }))
}

/// Cutting templates: the project made to find its pieces, then a Letter document with each piece
/// life size (or drawn smaller when bigger than a page), its name and size written in it.
fn templates(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.ith.templates";
    let made = s.execute("embroidery.ith.make", p)?;
    // the design was only made to measure the pieces
    s.execute("file.close", &json!({}))?;
    let pieces: Vec<(String, f64, f64)> = made["pieces"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Some((v["name"].as_str()?.to_string(), v["widthMm"].as_f64()?, v["heightMm"].as_f64()?)))
        .collect();
    if pieces.is_empty() {
        return Err(bad(CMD, "the project has no pieces to cut"));
    }
    let name = PROJECTS.iter().find(|x| Some(x.id) == str_param(p, "project")).map_or("Project", |x| x.name);
    let (page_w, page_h) = (8.5 * IN, 11.0 * IN);
    let margin = 0.5 * IN;
    let (room_w, room_h) = (page_w - 2.0 * margin, page_h - 2.0 * margin - 10.0);
    s.execute("file.new", &json!({ "name": format!("{name} — cutting templates"), "width": page_w * PT_PER_MM, "height": page_h * PT_PER_MM }))?;
    // shelves of pieces, a new page when one is full
    let mut page = 0usize;
    let (mut x, mut y, mut shelf) = (0.0f64, 0.0f64, 0.0f64);
    let origin = |s: &mut Session, page: usize| -> Result<(f64, f64)> {
        let d = &s.doc()?.doc;
        d.artboards.get(page).map(|a| (a.rect.x0, a.rect.y0)).ok_or_else(|| bad(CMD, "the page wasn't made"))
    };
    let mut at = origin(s, 0)?;
    let title = |s: &mut Session, at: (f64, f64), page: usize| -> Result<()> {
        let t = format!("{name}: cutting templates (page {}). Print at 100%; check the 1″ square.", page + 1);
        s.execute("text.create", &json!({ "x": at.0 + margin * PT_PER_MM, "y": at.1 + (margin - 2.0) * PT_PER_MM, "text": t, "size": 10.0 }))?;
        // a 1″ square to check the print's scale
        s.execute("shape.rectangle", &json!({ "x": at.0 + (page_w - margin - IN) * PT_PER_MM, "y": at.1 + (page_h - margin - IN) * PT_PER_MM, "width": IN * PT_PER_MM, "height": IN * PT_PER_MM }))?;
        s.execute("paint.default", &json!({}))?;
        s.execute("paint.setFill", &json!({ "none": true }))?;
        Ok(())
    };
    title(s, at, 0)?;
    for (piece, w, h) in &pieces {
        // too big for a page: drawn to fit, its true size written
        let k = (room_w / w).min(room_h / h).min(1.0);
        let (dw, dh) = (w * k, h * k);
        if x > 0.0 && x + dw > room_w {
            (x, y, shelf) = (0.0, y + shelf + 6.0, 0.0);
        }
        if y > 0.0 && y + dh > room_h {
            page += 1;
            s.execute("artboard.new", &json!({}))?;
            at = origin(s, page)?;
            title(s, at, page)?;
            (x, y, shelf) = (0.0, 0.0, 0.0);
        }
        let (px, py) = (at.0 + (margin + x) * PT_PER_MM, at.1 + (margin + 10.0 + y) * PT_PER_MM);
        s.execute("shape.rectangle", &json!({ "x": px, "y": py, "width": dw * PT_PER_MM, "height": dh * PT_PER_MM }))?;
        s.execute("paint.default", &json!({}))?;
        s.execute("paint.setFill", &json!({ "none": true }))?;
        let label =
            if k < 1.0 { format!("{piece}\n{}\n(drawn smaller: cut to this size)", size(*w, *h)) } else { format!("{piece}\n{}", size(*w, *h)) };
        s.execute("text.create", &json!({ "x": px + 6.0, "y": py + 14.0, "text": label, "size": 9.0 }))?;
        x += dw + 6.0;
        shelf = shelf.max(dh);
    }
    s.execute("select.none", &json!({}))?;
    Ok(json!({ "pages": page + 1, "pieces": pieces.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// K4: ribbon, vinyl and a button add their steps, threads and supplies; templates lay every
    /// piece out life size on Letter pages.
    #[test]
    fn ribbon_vinyl_and_button_steps_and_cutting_templates() {
        let mut s = Session::new();
        let r =
            s.execute("embroidery.ith.make", &json!({"project": "coaster", "ribbon": true, "vinyl": true, "button": true, "text": "Hi"})).unwrap();
        let threads: Vec<String> = r["steps"].as_array().unwrap().iter().map(|x| x["thread"].as_str().unwrap().to_string()).collect();
        for want in [VINYL.1, RIBBON_MARK.1, RIBBON_TACK.1, BUTTON.1, BACKING.1, EDGE.1] {
            assert!(threads.iter().any(|t| t == want), "{want} in {threads:?}");
        }
        // vinyl before the ribbon, both before the backing and the edge
        let pos = |t: &str| threads.iter().position(|x| x == t).unwrap();
        assert!(pos(VINYL.1) < pos(RIBBON_MARK.1) && pos(RIBBON_TACK.1) < pos(BACKING.1) && pos(BACKING.1) < pos(EDGE.1), "{threads:?}");
        let sup = r["supplies"].to_string();
        assert!(sup.contains("Clear vinyl") && sup.contains("Ribbon") && sup.contains("button"), "{sup}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        // a pillow front's ribbon band; vinyl isn't for panels
        let r = s.execute("embroidery.ith.make", &json!({"project": "pillow-square", "ribbon": true})).unwrap();
        assert!(r["supplies"].to_string().contains("Ribbon ⅝″"), "{r}");
        assert!(s.execute("embroidery.ith.make", &json!({"project": "pillow-square", "vinyl": true})).is_err());
        // templates: a page or more, every piece on it
        let docs = s.documents().len();
        let t = s.execute("embroidery.ith.templates", &json!({"project": "pillow-square", "ribbon": true})).unwrap();
        assert!(t["pages"].as_u64().unwrap() >= 1 && t["pieces"].as_u64().unwrap() >= 4, "{t}");
        assert_eq!(s.documents().len(), docs + 1, "the measuring design closed, the templates open");
        let d = &s.doc().unwrap().doc;
        assert_eq!(d.artboards.len() as u64, t["pages"].as_u64().unwrap());
        assert!(s.execute("embroidery.ith.templates", &json!({"project": "nope"})).is_err());
    }

    /// K2: every project is made in a hoop it fits, its steps each a thread of their own in sewing
    /// order, its supplies and steps kept as the design's notes.
    #[test]
    fn each_project_sews_step_by_step_in_a_hoop_it_fits() {
        let mut s = Session::new();
        let list = s.execute("embroidery.ith.projects", &json!({})).unwrap();
        assert_eq!(list["projects"].as_array().unwrap().len(), PROJECTS.len());
        for pr in PROJECTS {
            let r = s
                .execute("embroidery.ith.make", &json!({"project": pr.id, "text": "Joy", "shape": "Heart"}))
                .unwrap_or_else(|e| panic!("{}: {e}", pr.id));
            let info = s.execute("embroidery.info", &json!({})).unwrap();
            assert_eq!(info["fits"], true, "{}: {info}", pr.id);
            let threads: Vec<String> = info["colors"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap_or("").to_string()).collect();
            if pr.panel {
                assert_eq!(threads.first().map(String::as_str), Some(TRIM_LINE.1), "{}: {threads:?}", pr.id);
                assert_eq!(threads.last().map(String::as_str), Some(FRAME.1), "{}: {threads:?}", pr.id);
            } else {
                assert_eq!(threads.first().map(String::as_str), Some(PLACEMENT.1), "{}: {threads:?}", pr.id);
                assert_eq!(threads.last().map(String::as_str), Some(EDGE.1), "{}: {threads:?}", pr.id);
                assert!(threads.iter().any(|t| t == BACKING.1), "{}: {threads:?}", pr.id);
                // placement, tack-down and trim stops: the machine stops for each layer
                assert!(info["stops"].as_u64().unwrap_or(0) + info["colorChanges"].as_u64().unwrap_or(0) >= 4, "{}: {info}", pr.id);
            }
            assert!(r["steps"].as_array().unwrap().len() >= 4, "{r}");
            assert!(info["notes"].as_str().unwrap().contains("You need:"), "{info}");
        }
        // the 8″ square front: a square panel from a 5×7 hoop, borders to make up the rest
        let r = s.execute("embroidery.ith.make", &json!({"project": "pillow-square", "hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let sup = r["supplies"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>().join("\n");
        assert!(sup.contains("Side borders: 2 strips") && sup.contains("Envelope back"), "{sup}");
        assert!(sup.contains("Pillow form 8″ × 8″"), "{sup}");
        // the key fob and the tag come with their loop, the bookmark without unless asked
        for (id, looped) in [("key-fob", true), ("luggage-tag", true), ("bookmark", false)] {
            let r = s.execute("embroidery.ith.make", &json!({"project": id, "text": "Al"})).unwrap();
            let has_loop = r["steps"].as_array().unwrap().iter().any(|st| st["thread"] == RIBBON_MARK.1);
            assert_eq!(has_loop, looped, "{id}: {r}");
        }
        let r = s.execute("embroidery.ith.make", &json!({"project": "key-fob", "ribbon": false})).unwrap();
        assert!(!r["steps"].as_array().unwrap().iter().any(|st| st["thread"] == RIBBON_MARK.1));
        let sup = s.execute("embroidery.ith.make", &json!({"project": "key-fob"})).unwrap()["supplies"].to_string();
        assert!(sup.contains("key ring"), "{sup}");
        // the wide front turns the hoop landscape
        s.execute("embroidery.ith.make", &json!({"project": "pillow-wide", "hoop": "Brother 130 × 180 (5×7)", "text": "Hello fall"})).unwrap();
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert!(info["widthMm"].as_f64().unwrap() > info["heightMm"].as_f64().unwrap(), "{info}");
        for bad in
            [json!({}), json!({"project": "quilt"}), json!({"project": "coaster", "hoop": "nope"}), json!({"project": "coaster", "color": "red"})]
        {
            assert!(s.execute("embroidery.ith.make", &bad).is_err(), "{bad}");
        }
        assert_eq!(inches(8.0 * IN), "8″");
        assert_eq!(inches(4.625 * IN), "4⅝″");
        assert_eq!(inches(0.25 * IN), "¼″");
    }
}
