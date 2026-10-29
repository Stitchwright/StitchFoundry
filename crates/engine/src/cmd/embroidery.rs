//! Embroidery menu: stitch types and parameters, threads, inclination lines,
//! start/stop beads, brush strokes, artwork conversion and the stitch plan.
//!
//! Positions in params are document points; stitch parameters are millimetres
//! (the units embroidery machines and digitizers use).

use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use sf_core::attr::{DocEmbroidery, Embroidery, StitchType};
use sf_core::brush::{Stamp, WidthProfile, fit_stroke, stamp};
use sf_core::direction::{Guide, auto_fill_angle, auto_guides};
use sf_core::model::{AppliqueParams, FillKind, FillParams, RunKind, RunParams, SatinParams};
use sf_core::threads::{Rgb, Thread};
use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{Appearance, Document, NodeId, NodeKind};
use vectorcraft_embroidery::{MM_PER_PT, PT_PER_MM, Planner, StitchPlan, contour_to_path, is_line_art, is_open, regions_mm, stitchable};

use super::{
    CommandSpec, Point, Result, Session, always, bad, bool_or, cmd, color_value, f64_or, has_doc, has_selection, id_param, leaf_targets, point_param,
    str_param, targets,
};
use crate::EngineError;

/// Per-document stitch planner, shared by commands and the canvas.
#[derive(Clone, Default)]
pub struct EmbroideryState(Arc<Mutex<Planner>>);

impl std::fmt::Debug for EmbroideryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EmbroideryState")
    }
}

impl EmbroideryState {
    /// The stitch plan of `doc` (regenerated only for objects that changed).
    pub fn plan(&self, doc: &Document) -> Arc<StitchPlan> {
        // A poisoned lock only means an earlier plan panicked half way: the cache is still usable.
        let mut p = self.0.lock().unwrap_or_else(|e| e.into_inner());
        p.plan(doc)
    }
}

/// Start and stop bead (document points) of each selected stitched object: where
/// its stitches begin and end in the plan.
pub(crate) fn selection_beads(st: &crate::DocState) -> Vec<(NodeId, Point, Point)> {
    let stitched: Vec<NodeId> = st.selection.objects.iter().copied().filter(|id| st.doc.node(*id).is_some_and(|n| n.embroidery.is_some())).collect();
    if stitched.is_empty() {
        return Vec::new();
    }
    let plan = st.embroidery.plan(&st.doc);
    let pat = plan.pattern();
    let first_last = |id: NodeId| {
        let ranges = plan.records_of(id);
        let mut it = ranges.iter().flat_map(|r| pat.stitches.get(r.clone()).unwrap_or_default().iter()).filter(|s| s.cmd == sf_core::Cmd::Stitch);
        let first = it.next()?.pos();
        let last = it.next_back().map_or(first, |s| s.pos());
        let p = |q: sf_core::Point| vectorcraft_embroidery::pt(q);
        Some((id, p(first), p(last)))
    };
    stitched.into_iter().filter_map(first_last).collect()
}

/// The own stitches (document points) of each selected stitched object, for the
/// Stitch Edit tool: its frozen ones, else those its shape generates.
pub(crate) fn selection_stitches(st: &crate::DocState) -> Vec<(NodeId, Vec<(Point, sf_core::Cmd)>)> {
    let mut out = Vec::new();
    let mut plan = None;
    for id in &st.selection.objects {
        let Some(e) = st.doc.node(*id).and_then(|n| n.embroidery.as_deref()) else { continue };
        let list: Vec<(Point, sf_core::Cmd)> = match &e.frozen {
            Some(f) => f.iter().map(|s| (Point::new(s.x, s.y), s.cmd)).collect(),
            None => {
                let plan = plan.get_or_insert_with(|| st.embroidery.plan(&st.doc));
                plan.own_stitches(*id).iter().map(|s| (vectorcraft_embroidery::pt(s.pos()), s.cmd)).collect()
            }
        };
        out.push((*id, list));
    }
    out
}

impl Session {
    /// The active document's stitch plan.
    pub fn embroidery_plan(&self) -> Option<Arc<StitchPlan>> {
        let st = self.active()?;
        Some(st.embroidery.plan(&st.doc))
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.set",
            "Stitch Type",
            [],
            None,
            "{type: run|bean|backstitch|stemstitch|blanket|zigzag|motif|satin|fill|satinfill|contour|spiral|radial|cross|meander|motiffill|applique|manual|redwork|none, ids?} give the selection (or ids; groups: their paths) a stitch type, keeping thread and inclination lines; a redwork group takes only redwork or none (its art's own stitching back) → {ids}",
            has_selection,
            set_type
        ),
        cmd!(
            "embroidery.setParams",
            "Stitch Parameters",
            [],
            None,
            "{params: {spacing?, stitch_length?, angle?, pull_comp?, pull_comp_2?: mm|null (satin side 2), push_comp?: mm (satin: shorter at open ends; fills: rows short of the outline across them; 0–5), preserve_long?, passes?: 1–9 (runs; 2 = backtrack), kind?, underlay?: {…, double_zigzag?}, corners?: Auto|Turn|Miter|Cap (satin lines), width?, profile?, auto_direction?, trim_after?, own_block?: bool (a colour block of its own: a colour change before it even in the same colour), tie_in?: bool|null, tie_off?: bool|null (null: the design setting), remove_underlying?: bool (fills below leave out what this covers), underlying_overlap?: mm|null (kept under its edge; null: 1 mm), texture?: {pattern: diamonds|waves|herringbone|bricks|circles|scales|hexagons|chevrons|zigzag|ogee|basketweave|stars|hearts|leaves|stripes|slants|pinstripes|lattice|arches|rings|dots|spirals|pyramids|ripples|greekkey|crosses|quatrefoils|flowers|trefoils|moons|arrows|nesteddiamonds|argyle|wicker|feathers|paisley|swirls|snowflakes|bubbles|sunbursts, size?: mm (tile width, 6), angle?: degrees (turn against the rows)}|null (tatami programmed splits), trapunto?: bool (tatami: travel keeps to the outline), jagged?: {sides?: both|first|second, depth?: mm (1.5)}|null (satin: stitches end short of the edge at random), spacing_along?: [[t, mm], …] (satin: the spacing along the column, t 0 at its start to 1 at its end, straight between; [] for even), random_density?: 0–0.9 (satin: each row's spacing changed by up to this share at random, the same every time; 0 even), spacing_profile?: [[t, mm], …] (tatami and satin fills: the row spacing across the shape, 0 for no rows), rings?: n (contour fills: rings in from the outline; 0 = all), motif?, motif_size?: mm (motif fills), motif_pair?: motif|null (motif fills: a second motif in turn with the first), warp?: {kind?: GlobeOut|GlobeIn|Perspective, amount?: 0–0.8 (0.5), angle?: degrees, the way a perspective recedes (90: up)}|null (motif fills: a 3D motif fill, a dome, a bowl or receding), custom_motif?: {name, points: [[along, across], …], aspect}|null (motif runs and fills: a motif drawn for the design, as embroidery.motif.make makes it, sewn instead of `motif`), groups?: [triple, single] (sculpture runs: how many triple stitches `stitch_length` long, then single ones `spacing` long; 3 and 1), cross?: Full|Half|Upright|Double (cross fills), motif_column?: {motif?, custom_motif?, length?: × the width (1.5), stitch_length?: mm (1.5)}|null (satin: sewn as motifs as wide as the column), …} (mm and degrees; camelCase also accepted), ids?} merge stitch parameters into the selected stitched objects → {ids}",
            has_selection,
            set_params
        ),
        cmd!(
            "embroidery.setThread",
            "Set Thread",
            [],
            None,
            "{color: \"#rrggbb\" | null (use the paint colour), name?, code?, chart?, paint?: bool (default true: recolour the art to match), ids?} → {ids}; art not stitched yet becomes a fill (shapes) or a run (open lines)",
            has_selection,
            set_thread
        ),
        cmd!(
            "embroidery.matchThreads",
            "Match Threads to Chart",
            [],
            None,
            "{chart? (default: the design's), all?: bool, paint?: bool (default true), ids?} give stitched objects (the selection; all of them with `all` or nothing selected) the chart's closest thread, recolouring their art → {matched, chart}",
            has_doc,
            match_threads
        ),
        cmd!(
            "embroidery.autoDirection",
            "Auto Stitch Direction",
            ["Embroidery"],
            None,
            "{ids?} place inclination lines along column-like shapes (stitches turn with the shape) or pick the straight angle with the fewest breaks; the result is ordinary, editable inclination lines → {ids, guides, angles}",
            has_selection,
            auto_direction
        ),
        cmd!(
            "embroidery.setGuides",
            "Set Inclination Lines",
            [],
            None,
            "{id, guides: [[x1, y1, x2, y2], …]} (points) replace an object's inclination lines",
            has_doc,
            set_guides
        ),
        cmd!(
            "embroidery.addGuide",
            "Add Inclination Line",
            [],
            None,
            "{id, a: [x, y], b: [x, y], index?} add an inclination line (stitches run parallel to it) → {index}",
            has_doc,
            add_guide
        ),
        cmd!(
            "embroidery.moveGuide",
            "Move Inclination Line",
            [],
            None,
            "{id, index, a?: [x, y], b?: [x, y]} move one end (or both) of an inclination line",
            has_doc,
            move_guide
        ),
        cmd!("embroidery.removeGuide", "Delete Inclination Line", [], None, "{id, index}", has_doc, remove_guide),
        cmd!(
            "embroidery.clearGuides",
            "Clear Inclination Lines",
            ["Embroidery"],
            None,
            "{ids?} remove inclination lines and go back to automatic stitch direction",
            has_selection,
            clear_guides
        ),
        cmd!(
            "embroidery.setBeads",
            "Set Start and Stop",
            [],
            None,
            "{id, entry?: [x, y] | null, exit?: [x, y] | null} where sewing starts (green bead) and stops (red bead); null = automatic closest join",
            has_doc,
            set_beads
        ),
        cmd!(
            "embroidery.convert",
            "Convert Artwork to Embroidery",
            ["Embroidery"],
            None,
            "{ids?, force?: bool} give each path without embroidery a stitch type from its look: filled shapes become tatami (turning satin when long and thin), strokes satin lines or runs → {ids}",
            has_selection,
            convert_art
        ),
        cmd!(
            query "embroidery.export",
            "Export Machine File",
            ["Embroidery"],
            None,
            "{format?: dst|exp|jef|pes|pec|vp3|xxx|u01|pcs|tap|dsb (the design's machine, else dst; or an open text export: csv|json (stitch lists: x, y in mm from the centre, y down, command, thread) or gcode (G21 G90; G0 jumps, G1 stitches each followed by a needle cycle; thread changes pause)), name?: design label, center?: bool (true), gcode?: {feed?: mm/min (1200), zStep?: Z advance a stitch (1; 0: none), stitchCommand?: after each stitch (none), colorCommand?: at a thread change or stop (M0), trimCommand?: at a trim (none), flipY?: bool (true: y up)}} the stitch plan as a machine embroidery file → {dataBase64, format, bytes, stitches, colors, warnings: [what the machine may refuse or show otherwise: larger than the hoop or than the format's largest, thread colours a palette format shows as its nearest, colours the format doesn't store, stops written as colour changes]}",
            has_doc,
            export_machine_file
        ),
        cmd!(
            "embroidery.autoDigitize",
            "Auto-Digitize Image",
            ["Embroidery"],
            None,
            "{colors?: 2-32 (6), detail?: 0-100 (60), noise?: px (16), removeBackground?: bool (true), minArea?: mm² (1), size?: mm (longest side), keepImage?: bool (true), details?: mm (1.2: shapes no thicker, fine lines and details, sewn as a run round them on top of the rest, kept however small when they are lines; 0 none), outline?: none|design|shapes (none: a satin outline round the whole design, or round each filled shape), outlineWidth?: mm (1.5), outlineColor?: \"#rrggbb\" (the design's darkest thread), border?: mm (0: none; else a filled band this wide round the design, as a sticker's or patch's border, as embroidery.addBorder makes), borderColor?: \"#rrggbb\" (#ffffff), borderEdge?: satin|run|none (satin), chart?: a thread chart to match the threads to (none)} trace the selected image into flat colour shapes and give each a stitch type: satin for narrow parts, tatami at its best angle for the rest, details as runs, specks and the background dropped, sewn colour by colour, outlined and matched to a chart if asked, small holes and gaps where nothing is sewn closed in the colour round them → {id, objects, satin, fill, run, outlines, border (its id), removed, holesClosed, gapsFilled, colors, scale}",
            has_selection,
            auto_digitize
        ),
        cmd!(
            "embroidery.cost",
            "Cost Estimate",
            [],
            None,
            "{quantity?: pieces (1; 1–1000000)} what sewing the design costs by its cost settings (embroidery.settings {cost}; else 1 a 1000 stitches and 3 a 1000 m of thread): stitches, machine time shared over its heads, thread and the set-up spread over the order → {currency, quantity, perPiece, total, stitches, time, thread, setup, text}",
            has_doc,
            cost
        ),
        cmd!(
            "embroidery.outlineAround",
            "Outline Around",
            ["Embroidery"],
            None,
            "{ids?, gap?: mm between the art and the outline (2; 0–50), width?: mm (satin 3, 0.5–10), kind?: satin|run|triple|knockdown|emboss|echo|stipple (satin), shape?: rectangle|circle (emboss and stipple; rectangle), extend?: mm the emboss or stipple shape reaches beyond (5; 0–200), rect?: [x, y, width, height] mm (emboss and stipple: that rectangle instead), count?: echo lines (3; 1–30), spacing?: mm between echo lines (3; 0.5–50), color?: \"#rrggbb\" (#1a1a1a)} bubble lettering, sticker and patch borders: one outline round the selected stitched art as it sews (lettering, shapes, lines), its holes left out, `gap` from it, sewn after everything else; knockdown: a light tatami fill of that area sewn first, under the art (towels, fleece); emboss: a rectangle or circle round it filled with light tatami but for the art's area, sewn first; echo quilting: `count` running-stitch outlines, the first `gap` from the art, each `spacing` further out; stipple: background quilting, a rectangle or circle round it filled with a meander but for the art's area, sewn first; a new path, selected → {id, pieces}",
            has_selection,
            outline_around
        ),
        cmd!(
            "embroidery.worksheet",
            "Worksheet",
            ["Embroidery"],
            None,
            "{kind?: production|approval|pattern|template (production; template: the design life size with lines through its middle, which way is up and a 100 mm bar to check the print, to lay on the garment), placement?: left-chest|right-chest|full-front|full-back|cap-front|sleeve|yoke|pocket (the usual size and place for it, printed), page?: a4|letter (a4), customer?: text, notes?: text (the design's own notes when not given), barcode?: code128|code39|none (code128: the job's barcode at the top right), barcodeText?: text (the design's name; up to 48 characters, Code 39: digits, letters, space and -.$/+%), colorways?: bool (true: each colorway's threads, colour by colour, when the design has any), hide?: [figures|colors|colorways|notes|barcode] (the setup's, embroidery.worksheet.setup)} the shop's name, details and logo from the worksheet setup in the footer; a worksheet for the design as a new A4 or Letter document to print or export (PDF): the design drawn in its thread colours (life size when it fits), its size, stitches, colours, colour changes, trims, thread and sewing time, hoop, fabric and machine, a table of its colours in sewing order (thread, stitches, length), the notes, and on an approval sheet the customer and a line to sign; a pattern is the design over the whole page with one line of figures; opens a new document → {kind, colors, scale}",
            has_doc,
            worksheet
        ),
        cmd!(
            "embroidery.send",
            "Send to Machine",
            ["Embroidery"],
            None,
            "{folder?: the machine's folder (a USB stick, the machine's folder or a network share; default: the last one used), format?: as Export Machine File (the design's machine, else dst), name?: the file's name (the design's), replace?: bool (false: a number is added when the name is taken)} send to machine: the design's machine file written to the folder, which is checked first and remembered → {path, format, bytes, stitches, warnings (as embroidery.export's)}",
            has_doc,
            send_to_machine
        ),
        cmd!(
            "embroidery.multiHoop",
            "Multi-Hooping",
            [],
            None,
            "{dir?: the folder to write to (without it: the plan only), hoop?: a standard hoop's name (default: the design's), width?, height?: mm (a hoop of this field instead), overlap?: mm the hoopings share (10; 0–100), format?: as Export Machine File, prefix?: what the file names start with, marks?: bool (true), split?: bool (true: an object no hooping holds whole is split, each stitch sewn in the hooping whose middle is nearest it, each piece tied in and off; false: refused), sheets?: bool (false: true opens a document of a page per hooping, to print: its stitches in its hoop, life size when they fit, its place in the grid and its file)} multi-hooping: a design larger than the hoop split into hoopings on a grid that overlap by `overlap`, each object sewn whole in the first hooping that holds it, each hooping a machine file of its own (centred on the hoop, `name-1`, `name-2`…) that first sews registration crosses in the overlaps it shares with its neighbours, as a colour of its own, to line the fabric up by; with a multi-position hoop (`embroidery.hoops`: positions) the frame's positions are the hoopings, hooped once, without marks → {hoopings: [{index, column, row, objects, stitches, file?}], columns, rows, files}",
            has_doc,
            multi_hoop
        ),
        cmd!(
            "embroidery.send.targets",
            "Machine Folders",
            [],
            None,
            "{add?: a folder to remember, forget?: a folder to forget} the machine folders designs are sent to, the last used first, each with whether it is there now (a USB stick plugged in, a share reachable) → {targets: [{folder, ready}]}",
            always,
            send_targets
        ),
        cmd!(
            "embroidery.clickStitch",
            "Click to Stitch",
            [],
            None,
            "{at: [x, y] (points) a spot on an image, as?: fill|fillNoHoles|satin|outline|centerline (fill), tolerance?: 0–255 how far a colour may be from the spot's and still join the region (32), contiguous?: bool (true: only the pixels joined to the spot; false: every pixel of about its colour across the image), minHole?: px holes smaller than this are filled (16), id?: the image (default: the topmost one under the spot)} click-to-stitch: the region of the image round the spot (its pixels joined to it in about its colour) becomes a stitched object in its colour: tatami with or without its holes, satin across it, a running outline, or running centre lines; added over the image and selected → {id, as, pixels, color}",
            has_doc,
            click_stitch
        ),
        cmd!(
            "embroidery.draw",
            "Brush Stroke",
            [],
            None,
            "{brush: run|bean|steil|tapered|calligraphy|pressure|applique|fill|satin, points?: [[x, y], …], closed?, stamp?: ellipse|rectangle|roundedRectangle|triangle|diamond|pentagon|hexagon|star|heart|arrow|line, from?: [x, y], to?: [x, y], square?, width?: mm, density?: mm, length?: mm, nibAngle?: deg, pressures?: [0..1], color?, smooth?: pt, order?: top|before|after, relativeTo?: id} draw a stitched object with a widget brush (order: sewn last, or just before / after object relativeTo) → {id}",
            has_doc,
            draw
        ),
        cmd!(
            "embroidery.settings",
            "Embroidery Settings",
            [],
            None,
            "{hoop?: name | {width, height, shape?: rect|oval} (mm) | null (the page takes the hoop's size about its middle), page?: bool (true; false: the page keeps its size), fabric?, chart?, machine?: a machine profile (dst|exp|u01|pes|jef|vp3|xxx, or one of yours: embroidery.machine.list) | null (sets its values first; the design is exported in its format), trimThreshold?, overlapTouching?: mm (0.5; 0: shapes as drawn; up to 3: how far a fill reaches in under a shape sewn after it that it touches, so no fabric shows between them), hideTravel?: bool (true: between objects of one thread that touch or overlap, the thread runs hidden under the next one instead of a trim), tieIn?, tieOff?, tieLength?, tieStyle?: single|double|triangle, minStitch?, maxStitch?, maxJump?, machineSpeed?, trimSeconds?, colorChangeSeconds?, notes?: text (kept with the design, printed on its worksheets; 2000 characters at most), cost?: true | {perThousand?: price a 1000 stitches (0–1000), perHour?: machine time (0–10000), threadPerKm?: thread a 1000 m (0–10000), setup?: an order (0–1000000), heads?: pieces sewn at once (1–100), currency?: up to 4 characters} | null (the cost estimate shown with the figures and on worksheets), basting?: bool | {hoop?: bool (round the hoop's field, else the design), margin?: mm (5; 0–50: out from the design, in from the hoop's edge), length?: mm (6; 2–12)} | null (a basting run sewn first, in the first thread), start?: centre|top-left|top|top-right|left|right|bottom-left|bottom|bottom-right|null (the first object entered closest to that point of the design's box; null: where it starts), end?: the same|null (the frame there when the design is done, after a trim), checks?: {denseLayers?, longStitch?, smallLettering?, tinyObject?} (what Check Design flags), hoopPosition?: [x, y] mm (the design's middle from the hoop's centre; centre or null: centred), registration?: {shape?: cross|circle|target, size?: mm (1–20), perOverlap?: 1–4, color?: \"#rrggbb\"} (the marks that line up the hoopings of a design split for several hoopings)} → the settings",
            has_doc,
            settings
        ),
        cmd!(
            "embroidery.applyFabric",
            "Apply Fabric",
            [],
            None,
            "{name} set density, stitch length, pull and push compensation and underlay (satin underlay type, fill edge run and tatami) of every stitched object for a fabric, and whether Check Design treats the design as clothing (see embroidery.info: fabrics)",
            has_doc,
            apply_fabric
        ),
        cmd!(
            "embroidery.sortByColor",
            "Sort by Thread Color",
            ["Embroidery"],
            None,
            "{} reorder stitched objects within each layer so each thread colour is sewn in one go (fewer colour changes) → {moved}",
            has_doc,
            sort_by_color
        ),
        cmd!(query
            "embroidery.info",
            "Design Information",
            [],
            None,
            "{} → {grade (A: made here, objects with their stitch settings; C: opened from a machine file, its stitches made into objects; D: a machine file, stitches only), openedFrom (that file's format, else empty), objects, stitches, jumps, trims, colorChanges, stops, widthMm, heightMm, threadM, bobbinM, seconds, time, colors: [{color, name, stitches, threadM}], hoop, fits, fabric, fabrics, hoops, charts, machine, machineFormat (what it exports in), machines (as embroidery.machine.list), settings}",
            has_doc,
            info
        ),
        cmd!(query
            "embroidery.plan",
            "Stitch Plan",
            [],
            None,
            "{limit?} → {threads: [\"#rrggbb\"], stitches: [[x, y, cmd], …] (points; cmd: stitch|jump|trim|color|stop|end), nodes: [id per object]}",
            has_doc,
            plan
        ),
    ]
}

// ---------- helpers ----------

/// Converts `camelCase` keys to `snake_case` (params accept both).
/// `pullComp2` → `pull_comp_2` (snake_case keys pass through unchanged).
fn snake(k: &str) -> String {
    let mut out = String::with_capacity(k.len() + 4);
    let mut prev_letter = false;
    for ch in k.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_ascii_digit() && prev_letter {
            out.push('_');
            out.push(ch);
        } else {
            out.push(ch);
        }
        prev_letter = ch.is_ascii_alphabetic();
    }
    out
}

/// Recursive JSON merge: objects merge key by key, anything else replaces.
fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                let k = snake(k);
                match t.get_mut(&k) {
                    Some(slot) if slot.is_object() && v.is_object() => merge(slot, v),
                    _ => {
                        let mut v = v.clone();
                        snake_keys(&mut v);
                        t.insert(k, v);
                    }
                }
            }
        }
        (t, p) => *t = p.clone(),
    }
}

fn snake_keys(v: &mut Value) {
    if let Value::Object(m) = v {
        let old = std::mem::take(m);
        for (k, mut val) in old {
            snake_keys(&mut val);
            m.insert(snake(&k), val);
        }
    }
}

/// The stitch type an `embroidery.set` type names: `Some(None)` for `none`.
pub(crate) fn stitch_type(name: &str) -> Option<Option<StitchType>> {
    if name == "none" {
        return Some(None);
    }
    StitchType::from_name(name).map(Some)
}

/// The stitch type menu entry a type name or alias stands for.
fn canonical(name: &str) -> &str {
    match name {
        "single" => "run",
        "triple" => "bean",
        "back" => "backstitch",
        "stem" => "stemstitch",
        "steil" => "satin",
        "tatami" => "fill",
        n => n,
    }
}

fn guide_pt(g: &Guide) -> Value {
    json!([g.a.x, g.a.y, g.b.x, g.b.y])
}

fn sf_point(p: Point) -> sf_core::Point {
    sf_core::Point::new(p.x, p.y)
}

/// The stitched paths among the targets (groups and layers expand to their contents); art in a
/// redwork group stands for the group, which is stitched as a whole.
pub(crate) fn stitched_targets(s: &Session, p: &Value) -> Result<Vec<NodeId>> {
    let ids = targets(s, p)?;
    let d = &s.doc()?.doc;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in leaf_targets(s, &ids)? {
        if d.node(id).is_some_and(stitchable) {
            let id = redwork_owner(d, id).unwrap_or(id);
            if seen.insert(id) {
                out.push(id);
            }
        }
    }
    Ok(out)
}

/// The outermost redwork group `id` is in, if any.
pub(crate) fn redwork_owner(d: &Document, id: NodeId) -> Option<NodeId> {
    let mut owner = None;
    let mut at = d.parent_of(id);
    while let Some(p) = at {
        if d.node(p).is_some_and(vectorcraft_embroidery::sews_whole) {
            owner = Some(p);
        }
        at = d.parent_of(p);
    }
    owner
}

pub(crate) fn ids_json(ids: &[NodeId]) -> Value {
    json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() })
}

fn rgb_of(c: &Color) -> Rgb {
    let v = c.to_rgba8(1.0);
    Rgb::new(v[0], v[1], v[2])
}

// ---------- commands ----------

/// `embroidery.set`: shapes made satin get their editable inclination lines at once, as
/// Auto Direction places them (a satin brush places its direction lines as it is drawn), so the
/// panel and the Inclination tool show them to adjust; one undo step.
fn set_type(s: &mut Session, p: &Value) -> Result<Value> {
    super::embroidery_simple::one_step(s, |s| {
        let r = set_stitch_type(s, p)?;
        if str_param(p, "type").map(canonical) == Some("satin") {
            let d = &s.doc()?.doc;
            let fresh: Vec<u64> = r["ids"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(Value::as_u64)
                .filter(|id| {
                    d.node(NodeId(*id))
                        .is_some_and(|n| !is_line_art(n) && n.embroidery.as_deref().is_some_and(|e| e.guides.is_empty() && e.auto_direction))
                })
                .collect();
            if !fresh.is_empty() {
                auto_direction(s, &json!({ "ids": fresh }))?;
            }
        }
        Ok(r)
    })
}

fn set_stitch_type(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.set";
    let name = str_param(p, "type").ok_or_else(|| bad(CMD, "missing `type`"))?;
    let st = stitch_type(name).ok_or_else(|| bad(CMD, format!("unknown stitch type `{name}`")))?;
    let ids = stitched_targets(s, p)?;
    if ids.is_empty() {
        return Err(bad(CMD, "select a path or compound path to stitch"));
    }
    // the properties made current for the type (not when a run style of it is asked for)
    let current = match &st {
        Some(st) if st.type_name() == canonical(name) => current_props(s, st.type_name()),
        _ => None,
    };
    s.edit("Stitch Type", |d, _| {
        for id in &ids {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            // a redwork group stays redwork (or, with none, gives its art back its own stitching)
            if vectorcraft_embroidery::sews_whole(n) && !matches!(st, None | Some(StitchType::Redwork(_))) {
                continue;
            }
            match (&st, n.embroidery.as_deref_mut()) {
                (None, _) => n.embroidery = None,
                // a stitched object takes the type's stitch parameters and keeps the rest
                (Some(st), Some(e)) => e.stitch = current.as_ref().map_or_else(|| st.clone(), |c| c.stitch.clone()),
                (Some(st), None) => n.embroidery = Some(Box::new(current.clone().unwrap_or_else(|| Embroidery::new(st.clone())))),
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}

/// `e` with a parameter patch merged in (camelCase or snake_case names): the
/// object-level options go to the record, the rest to its stitch parameters. An error
/// says what is wrong with the patch.
pub(crate) fn patched(e: &Embroidery, patch: &Value) -> std::result::Result<Embroidery, String> {
    let mut v = serde_json::to_value(e).map_err(|err| err.to_string())?;
    let mut top = Map::new();
    let mut stitch = Map::new();
    if let Value::Object(m) = patch {
        for (k, val) in m {
            let key = snake(k);
            if matches!(
                key.as_str(),
                "width"
                    | "profile"
                    | "auto_direction"
                    | "trim_after"
                    | "stop_after"
                    | "own_block"
                    | "tie_in"
                    | "tie_off"
                    | "remove_underlying"
                    | "underlying_overlap"
                    | "omit_last"
            ) {
                top.insert(key, val.clone());
            } else {
                stitch.insert(key, val.clone());
            }
        }
    }
    merge(&mut v, &Value::Object(top));
    if let Some(slot) = v.get_mut("stitch") {
        merge(slot, &Value::Object(stitch));
    }
    serde_json::from_value(v).map_err(|err| format!("invalid stitch parameters: {err}"))
}

fn set_params(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.setParams";
    let patch = p.get("params").filter(|v| v.is_object()).ok_or_else(|| bad(CMD, "missing object `params`"))?;
    let ids = stitched_targets(s, p)?;
    // Validate on copies first so a bad value changes nothing.
    let mut updated = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(e) = d.node(*id).and_then(|n| n.embroidery.as_deref()) else { continue };
            let e = patched(e, patch).map_err(|m| bad(CMD, m))?;
            updated.push((*id, e));
        }
    }
    if updated.is_empty() {
        return Err(bad(CMD, "select stitched objects (Embroidery › Stitch Type first)"));
    }
    s.edit("Stitch Parameters", |d, _| {
        for (id, e) in &updated {
            d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery = Some(Box::new(e.clone()));
        }
        Ok(())
    })?;
    Ok(ids_json(&updated.iter().map(|u| u.0).collect::<Vec<_>>()))
}

fn set_thread(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.setThread";
    let color = match p.get("color") {
        None => return Err(bad(CMD, "missing `color` (\"#rrggbb\" or null)")),
        Some(Value::Null) => None,
        Some(v) => Some(color_value(v).ok_or_else(|| bad(CMD, format!("not a colour: {v}")))?),
    };
    let repaint = bool_or(p, "paint", true);
    let ids = stitched_targets(s, p)?;
    let thread = color.as_ref().map(|c| {
        let rgb = rgb_of(c);
        Thread {
            color: rgb,
            name: str_param(p, "name").map(str::to_string).unwrap_or_else(|| rgb.to_hex()),
            code: str_param(p, "code").unwrap_or_default().to_string(),
            chart: str_param(p, "chart").unwrap_or_default().to_string(),
        }
    });
    let new = new_stitching(s);
    s.edit("Set Thread", |d, _| {
        for id in &ids {
            put_thread(d.node_mut(*id).ok_or(EngineError::NoNode(*id))?, &new, thread.clone(), color.filter(|_| repaint));
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}

/// The properties new objects of stitch type `key` take: the design's own (Properties for New
/// Objects), else the user's defaults for every design (`embroidery.current.saveAsDefault`).
pub(crate) fn current_props(s: &Session, key: &str) -> Option<Embroidery> {
    let own = s.doc().ok().and_then(|st| st.doc.embroidery.as_deref().and_then(|c| c.current_for(key).cloned()));
    own.or_else(|| s.prefs.embroidery_default_properties.get(key).filter(|e| e.stitch.type_name() == key).cloned())
}

/// What unstitched art becomes when it gets a thread: shapes a fill, open lines a run (each with
/// the properties made current for it).
pub(super) fn new_stitching(s: &Session) -> [Embroidery; 2] {
    [current_props(s, "fill").unwrap_or_else(Embroidery::fill), current_props(s, "run").unwrap_or_else(Embroidery::run)]
}

/// Gives a node its thread (unstitched art becomes `new`: `[fill, run]` for shapes and open
/// lines), and paints its art `repaint`: the stroke of lines, runs and satin borders, else the
/// fill.
pub(super) fn put_thread(n: &mut vectorcraft_doc::Node, new: &[Embroidery; 2], thread: Option<Thread>, repaint: Option<Color>) {
    let (lines, border) = (is_open(n), is_line_art(n));
    let e = n.embroidery.get_or_insert_with(|| Box::new(if lines { new[1].clone() } else { new[0].clone() }));
    e.thread = thread;
    let stroke_paint = lines || matches!(e.stitch, StitchType::Run(_) | StitchType::Manual) || (border && matches!(e.stitch, StitchType::Satin(_)));
    if let Some(c) = repaint {
        if stroke_paint {
            n.appearance.set_stroke(Paint::solid(c));
        } else {
            n.appearance.set_fill(Paint::solid(c));
        }
    }
}

fn match_threads(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.matchThreads";
    let cfg = s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default();
    let name = str_param(p, "chart").map_or_else(|| cfg.chart.clone(), str::to_string);
    let chart = super::embroidery_charts::find_chart(s, &name).ok_or_else(|| bad(CMD, format!("unknown thread chart `{name}`")))?;
    let repaint = bool_or(p, "paint", true);
    // the selection's stitched objects; every one with `all` or nothing selected
    let everything = bool_or(p, "all", false) || (p.get("ids").is_none() && s.doc()?.selection.objects.is_empty());
    let ids: Vec<NodeId> =
        if everything { vectorcraft_embroidery::stitched_nodes(&s.doc()?.doc).iter().map(|n| n.id).collect() } else { stitched_targets(s, p)? };
    let plans: Vec<(NodeId, Thread)> = {
        let d = &s.doc()?.doc;
        ids.iter()
            .filter_map(|id| {
                let n = d.node(*id)?;
                let e = n.embroidery.as_deref()?;
                if matches!(e.stitch, StitchType::Branch) || matches!(n.kind, NodeKind::Group { .. } | NodeKind::Layer { .. }) {
                    return None;
                }
                let now = vectorcraft_embroidery::node_thread(n, e).color;
                chart.threads.get(chart.nearest(now)).map(|t| (*id, t.clone()))
            })
            .collect()
    };
    if plans.is_empty() {
        return Err(bad(CMD, "nothing stitched to match"));
    }
    let new = new_stitching(s);
    s.edit("Match Threads to Chart", |d, _| {
        for (id, t) in &plans {
            let c = Color::rgb8(t.color.r, t.color.g, t.color.b);
            put_thread(d.node_mut(*id).ok_or(EngineError::NoNode(*id))?, &new, Some(t.clone()), repaint.then_some(c));
        }
        Ok(())
    })?;
    Ok(json!({ "matched": plans.len(), "chart": chart.name }))
}

fn auto_direction(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = stitched_targets(s, p)?;
    let mut changes: Vec<(NodeId, Vec<Guide>, Option<f64>)> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(n) = d.node(*id) else { continue };
            let Some(e) = n.embroidery.as_deref() else { continue };
            let satin = match &e.stitch {
                StitchType::Satin(_) => true,
                StitchType::Fill(f) => f.kind == FillKind::Satin,
                _ => false,
            };
            let is_area = matches!(e.stitch, StitchType::Fill(_)) || (matches!(e.stitch, StitchType::Satin(_)) && !is_line_art(n));
            if !is_area {
                continue;
            }
            let regions = regions_mm(n);
            let Some(region) = regions.iter().max_by(|a, b| a.area().total_cmp(&b.area())) else { continue };
            let spacing = match &e.stitch {
                StitchType::Fill(f) => f.spacing,
                _ => 0.4,
            };
            // One part without holes can take editable inclination lines.
            let guides = if region.holes.is_empty() && regions.len() == 1 { auto_guides(&region.outer) } else { None };
            match guides {
                Some(g) if satin || long_and_thin(region, &g) => {
                    let pts = g.iter().map(|g| Guide::new(g.a * PT_PER_MM, g.b * PT_PER_MM)).collect();
                    changes.push((*id, pts, None));
                }
                // Satin that can't turn as one column (a star, type) stays automatic: it is cut into
                // simple columns, each with its own direction.
                _ if satin => changes.push((*id, Vec::new(), None)),
                _ => changes.push((*id, Vec::new(), Some(auto_fill_angle(region, spacing.max(0.8))))),
            }
        }
    }
    let summary: Vec<Value> = changes.iter().map(|(id, g, a)| json!({"id": id.0, "guides": g.len(), "angle": a})).collect();
    s.edit("Auto Stitch Direction", |d, _| {
        for (id, guides, angle) in &changes {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            if let Some(e) = n.embroidery.as_deref_mut() {
                e.guides = guides.clone();
                // Lines or an angle are now the object's own; otherwise it stays automatic.
                e.auto_direction = guides.is_empty() && angle.is_none();
                if let (Some(a), StitchType::Fill(f)) = (angle, &mut e.stitch) {
                    f.angle = *a;
                }
            }
        }
        Ok(())
    })?;
    Ok(json!({ "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>(), "objects": summary }))
}

/// Long thin shapes read better with stitches that turn along them. Elongation
/// (half perimeter)² / area is 4 for a square and about 7 for a 5:1 bar.
fn long_and_thin(region: &sf_core::geom::Polygon, guides: &[Guide]) -> bool {
    let area = region.area();
    let half_perimeter = region.perimeter() * 0.5;
    !guides.is_empty() && area > 1e-9 && half_perimeter * half_perimeter / area > 6.5
}

fn guide_target(s: &Session, p: &Value, cmd: &str) -> Result<NodeId> {
    let id = id_param(p, "id").ok_or_else(|| bad(cmd, "missing object `id`"))?;
    let has = s.doc()?.doc.node(id).ok_or_else(|| bad(cmd, format!("no such object id {}", id.0)))?.embroidery.is_some();
    if !has {
        return Err(bad(cmd, "the object has no stitch type"));
    }
    Ok(id)
}

fn edit_embroidery(s: &mut Session, label: &str, id: NodeId, f: impl FnOnce(&mut Embroidery) -> Result<Value>) -> Result<Value> {
    s.edit(label, |d, _| {
        let n = d.node_mut(id).ok_or(EngineError::NoNode(id))?;
        let e = n.embroidery.as_deref_mut().ok_or(EngineError::NoNode(id))?;
        f(e)
    })
}

fn guide_from(p: &Value, cmd: &str) -> Result<Guide> {
    let a = point_param(p, "a").ok_or_else(|| bad(cmd, "missing point `a` [x, y]"))?;
    let b = point_param(p, "b").ok_or_else(|| bad(cmd, "missing point `b` [x, y]"))?;
    if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) || (a - b).hypot() < 1e-6 {
        return Err(bad(cmd, "an inclination line needs two different points"));
    }
    Ok(Guide::new(sf_point(a), sf_point(b)))
}

fn set_guides(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.setGuides";
    let id = guide_target(s, p, CMD)?;
    let list = p.get("guides").and_then(Value::as_array).ok_or_else(|| bad(CMD, "missing `guides` array"))?;
    let mut guides = Vec::with_capacity(list.len());
    for g in list {
        let c: Vec<f64> = g.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
        let [x1, y1, x2, y2] = c.as_slice() else {
            return Err(bad(CMD, format!("each guide is [x1, y1, x2, y2], not {g}")));
        };
        if !(x1.is_finite() && y1.is_finite() && x2.is_finite() && y2.is_finite()) {
            return Err(bad(CMD, "guide coordinates must be finite"));
        }
        guides.push(Guide::new(sf_core::Point::new(*x1, *y1), sf_core::Point::new(*x2, *y2)));
    }
    edit_embroidery(s, "Set Inclination Lines", id, |e| {
        e.guides = guides;
        e.auto_direction = false;
        Ok(json!({ "guides": e.guides.len() }))
    })
}

fn add_guide(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.addGuide";
    let id = guide_target(s, p, CMD)?;
    let g = guide_from(p, CMD)?;
    let index = p.get("index").and_then(Value::as_u64).map(|i| i as usize);
    edit_embroidery(s, "Add Inclination Line", id, |e| {
        // Insert where it fits along the shape: between the guides it lies between.
        let at = index.unwrap_or_else(|| best_insert(&e.guides, &g)).min(e.guides.len());
        e.guides.insert(at, g);
        e.auto_direction = false;
        Ok(json!({ "index": at }))
    })
}

/// Index that keeps guides in order along the shape (nearest neighbours).
pub(super) fn best_insert(guides: &[Guide], g: &Guide) -> usize {
    if guides.len() < 2 {
        return guides.len();
    }
    let m = g.midpoint();
    let mut best = (f64::INFINITY, guides.len());
    for i in 0..=guides.len() {
        // cost of putting it between i-1 and i
        let prev = i.checked_sub(1).and_then(|k| guides.get(k)).map(Guide::midpoint);
        let next = guides.get(i).map(Guide::midpoint);
        let cost = match (prev, next) {
            (Some(a), Some(b)) => a.distance(m) + m.distance(b) - a.distance(b),
            (Some(a), None) => a.distance(m),
            (None, Some(b)) => m.distance(b),
            (None, None) => 0.0,
        };
        if cost < best.0 {
            best = (cost, i);
        }
    }
    best.1
}

fn move_guide(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.moveGuide";
    let id = guide_target(s, p, CMD)?;
    let index = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(CMD, "missing `index`"))? as usize;
    let a = point_param(p, "a").filter(|q| q.x.is_finite() && q.y.is_finite());
    let b = point_param(p, "b").filter(|q| q.x.is_finite() && q.y.is_finite());
    edit_embroidery(s, "Move Inclination Line", id, |e| {
        let g = e.guides.get_mut(index).ok_or_else(|| bad(CMD, format!("no inclination line {index}")))?;
        if let Some(a) = a {
            g.a = sf_point(a);
        }
        if let Some(b) = b {
            g.b = sf_point(b);
        }
        Ok(guide_pt(g))
    })
}

fn remove_guide(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.removeGuide";
    let id = guide_target(s, p, CMD)?;
    let index = p.get("index").and_then(Value::as_u64).ok_or_else(|| bad(CMD, "missing `index`"))? as usize;
    edit_embroidery(s, "Delete Inclination Line", id, |e| {
        if index >= e.guides.len() {
            return Err(bad(CMD, format!("no inclination line {index}")));
        }
        e.guides.remove(index);
        Ok(json!({ "guides": e.guides.len() }))
    })
}

fn clear_guides(s: &mut Session, p: &Value) -> Result<Value> {
    let ids = stitched_targets(s, p)?;
    s.edit("Clear Inclination Lines", |d, _| {
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.guides.clear();
                e.auto_direction = true;
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}

fn set_beads(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.setBeads";
    let id = guide_target(s, p, CMD)?;
    let read = |key: &str| -> Result<Option<Option<sf_core::Point>>> {
        match p.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(_) => {
                let q = point_param(p, key)
                    .filter(|q| q.x.is_finite() && q.y.is_finite())
                    .ok_or_else(|| bad(CMD, format!("`{key}` is [x, y] or null")))?;
                Ok(Some(Some(sf_point(q))))
            }
        }
    };
    let entry = read("entry")?;
    let exit = read("exit")?;
    edit_embroidery(s, "Set Start and Stop", id, |e| {
        if let Some(v) = entry {
            e.entry = v;
        }
        if let Some(v) = exit {
            e.exit = v;
        }
        Ok(json!({ "entry": e.entry.map(|q| [q.x, q.y]), "exit": e.exit.map(|q| [q.x, q.y]) }))
    })
}

fn convert_art(s: &mut Session, p: &Value) -> Result<Value> {
    let force = bool_or(p, "force", false);
    let ids = stitched_targets(s, p)?;
    let mut plans: Vec<(NodeId, Embroidery)> = Vec::new();
    {
        let d = &s.doc()?.doc;
        for id in &ids {
            let Some(n) = d.node(*id) else { continue };
            if n.embroidery.is_some() && !force {
                continue;
            }
            let mut e = vectorcraft_embroidery::suggest(n);
            if let Some(c) = current_props(s, e.stitch.type_name()) {
                // the type's current properties; a width read from the stroke stays
                let width = e.width;
                e.take_properties(&c);
                e.width = width.or(e.width);
            }
            plans.push((*id, e));
        }
    }
    let done: Vec<NodeId> = plans.iter().map(|p| p.0).collect();
    if plans.is_empty() {
        return Ok(ids_json(&done));
    }
    s.edit("Convert Artwork to Embroidery", |d, _| {
        for (id, e) in &plans {
            d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery = Some(Box::new(e.clone()));
        }
        Ok(())
    })?;
    Ok(ids_json(&done))
}

/// The machine formats StitchFoundry writes, by extension (what `embroidery.export` takes).
pub const MACHINE_EXTS: &[&str] = &["dst", "exp", "jef", "pes", "pec", "vp3", "xxx", "u01", "pcs", "tap", "dsb", "hus", "sew"];

/// The machine formats StitchFoundry reads (File › Open, the design library): the ones it
/// writes and ZSK DSZ.
pub const MACHINE_READ_EXTS: &[&str] = &["dst", "exp", "jef", "pes", "pec", "vp3", "xxx", "u01", "pcs", "tap", "dsb", "dsz", "hus", "sew", "vip"];

/// Open text exports of the stitches (export only): CSV and JSON stitch lists, G-code.
pub const TEXT_EXTS: &[&str] = &["csv", "json", "gcode"];

/// The stitches as one of [`TEXT_EXTS`], G-code as `p`'s `gcode` options say.
fn text_export(key: &str, pattern: &sf_core::Pattern, p: &Value) -> Result<Vec<u8>> {
    const CMD: &str = "embroidery.export";
    let center = bool_or(p, "center", true);
    Ok(match key {
        "csv" => sf_formats::textual::write_csv(pattern, center).into_bytes(),
        "json" => sf_formats::textual::write_json(pattern, center).into_bytes(),
        _ => {
            let g = &p["gcode"];
            let mut o = sf_formats::textual::GcodeOptions { center, ..Default::default() };
            if let Some(v) = g.get("feed").filter(|v| !v.is_null()) {
                o.feed = v.as_f64().filter(|f| (1.0..=100_000.0).contains(f)).ok_or_else(|| bad(CMD, "`gcode.feed` is 1 to 100000 mm/min"))?;
            }
            if let Some(v) = g.get("zStep").filter(|v| !v.is_null()) {
                o.z_step = v.as_f64().filter(|z| (0.0..=100.0).contains(z)).ok_or_else(|| bad(CMD, "`gcode.zStep` is 0 to 100"))?;
            }
            let text = |k: &str| g.get(k).and_then(Value::as_str).map(|t| t.chars().take(200).collect::<String>());
            if let Some(t) = text("stitchCommand") {
                o.stitch_command = t;
            }
            if let Some(t) = text("colorCommand") {
                o.color_command = t;
            }
            if let Some(t) = text("trimCommand") {
                o.trim_command = t;
            }
            o.flip_y = g.get("flipY").and_then(Value::as_bool).unwrap_or(true);
            sf_formats::textual::write_gcode(pattern, &o).into_bytes()
        }
    })
}

pub(super) fn export_machine_file(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.export";
    // the design's machine reads this format, unless another is asked for
    let machine = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.machine_file_format(&s.prefs.embroidery_machines));
    let mine = super::embroidery_my_machine::mine(s).map(|m| m.format.to_string());
    let key = str_param(p, "format").map(str::to_string).or(machine).or(mine).unwrap_or_else(|| "dst".to_string());
    let key = key.trim_start_matches('.').to_ascii_lowercase();
    let key = key.as_str();
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    if TEXT_EXTS.contains(&key) {
        if plan.is_empty() {
            return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type first"));
        }
        let mut pattern = plan.pattern().clone();
        if let Some(name) = str_param(p, "name") {
            pattern.name = name.to_string();
        }
        let bytes = text_export(key, &pattern, p)?;
        return Ok(json!({
            "dataBase64": vectorcraft_format::base64_encode(&bytes),
            "format": key,
            "bytes": bytes.len(),
            "stitches": pattern.stitch_count(),
            "colors": pattern.color_blocks().len(),
        }));
    }
    let format = sf_formats::Format::from_extension(key)
        .ok_or_else(|| bad(CMD, format!("unknown format `{key}`: one of {}, {}", MACHINE_EXTS.join(", "), TEXT_EXTS.join(", "))))?;
    if plan.is_empty() {
        return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type first"));
    }
    let title = &s.doc()?.doc.title;
    let name = str_param(p, "name").map_or_else(|| super::fileio::file_stem(title), str::to_string);
    // a chenille design keeps all its stops: each sets the colour, the stitch or the needle height
    let every_stop = !plan.built.chenille_stops.is_empty();
    let mut opts = sf_formats::WriteOptions { name, center: bool_or(p, "center", true), every_stop, ..sf_formats::WriteOptions::default() };
    // a fixed place in the hoop: the design's middle there, not on the hoop's centre
    let offset = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.settings.hoop_offset).filter(|_| opts.center);
    let moved;
    let pattern = match offset {
        Some([x, y]) => {
            let mut q = plan.pattern().clone();
            let b = q.bounds();
            let (dx, dy) = (x - (b.min.x + b.max.x) / 2.0, y - (b.min.y + b.max.y) / 2.0);
            for st in &mut q.stitches {
                st.x += dx;
                st.y += dy;
            }
            opts.center = false;
            moved = q;
            &moved
        }
        None => plan.pattern(),
    };
    let bytes = sf_formats::write(format, pattern, &opts).map_err(|e| EngineError::Other(format!("{} export failed: {e}", format.name())))?;
    // what the machine may refuse or show otherwise, sewn in the design's hoop where it is placed
    let cfg = s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default();
    let warnings = super::embroidery_machine_check::warnings(format, plan.pattern(), cfg.hoop.as_ref(), cfg.settings.hoop_offset);
    Ok(json!({
        "dataBase64": vectorcraft_format::base64_encode(&bytes),
        "format": format.extension(),
        "bytes": bytes.len(),
        "warnings": warnings,
        "stitches": pattern.stitch_count(),
        "colors": pattern.color_blocks().len(),
    }))
}

/// A machine embroidery file (DST, PES, JEF…) as a new document: each thread's stitches become a
/// path of manual stitches, one subpath per run between jumps and trims, on an artboard 10 mm
/// round the design. → (document, warnings)
pub(crate) fn machine_file_doc(title: &str, bytes: &[u8], ext: &str) -> Result<(Document, Vec<String>)> {
    let (_, pattern) = sf_formats::read_any(bytes, Some(ext)).map_err(|e| EngineError::Other(format!("can't open `{title}`: {e}")))?;
    // Runs of stitches per thread.
    let mut threads: Vec<Vec<Vec<sf_core::Point>>> = vec![Vec::new()];
    let (mut run, mut at, mut stops) = (Vec::<sf_core::Point>::new(), sf_core::Point::ZERO, 0usize);
    let flush = |run: &mut Vec<sf_core::Point>, threads: &mut Vec<Vec<Vec<sf_core::Point>>>| {
        let r = std::mem::take(run);
        if r.len() >= 2
            && let Some(t) = threads.last_mut()
        {
            t.push(r);
        }
    };
    for st in &pattern.stitches {
        let p = st.pos();
        match st.cmd {
            sf_core::Cmd::Stitch => {
                if run.is_empty() {
                    run.push(at);
                }
                if run.last().is_none_or(|q| q.distance(p) > 1e-9) {
                    run.push(p);
                }
                at = p;
            }
            sf_core::Cmd::Jump => {
                flush(&mut run, &mut threads);
                at = p;
            }
            sf_core::Cmd::Trim | sf_core::Cmd::End => flush(&mut run, &mut threads),
            sf_core::Cmd::Stop => {
                flush(&mut run, &mut threads);
                stops += 1;
            }
            sf_core::Cmd::ColorChange => {
                flush(&mut run, &mut threads);
                threads.push(Vec::new());
            }
        }
    }
    flush(&mut run, &mut threads);
    let b = pattern.bounds();
    if threads.iter().all(Vec::is_empty) || b.is_empty() {
        return Err(EngineError::Other(format!("can't open `{title}`: the file has no stitches")));
    }
    const MARGIN: f64 = 10.0;
    let to_pt = |q: sf_core::Point| kurbo::Point::new((q.x - b.min.x + MARGIN) * PT_PER_MM, (q.y - b.min.y + MARGIN) * PT_PER_MM);
    let mut doc = Document::new((b.width() + 2.0 * MARGIN) * PT_PER_MM, (b.height() + 2.0 * MARGIN) * PT_PER_MM);
    doc.title = title.to_string();
    let layer = doc.layers.first().map(|l| l.id);
    for (i, runs) in threads.iter().enumerate().filter(|(_, r)| !r.is_empty()) {
        use vectorcraft_geom::path::{Anchor, AnchorKind, PathData, SubPath};
        let subpaths = runs
            .iter()
            .map(|r| {
                let anchors = r.iter().map(|q| to_pt(*q)).map(|p| Anchor { p, h_in: p, h_out: p, kind: AnchorKind::Corner }).collect();
                SubPath::new(anchors, false)
            })
            .collect();
        let thread = pattern.thread(i);
        let c = thread.color;
        let id = doc.alloc_id();
        let kind =
            NodeKind::Path { path: PathData { subpaths }, rule: vectorcraft_geom::FillRule::NonZero, live: None, clipping: false, guide: false };
        let mut n = vectorcraft_doc::Node::new(id, kind);
        n.name = Some(thread.label()).filter(|l| !l.is_empty());
        n.appearance = Appearance::basic(Paint::None, Paint::solid(Color::rgb8(c.r, c.g, c.b)), 0.3 * PT_PER_MM).into();
        let mut e = Embroidery::manual();
        e.thread = Some(thread);
        n.embroidery = Some(Box::new(e));
        doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    // its design grade goes by the file it came from
    doc.embroidery.get_or_insert_with(Box::default).opened_from = ext.to_ascii_lowercase();
    let mut warnings = Vec::new();
    if stops > 0 {
        warnings.push(format!("the design's {stops} machine stop(s) weren't kept"));
    }
    Ok((doc, warnings))
}

/// Whether a traced shape is the image's background: its outline is (nearly) the whole image.
fn is_backdrop(n: &vectorcraft_doc::Node, image: kurbo::Rect) -> bool {
    let regions = regions_mm(n);
    let outer: f64 = regions.iter().map(|r| sf_core::geom::Polygon::from_ring(r.outer.clone()).area()).sum();
    let whole = image.width() * image.height() * MM_PER_PT * MM_PER_PT;
    whole > 0.0 && outer >= 0.97 * whole
}

/// Most machine folders remembered.
const MAX_SEND_DIRS: usize = 16;

/// Whether `dir` is a folder that is there now (a USB stick plugged in, a share reachable).
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn folder_ready(dir: &str) -> bool {
    std::path::Path::new(dir).is_dir()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn folder_ready(_: &str) -> bool {
    false
}

/// Whether a file is at `path` already.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn file_there(path: &std::path::Path) -> bool {
    path.exists()
}

#[cfg(target_arch = "wasm32")]
pub(super) fn file_there(_: &std::path::Path) -> bool {
    false
}

/// `dir` remembered as the machine folder last used.
fn remember_send_dir(s: &mut Session, dir: &str) {
    let dirs = &mut s.prefs.embroidery_send_dirs;
    dirs.retain(|d| d != dir);
    dirs.insert(0, dir.to_string());
    dirs.truncate(MAX_SEND_DIRS);
}

fn send_to_machine(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.send";
    let folder = match str_param(p, "folder").map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => f.to_string(),
        None => s
            .prefs
            .embroidery_send_dirs
            .first()
            .cloned()
            .ok_or_else(|| bad(CMD, "no machine folder yet: add one in the Embroidery panel, Send to Machine (or give `folder`)"))?,
    };
    if !folder_ready(&folder) {
        return Err(bad(CMD, format!("{folder} isn't there: plug in the USB stick, or check the network share")));
    }
    let stem = match str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => super::fileio::file_stem(&s.doc()?.doc.title),
    };
    let mut q = json!({ "center": true, "name": stem });
    if let Some(f) = str_param(p, "format") {
        q["format"] = json!(f);
    }
    let out = export_machine_file(s, &q)?;
    let bytes = out["dataBase64"]
        .as_str()
        .and_then(vectorcraft_format::base64_decode)
        .ok_or_else(|| EngineError::Other("the machine file didn't encode".into()))?;
    let ext = out["format"].as_str().unwrap_or("dst").to_string();
    let stem = super::embroidery_lettering::safe_name(&stem);
    let base = std::path::Path::new(&folder);
    let mut path = base.join(format!("{stem}.{ext}"));
    // a file of that name already: a number added, unless it is to be replaced
    if !bool_or(p, "replace", false) {
        let mut k = 2;
        while file_there(&path) && k < 1000 {
            path = base.join(format!("{stem}-{k}.{ext}"));
            k += 1;
        }
    }
    let path = path.to_string_lossy().to_string();
    super::fileio::write_file(&path, &bytes)?;
    remember_send_dir(s, &folder);
    Ok(json!({ "path": path, "format": ext, "bytes": bytes.len(), "stitches": out["stitches"], "warnings": out["warnings"] }))
}

fn send_targets(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(f) = str_param(p, "add").map(str::trim).filter(|f| !f.is_empty()) {
        remember_send_dir(s, f);
    }
    if let Some(f) = str_param(p, "forget") {
        s.prefs.embroidery_send_dirs.retain(|d| d != f);
    }
    let targets: Vec<Value> = s.prefs.embroidery_send_dirs.iter().map(|d| json!({ "folder": d, "ready": folder_ready(d) })).collect();
    Ok(json!({ "targets": targets }))
}

/// Where `n` marks go along `a`..`b`: one in the middle, else spread from a quarter in from each end.
fn spread(a: f64, b: f64, n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![(a + b) / 2.0];
    }
    let (lo, hi) = (a + (b - a) / 4.0, b - (b - a) / 4.0);
    (0..n).map(|k| lo + (hi - lo) * k as f64 / (n - 1) as f64).collect()
}

/// The registration marks of hooping `(col, row)` on a `cols` × `rows` grid of `cells`: `per` in
/// each overlap it shares with a neighbour (mm).
fn marks_of(cells: &[sf_core::BBox], cols: usize, rows: usize, col: usize, row: usize, per: usize) -> Vec<sf_core::Point> {
    let cell = |c: usize, r: usize| cells.get(r * cols + c).copied();
    let Some(me) = cell(col, row) else { return Vec::new() };
    let mut out = Vec::new();
    let mut shared = |o: sf_core::BBox, across: bool| {
        // the overlap strip between the two, a mark a quarter in from each end
        let (x0, y0, x1, y1) = (me.min.x.max(o.min.x), me.min.y.max(o.min.y), me.max.x.min(o.max.x), me.max.y.min(o.max.y));
        if across {
            let x = (x0 + x1) / 2.0;
            out.extend(spread(y0, y1, per).into_iter().map(|y| sf_core::Point::new(x, y)));
        } else {
            let y = (y0 + y1) / 2.0;
            out.extend(spread(x0, x1, per).into_iter().map(|x| sf_core::Point::new(x, y)));
        }
    };
    if col > 0
        && let Some(o) = cell(col - 1, row)
    {
        shared(o, true);
    }
    if col + 1 < cols
        && let Some(o) = cell(col + 1, row)
    {
        shared(o, true);
    }
    if row > 0
        && let Some(o) = cell(col, row - 1)
    {
        shared(o, false);
    }
    if row + 1 < rows
        && let Some(o) = cell(col, row + 1)
    {
        shared(o, false);
    }
    out
}

/// A multi-position hoop's fields: the frame centred on the design, a field at each position.
fn position_cells(positions: &[[f64; 2]], c: sf_core::Point, field: (f64, f64)) -> Vec<sf_core::BBox> {
    positions
        .iter()
        .map(|[x, y]| {
            let m = sf_core::Point::new(c.x + x, c.y + y);
            sf_core::BBox::new(
                sf_core::Point::new(m.x - field.0 / 2.0, m.y - field.1 / 2.0),
                sf_core::Point::new(m.x + field.0 / 2.0, m.y + field.1 / 2.0),
            )
        })
        .collect()
}

/// The grid of hoopings from `origin`, row by row.
fn grid_cells(origin: sf_core::Point, (cols, rows): (usize, usize), (step_x, step_y): (f64, f64), field: (f64, f64)) -> Vec<sf_core::BBox> {
    (0..rows)
        .flat_map(|r| (0..cols).map(move |k| (k, r)))
        .map(|(k, r)| {
            let min = sf_core::Point::new(origin.x + k as f64 * step_x, origin.y + r as f64 * step_y);
            sf_core::BBox::new(min, sf_core::Point::new(min.x + field.0, min.y + field.1))
        })
        .collect()
}

/// An object split over the hoopings: each stitch in the hooping whose middle is nearest its
/// middle, the runs kept whole, as (hooping, run).
fn split_over_hoopings(stitches: &[sf_core::Stitch], cells: &[sf_core::BBox]) -> Result<Vec<(usize, Vec<sf_core::Point>)>> {
    const CMD: &str = "embroidery.multiHoop";
    let middles: Vec<sf_core::Point> = cells.iter().map(|c| sf_core::Point::new((c.min.x + c.max.x) / 2.0, (c.min.y + c.max.y) / 2.0)).collect();
    let nearest = |q: sf_core::Point| middles.iter().enumerate().min_by(|a, b| a.1.distance(q).total_cmp(&b.1.distance(q))).map_or(0, |(i, _)| i);
    let inside =
        |c: &sf_core::BBox, q: sf_core::Point| q.x >= c.min.x - 1e-6 && q.y >= c.min.y - 1e-6 && q.x <= c.max.x + 1e-6 && q.y <= c.max.y + 1e-6;
    let mut out = Vec::new();
    let mut cur: Option<(usize, Vec<sf_core::Point>)> = None;
    let mut prev: Option<sf_core::Point> = None;
    let mut flush = |cur: &mut Option<(usize, Vec<sf_core::Point>)>| {
        if let Some((c, run)) = cur.take()
            && run.len() >= 2
            && c < cells.len()
        {
            out.push((c, run));
        }
    };
    for st in stitches {
        let q = st.pos();
        if st.cmd != sf_core::Cmd::Stitch {
            flush(&mut cur);
            prev = None;
            continue;
        }
        if let Some(a) = prev {
            let c = nearest(sf_core::Point::new((a.x + q.x) / 2.0, (a.y + q.y) / 2.0));
            if !cells.get(c).is_some_and(|cell| inside(cell, a) && inside(cell, q)) {
                return Err(bad(CMD, "a stitch is longer than the hoopings' overlap: widen the overlap"));
            }
            match &mut cur {
                Some((cc, run)) if *cc == c && run.last().is_some_and(|l| l.distance(a) < 1e-9) => run.push(q),
                _ => {
                    flush(&mut cur);
                    cur = Some((c, vec![a, q]));
                }
            }
        }
        prev = Some(q);
    }
    flush(&mut cur);
    Ok(out)
}

/// Sews a piece of a split object into a hooping centred on `centre`, tied in and off.
fn sew_piece(hp: &mut sf_core::Pattern, run: &[sf_core::Point], centre: sf_core::Point) {
    let local = |q: sf_core::Point| sf_core::Point::new(q.x - centre.x, q.y - centre.y);
    let (Some(a), Some(b)) = (run.first().copied().map(local), run.get(1).copied().map(local)) else { return };
    let toward = |from: sf_core::Point, to: sf_core::Point| {
        let d = from.distance(to).max(1e-9);
        let t = (0.4 / d).min(0.5);
        sf_core::Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
    };
    hp.jump(a);
    hp.stitch(a);
    hp.stitch(toward(a, b));
    hp.stitch(a);
    for q in run.iter().skip(1) {
        hp.stitch(local(*q));
    }
    if let (Some(z), Some(y)) = (run.last().copied().map(local), run.len().checked_sub(2).and_then(|i| run.get(i)).copied().map(local)) {
        hp.stitch(toward(z, y));
        hp.stitch(z);
    }
    hp.trim();
}

/// Sews a whole object into a hooping centred on `centre`, without the travel in to it.
fn sew_object(hp: &mut sf_core::Pattern, stitches: &[sf_core::Stitch], centre: sf_core::Point) {
    let mut first = true;
    for st in stitches {
        let q = sf_core::Point::new(st.pos().x - centre.x, st.pos().y - centre.y);
        match st.cmd {
            sf_core::Cmd::Stitch if first => {
                hp.jump(q);
                hp.stitch(q);
                first = false;
            }
            sf_core::Cmd::Stitch => hp.stitch(q),
            // the travel in from the object before, perhaps in another hooping
            sf_core::Cmd::Jump | sf_core::Cmd::Trim if first => {}
            sf_core::Cmd::Jump => hp.jump(q),
            sf_core::Cmd::Trim => hp.trim(),
            sf_core::Cmd::Stop => hp.stop(),
            sf_core::Cmd::ColorChange | sf_core::Cmd::End => {}
        }
    }
    hp.trim();
}

fn multi_hoop(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.multiHoop";
    let cfg = s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default();
    // the hoop's field (mm), and a multi-position hoop's positions
    let (field, positions) = match (p.get("width").and_then(Value::as_f64), p.get("height").and_then(Value::as_f64)) {
        (Some(w), Some(h)) if (20.0..=2000.0).contains(&w) && (20.0..=2000.0).contains(&h) => ((w, h), Vec::new()),
        (Some(_), _) | (_, Some(_)) => return Err(bad(CMD, "`width` and `height` are 20 to 2000 mm, both given")),
        _ => match str_param(p, "hoop") {
            Some(name) => super::embroidery_hoops::find_hoop(s, name)
                .map(|h| ((h.width, h.height), h.positions.clone()))
                .ok_or_else(|| bad(CMD, format!("no hoop `{name}`")))?,
            None => cfg
                .hoop
                .as_ref()
                .map(|h| ((h.width, h.height), h.positions.clone()))
                .ok_or_else(|| bad(CMD, "choose a hoop: the design has none (or give `hoop`, or `width` and `height`)"))?,
        },
    };
    let multi_position = !positions.is_empty();
    let overlap = super::embroidery_lettering::ranged(p, "overlap", 0.0..=100.0, CMD, "`overlap` is 0 to 100 mm")?.unwrap_or(10.0);
    if overlap * 2.0 >= field.0.min(field.1) && !multi_position {
        return Err(bad(CMD, "the overlap is too wide for the hoop"));
    }
    // a multi-position hoop isn't hooped again: nothing to line up
    let marks = bool_or(p, "marks", true) && !multi_position;
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    if plan.built.pattern.stitch_count() == 0 {
        return Err(bad(CMD, "nothing is stitched yet: give objects a stitch type first"));
    }
    // the grid of hoopings over the design, centred on it
    let b = plan.built.pattern.bounds();
    let (step_x, step_y) = (field.0 - overlap, field.1 - overlap);
    let count = |len: f64, size: f64, step: f64| if len <= size { 1 } else { ((len - overlap) / step).ceil().max(1.0) as usize };
    let (cols, rows) = if multi_position { (1, positions.len()) } else { (count(b.width(), field.0, step_x), count(b.height(), field.1, step_y)) };
    if cols * rows > 64 {
        return Err(bad(CMD, format!("{cols} × {rows} hoopings: more than 64; choose a larger hoop")));
    }
    let span = (cols as f64 * step_x + overlap, rows as f64 * step_y + overlap);
    let c = sf_core::Point::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
    let origin = sf_core::Point::new(c.x - span.0 / 2.0, c.y - span.1 / 2.0);
    let cells: Vec<sf_core::BBox> =
        if multi_position { position_cells(&positions, c, field) } else { grid_cells(origin, (cols, rows), (step_x, step_y), field) };
    // each object whole in the first hooping that holds it (its stitches: the travel in may be far off)
    let pattern = &plan.built.pattern;
    let mut assigned: Vec<Vec<usize>> = vec![Vec::new(); cells.len()];
    // the pieces of objects split over hoopings: (object, its stitches in that hooping)
    let mut parts: Vec<Vec<(usize, Vec<sf_core::Point>)>> = vec![Vec::new(); cells.len()];
    let split = bool_or(p, "split", true);
    for (k, span) in plan.built.spans.iter().enumerate() {
        let pts: Vec<sf_core::Point> =
            pattern.stitches.get(span.range.clone()).unwrap_or_default().iter().filter(|s| s.cmd == sf_core::Cmd::Stitch).map(|s| s.pos()).collect();
        let Some(ob) = pts.iter().copied().map(|q| sf_core::BBox::new(q, q)).reduce(sf_core::BBox::union) else { continue };
        let fits = |cell: &sf_core::BBox| {
            ob.min.x >= cell.min.x - 1e-6 && ob.min.y >= cell.min.y - 1e-6 && ob.max.x <= cell.max.x + 1e-6 && ob.max.y <= cell.max.y + 1e-6
        };
        let Some(i) = cells.iter().position(fits) else {
            if split {
                for (c, run) in split_over_hoopings(pattern.stitches.get(span.range.clone()).unwrap_or_default(), &cells)? {
                    if let Some(slot) = parts.get_mut(c) {
                        slot.push((k, run));
                    }
                }
                continue;
            }
            return Err(bad(
                CMD,
                format!(
                    "an object {:.0} × {:.0} mm doesn't fit in any hooping: choose a larger hoop or a wider overlap (objects aren't split yet)",
                    ob.width(),
                    ob.height()
                ),
            ));
        };
        if let Some(a) = assigned.get_mut(i) {
            a.push(k);
        }
    }
    // each hooping's pattern, centred on its hoop
    let dir = str_param(p, "dir").filter(|d| !d.trim().is_empty());
    let (format, stem) = {
        let machine = cfg.machine_file_format(&s.prefs.embroidery_machines);
        let key = str_param(p, "format").map(str::to_string).or(machine).unwrap_or_else(|| "dst".to_string());
        let f = sf_formats::Format::from_extension(&key)
            .ok_or_else(|| bad(CMD, format!("unknown format `{key}`: one of {}", MACHINE_EXTS.join(", "))))?;
        let stem = format!(
            "{}{}",
            str_param(p, "prefix").unwrap_or(""),
            super::embroidery_lettering::safe_name(&super::fileio::file_stem(&s.doc()?.doc.title))
        );
        (f, stem)
    };
    if let Some(d) = dir {
        super::fileio::create_dir(d)?;
    }
    let reg = cfg.settings.registration;
    let mark_thread = Thread::new(reg.color, "Registration");
    let mut out = Vec::new();
    let mut files = Vec::new();
    let mut index = 0;
    let sheets = bool_or(p, "sheets", false);
    let mut printed: Vec<(usize, usize, usize, sf_core::Pattern, String)> = Vec::new();
    for (i, (cell, objs)) in cells.iter().zip(&assigned).enumerate() {
        let pieces = parts.get(i).map(Vec::as_slice).unwrap_or_default();
        if objs.is_empty() && pieces.is_empty() {
            continue;
        }
        // whole objects and pieces of split ones, in sewing order
        let mut order: Vec<usize> = objs.clone();
        order.extend(pieces.iter().map(|(k, _)| *k));
        order.sort_unstable();
        order.dedup();
        index += 1;
        let (col, row) = (i % cols, i / cols);
        let centre = sf_core::Point::new((cell.min.x + cell.max.x) / 2.0, (cell.min.y + cell.max.y) / 2.0);
        let local = |q: sf_core::Point| sf_core::Point::new(q.x - centre.x, q.y - centre.y);
        let mut hp = sf_core::Pattern::new();
        hp.threads.clear();
        // registration crosses first, in a colour of their own
        let crosses = if marks { marks_of(&cells, cols, rows, col, row, usize::from(reg.per_overlap.clamp(1, 4))) } else { Vec::new() };
        for m in &crosses {
            for run in reg.runs(local(*m)) {
                let Some(first) = run.first() else { continue };
                hp.jump(*first);
                for q in &run {
                    hp.stitch(*q);
                }
            }
            hp.trim();
        }
        if !crosses.is_empty() {
            hp.threads.push(mark_thread.clone());
        }
        let mut colour: Option<usize> = None;
        for k in &order {
            let Some(span) = plan.built.spans.get(*k) else { continue };
            let thread_ix = plan.design.objects.get(span.index).map_or(0, |o| o.color);
            if colour != Some(thread_ix) {
                if !hp.threads.is_empty() {
                    hp.trim();
                    hp.color_change();
                }
                hp.threads.push(plan.design.thread(thread_ix));
                colour = Some(thread_ix);
            }
            if !objs.contains(k) {
                // a split object's pieces in this hooping, each tied in and off
                for (_, run) in pieces.iter().filter(|(o, _)| o == k) {
                    sew_piece(&mut hp, run, centre);
                }
                continue;
            }
            sew_object(&mut hp, pattern.stitches.get(span.range.clone()).unwrap_or_default(), centre);
        }
        hp.ensure_end();
        let stitches = hp.stitch_count();
        let mut entry = json!({ "index": index, "column": col + 1, "row": row + 1, "objects": order.len(), "split": order.len() - objs.len(), "stitches": stitches });
        if let Some(d) = dir {
            let opts = sf_formats::WriteOptions { name: format!("{stem} {index}"), center: false, ..sf_formats::WriteOptions::default() };
            let bytes = sf_formats::write(format, &hp, &opts).map_err(|e| EngineError::Other(format!("{} export failed: {e}", format.name())))?;
            let path = std::path::Path::new(d).join(format!("{stem}-{index}.{}", format.extension())).to_string_lossy().to_string();
            super::fileio::write_file(&path, &bytes)?;
            entry["file"] = json!(path);
            files.push(path);
        }
        if sheets {
            let file = entry["file"].as_str().map(super::fileio::file_name).unwrap_or_default();
            printed.push((index, col, row, hp, file));
        }
        out.push(entry);
    }
    let mut r = json!({ "hoopings": out, "columns": cols, "rows": rows, "files": files });
    if sheets {
        let title = super::fileio::file_stem(&s.doc()?.doc.title);
        let doc = hooping_sheets(&title, &printed, cols, rows, field)?;
        s.add_document(doc, None);
        r["sheets"] = json!(printed.len());
    }
    Ok(r)
}

/// A page per hooping to print beside the machine: the hooping's stitches in their colours inside
/// the hoop's field (life size when it fits the page), where it is in the grid, its file and how to
/// line it up.
fn hooping_sheets(
    title: &str,
    hoopings: &[(usize, usize, usize, sf_core::Pattern, String)],
    cols: usize,
    rows: usize,
    field: (f64, f64),
) -> Result<Document> {
    let (pw, ph) = SHEET;
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let grey = Color::rgb8(0x66, 0x66, 0x66);
    let accent = Color::rgb8(0x1d, 0x35, 0x57);
    let mut doc = Document::new(pw, ph);
    doc.title = format!("{title} hoopings");
    let total = hoopings.len();
    for (page, (index, col, row, pattern, file)) in hoopings.iter().enumerate() {
        let ox = page as f64 * (pw + 40.0);
        if page > 0 {
            let id = doc.next_artboard_id();
            doc.artboards.push(vectorcraft_doc::Artboard {
                id,
                name: format!("Hooping {index}"),
                rect: vectorcraft_geom::Rect::new(ox, 0.0, ox + pw, ph),
                show_center_mark: false,
                show_cross_hairs: false,
                locked: false,
                show_video_safe_areas: false,
                video_ruler_pixel_aspect_ratio: 1.0,
                color: None,
            });
        }
        sheet_text(&mut doc, ox + 40.0, 56.0, &format!("Hooping {index} of {total}"), 20.0, black)?;
        let mut sub = format!("{title}   column {}, row {}   hoop field {:.0} × {:.0} mm", col + 1, row + 1, field.0, field.1);
        if !file.is_empty() {
            sub = format!("{sub}   file {file}");
        }
        sheet_text(&mut doc, ox + 40.0, 76.0, &sub, 9.0, grey)?;
        // the grid, this hooping filled
        let cell = 14.0;
        let (gx, gy) = (ox + pw - 40.0 - cols as f64 * cell, 40.0);
        let mut boxes = Vec::new();
        for r in 0..rows {
            for c in 0..cols {
                let (x, y) = (gx + c as f64 * cell, gy + r as f64 * cell);
                boxes.push(vec![(x, y), (x + cell, y), (x + cell, y + cell), (x, y + cell), (x, y)]);
            }
        }
        sheet_lines(&mut doc, &boxes, grey, 0.6)?;
        let (x, y) = (gx + *col as f64 * cell, gy + *row as f64 * cell);
        let fill: Vec<Vec<(f64, f64)>> = (1..7).map(|k| vec![(x + 1.0, y + 2.0 * f64::from(k)), (x + cell - 1.0, y + 2.0 * f64::from(k))]).collect();
        sheet_lines(&mut doc, &fill, accent, 1.6)?;
        // the hoop's field and the stitches in it, life size when it fits
        let (bx, by, bw, bh) = (ox + 40.0, 100.0, pw - 80.0, ph - 100.0 - 110.0);
        let k = (bw / (field.0 * PT_PER_MM)).min(bh / (field.1 * PT_PER_MM)).min(1.0) * PT_PER_MM;
        let (cx, cy) = (bx + bw / 2.0, by + bh / 2.0);
        let (hw, hh) = (field.0 * k / 2.0, field.1 * k / 2.0);
        sheet_lines(
            &mut doc,
            &[vec![(cx - hw, cy - hh), (cx + hw, cy - hh), (cx + hw, cy + hh), (cx - hw, cy + hh), (cx - hw, cy - hh)]],
            grey,
            0.8,
        )?;
        sheet_lines(&mut doc, &[vec![(cx - 6.0, cy), (cx + 6.0, cy)], vec![(cx, cy - 6.0), (cx, cy + 6.0)]], grey, 0.4)?;
        for (i, runs) in stitch_runs(pattern).iter().enumerate() {
            let lines: Vec<Vec<(f64, f64)>> = runs.iter().map(|r| r.iter().map(|q| (cx + q.x * k, cy + q.y * k)).collect()).collect();
            if lines.is_empty() {
                continue;
            }
            let t = pattern.thread(i).color;
            sheet_lines(&mut doc, &lines, Color::rgb8(t.r, t.g, t.b), 0.35 * k.max(0.5))?;
        }
        let scale = if k >= PT_PER_MM - 1e-9 { "Life size".to_string() } else { format!("Scale {:.0}%", k / PT_PER_MM * 100.0) };
        sheet_text(&mut doc, ox + 40.0, ph - 96.0, &scale, 9.0, grey)?;
        let how = if total > 1 {
            "Sew the registration marks first (their own colour). For the next hooping, hoop the fabric again so its marks land on the ones sewn here, then sew on."
        } else {
            "One hooping: sew it as it is."
        };
        for (n, line) in wrap_text(how, 95).iter().enumerate() {
            sheet_text(&mut doc, ox + 40.0, ph - 78.0 + n as f64 * 13.0, line, 9.0, black)?;
        }
    }
    Ok(doc)
}

/// The ways click-to-stitch can stitch a region.
const CLICK_AS: [&str; 5] = ["fill", "fillNoHoles", "satin", "outline", "centerline"];

/// Every pixel of `img` whose colour is within `tolerance` of the one at `(x, y)` (on every
/// channel, alpha too), joined to it or not: a colour range; none on a (half) transparent pixel.
fn pick_everywhere(img: &vectorcraft_trace::Raster, x: u32, y: u32, tolerance: u8) -> Option<vectorcraft_trace::region::Region> {
    let (w, h) = (img.width as usize, img.height as usize);
    if x as usize >= w || y as usize >= h || img.rgba.len() != w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    let at = (y as usize * w + x as usize) * 4;
    let c0: [u8; 4] = img.rgba.get(at..at + 4)?.try_into().ok()?;
    if c0[3] < 128 {
        return None;
    }
    let mut sum = [0u64; 4];
    let mut pixels = 0usize;
    let mask: Vec<bool> = img
        .rgba
        .chunks_exact(4)
        .map(|c| {
            let near = c[3] >= 128 && c.iter().zip(c0).all(|(a, b)| a.abs_diff(b) <= tolerance);
            if near {
                pixels += 1;
                for (s, v) in sum.iter_mut().zip(c) {
                    *s += u64::from(*v);
                }
            }
            near
        })
        .collect();
    let n = pixels.max(1) as u64;
    Some(vectorcraft_trace::region::Region { mask, pixels, color: sum.map(|s| u8::try_from(s / n).unwrap_or(u8::MAX)) })
}

fn click_stitch(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.clickStitch";
    let at = point_param(p, "at").filter(|q| q.x.is_finite() && q.y.is_finite()).ok_or_else(|| bad(CMD, "`at` is [x, y], a spot on an image"))?;
    let kind = str_param(p, "as").unwrap_or("fill");
    let kind = CLICK_AS
        .into_iter()
        .find(|k| k.eq_ignore_ascii_case(kind))
        .ok_or_else(|| bad(CMD, "`as` is fill, fillNoHoles, satin, outline or centerline"))?;
    let whole = |key: &str, max: u64, default: u64| -> Result<u64> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_u64().filter(|v| *v <= max).ok_or_else(|| bad(CMD, format!("`{key}` is 0 to {max}"))),
        }
    };
    let tolerance = u8::try_from(whole("tolerance", 255, 32)?).unwrap_or(32);
    let min_hole = usize::try_from(whole("minHole", 1_000_000, 16)?).unwrap_or(16);
    let contiguous = bool_or(p, "contiguous", true);
    // the image: the one given, else the topmost under the spot
    let (im, bytes) = {
        let d = &s.doc()?.doc;
        let node = match p.get("id").and_then(Value::as_u64) {
            Some(id) => d.node(NodeId(id)).cloned(),
            None => {
                let mut top = None;
                d.walk(|n| {
                    if matches!(n.kind, NodeKind::Image(_)) && n.visible && n.geometric_bounds().is_some_and(|b| b.contains(at)) {
                        top = Some(n.clone());
                    }
                });
                top
            }
        };
        let Some(NodeKind::Image(im)) = node.map(|n| n.kind) else { return Err(bad(CMD, "click on an image (or give its `id`)")) };
        let blob = d.images.get(&im.key).ok_or_else(|| EngineError::Other(format!("image data `{}` is missing", im.key)))?;
        (im, blob.bytes.clone())
    };
    let raster = vectorcraft_trace::Raster::decode(&bytes).map_err(|e| bad(CMD, e.to_string()))?;
    // the raster may be bigger or smaller than the image's own size: pixels mapped over it
    let to_doc =
        im.xf * kurbo::Affine::scale_non_uniform(f64::from(im.width) / f64::from(raster.width), f64::from(im.height) / f64::from(raster.height));
    if to_doc.determinant().abs() < 1e-12 {
        return Err(bad(CMD, "the image has no size"));
    }
    let q = to_doc.inverse() * at;
    if !(q.x.is_finite() && q.y.is_finite() && q.x >= 0.0 && q.y >= 0.0 && q.x < f64::from(raster.width) && q.y < f64::from(raster.height)) {
        return Err(bad(CMD, "the spot isn't on the image"));
    }
    let picked = if contiguous {
        vectorcraft_trace::region::pick(&raster, q.x as u32, q.y as u32, tolerance)
    } else {
        pick_everywhere(&raster, q.x as u32, q.y as u32, tolerance)
    };
    let region = picked.ok_or_else(|| bad(CMD, "nothing to stitch there: the spot is transparent"))?;
    let (w, h) = (raster.width as usize, raster.height as usize);
    let path = match kind {
        "centerline" => {
            let subs: Vec<_> = vectorcraft_trace::region::centre_lines(&region.mask, w, h).into_iter().flat_map(|(p, _)| p.subpaths).collect();
            (!subs.is_empty()).then(|| vectorcraft_geom::path::PathData::new(subs))
        }
        _ => vectorcraft_trace::region::outline(&region.mask, w, h, kind != "fillNoHoles", min_hole),
    }
    .ok_or_else(|| bad(CMD, "the region is too small to stitch"))?
    .transformed(to_doc);
    let [r, g, b, _] = region.color;
    let paint = Paint::solid(Color::rgb8(r, g, b));
    let line = matches!(kind, "outline" | "centerline");
    let look = if line { Appearance::basic(Paint::None, paint, 1.0) } else { Appearance::basic(paint, Paint::None, 0.0) };
    let stitch = match kind {
        "satin" => StitchType::Satin(SatinParams::default()),
        "outline" | "centerline" => StitchType::Run(RunParams::default()),
        _ => StitchType::Fill(FillParams::default()),
    };
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Click to Stitch", |d, sel| {
        let mut n = vectorcraft_doc::Node::path(d.alloc_id(), path, look);
        n.embroidery = Some(Box::new(Embroidery::new(stitch)));
        let id = n.id;
        d.insert(parent, usize::MAX, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0, "as": kind, "pixels": region.pixels, "color": format!("#{r:02x}{g:02x}{b:02x}") }))
}

fn auto_digitize(s: &mut Session, p: &Value) -> Result<Value> {
    // The trace and the stitch types are one undo step.
    // the active document's own group: an undo group open in another document stays open
    let group = s.active().filter(|d| d.undo_group.is_none()).map(|d| d.uid);
    if group.is_some() {
        s.begin_undo_group();
    }
    let res = digitize_image(s, p);
    if let Some(uid) = group {
        s.end_document_undo_group(uid, res.is_err());
    }
    res
}

fn digitize_image(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.autoDigitize";
    let colors = p.get("colors").and_then(Value::as_u64).unwrap_or(6).clamp(2, 32);
    let detail = f64_or(p, "detail", 60.0).clamp(0.0, 100.0);
    let noise = p.get("noise").and_then(Value::as_u64).unwrap_or(16).clamp(1, 10_000);
    let remove_bg = bool_or(p, "removeBackground", true);
    let min_area = f64_or(p, "minArea", 1.0).max(0.0);
    let keep_image = bool_or(p, "keepImage", true);
    let details = f64_or(p, "details", DETAILS).clamp(0.0, 5.0);
    let outline = str_param(p, "outline").unwrap_or("none");
    if !["none", "design", "shapes"].contains(&outline) {
        return Err(bad(CMD, "`outline` is none, design or shapes"));
    }
    let outline_width = f64_or(p, "outlineWidth", 1.5).clamp(0.5, 10.0);
    let outline_color = match str_param(p, "outlineColor") {
        Some(c) => Some(Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`outlineColor` is \"#rrggbb\""))?),
        None => None,
    };
    let border = f64_or(p, "border", 0.0);
    if !(border == 0.0 || (1.0..=30.0).contains(&border)) {
        return Err(bad(CMD, "`border` is 0 (none) or 1 to 30 mm"));
    }
    let border_color = str_param(p, "borderColor").unwrap_or("#ffffff").to_string();
    let border_edge = str_param(p, "borderEdge").unwrap_or("satin").to_string();
    let chart = match str_param(p, "chart") {
        Some(name) => Some(super::embroidery_charts::find_chart(s, name).ok_or_else(|| bad(CMD, format!("unknown thread chart `{name}`")))?.name),
        None => None,
    };
    let size = match p.get("size").filter(|v| !v.is_null()) {
        Some(v) => Some(v.as_f64().filter(|v| v.is_finite() && *v >= 5.0).ok_or_else(|| bad(CMD, "size must be at least 5 mm"))?),
        None => None,
    };
    // The image to trace (kept under the stitches as the digitizing backdrop).
    let source = {
        let st = s.doc()?;
        st.selection.objects.iter().find_map(|id| st.doc.node(*id).filter(|n| matches!(n.kind, NodeKind::Image(_))).cloned())
    };
    let Some(source) = source else { return Err(bad(CMD, "select an image to auto-digitize")) };
    let trace = json!({
        "preset": "6 Colors",
        "params": {
            "mode": "color", "palette": "limited", "colors": colors, "paths": detail, "corners": 60.0, "noise": noise,
            "method": "abutting", "ignoreWhite": remove_bg, "fills": true, "strokes": false
        }
    });
    let traced = s.execute("imageTrace.makeAndExpand", &trace)?;
    let gid = traced
        .get("id")
        .and_then(Value::as_u64)
        .map(NodeId)
        .ok_or_else(|| EngineError::Other("Auto-Digitize: the image traced to nothing".into()))?;
    let (kids, bounds, hoop) = {
        let st = s.doc()?;
        let g = st.doc.node(gid).ok_or(EngineError::NoNode(gid))?;
        let kids: Vec<Arc<vectorcraft_doc::Node>> = g.children().cloned().unwrap_or_default();
        let hoop = st.doc.embroidery.as_deref().and_then(|e| e.hoop.clone());
        (kids, g.geometric_bounds(), hoop)
    };
    let Some(bounds) = bounds else { return Err(EngineError::Other("Auto-Digitize: the image traced to nothing".into())) };
    // Size: as asked, else shrunk into the hoop (or to 100 mm when larger than 200 mm without one).
    let longest = bounds.width().max(bounds.height()) * MM_PER_PT;
    let scale = match (size, &hoop) {
        (Some(mm), _) if longest > 0.0 => mm / longest,
        (None, Some(h)) => {
            let k = (h.width * 0.9 / (bounds.width() * MM_PER_PT).max(1e-9)).min(h.height * 0.9 / (bounds.height() * MM_PER_PT).max(1e-9));
            k.min(1.0)
        }
        (None, None) if longest > 200.0 => 100.0 / longest,
        _ => 1.0,
    };
    let xf = kurbo::Affine::scale_about(scale, bounds.origin());
    let image_rect = xf.transform_rect_bbox(bounds);
    let (mut keep, mut removed, mut satin, mut fill, mut run) = (Vec::new(), 0usize, 0usize, 0usize, 0usize);
    for (k, c) in kids.iter().enumerate() {
        let mut n = (**c).clone();
        if (scale - 1.0).abs() > 1e-9 {
            n.transform(xf, true);
        }
        let regions = regions_mm(&n);
        let area: f64 = regions.iter().map(sf_core::geom::Polygon::area).sum();
        // fine lines and details, too thin for satin: a run round them; a line is kept however
        // small its area (a stripe on a face, a whisker)
        let thin = details > 0.0 && regions.iter().all(|r| r.thickness() <= details);
        // a line: long for its width (a dark one sooner, as it is usually a drawn line); a thin
        // sliver that isn't one is left to the shapes round it, which close up over it
        let dark = match n.appearance.fill_paint() {
            Paint::Solid { color, .. } => rgb_of(&color).to_lab()[0] < DARK_LINE,
            _ => false,
        };
        let length = regions.iter().map(sf_core::geom::Polygon::perimeter).sum::<f64>() / 2.0;
        let line = thin && length >= if dark { MIN_LINE } else { MIN_COLOUR_LINE };
        if (area < min_area && !line) || (thin && !line) || (remove_bg && k == 0 && is_backdrop(&n, image_rect)) {
            removed += 1;
            continue;
        }
        // a dark line is sewn down its middle, three times over so it reads (a bean stitch);
        // other thin shapes are run round
        let middles: Vec<Vec<sf_core::Point>> = if thin && dark { regions.iter().filter_map(centreline).collect() } else { Vec::new() };
        let e = if !middles.is_empty() {
            let color = match n.appearance.fill_paint() {
                Paint::Solid { color, .. } => color,
                _ => Color::rgb8(0, 0, 0),
            };
            let pt = |l: &Vec<sf_core::Point>| {
                let pts: Vec<kurbo::Point> = l.iter().map(|p| kurbo::Point::new(p.x * PT_PER_MM, p.y * PT_PER_MM)).collect();
                vectorcraft_geom::path::SubPath::polyline(&pts, false)
            };
            n.kind = NodeKind::Path {
                path: vectorcraft_geom::PathData::new(middles.iter().map(pt).collect()),
                rule: vectorcraft_geom::FillRule::NonZero,
                live: None,
                clipping: false,
                guide: false,
            };
            n.appearance = Appearance::basic(Paint::None, Paint::solid(color), 0.35 * PT_PER_MM).into();
            Embroidery::new(StitchType::Run(RunParams { kind: RunKind::Triple, stitch_length: DETAIL_STITCH, ..RunParams::default() }))
        } else if thin {
            Embroidery::run()
        } else {
            vectorcraft_embroidery::suggest(&n)
        };
        if thin {
            run += 1;
        } else if matches!(e.stitch, StitchType::Satin(_)) {
            satin += 1;
        } else {
            fill += 1;
        }
        n.embroidery = Some(Box::new(e));
        keep.push(n);
    }
    if keep.is_empty() {
        return Err(EngineError::Other("Auto-Digitize found no shapes to stitch: lower Noise or raise Colors".into()));
    }
    // the details' runs are sewn last, on top: the shapes under them don't leave room for them
    let is_run = |n: &vectorcraft_doc::Node| n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, StitchType::Run(_)));
    let (mut lines, mut areas): (Vec<_>, Vec<_>) = keep.into_iter().partition(is_run);
    // the specks dropped (and the light ones the trace left out) leave no holes for the fabric to
    // show through: the shape round them covers them
    let holes_closed = close_small_holes(&mut areas, (3.0 * min_area).max(3.0) / (MM_PER_PT * MM_PER_PT));
    // and the gaps between neighbouring colours (a seam the trace left, a speck dropped where
    // colours meet, a detail sewn on top) are sewn by the neighbour sharing most of their edge
    let gaps_filled = fill_gaps(&mut areas, (3.0 * min_area).max(3.0), MAX_SEAM_WIDTH.max(details));
    let mut keep = areas;
    // Abutting shapes don't overlap, so they can be sewn colour by colour (in the order the colours
    // first appear), each colour nearest-first.
    // a shape's colour is its fill; a detail's run, which has none, its stroke
    let colour = |n: &vectorcraft_doc::Node| match (n.appearance.fill_paint(), n.appearance.stroke_paint()) {
        (Paint::Solid { color, .. }, _) | (_, Paint::Solid { color, .. }) => rgb_of(&color).to_u32(),
        _ => 0,
    };
    let centre = |n: &vectorcraft_doc::Node| n.geometric_bounds().map_or(kurbo::Point::ZERO, |b| b.center());
    let mut order: Vec<u32> = Vec::new();
    for n in keep.iter().chain(&lines) {
        let c = colour(n);
        if !order.contains(&c) {
            order.push(c);
        }
    }
    let mut sorted = Vec::with_capacity(keep.len() + lines.len());
    let mut here = image_rect.origin();
    // the shapes, then the details over them, each colour by colour
    for group in [&mut keep, &mut lines] {
        for c in &order {
            let mut todo: Vec<vectorcraft_doc::Node> = Vec::new();
            let mut rest = Vec::new();
            for n in group.drain(..) {
                if colour(&n) == *c { todo.push(n) } else { rest.push(n) }
            }
            *group = rest;
            while !todo.is_empty() {
                let i =
                    todo.iter().enumerate().min_by(|a, b| centre(a.1).distance(here).total_cmp(&centre(b.1).distance(here))).map_or(0, |(i, _)| i);
                let n = todo.swap_remove(i);
                here = centre(&n);
                sorted.push(n);
            }
        }
    }
    let objects = sorted.len();
    s.edit("Auto-Digitize", |d, sel| {
        let g = d.node_mut(gid).ok_or(EngineError::NoNode(gid))?;
        g.name = Some("Auto-Digitized".into());
        if let Some(ch) = g.children_mut() {
            *ch = sorted.into_iter().map(Arc::new).collect();
        }
        if keep_image {
            let mut img = source.clone();
            img.id = d.alloc_id();
            img.name = Some("Auto-Digitize Artwork".into());
            if (scale - 1.0).abs() > 1e-9 {
                img.transform(xf, true);
            }
            let (parent, idx, _) = d.position(gid).ok_or(EngineError::NoNode(gid))?;
            d.insert(parent, idx, img)?;
        }
        sel.set([gid]);
        Ok(())
    })?;
    // matched to a chart's threads
    if let Some(chart) = &chart {
        s.execute("embroidery.matchThreads", &json!({ "chart": chart, "ids": [gid.0] }))?;
    }
    // outlined: round the whole design, or round each filled shape, in the darkest thread
    let mut outlines = 0usize;
    if outline != "none" {
        let (fills, darkest) = {
            let d = &s.doc()?.doc;
            let kids: Vec<Arc<vectorcraft_doc::Node>> = d.node(gid).and_then(|g| g.children().cloned()).unwrap_or_default();
            let fills: Vec<u64> =
                kids.iter().filter(|n| n.embroidery.as_deref().is_some_and(|e| matches!(e.stitch, StitchType::Fill(_)))).map(|n| n.id.0).collect();
            let darkest = kids
                .iter()
                .filter_map(|n| n.embroidery.as_deref().map(|e| vectorcraft_embroidery::node_thread(n, e).color))
                .min_by(|a, b| a.to_lab()[0].total_cmp(&b.to_lab()[0]));
            (fills, darkest)
        };
        let color = outline_color.or(darkest).unwrap_or(Rgb::new(0x1a, 0x1a, 0x1a)).to_hex();
        let groups: Vec<Vec<u64>> = if outline == "design" { vec![vec![gid.0]] } else { fills.iter().map(|id| vec![*id]).collect() };
        for ids in groups {
            s.execute("embroidery.outlineAround", &json!({ "ids": ids, "gap": 0, "width": outline_width, "color": color }))?;
            outlines += 1;
        }
        s.execute("select.set", &json!({ "ids": [gid.0] }))?;
    }
    // a border round it all, sewn first
    let border_id = if border > 0.0 {
        let r = s.execute("embroidery.addBorder", &json!({ "ids": [gid.0], "width": border, "color": border_color, "edge": border_edge }))?;
        s.execute("select.set", &json!({ "ids": [gid.0] }))?;
        r["id"].as_u64()
    } else {
        None
    };
    Ok(json!({
        "id": gid.0, "objects": objects, "satin": satin, "fill": fill, "run": run, "outlines": outlines, "border": border_id,
        "removed": removed, "holesClosed": holes_closed, "gapsFilled": gaps_filled, "colors": order.len(), "scale": scale,
    }))
}

/// Auto-Digitize's details: shapes no thicker than this (mm) are sewn as runs, on top.
const DETAILS: f64 = 1.2;
/// A dark detail at least this long (mm) is a line, kept however small its area; a detail of
/// another colour is a line from the longer length. Shorter ones are slivers, left out.
const MIN_LINE: f64 = 1.5;
const MIN_COLOUR_LINE: f64 = 3.0;
/// A dark line's stitch length (mm): short, to follow its curves.
const DETAIL_STITCH: f64 = 1.5;

/// The line down the middle of a thin region (mm) without holes: its outline split at its two
/// ends (the points farthest apart) into two sides, and the points halfway between them.
fn centreline(region: &sf_core::geom::Polygon) -> Option<Vec<sf_core::Point>> {
    use sf_core::Point;
    if !region.holes.is_empty() {
        return None;
    }
    // at most 400 points, to keep the search for the ends quick
    let step = region.outer.len().div_ceil(400).max(1);
    let ring: Vec<Point> = region.outer.iter().step_by(step).copied().collect();
    let n = ring.len();
    if n < 4 {
        return None;
    }
    let (mut a, mut b, mut far) = (0, 0, 0.0);
    for (i, p) in ring.iter().enumerate() {
        for (j, q) in ring.iter().enumerate().skip(i + 1) {
            let d = (p.x - q.x).hypot(p.y - q.y);
            if d > far {
                (a, b, far) = (i, j, d);
            }
        }
    }
    if far <= 0.0 {
        return None;
    }
    // the two sides from end `a` to end `b`
    let one: Vec<Point> = (a..=b).filter_map(|i| ring.get(i).copied()).collect();
    let other: Vec<Point> = (b..n + a + 1).rev().filter_map(|i| ring.get(i % n).copied()).collect();
    let length = |l: &[Point]| l.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum::<f64>();
    let along = |l: &[Point], t: f64| -> Option<Point> {
        let total = length(l);
        let mut left = t * total;
        for w in l.windows(2) {
            let d = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
            if left <= d && d > 0.0 {
                let k = left / d;
                return Some(Point::new(w[0].x + (w[1].x - w[0].x) * k, w[0].y + (w[1].y - w[0].y) * k));
            }
            left -= d;
        }
        l.last().copied()
    };
    let steps = ((length(&one).max(length(&other)) / 0.4).ceil() as usize).clamp(2, 2000);
    let mid: Vec<Point> = (0..=steps)
        .filter_map(|k| {
            let t = k as f64 / steps as f64;
            let (p, q) = (along(&one, t)?, along(&other, t)?);
            Some(Point::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0))
        })
        .collect();
    (mid.len() >= 2).then_some(mid)
}

/// The lightness (CIELAB L*) below which a detail is dark.
const DARK_LINE: f64 = 40.0;

/// How far a shape reaches into the gap it fills, past its edge (mm), so no seam is left.
const GAP_REACH: f64 = 0.15;

/// The widest seam between colours that is filled whatever its length (mm), and the largest (mm²).
const MAX_SEAM_WIDTH: f64 = 1.0;
const MAX_SEAM_AREA: f64 = 200.0;

/// Fill the gaps smaller than `cap` (mm²), and the seams no wider than `seam` (mm), that the shapes in `nodes` leave between them: each
/// is given to the shape sharing most of its edge, which then covers it. → how many were filled.
fn fill_gaps(nodes: &mut [vectorcraft_doc::Node], cap: f64, seam: f64) -> usize {
    use sf_core::geom::{Polygon, poly};
    let regions: Vec<Vec<Polygon>> = nodes.iter().map(regions_mm).collect();
    let all: Vec<Polygon> = regions.iter().flatten().cloned().collect();
    if all.is_empty() {
        return 0;
    }
    let gaps: Vec<Polygon> = poly::union(&all)
        .iter()
        .flat_map(|p| p.holes.iter())
        .map(|h| Polygon::from_ring(h.clone()))
        .filter(|g| g.area() < cap || (g.area() < MAX_SEAM_AREA && g.thickness() <= seam))
        .collect();
    let mut given: Vec<Vec<Polygon>> = vec![Vec::new(); nodes.len()];
    let mut filled = 0;
    for g in gaps {
        let gb = g.bbox().expanded(0.5);
        let near = poly::offset(std::slice::from_ref(&g), 0.3);
        let best = regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.iter().any(|p| p.bbox().intersects(&gb)))
            .map(|(i, r)| (i, poly::intersection(&near, r).iter().map(Polygon::area).sum::<f64>()))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, shared)) = best
            && shared > 1e-4
            && let Some(v) = given.get_mut(i)
        {
            v.push(g);
            filled += 1;
        }
    }
    for (i, gaps) in given.into_iter().enumerate() {
        let (Some(node), Some(mine)) = (nodes.get_mut(i), regions.get(i)) else { continue };
        if gaps.is_empty() {
            continue;
        }
        let grown = poly::union2(mine, &poly::offset(&gaps, GAP_REACH));
        let pt = |p: &sf_core::Point| vectorcraft_geom::path::Anchor::corner(kurbo::Point::new(p.x * PT_PER_MM, p.y * PT_PER_MM));
        let subpaths = grown
            .iter()
            .flat_map(|p| p.rings())
            .filter(|r| r.len() >= 3)
            .map(|r| vectorcraft_geom::path::SubPath::new(r.iter().map(pt).collect(), true))
            .collect();
        node.kind = NodeKind::Path {
            path: vectorcraft_geom::PathData { subpaths },
            rule: vectorcraft_geom::FillRule::EvenOdd,
            live: None,
            clipping: false,
            guide: false,
        };
    }
    filled
}

/// Whether a path's or compound path's fill covers `at` (document points), by its fill rule.
fn paints(n: &vectorcraft_doc::Node, at: kurbo::Point) -> bool {
    use kurbo::Shape;
    let rule = match &n.kind {
        NodeKind::Path { rule, .. } | NodeKind::Compound { rule, .. } => *rule,
        _ => return false,
    };
    let Some(path) = n.stroke_path() else { return false };
    let w = path.winding(at);
    match rule {
        vectorcraft_geom::FillRule::EvenOdd => w % 2 != 0,
        vectorcraft_geom::FillRule::NonZero => w != 0,
    }
}

/// A point inside a closed subpath: the centroid of its outline, when that is inside it.
fn inside_point(sp: &vectorcraft_geom::path::SubPath) -> Option<kurbo::Point> {
    use kurbo::Shape;
    let mut bp = kurbo::BezPath::new();
    sp.to_bezpath_into(&mut bp);
    let mut pts = Vec::new();
    kurbo::flatten(&bp, 0.05, |e| match e {
        kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => pts.push(p),
        _ => {}
    });
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for (i, p) in pts.iter().enumerate() {
        let q = pts.get((i + 1) % pts.len())?;
        let k = p.x * q.y - q.x * p.y;
        a += k;
        cx += (p.x + q.x) * k;
        cy += (p.y + q.y) * k;
    }
    if a.abs() < 1e-12 {
        return None;
    }
    let c = kurbo::Point::new(cx / (3.0 * a), cy / (3.0 * a));
    (c.is_finite() && bp.winding(c) != 0).then_some(c)
}

/// Remove the holes smaller than `cap` (square points) from the shapes in `nodes` where no shape
/// is painted: a hole left by a dropped speck is sewn in the colour round it. A hole with
/// another part of its shape inside it stays. → how many holes were closed.
fn close_small_holes(nodes: &mut [vectorcraft_doc::Node], cap: f64) -> usize {
    use kurbo::Shape;
    use vectorcraft_geom::path::SubPath;
    let mut closed = 0;
    for i in 0..nodes.len() {
        let Some(node) = nodes.get(i) else { continue };
        // every ring of the shape: (its compound member, its subpath index, the subpath)
        let rings: Vec<(Option<usize>, usize, &SubPath)> = match &node.kind {
            NodeKind::Path { path, .. } => path.subpaths.iter().enumerate().map(|(j, sp)| (None, j, sp)).collect(),
            NodeKind::Compound { children, .. } => children
                .iter()
                .enumerate()
                .flat_map(|(c, ch)| {
                    ch.path_data().map(|pd| pd.subpaths.iter().enumerate().map(|(j, sp)| (Some(c), j, sp)).collect::<Vec<_>>()).unwrap_or_default()
                })
                .collect(),
            _ => continue,
        };
        if rings.len() < 2 {
            continue;
        }
        let shapes: Vec<kurbo::BezPath> = rings
            .iter()
            .map(|r| {
                let mut bp = kurbo::BezPath::new();
                r.2.to_bezpath_into(&mut bp);
                bp
            })
            .collect();
        let areas: Vec<f64> = shapes.iter().map(|b| b.area().abs()).collect();
        let points: Vec<Option<kurbo::Point>> = rings.iter().map(|r| if r.2.closed { inside_point(r.2) } else { None }).collect();
        let mut drop = Vec::new();
        for (k, ring) in rings.iter().enumerate() {
            let (Some(&area), Some(Some(at)), Some(shape)) = (areas.get(k), points.get(k), shapes.get(k)) else { continue };
            if area >= cap || paints(node, *at) {
                continue;
            }
            let enclosed = shapes.iter().zip(&areas).enumerate().any(|(m, (o, a))| m != k && *a > area && o.winding(*at) != 0);
            let has_island = points.iter().zip(&areas).enumerate().any(|(m, (p, a))| m != k && *a < area && p.is_some_and(|p| shape.winding(p) != 0));
            let covered = nodes.iter().enumerate().any(|(m, o)| m != i && paints(o, *at));
            if enclosed && !has_island && !covered {
                drop.push((ring.0, ring.1));
            }
        }
        if drop.is_empty() {
            continue;
        }
        closed += drop.len();
        // the subpaths removed from the last, so the indices stay right
        drop.sort_unstable_by(|a, b| b.cmp(a));
        let Some(node) = nodes.get_mut(i) else { continue };
        match &mut node.kind {
            NodeKind::Path { path, .. } => {
                for (_, j) in &drop {
                    if *j < path.subpaths.len() {
                        path.subpaths.remove(*j);
                    }
                }
            }
            NodeKind::Compound { children, .. } => {
                for (c, j) in &drop {
                    if let Some(ch) = c.and_then(|c| children.get_mut(c))
                        && let Some(pd) = Arc::make_mut(ch).path_data_mut()
                        && *j < pd.subpaths.len()
                    {
                        pd.subpaths.remove(*j);
                    }
                }
                children.retain(|ch| ch.path_data().is_none_or(|pd| !pd.subpaths.is_empty()));
            }
            _ => {}
        }
    }
    closed
}

fn stamp_named(name: &str) -> Option<Stamp> {
    Some(match name {
        "free" => Stamp::Free,
        "line" => Stamp::Line,
        "ellipse" | "circle" => Stamp::Ellipse,
        "rectangle" | "square" => Stamp::Rectangle,
        "roundedRectangle" | "rounded_rectangle" => Stamp::RoundedRectangle,
        "triangle" => Stamp::Triangle,
        "diamond" => Stamp::Diamond,
        "pentagon" => Stamp::Pentagon,
        "hexagon" => Stamp::Hexagon,
        "star" => Stamp::Star,
        "heart" => Stamp::Heart,
        "arrow" => Stamp::Arrow,
        _ => return None,
    })
}

fn draw(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.draw";
    let brush = str_param(p, "brush").unwrap_or("run");
    let width = f64_or(p, "width", 3.0).clamp(0.3, 50.0);
    let density = f64_or(p, "density", 0.4).clamp(0.1, 10.0);
    let length = f64_or(p, "length", 2.5).clamp(0.3, 12.0);
    let stamp_kind = match str_param(p, "stamp") {
        Some(n) => stamp_named(n).ok_or_else(|| bad(CMD, format!("unknown stamp `{n}`")))?,
        None => Stamp::Free,
    };
    let closed_brush = matches!(brush, "fill" | "applique" | "satin");
    let contour = if stamp_kind != Stamp::Free {
        let from = point_param(p, "from").ok_or_else(|| bad(CMD, "a stamp needs `from` and `to`"))?;
        let to = point_param(p, "to").ok_or_else(|| bad(CMD, "a stamp needs `from` and `to`"))?;
        stamp(stamp_kind, sf_point(from), sf_point(to), bool_or(p, "square", false)).ok_or_else(|| bad(CMD, "drag to give the shape a size"))?
    } else {
        let pts: Vec<sf_core::Point> = p
            .get("points")
            .and_then(Value::as_array)
            .ok_or_else(|| bad(CMD, "missing `points` (or `stamp`)"))?
            .iter()
            .filter_map(|q| {
                let a = q.as_array()?;
                let (x, y) = (a.first()?.as_f64()?, a.get(1)?.as_f64()?);
                (x.is_finite() && y.is_finite()).then(|| sf_core::Point::new(x, y))
            })
            .take(20_000)
            .collect();
        if pts.len() < 2 {
            return Err(bad(CMD, "a brush stroke needs at least two points"));
        }
        let closed = bool_or(p, "closed", closed_brush);
        let tol = f64_or(p, "smooth", 0.8).clamp(0.05, 20.0);
        fit_stroke(&pts, tol, closed)
    };
    let closed = contour.closed;
    let profile = match brush {
        "tapered" => WidthProfile::Tapered,
        "calligraphy" => WidthProfile::Calligraphy { nib_angle: f64_or(p, "nibAngle", 145.0) },
        "pressure" => WidthProfile::Pressure(
            p.get("pressures")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_f64).map(|v| v.clamp(0.0, 1.0) as f32).take(20_000).collect())
                .unwrap_or_default(),
        ),
        _ => WidthProfile::Constant,
    };
    let stitch = match brush {
        "run" => StitchType::Run(RunParams { stitch_length: length, ..RunParams::default() }),
        "bean" => StitchType::Run(RunParams { kind: RunKind::Triple, stitch_length: length, ..RunParams::default() }),
        "steil" | "tapered" | "calligraphy" | "pressure" | "satin" => StitchType::Satin(SatinParams { spacing: density, ..SatinParams::default() }),
        "applique" => StitchType::Applique(AppliqueParams { cover_width: width.min(8.0), cover_spacing: density, ..AppliqueParams::default() }),
        "fill" => StitchType::Fill(FillParams { spacing: density, stitch_length: length.max(2.0), ..FillParams::default() }),
        other => return Err(bad(CMD, format!("unknown brush `{other}`"))),
    };
    let color = match p.get("color") {
        Some(v) => color_value(v).ok_or_else(|| bad(CMD, format!("not a colour: {v}")))?,
        None => Color::rgb8(0x1a, 0x3e, 0x8c),
    };
    let mut e = Embroidery::new(stitch);
    if matches!(brush, "steil" | "tapered" | "calligraphy" | "pressure") {
        e.width = Some(width);
        e.profile = profile;
    }
    let path = contour_to_path(&contour);
    let area = closed && !matches!(brush, "run" | "bean" | "steil" | "tapered" | "calligraphy" | "pressure");
    let appearance = if area {
        Appearance::basic(Paint::solid(color), Paint::None, 0.0)
    } else {
        let w = if matches!(e.stitch, StitchType::Satin(_)) { width * PT_PER_MM } else { 0.75 };
        Appearance::basic(Paint::None, Paint::solid(color), w)
    };
    let kind = NodeKind::Path { path, rule: vectorcraft_geom::FillRule::NonZero, live: None, clipping: false, guide: false };
    // Digitize Before / After: next to an object in the sewing (paint) order, else on top.
    let (parent, index) = match (str_param(p, "order"), id_param(p, "relativeTo")) {
        (Some(order @ ("before" | "after")), Some(rel)) => {
            let (parent, idx, _) = s.doc()?.doc.position(rel).ok_or_else(|| bad(CMD, format!("no such object id {}", rel.0)))?;
            (parent, if order == "after" { idx + 1 } else { idx })
        }
        (Some("before" | "after"), None) => return Err(bad(CMD, "`order` before or after needs `relativeTo`")),
        (Some(o), _) if o != "top" => return Err(bad(CMD, "order must be top, before or after")),
        _ => (s.doc()?.insertion_parent(), usize::MAX),
    };
    let label = "Brush Stroke";
    let id = s.edit(label, |d, sel| {
        let id = d.alloc_id();
        let mut n = vectorcraft_doc::Node::new(id, kind);
        n.appearance = appearance.into();
        n.embroidery = Some(Box::new(e));
        d.insert(parent, index, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0 }))
}

/// The hoop `embroidery.settings` was given: a hoop's name, a custom size or none.
fn settings_hoop(s: &Session, h: &Value) -> Result<Option<sf_core::hoops::Hoop>> {
    const CMD: &str = "embroidery.settings";
    Ok(match h {
        Value::Null => None,
        Value::String(name) => Some(super::embroidery_hoops::find_hoop(s, name).ok_or_else(|| bad(CMD, format!("unknown hoop `{name}`")))?),
        Value::Object(_) => {
            let w = h.get("width").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 1.0).ok_or_else(|| bad(CMD, "hoop width (mm) > 1"))?;
            let ht = h.get("height").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 1.0).ok_or_else(|| bad(CMD, "hoop height (mm) > 1"))?;
            let shape = if str_param(h, "shape") == Some("oval") { sf_core::hoops::HoopShape::Oval } else { sf_core::hoops::HoopShape::Rect };
            Some(sf_core::hoops::Hoop::new(str_param(h, "name").unwrap_or("Custom"), w, ht, shape))
        }
        _ => return Err(bad(CMD, "hoop is a name, {width, height, shape?} or null")),
    })
}

/// Sets the design's machine profile (its settings and format), or clears it.
fn settings_machine(s: &Session, cfg: &mut DocEmbroidery, m: &Value) -> Result<()> {
    const CMD: &str = "embroidery.settings";
    match m {
        Value::Null => (cfg.machine, cfg.machine_format) = (String::new(), String::new()),
        Value::String(f) if f.trim().is_empty() => (cfg.machine, cfg.machine_format) = (String::new(), String::new()),
        Value::String(f) => {
            let saved = &s.prefs.embroidery_machines;
            let profile = sf_core::machines::find(saved, f).ok_or_else(|| {
                let names: Vec<String> = sf_core::machines::profiles(saved).into_iter().map(|m| m.name).collect();
                bad(CMD, format!("unknown machine `{f}`: one of {}", names.join(", ")))
            })?;
            cfg.settings = profile.apply(&cfg.settings);
            // the user's profiles: the design keeps the format too, for where it isn't saved
            cfg.machine_format = if profile.is_standard() { String::new() } else { profile.format.clone() };
            cfg.machine = profile.name;
        }
        _ => return Err(bad(CMD, "machine is a machine profile's name (\"dst\", \"pes\", … or one of yours) or null")),
    }
    Ok(())
}

/// Sets the Check Design limits given; whether any was.
fn settings_checks(lim: &mut sf_core::attr::CheckLimits, c: &Value) -> Result<bool> {
    const CMD: &str = "embroidery.settings";
    let o = c.as_object().ok_or_else(|| {
        bad(CMD, "checks is {denseLayers?, longStitch?, smallLettering?, tinyObject?, garment?, shortStitch?, narrowSatin?, shortFill?}")
    })?;
    let mut changed = false;
    // clothing or not sets the long-stitch limit, unless it is given too
    if let Some(g) = o.get("garment") {
        let g = g.as_bool().ok_or_else(|| bad(CMD, "checks.garment is true or false"))?;
        lim.garment = g;
        lim.long_stitch = if g { sf_core::attr::LONG_STITCH_GARMENT } else { sf_core::attr::LONG_STITCH_OTHER };
        changed = true;
    }
    for (k, slot, lo, hi) in [
        ("denseLayers", &mut lim.dense_layers, 1.5, 20.0),
        ("longStitch", &mut lim.long_stitch, 2.0, 20.0),
        ("smallLettering", &mut lim.small_lettering, 1.0, 30.0),
        ("tinyObject", &mut lim.tiny_object, 0.1, 10.0),
        ("shortStitch", &mut lim.short_stitch, 0.0, 3.0),
        ("narrowSatin", &mut lim.narrow_satin, 0.0, 5.0),
        ("shortFill", &mut lim.short_fill, 0.0, 10.0),
    ] {
        if let Some(v) = o.get(k) {
            *slot = v.as_f64().filter(|x| (lo..=hi).contains(x)).ok_or_else(|| bad(CMD, format!("checks.{k} is a number from {lo} to {hi}")))?;
            changed = true;
        }
    }
    Ok(changed)
}

/// The cost model `embroidery.settings` was given, from the design's current one.
fn settings_cost(current: Option<&sf_core::attr::CostModel>, v: &Value) -> Result<Option<sf_core::attr::CostModel>> {
    const CMD: &str = "embroidery.settings";
    Ok(match v {
        Value::Null | Value::Bool(false) => None,
        Value::Bool(true) => Some(current.cloned().unwrap_or_default()),
        Value::Object(o) => {
            let mut m = current.cloned().unwrap_or_default();
            for (k, slot, hi) in [
                ("perThousand", &mut m.per_thousand, 1000.0),
                ("perHour", &mut m.per_hour, 10_000.0),
                ("threadPerKm", &mut m.thread_per_km, 10_000.0),
                ("setup", &mut m.setup, 1_000_000.0),
            ] {
                if let Some(v) = o.get(k) {
                    *slot = v.as_f64().filter(|x| (0.0..=hi).contains(x)).ok_or_else(|| bad(CMD, format!("cost.{k} is 0 to {hi}")))?;
                }
            }
            if let Some(v) = o.get("heads") {
                m.heads = v
                    .as_u64()
                    .filter(|h| (1..=100).contains(h))
                    .and_then(|h| u32::try_from(h).ok())
                    .ok_or_else(|| bad(CMD, "cost.heads is 1 to 100"))?;
            }
            if let Some(v) = o.get("currency") {
                m.currency = v
                    .as_str()
                    .filter(|c| c.chars().count() <= 4 && !c.chars().any(char::is_control))
                    .ok_or_else(|| bad(CMD, "cost.currency is up to 4 characters"))?
                    .to_string();
            }
            Some(m)
        }
        _ => return Err(bad(CMD, "cost is true, false or {perThousand?, perHour?, threadPerKm?, setup?, heads?, currency?}")),
    })
}

/// Where in the hoop the design sits: its centre (none) or an offset in mm.
fn settings_hoop_position(v: &Value) -> Result<Option<[f64; 2]>> {
    const CMD: &str = "embroidery.settings";
    Ok(match v {
        Value::Null => None,
        Value::String(c) if c == "centre" || c == "center" => None,
        Value::Array(a) => {
            let n = |i: usize| a.get(i).and_then(Value::as_f64).filter(|v| v.is_finite() && v.abs() <= 1000.0);
            match (n(0), n(1), a.len()) {
                (Some(x), Some(y), 2) => Some([x, y]),
                _ => return Err(bad(CMD, "hoopPosition is [x, y]: mm from the hoop's centre, each within 1000")),
            }
        }
        _ => return Err(bad(CMD, "hoopPosition is centre or [x, y] (mm from the hoop's centre)")),
    })
}

/// Sets the registration marks' values given.
fn settings_registration(r: &mut sf_core::model::RegistrationMarks, v: &Value) -> Result<()> {
    const CMD: &str = "embroidery.settings";
    let o = v.as_object().ok_or_else(|| bad(CMD, "registration is {shape?, size?, perOverlap?, color?}"))?;
    if let Some(sh) = o.get("shape") {
        r.shape =
            sh.as_str().and_then(sf_core::model::MarkShape::from_id).ok_or_else(|| bad(CMD, "registration.shape is cross, circle or target"))?;
    }
    if let Some(v) = o.get("size") {
        r.size = v.as_f64().filter(|x| (1.0..=20.0).contains(x)).ok_or_else(|| bad(CMD, "registration.size is 1 to 20 mm"))?;
    }
    if let Some(v) = o.get("perOverlap") {
        r.per_overlap = v.as_u64().filter(|n| (1..=4).contains(n)).ok_or_else(|| bad(CMD, "registration.perOverlap is 1 to 4"))? as u8;
    }
    if let Some(v) = o.get("color") {
        r.color = v.as_str().and_then(Rgb::from_hex).ok_or_else(|| bad(CMD, "registration.color is \"#rrggbb\""))?;
    }
    Ok(())
}

/// The basting `embroidery.settings` was given, from the design's current one.
fn settings_basting(current: Option<sf_core::model::Basting>, v: &Value) -> Result<Option<sf_core::model::Basting>> {
    const CMD: &str = "embroidery.settings";
    Ok(match v {
        Value::Null | Value::Bool(false) => None,
        Value::Bool(true) => Some(current.unwrap_or_default()),
        Value::Object(o) => {
            let mut b = current.unwrap_or_default();
            if let Some(h) = o.get("hoop") {
                b.hoop = h.as_bool().ok_or_else(|| bad(CMD, "basting.hoop is true or false"))?;
            }
            for (k, slot, lo, hi) in [("margin", &mut b.margin, 0.0, 50.0), ("length", &mut b.length, 2.0, 12.0)] {
                if let Some(v) = o.get(k) {
                    *slot = v.as_f64().filter(|x| (lo..=hi).contains(x)).ok_or_else(|| bad(CMD, format!("basting.{k} is {lo} to {hi} mm")))?;
                }
            }
            Some(b)
        }
        _ => return Err(bad(CMD, "basting is true, false or {hoop?, margin?, length?}")),
    })
}

fn settings(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.settings";
    let mut cfg: DocEmbroidery = s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default();
    let mut changed = false;
    if let Some(h) = p.get("hoop") {
        changed = true;
        cfg.hoop = settings_hoop(s, h)?;
    }
    if let Some(f) = str_param(p, "fabric") {
        cfg.fabric = f.to_string();
        changed = true;
        // a fabric worn or not sets what Check Design counts as a long stitch
        if let Some(g) = cfg.fabric(f).and_then(|fab| fab.garment) {
            cfg.checks.garment = g;
            cfg.checks.long_stitch = if g { sf_core::attr::LONG_STITCH_GARMENT } else { sf_core::attr::LONG_STITCH_OTHER };
        }
    }
    if let Some(c) = str_param(p, "chart") {
        cfg.chart = c.to_string();
        changed = true;
    }
    // A machine first, so values given with it override the machine's.
    if let Some(m) = p.get("machine") {
        changed = true;
        settings_machine(s, &mut cfg, m)?;
    }
    if let Some(c) = p.get("checks") {
        changed |= settings_checks(&mut cfg.checks, c)?;
    }
    let num = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0);
    let st = &mut cfg.settings;
    for (k, slot) in [
        ("trimThreshold", &mut st.trim_threshold),
        ("overlapTouching", &mut st.overlap_touching),
        ("tieLength", &mut st.tie_length),
        ("minStitch", &mut st.min_stitch),
        ("maxStitch", &mut st.max_stitch),
        ("maxJump", &mut st.max_jump),
        ("machineSpeed", &mut st.machine_speed),
        ("trimSeconds", &mut st.trim_seconds),
        ("colorChangeSeconds", &mut st.color_change_seconds),
    ] {
        if let Some(v) = num(k) {
            *slot = v;
            changed = true;
        }
    }
    if let Some(v) = p.get("tieIn").and_then(Value::as_bool) {
        st.tie_in = v;
        changed = true;
    }
    if let Some(v) = p.get("hideTravel").and_then(Value::as_bool) {
        st.hide_travel = v;
        changed = true;
    }
    if let Some(v) = p.get("tieOff").and_then(Value::as_bool) {
        st.tie_off = v;
        changed = true;
    }
    if let Some(v) = p.get("notes") {
        let t = match v {
            Value::Null => "",
            Value::String(t) => t.as_str(),
            _ => return Err(bad(CMD, "notes is text")),
        };
        cfg.notes = t.trim().chars().take(2000).collect();
        changed = true;
    }
    if let Some(v) = p.get("cost") {
        cfg.cost = settings_cost(cfg.cost.as_ref(), v)?;
        changed = true;
    }
    for (key, slot) in [("start", &mut st.start), ("end", &mut st.end)] {
        if let Some(v) = p.get(key) {
            *slot = match v {
                Value::Null => None,
                Value::String(id) if id.is_empty() || id == "none" => None,
                Value::String(id) => Some(sf_core::model::DesignPoint::from_id(id).ok_or_else(|| {
                    bad(CMD, format!("{key} is centre, top-left, top, top-right, left, right, bottom-left, bottom, bottom-right or null"))
                })?),
                _ => return Err(bad(CMD, format!("{key} is a point of the design's box or null"))),
            };
            changed = true;
        }
    }
    if let Some(v) = p.get("hoopPosition") {
        st.hoop_offset = settings_hoop_position(v)?;
        changed = true;
    }
    if let Some(v) = p.get("registration") {
        settings_registration(&mut st.registration, v)?;
        changed = true;
    }
    if let Some(v) = p.get("basting") {
        st.basting = settings_basting(st.basting, v)?;
        changed = true;
    }
    if let Some(v) = p.get("tieStyle") {
        let name = v.as_str().unwrap_or_default();
        st.tie_style =
            sf_core::model::TieStyle::ALL.into_iter().find(|t| t.id() == name).ok_or_else(|| bad(CMD, "tieStyle is single, double or triangle"))?;
        changed = true;
    }
    if changed {
        let new = cfg.clone();
        // a hoop chosen: the page follows it (the first artboard takes the hoop's size about its
        // middle, so the design stays where it is)
        let follow = p.get("page").and_then(Value::as_bool).unwrap_or(true);
        let page = (follow && p.get("hoop").is_some()).then_some(cfg.hoop.as_ref()).flatten().map(|h| (h.width * PT_PER_MM, h.height * PT_PER_MM));
        s.edit("Embroidery Settings", |d, _| {
            d.embroidery = Some(Box::new(new));
            if let Some((w, h)) = page.filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0)
                && let Some(a) = d.artboards.first_mut()
                && ((a.rect.width() - w).abs() > 0.01 || (a.rect.height() - h).abs() > 0.01)
            {
                let c = a.rect.center();
                a.rect = kurbo::Rect::new(c.x - w / 2.0, c.y - h / 2.0, c.x + w / 2.0, c.y + h / 2.0);
            }
            Ok(())
        })?;
    }
    serde_json::to_value(&cfg).map_err(|e| bad(CMD, e.to_string()))
}

fn apply_fabric(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.applyFabric";
    let name = str_param(p, "name").ok_or_else(|| bad(CMD, "missing fabric `name`"))?;
    let fabric =
        s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default().fabric(name).ok_or_else(|| bad(CMD, format!("unknown fabric `{name}`")))?;
    let ids: Vec<NodeId> = vectorcraft_embroidery::stitched_nodes(&s.doc()?.doc).iter().map(|n| n.id).collect();
    s.edit("Apply Fabric", |d, _| {
        for id in &ids {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            if let Some(e) = n.embroidery.as_deref_mut() {
                match &mut e.stitch {
                    StitchType::Satin(sp) => {
                        sp.spacing = fabric.satin_spacing;
                        sp.pull_comp = fabric.pull_comp;
                        sp.push_comp = fabric.push_comp;
                        fabric.satin_underlay.apply(&mut sp.underlay);
                    }
                    StitchType::Fill(f) => {
                        f.spacing = if f.kind == FillKind::Satin { fabric.satin_spacing } else { fabric.fill_spacing };
                        f.stitch_length = fabric.fill_stitch_length;
                        f.pull_comp = fabric.pull_comp;
                        f.push_comp = fabric.push_comp;
                        f.underlay.tatami = fabric.fill_underlay;
                        f.underlay.edge_run = fabric.fill_edge_run;
                        f.underlay.spacing = fabric.underlay_spacing;
                    }
                    StitchType::Applique(a) => {
                        a.cover_spacing = fabric.satin_spacing;
                        a.pull_comp = fabric.pull_comp;
                    }
                    StitchType::Run(_)
                    | StitchType::Manual
                    | StitchType::Branch
                    | StitchType::Redwork(_)
                    | StitchType::Lettering(_)
                    | StitchType::Decoration(_) => {}
                }
            }
        }
        let cfg = d.embroidery.get_or_insert_with(Box::default);
        cfg.fabric = fabric.name.to_string();
        // the fabric's own fill stitch length isn't flagged as short
        cfg.checks.short_fill = sf_core::attr::CheckLimits::default().short_fill.min(fabric.fill_stitch_length);
        // worn and washed or not: what Check Design counts as a long stitch
        if let Some(g) = fabric.garment {
            cfg.checks.garment = g;
            cfg.checks.long_stitch = if g { sf_core::attr::LONG_STITCH_GARMENT } else { sf_core::attr::LONG_STITCH_OTHER };
        }
        Ok(())
    })?;
    Ok(json!({ "objects": ids.len(), "fabric": fabric.name }))
}

fn sort_by_color(s: &mut Session, _p: &Value) -> Result<Value> {
    let plan = s.embroidery_plan().ok_or_else(|| bad("embroidery.sortByColor", "no document"))?;
    // Thread of each stitched node, in current order.
    let mut color_of: std::collections::HashMap<NodeId, Rgb> = std::collections::HashMap::new();
    for (k, obj) in plan.design.objects.iter().enumerate() {
        if let Some(node) = plan.nodes.get(k) {
            color_of.entry(*node).or_insert_with(|| plan.design.thread(obj.color).color);
        }
    }
    let mut moved = 0usize;
    s.edit("Sort by Thread Color", |d, _| {
        let parents: Vec<Option<NodeId>> = {
            let mut v: Vec<Option<NodeId>> = color_of.keys().map(|id| d.parent_of(*id)).collect();
            v.sort_by_key(|p| p.map(|x| x.0));
            v.dedup();
            v
        };
        for parent in parents {
            let Some(parent) = parent else { continue };
            let Some(children) = d.node(parent).and_then(|n| n.children()).cloned() else { continue };
            // positions held by stitched children, and their colours in order of first use
            let slots: Vec<usize> = children.iter().enumerate().filter(|(_, c)| color_of.contains_key(&c.id)).map(|(i, _)| i).collect();
            let mut order: Vec<Rgb> = Vec::new();
            for &i in &slots {
                if let Some(c) = children.get(i).and_then(|n| color_of.get(&n.id))
                    && !order.contains(c)
                {
                    order.push(*c);
                }
            }
            let mut sorted: Vec<NodeId> = slots.iter().filter_map(|&i| children.get(i).map(|n| n.id)).collect();
            let rank = |id: &NodeId| color_of.get(id).and_then(|c| order.iter().position(|o| o == c)).unwrap_or(usize::MAX);
            sorted.sort_by_key(rank); // stable: keeps the order within one colour
            for (slot, id) in slots.iter().zip(&sorted) {
                if children.get(*slot).map(|n| n.id) != Some(*id) {
                    moved += 1;
                }
            }
            // the stitched objects trade places; anything else stays where it is
            let nodes: Vec<Arc<vectorcraft_doc::Node>> = sorted.iter().filter_map(|id| children.iter().find(|c| c.id == *id).cloned()).collect();
            let ch = d.children_mut(Some(parent))?;
            for (&slot, node) in slots.iter().zip(nodes) {
                ch[slot] = node;
            }
        }
        Ok(())
    })?;
    Ok(json!({ "moved": moved }))
}

fn outline_around(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.outlineAround";
    let gap = super::embroidery_lettering::ranged(p, "gap", 0.0..=50.0, CMD, "`gap` is 0 to 50 mm")?.unwrap_or(2.0);
    let kind = str_param(p, "kind").unwrap_or("satin");
    let width = super::embroidery_lettering::ranged(p, "width", 0.5..=10.0, CMD, "`width` is 0.5 to 10 mm")?.unwrap_or(3.0);
    let stitch = match kind {
        "satin" => StitchType::Satin(SatinParams::default()),
        "run" => StitchType::Run(RunParams::default()),
        "triple" => StitchType::Run(RunParams { kind: RunKind::Triple, ..RunParams::default() }),
        // light tatami: knockdown stitches press a towel's or fleece's pile flat under the design
        "knockdown" | "emboss" => StitchType::Fill(FillParams { spacing: 1.2, angle: 45.0, ..FillParams::default() }),
        // echo quilting: running-stitch outlines stepping out from the art
        "echo" => StitchType::Run(RunParams::default()),
        // background quilting: a meander wandering round the art
        "stipple" => StitchType::Fill(FillParams { kind: FillKind::Meander, ..FillParams::default() }),
        _ => return Err(bad(CMD, "`kind` is satin, run, triple, knockdown, emboss, echo or stipple")),
    };
    let filled = matches!(kind, "knockdown" | "emboss" | "stipple");
    let count = super::embroidery_lettering::ranged(p, "count", 1.0..=30.0, CMD, "`count` is 1 to 30")?.map_or(3, |c| c.round() as usize);
    let spacing = super::embroidery_lettering::ranged(p, "spacing", 0.5..=50.0, CMD, "`spacing` is 0.5 to 50 mm")?.unwrap_or(3.0);
    let circle = match str_param(p, "shape").unwrap_or("rectangle") {
        "rectangle" => false,
        "circle" => true,
        _ => return Err(bad(CMD, "`shape` is rectangle or circle")),
    };
    let extend = super::embroidery_lettering::ranged(p, "extend", 0.0..=200.0, CMD, "`extend` is 0 to 200 mm")?.unwrap_or(5.0);
    let color = match str_param(p, "color") {
        Some(c) => Rgb::from_hex(c).ok_or_else(|| bad(CMD, "`color` is #rrggbb"))?,
        None => Rgb::new(0x1a, 0x1a, 0x1a),
    };
    let ids = super::edit::roots_of(&s.doc()?.doc, targets(s, p)?);
    // the selected art and everything in it
    let mut inside = std::collections::HashSet::new();
    {
        let d = &s.doc()?.doc;
        let mut stack: Vec<NodeId> = ids.clone();
        while let Some(id) = stack.pop() {
            if inside.insert(id)
                && let Some(kids) = d.node(id).and_then(|n| n.children())
            {
                stack.extend(kids.iter().map(|k| k.id));
            }
        }
    }
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let polys: Vec<sf_core::geom::Polygon> = plan
        .design
        .objects
        .iter()
        .zip(&plan.nodes)
        .filter(|(_, n)| inside.contains(*n))
        .flat_map(|(o, _)| sf_core::engine::object_polygons(o))
        .filter(|p| p.outer.len() >= 3)
        .collect();
    if polys.is_empty() {
        return Err(bad(CMD, "select stitched art to outline"));
    }
    // a satin outline's inner edge `gap` from the art; a run `gap` from it
    let reach = gap + if kind == "satin" { width / 2.0 } else { 0.0 };
    let to_pt = |q: &sf_core::Point| kurbo::Point::new(q.x * PT_PER_MM, q.y * PT_PER_MM);
    // grown further and back again, so pieces close together (letters) share one outline
    let close = gap.max(1.0);
    let wide = sf_core::geom::poly::offset_round(&sf_core::geom::poly::union(&polys), reach + close);
    let grown = sf_core::geom::poly::offset_round(&wide, -close);
    let mut subpaths: Vec<vectorcraft_geom::path::SubPath> = grown
        .iter()
        .filter(|g| g.outer.len() >= 3)
        .map(|g| vectorcraft_geom::path::SubPath::polyline(&g.outer.iter().map(to_pt).collect::<Vec<_>>(), true))
        .collect();
    if subpaths.is_empty() {
        return Err(bad(CMD, "the art is too small to outline"));
    }
    let pieces = subpaths.len();
    // echo quilting: more outlines, each `spacing` further out (pieces that meet share one)
    if kind == "echo" {
        let union = sf_core::geom::poly::union(&polys);
        for k in 1..count {
            let out = reach + spacing * k as f64;
            let ring = sf_core::geom::poly::offset_round(&sf_core::geom::poly::offset_round(&union, out + close), -close);
            subpaths.extend(
                ring.iter()
                    .filter(|g| g.outer.len() >= 3)
                    .map(|g| vectorcraft_geom::path::SubPath::polyline(&g.outer.iter().map(to_pt).collect::<Vec<_>>(), true)),
            );
        }
    }
    // emboss and stipple: a rectangle or circle round it all, `extend` beyond, the art's outline left out
    if matches!(kind, "emboss" | "stipple") {
        let Some(b) = grown.iter().map(sf_core::geom::Polygon::bbox).reduce(sf_core::BBox::union) else {
            return Err(bad(CMD, "the art is too small to outline"));
        };
        let c = sf_core::Point::new((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
        // a rectangle given outright (mm): a quilt block round the art
        let given: Option<Vec<f64>> = p
            .get("rect")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_f64).filter(|v| v.is_finite()).collect())
            .filter(|v: &Vec<f64>| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0 && v[2] < 5000.0 && v[3] < 5000.0);
        let ring: Vec<sf_core::Point> = if let Some(r) = given {
            let (x0, y0, x1, y1) = (r[0], r[1], r[0] + r[2], r[1] + r[3]);
            vec![sf_core::Point::new(x0, y0), sf_core::Point::new(x1, y0), sf_core::Point::new(x1, y1), sf_core::Point::new(x0, y1)]
        } else if circle {
            let r = (b.width().hypot(b.height())) / 2.0 + extend;
            (0..96).map(|i| f64::from(i) * std::f64::consts::TAU / 96.0).map(|a| sf_core::Point::new(c.x + r * a.cos(), c.y + r * a.sin())).collect()
        } else {
            let (x0, y0, x1, y1) = (b.min.x - extend, b.min.y - extend, b.max.x + extend, b.max.y + extend);
            vec![sf_core::Point::new(x0, y0), sf_core::Point::new(x1, y0), sf_core::Point::new(x1, y1), sf_core::Point::new(x0, y1)]
        };
        subpaths.insert(0, vectorcraft_geom::path::SubPath::polyline(&ring.iter().map(to_pt).collect::<Vec<_>>(), true));
    }
    let path = vectorcraft_geom::path::PathData::new(subpaths);
    let parent = s.doc()?.insertion_parent();
    let id = s.edit("Outline Around", |d, sel| {
        let paint = Paint::solid(Color::rgb8(color.r, color.g, color.b));
        let look = if filled { Appearance::basic(paint, Paint::None, 0.0) } else { Appearance::basic(Paint::None, paint, 1.0) };
        let mut n = vectorcraft_doc::Node::path(d.alloc_id(), path, look);
        if let NodeKind::Path { rule, .. } = &mut n.kind {
            // the art's outline is a hole in an emboss shape
            *rule = vectorcraft_geom::FillRule::EvenOdd;
        }
        n.name = Some(match kind {
            "knockdown" => "Knockdown".into(),
            "emboss" => "Emboss".into(),
            "echo" => "Echo quilting".into(),
            "stipple" => "Background quilting".into(),
            _ => "Outline".into(),
        });
        let mut e = Embroidery::new(stitch);
        e.thread = Some(Thread::new(color, ""));
        // a knockdown, emboss or background fill keeps its own angle: picked automatically, its
        // rows would swirl round the art's outline
        if filled {
            e.auto_direction = false;
        }
        if kind == "satin" {
            e.width = Some(width);
        }
        n.embroidery = Some(Box::new(e));
        let id = n.id;
        // knockdown and emboss stitches go down first, under the design
        d.insert(parent, if filled { 0 } else { usize::MAX }, n)?;
        sel.set([id]);
        Ok(id)
    })?;
    Ok(json!({ "id": id.0, "pieces": pieces }))
}

/// A4 portrait (points).
pub(super) const SHEET: (f64, f64) = (595.0, 842.0);

/// A line of `text` at `(x, y)` (its baseline; points), `size` points high, in `color`.
pub(super) fn sheet_text(doc: &mut Document, x: f64, y: f64, text: &str, size: f64, color: Color) -> Result<()> {
    let style = vectorcraft_doc::text::CharStyle {
        font_family: vectorcraft_text::FALLBACK_FAMILY.to_string(),
        font_style: "Regular".into(),
        size,
        ..vectorcraft_doc::text::CharStyle::default()
    };
    let t = vectorcraft_doc::text::TextObject::point(kurbo::Point::new(x, y), text, style);
    let mut n = vectorcraft_doc::Node::new(doc.alloc_id(), NodeKind::Text(Box::new(t)));
    n.appearance = Appearance::basic(Paint::solid(color), Paint::None, 0.0).into();
    let layer = doc.layers.first().map(|l| l.id);
    doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    Ok(())
}

/// The usual size and place of a placement (typical values; garments and customers vary).
fn placement_text(id: &str) -> Option<&'static str> {
    Some(match id {
        "left-chest" => "Left chest: usually 75–100 mm wide, its middle 180–230 mm below the shoulder seam and 75–100 mm left of the centre front",
        "right-chest" => "Right chest: usually 75–100 mm wide, its middle 180–230 mm below the shoulder seam and 75–100 mm right of the centre front",
        "full-front" => "Full front: usually up to 250–300 mm wide, its top 75–100 mm below the neckline, on the centre front",
        "full-back" => "Full back: usually up to 300–350 mm wide, its top 100–125 mm below the collar, on the centre back",
        "cap-front" => "Cap front: usually 50–60 mm high and up to 110–130 mm wide, centred on the front seam, its bottom 12–20 mm above the brim",
        "sleeve" => "Sleeve: usually 75–100 mm wide, centred on the sleeve 75–100 mm below the shoulder seam or above the hem",
        "yoke" => "Back yoke: usually 75–125 mm wide, centred 25–50 mm below the collar",
        "pocket" => "Pocket: inside the pocket, at least 6 mm clear of its seams",
        _ => return None,
    })
}

/// Straight lines (points, page space) in `color`, `width` wide.
pub(super) fn sheet_lines(doc: &mut Document, lines: &[Vec<(f64, f64)>], color: Color, width: f64) -> Result<()> {
    use vectorcraft_geom::path::{PathData, SubPath};
    let subpaths: Vec<SubPath> = lines
        .iter()
        .filter(|l| l.len() >= 2)
        .map(|l| SubPath::polyline(&l.iter().map(|(x, y)| kurbo::Point::new(*x, *y)).collect::<Vec<_>>(), false))
        .collect();
    let n = vectorcraft_doc::Node::path(doc.alloc_id(), PathData::new(subpaths), Appearance::basic(Paint::None, Paint::solid(color), width));
    let layer = doc.layers.first().map(|l| l.id);
    doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    Ok(())
}

/// A barcode module's width (points) for one `total` modules wide: 0.75 pt (a common minimum
/// for laser printers), narrower to keep it within 230 pt.
fn barcode_module(total: f64) -> f64 {
    (230.0 / total.max(1.0)).min(0.75)
}

/// A barcode's bars (`widths`, bar first, in modules) ending at `right`, from `top` 44 pt down.
fn sheet_barcode(doc: &mut Document, widths: &[u8], right: f64, top: f64, color: Color) -> Result<()> {
    use vectorcraft_geom::path::{PathData, SubPath};
    let total: f64 = widths.iter().map(|w| f64::from(*w)).sum();
    let m = barcode_module(total);
    let mut x = right - total * m;
    let mut bars = Vec::new();
    for (i, w) in widths.iter().enumerate() {
        let w = f64::from(*w) * m;
        if i % 2 == 0 {
            let corners =
                [kurbo::Point::new(x, top), kurbo::Point::new(x + w, top), kurbo::Point::new(x + w, top + 44.0), kurbo::Point::new(x, top + 44.0)];
            bars.push(SubPath::polyline(&corners, true));
        }
        x += w;
    }
    let mut n = vectorcraft_doc::Node::path(doc.alloc_id(), PathData::new(bars), Appearance::basic(Paint::solid(color), Paint::None, 0.0));
    n.name = Some("Barcode".into());
    let layer = doc.layers.first().map(|l| l.id);
    doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    Ok(())
}

/// `text` in lines of at most `width` characters, broken between words (a longer word cut) and
/// at its own line breaks.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for para in text.lines() {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let mut word: Vec<char> = word.chars().collect();
            while word.len() > width {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                let rest = word.split_off(width);
                out.push(word.into_iter().collect());
                word = rest;
            }
            let n = line.chars().count();
            if n > 0 && n + 1 + word.len() > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.extend(word);
        }
        out.push(line);
    }
    out
}

/// The stitches of `pattern` as runs per colour block (millimetres), broken at jumps, trims and stops.
pub(super) fn stitch_runs(pattern: &sf_core::Pattern) -> Vec<Vec<Vec<sf_core::Point>>> {
    let mut blocks: Vec<Vec<Vec<sf_core::Point>>> = vec![Vec::new()];
    let (mut run, mut at) = (Vec::<sf_core::Point>::new(), sf_core::Point::ZERO);
    let flush = |run: &mut Vec<sf_core::Point>, blocks: &mut Vec<Vec<Vec<sf_core::Point>>>| {
        let r = std::mem::take(run);
        if r.len() >= 2
            && let Some(b) = blocks.last_mut()
        {
            b.push(r);
        }
    };
    for st in &pattern.stitches {
        let p = st.pos();
        match st.cmd {
            sf_core::Cmd::Stitch => {
                if run.is_empty() {
                    run.push(at);
                }
                run.push(p);
                at = p;
            }
            sf_core::Cmd::Jump => {
                flush(&mut run, &mut blocks);
                at = p;
            }
            sf_core::Cmd::ColorChange => {
                flush(&mut run, &mut blocks);
                blocks.push(Vec::new());
            }
            _ => flush(&mut run, &mut blocks),
        }
    }
    flush(&mut run, &mut blocks);
    blocks
}

/// The worksheet's heading for its kind.
fn worksheet_heading(approval: bool, template: bool, full: bool) -> &'static str {
    if approval {
        "Approval sheet"
    } else if template {
        "Placement template"
    } else if full {
        "Pattern"
    } else {
        "Production worksheet"
    }
}

/// The job's barcode (its text and bar widths): the design's name unless other text is given.
fn worksheet_barcode(p: &Value, shown: bool, title: &str) -> Result<Option<(String, Vec<u8>)>> {
    const CMD: &str = "embroidery.worksheet";
    Ok(match str_param(p, "barcode").unwrap_or(if shown { "code128" } else { "none" }) {
        "none" => None,
        name => {
            let kind = sf_core::barcode::Symbology::from_name(name).ok_or_else(|| bad(CMD, "`barcode` is code128, code39 or none"))?;
            match str_param(p, "barcodeText").map(str::trim).filter(|t| !t.is_empty()) {
                Some(text) => {
                    let widths = sf_core::barcode::encode(kind, text)
                        .ok_or_else(|| bad(CMD, "`barcodeText` is 1 to 48 characters the code has (Code 39: digits, letters, space and -.$/+%)"))?;
                    Some((text.to_string(), widths))
                }
                // a name the code can't carry: no barcode rather than no worksheet
                None => sf_core::barcode::encode(kind, title).map(|w| (title.to_string(), w)),
            }
        }
    })
}

/// Draws the design's stitches on the sheet in their thread colours, at `(x, y, scale)`.
fn worksheet_design(doc: &mut Document, pattern: &sf_core::Pattern, (ox, oy, k): (f64, f64, f64)) -> Result<()> {
    use vectorcraft_geom::path::{Anchor, AnchorKind, PathData, SubPath};
    let b = pattern.bounds();
    let layer = doc.layers.first().map(|l| l.id);
    for (i, runs) in stitch_runs(pattern).iter().enumerate().filter(|(_, r)| !r.is_empty()) {
        let to_pt = |q: &sf_core::Point| kurbo::Point::new(ox + (q.x - b.min.x) * k, oy + (q.y - b.min.y) * k);
        let subpaths = runs
            .iter()
            .map(|r| SubPath::new(r.iter().map(to_pt).map(|p| Anchor { p, h_in: p, h_out: p, kind: AnchorKind::Corner }).collect(), false))
            .collect();
        let c = pattern.thread(i).color;
        let kind =
            NodeKind::Path { path: PathData { subpaths }, rule: vectorcraft_geom::FillRule::NonZero, live: None, clipping: false, guide: false };
        let mut n = vectorcraft_doc::Node::new(doc.alloc_id(), kind);
        n.appearance = Appearance::basic(Paint::None, Paint::solid(Color::rgb8(c.r, c.g, c.b)), 0.5).into();
        doc.insert(layer, usize::MAX, n).map_err(|e| EngineError::Other(e.to_string()))?;
    }
    Ok(())
}

/// A placement template's marks: lines through the design's middle to the page's margins, which
/// way is up, and a bar to check the print is life size.
fn worksheet_template_marks(doc: &mut Document, (cx, cy): (f64, f64), (pw, ph): (f64, f64)) -> Result<()> {
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let grey = Color::rgb8(0x66, 0x66, 0x66);
    let grey_line = Color::rgb8(0x99, 0x99, 0x99);
    sheet_lines(doc, &[vec![(20.0, cy), (pw - 20.0, cy)], vec![(cx, 104.0), (cx, ph - 96.0)]], grey_line, 0.5)?;
    sheet_lines(doc, &[vec![(cx - 6.0, 116.0), (cx, 106.0), (cx + 6.0, 116.0)]], black, 1.0)?;
    sheet_text(doc, cx + 9.0, 116.0, "Up", 9.0, black)?;
    let bar = 100.0 * PT_PER_MM;
    let y = ph - 84.0;
    sheet_lines(
        doc,
        &[vec![(40.0, y), (40.0 + bar, y)], vec![(40.0, y - 5.0), (40.0, y + 5.0)], vec![(40.0 + bar, y - 5.0), (40.0 + bar, y + 5.0)]],
        black,
        1.0,
    )?;
    sheet_text(doc, 40.0, y + 18.0, "100 mm: print at 100 % (no fitting to the page) and check this bar", 8.0, grey)
}

/// The worksheet's figures: size, stitches, thread, time, hoop, stabiliser and cost.
fn worksheet_figures(stats: &sf_core::stats::PatternStats, cfg: &DocEmbroidery, shown: f64) -> Vec<String> {
    let st = &cfg.settings;
    let mut lines = vec![
        format!("Size: {:.1} × {:.1} mm (shown at {shown} %)", stats.width_mm, stats.height_mm),
        format!("Stitches: {}   Colours: {}   Colour changes: {}   Trims: {}", stats.stitches, stats.colors.len(), stats.color_changes, stats.trims),
        format!(
            "Thread: {:.1} m top, {:.1} m bobbin   Sewing time: {} at {} stitches a minute",
            stats.thread_m,
            stats.bobbin_m,
            stats.time_string(),
            st.machine_speed.round()
        ),
        format!(
            "Hoop: {}   Fabric: {}   Machine: {}",
            cfg.hoop.as_ref().map_or("none", |h| h.name.as_str()),
            if cfg.fabric.is_empty() { "none" } else { cfg.fabric.as_str() },
            if cfg.machine.is_empty() { "none" } else { cfg.machine.as_str() }
        ),
    ];
    // what to hoop under the fabric
    if let Some(f) = cfg.fabric(&cfg.fabric) {
        lines.push(format!("Stabiliser: {}", sf_core::fabrics::stabilizer_advice(&f)));
    }
    if let Some(m) = &cfg.cost {
        let e = m.estimate(stats, 1);
        lines.push(format!(
            "Cost: {} a piece (stitches {}, machine time {}, thread {}, set-up {})",
            m.money(e.per_piece),
            m.money(e.stitches),
            m.money(e.time),
            m.money(e.thread),
            m.money(e.setup)
        ));
    }
    lines
}

/// Lists the design's colorways from `y`, a line or two each; the `y` below them.
fn worksheet_colorways(doc: &mut Document, cfg: &DocEmbroidery, own: &[Thread], pw: f64, mut y: f64) -> Result<f64> {
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let width = ((pw - 98.0) / 4.8).max(20.0) as usize;
    y += 8.0;
    sheet_text(doc, 40.0, y, "Colorways", 11.0, black)?;
    y += 16.0;
    let schemes = std::iter::once(("The design's own".to_string(), own.to_vec()))
        .chain(cfg.colorways.iter().enumerate().map(|(i, c)| (c.name.clone(), cfg.threads_of(i + 1, own))));
    let mut printed = 0;
    for (k, (name, threads)) in schemes.enumerate() {
        let mark = if k == cfg.colorway { " (in use)" } else { "" };
        let list: Vec<String> = threads.iter().enumerate().map(|(i, t)| format!("{}. {}", i + 1, t.label())).collect();
        for line in wrap_text(&format!("{name}{mark}: {}", list.join(", ")), width).iter().take(3) {
            if printed >= 14 {
                break;
            }
            sheet_text(doc, 58.0, y, line, 9.0, black)?;
            y += 12.0;
            printed += 1;
        }
    }
    Ok(y)
}

fn worksheet(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.worksheet";
    let kind = str_param(p, "kind").unwrap_or("production");
    let (approval, full) = match kind {
        "production" => (false, false),
        "approval" => (true, false),
        "pattern" | "template" => (false, true),
        _ => return Err(bad(CMD, "`kind` is production, approval, pattern or template")),
    };
    let template = kind == "template";
    let placement = match str_param(p, "placement") {
        None => None,
        Some(id) => Some(
            placement_text(id)
                .ok_or_else(|| bad(CMD, "`placement` is left-chest, right-chest, full-front, full-back, cap-front, sleeve, yoke or pocket"))?,
        ),
    };
    let (pw, ph) = match str_param(p, "page").unwrap_or("a4") {
        "a4" | "A4" => SHEET,
        "letter" | "Letter" => (612.0, 792.0),
        _ => return Err(bad(CMD, "`page` is a4 or letter")),
    };
    let field = |key: &str| str_param(p, key).map(|t| t.trim().chars().take(2000).collect::<String>()).unwrap_or_default();
    let (customer, notes) = (field("customer"), field("notes"));
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let stats = plan.stats.clone().ok_or_else(|| bad(CMD, "nothing is stitched yet: give objects a stitch type first"))?;
    let pattern = plan.pattern().clone();
    let (title, cfg) = {
        let d = &s.doc()?.doc;
        (super::fileio::file_stem(&d.title), d.embroidery.as_deref().cloned().unwrap_or_default())
    };
    // the design's own notes unless others are given
    let notes = if notes.is_empty() { cfg.notes.trim().chars().take(2000).collect() } else { notes };
    // the shop's setup: sections left out (as asked, else as set up), its name, details and logo
    let sheet = s.prefs.embroidery_sheet_setup.clone();
    let hidden: Vec<String> = match p.get("hide").and_then(Value::as_array) {
        Some(a) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        None => sheet.hide.clone(),
    };
    let shows = |section: &str| !hidden.iter().any(|h| h == section);
    // the job's barcode: its name unless other text is given
    let code = worksheet_barcode(p, shows("barcode"), &title)?;
    let black = Color::rgb8(0x1a, 0x1a, 0x1a);
    let grey = Color::rgb8(0x66, 0x66, 0x66);
    let mut doc = Document::new(pw, ph);
    doc.title = format!("{title} worksheet");
    // heading
    sheet_text(&mut doc, 40.0, 56.0, worksheet_heading(approval, template, full), 20.0, black)?;
    sheet_text(&mut doc, 40.0, 78.0, &title, 13.0, black)?;
    let t = sf_formats::Timestamp::now();
    let mut sub = format!("{:04}-{:02}-{:02}", t.year, t.month, t.day);
    if approval && !customer.is_empty() {
        sub = format!("{sub}   Customer: {customer}");
    }
    if let Some(cw) = cfg.active_colorway() {
        sub = format!("{sub}   Colorway: {}", cw.name);
    }
    sheet_text(&mut doc, 40.0, 96.0, &sub, 9.0, grey)?;
    if let Some((text, widths)) = &code {
        sheet_barcode(&mut doc, widths, pw - 40.0, 36.0, black)?;
        let total: f64 = widths.iter().map(|w| f64::from(*w)).sum();
        let x = pw - 40.0 - total * barcode_module(total);
        sheet_text(&mut doc, x, 92.0, text, 8.0, black)?;
    }
    // the design, life size when it fits the box, in its thread colours
    let b = pattern.bounds();
    // the box the design is drawn in: most of the page for a pattern
    let (bw, bh) = if template {
        // room below for the check bar
        (pw - 80.0, ph - 210.0)
    } else if full {
        (pw - 80.0, ph - 170.0)
    } else {
        (pw - 80.0, 380.0)
    };
    let fit = (bw / b.width().max(1e-6)).min(bh / b.height().max(1e-6));
    let k = fit.min(PT_PER_MM);
    if !(k.is_finite() && k > 0.0) {
        return Err(bad(CMD, "the design has no size"));
    }
    if template && fit < PT_PER_MM - 1e-9 {
        return Err(bad(CMD, "the design is larger than the page at life size: print a pattern (scaled) instead"));
    }
    let (ox, oy) = (40.0 + (bw - b.width() * k) / 2.0, 110.0 + (bh - b.height() * k) / 2.0);
    let layer = doc.layers.first().map(|l| l.id);
    worksheet_design(&mut doc, &pattern, (ox, oy, k))?;
    // the figures (a pattern: one line under the design)
    let shown = (k / PT_PER_MM * 100.0).round();
    if template {
        worksheet_template_marks(&mut doc, (ox + b.width() * k / 2.0, oy + b.height() * k / 2.0), (pw, ph))?;
    }
    if let Some(text) = placement {
        sheet_text(&mut doc, 40.0, if full { ph - 24.0 } else { 104.0 }, text, 8.0, grey)?;
    }
    if full {
        let colours = stats.colors.len();
        let line = format!(
            "{:.1} × {:.1} mm (shown at {shown} %)   {} stitches   {colours} {}",
            stats.width_mm,
            stats.height_mm,
            stats.stitches,
            if colours == 1 { "colour" } else { "colours" }
        );
        sheet_text(&mut doc, 40.0, ph - 40.0, &line, 9.0, grey)?;
        s.add_document(doc, None);
        return Ok(json!({ "kind": kind, "colors": stats.colors.len(), "scale": shown }));
    }
    let mut y = 512.0;
    let lines = worksheet_figures(&stats, &cfg, shown);
    if shows("figures") {
        for l in &lines {
            sheet_text(&mut doc, 40.0, y, l, 10.0, black)?;
            y += 15.0;
        }
    }
    // the colours in sewing order
    let room = if shows("colors") { ((ph - 90.0 - y - 24.0) / 15.0).max(1.0) as usize } else { 0 };
    if room > 0 {
        y += 8.0;
        sheet_text(&mut doc, 40.0, y, "Colours in sewing order", 11.0, black)?;
        y += 16.0;
    }
    for (i, c) in stats.colors.iter().enumerate().take(room) {
        let rgb = c.thread.color;
        let corners = [(40.0, y - 9.0), (50.0, y - 9.0), (50.0, y + 1.0), (40.0, y + 1.0)].map(|(x, y)| kurbo::Point::new(x, y)).to_vec();
        let square = vectorcraft_geom::path::PathData::new(vec![vectorcraft_geom::path::SubPath::polyline(&corners, true)]);
        let swatch = vectorcraft_doc::Node::path(
            doc.alloc_id(),
            square,
            Appearance::basic(Paint::solid(Color::rgb8(rgb.r, rgb.g, rgb.b)), Paint::solid(grey), 0.5),
        );
        doc.insert(layer, usize::MAX, swatch).map_err(|e| EngineError::Other(e.to_string()))?;
        let name = c.thread.label();
        let brand = if c.thread.chart.is_empty() { String::new() } else { format!(" ({})", c.thread.chart) };
        sheet_text(&mut doc, 58.0, y, &format!("{}. {name}{brand}   {} stitches, {:.1} m", i + 1, c.stitches, c.thread_m), 9.0, black)?;
        y += 15.0;
    }
    if room > 0 && stats.colors.len() > room {
        sheet_text(&mut doc, 58.0, y, &format!("… and {} more", stats.colors.len() - room), 9.0, grey)?;
        y += 15.0;
    }
    // the design's colorways, a line or two each: its threads colour by colour
    if !cfg.colorways.is_empty() && p.get("colorways").and_then(Value::as_bool).unwrap_or(true) && shows("colorways") {
        y = worksheet_colorways(&mut doc, &cfg, &plan.own_threads, pw, y)?;
    }
    if !notes.is_empty() && shows("notes") {
        // wrapped to the page, eight lines at most
        let width = ((pw - 80.0) / 4.8).max(20.0) as usize;
        let lines = wrap_text(&format!("Notes: {notes}"), width);
        for (i, line) in lines.iter().take(8).enumerate() {
            sheet_text(&mut doc, 40.0, y + 8.0 + 12.0 * i as f64, line, 9.0, black)?;
        }
    }
    if approval {
        sheet_text(&mut doc, 40.0, ph - 40.0, "Approved by: ______________________________   Date: ______________", 10.0, black)?;
    }
    // the shop in the footer, its logo at the right
    if !sheet.company.is_empty() || !sheet.details.is_empty() {
        let line = [sheet.company.as_str(), sheet.details.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join("   ");
        sheet_text(&mut doc, 40.0, ph - 18.0, &line, 8.0, grey)?;
    }
    s.add_document(doc, None);
    if !sheet.logo.is_empty() {
        // a logo that can't be read leaves the worksheet without it, not unmade
        let _ = s.execute("file.place", &json!({ "path": sheet.logo, "rect": [pw - 150.0, ph - 60.0, 110.0, 44.0] }));
        let _ = s.execute("select.none", &json!({}));
    }
    Ok(json!({ "kind": kind, "colors": stats.colors.len(), "scale": shown }))
}

/// A cost estimate as the commands give it.
fn cost_json(m: &sf_core::attr::CostModel, e: &sf_core::attr::CostEstimate, quantity: u32) -> Value {
    let r = |v: f64| (v * 100.0).round() / 100.0;
    json!({
        "currency": m.currency, "quantity": quantity, "perPiece": r(e.per_piece), "total": r(e.total),
        "stitches": r(e.stitches), "time": r(e.time), "thread": r(e.thread), "setup": r(e.setup),
        "text": format!("{} a piece", m.money(e.per_piece)),
    })
}

fn cost(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.cost";
    let quantity = match p.get("quantity") {
        None | Some(Value::Null) => 1,
        Some(v) => v
            .as_u64()
            .filter(|q| (1..=1_000_000).contains(q))
            .and_then(|q| u32::try_from(q).ok())
            .ok_or_else(|| bad(CMD, "`quantity` is 1 to 1000000"))?,
    };
    let model = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.cost.clone()).unwrap_or_default();
    let plan = s.embroidery_plan().ok_or_else(|| bad(CMD, "no document"))?;
    let stats = plan.stats.as_ref().ok_or_else(|| bad(CMD, "nothing is stitched yet: give objects a stitch type first"))?;
    Ok(cost_json(&model, &model.estimate(stats, quantity), quantity))
}

fn info(s: &mut Session, _p: &Value) -> Result<Value> {
    let plan = s.embroidery_plan().ok_or_else(|| bad("embroidery.info", "no document"))?;
    let cfg = s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default();
    // the built-in fabrics, then the design's own
    let builtin = sf_core::fabrics::fabrics();
    let fabrics: Vec<&str> = builtin.iter().chain(&cfg.fabrics).map(|f| f.name.as_str()).collect();
    let details: Vec<Value> = builtin
        .iter()
        .map(|f| super::embroidery_props::fabric_json(f, false))
        .chain(cfg.fabrics.iter().map(|f| super::embroidery_props::fabric_json(f, true)))
        .collect();
    let custom = &s.prefs.embroidery_custom_hoops;
    let hoops: Vec<Value> = super::embroidery_hoops::all_hoops(s)
        .iter()
        .map(|h| json!({"name": h.name, "width": h.width, "height": h.height, "custom": custom.iter().any(|c| c.name == h.name)}))
        .collect();
    let charts: Vec<String> = super::embroidery_charts::all_charts(s).into_iter().map(|c| c.name).collect();
    let saved = &s.prefs.embroidery_machines;
    let machines: Vec<Value> = sf_core::machines::profiles(saved).iter().map(|m| super::embroidery_machines::machine_json(m, saved)).collect();
    let machine_format = cfg.machine_file_format(saved);
    let st = &cfg.settings;
    let settings = json!({
        "maxStitch": st.max_stitch, "maxJump": st.max_jump, "minStitch": st.min_stitch, "trimThreshold": st.trim_threshold, "overlapTouching": st.overlap_touching, "hideTravel": st.hide_travel,
        "tieIn": st.tie_in, "tieOff": st.tie_off, "tieLength": st.tie_length, "tieStyle": st.tie_style.id(), "machineSpeed": st.machine_speed,
        "trimSeconds": st.trim_seconds, "colorChangeSeconds": st.color_change_seconds,
        "basting": st.basting.map(|b| json!({ "hoop": b.hoop, "margin": b.margin, "length": b.length })),
        "hoopPosition": st.hoop_offset,
        "registration": json!({ "shape": st.registration.shape.id(), "size": st.registration.size, "perOverlap": st.registration.per_overlap, "color": st.registration.color.to_hex() }),
        "start": st.start.map(|q| q.id()),
        "end": st.end.map(|q| q.id()),
        "cost": cfg.cost.as_ref().map(|m| json!({
            "perThousand": m.per_thousand, "perHour": m.per_hour, "threadPerKm": m.thread_per_km,
            "setup": m.setup, "heads": m.heads, "currency": m.currency,
        })),
        "checks": {
            "denseLayers": cfg.checks.dense_layers, "longStitch": cfg.checks.long_stitch,
            "smallLettering": cfg.checks.small_lettering, "tinyObject": cfg.checks.tiny_object,
            "garment": cfg.checks.garment, "shortStitch": cfg.checks.short_stitch,
            "narrowSatin": cfg.checks.narrow_satin, "shortFill": cfg.checks.short_fill,
        },
    });
    let fits = cfg.hoop.as_ref().map(|h| {
        let b = plan.pattern().bounds();
        // centred, or where the design is placed in the hoop
        let [ox, oy] = cfg.settings.hoop_offset.unwrap_or([0.0, 0.0]);
        let placed = sf_core::BBox::new(
            sf_core::Point::new(ox - b.width() / 2.0, oy - b.height() / 2.0),
            sf_core::Point::new(ox + b.width() / 2.0, oy + b.height() / 2.0),
        );
        h.fits(&placed)
    });
    let grade = {
        let d = &s.doc()?.doc;
        let mut types = Vec::new();
        d.walk(|n| {
            if let Some(e) = n.embroidery.as_deref() {
                types.push(e.stitch.clone());
            }
        });
        sf_core::attr::DesignGrade::of(d.embroidery.as_deref(), types.iter())
    };
    let Some(st) = &plan.stats else {
        return Ok(
            json!({"objects": 0, "stitches": 0, "fabrics": fabrics, "fabricDetails": details, "hoops": hoops, "charts": charts, "machines": machines, "hoop": cfg.hoop.map(|h| h.name), "fabric": cfg.fabric, "machine": cfg.machine, "machineFormat": machine_format, "settings": settings, "notes": cfg.notes, "grade": grade.letter(), "openedFrom": cfg.opened_from}),
        );
    };
    Ok(json!({
        "grade": grade.letter(),
        "openedFrom": cfg.opened_from,
        "objects": plan.design.objects.len(),
        "stitches": st.stitches,
        "jumps": st.jumps,
        "trims": st.trims,
        "colorChanges": st.color_changes,
        "stops": st.stops,
        "widthMm": st.width_mm,
        "heightMm": st.height_mm,
        "threadM": st.thread_m,
        "bobbinM": st.bobbin_m,
        "seconds": st.seconds,
        "time": st.time_string(),
        "maxStitchMm": st.max_stitch_mm,
        "colors": st.colors.iter().map(|c| json!({"color": c.thread.color.to_hex(), "name": c.thread.label(), "stitches": c.stitches, "threadM": c.thread_m})).collect::<Vec<_>>(),
        "hoop": cfg.hoop.as_ref().map(|h| h.name.clone()),
        "hoopSize": cfg.hoop.as_ref().map(|h| [h.width, h.height]),
        "fits": fits,
        "fabric": cfg.fabric,
        "fabrics": fabrics,
        "fabricDetails": details,
        "hoops": hoops,
        "charts": charts,
        "machine": cfg.machine,
        "machineFormat": machine_format,
        "machines": machines,
        "settings": settings,
        "notes": cfg.notes,
        "costEstimate": cfg.cost.as_ref().map(|m| cost_json(m, &m.estimate(st, 1), 1)),
    }))
}

fn plan(s: &mut Session, p: &Value) -> Result<Value> {
    let plan = s.embroidery_plan().ok_or_else(|| bad("embroidery.plan", "no document"))?;
    let limit = p.get("limit").and_then(Value::as_u64).map_or(usize::MAX, |l| l as usize);
    let pat = plan.pattern();
    let stitches: Vec<Value> = pat
        .stitches
        .iter()
        .take(limit)
        .map(|st| {
            let q = vectorcraft_embroidery::pt(st.pos());
            let cmd = match st.cmd {
                sf_core::Cmd::Stitch => "stitch",
                sf_core::Cmd::Jump => "jump",
                sf_core::Cmd::Trim => "trim",
                sf_core::Cmd::ColorChange => "color",
                sf_core::Cmd::Stop => "stop",
                sf_core::Cmd::End => "end",
            };
            json!([q.x, q.y, cmd])
        })
        .collect();
    Ok(json!({
        "threads": pat.threads.iter().map(|t| t.color.to_hex()).collect::<Vec<_>>(),
        "stitches": stitches,
        "nodes": plan.nodes.iter().map(|n| n.0).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod wrap_tests {
    #[test]
    fn notes_wrap_between_words_and_at_line_breaks() {
        assert_eq!(super::wrap_text("left chest, 9 cm wide\nwhite thread", 12), ["left chest,", "9 cm wide", "white thread"]);
        assert_eq!(super::wrap_text("abcdefghij xy", 4), ["abcd", "efgh", "ij", "xy"]);
        assert_eq!(super::wrap_text("", 10), Vec::<String>::new());
    }
}
