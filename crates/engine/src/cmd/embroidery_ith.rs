//! In-the-hoop projects (K2, K3): small sewn pieces made in the hoop step by step, each step its
//! own thread named for what to do, so the machine stops and says it. Pillow fronts for the little
//! seasonal bench pillows (an 8″ square or 5½ × 9½″ form, the embroidered panel from a 5×7 hoop
//! with borders sewn on after), and pieces finished in the hoop: a mug rug, a coaster, an
//! ornament, a key fob, a luggage tag and a bookmark (placement, batting and top fabric tacked
//! down, the design, the backing, a satin edge; the key fob and the tag with a loop; the card
//! holder with a slip pocket folded over its lower half), a phone case and a passport holder (front and
//! back sewn face to face on three sides, turned out through its open top), a book cover (its front panel from the
//! hoop, made up with a back, a lining and flaps), and a zipper pouch and a pencil case
//! (front and top strip tacked either side of the zipper line, the design, the zipper sewn over
//! their edges, the back sewn on face down and the piece turned out through the zipper). The
//! supplies with their cut sizes and the steps are kept as the design's notes, so its worksheet is
//! the step sheet. Original work: shapes, sizes and wording are our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::{CommandSpec, EngineError, NodeId, Result, Session, always, bad, bool_or, cmd, str_param};

/// mm in an inch.
const IN: f64 = 25.4;
/// Seam allowance (mm): ¼″.
const SEAM: f64 = IN / 4.0;
/// Room kept between the project and the hoop's sewing field (mm).
const MARGIN: f64 = 6.0;
/// The small and large sizes, as a share of the project's own.
const SMALL: f64 = 0.8;
const LARGE: f64 = 1.25;

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
    Project {
        id: ZIPPER_POUCH, name: "Zipper pouch, 6 × 4½″", finished: (6.0 * IN, 4.5 * IN), round: false, panel: false, looped: false, extra: ""
    },
    Project {
        id: "pencil-case", name: "Pencil case, 8 × 3½″", finished: (8.0 * IN, 3.5 * IN), round: false, panel: false, looped: false, extra: ""
    },
    Project {
        id: "card-holder",
        name: "Card holder, 4 × 3″",
        finished: (4.0 * IN, 3.0 * IN),
        round: false,
        panel: false,
        looped: false,
        extra: "Thin batting, or fusible fleece, keeps the pocket from bulging with cards in it",
    },
    Project {
        id: PHONE_CASE,
        name: "Phone case, 3½ × 6½″",
        finished: (3.5 * IN, 6.5 * IN),
        round: false,
        panel: false,
        looped: false,
        extra: "Measure the phone in its case: the finished case wants about ¼″ more than its width and ½″ more than its height",
    },
    Project {
        id: PASSPORT,
        name: "Passport holder, 4 × 5¾″",
        finished: (4.0 * IN, 5.75 * IN),
        round: false,
        panel: false,
        looped: false,
        extra: "For a passport of the usual size (125 × 88 mm, about 5 × 3½″), slid in closed through the open top: the holder is ½″ wider and ¾″ taller to leave room for its thickness. Leave out the fleece if the passport fits tight",
    },
    Project {
        id: BOOK_COVER,
        name: "Book cover, for a 5½ × 8½″ book",
        finished: (5.5 * IN, 8.5 * IN),
        round: false,
        panel: true,
        looped: false,
        extra: "",
    },
];

/// The zipper pouch: sewn face down and turned out, so it has its own steps, as the pencil case
/// has.
const ZIPPER_POUCH: &str = "zipper-pouch";
const ZIPPERED: &[&str] = &[ZIPPER_POUCH, "pencil-case"];
/// The phone case: front and back face to face, sewn on three sides and turned out through its
/// open top, whose edges are folded under before they are laid.
const PHONE_CASE: &str = "phone-case";
/// The passport holder: a sleeve made as the phone case is, its size the passport's.
const PASSPORT: &str = "passport-holder";
/// The pieces sewn as the phone case is.
const SLEEVES: &[&str] = &[PHONE_CASE, PASSPORT];
/// The book cover: a panel, like the pillow fronts, made up into a cover with flaps.
const BOOK_COVER: &str = "book-cover";
/// The book cover's spine (mm): a book about ¾″ thick.
const SPINE: f64 = 0.75 * IN;
/// Pieces with a slip pocket over their lower part, and the share of the height it covers: the
/// design goes above it (sewn through the pocket it would close it).
const POCKETS: &[(&str, f64)] = &[("card-holder", 0.55)];
/// How far the zipper's middle is below the pouch's top edge (mm).
const ZIPPER_DOWN: f64 = 25.0;

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
const POUCH_PLACEMENT: (&str, &str) = ("#9aa5b1", "Placement: the pouch and its zipper line");
const POUCH_FRONT: (&str, &str) = ("#9aa5b1", "Front: lay the front fabric below the zipper line");
const POUCH_TOP: (&str, &str) = ("#9aa5b1", "Top strip: lay it above the zipper line");
const ZIPPER: (&str, &str) = ("#c9a227", "Zipper: lay it face up on the line and tape it");
const POUCH_BACK: (&str, &str) = ("#7b8794", "Back: open the zipper, lay the back face down");
const POCKET_MARK: (&str, &str) = ("#c9a227", "Pocket: lay its fold on the line");
const POCKET_TACK: (&str, &str) = ("#a8862a", "Pocket: tacked down");
const CASE_PLACEMENT: (&str, &str) = ("#9aa5b1", "Placement: the case's outline");
const CASE_FRONT: (&str, &str) = ("#9aa5b1", "Front: fold its top under, lay it on the outline");
const CASE_BACK: (&str, &str) = ("#7b8794", "Back: fold its top under, lay it face down");

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.ith.projects",
            "In-the-Hoop Projects",
            [],
            None,
            "{} the in-the-hoop projects → {projects: [{id, name, widthMm, heightMm, panel (borders finish it), sizes (small and large made), edges (rounded, scalloped and round edges made) }]}",
            always,
            projects
        ),
        cmd!(
            "embroidery.ith.make",
            "Make In-the-Hoop Project",
            [],
            None,
            "{project: pillow-square|pillow-wide|mug-rug|coaster|ornament|key-fob|luggage-tag|bookmark|zipper-pouch|pencil-case|card-holder|phone-case|passport-holder|book-cover, hoop?: a hoop's name (else the smallest of My Machine's, or the usual ones, that it fits), text?: words, height?: mm (the words), shape?: Heart|Star|… (embroidery.addShape's), color?: \"#rrggbb\" (the design's thread), ribbon?: bool (a hanging loop at the top of a piece finished in the hoop, on by default for the key fob and the luggage tag; on a pillow front, a ribbon band across the panel below the design, sewn along both its edges), vinyl?: bool (pieces finished in the hoop: clear vinyl over the top, tacked with long stitches, wipe-clean), button?: bool (a small cross marking where to sew a button by hand), size?: small|regular|large (four fifths or five quarters of the project's own size; not the pillow fronts, book cover, phone case or passport holder, which fit what goes in them), edge?: square|rounded|scalloped|round (the outline of a piece finished in the hoop with nothing sewn into its edge: mug rug, coaster, ornament, key fob, luggage tag, bookmark)} a new design: the project's steps in sewing order, each its own thread named for what to do (the machine stops between them), the words and shape in the middle; the supplies with cut sizes and the steps become the design's notes (its worksheet is the step sheet) → {project, hoop, steps: [{step, thread, what}], supplies: [text], pieces: [{name, widthMm, heightMm}], design: [ids of the shape and words], notes}",
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
        "projects": PROJECTS.iter().map(|p| {
            let case = SLEEVES.contains(&p.id);
            let edges = !p.panel && !case && !ZIPPERED.contains(&p.id) && !POCKETS.iter().any(|x| x.0 == p.id);
            json!({
                "id": p.id, "name": p.name, "widthMm": p.finished.0, "heightMm": p.finished.1, "panel": p.panel,
                "sizes": !p.panel && !case, "edges": edges,
            })
        }).collect::<Vec<_>>()
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
fn outline(s: &mut Session, c: kurbo::Point, w: f64, h: f64, edge: Edge) -> Result<u64> {
    let (wp, hp) = (w * PT_PER_MM, h * PT_PER_MM);
    let q = json!({ "x": c.x - wp / 2.0, "y": c.y - hp / 2.0, "width": wp, "height": hp });
    let r = match edge {
        Edge::Square => s.execute("shape.rectangle", &q)?,
        Edge::Round => s.execute("shape.ellipse", &q)?,
        Edge::Rounded => {
            let mut q = q;
            q["radius"] = json!(ROUNDED * w.min(h) * PT_PER_MM);
            s.execute("shape.rectangle", &q)?
        }
        Edge::Scalloped(nx, ny) => s.execute("path.create", &json!({ "d": scallops(c, w, h, nx, ny) }))?,
    };
    r["id"].as_u64().ok_or_else(|| bad("embroidery.ith.make", "the outline wasn't made"))
}

/// A piece's edge: square corners, rounded ones, scallops all round (how many along the width and
/// the height), or round (an ellipse).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Edge {
    Square,
    Rounded,
    Scalloped(usize, usize),
    Round,
}

/// How round a rounded piece's corners are: their radius, a share of its shorter side.
const ROUNDED: f64 = 0.15;
/// About how wide a scallop is (mm).
const SCALLOP: f64 = 18.0;

/// How many scallops go along `w` and `h` (mm): every scallop about as deep as the others.
fn scallop_counts(w: f64, h: f64) -> (usize, usize) {
    let nx = ((w / SCALLOP).round() as usize).clamp(2, 40);
    // each scallop is as deep as half its width; the corners are where two meet
    let depth = w / (2.0 * nx as f64 + 2.0);
    let ny = ((h / (2.0 * depth) - 1.0).round() as usize).clamp(2, 40);
    (nx, ny)
}

/// SVG path data (points) for a w × h (mm) outline round `c` of scallops bulging outwards: `nx`
/// along the top and bottom, `ny` down the sides, the outline exactly w × h.
fn scallops(c: kurbo::Point, w: f64, h: f64, nx: usize, ny: usize) -> String {
    let (nx, ny) = (nx.max(1), ny.max(1));
    // the scallops' depth, the same all round: the corners' arcs meet there
    let depth = (w / (2.0 * nx as f64 + 2.0)).min(h / (2.0 * ny as f64 + 2.0)) * PT_PER_MM;
    let (wp, hp) = (w * PT_PER_MM, h * PT_PER_MM);
    let (l, r, t, b) = (c.x - wp / 2.0 + depth, c.x + wp / 2.0 - depth, c.y - hp / 2.0 + depth, c.y + hp / 2.0 - depth);
    let (sx, sy) = ((r - l) / nx as f64, (b - t) / ny as f64);
    let mut d = format!("M{l:.3} {t:.3}");
    // clockwise on the page (y down): each arc's sweep flag 1 bulges outwards
    for i in 1..=nx {
        d += &format!(" A{:.3} {depth:.3} 0 0 1 {:.3} {t:.3}", sx / 2.0, l + sx * i as f64);
    }
    for i in 1..=ny {
        d += &format!(" A{depth:.3} {:.3} 0 0 1 {r:.3} {:.3}", sy / 2.0, t + sy * i as f64);
    }
    for i in 1..=nx {
        d += &format!(" A{:.3} {depth:.3} 0 0 1 {:.3} {b:.3}", sx / 2.0, r - sx * i as f64);
    }
    for i in 1..=ny {
        d += &format!(" A{depth:.3} {:.3} 0 0 1 {l:.3} {:.3}", sy / 2.0, b - sy * i as f64);
    }
    d + " Z"
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

/// The command `make` runs as, for its errors.
const MAKE: &str = "embroidery.ith.make";

/// What `embroidery.ith.make` was asked for, checked.
struct Ask {
    pr: &'static Project,
    text: Option<String>,
    shape: Option<String>,
    color: String,
    ribbon: bool,
    vinyl: bool,
    button: bool,
    /// Sewn with a zipper across it (a pouch).
    zipper: bool,
    /// The share of its height a pocket takes, when it has one.
    pocket: Option<f64>,
    /// A sleeve sewn inside out and turned (a phone or passport case).
    case: bool,
    scale: f64,
    finished: (f64, f64),
    edge: Edge,
}

/// The piece sewn in the hoop: its middle on the page (points), its size and the zipper's middle
/// from its centre (mm, down is +).
struct Piece {
    c: kurbo::Point,
    pw: f64,
    ph: f64,
    zip_y: f64,
}

impl Piece {
    /// The point `dy` mm below the piece's middle.
    fn at(&self, dy: f64) -> kurbo::Point {
        kurbo::Point::new(self.c.x, self.c.y + dy * PT_PER_MM)
    }
}

/// Where the design goes (mm from the piece's middle, down is +) and how it's laid out.
struct Room {
    top: f64,
    bottom: f64,
    /// The size the design is made to.
    inner: f64,
    /// A narrow piece (a key fob, a bookmark): sized by its width, its words turned along it.
    narrow: bool,
    /// Both a shape and words.
    both: bool,
    /// The shape at the left, the words beside it.
    side_by_side: bool,
}

/// A step of the instructions: its thread and what to do.
fn add_step(steps: &mut Vec<Value>, thread: &str, what: &str) {
    steps.push(json!({ "step": steps.len() + 1, "thread": thread, "what": what }));
}

/// A line through `pts` (mm from the piece's middle `c`), as a new path.
fn line(s: &mut Session, c: kurbo::Point, pts: &[(f64, f64)]) -> Result<u64> {
    let mut d = String::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        d.push_str(&format!("{}{:.3} {:.3} ", if i == 0 { "M" } else { "L" }, c.x + x * PT_PER_MM, c.y + y * PT_PER_MM));
    }
    s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(MAKE, "a step wasn't made"))
}

/// The params of `embroidery.ith.make`, checked.
fn ask(p: &Value) -> Result<Ask> {
    const CMD: &str = MAKE;
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
    let zipper = ZIPPERED.contains(&pr.id);
    let pocket = POCKETS.iter().find(|x| x.0 == pr.id).map(|x| x.1);
    let case = SLEEVES.contains(&pr.id);
    // a size: smaller or larger than the project's own, for the pieces that fit nothing in them
    let scale = match str_param(p, "size").unwrap_or("regular") {
        "regular" => 1.0,
        "small" | "large" if pr.panel || case => {
            return Err(bad(CMD, "this project's size is set by what it fits (the pillow form, the phone, the passport): it comes in one size"));
        }
        "small" => SMALL,
        "large" => LARGE,
        _ => return Err(bad(CMD, "`size` is small, regular or large")),
    };
    let finished = (pr.finished.0 * scale, pr.finished.1 * scale);
    // the edge: square, rounded, scalloped or round, for the pieces finished in the hoop with
    // nothing sewn into their edge
    let shaped = !pr.panel && !zipper && !case && pocket.is_none();
    let edge = edge_of(p, pr, shaped, finished)?;
    if (zipper || case) && (vinyl || ribbon) {
        return Err(bad(CMD, "this piece is sewn inside out and turned, so it takes no ribbon or vinyl"));
    }
    Ok(Ask { pr, text, shape, color, ribbon, vinyl, button, zipper, pocket, case, scale, finished, edge })
}

/// The edge asked for in `p`: the project's own unless it's `shaped` (finished in the hoop with
/// nothing sewn into its edge).
fn edge_of(p: &Value, pr: &Project, shaped: bool, finished: (f64, f64)) -> Result<Edge> {
    const CMD: &str = MAKE;
    let base = if pr.round { "round" } else { "square" };
    Ok(match str_param(p, "edge").unwrap_or(base) {
        e if e == base => {
            if pr.round {
                Edge::Round
            } else {
                Edge::Square
            }
        }
        _ if !shaped => return Err(bad(CMD, "this project's edge holds a zipper, a pocket or a seam: it keeps square corners")),
        "square" => Edge::Square,
        "rounded" => Edge::Rounded,
        "round" => Edge::Round,
        "scalloped" => {
            let (nx, ny) = scallop_counts(finished.0, finished.1);
            Edge::Scalloped(nx, ny)
        }
        _ => return Err(bad(CMD, "`edge` is square, rounded, scalloped or round")),
    })
}

/// The piece sewn in a field `fw` × `fh` (mm): a panel as big as the field allows (no bigger than
/// the front), or the finished piece itself.
fn piece_size(a: &Ask, fw: f64, fh: f64) -> (f64, f64) {
    let finished = a.finished;
    if !a.pr.panel {
        return finished;
    }
    let (long, short) = (fw.max(fh) - 2.0 * MARGIN, fw.min(fh) - 2.0 * MARGIN);
    let landscape = finished.0 >= finished.1;
    if (finished.0 - finished.1).abs() < 1.0 {
        let side = short.min(finished.0);
        (side, side)
    } else {
        let (a, b) = (long.min(finished.0.max(finished.1)), short.min(finished.0.min(finished.1)));
        if landscape { (a, b) } else { (b, a) }
    }
}

fn make(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = MAKE;
    let a = ask(p)?;
    let pr = a.pr;
    // a panel needs a 5×7-class hoop; a finished piece its own size
    let need = if pr.panel { (118.0 + 2.0 * MARGIN, 160.0 + 2.0 * MARGIN) } else { (a.finished.0 + 2.0 * MARGIN, a.finished.1 + 2.0 * MARGIN) };
    let (hoop, fw, fh) = pick_hoop(s, p, need, CMD)?;
    let (pw, ph) = piece_size(&a, fw, fh);
    // the hoop turned to the piece's way round
    let hoop = if (pw > ph) != (fw > fh) && (pw - ph).abs() > 1.0 { json!({ "width": fh, "height": fw }) } else { hoop };
    let made = s.execute("embroidery.newDesign", &json!({ "hoop": hoop }))?;
    let c = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    // the zipper's middle, from the pouch's centre (mm, down is +)
    let pc = Piece { c, pw, ph, zip_y: -ph / 2.0 + ZIPPER_DOWN };
    let mut steps: Vec<Value> = Vec::new();
    // 1: where the piece goes
    placement(s, &a, &pc, &mut steps)?;
    // 2: the design in the middle (above the words when both)
    let design = design(s, p, &a, &pc, &mut steps)?;
    // the extras: a ribbon band across a panel, vinyl over a finished piece, a button mark
    extras(s, &a, &pc, &mut steps)?;
    // 3: finishing in the hoop
    finishing(s, &a, &pc, &mut steps)?;
    // the supplies and cut sizes
    let mut supplies = vec![format!("Hoop: {} (field {})", made["hoop"].as_str().unwrap_or("custom"), size(fw, fh))];
    // the pieces to cut, for the cutting templates
    let mut pieces: Vec<(String, f64, f64)> = Vec::new();
    let cut = (&mut supplies, &mut pieces);
    if pr.panel {
        panel_supplies(&a, &pc, (fw, fh), cut, &mut steps);
    } else if a.case {
        case_supplies(&a, &pc, cut);
    } else if a.zipper {
        pouch_supplies(&pc, cut);
    } else {
        piece_supplies(&a, &pc, cut);
    }
    let title = if a.scale == 1.0 { pr.name.to_string() } else { format!("{}, made {}", pr.name, size(pw, ph)) };
    let notes = notes(title, &supplies, &steps);
    s.execute("embroidery.settings", &json!({ "notes": notes }))?;
    s.execute("select.none", &json!({}))?;
    if a.button {
        supplies.push("A button, ½″ to ¾″".into());
    }
    let pieces: Vec<Value> = pieces.into_iter().map(|(name, w, h)| json!({ "name": name, "widthMm": w, "heightMm": h })).collect();
    Ok(json!({ "project": pr.id, "hoop": made["hoop"], "design": design, "steps": steps, "supplies": supplies, "pieces": pieces, "notes": notes }))
}

/// Step 1: where the piece goes, and the fabrics laid on it.
fn placement(s: &mut Session, a: &Ask, pc: &Piece, steps: &mut Vec<Value>) -> Result<()> {
    let (c, pw, ph, zip_y) = (pc.c, pc.pw, pc.ph, pc.zip_y);
    if a.zipper {
        // the outline and the zipper line in one: one stop
        let (x0, x1, y0, y1) = (c.x - pw / 2.0 * PT_PER_MM, c.x + pw / 2.0 * PT_PER_MM, c.y - ph / 2.0 * PT_PER_MM, c.y + ph / 2.0 * PT_PER_MM);
        let zy = c.y + zip_y * PT_PER_MM;
        let d = format!("M{x0:.3} {y0:.3} L{x1:.3} {y0:.3} L{x1:.3} {y1:.3} L{x0:.3} {y1:.3} Z M{x0:.3} {zy:.3} L{x1:.3} {zy:.3}");
        let o = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(MAKE, "the outline wasn't made"))?;
        step(s, o, "run", json!({}), POUCH_PLACEMENT)?;
        add_step(steps, POUCH_PLACEMENT.1, "Hoop tear-away stabilizer and sew the pouch's outline and the zipper line across it.");
        // the front below the line, the top strip above: their edges meet on it, under the zipper
        let (front_top, bottom) = (zip_y + 4.0, ph / 2.0);
        let o = outline(s, pc.at((front_top + bottom) / 2.0), pw, bottom - front_top, Edge::Square)?;
        step(s, o, "run", json!({}), POUCH_FRONT)?;
        add_step(
            steps,
            POUCH_FRONT.1,
            "Lay the front fabric right side up with its top edge on the zipper line, covering the outline below it; the machine tacks it down.",
        );
        let (top, strip_bottom) = (-ph / 2.0, zip_y - 4.0);
        let o = outline(s, pc.at((top + strip_bottom) / 2.0), pw, strip_bottom - top, Edge::Square)?;
        step(s, o, "run", json!({}), POUCH_TOP)?;
        add_step(
            steps,
            POUCH_TOP.1,
            "Lay the top strip right side up with its bottom edge on the zipper line, covering the outline above it; the machine tacks it down.",
        );
    } else if a.case {
        let o = outline(s, c, pw, ph, Edge::Square)?;
        step(s, o, "run", json!({}), CASE_PLACEMENT)?;
        add_step(steps, CASE_PLACEMENT.1, "Hoop tear-away stabilizer and sew the case's outline.");
        let o = outline(s, c, pw - 3.0, ph - 3.0, Edge::Square)?;
        step(s, o, "run", json!({}), CASE_FRONT)?;
        add_step(
            steps,
            CASE_FRONT.1,
            "Fold the front fabric's top edge ½″ to the wrong side and press. Lay it right side up, the fold on the outline's top and the rest past the outline (fusible fleece under it, if you like); the machine tacks it down.",
        );
    } else if a.pr.panel {
        let o = outline(s, c, pw, ph, a.edge)?;
        step(s, o, "run", json!({}), TRIM_LINE)?;
        add_step(steps, TRIM_LINE.1, "Hoop the top fabric with tear-away stabilizer. This line marks the panel; trim on it after.");
    } else {
        let o = outline(s, c, pw, ph, a.edge)?;
        step(s, o, "applique", json!({ "placement": true, "tackdown": "Run", "trim_stop": false, "cover": "None" }), PLACEMENT)?;
        add_step(
            steps,
            PLACEMENT.1,
            "Hoop cut-away stabilizer and sew the line. Lay the batting, then the top fabric right side up over it; the machine tacks them down.",
        );
    }
    Ok(())
}

/// The pocket's fold line (mm from the piece's middle, down is +), when it has one.
fn fold_y(a: &Ask, pc: &Piece) -> Option<f64> {
    a.pocket.map(|share| pc.ph / 2.0 - pc.ph * share)
}

/// Where the design goes: a pouch's on the front below the zipper, a pocketed piece's above the
/// pocket.
fn room(a: &Ask, pc: &Piece) -> Room {
    let (pw, ph, zip_y) = (pc.pw, pc.ph, pc.zip_y);
    // A narrow piece (a key fob, a bookmark) is sized by its width, its words turned to run along it.
    let narrow = pw < ph * 0.5;
    let front_h = ph / 2.0 - zip_y;
    let fold_y = fold_y(a, pc);
    let (top, bottom) = match (a.zipper, fold_y) {
        (true, _) => (zip_y, ph / 2.0),
        (false, Some(f)) => (-ph / 2.0, f),
        (false, None) => (-ph / 2.0, ph / 2.0),
    };
    let area_h = bottom - top;
    let inner = match (a.zipper, fold_y) {
        (true, _) => (pw.min(front_h) - 20.0).max(10.0),
        (false, Some(_)) => (pw.min(area_h) - 12.0).max(10.0),
        (false, None) => (pw.min(ph) - 24.0).max(10.0),
    };
    let both = a.text.is_some() && a.shape.is_some();
    // a wide, short space (above a card holder's pocket): the shape at the left, the words beside
    // it, rather than one over the other, too small
    let side_by_side = both && !narrow && pw > 2.2 * area_h;
    Room { top, bottom, inner, narrow, both, side_by_side }
}

/// Step 2: the design in the middle (above the words when both), moved into its room; its ids.
fn design(s: &mut Session, p: &Value, a: &Ask, pc: &Piece, steps: &mut Vec<Value>) -> Result<Vec<u64>> {
    let r = room(a, pc);
    let mut design: Vec<u64> = Vec::new();
    let mut shape_size = 0.0;
    if let Some(name) = &a.shape {
        let (id, size) = add_shape(s, pc, &r, name)?;
        shape_size = size;
        design.extend(id);
        add_step(steps, "The design", "Sew the shape in its colours.");
    }
    if let Some(t) = &a.text {
        design.extend(add_words(s, p, a, pc, &r, t, shape_size)?);
        add_step(steps, "The words", "Sew the words.");
    }
    let area_mid = (r.top + r.bottom) / 2.0;
    if area_mid.abs() > 0.5 && !design.is_empty() {
        s.execute("select.set", &json!({ "ids": design }))?;
        s.execute("object.move", &json!({ "dx": 0.0, "dy": area_mid * PT_PER_MM }))?;
    }
    Ok(design)
}

/// The shape `name`, sized and placed for the room: its id and size (mm).
fn add_shape(s: &mut Session, pc: &Piece, r: &Room, name: &str) -> Result<(Option<u64>, f64)> {
    let (pw, ph, inner) = (pc.pw, pc.ph, r.inner);
    let size = match (r.narrow, r.both) {
        _ if r.side_by_side => (r.bottom - r.top - 10.0).max(8.0),
        (true, _) => (pw - 14.0).max(8.0),
        (false, true) => inner * 0.5,
        (false, false) => inner * 0.7,
    };
    let made = s.execute("embroidery.addShape", &json!({ "name": name, "size": size }))?;
    let id = made["id"].as_u64();
    if r.both
        && let Some(sid) = id
    {
        let (dx, dy) = match r.narrow {
            _ if r.side_by_side => (-(pw / 2.0 - 8.0 - size / 2.0), 0.0),
            true => (0.0, -(ph * 0.25)),
            false => (0.0, -inner * 0.18),
        };
        on(s, sid, "object.move", json!({ "dx": dx * PT_PER_MM, "dy": dy * PT_PER_MM }))?;
    }
    Ok((id, size))
}

/// The words `t`, sized and placed for the room (beside a shape `shape_size` mm wide when side by
/// side): their id.
fn add_words(s: &mut Session, p: &Value, a: &Ask, pc: &Piece, r: &Room, t: &str, shape_size: f64) -> Result<Option<u64>> {
    let (pw, area_h) = (pc.pw, r.bottom - r.top);
    let height = match p.get("height").and_then(Value::as_f64) {
        Some(h) => h.clamp(5.0, 80.0),
        None if r.narrow => ((pw - 14.0) * 0.55).clamp(6.0, 40.0),
        None if r.side_by_side => ((area_h - 10.0) * 0.45).clamp(6.0, 40.0),
        None => (r.inner * if r.both { 0.18 } else { 0.28 }).clamp(6.0, 40.0),
    };
    let made = s.execute("embroidery.addWords", &json!({ "text": t, "height": height, "color": a.color }))?;
    let id = made["id"].as_u64();
    if let Some(wid) = id {
        if r.narrow {
            along_strip(s, wid, pc, r)?;
        } else if r.side_by_side {
            beside_shape(s, wid, pc, shape_size)?;
        } else if r.both {
            on(s, wid, "object.move", json!({ "dx": 0.0, "dy": r.inner * 0.3 * PT_PER_MM }))?;
        }
    }
    Ok(id)
}

/// The size of node `id`'s bounds (mm), none when it has none.
fn bounds_mm(s: &Session, id: u64) -> Result<(f64, f64)> {
    let d = &s.doc()?.doc;
    Ok(d.node(NodeId(id)).and_then(|n| n.geometric_bounds()).map_or((0.0, 0.0), |b| (b.width() / PT_PER_MM, b.height() / PT_PER_MM)))
}

/// The words `wid` on a narrow piece: reading upwards, the way a strip is held; kept within the
/// piece's length.
fn along_strip(s: &mut Session, wid: u64, pc: &Piece, r: &Room) -> Result<()> {
    let ph = pc.ph;
    on(s, wid, "object.rotate", json!({ "angle": 90.0 }))?;
    let room = ph * if r.both { 0.45 } else { 0.7 };
    let long = bounds_mm(s, wid)?.1;
    if long > room && long > 0.0 {
        let k = room / long * 100.0;
        on(s, wid, "object.scale", json!({ "sx": k, "sy": k }))?;
    }
    if r.both {
        on(s, wid, "object.move", json!({ "dx": 0.0, "dy": ph * 0.15 * PT_PER_MM }))?;
    }
    Ok(())
}

/// The words `wid` in the room right of a shape `shape_size` mm wide, made smaller if they don't
/// fit it.
fn beside_shape(s: &mut Session, wid: u64, pc: &Piece, shape_size: f64) -> Result<()> {
    let pw = pc.pw;
    let (left, right) = (-pw / 2.0 + 8.0 + shape_size + 5.0, pw / 2.0 - 8.0);
    let wide = bounds_mm(s, wid)?.0;
    if wide > right - left && wide > 0.0 {
        let k = (right - left) / wide * 100.0;
        on(s, wid, "object.scale", json!({ "sx": k, "sy": k }))?;
    }
    on(s, wid, "object.move", json!({ "dx": (left + right) / 2.0 * PT_PER_MM, "dy": 0.0 }))?;
    Ok(())
}

/// The extras sewn into the piece: a pocket, a zipper, a ribbon band or loop, vinyl, a button mark.
fn extras(s: &mut Session, a: &Ask, pc: &Piece, steps: &mut Vec<Value>) -> Result<()> {
    if let Some(f) = fold_y(a, pc) {
        pocket_steps(s, pc, f, steps)?;
    }
    if a.zipper {
        zipper_steps(s, pc, steps)?;
    }
    if a.ribbon && a.pr.panel {
        ribbon_band(s, pc, steps)?;
    }
    if a.vinyl {
        let v = outline(s, pc.c, pc.pw - 4.0, pc.ph - 4.0, a.edge)?;
        step(s, v, "run", json!({ "stitch_length": 4.0 }), VINYL)?;
        add_step(
            steps,
            VINYL.1,
            "Lay clear vinyl over the whole piece and tape it outside the stitching; the machine tacks it down with long stitches, which don't tear it.",
        );
    }
    if a.ribbon && !a.pr.panel {
        ribbon_loop(s, pc, steps)?;
    }
    if a.button {
        let (bx, by) = (pc.pw / 2.0 - 16.0, pc.ph / 2.0 - 16.0);
        let cross = line(s, pc.c, &[(bx - 3.0, by - 3.0), (bx + 3.0, by + 3.0), (bx + 3.0, by - 3.0), (bx - 3.0, by + 3.0)])?;
        step(s, cross, "run", json!({ "stitch_length": 1.5 }), BUTTON)?;
        add_step(steps, BUTTON.1, "The machine marks the spot with a small cross; sew a button over it by hand when the piece is done.");
    }
    Ok(())
}

/// The pocket: its fold on the line `f`, its raw edges at the bottom; tacked on three sides, the
/// backing and the satin edge close them.
fn pocket_steps(s: &mut Session, pc: &Piece, f: f64, steps: &mut Vec<Value>) -> Result<()> {
    let (c, pw, ph) = (pc.c, pc.pw, pc.ph);
    let mark = line(s, c, &[(-pw / 2.0, f), (pw / 2.0, f)])?;
    step(s, mark, "run", json!({}), POCKET_MARK)?;
    add_step(
        steps,
        POCKET_MARK.1,
        "Fold the pocket fabric in half, wrong sides together, and press. Lay it with the fold on the line and the raw edges past the bottom of the piece; tape it.",
    );
    let (x0, x1, b) = (-pw / 2.0 + 1.5, pw / 2.0 - 1.5, ph / 2.0 - 1.5);
    let tack = line(s, c, &[(x0, f), (x0, b), (x1, b), (x1, f)])?;
    step(s, tack, "run", json!({}), POCKET_TACK)?;
    add_step(steps, POCKET_TACK.1, "The machine tacks the pocket's sides and bottom; its fold stays open for cards.");
    Ok(())
}

/// The zipper: both tapes sewn down, over the fabrics' edges: two lines, a jump between.
fn zipper_steps(s: &mut Session, pc: &Piece, steps: &mut Vec<Value>) -> Result<()> {
    let (c, pw, zip_y) = (pc.c, pc.pw, pc.zip_y);
    let (x0, x1) = (c.x - pw / 2.0 * PT_PER_MM, c.x + pw / 2.0 * PT_PER_MM);
    let (ya, yb) = (c.y + (zip_y - 7.0) * PT_PER_MM, c.y + (zip_y + 7.0) * PT_PER_MM);
    let d = format!("M{x0:.3} {ya:.3} L{x1:.3} {ya:.3} M{x1:.3} {yb:.3} L{x0:.3} {yb:.3}");
    let z = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(MAKE, "a step wasn't made"))?;
    step(s, z, "run", json!({ "passes": 3 }), ZIPPER)?;
    add_step(
        steps,
        ZIPPER.1,
        "Lay the zipper face up with its teeth on the line, its pull inside the outline and its ends past both sides; tape the ends. The machine sews both tapes down, covering the fabrics' edges.",
    );
    Ok(())
}

/// How wide a panel's ribbon band is (mm).
const RIBBON_W: f64 = 16.0;

/// A ribbon band across a panel, below the design: where its top edge goes, then both its edges
/// sewn down.
fn ribbon_band(s: &mut Session, pc: &Piece, steps: &mut Vec<Value>) -> Result<()> {
    let (x0, x1, y) = (-pc.pw / 2.0, pc.pw / 2.0, pc.ph / 2.0 - 16.0 - RIBBON_W);
    let mark = line(s, pc.c, &[(x0, y), (x1, y)])?;
    step(s, mark, "run", json!({}), RIBBON_MARK)?;
    add_step(steps, RIBBON_MARK.1, "Lay the ribbon across the panel with its top edge on the line, ends past the trim line; tape the ends.");
    let tack = line(s, pc.c, &[(x1, y + 1.5), (x0, y + 1.5), (x0, y + RIBBON_W - 1.5), (x1, y + RIBBON_W - 1.5)])?;
    step(s, tack, "run", json!({ "passes": 2 }), RIBBON_TACK)?;
    add_step(steps, RIBBON_TACK.1, "The machine sews the ribbon down along both its edges.");
    Ok(())
}

/// A hanging loop at the top: its ends inside the edge, the loop out beyond it.
fn ribbon_loop(s: &mut Session, pc: &Piece, steps: &mut Vec<Value>) -> Result<()> {
    let top = -pc.ph / 2.0;
    let mark = line(s, pc.c, &[(-6.0, top + 14.0), (-6.0, top), (6.0, top), (6.0, top + 14.0)])?;
    step(s, mark, "run", json!({}), RIBBON_MARK)?;
    add_step(
        steps,
        RIBBON_MARK.1,
        "Fold a 6″ ribbon in half; lay its ends inside the mark and the loop out beyond the top edge, and tape the loop out of the way of the edge.",
    );
    let tack = line(s, pc.c, &[(-8.0, top + 6.0), (8.0, top + 6.0), (8.0, top + 9.0), (-8.0, top + 9.0)])?;
    step(s, tack, "run", json!({ "passes": 2 }), RIBBON_TACK)?;
    add_step(steps, RIBBON_TACK.1, "The machine tacks the ribbon's ends down; the backing covers them.");
    Ok(())
}

/// Step 3: finishing in the hoop.
fn finishing(s: &mut Session, a: &Ask, pc: &Piece, steps: &mut Vec<Value>) -> Result<()> {
    let (c, pw, ph, edge) = (pc.c, pc.pw, pc.ph, a.edge);
    if a.case {
        // sides and bottom, the top left open
        let (x0, x1, t, b) = (-pw / 2.0, pw / 2.0, -ph / 2.0, ph / 2.0);
        let seam = line(s, c, &[(x0, t), (x0, b), (x1, b), (x1, t)])?;
        step(s, seam, "run", json!({ "stitch_length": 2.0, "passes": 3 }), CASE_BACK)?;
        add_step(
            steps,
            CASE_BACK.1,
            "Fold the back fabric's top edge ½″ to the wrong side and press. Lay it right side down over the front, the folds together at the top; tape it. The machine sews the sides and the bottom, leaving the top open.",
        );
        add_step(
            steps,
            "Turn it out",
            "Take it out of the hoop and tear the stabilizer away. Trim ¼″ outside the stitching, clip the bottom corners, turn the case out through the top and press. Topstitch the top closed by machine or slipstitch it by hand.",
        );
    } else if a.zipper {
        let back = outline(s, c, pw, ph, Edge::Square)?;
        step(s, back, "run", json!({ "stitch_length": 2.0, "passes": 3 }), POUCH_BACK)?;
        add_step(
            steps,
            POUCH_BACK.1,
            "Open the zipper halfway (you turn the pouch out through it). Lay the back fabric right side down over everything and tape it; the machine sews all round.",
        );
        add_step(
            steps,
            "Turn it out",
            "Take it out of the hoop and tear the stabilizer away. Trim ¼″ outside the stitching and cut the zipper's ends off there, clip the corners, turn the pouch out through the zipper and press.",
        );
    } else if a.pr.panel {
        let o = outline(s, c, pw - 16.0, ph - 16.0, edge)?;
        step(s, o, "run", json!({ "passes": 3 }), FRAME)?;
        add_step(steps, FRAME.1, "A triple-stitch frame round the design. Take the panel out of the hoop and tear the stabilizer away.");
    } else {
        let back = outline(s, c, pw, ph, edge)?;
        step(s, back, "applique", json!({ "placement": false, "tackdown": "Run", "trim_stop": true, "cover": "None" }), BACKING)?;
        add_step(
            steps,
            BACKING.1,
            "Take the hoop off the machine without unhooping. Tape the backing fabric right side out under the hoop over the piece, put the hoop back; the machine tacks it on, then stops: trim all the layers close to the stitching, front and back.",
        );
        let edge = outline(s, c, pw, ph, edge)?;
        step(s, edge, "applique", json!({ "placement": false, "tackdown": "None", "trim_stop": false, "cover": "Satin", "cover_width": 4.0 }), EDGE)?;
        add_step(
            steps,
            EDGE.1,
            "A satin edge all round covers the cut edges. Take it out of the hoop and cut the stabilizer away close to the satin.",
        );
    }
    Ok(())
}

/// The supplies list and the pieces to cut (name, width, height in mm), added to.
type Cut<'a> = (&'a mut Vec<String>, &'a mut Vec<(String, f64, f64)>);

/// A panel's supplies and pieces, in a field `field` (mm), and the steps that sew it into its
/// cover.
fn panel_supplies(a: &Ask, pc: &Piece, field: (f64, f64), cut: Cut<'_>, steps: &mut Vec<Value>) {
    let (supplies, pieces) = cut;
    let (pw, ph) = (pc.pw, pc.ph);
    let (fw, fh) = field;
    let (w, h) = a.finished;
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
    if a.ribbon {
        supplies.push(format!("Ribbon ⅝″ wide, {} long", inches(pw + 2.0 * IN)));
    }
    if a.pr.id == BOOK_COVER {
        book_cover_supplies((w, h), (supplies, pieces), steps);
    } else {
        let back_h = (h * 0.65 + IN).round();
        supplies.push(format!("Envelope back: 2 pieces {} (hem one long edge of each, overlap them)", size(w + 2.0 * SEAM, back_h)));
        pieces.push(("Envelope back (cut 2)".into(), w + 2.0 * SEAM, back_h));
        supplies.push(format!("Pillow form {} finished", size(w, h)));
        add_step(
            steps,
            "Sew the front",
            "Sew the side borders on, then the top and bottom ones, ¼″ seams; press. The front measures the pillow's size plus ¼″ all round.",
        );
        add_step(
            steps,
            "Sew the cover",
            "Lay the two hemmed back pieces right side down on the front, overlapping; sew all round with a ¼″ seam, clip the corners, turn out and put the form in.",
        );
    }
}

/// A book cover's pieces past the front (`finished` w × h mm): the spine and back beside it, a
/// lining as big, a flap folded in half at each end; and the steps that sew it.
fn book_cover_supplies(finished: (f64, f64), cut: Cut<'_>, steps: &mut Vec<Value>) {
    let (supplies, pieces) = cut;
    let (w, h) = finished;
    let (flat_w, cut_h) = (2.0 * w + SPINE + 2.0 * SEAM, h + 2.0 * SEAM);
    let flap = 3.0 * IN;
    supplies.push(format!("Back and spine: 1 piece {}", size(w + SPINE + 2.0 * SEAM, cut_h)));
    supplies.push(format!("Lining: 1 piece {}", size(flat_w, cut_h)));
    supplies.push(format!("Flaps: 2 pieces {}, folded in half to {}", size(2.0 * flap, cut_h), size(flap, cut_h)));
    supplies.push(format!("For a book {} with a {} spine; the cover lies flat at {}", size(w, h), inches(SPINE), size(2.0 * w + SPINE, h)));
    pieces.push(("Back and spine".into(), w + SPINE + 2.0 * SEAM, cut_h));
    pieces.push(("Lining".into(), flat_w, cut_h));
    pieces.push(("Flap (cut 2, fold in half)".into(), 2.0 * flap, cut_h));
    add_step(
        steps,
        "Sew the front",
        "Sew the side borders on, then the top and bottom ones, ¼″ seams; press. The front measures the book's cover plus ¼″ all round.",
    );
    add_step(steps, "Join the back", "Sew the back and spine piece to the front's left edge, ¼″ seam; press the seam open.");
    add_step(
        steps,
        "Sew the cover",
        "Fold the flaps in half, wrong sides together, and lay one at each end of the cover, raw edges together. Lay the lining right side down over them; sew all round with a ¼″ seam, leaving 4″ open at the bottom. Clip the corners, turn out, press, and topstitch the opening closed. Slide the book's covers into the flaps.",
    );
}

/// A case's supplies and pieces: a ½″ fold at the top, ½″ round the rest to trim to ¼″.
fn case_supplies(a: &Ask, pc: &Piece, cut: Cut<'_>) {
    let (supplies, pieces) = cut;
    let (pw, ph) = (pc.pw, pc.ph);
    let cut = (pw + IN, ph + IN);
    supplies.push("Tear-away stabilizer, the size of the hoop".into());
    supplies.push(format!("Front fabric {}; back fabric {}; fusible fleece {} (optional)", size(cut.0, cut.1), size(cut.0, cut.1), size(pw, ph)));
    supplies.push(format!("Finished size {}", size(pw, ph)));
    pieces.push(("Front fabric".into(), cut.0, cut.1));
    pieces.push(("Back fabric".into(), cut.0, cut.1));
    if !a.pr.extra.is_empty() {
        supplies.push(a.pr.extra.into());
    }
}

/// A zippered pouch's supplies and pieces.
fn pouch_supplies(pc: &Piece, cut: Cut<'_>) {
    let (supplies, pieces) = cut;
    let (pw, ph, zip_y) = (pc.pw, pc.ph, pc.zip_y);
    let front = (pw + IN, ph / 2.0 - zip_y + IN);
    let strip = (pw + IN, ZIPPER_DOWN + IN / 2.0);
    supplies.push("Tear-away stabilizer, the size of the hoop".into());
    supplies.push(format!("Front fabric {}; top strip {}; back fabric {}", size(front.0, front.1), size(strip.0, strip.1), size(pw + IN, ph + IN)));
    supplies.push(format!("A nylon coil zipper at least {} long (the needle sews over its ends; metal teeth break needles)", inches(pw + 2.0 * IN)));
    supplies.push(format!("Finished size {}", size(pw, ph)));
    pieces.push(("Front fabric".into(), front.0, front.1));
    pieces.push(("Top strip".into(), strip.0, strip.1));
    pieces.push(("Back fabric".into(), pw + IN, ph + IN));
}

/// The supplies and pieces of a piece finished in the hoop.
fn piece_supplies(a: &Ask, pc: &Piece, cut: Cut<'_>) {
    let (supplies, pieces) = cut;
    let (pw, ph) = (pc.pw, pc.ph);
    supplies.push(format!("Cut-away stabilizer, the size of the hoop; batting {}", size(pw + IN, ph + IN)));
    supplies.push(format!("Top fabric {}; backing fabric {}", size(pw + IN, ph + IN), size(pw + IN, ph + IN)));
    supplies.push(format!("Finished size {}", size(pw, ph)));
    let cut = (pw + IN, ph + IN);
    for name in ["Top fabric", "Batting", "Backing"] {
        pieces.push((name.into(), cut.0, cut.1));
    }
    if a.vinyl {
        supplies.push(format!("Clear vinyl {}", size(cut.0, cut.1)));
        pieces.push(("Clear vinyl".into(), cut.0, cut.1));
    }
    if a.ribbon {
        supplies.push("Ribbon ⅜″ wide, 6″ long, for the hanging loop".into());
    }
    if let Some(share) = a.pocket {
        let folded = (pw + IN, 2.0 * ph * share + IN / 2.0);
        supplies.push(format!("Pocket fabric {}, folded in half to {}", size(folded.0, folded.1), size(folded.0, folded.1 / 2.0)));
        pieces.push(("Pocket (fold in half)".into(), folded.0, folded.1));
    }
    if !a.pr.extra.is_empty() {
        supplies.push(a.pr.extra.into());
    }
}

/// The design's notes: its title, what you need and the steps, at most 2000 characters.
fn notes(title: String, supplies: &[String], steps: &[Value]) -> String {
    std::iter::once(title)
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
        .collect()
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

    /// HV-01: the phone case: outline, front, design, then the back sewn on three sides only (its
    /// top open to turn it out); the book cover: a panel like a pillow front, made up with a back
    /// and spine, a lining and two flaps.
    #[test]
    fn the_phone_case_is_open_at_the_top_and_the_book_cover_has_flaps() {
        let mut s = Session::new();
        let r = s.execute("embroidery.ith.make", &json!({"project": "phone-case", "text": "Ana", "shape": "Heart"})).unwrap();
        let threads: Vec<&str> = r["steps"].as_array().unwrap().iter().filter_map(|x| x["thread"].as_str()).collect();
        assert_eq!(threads.first(), Some(&CASE_PLACEMENT.1), "{threads:?}");
        assert!(threads.iter().position(|t| *t == CASE_FRONT.1) < threads.iter().position(|t| *t == "The design"), "{threads:?}");
        assert_eq!(&threads[threads.len() - 2..], &[CASE_BACK.1, "Turn it out"], "{threads:?}");
        assert!(!threads.contains(&EDGE.1) && !threads.contains(&BACKING.1), "turned, no satin edge");
        // the back's seam: three sides, its ends at the top corners
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        assert_eq!(r["pieces"].as_array().unwrap().len(), 2, "{r}");
        assert!(s.execute("embroidery.ith.make", &json!({"project": "phone-case", "vinyl": true})).is_err());
        // the passport holder: sewn as the phone case is, roomy enough for a passport, from a 5×7
        let r = s.execute("embroidery.ith.make", &json!({"project": "passport-holder", "text": "Ana", "shape": "Star"})).unwrap();
        let threads: Vec<&str> = r["steps"].as_array().unwrap().iter().filter_map(|x| x["thread"].as_str()).collect();
        assert_eq!((threads.first(), &threads[threads.len() - 2..]), (Some(&CASE_PLACEMENT.1), &[CASE_BACK.1, "Turn it out"][..]), "{threads:?}");
        assert!(r["hoop"].to_string().contains("130 × 180"), "{}", r["hoop"]);
        assert!(r["supplies"].to_string().contains("Finished size 4″ × 5¾″"), "{}", r["supplies"]);
        let b = s.execute("embroidery.info", &json!({})).unwrap();
        assert!(b["fits"] == true && b["widthMm"].as_f64().unwrap() >= 88.0 + 12.0 && b["heightMm"].as_f64().unwrap() >= 125.0 + 18.0, "{b}");
        assert!(s.execute("embroidery.ith.make", &json!({"project": "passport-holder", "ribbon": true})).is_err());
        // the book cover
        let r = s.execute("embroidery.ith.make", &json!({"project": "book-cover", "text": "Notes", "hoop": "Brother 130 × 180 (5×7)"})).unwrap();
        let sup = r["supplies"].to_string();
        assert!(sup.contains("Back and spine") && sup.contains("Lining") && sup.contains("Flaps: 2 pieces") && !sup.contains("Pillow form"), "{sup}");
        assert!(sup.contains("lies flat at 11¾″ × 8½″"), "{sup}");
        let names: Vec<&str> = r["steps"].as_array().unwrap().iter().filter_map(|x| x["thread"].as_str()).collect();
        assert!(names.contains(&TRIM_LINE.1) && names.contains(&"Join the back") && names.last() == Some(&"Sew the cover"), "{names:?}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert!(info["heightMm"].as_f64().unwrap() > info["widthMm"].as_f64().unwrap(), "a portrait panel: {info}");
    }

    /// HV-01: the card holder: its pocket marked and tacked after the design, the design above
    /// the fold (not sewn through the pocket), the pocket in the supplies; the pencil case is a
    /// longer zipper pouch, in a 6×10-class hoop.
    #[test]
    fn the_card_holder_has_a_pocket_below_its_design_and_the_pencil_case_a_zipper() {
        let mut s = Session::new();
        let r = s.execute("embroidery.ith.make", &json!({"project": "card-holder", "text": "Sam"})).unwrap();
        let threads: Vec<String> = r["steps"].as_array().unwrap().iter().map(|x| x["thread"].as_str().unwrap().to_string()).collect();
        let pos = |t: &str| threads.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t} in {threads:?}"));
        assert!(pos(PLACEMENT.1) < pos("The words") && pos("The words") < pos(POCKET_MARK.1), "{threads:?}");
        assert!(pos(POCKET_MARK.1) < pos(POCKET_TACK.1) && pos(POCKET_TACK.1) < pos(BACKING.1) && pos(BACKING.1) < pos(EDGE.1), "{threads:?}");
        let d = &s.doc().unwrap().doc;
        let page = d.artboards[0].rect;
        let words = d.node(NodeId(r["design"][0].as_u64().unwrap())).and_then(|n| n.geometric_bounds()).unwrap();
        let fold = page.center().y + (3.0 * IN / 2.0 - 3.0 * IN * 0.55) * PT_PER_MM;
        assert!(words.y1 < fold, "the words {words:?} above the pocket's fold at {fold}");
        assert!(r["supplies"].to_string().contains("Pocket fabric"), "{r}");
        // with a shape too: side by side above the pocket, apart, both inside the piece
        let r2 = s.execute("embroidery.ith.make", &json!({"project": "card-holder", "text": "Bruce", "shape": "Heart"})).unwrap();
        let d2 = &s.doc().unwrap().doc;
        let b = |i: usize| d2.node(NodeId(r2["design"][i].as_u64().unwrap())).and_then(|n| n.geometric_bounds()).unwrap();
        let (heart, name) = (b(0), b(1));
        assert!(heart.x1 < name.x0, "the heart {heart:?} left of the name {name:?}");
        let page2 = d2.artboards[0].rect;
        assert!(heart.x0 > page2.center().x - 2.0 * IN * PT_PER_MM && name.x1 < page2.center().x + 2.0 * IN * PT_PER_MM, "{heart:?} {name:?}");
        assert!(heart.y1.max(name.y1) < fold, "above the fold");
        assert!(r["pieces"].as_array().unwrap().iter().any(|p| p["name"] == "Pocket (fold in half)"), "{r}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        // the pencil case
        let r = s.execute("embroidery.ith.make", &json!({"project": "pencil-case", "text": "Pens"})).unwrap();
        let threads: Vec<&str> = r["steps"].as_array().unwrap().iter().filter_map(|x| x["thread"].as_str()).collect();
        assert!(threads.contains(&ZIPPER.1) && threads.last() == Some(&"Turn it out"), "{threads:?}");
        assert!(r["supplies"].to_string().contains("at least 10″ long"), "{r}");
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["fits"], true);
        assert!(s.execute("embroidery.ith.make", &json!({"project": "pencil-case", "ribbon": true})).is_err());
    }

    /// HV-01: the zipper pouch: outline and zipper line, front and top strip, the design on the
    /// front below the zipper, the zipper over their edges, the back face down; no ribbon or vinyl.
    #[test]
    fn the_zipper_pouch_sews_its_layers_round_the_zipper() {
        let mut s = Session::new();
        let r = s.execute("embroidery.ith.make", &json!({"project": "zipper-pouch", "text": "Bits", "shape": "Heart"})).unwrap();
        let threads: Vec<String> = r["steps"].as_array().unwrap().iter().map(|x| x["thread"].as_str().unwrap().to_string()).collect();
        let pos = |t: &str| threads.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t} in {threads:?}"));
        assert!(pos(POUCH_PLACEMENT.1) < pos(POUCH_FRONT.1) && pos(POUCH_FRONT.1) < pos(POUCH_TOP.1), "{threads:?}");
        assert!(pos(POUCH_TOP.1) < pos("The design") && pos("The words") < pos(ZIPPER.1) && pos(ZIPPER.1) < pos(POUCH_BACK.1), "{threads:?}");
        assert!(!threads.iter().any(|t| t == EDGE.1 || t == BACKING.1), "no satin edge: it is turned out");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        assert!(info["widthMm"].as_f64().unwrap() > info["heightMm"].as_f64().unwrap(), "wider than tall: {info}");
        // the design sits on the front, below the zipper
        let d = &s.doc().unwrap().doc;
        let page = d.artboards[0].rect;
        let ids = r["design"].as_array().unwrap();
        assert_eq!(ids.len(), 2, "{r}");
        for id in ids {
            let b = d.node(NodeId(id.as_u64().unwrap())).and_then(|n| n.geometric_bounds()).unwrap();
            // below the zipper's lower tape (the pouch is 4½″ tall, centred on the page)
            let tape = page.center().y + (-4.5 * IN / 2.0 + ZIPPER_DOWN + 7.0) * PT_PER_MM;
            assert!(b.y0 > tape, "{b:?} below the zipper at {tape}");
        }
        let sup = r["supplies"].to_string();
        assert!(sup.contains("nylon coil zipper") && sup.contains("top strip"), "{sup}");
        assert_eq!(r["pieces"].as_array().unwrap().len(), 3, "{r}");
        for bad in [json!({"project": "zipper-pouch", "ribbon": true}), json!({"project": "zipper-pouch", "vinyl": true})] {
            assert!(s.execute("embroidery.ith.make", &bad).is_err(), "{bad}");
        }
    }

    /// K2: every project is made in a hoop it fits, its steps each a thread of their own in sewing
    /// order, its supplies and steps kept as the design's notes.
    #[test]
    fn pieces_come_smaller_or_larger_with_rounded_scalloped_or_round_edges() {
        let mut s = Session::new();
        let list = s.execute("embroidery.ith.projects", &json!({})).unwrap();
        let find = |id: &str| list["projects"].as_array().unwrap().iter().find(|p| p["id"] == id).unwrap().clone();
        assert_eq!((find("mug-rug")["sizes"].clone(), find("mug-rug")["edges"].clone()), (json!(true), json!(true)));
        assert_eq!((find("phone-case")["sizes"].clone(), find("card-holder")["edges"].clone()), (json!(false), json!(false)));
        // the finished size follows the size asked for, and the hoop grows to fit it
        let finished = |r: &Value| {
            r["supplies"].as_array().unwrap().iter().filter_map(|x| x.as_str()).find(|x| x.starts_with("Finished size")).map(str::to_string)
        };
        let regular = s.execute("embroidery.ith.make", &json!({"project": "coaster"})).unwrap();
        let large = s.execute("embroidery.ith.make", &json!({"project": "coaster", "size": "large"})).unwrap();
        let small = s.execute("embroidery.ith.make", &json!({"project": "coaster", "size": "small"})).unwrap();
        assert!(finished(&regular).unwrap().contains("3¾″"), "{regular}");
        assert!(finished(&large).unwrap().contains("4¾″"), "{large}");
        assert!(finished(&small).unwrap().contains("3″"), "{small}");
        assert!(large["notes"].as_str().unwrap().starts_with("Coaster, 3¾″ square, made"), "{}", large["notes"]);
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["fits"], true);
        // each edge sews in the hoop, its satin edge following the outline
        for (id, edge) in [
            ("mug-rug", "scalloped"),
            ("coaster", "rounded"),
            ("coaster", "round"),
            ("luggage-tag", "scalloped"),
            ("bookmark", "rounded"),
            ("ornament", "scalloped"),
        ] {
            s.execute("embroidery.ith.make", &json!({"project": id, "edge": edge, "size": "large"})).unwrap_or_else(|e| panic!("{id} {edge}: {e}"));
            let info = s.execute("embroidery.info", &json!({})).unwrap();
            assert_eq!(info["fits"], true, "{id} {edge}: {info}");
        }
        // the scallops make the outline exactly the piece's size
        let c = kurbo::Point::new(0.0, 0.0);
        let (nx, ny) = scallop_counts(165.0, 114.0);
        assert!((7..=11).contains(&nx) && (4..=8).contains(&ny), "{nx} {ny}");
        let id = s.execute("path.create", &json!({ "d": scallops(c, 165.0, 114.0, nx, ny) })).unwrap()["id"].as_u64().unwrap();
        let bb = s.doc().unwrap().doc.node(vectorcraft_doc::NodeId(id)).unwrap().visual_bounds().unwrap();
        assert!((bb.width() / PT_PER_MM - 165.0).abs() < 0.5 && (bb.height() / PT_PER_MM - 114.0).abs() < 0.5, "{bb:?}");
        // what a project can't be
        for bad in [
            json!({"project": "phone-case", "size": "large"}),
            json!({"project": "pillow-square", "size": "small"}),
            json!({"project": "card-holder", "edge": "scalloped"}),
            json!({"project": "zipper-pouch", "edge": "round"}),
            json!({"project": "coaster", "size": "huge"}),
            json!({"project": "coaster", "edge": "wavy"}),
        ] {
            assert!(s.execute("embroidery.ith.make", &bad).is_err(), "{bad}");
        }
        // the guide passes them on
        let r = s.execute("embroidery.makeDesign", &json!({"what": "ith", "project": "mug-rug", "pieceSize": "small", "edge": "rounded"})).unwrap();
        assert_eq!(r["what"], "ith");
    }

    /// The first and last threads of project `pr`'s steps (and the stops between them), by kind.
    fn thread_order(pr: &Project, threads: &[String], info: &Value) {
        if SLEEVES.contains(&pr.id) {
            assert_eq!(threads.first().map(String::as_str), Some(CASE_PLACEMENT.1), "{}: {threads:?}", pr.id);
            assert_eq!(threads.last().map(String::as_str), Some(CASE_BACK.1), "{}: {threads:?}", pr.id);
        } else if ZIPPERED.contains(&pr.id) {
            assert_eq!(threads.first().map(String::as_str), Some(POUCH_PLACEMENT.1), "{}: {threads:?}", pr.id);
            assert_eq!(threads.last().map(String::as_str), Some(POUCH_BACK.1), "{}: {threads:?}", pr.id);
        } else if pr.panel {
            assert_eq!(threads.first().map(String::as_str), Some(TRIM_LINE.1), "{}: {threads:?}", pr.id);
            assert_eq!(threads.last().map(String::as_str), Some(FRAME.1), "{}: {threads:?}", pr.id);
        } else {
            assert_eq!(threads.first().map(String::as_str), Some(PLACEMENT.1), "{}: {threads:?}", pr.id);
            assert_eq!(threads.last().map(String::as_str), Some(EDGE.1), "{}: {threads:?}", pr.id);
            assert!(threads.iter().any(|t| t == BACKING.1), "{}: {threads:?}", pr.id);
            // placement, tack-down and trim stops: the machine stops for each layer
            assert!(info["stops"].as_u64().unwrap_or(0) + info["colorChanges"].as_u64().unwrap_or(0) >= 4, "{}: {info}", pr.id);
        }
    }

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
            thread_order(pr, &threads, &info);
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
