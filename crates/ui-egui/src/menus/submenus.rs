use super::{Item, Sep, Stamp, c, cp, json, sub, todo};

/// Type → Size presets.
pub(super) const TYPE_SIZES: [(&str, u32); 14] = [
    ("6 pt", 6),
    ("8 pt", 8),
    ("9 pt", 9),
    ("10 pt", 10),
    ("11 pt", 11),
    ("12 pt", 12),
    ("14 pt", 14),
    ("18 pt", 18),
    ("24 pt", 24),
    ("30 pt", 30),
    ("36 pt", 36),
    ("48 pt", 48),
    ("60 pt", 60),
    ("72 pt", 72),
];

/// Type → Type on a Path effects (label, `type.pathOptions` effect), also the choices of its
/// Options dialog.
pub(crate) const PATH_EFFECTS: &[(&str, &str)] =
    &[("Rainbow", "rainbow"), ("Skew", "skew"), ("3D Ribbon", "3dRibbon"), ("Stair Step", "stairStep"), ("Gravity", "gravity")];

pub(super) const INSERT_SPECIAL: &[(&str, &str)] = &[
    ("Bullet", "bullet"),
    ("Copyright Symbol", "copyright"),
    ("Ellipsis", "ellipsis"),
    ("Paragraph Symbol", "paragraph"),
    ("Registered Trademark Symbol", "registered"),
    ("Section Symbol", "section"),
    ("Trademark Symbol", "trademark"),
    ("Em Dash", "emDash"),
    ("En Dash", "enDash"),
    ("Discretionary Hyphen", "discretionaryHyphen"),
    ("Nonbreaking Hyphen", "nonBreakingHyphen"),
    ("Double Left Quotation Marks", "doubleLeftQuote"),
    ("Double Right Quotation Marks", "doubleRightQuote"),
    ("Single Left Quotation Mark", "singleLeftQuote"),
    ("Single Right Quotation Mark", "singleRightQuote"),
];

pub(super) const INSERT_WHITESPACE: &[(&str, &str)] = &[
    ("Em Space", "emSpace"),
    ("En Space", "enSpace"),
    ("Hair Space", "hairSpace"),
    ("Sixth Space", "sixthSpace"),
    ("Thin Space", "thinSpace"),
    ("Nonbreaking Space", "nonBreakingSpace"),
    ("Figure Space", "figureSpace"),
    ("Punctuation Space", "punctuationSpace"),
    ("Third Space", "thirdSpace"),
    ("Quarter Space", "quarterSpace"),
];

pub(super) const INSERT_BREAK: &[(&str, &str)] = &[("Forced Line Break", "forcedLineBreak"), ("Tab", "tab")];

pub(super) fn insert_items(list: &[(&'static str, &'static str)]) -> Vec<Item> {
    list.iter().map(|(l, c)| cp(l, "type.insert", json!({ "char": c }))).collect()
}

/// Type → Font: one item per available family, the installed fonts included. Built again only when
/// the fonts or Show Font Names in English change (the menu tree is built every frame); labels are
/// interned once each, so rebuilding doesn't allocate forever. The command still sets the English
/// family name ([`vectorcraft_text::FontDb::canonical`]).
pub(super) fn font_items(english_names: bool) -> Vec<Item> {
    static NAMES: std::sync::Mutex<std::collections::BTreeSet<&'static str>> = std::sync::Mutex::new(std::collections::BTreeSet::new());
    static ITEMS: std::sync::Mutex<(u64, bool, Vec<Item>)> = std::sync::Mutex::new((u64::MAX, true, Vec::new()));
    let db = vectorcraft_text::FontDb::global();
    // Read first: fonts that load meanwhile make the next frame build the list again.
    let generation = db.generation();
    let fams = db.menu_family_list();
    let (Ok(mut names), Ok(mut items)) = (NAMES.lock(), ITEMS.lock()) else { return vec![] };
    if items.0 != generation || items.1 != english_names {
        let list = fams
            .iter()
            .map(|f| {
                let english = f.as_str();
                let shown = db.family_display_name(english, english_names);
                let label: &'static str = match names.get(shown.as_str()) {
                    Some(n) => n,
                    None => {
                        let n: &'static str = Box::leak(shown.into_boxed_str());
                        names.insert(n);
                        n
                    }
                };
                let font: &'static str = match names.get(english) {
                    Some(n) => n,
                    None => {
                        let n: &'static str = Box::leak(english.to_string().into_boxed_str());
                        names.insert(n);
                        n
                    }
                };
                cp(label, "text.setStyle", json!({ "font": font }))
            })
            .collect();
        *items = (generation, english_names, list);
    }
    items.2.clone()
}

/// The raster effects' submenus of the Effect menu (the Photoshop-style effects, below the
/// vector effects): (submenu, the catalogue's menu path of its effects).
/// The raster Stylize submenu shares its path with the vector one and lists only the effects at
/// that path that [`vectorcraft_effects::PIXEL_EFFECTS`] names.
const RASTER_MENUS: [(&str, &[&str]); 10] = [
    ("Artistic", &["Effect", "Artistic"]),
    ("Blur", &["Effect", "Blur"]),
    ("Brush Strokes", &["Effect", "Brush Strokes"]),
    ("Distort", &["Effect", "Distort"]),
    ("Pixelate", &["Effect", "Pixelate"]),
    ("Sharpen", &["Effect", "Sharpen"]),
    ("Sketch", &["Effect", "Sketch"]),
    ("Stylize", &["Effect", "Stylize"]),
    ("Texture", &["Effect", "Texture"]),
    ("Video", &["Effect", "Video"]),
];

/// The Effect menu, built from the effects catalogue (vector effects), plus raster effects.
pub(super) fn effect_menu() -> Vec<Item> {
    let cat = vectorcraft_effects::effect_catalog();
    let mut out = vec![
        c("Apply Last Effect", "effect.applyLast"),
        c("Last Effect…", "effect.last"),
        Sep,
        c("Document Raster Effects Settings…", "document.rasterEffectsSettings"),
        Sep,
        Item::Header("Vector Effects"),
    ];
    // Submenus (and Crop Marks and Rasterize…, items of their own) in the reference app's order.
    let order = [
        "3D and Materials",
        "Color Adjustments",
        "Convert to Shape",
        "Crop Marks",
        "Distort & Transform",
        "Path",
        "Pathfinder",
        "Rasterize…",
        "Stylize",
        "SVG Filters",
        "Warp",
    ];
    let top_level = |e: &vectorcraft_effects::EffectInfo| e.menu == ["Effect"] && order.contains(&e.label);
    // An effect goes in the vector submenu its path names unless it is a Photoshop-style effect,
    // which goes in the raster submenu of the same name when `RASTER_MENUS` has one (Stylize ›
    // Glowing Edges).
    let in_vector_menu =
        |e: &vectorcraft_effects::EffectInfo| order.iter().any(|s| e.menu == ["Effect", *s]) && !vectorcraft_effects::PIXEL_EFFECTS.contains(&e.id);
    let item = |e: &vectorcraft_effects::EffectInfo| match e.defaults.as_object().is_some_and(|o| o.is_empty()) {
        // No options (Effect → Pathfinder, Crop Marks): apply directly, like Illustrator.
        true => Item::Cmd(e.label, "effect.apply", json!({ "effect": e.id })),
        false => Item::Cmd(e.label, "effect.dialog", json!({ "effect": e.id })),
    };
    // The effects whose catalogue menu path is `path`.
    // `vector` picks the ones for the vector submenu or the ones for the raster submenu.
    let items_at =
        |path: &[&str], vector: bool| -> Vec<Item> { cat.iter().filter(|e| e.menu == path && in_vector_menu(e) == vector).map(item).collect() };
    for sub_name in order {
        if let Some(e) = cat.iter().find(|e| top_level(e) && e.label == sub_name) {
            out.push(item(e));
            continue;
        }
        let items = items_at(&["Effect", sub_name], true);
        if items.is_empty() {
            let placeholder = match sub_name {
                "3D and Materials" => vec![todo("Extrude & Bevel…"), todo("Revolve…"), todo("Inflate…"), todo("Rotate…"), todo("Materials…")],
                "SVG Filters" => vec![todo("Apply SVG Filter…")],
                _ => continue,
            };
            out.push(sub(sub_name, placeholder));
        } else {
            let mut items = items;
            if sub_name == "3D and Materials" {
                // In the panel's order of 3D types, then Materials (not built yet).
                let rank = |label: &str| ["Extrude & Bevel…", "Revolve…", "Inflate…", "Rotate…"].iter().position(|l| *l == label).unwrap_or(9);
                items.sort_by_key(|i| match i {
                    Item::Cmd(label, ..) => rank(label),
                    _ => 9,
                });
                items.push(todo("Materials…"));
            }
            out.push(sub(sub_name, items));
        }
    }
    // Live-effect plug-ins close the vector effects.
    out.extend(crate::dialogs::plugin::effect_menu());
    let raster: Vec<Item> = RASTER_MENUS
        .iter()
        .filter_map(|(name, path)| {
            let items = items_at(path, false);
            (!items.is_empty()).then(|| sub(name, items))
        })
        .collect();
    if !raster.is_empty() {
        out.push(Sep);
        out.push(Item::Header("Raster Effects"));
        out.push(c("Effect Gallery…", "ui.effectGalleryDialog"));
        out.extend(raster);
    }
    // Anything not placed above (future effects) still shows up.
    let placed = |e: &vectorcraft_effects::EffectInfo| top_level(e) || in_vector_menu(e) || RASTER_MENUS.iter().any(|(_, path)| e.menu == *path);
    for e in cat.iter().filter(|e| !placed(e)) {
        out.push(Item::Cmd(e.label, "effect.dialog", json!({ "effect": e.id })));
    }
    out
}

/// One entry per machine embroidery format (File › Export, Embroidery menu).
pub(super) fn machine_file_items() -> Vec<Item> {
    let f = |label: &'static str, ext: &'static str| cp(label, "file.export.embroidery", json!({ "format": ext }));
    vec![
        f("Tajima DST (.dst)…", "dst"),
        f("Brother PES (.pes)…", "pes"),
        f("Janome JEF (.jef)…", "jef"),
        f("Melco EXP (.exp)…", "exp"),
        f("Pfaff VP3 (.vp3)…", "vp3"),
        f("Singer XXX (.xxx)…", "xxx"),
        f("Barudan U01 (.u01)…", "u01"),
        f("Barudan DSB (.dsb)…", "dsb"),
        f("Pfaff PCS (.pcs)…", "pcs"),
        f("Husqvarna HUS (.hus)…", "hus"),
        f("Janome SEW (.sew)…", "sew"),
        f("Happy TAP (.tap)…", "tap"),
        f("Brother PEC (.pec)…", "pec"),
        Sep,
        f("Stitch List CSV (.csv)…", "csv"),
        f("Stitch List JSON (.json)…", "json"),
        f("G-code (.gcode)…", "gcode"),
        c("Stitch Image PNG (.png)…", "file.export.stitchImage"),
        c("Sew-out Animation GIF (.gif)…", "file.export.sewAnimation"),
        cp("Sew-out Video MP4 (.mp4)…", "file.export.sewAnimation", json!({ "format": "mp4" })),
    ]
}

/// Quilt sizes for edge-to-edge rows: label, width × height (mm), from the usual US sizes.
pub(super) const QUILT_SIZES: [(&str, f64, f64); 5] = [
    ("Crib (36 × 52″)", 914.4, 1320.8),
    ("Throw (50 × 65″)", 1270.0, 1651.0),
    ("Twin (70 × 90″)", 1778.0, 2286.0),
    ("Queen (90 × 108″)", 2286.0, 2743.2),
    ("King (108 × 108″)", 2743.2, 2743.2),
];

/// Embroidery › Longarm Quilting: continuous lines for longarm machines (Q1–Q7).
pub(super) fn quilting_items() -> Vec<Item> {
    vec![
        c("Check Continuous Line", "ui.quiltCheck"),
        c("Join into One Line", "quilt.join"),
        c("Start and End on the Edges", "quilt.edges"),
        sub("Edge-to-Edge Rows", QUILT_SIZES.iter().map(|(label, w, h)| cp(label, "quilt.pantograph", json!({ "width": w, "height": h }))).collect()),
        sub(
            "Continuous Fill",
            vec![
                cp("Stipple", "quilt.fill", json!({ "kind": "stipple" })),
                cp("Meander", "quilt.fill", json!({ "kind": "meander" })),
                cp("Echo", "quilt.fill", json!({ "kind": "echo" })),
                cp("Stitch in the Ditch", "quilt.fill", json!({ "kind": "ditch" })),
            ],
        ),
        sub(
            "Export Quilting Lines",
            vec![
                cp("HP-GL Plot (.plt)…", "file.export.quilt", json!({ "format": "plt" })),
                cp("DXF Lines (.dxf)…", "file.export.quilt", json!({ "format": "dxf" })),
                cp("SVG Lines (.svg)…", "file.export.quilt", json!({ "format": "svg" })),
            ],
        ),
    ]
}

/// Embroidery: digitizing, stitch types, stitch direction and the stitch order.
pub(super) fn embroidery_menu() -> Vec<Item> {
    let ty = |label: &'static str, ty: &'static str| cp(label, "embroidery.set", json!({ "type": ty }));
    let panel = |label: &'static str, id: &'static str| cp(label, "window.panel", json!({ "panel": id }));
    // The everyday steps first, then the stitching of the selection, then the whole design; the
    // rest is grouped in submenus so the menu fits on a laptop screen.
    vec![
        c("Make a Design…", "ui.makeDesign"),
        c("Add Words…", "ui.addWords"),
        c("Save for My Machine", "ui.saveForMyMachine"),
        c("Save for a Memory Card…", "ui.saveForMemoryCard"),
        c("Online Orders…", "ui.onlineOrders"),
        c("My Machine…", "ui.myMachine"),
        c("Ask About Embroidery…", "ui.askAboutEmbroidery"),
        c("Check My Sew-out…", "ui.checkSewout"),
        c("Foundry Account…", "ui.account"),
        Sep,
        sub(
            "Create",
            vec![
                c("Convert Artwork to Embroidery", "embroidery.convert"),
                c("Auto-Digitize Image", "embroidery.autoDigitize"),
                c("Design from Prompt…", "ui.designFromPrompt"),
                c("Redraw Image with Claude…", "ui.redrawImage"),
                c("Family Tree…", crate::embroidery::family::COMMAND),
                Sep,
                c("Digitizing Widget", "ui.embroideryWidget"),
                sub(
                    "Cross Stitch",
                    vec![
                        c("New Cross-Stitch Pattern…", "embroidery.crossStitch.new"),
                        c("Cross Stitch from Image…", "embroidery.crossStitch.fromImage"),
                        c("Print Cross-Stitch Chart…", "embroidery.crossStitch.chart"),
                    ],
                ),
                cp("Lettering Kiosk", "embroidery.kiosk", json!({ "on": true })),
            ],
        ),
        sub(
            "Stitch Type",
            vec![
                ty("Run", "run"),
                ty("Bean Run", "bean"),
                ty("Backstitch", "backstitch"),
                ty("Stem Stitch", "stemstitch"),
                ty("Blanket", "blanket"),
                ty("Satin", "satin"),
                ty("Tatami Fill", "fill"),
                ty("Satin Fill", "satinfill"),
                ty("Contour Fill", "contour"),
                ty("Spiral Fill", "spiral"),
                ty("Radial Fill", "radial"),
                ty("Cross Fill", "cross"),
                ty("Meander Fill", "meander"),
                ty("Motif Fill", "motiffill"),
                ty("Appliqué", "applique"),
                ty("Manual Stitches", "manual"),
                ty("Sequins", "sequin"),
                ty("Beads", "bead"),
                ty("Rhinestones", "stone"),
                ty("Cord", "cord"),
                ty("Boring", "boring"),
                ty("Print", "print"),
                ty("Foil", "foil"),
                Sep,
                ty("No Stitches", "none"),
            ],
        ),
        sub(
            "Stitch Direction",
            vec![
                cp("Inclination Tool", "tool.select", json!({ "tool": "embroideryDirection" })),
                c("Use Line as Direction", "embroidery.lineAsDirection"),
                Sep,
                c("Auto Direction", "embroidery.autoDirection"),
                c("Reverse Stitch Direction", "embroidery.reverse"),
                c("Swap Satin Sides", "embroidery.swapSides"),
                c("Convert to Satin Column", "embroidery.toColumn"),
                c("Clear Inclination Lines", "embroidery.clearGuides"),
                Sep,
                c("Fill Along Lines", "embroidery.fillAlongLines"),
                c("Clear Flow Lines", "embroidery.clearFlowLines"),
            ],
        ),
        sub(
            "Split Lines",
            vec![
                c("Carve Split Lines", "embroidery.carveSplits"),
                c("Contour Split Lines", "embroidery.contourSplits"),
                sub("Carve Stamp", Stamp::ALL.into_iter().map(|s| cp(s.label(), "embroidery.carveStamp", json!({ "stamp": s.id() }))).collect()),
                c("Clear Split Lines", "embroidery.clearSplits"),
            ],
        ),
        sub(
            "Motifs",
            vec![
                sub(
                    "Place Motif",
                    ["Wave", "Scallop", "Diamond", "Cross", "Feather", "Heart", "Star", "Leaf", "Loop", "Zigzag", "E-stitch"]
                        .into_iter()
                        .map(|m| cp(m, "embroidery.placeMotif", json!({ "motif": m.to_lowercase() })))
                        .collect(),
                ),
                c("Make Motif", "embroidery.motif.make"),
                sub(
                    "Place Ring",
                    vec![
                        cp("Satin Ring", "embroidery.placeRing", json!({ "stitch": "satin" })),
                        cp("Filled Ring", "embroidery.placeRing", json!({ "stitch": "fill" })),
                        cp("Bean Ring", "embroidery.placeRing", json!({ "stitch": "bean" })),
                    ],
                ),
            ],
        ),
        sub(
            "Add to Design",
            vec![
                sub(
                    "Add Outline",
                    vec![
                        cp("Satin Outline", "embroidery.addOutline", json!({ "stitch": "satin" })),
                        cp("Bean Outline", "embroidery.addOutline", json!({ "stitch": "bean" })),
                        cp("Rectangle Border", "embroidery.addOutline", json!({ "shape": "rectangle", "offset": 4 })),
                        cp("Oval Border", "embroidery.addOutline", json!({ "shape": "oval", "offset": 2 })),
                        cp("Rounded Border", "embroidery.addOutline", json!({ "shape": "rounded", "offset": 4 })),
                        cp(
                            "Motif Border",
                            "embroidery.addOutline",
                            json!({ "shape": "rounded", "offset": 4, "stitch": "motif", "motif": "scallop", "width": 3 }),
                        ),
                    ],
                ),
                c("Add Laydown", "embroidery.addLaydown"),
                sub(
                    "Add Buttonholes",
                    vec![
                        cp("Straight Buttonhole", "embroidery.addButtonholes", json!({ "style": "straight" })),
                        cp("Rounded Buttonhole", "embroidery.addButtonholes", json!({ "style": "rounded" })),
                        cp("Keyhole Buttonhole", "embroidery.addButtonholes", json!({ "style": "keyhole" })),
                    ],
                ),
                Sep,
                cp("Echo Quilting", "embroidery.outlineAround", json!({ "kind": "echo" })),
                cp("Background Quilting", "embroidery.outlineAround", json!({ "kind": "stipple" })),
            ],
        ),
        sub(
            "Shape",
            vec![
                c("Remove Holes", "embroidery.removeHoles"),
                c("Fill Holes", "embroidery.fillHoles"),
                c("Automatic Knife", "embroidery.knife"),
                Sep,
                c("Branch", "embroidery.branch"),
                c("Redwork", "embroidery.redwork"),
                c("Break Apart", "embroidery.breakApart"),
            ],
        ),
        Sep,
        sub(
            "Colors and Order",
            vec![
                c("Sort by Thread Color", "embroidery.sortByColor"),
                c("Match Threads to Chart", "embroidery.matchThreads"),
                c("Merge Colours", "embroidery.colors.merge"),
                c("Auto Sequence", "embroidery.autoSequence"),
            ],
        ),
        c("Check Design", "ui.embroideryCheck"),
        c("Density Map", "ui.embroideryDensity"),
        c("Compare Designs…", "ui.compareDesigns"),
        sub(
            "Stitches",
            vec![
                c("Make Properties Current", "embroidery.makeCurrent"),
                Sep,
                c("Freeze Stitches", "embroidery.freeze"),
                c("Regenerate Stitches", "embroidery.regenerate"),
                c("Remove Short Stitches", "embroidery.removeShortStitches"),
            ],
        ),
        Sep,
        sub(
            "Special Machines",
            vec![
                sub(
                    "Chenille",
                    vec![
                        cp("Chain", "embroidery.chenille.set", json!({ "stitch": "chain" })),
                        cp("Moss", "embroidery.chenille.set", json!({ "stitch": "moss" })),
                        cp("Lockstitch", "embroidery.chenille.set", json!({ "stitch": "none" })),
                        Sep,
                        c("Compound Chenille…", "embroidery.chenille.compound"),
                    ],
                ),
                sub("Longarm Quilting", quilting_items()),
            ],
        ),
        sub(
            "Embroidery Fonts",
            vec![
                c("Get Embroidery Fonts…", "ui.fontLibrary"),
                Sep,
                c("New Embroidery Font", "embroidery.fontDoc.new"),
                c("Save Embroidery Font…", "ui.saveEmbroideryFont"),
            ],
        ),
        sub("Export Machine File", machine_file_items()),
        Sep,
        sub(
            "Panels",
            vec![
                panel("Embroidery", "embroidery"),
                panel("Stitch Sequence", "stitchSequence"),
                panel("Stitch List", "stitchList"),
                panel("Embroidery Styles", "embroideryStyles"),
            ],
        ),
    ]
}
