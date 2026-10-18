# StitchFoundry build spec

The hand-off document for anyone, person or Claude Code session, continuing StitchFoundry. It
says what the product is, the rules, what exists today, everything still to build to reach
feature parity with the reference digitizing suite (Wilcom EmbroideryStudio 2026), in what
order, and how to verify it.

State described: branch `stitchforge/embroidery-core`, end of the first build session (2026-10-11).
User-facing behaviour of what exists is in [`embroidery.md`](embroidery.md).

- [1. What StitchFoundry is](#1-what-stitchfoundry-is)
- [2. Start here (new session checklist)](#2-start-here-new-session-checklist)
- [3. Ground rules](#3-ground-rules)
- [4. Architecture](#4-architecture)
- [5. What exists today](#5-what-exists-today)
- [6. Parity matrix](#6-parity-matrix)
- [7. Designs for the large missing pieces](#7-designs-for-the-large-missing-pieces)
- [8. Roadmap and estimates](#8-roadmap-and-estimates)
- [9. Verification and the quality bar](#9-verification-and-the-quality-bar)
- [10. Decisions for the owner](#10-decisions-for-the-owner)
- [11. Open source and AI features](#11-open-source-and-ai-features)
- [Appendix A: driving the app for screenshots](#appendix-a-driving-the-app-for-screenshots)
- [Appendix B: public sources](#appendix-b-public-sources)

---

## 1. What StitchFoundry is

**A standalone machine-embroidery design and digitizing application** (not a plugin). It is a
fork of [VectorCraft](https://github.com/storytold/vectorcraft), a pure-Rust, clean-room
vector illustration app, with embroidery built into its document model, engine and UI. It runs
natively on macOS, Windows, Linux and FreeBSD, and in the browser through WebAssembly.

**Goal:** the complete workflow of a professional digitizing suite, using the feature set of
Wilcom EmbroideryStudio 2026 (Designing level, all add-on Elements, multi-decoration) as the
checklist, written clean-room. On top of that:

- the digitizing **widget** of Embroidery Legacy (radial dial of brushes, widths, densities,
  stitch lengths and shapes);
- ideas from **Ink/Stitch** (any vector object carries embroidery parameters; guided fills,
  meander, satin along paths), as feature references only;
- **automation**: every feature is an engine command, reachable from the MCP server, the
  control channel and the CLI, so AI agents can digitize too.

**Why a VectorCraft fork:** the reference suite pairs its embroidery engine with a full vector
editor (CorelDRAW inside, "graphics mode"). VectorCraft gives us that editor, already built:
pen, shapes, type with OpenType, pathfinder, image trace, SVG/PDF/AI/EPS/DXF/EMF import and
export, layers, symbols, variables, undo, i18n, MCP. Embroidery is an attribute any vector
object can carry, so there is no separate "graphics mode": art and stitches live in one
document.

**Users:** professional digitizers, embroidery shops (production, team names, multi-head),
and home embroiderers.

**Out of scope:** CorelDRAW integration (VectorCraft replaces it); the reference suite's
proprietary EMB format, fonts, motif and monogram libraries and sample designs (we make our
own); vendor machine protocols that are not publicly documented.

---

## 2. Start here (new session checklist)

1. **Get the code.** The public repository
   [Stitchwright/StitchFoundry](https://github.com/Stitchwright/StitchFoundry), branch `main`. It
   keeps VectorCraft's history, so VectorCraft's updates merge in (below).
2. **Read, in order:** this spec, [`embroidery.md`](embroidery.md), the repo's
   [`CLAUDE.md`](../CLAUDE.md) and `AGENTS.md` (VectorCraft's rules apply unchanged), then
   `docs/development.md`.
3. **Build and run** (Rust 1.97+):
   ```sh
   export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0   # keeps target/ small
   cargo run -p vectorcraft -- --control 7979
   ```
   On a headless Linux box install `xvfb libegl1 libegl-mesa0 mesa-vulkan-drivers
   libxkbcommon-x11-0 x11-utils` and run with `DISPLAY=:99 WGPU_BACKEND=vulkan
   VECTORCRAFT_NO_PREFS=1 VECTORCRAFT_SYNC_RENDER=1` under `Xvfb :99` (Appendix A).
4. **Fast checks while working:**
   ```sh
   cargo test -p sf-core -p sf-formats -p vectorcraft-embroidery
   cargo test -p vectorcraft-engine --lib tests_embroidery
   cargo test -p vectorcraft-tools embroidery
   cargo test -p vectorcraft-ui-egui --lib i18n        # new strings need translations
   ```
   **Before every commit:** `cargo xtask ci` (fmt, clippy `-D warnings` with the no-panic
   lints, all tests, assets, brands, layers, wasm); GitHub runs the same on every push, with
   Rust 1.97.0. Last full run: 5,524 tests passing, clippy clean, wasm clean.
5. **Pick work** from [§8](#8-roadmap-and-estimates) in order, one task id per commit
   (`E1.3: stitch edit mode`). For each feature: engine command(s) + tests → tool/UI →
   translations → docs ([`embroidery.md`](embroidery.md)) → look at it in the running app
   (Appendix A) → `cargo xtask ci` → commit → push.
6. **Update this spec** when you finish a task: flip its status in [§6](#6-parity-matrix) and
   note anything learned.

**Merging VectorCraft's updates** (last: VectorCraft 0.9.0 and later, upstream `d1a6926f`,
merged 2026-10-11):

```sh
git remote add upstream https://github.com/storytold/vectorcraft   # once
git fetch upstream main && git merge upstream/main
```

- The translation catalogs conflict whenever both sides add rows. Merge them row by row
  (context + source): keep both sides' rows, our renames (StitchFoundry) and our removals
  (Discord, the old product strings), and carry upstream's translations of a renamed string over
  under its new name. A language upstream makes complete (`complete_menus`, `COMPLETE_MESSAGES`)
  then needs our embroidery strings too.
- Upstream's new user-visible text and links still say VectorCraft or point to ArtCraft: rename
  them (`cargo xtask brands` catches ArtCraft names, not "VectorCraft").
- StitchFoundry keeps its own preferences folder (`APP_FOLDER` in
  `apps/vectorcraft/src/prefs_dir.rs`) and reads VectorCraft's once; check that upstream changes
  to the profile code keep that.

Disk: a target dir grows past 20 GB. Use the variables above, delete
`target/debug/incremental` when space is short, and give each parallel agent its own
`CARGO_TARGET_DIR` (or its own git worktree), deleted when it finishes.

---

## 3. Ground rules

All of VectorCraft's non-negotiables in `CLAUDE.md` apply: clean-room, asset rules, no fonts
in the repo, never crash (no `unwrap`/`expect`/`panic`, untrusted input everywhere),
everything is a command, layering, thin UI, Rust only, never break wasm, quality gates.
Embroidery adds:

- **Clean-room toward the reference suite.** Never install, run, automate, screenshot or
  measure Wilcom (or any other commercial digitizing) software, and never compare our stitches
  against its output. Never read or copy anything from its installs: fonts, motifs, monograms,
  templates, sample designs, thread charts as shipped files, help text. Behaviour comes only
  from public documentation (docs.wilcom.com online help, product pages, release notes),
  cited by URL, rewritten in our own words, and from files users own (used locally, never
  committed). Feature names may be paraphrased; the UI uses our own names.
- **No GPL code.** Ink/Stitch (GPL) and Embroidermodder 2 are feature references only. MIT or
  zlib projects (pyembroidery, libembroidery) may be read to understand public file formats,
  but write our own code; pyembroidery may be used **outside the repo** as a test oracle
  (scratch virtualenv), never vendored.
- **Brands:** no vendor names in user-visible strings except file-format names (Tajima DST,
  Brother PES…), which identify formats. `cargo xtask brands` checks Rust literals.
- **Embroidery fonts and content are ours.** Lettering comes from OFL fonts in
  `storytold/craft-fonts` (or the user's installed fonts) converted at run time, plus
  embroidery "recipes" we author (data, attributed in `ASSETS.md`). Same for motifs,
  monogram templates, buttonhole presets, sequin shapes.
- **Units:** documents are in points (1/72 in), y down; `sf-core` works in millimetres, y
  down; stitch parameters are always shown in mm. Convert only at the crate boundary
  (`vectorcraft_embroidery::{mm, pt, MM_PER_PT, PT_PER_MM}`).
- **Stitch order is paint order** (bottom object sews first), so sequencing is arranging.

---

## 4. Architecture

```
            apps/vectorcraft (desktop)   apps/vectorcraft-web (wasm)   stitchfoundry-cli (MCP, batch)
                                 \               |                /
L6  vectorcraft-ui-egui   embroidery/{overlay,panel,sequence,widget}.rs, menus, toolbar
L5  vectorcraft-engine    cmd/embroidery.rs (embroidery.* commands), fileio (open/place machine files)
L4  vectorcraft-tools     embroidery.rs: Embroidery Brush tool, Inclination tool
L2  vectorcraft-embroidery  document → embroidery objects (mm), Planner (cached per node), StitchPlan
L1  sf-formats            machine file readers/writers (DST EXP JEF PES PEC VP3 XXX U01)
L1  vectorcraft-doc       Node.embroidery, Document.embroidery (serde, optional)
L0  sf-core               geometry, model, stitch engine, direction, brushes, threads, hoops, fabrics
```

**Data model**

- `Node.embroidery: Option<Box<sf_core::attr::Embroidery>>` on paths, compound paths and type.
  `Embroidery { stitch: StitchType, thread: Option<Thread>, width, profile: WidthProfile,
  guides: Vec<Guide>, auto_direction, entry, exit, trim_after }`.
  `StitchType = Run(RunParams) | Satin(SatinParams) | Fill(FillParams) |
  Applique(AppliqueParams) | Manual`. Guides and beads are in document points and follow the
  node's transforms (`Node::transform_scaled` calls `Embroidery::transform`).
- `Document.embroidery: Option<Box<DocEmbroidery>>`: hoop, fabric, design settings (trim
  threshold, ties…), thread chart, palette.
- Both are `serde(default, skip_serializing_if)`: documents without embroidery are unchanged.

**Pipeline:** `Planner::plan(&Document)` walks visible stitched nodes in paint order,
converts each (`crates/embroidery/src/convert.rs`: flatten to mm, regions by fill rule, split
holes and stroke joins for satin, automatic direction) into `sf_core::model::ObjectKind`s,
builds them with `sf_core::engine::build_cached` (object id = node id × 64 + part) into one
`Pattern` (absolute mm records: Stitch, Jump, Trim, ColorChange, Stop, End) and caches per
node by `Arc` pointer, so editing one object restitches only that object. The canvas, the
statistics, the player and every export read that plan.

**Stitch engine** (`crates/sf-core/src/engine/`): runs (single, triple, blanket, zigzag,
motif), satin columns (rails/rungs, half-step pairs, pull compensation, auto spacing, short
stitches, auto split, underlay centre/edge/zigzag), scanline fills (boustrophedon sections,
travel graph with Dijkstra, tatami offsets, gradient spacing, jitter), turning fills along
strips built from guides, contour/spiral/radial/cross/meander fills, appliqué sequences, ties,
trims, tiny-stitch removal.

**Direction** (`crates/sf-core/src/direction.rs`): `Guide` lines; `strip_from_guides`;
`auto_guides` (tip search, rails paired by dynamic time warping, quality check); 
`auto_fill_angle`. Satin shapes are pre-cut by `poly::split_holes` and `poly::split_branches`.

**Commands today:** `embroidery.set`, `setParams`, `setThread`, `autoDirection`, `setGuides`,
`addGuide`, `moveGuide`, `removeGuide`, `clearGuides`, `setBeads`, `convert`, `autoDigitize`,
`export`, `draw`, `settings`, `applyFabric`, `sortByColor`, `info`, `plan`; UI commands
`ui.embroideryWidget`, `file.export.embroidery`; tools `embroideryBrush`,
`embroideryDirection`; panels `embroidery`, `stitchSequence`. Params are documented in each
command's spec (`document.commands`, MCP `list_commands`).

---

## 5. What exists today

Working and tested (see [`embroidery.md`](embroidery.md) for behaviour):

- Stitch types on any path, shape or type: run, bean, blanket, zigzag, motif runs (5 motifs),
  satin (columns between rails, satin lines along paths with constant/tapered/calligraphy/
  pressure width, turning satin on closed shapes), tatami, satin fill, contour, spiral,
  radial, cross-hatch, meander fills, appliqué (placement, tack-down, cover, stops), manual
  stitches.
- Underlay (auto by width, centre run, edge run, zigzag, tatami), pull compensation, auto
  spacing, short stitches, auto split, tatami offsets, gradient density, randomised needle
  points, ties, trims by connector length, tiny-stitch removal, closest joins.
- Tatami textures (6 programmed split patterns) and split lines carved in from other art
  (drawn lines, a shape's outline like a carving stamp, a curve repeated across the fill).
- Automatic, editable stitch direction: auto inclination lines, best fill angle, splitting at
  holes and stroke joins; Inclination tool; draggable start/stop beads.
- Digitizing widget + Embroidery Brush (9 brushes, 11 shapes, sew before/after selection,
  snapping, editable value lists).
- Lettering from any font (satin for small text, tatami for large, counters split).
- Auto-Digitize (trace an image, satin vs tatami by thickness, colour-by-colour order).
- Machine files: DST, PES, PEC, JEF, EXP, VP3, XXX, U01 export, open and place.
  Cross-checked against an independent library in both directions.
- Hoops (27 presets, outline on canvas, fit check), fabrics (9 presets), thread charts
  (generic, PEC, JEF), design statistics (stitches, trims, colours, time, thread).
- TrueView shaded-thread canvas, stitch lines, connectors, needle points, fabric tint,
  stitch player, Stitch Sequence panel (grouped by thread, reorder, sort by colour).
- Embroidery menu, toolbar section, Embroidery panel; translations in 11 languages (19 with
  Bulgarian, Greek, Finnish, Hungarian, Korean, Polish, Swedish and Vietnamese, 2026-10-17, drafted
  with AI assistance and awaiting native review; Brazilian Portuguese has its menus only);
  pathfinder keeps stitch settings (holes via Minus Front).
- Tests: `sf-core` 62, `sf-formats` 43, engine embroidery 10, tools 2, plus the workspace's
  5,173 overall.

Production (phase 6): colorways and a product visualizer (garments drawn in code, or a photo of the
customer's own), worksheets (production, approval, pattern, placement template) with the shop's
name, logo and Code 128 / Code 39 barcodes, stitch images and life-size print views, batch output
with profiles, cost estimates, hoops chosen, turned, placed and of one's own, multi-hooping
(registration marks, split objects, multi-position hoops), the design library (browse, search,
convert), design templates, clipart, job orders, background exports and their history, a send
queue for machine folders, placement aids on garments, colour merge and a lettering kiosk for the
counter. See [`embroidery.md`](embroidery.md).

Known weak spots: satin splitting on unusual glyph shapes, auto-digitize on photos,
VP3/XXX/U01 interop beyond the oracle, not yet sewn on real machines.

---

## 6. Parity matrix

Every feature the reference suite lists in its public product comparison (2025 edition) and
its 2025–2026 release notes, in our words, with our status and approach.

**Status:** **Done** works and is tested · **Partial** works with gaps (noted) · **Missing**
not started · **Out** out of scope (reason given). **Phase** points to [§8](#8-roadmap-and-estimates).

### 6.1 Basics, viewing and threads

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| BAS-01 | Graphics and embroidery in one workspace (reference: switch to CorelDRAW mode) | Done | VectorCraft is the graphics editor; embroidery overlays the same document | – |
| BAS-02 | Design library browser | Done | The Design Library section (DM-01) | 0 |
| BAS-03 | Object properties view | Done | Embroidery panel | – |
| BAS-04 | Choose the target machine format for the design | Done | Design › Machine picks a machine profile; export writes its format unless another is chosen | 1 |
| BAS-05 | Open machine designs | Done | 8 formats as manual-stitch paths | – |
| BAS-06 | New design from template | Done | Design Templates: 7 built-in starts (left chest, cap front, full back, sleeve, towel, round patch, name tape: page, hoop, area guide, patch borders) and one's own | 0 |
| BAS-07 | Save native design | Done | `.stitchfoundry` keeps every stitch setting (VectorCraft documents inside, so `.vectorcraft` files open too) | – |
| BAS-08 | Send design by email | Out | Desktop share sheets vary; users attach the exported file | – |
| BAS-09 | Select objects | Done | VectorCraft selection tools | – |
| BAS-10 | Grids, rulers, guides, units, measure | Done | VectorCraft (major/minor grid, guides, smart guides, measure tool); AL1: CAD-style centre alignment: the hoop's centre lines show faintly while dragging, centre to centre wins over a nearer edge, and a meeting of centres shows a crosshair (across the hoop for its centre), ringed centres and *page center* / *centered* | – |
| BAS-11 | View embroidery / artwork / both, zoom, pan, overview | Done | Show stitches (Hidden/Lines/TrueView), artwork (Normal/Dimmed/Hidden), Navigator panel | – |
| BAS-12 | Stitch player (slow redraw) | Done | Stitch Sequence panel player | – |
| BAS-13 | Travel tools: step through a design by stitch, object, colour, trim, function | Done | Stitch Sequence panel: Travel by (stitch, object, colour, trim, function) with back and next buttons; the object reached is selected | 1 |
| BAS-14 | Colour-Object list (objects in sewing order, grouped by colour) | Done | Stitch Sequence panel: rows grouped by thread, drag to reorder, Shift/Ctrl multi-select, eye and lock per row, hidden objects listed to show again | 1 |
| BAS-15 | Show repeats (preview tiled/repeated layouts) | Done | Repeat objects are sewn and shown copy by copy; the Embroidery panel's *Repeats across* and *Repeats down* (`ui.embroideryRepeats`) show the whole design repeated in a grid, a gap apart, without sewing it | – |
| BAS-16 | Design information (stitches, size, colours, time, thread, cost) | Done | The Design section: size, stitches, colours, trims, time, thread and bobbin, and the cost a piece (OUT-05) | 0 |
| BAS-17 | Print preview of designs | Done | **Print Stitch View** (`embroidery.printView`): the stitches as they sew, life size over pages or fitted to one, to print or export; worksheets (RP-*) | 0 |
| BAS-18 | Thread palette, assign and edit thread colours | Done | Panel colour + chart picker | – |
| BAS-19 | Create and edit thread charts (custom charts, import/export) | Done | Built-in generic/PEC/JEF; charts of one's own from CSV or a GIMP palette (.gpl, D6), from the design or thread by thread, exported as CSV, used everywhere a chart is | 0 |

### 6.2 Settings and hardware

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| SYS-01 | Autosave and backup | Done | VectorCraft recovery | – |
| SYS-02 | Digitizing presets (default properties for new objects) | Done | Current properties per stitch type (`DocEmbroidery.current`), saved with the design: Stitch Type, Convert Artwork and Set Thread give new objects them; the panel edits them with nothing selected | 1 |
| SYS-03 | Inserted-file thread handling (keep, match to chart) | Done | Machine files keep their colours; Match Threads to Chart (`embroidery.matchThreads`) maps them to a chart's closest threads | 1 |
| SYS-04 | Object property apply options (apply to selection vs. new objects) | Done | The Embroidery panel edits the selection, or with nothing selected the properties new objects take (`embroidery.current.setParams`) | 1 |
| SYS-05 | Custom hoop list, hoop options | Done | 60 standard hoops and frames (HO-04 added Husqvarna Viking, Pfaff creative, Janome, Bernina and Brother sewing fields from the makers' public product pages); hoops of one's own kept by name (`embroidery.hoop.saveCustom`, `.deleteCustom`), listed with them, and added from File › My Machine (My own hoops), so they show on the Home screen and in Make a Design | 0 |
| SYS-06 | Registration-mark settings for multi-hooping | Done | `embroidery.settings {registration}`: cross, circle or target marks, their size, how many in each overlap and their colour, kept with the design | 0 |
| SYS-07 | Reshape options (show stitch angles, beads, guides) | Done | Start and stop and Inclination lines toggles in the panel: the selection's beads and stitch-angle lines with any tool | 1 |
| SYS-08 | Warning options (long stitches, small objects, density too high) | Done | Check settings: density, long stitch, small lettering and tiny object limits saved with the design (`embroidery.settings {checks}`) | 1 |
| HW-01 | Machine connection / design queue manager | Done | Send to Machine and the send queue (`embroidery.queue.*`): designs wait for their machine's folder and go in order when it is there, kept across sessions; vendor network protocols are not publicly documented (OUT-04) | 0 |
| HW-02 | Runtime analyst (machine run statistics) | Out | Needs vendor machine telemetry | – |
| HW-03 | Scanner input | Out | Use the OS scanner app, then place the image | – |
| HW-04 | Digitizing tablet | Done | Pen pressure drives the pressure brush | – |

### 6.3 Machine formats and stitch generation limits

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| MF-01 | Machine format profiles (max stitch, max jump, trim method, tie-in/off, colour change codes) | Done | Seven built-in profiles, one per format (commercial: DST, EXP, U01; home: PES, JEF, VP3, XXX), set the design's longest stitch and jump, speed and trim time; `embroidery.settings {machine}`, Machine settings in the panel | 1 |
| MF-02 | Edit standard profiles, advanced settings | Done | Save to Machine keeps the design's machine settings in its profile: limits, speed and trim time, and the advanced ones (shortest stitch, trim length, ties, colour change time); Reset Machine puts a standard profile back; `embroidery.machine.save` / `delete` | 1 |
| MF-03 | Custom machine formats | Done | Profiles of your own (a name and one of the formats) kept in the preferences; Save as New Machine, rename, change format, delete; a design keeps its machine's name and format | 1 |
| MF-04 | More file formats | Partial | Pfaff PCS (experimental), Happy TAP and Barudan DSB read and written, ZSK DSZ read (D1); HUS and VIP need a compression writer and SEW uncertain header fields (public descriptions only); open text exports written (CSV and JSON stitch lists, G-code for CNC-driven machines); add ZSK Z00, Barudan FDR/U00, Toyota 100, Tajima TBF; CSD, JPX, PHB/PHC, ZHS, EMD are not publicly described | 6 |

### 6.4 Stitch types and digitizing input

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| ST-01 | Run (single) | Done | | – |
| ST-02 | Sculpture run (raised, satin-like run) | Done | A thick-thin run style: groups of triple stitches in turn with groups of single ones, each with its own stitch length and count (as public help describes it: https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Digitizing/stitches/stitches-6.htm) | – |
| ST-03 | Backstitch, stemstitch | Done | Backstitch and stem stitch run styles (forward two, back one; stem slanted) | 1 |
| ST-04 | Satin borders and fills | Done | | – |
| ST-05 | Tatami borders and fills | Done | Fills; tatami borders: *Tatami stitches* on any satin column, line or border (rows split at the stitch length, offset a quarter per row) | 1 |
| ST-06 | Graphical digitizing (draw vectors, then stitch) | Done | | – |
| ST-07 | Rounded-corner rectangles, per-corner radii | Done | VectorCraft live corners | – |
| ST-08 | Column C (regular column of set width along a line) | Done | Steil brush / satin along a path | – |
| ST-09 | Column A/B (irregular column: click left/right rail points alternately) | Done | Column A/B tool (`embroidery.column`): clicks alternate between the sides, Alt for smooth points, each pair an inclination line; Backspace, Enter, Undo per click | 1 |
| ST-10 | Complex turning / complex fill (free shapes with holes, stitch angles) | Done | Closed shapes, guides, holes | – |
| ST-11 | Continue digitizing an object from its exit (or entry) | Done | Column A/B tool continues the selected column from its end; the Pen tool continues a stitched open path from either end, keeping its stitch type | 1 |

### 6.5 Object properties, styles, colorways

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| OP-01 | Change properties of selected objects | Done | | – |
| OP-02 | Make selected object's properties current | Done | Make Current in the panel, Embroidery › Stitches › Make Properties Current (`embroidery.makeCurrent`) | 1 |
| OP-03 | Fabrics: choose, apply, manage custom fabrics | Done | 9 presets plus fabrics saved with the design: Fabric settings in the panel, `embroidery.fabric.save` / `delete`; a library shared between designs comes with templates (OP-05) | 1 |
| OP-04 | Styles: named property sets, apply and update | Done | Embroidery Styles panel and `embroidery.style.*`: new from the selection, apply, redefine (updates every object following it), break link, rename, duplicate, delete | 1 |
| OP-05 | Design templates (properties + layout) | Done | `embroidery.template.save`: a design's page, layers, art, hoop, fabric, machine and settings kept as a template of one's own | 0 |
| OP-06 | The fabric asked, not left to be found | Done | From the first sew-out (a T-shirt sewn with woven settings): Make a Design asks "What fabric will you sew it on?" after making the design, and Save for My Machine asks first when the design has no fabric (`ui.saveForMyMachine {fabric?}` → `{askFabric}`); "not sure" leaves it as it is, asked once a design. A fabric's own fill stitch length is no longer flagged as short by Check Design | 0 |
| CW-01 | Colorways (several thread schemes per design) | Done | `DocEmbroidery.colorways` (a thread per slot: the design's colours in first-use order) and the active one, used by the preview, machine files and worksheets; `embroidery.colorway.*` and the Design section | 0 |
| CW-02 | Name design elements | Done | Layers/object names | – |
| CW-03 | Split colour blocks (same colour, separate stops) | Done | *Own colour block* (`ownBlock`): a colour change before the object even in the same colour, kept in machine files | 0 |
| CW-04 | Backgrounds (fabric colour, garment image) | Done | Fabric colour; **Garment Photo** (`embroidery.background`): a photo life size behind the design on a locked template layer, never stitched | 0 |
| CW-05 | Product visualizer (design on a garment mockup) | Done | `embroidery.visualize`: life size on a T-shirt, polo, hoodie, cap (ball cap, snapback, trucker or dad cap), tote or towel drawn in its colour at the usual placement (back view for back placements), or on the customer's photo; too-wide designs warned of | 0 |
| CW-06 | Change colour schemes (recolour design) | Done | `embroidery.recolor`: threads changed by a colour map or a scheme (hue turn, one hue, grey, inverted), matched to a chart, art repainted, one undo; colorways keep schemes side by side | 0 |

### 6.6 Combine, sequence, transform, reshape

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| CO-01 | Copy, paste, duplicate, clone | Done | | – |
| CO-02 | Group and lock | Done | | – |
| CO-03 | Select by colour or stitch type | Done | Select › Same › Thread Color and Stitch Type (`select.same.thread`, `select.same.stitchType`) | 1 |
| CO-04 | Sequence objects (move earlier/later, to start/end) | Done | | – |
| CO-05 | Auto-sequence whole design (by colour, by object, closest path) | Done | Embroidery › Colors and Order › Auto Sequence (`embroidery.autoSequence`): fewest colour changes, then the closest next object; objects that overlap keep their stacking order; reorders within each layer or group. Sort by Thread Color as before | 1 |
| TR-01 | Position, align, distribute, scale, rotate, skew, mirror | Done | Stitch settings follow transforms | – |
| PT-01 | Decorative borders (frames around designs) | Done | Add Outline › rectangle, oval, rounded and motif borders, in satin, bean, run or a motif | – |
| PT-02 | Stars and rings (stitched rings with satin/run/fill) | Done | Star stamp; Embroidery › Motifs › Place Ring: a satin, filled or bean ring of any diameter and width | – |
| PT-03 | Basic shapes | Done | | – |
| PT-04 | Add / remove holes; fill holes | Done | `embroidery.removeHoles {maxArea?}` and `embroidery.fillHoles` (a fill object per hole) | 1 |
| PT-05 | Offsets and outlines (closed, open) | Done | Add Outline: an outline following shapes at a distance, in any stitch, and round open lines (the band their stitches cover) | – |
| PT-06 | Wreaths and arrays (mirror-merge) | Done | Repeat objects (radial, grid, mirror) sew every copy | – |
| RS-01 | Reshape objects and stitch angles | Done | Direct selection + Inclination tool | – |
| RS-02 | Reshape stars and rings | Done | Stars and rings are ordinary paths: scale, reshape and edit their points | – |
| RS-03 | Envelopes on embroidery | Done | Stitched objects in an envelope are sewn bent through it (their inclination lines and beads aren't bent) | – |
| RS-04 | Break apart branched objects | Done | Embroidery › Shape › Break Apart (`embroidery.breakApart`): a branch gives back its objects; a stitched path or compound path of separate parts becomes one object per part, holes staying with their part | 1 |
| RS-05 | Knife (cut shapes) | Done | VectorCraft knife/scissors | – |
| RS-06 | Automatic knife (split long shapes into sewable parts) | Done | Embroidery › Shape › Automatic Knife: shapes cut round their holes and at sharp inside corners into pieces with automatic directions, overlapping along the cuts, as a branch group (public description: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Modifying/reshape/reshape-14.htm) | – |
| RS-07 | Shaping tools (weld, trim, intersect on embroidery) | Done | Pathfinder keeps stitch settings | – |

### 6.7 Stitch editing, reinforcement, quality, connectors

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| SE-01 | Stitch edit: select, move, insert, delete individual stitches | Done | Stitch Edit tool: pick (click, Shift, marquee), drag to move, Alt-click to insert, Delete; the first edit freezes the object (`Embroidery.frozen`, host coordinates, follows transforms); Regenerate Stitches; `embroidery.stitches.edit` | 1 |
| SE-02 | Split an object's stitches | Done | Split Here (panel, one stitch picked with the Stitch Edit tool) or `embroidery.stitches.split {id, at}`: the object keeps the stitches before, a copy sewn right after it the rest, both frozen | 1 |
| SE-03 | Insert/edit/clear machine functions (trim, stop, colour change, jump) at stitches | Done | `embroidery.function.insert {kind: trim\|stop\|jump}` and `.clear` at single stitches (freezes); Stop after per object without freezing. Colour changes inside an object are not supported | 1 |
| SE-04 | Stitch list (table of stitches with coordinates and functions) | Done | Stitch List panel: every record with function, position, length and object; click plays to it and selects the object; follows the player | 1 |
| SE-05 | Manual stitches | Done | | – |
| SE-06 | Convert selected stitches to objects (recognise runs/satin/fill from stitch data) | Done | Embroidery › Convert Stitches to Objects (`embroidery.stitchesToObjects`): an opened machine file's manual stitches cut where the needle turns back, single swings side to side read as satin (rails and angles), side-by-side rows of several stitches as tatami (outline, angle, spacing, length), the rest as runs; same threads and order, one undo step | 0 |
| RE-01 | Backtrack / repeat outlines | Done | Passes 1–9 on every run style; 2 = backtrack | 1 |
| RE-02 | Reverse stitch direction | Done | `embroidery.reverse` swaps where sewing starts and stops; Swap Satin Sides (`embroidery.swapSides`, D4) starts a satin column's zigzag on its other side so it ends on the other side; Convert to Satin Column (`embroidery.toColumn`, D3) turns a closed turning-satin shape into a column between two editable rails; Density Map (`embroidery.densityMap`, D2) colours the design by thread layers | 1 |
| RE-03 | Automatic underlay | Done | | – |
| RE-04 | Underlay by shape (choose per object), double zigzag | Done | Centre/edge/zigzag/double zigzag/tatami, automatic by width; per-type underlay presets through the current properties (SYS-02) | 1 |
| RE-05 | Pull compensation, twin-sided (different per side) | Done | Pull compensation per side (`pull_comp_2`) | 1 |
| RE-06 | Laydown (open base stitching for pile fabrics) | Done | Embroidery › Add to Design › Add Laydown: an open tatami under the selected shapes and type, a margin beyond them, sewn first | – |
| SQ-01 | Remove underlying stitching (cut lower fills where upper objects cover) | Done | Per-object option with overlap (default 1 mm): fills sewn before it are cut by what it covers, live | 1 |
| SQ-02 | Stitch angles, density | Done | | – |
| SQ-03 | Remove small stitches | Done | | – |
| SQ-04 | Smart corners (satin corner treatment: mitre, cap, overlap) | Done | Corners on satin lines and satin borders: automatic (turn ≤ 45°, miter ≤ 110°, capped beyond), turn, miter, capped; no fold-back on tight curves. An overlap (lap) corner is not built | 1 |
| SQ-05 | Reduce bunching (short stitches at inner curves) | Done | | – |
| SQ-06 | Split long stitches | Done | Auto split | – |
| SQ-07 | Preserve long stitches (keep long satin as is) | Done | Preserve long stitches: frame moves in steps, one needle drop | 1 |
| SQ-08 | Fractional / accordion spacing control | Done | Gradient end spacing (SP-10) and spacing profiles: even, accordion, fade out and in, or any points (SP-09) | – |
| SQ-09 | Design checks (density hot spots, tiny objects, long jumps, gaps) | Done | `embroidery.check`: dense spots (thread layers over 2 × 2 mm), long stitches, tiny objects, small lettering, no stitches, hoop; Check Design in the panel | 1 |
| CN-01 | Automatic connectors (jump vs trim by length) | Done | | – |
| CN-02 | Manual tie-offs and trims | Done | Trim after; per-object tie-in/tie-off on/off; design tie style (single, double, triangle) and tie length | 1 |
| CN-03 | Keep / omit last stitch | Done | Omit last stitch object option | 1 |
| CN-04 | Entry/exit points | Done | Beads | – |
| CN-05 | Closest join | Done | | – |
| CN-06 | Branching (sew a group as one path, travel hidden under later fills) | Done | Embroidery › Shape › Branch (`embroidery.branch`), §7.2: closest-first order keeping overlaps, exits aimed under the objects still to sew, travel runs routed inside them; trims where nothing covers the way | 1 |

### 6.8 Patterned, curved and motif stitching

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| PS-01 | Tatami offsets (texture by row offset patterns) | Done | | – |
| PS-02 | Program splits (needle-point patterns inside fills) | Done | Tatami *Texture*: 40 patterns generated in code (diamonds, waves, herringbone, bricks, circles, scales, hexagons, chevrons, zigzag, ogee, basket weave, stars, hearts, leaves, stripes, slants, pinstripes, lattice, arches, rings, dots, spirals, pyramids, ripples, Greek key, crosses, quatrefoils, flowers, trefoils, moons, arrows, nested diamonds, argyle, wicker, feathers, paisley, swirls, snowflakes, bubbles, sunbursts) with tile size and turn | – |
| PS-03 | User-defined splits (draw split lines) | Done | Paths drawn over a tatami fill carved in as split lines (Embroidery › Split Lines › Carve Split Lines, `embroidery.setSplits`), shown dashed on the fill | – |
| PS-04 | Carving stamps (embossed patterns pressed into fills) | Done | Any closed shape's outline pressed in (Carve Split Lines), and a library of 16 stamps drawn in code (Embroidery › Split Lines › Carve Stamp: star, heart, flower, leaf, spiral, sun, moon, snowflake, paw print, swirl, shell, butterfly, clover, feather, rings, diamond) | – |
| CS-01 | Contour fill/outline | Done | | – |
| CS-02 | Offset fills (concentric offsets) | Done | Contour fill with a ring count and spacing (*Rings*) | – |
| CS-03 | Spiral fills | Done | | – |
| CS-04 | Radial (ripple) fills | Done | | – |
| CS-05 | Flexi-splits (split lines following a curve) | Done | A drawn curve repeated across the fill (`embroidery.carveSplits` `repeat`), and split lines that follow the shape by themselves: its outline repeated inward (Embroidery › Split Lines › Contour Split Lines) | – |
| CS-06 | Florentine effect (stitches follow a drawn curve) | Done | Turning fill along inclination lines; tatami rows that copy a drawn line (Embroidery › Stitch Direction › Fill Along Lines, one line) | – |
| CS-07 | Liquid effect (curved fill between two guide curves) | Done | Embroidery › Stitch Direction › Fill Along Lines with two lines: tatami rows changing shape evenly from one line to the other (public description: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Decorative/curves/curves-11.htm) | – |
| MO-01 | Single motifs (place one stitched motif) | Done | Embroidery › Motifs › Place Motif puts one motif on the artboard as a run you move and reshape; the Motif tool places one where you click, or as long as you drag and turned your way | – |
| MO-02 | Motif runs | Done | 11 built-in motifs, and the design's own | – |
| MO-03 | Graded motif runs (motif size varies along the line) | Done | Motif runs grow or shrink to *Width at end* | – |
| MO-04 | Motif fills, paired motif fills, 3D motif fills | Done | Motif fill: rows of any motif inside the shape, row offset; paired motif fills (a second motif in turn along the rows); 3D motif fills (*3D effect*: globe out, globe in, perspective, with a strength and a direction; public description: https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Decorative/motifs/motifs-14.htm and motifs-15.htm) | – |
| MO-05 | Graded motif columns | Done | Satin columns and lines sewn as motifs (*Sew as: Motifs*): each as wide as the column where it sits, its length a multiple of that width, turning with the rungs; tapered lines grade them | – |
| MO-06 | Motif sets and custom motifs (make a motif from an object) | Done | Embroidery › Motifs › Make Motif turns a drawn path into a motif kept with the design: motif runs, motif fills, borders, the Motif tool and Place Motif sew it, upright as drawn; remaking it by name updates what sews it; My Motifs keeps motifs for any design (`embroidery.motif.keep`) | – |

### 6.9 Specialty stitching

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| SP-01 | Freehand embroidery | Done | Brush tool | – |
| SP-02 | Hand-stitch effect (random spacing and thickness) | Done | *Hand stitch* run style: thick (triple) stitches and thin gaps, lengths varied at random | – |
| SP-03 | Jagged edges (random satin length) | Done | Satin *Jagged edges* on side 1, side 2 or both, up to a depth | – |
| SP-04 | String stitching (long free-hanging threads) | Done | String stitch, a run style: long straight stitches spanning part of the line (*Chord length*), one starting every *Spacing*, overlapping like string art; round a closed line they make a ring, a coverage set by the chord (public description: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Decorative/specialty/specialty-9.htm) | – |
| SP-05 | Zigzag and E-stitch borders/fills | Done | Zigzag and blanket runs; zigzag and E-stitch motifs for motif fills | – |
| SP-06 | Stippling (random meandering run fill) | Done | Meander fill: a rounded line that never crosses itself, with path spacing, stitch length and randomness | – |
| SP-07 | Maze-like fills | Done | Meander | – |
| SP-08 | Trapunto (open fill with raised outline effect) | Done | Tatami *Trapunto*: travel keeps to the outline (with an open density) | – |
| SP-09 | Accordion spacing with custom profiles | Done | Tatami *Spacing profile*: even, accordion, fade out and in, a curve to reshape, or any points agents set | – |
| SP-10 | Colour blending / multi-blend (several layered fills with graded density) | Done | Make Blend: stacked tatami layers, 2 to 6 colours, each fading into the next | – |
| SP-11 | Chenille-type patterns on regular machines (square, coil) | Done | Coil and square-loop motifs: overlapping loops as motif runs and motif fills | 0 |
| SP-12 | Cross-stitch fills (X stitches on a grid) | Done | Cross fill: X stitches on a square grid of any cell size and angle, as full, half, upright or double crosses | – |
| SP-13 | Redwork (outlines grouped into one path, entry = exit) | Done | Embroidery › Shape › Redwork: line art becomes a redwork group sewn as one tour (entry = exit) over the lines' graph: odd junctions paired along the shortest ways, an Euler tour, each line sewn once in single, triple, back or stem stitch and the retraced ways as a single run; ends within 0.3 mm meet; Break Apart restores (public description: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Release/rn-28-notes/rn-28-notes-5.htm) | – |
| SP-14 | Buttonholes (preset types, single or a string) | Done | Add Buttonholes: straight, rounded and keyhole buttonholes (a holding run, satin sides, bartacks, a rounded end or an eye round an open slit), one or an evenly spaced string (`count`, `at`, `to`), each a group of satin paths; the Embroidery panel's Buttonholes section sets the style, size, count, spacing and angle | – |

### 6.10 Multi-decoration

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| MD-01 | Virtual decoration preview (print, foil, etc.) | Done | Print and Foil types: shapes shown printed (flat) or foiled (metallic sheen) under the stitches, never sewn | 0 |
| MD-02 | Multi-head cording / couching | Done | Cord couched with a zigzag over it, the cord shown; a stop to lay it (cording device codes aren't public) | 0 |
| MD-03 | Boring (cut-work holes) | Done | Each closed shape a hole: outline run, stop to cut, satin edge; shown as a hole (boring device codes aren't public) | 0 |
| SQN-01 | Sequin mode, sequin palette (sizes, shapes, colours) | Done | Sequins as a stitch type: round, square, star, heart, flower; 3–9 mm palette (`embroidery.decor.palette`); a colour of their own; shown as sequins on the canvas | 0 |
| SQN-02 | Sequin runs, multi-sequin runs, sequin fixings | Done | Runs along lines with a gap or overlap, sizes in turn (multi), single, double, three-way and cross fixings; a stop to load them (device codes aren't public) | 0 |
| SQN-03 | Sequin fills (digitize, border, fancy, multi, split, flip) | Done | Honeycomb or grid fills at an angle (sewn back and forth), borders inside the edge, fancy shapes, multi sizes, half-overlap (split) by a negative gap, every other flipped | 0 |
| SQN-04 | Convert objects to sequins; manual sequins; custom sequins | Done | Any line or shape set to sequins; one at each drawn point (`layout: manual`); custom size, shape and colour | 0 |
| BD-01 | Beading: bead runs, fixings, edit runs, manual beads | Done | Beads along lines, in shapes or at points, sewn through and back; edited as the path; counted | 0 |
| BL-01 | Bling: palettes, runs, fills, borders, manual stones, bling lettering, edit | Done | Rhinestones SS6–SS30 along lines, round edges, packed in shapes, at points, on type (bling lettering); not sewn; shown with facets; templates by Cut File | 0 |
| AP-01 | Convert vectors to appliqué; create appliqué | Done | Appliqué type | – |
| AP-02 | Fabric swatches on appliqué (show the fabric) | Done | Each patch's fabric drawn under its stitches: a colour, plain or patterned (stripes, check, dots, denim, felt) with a second colour | 0 |
| AP-03 | Partial-cover appliqué (cover only uncovered edges) | Done | `partial` on appliqué: the cover leaves out the edges objects sewn later lie over | – |
| AP-04 | Combine appliqué components (one sequence for several pieces) | Done | Combine Appliqué: pieces made one compound sewn as one sequence: all placement lines, one stop, all tack-downs, one stop, all covers | 0 |
| AP-05 | Auto-close and break-up appliqué (open shapes, overlapping pieces) | Done | Prepare Appliqué: open outlines closed, overlapping pieces trimmed under the ones in front | 0 |

### 6.11 Lettering, team names, monograms, fonts

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| LT-01 | Create lettering on the canvas | Done | VectorCraft Type tool + stitch type | – |
| LT-02 | Format text (font, size, spacing, case) | Done | VectorCraft Character/Paragraph | – |
| LT-03 | Baselines: straight, arc, circle, vertical, free line, any shape | Done | Embroidery-font lettering straight, round a circle (arched or along its bottom) or a spiral, stacked vertically, or along any drawn line or shape (`arc`, `spiral`, `vertical`, `path`), aligned left, centre or right | 0 |
| LT-04 | Edit text, scale, transform | Done | Restitches live | – |
| LT-05 | Adjust individual letters (position, size, rotation, stitch settings) | Done | Embroidery-font lettering: a letter moved, scaled or turned (`embroidery.lettering.letter`; the Letter rows in the panel), kept when it is set again; a glyph's stitching changes like any art's until it is set again; VectorCraft type: kerning and baseline shift | – |
| LT-06 | Break apart lettering | Done | Create Outlines, then stitch | – |
| LT-07 | Special characters | Done | Glyphs panel | – |
| LT-08 | Lettering stitch types | Done | Embroidery-font lettering sewn as the font has it, all satin or each letter one fill (`stitch`); 3D puff and appliqué letters too | 0 |
| LT-09 | Lettering art (envelopes, arcs, warps) | Done | VectorCraft envelopes and warps on embroidery lettering: sewn as they look, the lettering editable inside them; arcs and spirals built in (LT-03) | 0 |
| LT-10 | Letter sequencing (left-right, centre-out) and join methods (closest, baseline) | Done | Embroidery-font lettering sews its letters left to right, right to left or centre out and its lines down or up, joined where closest or along the baseline, trimmed after each letter, word or line (`order`, `lines`, `join`, `trim`; Sew order, Join, Trims in the panel) | – |
| LT-11 | Auto kerning and letter spacing | Done | Font kerning, tracking | – |
| LT-12 | Lettering underlay | Done | Auto underlay | – |
| LT-13 | Recommended minimum/maximum height per font shown while typing | Done | Embroidery fonts give their sizes (`embroidery.fonts`: minSize, maxSize); the lettering section shows them by the size, marked as a warning while the size typed is outside them, and lettering set outside them comes with a warning | – |
| LT-15 | Type sewn letter by letter, not piece by piece | Done | From the first sew-out (275 trims, ragged serif tips): the satin pieces a letter is cut into are sewn as one path, each next piece one that touches the rest and leaves it whole, the thread carried under the pieces still to come; a column that must end where it began runs up its middle first and lands on the side nearer the next piece; satin is never narrower than 0.8 mm. The same design: 275 → 102 trims, nearly all between letters | 0 |
| LT-14 | Small-text optimisation (short stitches, lighter pull compensation, special underlay) | Done | Lettering `small` (auto, on or off; auto under 6 mm): centre-run underlay, half the pull compensation, short stitches; fills and runs shortened | 0 |
| TM-01 | Team names: many names under one design, CSV import/export | Done | Embroidery panel › Team Names: names typed or read from a CSV column (`embroidery.teamNames.read`), set in the selected embroidery-font lettering; VectorCraft Variables datasets still drive type | – |
| TM-02 | Team-name templates; output per name (files, multi-head) | Done | One machine file per name (`embroidery.teamNames.export`); all names laid out in one design, each still lettering (`.layout`); a names worksheet with sizes, stitches, times and totals (`.worksheet`); any design with its lettering is the template | 0 |
| TM-03 | Monograms: templates, letter styles, ornaments, borders | Done | Embroidery panel › Monogram (`embroidery.monogram`): one to three initials in an embroidery font, classic, in a line or stacked; nine border shapes in up to four rings; ornaments either side; a gallery of nine templates and the user's own saved (`embroidery.monogram.templates`) | 0 |
| CL-01 | Convert TrueType/OpenType fonts | Done | Live from glyph outlines | – |
| CL-02 | User-refined letters (save edited glyphs) | Done | Any loaded embroidery font opens as a font document (`embroidery.font.edit`): refine its glyphs, add missing ones, change its properties and kerning, save it back | – |
| CL-03 | Create custom embroidery fonts (from art or fonts) | Done | The font creator: a font document, one artboard per glyph with baseline and cap-height guides, digitized with the usual tools or started from a TrueType/OpenType family's outlines (`embroidery.fontDoc.fromType`), saved as a font folder lettering loads (`embroidery.fontDoc.*`); fonts in Ink/Stitch's format load and edit too | – |
| CL-04 | Merge fonts | Done | `embroidery.fontDoc.merge`: another font's glyphs added to a font document, scaled to its capitals' height | – |
| KDC-01 | Keyboard design collections (map designs to keys, type with them) | Done | A design assigned to a character in a font document (`embroidery.fontDoc.assign`) becomes its glyph; saved, the font types the designs like lettering | – |
| KI-01 | Lettering kiosk (simplified mode for counter staff) | Done | Embroidery › Create › Lettering Kiosk (`embroidery.kiosk`, `embroidery.kiosk.order`): one panel for the counter (template, name, font, thread, size; made to fit the template's area; shown on the product; sent to the machine folder or saved), the shop's choice of templates, fonts and threads, locked with a PIN, kept on across restarts | 0 |

### 6.12 Artwork and automatic digitizing

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| BM-01 | Insert, paste, crop bitmaps; edit in another app | Done | VectorCraft place, crop, Edit Original | – |
| BM-02 | Smooth bitmaps, prepare (reduce colours, clean edges) | Done | Embroidery › Prepare Image (`embroidery.image.prepare`): median smoothing that keeps edges, colours reduced to the main ones (k-means in Lab), specks and ragged edges cleaned by a majority filter; a prepared copy over the original, or in its place | 0 |
| BM-03 | Add and match image colours to threads | Done | `embroidery.image.colors`: an image's main colours with their share and nearest chart thread; colour photo stitch threads matched to the chart; Match Threads to Chart after Auto-Digitize | 0 |
| VE-01 | Create, colour, import vectors (SVG, PDF, AI, EPS, DXF, EMF) | Done | VectorCraft | – |
| VE-02 | Auto-trace bitmaps to vectors | Done | Image Trace | – |
| VE-03 | Smooth curves | Done | Smooth tool, Simplify | – |
| AD-01 | Convert vector objects to embroidery (and back) | Done | Stitch types on vectors; "No Stitches" removes | – |
| AD-02 | Convert between stitch types | Done | | – |
| AD-03 | Auto-digitize shapes (click a region to stitch it) | Done | Whole-image Auto-Digitize; the Click-to-Stitch tool (`embroidery.clickStitch`): a magic-wand region becomes tatami (with or without its holes), satin, an outline or a centre-line run | – |
| AD-04 | Smart design (auto-digitize with outlines, details as runs, sequencing) | Done | Auto-Digitize: satin/tatami by thickness, details thinner than `details` as runs, satin outlines round the design or each filled shape in the darkest thread, threads matched to a chart, background and specks removed, sewn colour by colour nearest-first | 0 |
| PH-01 | Photo line embroidery (photo flash: lines of varying density) | Done | `embroidery.photoStitch {mode: lines}`: parallel lines whose zigzag widens with darkness, one line of manual stitches; spacing, angle, invert, contrast | 0 |
| PH-02 | Coral / single-run photo stitch | Done | `embroidery.photoStitch {mode: coral}`: points scattered by darkness, sewn nearest-first as one meandering run (trims over 5 mm gaps) | 0 |
| PH-03 | Colour photo stitch | Done | `embroidery.photoStitch {mode: color}`: main colours matched to chart threads on a cell grid, cleaned, each a short-stitch tatami fill, lightest first, angles turned per colour | 0 |

### 6.13 Hooping, production, output, management

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| HO-01 | Hoop selection, manual and automatic (smallest that fits) | Done | Manual, or **Smallest That Fits** (`embroidery.hoop.auto`): the smallest hoop that holds the design, turned when that fits better, by brand if asked | 0 |
| HO-02 | Auto start and end (start/finish at centre or a corner) | Done | `embroidery.settings {start, end}`: the first object entered closest to a point of the design's box, the frame moved to one at the end; Design section | 0 |
| HO-03 | Hoop positioning (auto centre vs fixed), rotate hoop, custom hoops | Done | Centred or at a fixed place in the hoop (`hoopPosition`: machine files, outline and fit follow); **Rotate Hoop**; hoops of one's own | 0 |
| HO-04 | More hoops and machines; hoops of one's own where they are chosen | Done | My Machine: Pfaff creative, Brother or Baby Lock multi-needle and commercial multi-needle (DST) added; the Designer Diamond deLuxe's three included hoops first (Crown, Royal, Splendid Square); 22 more standard hoops; My own hoops in the My Machine window, joining the machine's hoops on the Home screen and in Make a Design | 0 |
| MH-01 | Multi-hooping, automatic and manual (split designs over several hoopings) | Done | `embroidery.multiHoop`: a grid of overlapping hoopings, objects whole where they fit and split stitch by stitch (tied in and off) where they don't, a machine file and a printed sheet per hooping, registration marks | 0 |
| MH-02 | Registration marks, multi-position hoops, printing per hooping | Done | Registration marks in each overlap (shape, size, count, colour: SYS-06); multi-position hoops (a file a position, no marks); a printed sheet per hooping (`multiHoop {sheets}`) | 0 |
| RP-01 | Production worksheets and approval sheets (PDF/print, customisable) | Done | Embroidery › Worksheet: production, approval, pattern and placement-template sheets (design, figures, colours, colorways, notes, barcode, placement); the shop's name, details and logo and the sections shown set once (`embroidery.worksheet.setup`); job worksheets (DM-02) | 0 |
| RP-02 | Barcodes on worksheets (Code 39, Code 128) | Partial | `embroidery.worksheet {barcode, barcodeText}`: Code 128 (set B) or Code 39 of the job at the top right, from the public standards; still to confirm with a scanner | 6 |
| RP-03 | Print colorways, colour blocks | Done | Worksheets list the colour blocks in sewing order and every colorway's threads colour by colour, the one in use marked (`embroidery.worksheet {colorways}`) | 0 |
| RP-04 | Output designs as images | Done | `embroidery.exportImage`: the stitches as lit thread (as TrueView) or flat, PNG at up to 40 pixels a mm on white, a colour or transparent; art exports as before | 0 |
| MFL-01 | Native design file | Done | `.stitchfoundry` (the reference's EMB is proprietary: Out) | – |
| MFL-02 | Condensed formats (Melco CND), colour-merge | Done | Merge Colours (`embroidery.colors.merge`): threads of the same or a near colour (ΔE tolerance) made one and the design sequenced again so their blocks join; Melco CND is not publicly described, so it isn't read or written (clean-room) | 0 |
| OUT-01 | Export for machine | Done | 10 formats | – |
| OUT-02 | Batch output profiles (machine files + design + worksheets in one step) | Done | `embroidery.output`: machine files in several formats, the worksheet as PDF and the design, named alike, to a folder in one step; output profiles kept by name | 0 |
| OUT-03 | Send to machine / USB / network queue | Done | Send to Machine writes to remembered machine folders (USB, the machine's folder, network shares), checked first; a queue manager is HW-01 | – |
| OUT-04 | Wireless transfer to specific machine brands | Out | Proprietary protocols; revisit if documented | – |
| OUT-05 | Cost estimate (stitches, thread, time, machine heads) | Done | `embroidery.settings {cost}` (per 1000 stitches, machine hour over its heads, thread per 1000 m, set-up an order, currency); `embroidery.cost {quantity}`; shown in the Design section and on worksheets | 0 |
| MDO-01 | Export vectors, appliqué cut files, bling templates, multi-decoration files | Done | **Cut File**: rhinestone templates (holes 0.3 mm over the stone) and appliqué piece outlines as SVG or DXF in mm, mirrored if asked, or opened life size to print; stone counts. PLT and machine-specific multi-decoration codes are not public | 0 |
| NET-01 | Machine networking (device setup, queues, device settings) | Out | Vendor ecosystems | – |
| DM-01 | Design library: browse folders, thumbnails, search, design info | Done | The Design Library section (`embroidery.library.scan`): a folder's machine files as stitch thumbnails with size, stitches, colours and date, searched and sorted, cached, opened with a click | 0 |
| DM-02 | Job orders, batch convert, output selected designs | Done | Job orders (`embroidery.job.*`: customer, design, quantity, due, garment, placement, notes, status; worksheet with the job's barcode and cost; files in one step); batch convert of a folder or chosen designs | 0 |
| DM-03 | Embroidery clipart (insert library designs) | Done | 16 motifs drawn in code, placed digitized in the chart's threads (`embroidery.clipart.insert`); one's own designs from the Design Library | 0 |
| CH-01 | Chenille machines (chain and moss stitches) | Done | Any stitched object chain or moss at a needle height 1–9 (`embroidery.chenille.set`; moss makes fills and runs coil or square-loop motifs), Compound Chenille (moss fill with 0–3 chain borders before and after, `embroidery.chenille.compound`), machine files with one stop per change of colour, stitch or needle height (single-stop chenille machines; the writers keep every stop), and the operator's stop list (`embroidery.chenille.stops`, panel). Left: Tajima's multi-stop (TMCE-100) code tables and condition files, not publicly described; a canvas look for chain and moss loops | 7 |
| CH-02 | Schiffli machines | Out | Separate industry; low demand |  – |
| CH-03 | Cross-stitch designer (pattern-based X stitch designs) | Done | Embroidery › Create › Cross Stitch: a grid of squares (1–300 a side, 1–10 mm) painted and erased square by square with the Cross Stitch tool (drag, brush 1–9, Alt erases, one Undo a stroke), a picture made a pattern of its main colours matched to threads (background left out), full, half, upright or double crosses; each colour one object sewn as a cross fill anchored to the pattern's grid (`embroidery.crossStitch.*`). CH-04 (2026-10-17): a fill bucket for touching squares alike, a colour replaced everywhere, squares sized by fabric count (11–18-count Aida), and a printable counted chart (a symbol a square on its colour, numbered every 10, paged, with a key of threads and stitch counts). CH-05: backstitch outlines from grid corner to corner, sewn over the crosses and drawn on the chart. Public description: https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/CrossStitch/PART_CrossStitch/PART_CrossStitch-1.htm. Left: quarter and three-quarter stitches, backstitch outlines, a printed symbol chart | 7 |

### 6.14 Newest reference features (2025–2026)

| Id | Feature | Status | Our approach / gap | Phase |
|---|---|---|---|---|
| NEW-01 | Multi Blend (stacked layers of fills with graded spacing in one object) | Done | Make Blend keeps the layers in the one fill (`FillParams.blend`: a thread and spacing profile each), sewn as one object per layer in its thread, underlay under the first; Remove Blend in the panel; `separate: true` for objects of their own | 0 |
| NEW-02 | Custom accordion profiles | Done | Presets and a curve editor in the Embroidery panel (drag, add and take out points); any points through `embroidery.setParams` | – |
| NEW-03 | Redwork | Done | SP-13 | – |
| NEW-04 | Buttonholes | Done | SP-14 | – |
| NEW-05 | Grids, guides, snapping to objects | Done | VectorCraft | – |
| NEW-06 | Continue digitizing | Done | ST-11 | 1 |
| NEW-07 | In-app assistant for embroidery questions | Partial | Embroidery › Ask About Embroidery… (desktop and web, 2026-10-17): questions answered by Claude with the user's own key, the conversation and a summary of the open design (stitches, trims, threads, hoop, Check Design findings, the selection) sent along (`sf_core::ai::ask`, `ui.askAboutEmbroidery`); Claude also changes the design by running engine commands (finding them, reading at once, each change after the user's Allow, one undo step each; files, preferences, plug-ins and windows out of reach). Left: AI credits instead of a key | A |
| NEW-08 | Contour fill robustness on tiny objects | Done | Falls back cleanly | – |
| NEW-09 | Twin-sided pull compensation | Done | RE-05 | 1 |
| NEW-10 | Batch output, barcode formats | Partial | Batch output and profiles done (OUT-02); Code 128 and Code 39 on worksheets (RP-02) still to be checked with a scanner | 1 |
| NEW-11 | Lettering min/max height display | Done | LT-13 | – |
| NEW-12 | Background exports with history | Done | Background Export (from VectorCraft) encodes off the UI thread; `export.history` remembers every file-writing export (File › Export, send, batch output, multi-hooping, convert) with its files or error; `export.again`; the Exports section | 0 |
| NEW-13 | Keyboard design collections | Done | KDC-01 | – |
| NEW-14 | Laydown stitch | Done | RE-06 | – |
| NEW-15 | Magnetic hoop presets | Done | Magnetic frames by size (100 × 100 to 350 × 350, a pocket frame) among the standard hoops | 0 |
| NEW-16 | AI text to embroidery: a prompt becomes flat-colour art drawn to digitize well, then a design | Partial | Embroidery › Design from Prompt (desktop, and the web since 2026-10-17): Claude draws flat-colour SVG with the user's own Anthropic API key (`sf_core::ai`), and `embroidery.placeArt` places it, gives its shapes stitch types with underlying fills cut (0.4 mm overlap), matches the chart's threads and brings same-coloured shapes together where the stacking allows (`embroidery.autoSequence {ids}`), in one undo step. Left: StitchFoundry accounts and credits (§11), the web app | A |
| NEW-17 | AI image or logo to embroidery: the upload is redrawn as clean flat art, then digitized | Partial | Embroidery › Redraw Image with Claude (desktop and web): the selected image goes to Claude with the user's own key (no larger than the model looks at), its redrawing lands on the image (`embroidery.placeArt {frame}`) and is digitized as NEW-16's. Left: accounts and credits (§11), the web app, AD-03 and AD-04 for redrawings that need click-to-stitch or outlines | A |
| NEW-18 | AI product mockups: the design shown on a shirt, cap or bag | Missing | §11; MD-01 | A |
| NEW-19 | Puff fill for wide areas, and the puff sewing order (a flat outline first, a stop to lay the foam) | Done | A filled shape made puff is sewn as satin across it; Sew Puff Last (`embroidery.puff.last`) moves the puff after the rest with a stop before it | – |
| NEW-20 | A library of ready-digitized embroidery fonts, marked puff, small or appliqué where they suit | Partial | Get Embroidery Fonts (desktop, `ui.fontLibrary`): Ink/Stitch's font library and the project's own (Stitchwright/stitchfoundry-fonts, once published) listed from GitHub, a font downloaded into the Embroidery Fonts folder and loaded with a click, untrusted names and sizes checked (`vectorcraft_embroidery::font_library`); not in the web demo. Left: publishing the project's own library (never in this repo), font-list badges for puff, small and appliqué | 3 |
| NEW-21 | Bubble (outline) text, and circle and spiral baselines | Done | Outline Around (`embroidery.outlineAround`): one satin or run outline round the selected art at a gap (bubble lettering, sticker borders); circles via `arc`; spirals via `spiral` (mm a turn, in or out) | 0 |
| NEW-22 | Appliqué lettering (each letter a patch: placement, tack-down, cover) | Done | Lettering `applique` (fabric colour): each glyph one appliqué patch, the union of what it sews; kept on edit, off again restores the font's stitching | 0 |
| NEW-23 | Auto basting (a basting run round the design or the hoop, first) | Done | `embroidery.settings {basting}`: a long run round the design or inside the hoop (rectangle or oval), sewn first in the first thread without ties, in previews and machine files | 0 |
| NEW-24 | Scatter copies (copies of a motif strewn over an area) | Done | `embroidery.scatter`: random (seeded) or even (triangular grid), a gap apart, inside a closed shape, optionally turned | 0 |
| NEW-25 | Placement aids: work areas, neckline placement, basting and printed placement marks | Done | Product Visualizer `areas`: the usual work areas of the side shown drawn dashed and named; the distance below the neckline on shirts and hoodies (`belowNeckMm`); life-size placement templates (`embroidery.worksheet {kind: template}`: centre lines, up arrow, 100 mm check bar); usual placements printed (`placement`); basting (NEW-23) | 0 |
| NEW-26 | Design notes kept with the design and printed on worksheets | Done | *Design notes* in the Design section (`embroidery.settings {notes}`), saved with the design, printed wrapped on worksheets | 0 |
| NEW-27 | Echo and background quilting round objects | Done | Outline Around `kind: echo` (count runs, spacing apart, from the art) and `kind: stipple` (a rectangle or circle of meander round the art, its area left out, sewn first); Embroidery › Add to Design › Echo Quilting and Background Quilting | 0 |
| NEW-28 | Embossed pattern that follows a turning satin | Done | A texture on a turning satin fill is laid along the column (length along its middle, width across) and each stitch is cut where it crosses the pattern's lines (2026-10-15) | 7 |
| NEW-29 | Appliqué cover choice (satin, blanket, zigzag or one's own) and printed mirrored cutting templates | Done | Covers: satin, blanket, zigzag, triple run or none; cutting templates from Cut File, mirrored if asked, printed life size | 0 |
| NEW-30 | Catalogue print of a design folder (thumbnail sheets, lists) | Done | `embroidery.library.catalogue`: thumbnail sheets of a folder's designs in their colours with name, size, stitches, colours and format | 0 |
| NEW-31 | Fabric presets that recommend a stabiliser | Done | Each fabric names a stabiliser (tear-away, cut-away, no-show mesh, wash-away) and a topping where it wants one; shown under Fabric settings and printed on worksheets | 0 |
| NEW-32 | Help in place: F1 on a tool opens its help, a hints panel with short tips | Done | Help › StitchFoundry Help… (F1): the user guide compiled in, with topics, search and links, opened on the active tool's topic; a Tips tab with hints for the tool in hand; the Home screen starts designs by hoop, opens samples and walks through a first design | 0 |
| NEW-33 | True-size zoom (calibrate the screen so 100 % is life size) | Done | View › Calibrate Life Size…: a 100 mm ruler and a bank card drawn at the screen points per inch being tried, to match with real ones; kept as the `screenPointsPerInch` preference, with Display Print Size at 100% Zoom on, so View › Actual Size is life size (`ui.calibrateLifeSize`) | 0 |
| NEW-34 | A simple workspace and a start screen by experience (beginner, digitizer, business) | Done | Home asks How do you work? (Window › Experience, `window.experience`): beginner the Embroidery Basics workspace (basic tools, the panel's everyday sections), digitizer Digitizing (every digitizing tool, no orders or batches), business Embroidery Business (everything); the Embroidery panel shows its sections by level | 0 |
| NEW-35 | Send to machine: hot folder, USB stick or network share, with a connection check | Done | Embroidery › Send to Machine (`embroidery.send`): the machine file written to a remembered folder, checked first | – |

### 6.15 Beyond the reference

| Id | Feature | Status | Notes |
|---|---|---|---|
| BEY-01 | Radial digitizing widget (Embroidery Legacy style) | Done | Brushes, shapes, widths, densities, lengths, sew before/after, snapping |
| BEY-02 | Any vector object carries stitches (Ink/Stitch style) | Done | |
| BEY-03 | Agents digitize through MCP / control channel | Done | Every feature is a command |
| BEY-04 | Runs in the browser | Done | The web app tried in Chromium (WebGL2): Make a Design, stitches drawn, Watch It Sew; Save for My Machine downloads the machine file, Make Every Size a zip, Save Design the design file (2026-10-15) |
| BEY-05 | Ink/Stitch-inspired extras: tartan fill, linear-gradient fill, auto-route satin, cutwork | Done | Tartan Fill (`embroidery.tartan`: the sett as warp and open weft fills per colour), Blend from Gradient (`embroidery.gradientBlend`: a linear gradient as a Multi Blend along it), Cutwork (`embroidery.cutwork`: lines split by direction for up to four cutting needles), satins routed with Branch; written from Ink/Stitch's public descriptions, https://inkstitch.org/docs/ (2026-10-15) |
| BEY-06 | Open Ink/Stitch designs with their stitch settings | Done | An SVG with Ink/Stitch's `inkstitch:*` settings opens stitched: dashed strokes run (bean stitch, repeats), satin columns between their rails with their rungs as inclination lines, other strokes satin lines, filled shapes fills (method, angle, spacing, stitch length, underlay); the settings are kept with the art (public documentation: https://inkstitch.org/docs/params/) |
| BEY-07 | 3D puff (satin over foam, capped ends; puff lettering) | Done | `embroidery.puff` / Embroidery › 3D Puff: dense satin without underlay, sides pushed out, end caps past the ends; lettering keeps it (`puff`); public descriptions: https://wilcom.com/resources/blog/qa-from-3d-puff-embroidery-webinar, https://hatchembroidery.com/resources/blog/how-to-digitize-for-3d-puffy-foam-machine-embroidery |

**Tally:** 250 rows (grouping the roughly 300 features the reference lists): 227 done,
8 partial, 9 missing, 6 out of scope (counted 2026-10-12, after the Hatch and Chroma comparison). Most of what is missing is breadth (more stitch
effects, the lettering suite, multi-decoration, production tools); the core digitizing loop is
in place.

---

## 7. Designs for the large missing pieces

### 7.1 Stitch editing and machine functions (SE-01…04)

- Add `Embroidery.frozen: Option<FrozenStitches>`: the object's own stitches (mm, relative to
  the node's transform at freeze time) plus inserted functions. When present the planner uses
  them instead of generating. Editing the outline asks: regenerate (drop edits) or keep.
- **Stitch Edit tool:** shows needle points of the selected object; click/marquee selects
  stitches, drag moves them, Alt-click inserts, Delete removes; Shift extends. Each gesture is
  one `embroidery.stitches.edit {id, ops}` command (Begin/Preview/Commit).
- **Machine functions:** `embroidery.function.insert {id, at, kind: trim|stop|colorChange|jump}`
  and `.clear`. Functions live in the frozen list, or as object-level properties
  (`trim_after`, `stop_after`, tie-in/tie-off) when the object isn't frozen.
- **Stitch List panel:** virtualised table of the plan (index, x, y, function, object),
  synced with the selection and the player.
- Tests: edit, undo, transform a frozen object, export round trip, regenerate.

### 7.2 Branching and travel optimisation (CN-06, CO-05)

- Branching turns a group of objects into one sewing path: order the objects and route
  connectors as running stitches **inside regions that are sewn later** (so they get
  covered), falling back to jumps/trims.
- Model: a `branch` flag on a group; the planner builds a graph of object entry/exit
  candidates, solves an open travelling-salesman path (nearest neighbour + 2-opt) under the
  constraint that paint order between overlapping objects is kept, and emits travel runs
  through later-covered areas.
- Redwork (SP-13: entry = exit, tour of outlines with doubled travel) is its own group type:
  built (phase 2) as `StitchType::Redwork`; the planner sews the group's lines as one object
  routed in sf-core (`engine/redwork.rs`).
- Built (phase 1): a branch is a group with the stitch type `Branch`. The planner orders its
  objects greedily (same thread first, then the nearest, overlapping objects in paint order;
  no 2-opt yet), aims each object's exit (when it has no stop bead) at its point under the
  objects still to sew that is nearest to the next one, and routes travel as the shortest
  way inside those objects (`sf_core::travel`: visibility graph over the covering area's
  bending corners, 0.4 mm in from its edge), in 2.5 mm running stitches. No covered way →
  tie off and trim as before.

### 7.3 Pattern library: program splits, user splits, carving stamps (PS-02…04)

- A program split is a 2D tile of needle-penetration offsets applied to each tatami row. Store
  as data (`PatternDef { tile_w, tile_h, points }`), 40+ original patterns generated in code
  (bricks, waves, weaves, diamonds…).
- User-defined split: split lines drawn on the object; rows break where they cross a line.
- Carving stamp: a shape (or stamp library entry) whose outline forces penetrations on fills
  under it, producing embossed lines.
- All three plug into `engine::fill::row_points` as "penetration rules".

Built (P2.1, 2026-10-12): split lines and pattern lines both become crossings of each tatami row,
found through segments bucketed by row band (`engine::fill::Splits`). Drawn lines keep the tatami
grid and add a needle point at each crossing (grid points closer than the shortest stitch move
out of the way); a texture (programmed splits) uses the crossings alone, with grid points only
where the gap between two lines is too long for one stitch. Patterns are lines in a unit tile
(`fill::tile`), tiled from a fixed origin and capped at 400,000 segments per fill. Split lines live
on the object (`Embroidery.splits`, host coordinates, transformed with it), the texture in its fill
parameters (`FillParams.texture`). Satin fills, turning fills and underlay leave them out.
Still to do: the 40+ pattern library, a stamp library, splits that follow the shape (flexi
splits without a drawn curve), and a tool for drawing split lines on the object.

### 7.4 Multi Blend and accordion profiles (SP-09, SP-10, NEW-01…02)

- An object holds `layers: Vec<BlendLayer { thread, stitch: FillParams, spacing_profile:
  Curve }>`; each layer is a fill whose spacing follows its profile across the object (in the
  stitch direction), so stacked layers blend colours.
- Profile editor: a small curve widget (presets: linear, ease, step; flip).
- Sewing order: lighter layers first, underlay once.

### 7.5 Sequins and beads (SQN-*, BD-01)

- New stitch types `Sequin(SequinParams)` and `Bead(BeadParams)`: palette of sizes/shapes
  (round 3–9 mm, square, star, custom from a vector), drop spacing, fixing stitch pattern
  (single, double, tri), device number.
- Runs: sequins along a path at spacing ≥ sequin size. Fills: hexagonal or grid packing inside
  a shape (borders: offset run of sequins; fancy: motif-shaped; multi: several sizes; split:
  half-overlap; flip: alternating faces).
- Machine output: sequin drop commands. Add the codes to `sf-formats` where formats support
  them (Tajima TBF/DST variants, Barudan, ZSK); otherwise export a sequin layout PDF/SVG.
- Preview: draw sequins as discs with specular highlight under/over stitches.

### 7.6 Bling / rhinestones (BL-01, MDO-01)

- `Bling` objects: stone palette (SS6–SS30 sizes, colours), placement along paths
  (runs/borders), in shapes (fills: grid, offset rows, contour), manual placement, lettering.
- Output: template cut files (SVG, DXF, PLT/HPGL) with hole diameters + stone counts; not
  sewn, so excluded from machine files.

### 7.7 Team names and monograms (TM-*)

- Team names reuse VectorCraft **Variables** (data-driven text): a lettering object bound to
  a column of a CSV/XLSX table; Output produces one machine file per row (or a multi-head
  batch), name-based file names, and a worksheet listing.
- Monograms: templates (our own designs) with 1–3 letter slots, letter styles (font recipes),
  ornaments and borders; a Monogram panel builds them.

### 7.8 Embroidery font recipes and font editor (CL-02…04, LT-08, LT-13)

- A **recipe** is per-glyph embroidery structure derived from an outline font and refined by
  hand: columns (two rails + rung guides), fills with angles, run details, entry/exit, min/max
  recommended height, small-text variant. Stored as JSON data files (our own, attributed in
  `ASSETS.md`), keyed by font family + glyph.
- Without a recipe, lettering keeps today's automatic conversion. With one, conversion uses
  it, scaled to the text size.
- Font editor: open a glyph, adjust its columns/guides with the Inclination and direct
  selection tools, save into the recipe. "Merge fonts" combines glyph sets.

### 7.9 Photo digitizing (PH-01…03)

- Photo line embroidery: grayscale → parallel run lines whose spacing/zigzag amplitude follows
  darkness (density modulation), in one colour.
- Coral / single run: one continuous run following a space-filling or stippled path whose
  density follows darkness.
- Colour photo stitch: quantise to N thread colours (Lab, nearest chart threads), dither, and
  stitch each colour as short-stitch tatami fragments, lightest first.
- All outputs are ordinary objects (editable), from `embroidery.photoStitch {mode, …}`.

### 7.10 Multi-hooping (MH-*)

- Choose hoop, overlap and split direction; the design is cut into hoopings by region (whole
  objects where possible; long objects split along gaps). Each hooping gets registration marks
  (small crosses sewn first and last), its own start/end, and its own machine file
  (`name-1.dst`, `name-2.dst`), plus a placement sheet.

### 7.11 Worksheets and reports (RP-*)

- `embroidery.worksheet {kind: production|approval, colorway?, format: pdf|png}` renders through
  VectorCraft's PDF writer: design image (TrueView), size, stitches, trims, colours table with
  chart codes and stitch counts per colour, sewing sequence, hoop, fabric, notes, barcode
  (Code 39 / Code 128 generated in code), cost.

### 7.12 Design library (DM-*)

- An index (SQLite-free: a JSON cache) of folders the user adds: thumbnails rendered from
  machine files and native documents, stitch/colour/size metadata, search and filters, batch
  convert and output. Desktop only (web: drag-drop folders).

---

## 8. Roadmap and estimates

Estimates are in **agent-days**: one Claude Code session working steadily for a day (about
6–8 hours of tool use), including tests, UI, translations and verification. They assume the
pace of the work so far (the current state took about 2 agent-days) and the existing
VectorCraft base. With two or three agents in parallel on separate crates (one git worktree
and target dir each) calendar time drops by roughly half.

| Phase | Scope (matrix ids) | Agent-days |
|---|---|---|
| **0. Ground truth** | Push repo, CI on GitHub; owner test-sews the sample files on their machine(s); fix what the sew-out shows. First sew-out 2026-10-17 on a Designer Diamond deLuxe, block lettering and a fire truck on a T-shirt, 360 × 200 hoop: the front sewed well; the back showed 275 trims (LT-15), ragged serif tips (LT-15), the knit sewn with woven settings because nothing asked (OP-06), and the machine's own tension and bobbin-cut troubles | 0.5 + sewing time |
| **1. Digitizing core parity** | MF-01…03 machine profiles; SE-01…04 stitch editing + Stitch List; ST-03, ST-05, ST-09, ST-11; OP-02…04 presets, fabrics, styles; CO-03, CO-05; RE-01…05; SQ-01, SQ-04, SQ-07, SQ-09; CN-02, CN-03, CN-06 branching; RS-04; PT-04; BAS-13, BAS-14; SYS-02…08 | 5–7 |
| **2. Stitch effects library** (done 2026-10-12) | PS-02…04; CS-02, CS-05, CS-07; MO-01…06; SP-02…14 (incl. Multi Blend, accordion profiles, redwork, buttonholes); ST-02; RE-06 laydown; PT-01, PT-02, PT-05, PT-06; RS-02, RS-03, RS-06; BAS-15 | 5–7 |
| **3. Lettering suite** | LT-03, LT-05, LT-08…10, LT-13, LT-14; font recipes and editor (CL-02…04); team names (TM-01, TM-02); monograms (TM-03); KDC-01; puff (BEY-07, NEW-19); a font library (NEW-20); bubble and spiral text (NEW-21) | 6–8 |
| **4. Multi-decoration** (done 2026-10-13) | Sequins (SQN-*), beads (BD-01), bling (BL-01), appliqué completion (AP-02…05), cording/boring (MD-02, MD-03), virtual decoration (MD-01), chenille-type patterns (SP-11), multi-decoration outputs (MDO-01); appliqué lettering and cover choice (NEW-22, NEW-29) | 5–7 |
| **5. Auto and photo digitizing** (done 2026-10-13) | AD-03, AD-04, BM-02, BM-03, PH-01…03, SE-06 | 3–5 |
| **6. Production and management** (done 2026-10-13 but for MF-04 and RP-02, which wait on the owner) | Colorways and visualizer (CW-*); worksheets, barcodes, images (RP-*); batch output (OUT-02); cost (OUT-05); hooping and multi-hooping (HO-*, MH-*); more formats (MF-04, MFL-02); hot folders (OUT-03); design library (DM-*); templates and clipart; kiosk (KI-01); background exports; auto basting, placement aids, design notes, catalogue print, stabiliser advice, send to machine (NEW-23, NEW-25, NEW-26, NEW-30, NEW-31, NEW-35) | 6–8 |
| **R. Release readiness** | Rebrand done 2026-10-11 (publisher SiteSentrix; moved to Stitchwright 2026-10-15, R9): StitchFoundry by Stitchwright, an original icon, app id `com.stitchwright.stitchfoundry`, its own preferences folder (reads VectorCraft's once), the ArtCraft name, logos and links removed, presets and profiles renamed with their old names still accepted; the repository is public at github.com/Stitchwright/StitchFoundry, and CI (`.github/workflows/ci.yml`) runs `cargo xtask ci` on every push to main and every pull request; the Brother and Janome thread palettes are credited to pyembroidery (MIT) in `NOTICE`. R10 (2026-10-15): unsigned builds for Windows, Linux, macOS and the web on demand (Actions › Builds, `.github/workflows/builds.yml`), and the web app published on GitHub Pages from every push to main (`pages.yml`, https://stitchwright.github.io/StitchFoundry/; the owner turns Pages on once). The programs are `stitchfoundry` and `stitchfoundry-cli`, and the packages, installers and release files are named `stitchfoundry` too, so both apps install side by side (the Cargo packages keep VectorCraft's names). Designs save as `.stitchfoundry` (VectorCraft documents inside; `.vectorcraft` and `.drawcraft` still open), and macOS knows the type. Left: the web build's file names; VectorCraft's packaging and platform workflows, which only run in its repository; the maintainer address in the Linux packages (still SiteSentrix's until Stitchwright has one); signed installers; website and download page; a community space | 2–3, plus the owner's Apple and Windows developer accounts |
| **A. AI service** | The cloud AI features ([§11](#11-open-source-and-ai-features)): accounts, credits and billing behind a small cloud backend; text to embroidery (NEW-16) first, then image or logo to embroidery (NEW-17, pulling in AD-03 and AD-04), the assistant panel (NEW-07) and product mockups (NEW-18); bring-your-own-key for every AI feature. Landed 2026-10-11 with your own Anthropic API key (desktop): text to embroidery (Embroidery › Design from Prompt) and image or logo to embroidery (Redraw Image with Claude). CLOUD-01 (2026-10-18): the cloud service, `apps/stitchfoundry-cloud` ([cloud.md](cloud.md)): accounts (Argon2id passwords, session tokens), credits held then charged by what each request used, Stripe Checkout and its signed webhook, AI requests sent with the service's key. Next: the app's sign-in and credits (CLOUD-02), then hosting it, which needs the owner's Anthropic, Stripe and hosting accounts and prices | 8–12, plus the owner's payment, AI provider and hosting accounts |
| **TB. Tablets** | TB-01 (2026-10-16): the web app's first start on a touch tablet is in simple mode; choosing another way of working is kept. Left: a layout of its own for phones, bigger touch targets outside simple mode | 1 |
| **E7. Words** | E7 (2026-10-16): Embroidery › Add Words… in every way of working, with Along the selected line (words on a path); type made with the Type tools in an embroidery design sews at once as satin, one undo step | 1 |
| **CMP. Compare** | CMP-01 (2026-10-16): Embroidery › Compare Designs… shows two designs side by side (a machine file or the design open now), with stitches, trims, jumps, colour changes, stops, size, thread, time and stitch lengths for each and how much they differ (`embroidery.compare`). One over the Other (CMP-02, 2026-10-17): both drawn on one picture at the same scale, centred, in blue and red. Left: comparing stitch by stitch | 1 |
| **E. Easy to use** (ahead of the rest of 7) | For someone who isn't at home with computers: a beginner walkthrough with screenshots and its sticking points (E1), a simple mode with a short menu and a few big labelled tools (E2), a step-by-step Make a Design guide from words, a picture, a shape, appliqué, a quilting block or a sample through hoop, colours and watching it sew to saving (E3), My Machine set once and Save for My Machine to a USB stick (E4), plain words and larger text (E5), mistakes made safe: undo, start over, autosave (E6). Landed 2026-10-14: E4 (File › My Machine… sets the machine, its hoops, VP3/PES/JEF/EXP/XXX/DST, inches or millimetres and the USB folder; Save for My Machine writes there in one step) and E2 (New to Embroidery turns on simple mode: Select, Add Words, Add a Shape, Add Appliqué, Add a Picture, Watch It Sew, Undo, Save for My Machine; Your Design in place of the panels; short menus); then E3 (Make a Design, Step by Step: what to make, the hoop, the words, shape, picture or sample, the thread colours, watching it sew and saving, `embroidery.makeDesign`); E5 (simple mode: plain tab titles, no keyboard hints, a Text size control: 1×, 1.25×, 1.5×) and E6 (Undo and Redo, Start Over with a question and Undo, designs saved automatically in place or in My Designs, `file.keepSafe`, alongside Data Recovery). The E-series is done but for a sew-out by its user | 3–5 (done) |
| **7. Hardening and extras** | Real-machine tuning from sew-outs, performance at 100k+ stitches (PF-01, done 2026-10-15: `stitchfoundry-cli perf --embroidery 120` times a 155,000-stitch design; planning every stitch 1.8 s → 0.26 s from a memoised inclination search and shapes converted on several threads, a change or a move re-planned in about 12 ms, the canvas mesh rebuilt only where the stitches changed and placed on several threads for a pan, thread drawn flat once thinner than two pixels; dev-build timings), web app embroidery pass, translation review, chenille machines (CH-01, done 2026-10-15) and the cross-stitch designer (CH-03, done 2026-10-15), assistant panel (NEW-07), Ink/Stitch-inspired extras (BEY-05), Multi Blend as one object (NEW-01); scatter copies, echo quilting, embossed turning satin, help in place, true-size zoom, a simple workspace (NEW-24, NEW-27, NEW-28, NEW-32…34) | 5–10, spread over the calendar |
| **K. In-the-hoop projects** (after E) | Projects sewn in the hoop for the little seasonal bench pillows and more, our own designs and wording: K1 a bought design's steps listed by its thread names, resizing asks first on designs that stop for fabric; K2 `embroidery.ith.make`: pillow fronts (8″ square, 9½ × 5½″) as a 5×7 panel plus cut borders and an envelope back, and mug rug, coaster and ornament finished in the hoop (placement, batting and top, design, backing with a trim stop, satin edge), each step a named thread; K3 the supplies and steps as the design's notes, printed on its worksheet (Print the Steps); K5 An in-the-hoop project in Make a Design. Landed 2026-10-14. K4 ribbon (a hanging loop, or a band across a pillow front), clear vinyl on top and a button mark as steps of their own, and cutting templates life size on Letter pages (`embroidery.ith.templates`). The K-series is done | 2–3 (done) |
| **T. Tile quilting** (block by block on an embroidery machine) | Background quilting for clear marking tiles, our own patterns: T1 `embroidery.quiltBlock`: a block of any size quilted edge to edge in one running thread at a fixed scale (stipple, swirls, loops, feathers, pebbles, waves, squares spiral, crosshatch), lines half a spacing in from the edge so neighbouring blocks read as one; T2 `embroidery.quiltBlock.set`: one file per block size, named by size, for the usual sizes 2 × 2″ to 9 × 14″ or your own list, and border strips 1 × 4″ to 3 × 10″ (`kind: borders`); T3 Quilting for a tile in Make a Design. Landed 2026-10-14. T6 corner squares (a border's row bent round a quarter circle) and quilting round words, a shape or appliqué in the middle of a block (stipple leaves it out, row patterns follow its outline). Next: the owner's exact tile sizes, a block's pattern round appliqué or words | 2–4 |
| **Q. Longarm quilting** (after 7; Q1–Q7 landed 2026-10-15: `quilt.*`, Embroidery › Special Machines › Longarm Quilting and the panel's section; left: the machines' own formats, not publicly described) | Continuous-line designs for computerized longarms: a continuous-line check that joins the pieces (Q1), start and end points on the edge (Q2), an edge-to-edge (pantograph) builder with rows that nest (Q3), block, border and corner layouts (Q4), continuous stipple, meander, echo and stitch-in-the-ditch (Q5), an HP-GL PLT writer (Q6), quilting export presets: single-stroke DXF and SVG (Q7). The machines' own formats (QLI, HQF, IQP, BQM, CQP, PAT, SSD) have no public description, so they wait for one; DXF, SVG and PLT reach the quilting software that writes them | 4–6 |

**Status (October 2026):** phase 1 is complete, every item in its row (machine profiles of your
own and edited standard ones, MF-02 and MF-03, came last). Phase 0 is still open: nothing has been
sewn on a real machine yet; the owner has the VP3 test kit. Phase 2 started on 2026-10-12 with
tatami textures and split lines (PS-02…04, CS-05), then hand stitch, jagged edges and trapunto
(SP-02, SP-03, SP-08), then motifs (MO-01, MO-03, MO-04), then offset, zigzag, E-stitch and stipple fills and laydown
(CS-02, SP-05, SP-06, RE-06), then spacing profiles and colour blends (SP-09, SP-10, NEW-01,
NEW-02), then outlines and borders (PT-01, PT-05), then repeats and envelopes sewn as they look (PT-06,
RS-03, BAS-15), then the design's own motifs (MO-06), sculpture runs and cross-stitch styles (ST-02,
SP-12), redwork (SP-13), buttonholes (SP-14), rings and outlines round lines (PT-02, RS-02, PT-05),
more textures and paired motif fills (PS-02, MO-04), graded motif columns (MO-05), string stitching
(SP-04), the automatic knife (RS-06), 3D motif fills (MO-04) and flow lines, the liquid effect
(CS-07), a library of carving stamps (PS-04), a repeat preview (BAS-15), 40 textures (PS-02), the
Motif tool (MO-01), split lines that follow the shape (CS-05), My Motifs across designs (MO-06),
buttonholes from the Embroidery panel (SP-14, NEW-04) and a curve editor for spacing profiles
(NEW-02). Phase 2 is complete but for one refinement moved to phase 7: a Multi Blend kept as one
object (NEW-01; Make Blend makes one object per thread).

Phase 6 (production and management) was done on 2026-10-13: colorways and the product
visualizer, worksheets of the shop's own with barcodes, stitch images and print views, batch
output with profiles, cost, hoops chosen and turned and kept at a fixed place, hoops of one's own,
multi-hooping with registration marks and split objects, multi-position hoops, the design library,
design templates and clipart, job orders, background exports with their history, a send queue for
machine folders, placement aids, colour merge and the lettering kiosk. Two rows wait on the owner
(§10): MF-04, more machine formats (HUS and VIP), and RP-02, the barcodes checked with a scanner.

**Order (owner, 2026-10-11):** 0 → R → A → 2 → 3 → 4 → 5 → 6, with 7 alongside: the AI
service comes right after the public launch because it pays for the project.

**Totals:** feature breadth comparable to the reference suite's Designing level (phases 1–6)
is about **30–42 agent-days**: roughly 6–8 weeks with one session a day, or 3–5 weeks with
parallel agents. Production-grade stitch quality needs real sew-outs on real fabric; plan
**another 1–2 months** of weekly test-sew → fix cycles alongside, driven by the owner's
machine. A few items will not reach parity by design (proprietary EMB files, the reference's
licensed fonts and content, vendor-locked machine networking).

Suggested parallel split (each with its own worktree and target dir): an **engine** agent
(sf-core stitch types and effects), a **lettering** agent (fonts, team names, monograms), and
a **production** agent (formats, worksheets, hooping, library), with one integrating agent
owning the engine commands and UI.

---

## 9. Verification and the quality bar

Every task is done when:

1. Engine command(s) with params documented, `enabled` checks, and tests (behaviour,
   undo, bad params, round trip through save/open).
2. Untrusted input stays safe: importers and parsers extend the fuzz tests
   (`crates/sf-formats/tests/robustness.rs`, `engine/tests/import_fuzz.rs`).
3. UI: tool/panel/menu entries; every new string translated (run the i18n tests; dump the
   missing strings with `VECTORCRAFT_I18N_DUMP=…` and fill the `.tsv` catalogs).
4. Looked at in the running app (Appendix A): screenshot of the feature on a realistic design.
5. `cargo xtask ci` passes; docs updated ([`embroidery.md`](embroidery.md), this spec's
   status column).

**Stitch quality checks** to automate as we go (property tests over generated shapes):
stitch length within machine limits; no stitch longer than the satin split length unless
"preserve long stitches"; density (stitch length per mm²) within ±15 % of the setting; no
penetrations outside an object by more than its pull compensation + 0.1 mm; satin rungs
cross the column (rung length ≤ 1.6 × thickness); no zero-length or duplicate stitches;
connectors over the trim threshold always trimmed.

**Format checks:** each writer round-trips through our reader; files are read by an
independent library (pyembroidery, scratch venv only) with the same stitch, trim and colour
counts; files that library writes open correctly here. Real-machine sew-outs before
declaring a format production-ready.

**Golden designs:** keep a small set of original test designs (logo, lettering at 6, 10, 25 mm,
appliqué, satin-heavy badge, large fill) as `.vectorcraft` files in `crates/engine/tests/`
(original work, small) and check stitch counts and checks above in CI.

---

## 10. Decisions for the owner

1. **Machines — answered 2026-10-11:** the owner sews on a Husqvarna Viking Designer Diamond
   deLuxe (VP3; Designer Splendid Square 120 × 120, Crown 260 × 200 and Royal 360 × 200 hoops
   included, up to 360 × 350 mm; automatic jump stitch trim cuts the top thread). VP3 comes
   first; its hoops are in the hoop list (as "Husqvarna 120 × 120", "260 × 200", "360 × 200"
   and "360 × 350"), and the phase 0 test kit is VP3 for this machine.
2. **Names — decided 2026-10-11:** this app, the embroidery digitizing software, is
   **StitchFoundry**, published by **Stitchwright** (the GitHub organization `Stitchwright`;
   decided 2026-10-15: the sewing and cutting software has its own organization, and SiteSentrix,
   which published it until then, keeps its other products).
   ("StitchForge", the working name, is taken in this niche: stitchforge.com is a pre-launch AI
   image-to-embroidery service.) Planned siblings: **StitchBench** and **StitchShop** (running an
   embroidery business: quotes by stitch count, orders and approvals, mockups, production
   schedule, worksheets, costing; phase 6's production items move there), connected by
   **StitchHub** (a working name: "StitchHub" is taken by a cross-stitch app with its own .hub
   format). Check the domains and run trademark searches before launch. Phase R has rebranded the
   app and its programs (`stitchfoundry`, `stitchfoundry-cli`); internal names (crates `sf-*` and
   `vectorcraft-*`, the Cargo packages `vectorcraft` and `vectorcraft-cli`) stay.
3. **Open source — decided 2026-10-11:** StitchFoundry is free and open source, everything in it;
   cloud AI features are planned on optional credits, and work free with your own AI keys; see
   [§11](#11-open-source-and-ai-features).
4. **Priorities:** which phase matters most to your work (for example lettering and team
   names for a shop, multi-decoration for fashion, photo stitch for gifts)?
5. **AI assistant:** do you want an in-app "Ask" panel that calls Claude (needs an API key)?
6. **HUS (MF-04):** Husqvarna's older HUS format can be written from its public description, but
   its compression has no public test files to check against. Add it marked "experimental", or
   leave it out until a HUS file sewn on a machine can confirm it?
7. **VIP (MF-04):** the only public description of VIP's colour key is on a page marked "All
   Rights Reserved"; it is not copied without your approval. Leave VIP out, or find another
   source?
8. **Barcodes (RP-02):** worksheets print Code 128 and Code 39 from the public standards; they
   still need checking with a real barcode scanner. Scan a printed worksheet when you can.

---

## 11. Open source and AI features

Decided by the owner on 2026-10-11: StitchFoundry is free and open source, and stays that way.

- **Everything is open source:** the app, the AI features and the server behind them, under the
  workspace licence (MIT OR Apache-2.0). No editions, licence keys or feature gates.
- **AI features (phase A)** run in the cloud on optional credits, or free with your own AI keys.
  The app is complete without them. In build order:
  1. **Text to embroidery** (NEW-16): a prompt becomes flat-colour art drawn in a style that
     digitizes well, then a design. Easiest to make sew well, because we choose the art style.
     Built with your own key (Embroidery › Design from Prompt, desktop): Claude draws the art as
     SVG, filled shapes only, so no image model or tracing is needed, and `embroidery.placeArt`
     gives the shapes stitch types. The cloud credits come next.
  2. **Image or logo to embroidery** (NEW-17): the upload is redrawn as clean flat art, then it
     is digitized; needs AD-03, AD-04 and good test-sews. Built with your own key (Embroidery ›
     Redraw Image with Claude, desktop): Claude sees the image and draws it anew as SVG, which
     lands on the image. With your own key a line asks users to redraw only artwork they may use;
     the cloud version adds the terms of service.
  3. **Assistant** (NEW-07): Claude answers embroidery questions and edits the design by running
     engine commands (every feature is already a command). Answering is built with your own key
     (Embroidery › Ask About Embroidery…, desktop), with a summary of the open design, and so is
     editing: Claude runs commands, each change after the user allows it.
  4. **Product mockups** (NEW-18): the design shown on a shirt, cap or bag, for selling to
     customers.
  5. **Check My Sew-out** (SO-01, decided by the owner on 2026-10-18): photos of a finished
     sew-out are diagnosed and the design fixed, as was done by hand for the first sew-out
     (phase 0). Requirements:
     - **In:** photos (front, back, close-ups; a ruler or coin for scale), the design that was
       sewn when it is open, and a few taps for what a photo can't show: machine, fabric,
       stabilizer, anything that happened while sewing (thread broke, bobbin ran out, hoop
       knocked). Photos alone still get tension, stabilizer and fabric advice.
     - **The app adds:** the expected look rendered from its own stitches, each object's stitch
       type, density, compensation, overlaps, order and trims, the fabric, hoop and threads, and
       Check Design's findings.
     - **Out:** what is wrong and why, in plain words, split into what the design can fix and
       what the machine or sewer can (tension, needle, stabilizer); the design fixes proposed as
       engine commands, each run only after the user allows it (as Ask About Embroidery).
     - **Price:** 3 free checks a month for every account, then 10 credits a check (about cost);
       the fix itself is free; with one's own key, unlimited at Anthropic's price. The service
       counts a check as one only when it is one (photos plus the design, with the service's own
       instructions), so the free checks can't pay for other AI requests.
     - **Learning:** only with an explicit "share to improve StitchFoundry" choice, the service
       keeps the photos, the design details, what was found and what was fixed, and later asks
       "did the fix work?" for a photo of the next sew-out. People read the collection for
       patterns and change the software for everyone (as the 1 mm overlap for touching shapes
       came from the first sew-out); nothing changes itself. Shared photos are private to the
       person and the project, deleted on request, never published or committed; one shown as
       an example only with its owner's say and licence (and an `ASSETS.md` row).
  **Pricing (owner, 2026-10-18; set in the service's configuration):** a credit is 1 cent (as
  ArtCraft's); the markup on the AI's cost is 3× (a niche, professional tool: a design from the
  AI still costs about a dollar against $20–30 for a digitizer); packs from $10 (card fees on
  smaller ones are too large a share): $10 for 1,000, $25 for 2,750, $50 for 6,000; 100 credits
  on sign-up. Monthly plans (as ArtCraft's) may follow once packs sell. Hosting moves to Railway
  with a Postgres database (CLOUD-03).
  Uploads need terms of service: users confirm they may use the art, and prompts for others'
  characters or logos are refused (Design from Prompt already asks Claude to decline them; with
  your own key, Redraw Image only reminds users, as the owner decided on 2026-10-11). Server secrets (API keys, billing settings) live in
  configuration, never in the repository.
- A public GitHub repository with CI, signed release builds, a download page, and a community
  space for questions, test-sew photos and designs.
- The free benchmark is Ink/Stitch (GPL).

**Rules for the code:**

- New code is MIT OR Apache-2.0. Bundled data and assets follow `CLAUDE.md` › Assets: each has
  a row in `ASSETS.md`, and only original or openly licensed data ships (check where the PEC and
  JEF palette tables came from, and credit their source).
- Clean-room rules are unchanged (§3): public documentation only, and never install, run or
  compare against the reference suite or other commercial digitizing software.
- VectorCraft's brand licence (`docs/brand/LICENSE-brand.txt` in the VectorCraft repository)
  requires a published modified version to drop the ArtCraft name, wordmark, logos and links:
  StitchFoundry carries none of them (phase R removed them), and new code must not add them back
  (`cargo xtask brands` checks). Saying in plain text that StitchFoundry is based on VectorCraft
  by the ArtCraft team is allowed. Keep VectorCraft's copyright notices (`NOTICE`, the licence
  files).
- Sign the installers (Apple Developer Program, Windows code signing) so they open without
  security warnings.

---

## Appendix A: driving the app for screenshots

```python
# drive.py — start the app under Xvfb and drive it over the control channel (keep outside the repo)
import json, os, socket, subprocess, time
env = dict(os.environ, DISPLAY=":99", VECTORCRAFT_NO_PREFS="1", VECTORCRAFT_SYNC_RENDER="1", WGPU_BACKEND="vulkan")
subprocess.Popen(["Xvfb", ":99", "-screen", "0", "1600x1000x24"]); time.sleep(1.5)
app = subprocess.Popen(["target/debug/stitchfoundry", "--control", "7979"], env=env)
for _ in range(240):
    try: s = socket.create_connection(("127.0.0.1", 7979)); break
    except OSError: time.sleep(0.5)
f = s.makefile("rw"); n = 0
def call(method, params=None):
    global n; n += 1
    f.write(json.dumps({"id": n, "method": method, "params": params or {}}) + "\n"); f.flush()
    return json.loads(f.readline())
def run(cmd, params=None): return call("engine.execute", {"command": cmd, "params": params or {}})
call("ui.resize", {"width": 1600, "height": 1000})
run("file.new", {"width": 400, "height": 300})
run("shape.ellipse", {"x": 60, "y": 40, "width": 200, "height": 200}); run("embroidery.set", {"type": "fill"})
run("view.fitArtboard"); call("ui.screenshot", {"path": "/tmp/shot.png"})
call("app.quit")
```

`ui.click {x, y}`, `ui.drag`, `ui.key`, `ui.pointer` drive tools and widgets; methods are listed
in `crates/ui-egui/src/control.rs` and `docs/control-protocol.md`.

## Appendix B: public sources

Behaviour references (public documentation only, read for feature scope, not copied):

- EmbroideryStudio product comparison (2025 edition):
  https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Release/comparison/comparison-1.htm
- EmbroideryStudio 2026 release notes:
  https://docs.wilcom.com/embroiderystudio/28/en/downloads/ReleaseNotes.pdf
- Top new features of EmbroideryStudio 2025:
  https://wilcom.com/resources/blog/top-10-new-features-of-wilcom-embroiderystudio-2025
- Lettering overview: https://wilcom.com/explore/lettering
- Hatch Embroidery (feature scope for NEW-19…35): the feature comparison table
  https://hatch.embroideryhelp.net/v3/en/OnlineHelp/New_Release/prodiff_table/Feature_Comparison_Table.htm
  and https://hatchembroidery.com/faq/whats-new-hatch-4
- Chroma by Ricoma (feature scope for NEW-19…35): https://ricoma.com/chroma
- Florentine effect: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Decorative/curves/curves-10.htm
- Embroidery Legacy digitizing widget and Ink/Stitch documentation (feature references for
  BEY-01, BEY-02).
