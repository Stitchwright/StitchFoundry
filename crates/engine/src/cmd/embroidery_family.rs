//! Family Tree (HV-01): a family chart made from names, each name lettered, framed if asked, and
//! joined to its partner and its children (or parents) by connector lines. Behaviour from
//! mySewnet's public pages on its Family Tree wizard (a descendants chart, a person with partner,
//! children, grandchildren and great-grandchildren, or an ancestors chart, a person's parents,
//! grandparents and great-grandparents; names given only or in full; a frame per person;
//! connectors sewn as satin or running lines, single or double, or none;
//! <https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Family_Tree_Wizard.htm>). The
//! names are typed as an indented outline here, one person a line; the layout, wording and code
//! are our own.

use serde_json::{Value, json};
use vectorcraft_embroidery::PT_PER_MM;

use super::embroidery_lettering::ranged;
use super::{CommandSpec, EngineError, NodeId, Result, Session, bad, bool_or, cmd, has_doc, str_param};

const CMD: &str = "embroidery.familyTree";
/// Generations a chart holds: the person and three more (great-grandchildren or -parents).
const MAX_GENERATIONS: usize = 4;
/// People a chart holds at most.
const MAX_PEOPLE: usize = 60;
/// The longest name kept (characters).
const MAX_NAME: usize = 40;
/// The smallest letters a chart is made smaller to (mm).
const MIN_HEIGHT: f64 = 4.0;
/// Room kept between the chart and the hoop's sewing field's edge (mm).
const MARGIN: f64 = 5.0;

/// One box on the chart: a person, or a couple side by side, and the boxes below it (its
/// children) or above it (its parents).
#[derive(Debug, PartialEq)]
struct Unit {
    names: Vec<String>,
    kids: Vec<Unit>,
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.familyTree",
        "Family Tree",
        [],
        None,
        "{people: text (an outline, one person a line, each generation indented under the one before; descendants: \"Name + Partner\" for a couple, their children indented under them; ancestors: the person first, then their father and mother indented under them, and each one's parents under them; 4 generations and 60 people at most), chart?: descendants|ancestors (descendants: the first person at the top, the children below; ancestors: the person at the bottom, the parents above), names?: full|given (full: as typed; given: the first word of each), height?: mm the capital letters are tall (7; 4–30), color?: \"#rrggbb\" (the names; #1f2933), frame?: none|box|rounded|oval (a frame round each name; none), frameColor?: \"#rrggbb\" (#8a6d3b), connectors?: run|satin|none (the lines joining partners and children to parents; run), double?: bool (each connector two lines side by side), connectorColor?: \"#rrggbb\" (#5b4636), fit?: bool (true: made smaller to fit the hoop, down to 4 mm letters)} a family chart in the middle of the hoop: each name lettered as satin, framed if asked, the partners joined side by side and each family joined to its parents by connector lines; names, frames and connectors each a colour; in a group, selected; one undo step → {id, people, generations, heightMm, widthMm, chartHeightMm, fits}",
        has_doc,
        family_tree
    )]
}

/// The outline as units: one a line, each indented line a child of the last line less indented.
/// `ancestors`: each line one person and at most two lines under it (father, mother); else a
/// line is a person or a couple (`+` or `&` between them).
fn parse(text: &str, ancestors: bool, given_only: bool) -> Result<Vec<Unit>> {
    let mut lines: Vec<(usize, Vec<String>)> = Vec::new();
    for raw in text.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let indent: usize = raw.chars().take_while(|c| c.is_whitespace()).map(|c| if c == '\t' { 4 } else { 1 }).sum();
        let body = raw.trim().trim_start_matches(['-', '*', '•']).trim();
        let parts: Vec<&str> = if ancestors { vec![body] } else { body.split(['+', '&']).collect() };
        let names: Vec<String> = parts
            .iter()
            .map(|n| {
                let n: String = n.chars().filter(|c| !c.is_control()).collect();
                let n = n.split_whitespace().collect::<Vec<_>>();
                let n = if given_only { n.first().map(|s| s.to_string()).unwrap_or_default() } else { n.join(" ") };
                n.chars().take(MAX_NAME).collect::<String>()
            })
            .filter(|n| !n.is_empty())
            .collect();
        if names.is_empty() {
            continue;
        }
        if names.len() > 2 {
            return Err(bad(CMD, format!("one person and their partner a line: \"{}\" has {}", body, names.len())));
        }
        lines.push((indent, names));
    }
    let people: usize = lines.iter().map(|l| l.1.len()).sum();
    if people == 0 {
        return Err(bad(CMD, "type the names in `people`, one person a line"));
    }
    if people > MAX_PEOPLE {
        return Err(bad(CMD, format!("{MAX_PEOPLE} people at most on a chart")));
    }
    // each line's generation: how many of the open lines above it are less indented
    let mut open: Vec<usize> = Vec::new();
    let mut flat: Vec<(usize, Vec<String>)> = Vec::new();
    for (indent, names) in lines {
        while open.last().is_some_and(|&i| i >= indent) {
            open.pop();
        }
        let level = open.len();
        if level >= MAX_GENERATIONS {
            return Err(bad(CMD, format!("{MAX_GENERATIONS} generations at most on a chart")));
        }
        if level > 0 && flat.is_empty() {
            return Err(bad(CMD, "the first line is the first person, not indented"));
        }
        open.push(indent);
        flat.push((level, names));
    }
    fn build(flat: &[(usize, Vec<String>)], at: &mut usize, level: usize) -> Vec<Unit> {
        let mut out = Vec::new();
        while let Some((l, names)) = flat.get(*at) {
            if *l < level {
                break;
            }
            *at += 1;
            let kids = build(flat, at, level + 1);
            out.push(Unit { names: names.clone(), kids });
        }
        out
    }
    let mut at = 0;
    let roots = build(&flat, &mut at, 0);
    if ancestors {
        fn two(u: &Unit) -> bool {
            u.kids.len() <= 2 && u.kids.iter().all(two)
        }
        if roots.len() != 1 {
            return Err(bad(CMD, "an ancestors chart is one person's: their name first, their parents indented under it"));
        }
        if !roots.iter().all(two) {
            return Err(bad(CMD, "two parents a person: the father, then the mother"));
        }
    }
    Ok(roots)
}

fn generations(units: &[Unit]) -> usize {
    units.iter().map(|u| 1 + generations(&u.kids)).max().unwrap_or(0)
}

fn count(units: &[Unit]) -> usize {
    units.iter().map(|u| u.names.len() + count(&u.kids)).sum()
}

/// A unit placed: its names' boxes (centre x, width) on its row, and its children.
struct Placed {
    level: usize,
    /// (centre x, box width) of each name, mm.
    boxes: Vec<(f64, f64)>,
    kids: Vec<Placed>,
}

/// The sizes the layout works with (mm).
#[derive(Clone, Copy)]
struct Sizes {
    /// A name box's height.
    box_h: f64,
    /// Between partners' boxes (the partner line's length).
    partner_gap: f64,
    /// Between neighbouring families on a row.
    sibling_gap: f64,
}

/// Lays `units` out from `left`: each unit as wide as its names or its children, whichever is
/// wider, centred over them. `widths` gives each name's box width, in reading order. → the units
/// placed and the right edge.
fn place(units: &[Unit], widths: &mut impl Iterator<Item = f64>, level: usize, left: f64, sz: Sizes) -> (Vec<Placed>, f64) {
    let mut out = Vec::new();
    let mut x = left;
    for (i, u) in units.iter().enumerate() {
        if i > 0 {
            x += sz.sibling_gap;
        }
        let ws: Vec<f64> = u.names.iter().map(|_| widths.next().unwrap_or(10.0)).collect();
        let own = ws.iter().sum::<f64>() + sz.partner_gap * (ws.len().saturating_sub(1)) as f64;
        let (kids, kids_right) = place(&u.kids, widths, level + 1, x, sz);
        let kids_w = if u.kids.is_empty() { 0.0 } else { kids_right - x };
        let span = own.max(kids_w);
        // the names centred over the span; children narrower than the names centred under them
        let mut kids = kids;
        if kids_w < span && !kids.is_empty() {
            shift(&mut kids, (span - kids_w) / 2.0);
        }
        let mut bx = x + (span - own) / 2.0;
        let mut boxes = Vec::new();
        for w in ws {
            boxes.push((bx + w / 2.0, w));
            bx += w + sz.partner_gap;
        }
        out.push(Placed { level, boxes, kids });
        x += span;
    }
    (out, x)
}

fn shift(units: &mut [Placed], dx: f64) {
    for u in units {
        for b in &mut u.boxes {
            b.0 += dx;
        }
        shift(&mut u.kids, dx);
    }
}

/// The connector lines (mm, each a list of points): partners joined between their boxes, each
/// family joined to its parents through a bar halfway between the rows. `row_y` gives a row's
/// middle; `down`: children below their parents (a descendants chart), else above.
fn connectors(units: &[Placed], row_y: &dyn Fn(usize) -> f64, sz: Sizes, down: bool, out: &mut Vec<Vec<(f64, f64)>>) {
    let half = sz.box_h / 2.0;
    let dir = if down { 1.0 } else { -1.0 };
    for u in units {
        let y = row_y(u.level);
        for pair in u.boxes.windows(2) {
            if let [(ax, aw), (bx, bw)] = pair {
                out.push(vec![(ax + aw / 2.0, y), (bx - bw / 2.0, y)]);
            }
        }
        if !u.kids.is_empty() {
            // from the couple's line (or the person's box) to the bar
            let (from_x, from_y) = match u.boxes.as_slice() {
                [(ax, aw), (bx, bw), ..] => (((ax + aw / 2.0) + (bx - bw / 2.0)) / 2.0, y),
                [(ax, _)] => (*ax, y + dir * half),
                [] => continue,
            };
            let kid_y = row_y(u.level + 1);
            let bar = (y + kid_y) / 2.0;
            out.push(vec![(from_x, from_y), (from_x, bar)]);
            // the child's own box: its first name's
            let tops: Vec<f64> = u.kids.iter().filter_map(|k| k.boxes.first().map(|b| b.0)).collect();
            let (lo, hi) = tops.iter().fold((from_x, from_x), |(lo, hi), &x| (lo.min(x), hi.max(x)));
            if hi - lo > 0.01 {
                out.push(vec![(lo, bar), (hi, bar)]);
            }
            for x in tops {
                out.push(vec![(x, bar), (x, kid_y - dir * half)]);
            }
            connectors(&u.kids, row_y, sz, down, out);
        }
    }
}

fn all_boxes(units: &[Placed], out: &mut Vec<(usize, f64, f64)>) {
    for u in units {
        out.extend(u.boxes.iter().map(|b| (u.level, b.0, b.1)));
        all_boxes(&u.kids, out);
    }
}

fn color_param(p: &Value, key: &str, default: &str) -> Result<String> {
    let c = str_param(p, key).unwrap_or(default).to_string();
    sf_core::Rgb::from_hex(&c).ok_or_else(|| bad(CMD, format!("`{key}` is \"#rrggbb\"")))?;
    Ok(c)
}

fn family_tree(s: &mut Session, p: &Value) -> Result<Value> {
    let ancestors = match str_param(p, "chart").unwrap_or("descendants") {
        "descendants" => false,
        "ancestors" => true,
        _ => return Err(bad(CMD, "`chart` is descendants or ancestors")),
    };
    let given_only = match str_param(p, "names").unwrap_or("full") {
        "full" => false,
        "given" => true,
        _ => return Err(bad(CMD, "`names` is full or given")),
    };
    let units = parse(str_param(p, "people").unwrap_or_default(), ancestors, given_only)?;
    let height = ranged(p, "height", 4.0..=30.0, CMD, "`height` is 4 to 30 mm")?.unwrap_or(7.0);
    let frame = str_param(p, "frame").unwrap_or("none");
    if !["none", "box", "rounded", "oval"].contains(&frame) {
        return Err(bad(CMD, "`frame` is none, box, rounded or oval"));
    }
    let line = str_param(p, "connectors").unwrap_or("run");
    if !["run", "satin", "none"].contains(&line) {
        return Err(bad(CMD, "`connectors` is run, satin or none"));
    }
    let colors = (color_param(p, "color", "#1f2933")?, color_param(p, "frameColor", "#8a6d3b")?, color_param(p, "connectorColor", "#5b4636")?);
    let double = bool_or(p, "double", false);
    let fit = bool_or(p, "fit", true);
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_named_undo_group(Some("Family Tree".into()));
    }
    let done = make(s, &units, Style { height, frame, line, double, fit, colors, down: !ancestors });
    if let Some(uid) = group {
        s.end_document_undo_group(uid, done.is_err());
    }
    let mut r = done?;
    r["people"] = count(&units).into();
    r["generations"] = generations(&units).into();
    Ok(r)
}

struct Style<'a> {
    height: f64,
    frame: &'a str,
    line: &'a str,
    double: bool,
    fit: bool,
    /// The names', the frames' and the connectors' threads.
    colors: (String, String, String),
    down: bool,
}

/// The ids of the words made, in reading order, and their widths (mm).
fn letter(s: &mut Session, units: &[Unit], height: f64, color: &str, out: &mut Vec<(u64, f64)>) -> Result<()> {
    for u in units {
        for n in &u.names {
            let r = s.execute("embroidery.addWords", &json!({ "text": n, "height": height, "color": color }))?;
            let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "a name wasn't lettered"))?;
            out.push((id, r["widthMm"].as_f64().unwrap_or(height * n.chars().count() as f64 * 0.6)));
        }
        letter(s, &u.kids, height, color, out)?;
    }
    Ok(())
}

/// The bounds of node `id` in points.
fn bounds(s: &Session, id: u64) -> Result<kurbo::Rect> {
    let d = &s.doc()?.doc;
    d.node(NodeId(id)).and_then(|n| n.geometric_bounds()).ok_or_else(|| bad(CMD, "a name has no size"))
}

fn make(s: &mut Session, units: &[Unit], st: Style) -> Result<Value> {
    let page = {
        let d = &s.doc()?.doc;
        d.artboards.first().map(|a| a.rect.center()).ok_or_else(|| EngineError::Other("the design has no page".into()))?
    };
    let hoop = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.hoop.as_ref()).map(|h| (h.width, h.height));
    // lettered at Add Words' smallest at least, then made as small as asked
    let base = st.height.max(5.0);
    let mut words = Vec::new();
    letter(s, units, base, &st.colors.0, &mut words)?;
    let framed = st.frame != "none";
    // the layout at a letter height `h`, the words' widths scaled with it
    let layout = |h: f64| {
        let k = h / base;
        let pad = if framed { 0.5 * h } else { 0.25 * h };
        let sz = Sizes { box_h: if framed { 1.9 * h } else { 1.4 * h }, partner_gap: 1.6 * h, sibling_gap: 1.4 * h };
        let extra = if st.frame == "oval" { 0.9 * h } else { 0.0 };
        let mut widths = words.iter().map(|(_, w)| w * k + 2.0 * pad + 2.0 * extra);
        let (placed, right) = place(units, &mut widths, 0, 0.0, sz);
        let rows = generations(units);
        let pitch = sz.box_h + 2.4 * h;
        let chart_h = sz.box_h + pitch * rows.saturating_sub(1) as f64;
        (placed, right, chart_h, pitch, sz)
    };
    let (_, w0, h0, _, _) = layout(st.height);
    // made smaller to fit the hoop, not below the smallest letters
    let mut height = st.height;
    if st.fit
        && let Some((fw, fh)) = hoop
    {
        let room = |a: f64| (a - 2.0 * MARGIN).max(1.0);
        let k = (room(fw) / w0).min(room(fh) / h0).min(1.0);
        height = (st.height * k).max(MIN_HEIGHT.min(st.height));
    }
    let (placed, width, chart_h, pitch, sz) = layout(height);
    let k = height / base;
    // the chart's middle on the page's: rows from the top (descendants) or the bottom (ancestors)
    let (ox, oy) = (page.x / PT_PER_MM - width / 2.0, page.y / PT_PER_MM - chart_h / 2.0);
    let rows = generations(units);
    let row_y = |level: usize| {
        let from_top = if st.down { level } else { rows.saturating_sub(1).saturating_sub(level) };
        oy + sz.box_h / 2.0 + pitch * from_top as f64
    };
    let mut boxes = Vec::new();
    all_boxes(&placed, &mut boxes);
    let mut ids: Vec<u64> = Vec::new();
    for ((id, _), (level, cx, _)) in words.iter().zip(&boxes) {
        if (k - 1.0).abs() > 1e-6 {
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("object.scale", &json!({ "sx": k * 100.0, "sy": k * 100.0 }))?;
        }
        let b = bounds(s, *id)?;
        let to = kurbo::Point::new((ox + cx) * PT_PER_MM, row_y(*level) * PT_PER_MM);
        s.execute("select.set", &json!({ "ids": [id] }))?;
        s.execute("object.move", &json!({ "dx": to.x - b.center().x, "dy": to.y - b.center().y }))?;
        ids.push(*id);
    }
    // the frames round the names
    if framed {
        for (level, cx, bw) in &boxes {
            let (x, y) = ((ox + cx - bw / 2.0) * PT_PER_MM, (row_y(*level) - sz.box_h / 2.0) * PT_PER_MM);
            let (w, h) = (bw * PT_PER_MM, sz.box_h * PT_PER_MM);
            let q = json!({ "x": x, "y": y, "width": w, "height": h });
            let r = match st.frame {
                "oval" => s.execute("shape.ellipse", &q)?,
                "rounded" => s.execute("shape.rectangle", &json!({ "x": x, "y": y, "width": w, "height": h, "radius": h * 0.4 }))?,
                _ => s.execute("shape.rectangle", &q)?,
            };
            let id = r["id"].as_u64().ok_or_else(|| bad(CMD, "a frame wasn't made"))?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            s.execute("paint.setFill", &json!({ "none": true }))?;
            s.execute("embroidery.set", &json!({ "type": "run" }))?;
            s.execute("embroidery.setParams", &json!({ "params": { "passes": 3, "stitch_length": 2.0 } }))?;
            s.execute("embroidery.setThread", &json!({ "color": st.colors.1, "name": "Frames", "ids": [id], "paint": true }))?;
            ids.push(id);
        }
    }
    // the connectors: one path, a line a piece (doubled side by side if asked)
    if st.line != "none" {
        let mut lines = Vec::new();
        connectors(&placed, &row_y, sz, st.down, &mut lines);
        let off = if st.line == "satin" { 0.9 } else { 0.6 };
        let mut d = String::new();
        for l in &lines {
            let (Some(a), Some(b)) = (l.first(), l.last()) else { continue };
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if len < 0.5 {
                continue;
            }
            let (nx, ny) = (-dy / len, dx / len);
            let shifts: &[f64] = if st.double { &[-off, off] } else { &[0.0] };
            for sh in shifts {
                d.push_str(&format!(
                    "M{:.3} {:.3} L{:.3} {:.3} ",
                    (ox + a.0 + nx * sh) * PT_PER_MM,
                    (a.1 + ny * sh) * PT_PER_MM,
                    (ox + b.0 + nx * sh) * PT_PER_MM,
                    (b.1 + ny * sh) * PT_PER_MM
                ));
            }
        }
        if !d.is_empty() {
            let id = s.execute("path.create", &json!({ "d": d }))?["id"].as_u64().ok_or_else(|| bad(CMD, "the connectors weren't made"))?;
            s.execute("select.set", &json!({ "ids": [id] }))?;
            if st.line == "satin" {
                s.execute("embroidery.set", &json!({ "type": "satin" }))?;
                s.execute("embroidery.setParams", &json!({ "params": { "width": if st.double { 0.9 } else { 1.4 } } }))?;
            } else {
                s.execute("embroidery.set", &json!({ "type": "run" }))?;
                s.execute("embroidery.setParams", &json!({ "params": { "passes": 3, "stitch_length": 2.0 } }))?;
            }
            s.execute("embroidery.setThread", &json!({ "color": st.colors.2, "name": "Connectors", "ids": [id], "paint": true }))?;
            ids.push(id);
        }
    }
    s.execute("select.set", &json!({ "ids": ids }))?;
    let g = s.execute("object.group", &json!({}))?;
    let id = g["id"].as_u64().or_else(|| ids.first().copied()).unwrap_or_default();
    let fits = hoop.map(|(fw, fh)| width <= fw - 2.0 && chart_h <= fh - 2.0);
    Ok(
        json!({ "id": id, "heightMm": (height * 10.0).round() / 10.0, "widthMm": (width * 10.0).round() / 10.0, "chartHeightMm": (chart_h * 10.0).round() / 10.0, "fits": fits }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAMILY: &str = "John Green + Mary Green\n  Anna Green + Tom Hill\n    Lucy Hill\n    Sam Hill\n  Peter Green\n    Ella Green\n";

    #[test]
    fn the_outline_is_read_as_generations() {
        let u = parse(FAMILY, false, false).unwrap();
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].names, ["John Green", "Mary Green"]);
        assert_eq!(u[0].kids.len(), 2);
        assert_eq!(u[0].kids[0].names, ["Anna Green", "Tom Hill"]);
        assert_eq!(u[0].kids[0].kids.iter().map(|k| k.names[0].as_str()).collect::<Vec<_>>(), ["Lucy Hill", "Sam Hill"]);
        assert_eq!((count(&u), generations(&u)), (8, 3));
        // given names only; tabs and bullets
        let u = parse("- John Green & Mary Green\n\t* Anna Green\n", false, true).unwrap();
        assert_eq!((u[0].names.clone(), u[0].kids[0].names.clone()), (vec!["John".to_string(), "Mary".to_string()], vec!["Anna".to_string()]));
        // ancestors: one person, two parents each
        let a = parse("Lucy Hill\n  Tom Hill\n    Ed Hill\n    Rose Hill\n  Anna Green\n", true, false).unwrap();
        assert_eq!((a[0].kids.len(), a[0].kids[0].kids.len()), (2, 2));
        assert!(parse("Lucy\n  A\n  B\n  C\n", true, false).is_err(), "three parents");
        assert!(parse("Lucy\nTom\n", true, false).is_err(), "two people");
        assert!(parse("A + B", true, false).unwrap()[0].names == ["A + B"], "an ancestors line is one person");
        assert!(parse("A + B + C", false, false).is_err());
        assert!(parse("  \n", false, false).is_err());
        assert!(parse("A\n B\n  C\n   D\n    E\n", false, false).is_err(), "five generations");
        assert!(parse(&"Name\n".repeat(61), false, false).is_err());
        assert_eq!(parse(&format!("{}\n", "x".repeat(80)), false, false).unwrap()[0].names[0].len(), MAX_NAME);
    }

    #[test]
    fn parents_are_centred_over_their_children() {
        let u = parse(FAMILY, false, false).unwrap();
        let sz = Sizes { box_h: 10.0, partner_gap: 8.0, sibling_gap: 6.0 };
        let mut w = std::iter::repeat(20.0);
        let (placed, right) = place(&u, &mut w, 0, 0.0, sz);
        // Anna and Tom (48 wide, wider than their two children) and Peter (20) side by side
        assert!((right - (48.0 + 6.0 + 20.0)).abs() < 1e-6, "{right}");
        let top = &placed[0];
        let mid = (top.boxes[0].0 + top.boxes[1].0) / 2.0;
        assert!((mid - right / 2.0).abs() < 1e-6, "the couple over the middle");
        let anna = &top.kids[0];
        let kids_mid = (anna.kids[0].boxes[0].0 + anna.kids[1].boxes[0].0) / 2.0;
        let couple_mid = (anna.boxes[0].0 + anna.boxes[1].0) / 2.0;
        assert!((kids_mid - couple_mid).abs() < 1e-6);
        // no two boxes on a row overlap
        let mut boxes = Vec::new();
        all_boxes(&placed, &mut boxes);
        for a in &boxes {
            for b in &boxes {
                if a != b && a.0 == b.0 {
                    assert!((a.1 - b.1).abs() >= (a.2 + b.2) / 2.0 - 1e-6, "{a:?} {b:?}");
                }
            }
        }
        let mut lines = Vec::new();
        connectors(&placed, &|l| l as f64 * 30.0, sz, true, &mut lines);
        // two couples' lines, three families: a stem, a bar (when two children or more) and drops
        assert_eq!(lines.len(), 2 + (1 + 1 + 2) + (1 + 1 + 2) + (1 + 1), "{lines:?}");
    }

    #[test]
    fn a_family_tree_is_made_in_the_hoop() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 160 × 260 (6×10)"})).unwrap();
        let r = s.execute(CMD, &json!({"people": FAMILY, "frame": "rounded", "connectors": "satin"})).unwrap();
        assert_eq!((r["people"].as_u64(), r["generations"].as_u64(), r["fits"].as_bool()), (Some(8), Some(3), Some(true)), "{r}");
        let info = s.execute("embroidery.info", &json!({})).unwrap();
        assert_eq!(info["fits"], true, "{info}");
        let threads: Vec<&str> = info["colors"].as_array().unwrap().iter().filter_map(|c| c["color"].as_str()).collect();
        assert!(threads.contains(&"#1f2933") && threads.contains(&"#8a6d3b") && threads.contains(&"#5b4636"), "{threads:?}");
        // one undo takes it all back
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap_or(0), 0);
        // a big family in a small hoop: made smaller, down to 4 mm letters
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 100 × 100 (4×4)"})).unwrap();
        let r = s.execute(CMD, &json!({"people": FAMILY, "height": 10})).unwrap();
        // 4 mm letters asked: lettered at 5 and made smaller
        s.execute("edit.undo", &json!({})).unwrap();
        let r4 = s.execute(CMD, &json!({"people": "Ann + Bob\n  Cy\n", "height": 4})).unwrap();
        assert_eq!(r4["heightMm"].as_f64(), Some(4.0), "{r4}");
        assert!(r["heightMm"].as_f64().unwrap() < 10.0 && r["heightMm"].as_f64().unwrap() >= MIN_HEIGHT, "{r}");
        // ancestors, given names, double running lines, no frames
        s.execute("embroidery.newDesign", &json!({"hoop": "Brother 200 × 200 (8×8)"})).unwrap();
        let r = s
            .execute(CMD, &json!({"people": "Lucy Hill\n  Tom Hill\n    Ed Hill\n    Rose Hill\n  Anna Green\n", "chart": "ancestors", "names": "given", "double": true}))
            .unwrap();
        assert_eq!((r["people"].as_u64(), r["generations"].as_u64()), (Some(5), Some(3)));
        for bad in [
            json!({"people": FAMILY, "chart": "both"}),
            json!({"people": FAMILY, "frame": "star"}),
            json!({"people": FAMILY, "connectors": "zigzag"}),
            json!({"people": FAMILY, "height": 2}),
            json!({"people": FAMILY, "color": "red"}),
            json!({"people": ""}),
        ] {
            assert!(s.execute(CMD, &bad).is_err(), "{bad}");
        }
    }
}
