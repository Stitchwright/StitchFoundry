# BERNINA Embroidery Software: public feature survey (for StitchFoundry comparison)

Compiled 2026-10-22 from public sources only (bernina.com PDFs, BERNINA's public online help and
reference manual PDF, dealer pages, app-store listings, third-party help pages). Nothing was
downloaded, installed or run apart from public PDF documents. Descriptions are paraphrased; only
feature names are BERNINA's. Source numbers in brackets, e.g. [3], refer to the **Sources** list at the end.

## Summary

**There is no "version 10" in any public source.** No BERNINA page, dealer, press release or forum
post found mentions a BERNINA Embroidery Software 10. The current product is **BERNINA Embroidery
Software 9 (V9)**, first released in 2021 and still updated: the public release notes run through
**V9.4** (header dated August 2026: new hoops and the bernette 59 machine) [1]. Treat "V10" as
**unconfirmed / not announced**. The rest of this file covers what was new in V9 and its point
releases (9.0.3, 9.1, 9.2, 9.3, 9.4).

**The engine is Wilcom's.** The V9 release notes carry a Wilcom Pty Ltd copyright (1997–2026) and a
Corel copyright for the bundled CorelDRAW SE [1]. V9 reads and writes Wilcom's EMB as a "Grade A"
format on a par with ART [2]. So BERNINA V9 is a home-market OEM edition of Wilcom's digitizing
software.

**Tiers and prices** (US list prices from an authorized dealer [4]; BERNINA itself publishes no
prices online that we could reach, and bernina.com HTML pages refused automated fetches):

| Tier | What it is | US list price (MSRP) |
|---|---|---|
| **DesignerPlus 9** (full) | Top tier: full digitizing, CorelDRAW SE 2021, BERNINA Cross Stitch and BERNINA Quilter apps, multi-hooping, all special stitches. BERNINA WiFi Device included. 104 fonts, 930 designs [3]. | **$2,899** [4] |
| **Creator 9** | Mid tier: editing plus manual and auto-digitizing, 60 fonts, 384 designs. No CorelDRAW, no Cross Stitch/Quilter, many specialty effects missing (see the tables). WiFi Device is an extra-cost option [3]. | **$1,299** [4] |
| Creator 9 → DesignerPlus 9 upgrade | | **$1,749** [4]; UK £850 [5] |
| DesignerPlus 6/7/8 → 9 update | | **$899** [4] |
| BERNINA WiFi Device (hardware dongle) | USB WiFi adapter for machines without built-in WiFi | **$449** [4] |
| **Free Trial** | 30 days, DesignerPlus features, no updates, no WiFi Device, 45 designs and 11 fonts [3] | Free |
| **ARTlink 9** | Base, free level: opens ART and other formats, scaling, hoop selection, Stitch Player, hoop templates, Auto Start and End, writes to BERNINA machines [1][6]. A bernina.com search snippet says it is unlocked after the free trial and is "no longer available"; we could not confirm that. | Free |

Third-party listings show street prices well below MSRP (for example DesignerPlus around $2,049–2,549
and Creator $993–1,299) [7][8]. These are unconfirmed and change often.

**What stands out (as BERNINA sells it)**
- **CorelDRAW SE built in** (DesignerPlus only). The vector Artwork Canvas *is* CorelDRAW, with
  tracing, clipart and photo editing. BERNINA calls this integration unique [1][3].
- **Hardware tie-ins:** WiFi send and receive straight to the B 790 PRO and B 990. On older machines
  a $449 WiFi Device does the same job. The B 990 can scan the hooped fabric with a camera and send
  the picture to the software to position the design ("Scan & Send"). "Keep Hoop Position" keeps
  the design's place in the hoop [1].
- **Specialty techniques tied to BERNINA accessories:** CutWork (cutting-blade tool), PunchWork
  (needle felting with a punch tool or kit), Couching (Free Motion Couching Foot #43), StumpWork with
  wirelines, Trapunto/raised satin, buttonholes [2][3].
- **Keyboard Design Collection** (new in V9): map bought alphabet designs, or your own, to keyboard
  keys and type with them like a font [1].
- **Design grades A–D**, shown on the status bar, tell the user how editable a file is: native
  object file, converted outline file, machine file with objects recognised, or raw stitches [1][2].
- **Windows only.** Macs need Boot Camp, which only works on Intel Macs; Parallels has limited
  support. The licence must be checked online at least every 30 days. One user may install on 3
  devices and run 1 at a time [1].
- **Mobile apps are companions only:** *Stitchout* (iPhone, watch a machine's stitching progress) and
  *Bianco by BERNINA* (free iPad drawing-to-embroidery app) [9][10]. There is no full mobile or web
  digitizer. The older BERNINA Toolbox (2017, Mac and Windows, with the BERNINA Cloud web app) appears
  discontinued [11][12].

**.ART format (what public pages say)**
- ART is V9's native "all-in-one" design file. It holds object outlines, properties, stitches,
  thread colours, a thumbnail and comments, so designs can be rescaled and edited cleanly ("Grade
  A") [2].
- There are several ART generations. V9 reads and writes "BERNINA All-in-One Vx" (extensions
  ARTx0, e.g. ART50 to ART80 per fileinfo.com), "V4/V3" ART, A730/A200 ART (some BERNINA machines
  read it directly) and **OESD All-in-One ART42**. V1/V2 ART can no longer be written. Older
  software cannot open newer ART versions [2][13].
- **Compressed:** a BERNINA blog post, as quoted in search results, says ART files are compressed
  automatically on save and decompressed on open. We could not fetch the post itself, and it names
  no algorithm [14].
- **Encryption:** no public source says ART is encrypted. **Unconfirmed either way.** (Only the
  WiFi *transfers* are described as encrypted [1].)
- **Published spec:** none found. BERNINA does not publish the format. **Unconfirmed / no spec found.**
- **Other programs:** Wilcom (whose engine BERNINA uses) lists ART (Bernina OEM) as *read only, up to
  V6; write via special arrangement* [15]. Pulse's Catalog Xpress help says other software can read
  ART but **only BERNINA software can save ART** [16]. The ART, EMB and JAN all-in-one formats can
  largely be converted into one another [2].

---

## Feature tables

Tier columns come from the official V9 brochure comparison chart [3]. ● = included, – = not
included, L = limited (release notes [1]), ○ = extra-cost option. "DP" = DesignerPlus, "Cr" =
Creator. Blank tier = not stated in the chart. In that case the feature appears in the shared
reference manual [2], which covers both tiers, so it is probably in both, but that is unconfirmed.

### 1. Digitizing tools (manual)

| Feature (BERNINA name) | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Open Object / Closed Object | Click points (corners and curves) to make an open line or a closed shape that gets stitched in the current stitch type. | ● | ● | [2][3] |
| Ellipse / Rectangle | Shape tools that make a circle, ellipse, square or rectangle embroidery object directly. | ● | ● | [2][3] |
| Block | Digitize a column (satin-type) by marking pairs of edge points, with stitches turning along the column. | ● | ● | [2] |
| Open Freehand / Closed Freehand | Draw an embroidery outline or shape freehand with the mouse or a pen. | ● | – | [2][3] ("Freehand Embroidery tools") |
| Pattern Stamp | Drop single motif patterns from a library into the design. | ● | – | [2][3] |
| Basting Stitch Marker | Insert a placement mark (a stitched reference cross or line) at a chosen angle and position, to help line things up. | | | [2] |
| Insert Graphic Marker | Place a non-stitching guide mark in the design. | | | [2] (details unconfirmed) |
| Buttonhole / Buttonholes | Digitize one buttonhole, or a row of them, with all the stitching it needs. The type can be changed in properties. | ● | ● | [2][3] |
| Cut Buttonhole Slit | Make a cutting line for a buttonhole (for the CutWork tool). | | | [2] |
| PunchWork | Make needle-felting objects for BERNINA's punch tool (an optional kit with foot #45 is needed). | ● | – | [2][3] |
| Couching (effect) | Make couching designs (a cord or yarn tacked down) for the Free Motion Couching Foot #43. Can be mixed with normal embroidery. | ● | – | [2][3] |
| Object Properties / Effects | A docker for stitch type settings, spacing, width, offsets, underlay, pull compensation and effects, per object or as defaults. | ● | ● | [2] |
| Templates (.EMT) | Saved default property sets used when a new design starts. | | | [1][2] |
| Fabrics (auto fabric) | Pick a fabric type and the software adjusts density, underlay and pull compensation to suit it. Custom fabrics can be saved. | ● | ● | [1][3] ("Apply custom or auto fabrics") |

### 2. Stitch types: outlines

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Single Outline / Triple Outline | Running-stitch line, single or tripled for weight. | ● | ● | [2][3] ("Basic outlines": 9 in both) |
| Sculpture Outline | Thin line that alternates single and triple stitches for a hand-sewn look. | ● | ● | [2] |
| Backstitch Outline / Stemstitch Outline | Hand-stitch imitations for fine borders, stems and vines. | ● | ● | [2] |
| Blanket Outline / Zigzag Outline | Narrow open borders with a comb or zigzag look. | ● | ● | [2] |
| Satin Outline | A satin column of even width along a line. | ● | ● | [2] |
| Raised Satin Outline | Multi-layer satin border for a raised ("puffy") look. Can be combined with trapunto. | ● | – | [2][3] |
| Blackwork Outline | Decorative "craft stitch" border patterns in the blackwork tradition. | ● | – | [2][3] ("Special outlines": 700 DP, none in Cr) |
| Candlewicking Outline | Ornamental knot-style border with a set pattern size and spacing. | ● | – | [2][3] |
| Pattern Run Outline | Repeat a chosen library motif along a line. | ● | – | [2][3] |
| Motif patterns from BERNINA machines (new in V9) | The machines' built-in decorative stitch motifs, made available as patterns. | ● | – | [3] |

### 3. Stitch types: fills

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Satin Fill | Satin for narrow shapes, turning with the shape. | ● | ● | [2] |
| Step Fill | Tatami-style rows for big, irregular shapes. Step patterns can be chosen. | ● | ● | [2][3] ("Basic fills": 32 in both) |
| Fancy Fill / Sculptured Fancy Fill | Textured fills that keep a solid look. Large pattern library (Appendix C). | ● | partial | [2][3] ("Special fills": 892 DP, 562 Cr) |
| Elastic Fancy Fill (effect) | The fancy pattern follows the stitch angles and scales with the shape's width. | ● | – | [2][3] |
| Pattern Fill | Fill a shape with a repeated decorative motif from the library. | ● | | [2] |
| Create pattern fills | Make your own fill pattern. | ● | – | [3] |
| Alternating pattern fills | Fill patterns whose rows alternate. | ● | ● | [3] |
| Lacework Fill | Open, trellis-like lace fill. | ● | | [2] |
| Stipple Run Fill / Stipple Backstitch Fill / Stipple Stemstitch Fill | Meandering running stitches inside a border, in run, backstitch or stemstitch form. | ● | | [2] |
| Ripple Fill | Stitching spirals out from the centre of a shape. | ● | | [2] |
| Contour Fill | Rows follow the shape's outline for a curved, light-and-shade effect. | ● | | [2] |
| Cross Stitch Fill | Open fill made of crosses on a grid shared by the whole design. | ● | | [2] |
| Blackwork Fill / Candlewicking Fill | Craft-stitch fills in those styles. | ● | | [2] |
| Raised Satin Fill | Several layers of satin for raised embroidery, also usable for lettering. | ● | | [2] |
| Carving Stamp / Carving effect | Press a preset or custom pattern of needle points into a fill (an embossed look). | ● | – | [2][3] |
| Add Stitch Angles / Remove Stitch Angles | Give one fill several stitch directions, holes included. | ● | ● | [2][3] |

### 4. Stitch effects and quality

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Underlay / Auto Underlay (multiple underlay) | Stabilising stitches under objects, set automatically or by hand. Two underlays per object are possible. | ● | ● | [2][3] |
| Pull compensation | Widens objects to make up for the fabric pulling in. | ● | ● | [2][3] |
| Auto and manual density | Automatic or manual stitch spacing. | ● | ● | [3] |
| Automatic Corners | Mitre or cap sharp corners in columns, set object by object. | | | [2] |
| Auto Jump / trims (Always trim/Never trim, Set Trim Distance) | Control jumps and trims between objects. | ● | ● | [2][3] |
| Tie-in / tie-off | Lock stitches at the start and end of objects. | | | [2] |
| Travel on edge | Hidden travel runs along object edges instead of jumping across. | ● | ● | [2][3] |
| Textured Edge | Rough, ragged edges for fur, grass and shading. | ● | ● | [2][3] |
| Star Fill | Stitches radiate from a centre point. | ● | – | [2][3] ("Star Stitch effect") |
| Wave Fill | Stitches follow a drawn guide line at an even density. | ● | – | [2][3] |
| Gradient Fill | Stitch spacing changes across the object (dense to open) using preset profiles. | ● | – | [2][3] |
| Color Blending | Two colours blend into each other across an object. | ● | – | [2][3] |
| 3D Globe Effect | Distorts a patterned fill so it looks domed. | ● | – | [2][3] |
| Morphing | Envelope-style transforms of outlines and stitches. | ● | ● | [2][3] |
| Outlines & Offsets | Make outline or offset copies of any object, open or closed, with several offsets at once (V9). | ● | ● | [1][3] |
| Trapunto (Create Trapunto Outlines) | Quilting-style outlines around raised satin for a quilted, padded look. | ● | – | [2][3] |

### 5. Lettering and monograms

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Lettering (on screen or via dialog) | Type text in embroidery fonts or TrueType/OpenType fonts. Edit it on screen or in a dialog. | ● | ● | [2][3] |
| Built-in embroidery fonts | Ready-digitized embroidery fonts: 104 (DP), 60 (Cr), 11 (trial). New in V9: Amalfi Coast, Locker, Moly, Shella, Sm Run script, Swiss Run Hollow. | 104 | 60 | [1][3] |
| TrueType / OpenType conversion | System fonts are turned into stitches when used, with auto kerning. V9 added Farsi and Arabic. | ● | ● | [1][3] |
| Baselines / lettering shapes | 6 baseline types (straight, free line horizontal or vertical, arcs, circles, envelope shapes); reshape the baselines. | 6 | 6 | [2][3] |
| Single letter editing | Move, resize, rotate, skew, reshape and recolour individual letters. Adjust letter and line spacing. | ● | ● | [2][3] |
| Recommended font sizes (new in V9) | The font list shows the minimum and maximum recommended size for each font. Recently used fonts are listed first. | ● | ● | [1] |
| Special characters / fancy lettering / flair script | Add special symbols, decorative fancy lettering and "flair" script swashes. | | | [2] |
| 3D fonts (Puffy Lettering) | Raised lettering for use with puffy foam. | ● | ● | [3] |
| Convert Artwork to Embroidery (text) | Turn vector text into satin lettering with turning stitches. | ● | | [2] |
| Keyboard Design Collection (new in V9) | Map any design (a bought alphabet, your handwriting) to keyboard keys and type with it. 5 packaged KDC fonts come with DP. | ● | – (L in release notes) | [1][3] |
| Monogramming | 1–3 letter monograms from preset styles, border shapes and ornaments. | ● | ● | [2][3] |

### 6. Editing and arranging

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Edit objects and single stitches (Stitch Edit) | Edit at object level or stitch by stitch, selecting stitches while travelling through the design. | ● | ● | [2][3] |
| Convert between stitch types | Switch any object between fill, outline and stitch types. | ● | ● | [3] |
| Insert Embroidery / combine designs | Merge other design files and lettering into the current design. | ● | ● | [2][3] |
| Group / Ungroup / Lock / Unlock All | Usual object management. | ● | ● | [2] |
| Align (left, right, top, bottom, centres) / Space Evenly Across or Down | Usual alignment and distribution. | ● | ● | [2] |
| Rotate, skew, mirror, size ±10% | Transform toolbar steps (45° rotation, 10% sizing, Mirror X and Y). | ● | ● | [2][3] |
| Rescale designs | Resize with stitches recalculated (best on Grade A files). Grade A designs are good to about ±20% scaling. | ● | ● | [2][3] |
| Reshape Object | Edit nodes, entry and exit points, stitch angles and lettering baselines. Smooth Curves removes extra nodes. | ● | ● | [2][3] |
| Break Apart | Split monograms, appliqué, lettering and blackwork runs into their parts. | ● | ● | [2][3] |
| Knife | Cut objects along a drawn line, keeping their settings. | ● | ● | [2][3] |
| Weld (new in V9) | Merge overlapping objects into one flattened shape. | ● | ● | [1][3] |
| Remove Overlaps | Delete hidden stitching under other objects or lettering. | ● | ● | [2][3] |
| Add Holes / Remove Holes / Fill Holes | Cut or fill holes in filled objects. | ● | – | [2][3] |
| Mirror-Merge (Horizontal, Vertical, H&V), Wreath, Kaleidoscope (Mirror Alternate), Array, Reflect | Make mirrored, rotated or repeated copies. Array also re-sequences colours so batches of badges stitch efficiently. | ● | ● | [2][3] |
| Duplicate / Quick Clone / Duplicate with Offset | Right-drag to clone; Ctrl+D repeats the offset. | ● | ● | [1] |
| Color Film (resequence, Optimize Color Changes, Sequence by Selects) | List of colour blocks and objects. Drag to reorder, or reduce colour changes automatically. | ● | ● | [2][3] |
| Branching (V9: fills as well as outlines) | Automatically sequences and groups selected objects with the fewest connectors. | ● | ● | [1][3] |
| Apply Closest Join | Joins objects at their nearest points; can be re-applied after edits. | ● | ● | [1][3] |
| Double Run / Blackwork Run | Reinforce outlines by running back over them; sequence outlines so they stitch with no jumps. | | | [2] |
| Auto Start and End | Set the design's first and last stitch automatically (updated in V9.2). | ● | ● | [1][3] |
| Polygon Select / Tab through objects | Select by polygon; step through objects with the keyboard. | | | [2] |

### 7. Auto-digitizing from images

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Auto-Digitize | Turns prepared bitmap artwork into fill and outline objects. | ● | ● | [2][3] |
| Instant Auto-Digitize | One-click automatic digitizing of a bitmap. | ● | ● | [2][3] |
| Prepare Bitmap (V9; formerly the bitmap artwork preparation tool) | Reduces colours and sharpens outlines before auto-digitizing, with a set colour count and level of detail. | ● | ● | [1] |
| Adjust Bitmap / Crop Background / Touch Up Bitmap | Brightness, contrast and effects; crop or remove backgrounds; open the image in Paint or Corel PHOTO-PAINT for touch-ups. | ● | ● | [1][2] |
| Image transparency | Transparent backgrounds in 24 and 32-bit images are respected. | ● | ● | [1] |
| Magic Wand / Magic Wand Fill without Holes / Magic Wand Centerline / Magic Wand Block Digitizing | Click an area of the image to make a fill (keeping or ignoring holes), a centreline run or a satin column from it. | ● | ● | [2][3] |
| Color Matching Method | Map image colours to the palette, or match them automatically to real threads. | | | [2] |
| PhotoSnap | Turns a photo into rows of single-colour satin stitching. | ● | ● | [2][3] |
| Color PhotoStitch | Turns a photo into multicolour embroidery. | ● | ● | [2][3] |
| Convert Artwork to Embroidery | Turns selected vector artwork into fill or outline objects. | ● | ● | [2] |
| Convert Embroidery to Artwork (V9: vectors or bitmaps) | Exports objects as vectors for other programs, or as a "virtual embroidery" bitmap for catalogues and social media. | ● | ● | [1] |
| Scanner support (TWAIN) | Scan artwork straight into the software. | ● | ● | [1] |
| CorelDRAW SE Artwork Canvas: Quick Trace (centerline and outline), ART Strokes, Pointiliser, Picture effects, Connect Content | Corel's tracing, artistic brush strokes, a pointillism effect and photo effects, plus Corel's Connect docker of free and paid content. | ● | – | [1][3] |
| Artwork included | Clipart for backdrops: 497 (DP), 172 (Cr). | 497 | 172 | [3] |

### 8. Appliqué

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Digitize Appliqué (formerly "Appliqué") / Advanced Appliqué | Makes a placement line, tack-down and cover stitch, with a stop or colour change between them. | ● | ● | [1][3] |
| Digitize Appliqué with Holes (new in V9) | Appliqué shapes with inner boundaries. | ● | ● | [1][3] |
| Convert to Appliqué (new in V9) | Turns any closed object into an appliqué with one click. | ● | ● | [1][3] |
| Combine Appliqué (new in V9) | Breaks apart several appliqués and re-sequences them so they stitch efficiently. | ● | – | [1][3] |
| Remove Appliqué Overlaps | Trims overlapping appliqué pieces. | ● | ● | [1] |
| Separate offsets per component (V9) | Separate offsets for the placement/cutting line, tack-down and cover stitch (trim in place or pre-cut patch). | ● | | [1] |
| More tack-down and cover stitch types (V9) | Wider choice of stitches for each component. | ● | | [1] |
| Export Cutting File (new in V9) | Exports the appliqué shapes as **SVG or EXP** for a cutting machine or BERNINA's CutWork tool. | ● | ● | [1][3] |
| Show Appliqué Fabric | Shows the fabric colour or picture for appliqué pieces. | ● | ● | [2] |
| Palette appliqué slots | The palette has 56 thread slots plus 3 appliqué fabric slots. | ● | ● | [3] |

### 9. CutWork, StumpWork, raised and 3D work

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| CutWork / StumpWork toolbox (combined in V9) | One toolbox for cutting and for 3D stumpwork. | ● | L | [1] |
| Digitize Open Cut / Digitize Closed Cut (V9) | Draw cutting lines for the CutWork blade tool, either open lines or closed shapes. | ● | ● | [1][3] ("CutWork" ● in both) |
| Digitize Hole | Cutwork holes. V9 merged the five old Hole border types into one, with a covered or raw edge. | ● | | [1][2] |
| Digitize Piece / Add CutWork Border / Convert to CutWork Border / Cut Closed Border | Make a cut-out piece with its supporting embroidery; build borders from existing objects; leave a gap for wire ends. | ● | L | [1][2] |
| Combine Selected / Un-combine | Merge cutting lines from several objects into one optimised cut. | ● | L | [1][2] |
| 3D StumpWork: Create / Open / Export Stumpwork Sub-design, Digitize Wireline | Separately stitched, free-standing 3D pieces edged with wire, kept as a sub-design that can be saved as its own file. | ● | – | [2][3] |
| Raised embroidery (Raised Satin, puffy) | See the stitch tables. | ● | | [2] |
| Eyelet embroidery via CutWork | Marketing name for cut holes with satin edges. | ● | | [3] |
| CrystalWork, PaintWork (and the older CutWork module) | **Not in V9.** These were the separate, older **BERNINA DesignWorks** suite with its own tools: crystal templates, pen drawing, cutting. | – | – | [17] |
| Crystals / bling / rhinestones in V9 | **No rhinestone tool found in the V9 brochure or manual. Unconfirmed / probably absent.** | | | [2][3] |

### 10. Quilting and in-the-hoop

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| BERNINA Quilter (separate app, launched from Applications) | Patchwork quilt designer for patches and colours. Saves .ARQ (Grade D). | ● | – | [2][3] |
| Define Quilt Block / Show Quilt Block / Auto Fit / Auto Center / Mirror-Copy to Quilt Block Corners / Center Copy / margins | Lay motifs out on a quilt block of a given size. | ● | – (Show Quilt Block) | [2][3] |
| Quilting Background (5 types) | Automatic echo, stipple or scroll background quilting around motifs. | ● | – | [2][3] |
| Define Work Area / Layout to Rectangular Quilt Block Corners / Layout around Circular Work Area | Repeat items in corners or around a circle. | ● | | [2] |
| Trapunto | See the effects table. | ● | – | [3] |
| In-the-hoop specific tools | **No in-the-hoop project wizard is named.** It is done with appliqué, placement lines and stumpwork. Unconfirmed. | | | – |

### 11. Hooping

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Hoop selection filtered by machine, with realistic display | Hoops for the chosen machine, shown as they look. Custom hoops can be added (USRHOOPS). | ● | ● | [1][3] |
| Show Hoop Template | Overlay of the printed plastic positioning template. | ● | ● | [2][3] |
| Multi-Hooping: Automatically Add Hoops / Calculate Hoopings / Add Hoop / Add Hoop Left/Right / Add N Hoops Around / Add Splitting Line / Preview Hoopings / Delete Hoop | Split large designs over several hoopings, automatically or by hand (10 mm overlap allowed). Save as one design or as separate machine files per hooping. | ● | – | [2][3] |
| Rotate Hoop / Rotate Hoop 15° (V9) | Rotate the hoop and its template, or selected hoops in a multi-hoop layout. | ● | ● | [1] |
| Keep Hoop Position (V9.2) | Keep the design at a set spot in the hoop when sent to the machine (pockets, continuous designs). The machine setting must be on too. | ● | ● | [1] |
| Hoop and presser-foot updates (V9.0.3–9.4) | Magnetic hoops, Giant hoop (B 990), Medium Border Clamp, sock hoop inserts, presser feet #15 and #72 with their stitch areas, bernette 59 hoops. | ● | ● | [1] |
| Open scanned hoop from B 990 (V9.2) | Bring the B 990's camera picture of the hooped fabric in as a backdrop to place the design. | ● | ● | [1][3] |

### 12. Thread colours and charts

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Color Palette (56 + 3 appliqué) | Numbered slots, colour wheel, pick colour, add, remove or discard unused colours, cycle used colours. | ● | ● | [2][3] |
| Thread Colors / thread charts | Pick from commercial thread charts; search by code or shade. | ● | ● | [2][3] |
| Thread matching across brands | Match colours to the nearest thread in one or more other charts. | ● | ● | [2][3] |
| Custom thread charts (.TCH) | Make your own charts, even mixing brands. | ● | ● | [1][2] |
| Print thread charts | Print a chart as a shopping list. | | | [2] |
| Which brands ship | **Not listed in the parts of the manual we checked. Unconfirmed.** (The brochure has a "BERNINA recommends threads from…" line.) | | | [3] |

### 13. File formats

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Native design (Grade A) read/write | ART (Vx = ARTx0, V4/V3, A730/A200), OESD ART42, Wilcom EMB; JAN (Janome/Elna) is read only. Templates AMT70/AMT80. | ● | ● | [2] |
| Other native | ARX (Cross Stitch) and ARQ (Quilter), written only by those apps; ARX can also be read by V9. | ● | – | [2] |
| Machine files, read and write (Grade C/D) | EXP (BERNINA USB stick, write; Melco, read and write), DST, PES, PEC, SEW, EMD, VP3, VIP, SHV (save back only), HUS, PCS, PCD, PCQ, PCM, XXX, CSD. GNC (Great Notions) is read only and becomes Grade B. | ● | ● | [2] |
| Opening machine files | Converted to objects (Grade C) by default, or kept as raw stitches (Grade D). | ● | ● | [2] |
| Convert Selected Designs (batch) | Batch-convert library designs into other machine formats. | ● | ● | [2] |
| Separate folders (V9) | Designs, machine files and cutting files are saved to separate folders. | ● | ● | [3] |
| Bitmaps | BMP, JPG, PNG read and write ("Save design as image"). | ● | ● | [2][3] |
| Vectors | EMF, EPS, WMF read; CDR read and write via CorelDRAW SE; more formats through Corel. | ● | partial | [2][3] |
| Cutting files | SVG and EXP export (appliqué and cutwork). | ● | ● | [1] |
| Export Design List | Library list exported as tab-separated or CSV. | ● | ● | [2] |

### 14. Machine connection

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Write to Card/Machine / Read from Card/Machine | Send to or read from a BERNINA machine or design card (older machines by cable with a driver, e.g. the Aurora driver). | ● | ● | [1][2] |
| Direct machine connection with ADX | Brochure line item; **ADX is not explained publicly. Unconfirmed.** | ● | ● | [3] |
| Data transfer via USB stick | Save EXP to a stick. | ● | ● | [2][3] |
| BERNINA WiFi Connector (app) | Companion program: set up WiFi Devices, send designs to several devices, list, delete or read designs on them, set memory limits, choose formats. Transfers are encrypted. It may run on a second PC. | ● | ● | [1] |
| BERNINA WiFi Device | USB WiFi dongle that plugs into the machine, for machines without built-in WiFi. | ● (included) | ○ | [1][3] |
| Send Selected Design to BERNINA WiFi Device | Send straight from the Embroidery Library. | ● | ● | [1] |
| Direct WiFi to B 790 PRO (V9.1) and B 990 (V9.2) | Send to the machine's stitch-out screen or "My designs", and open designs from the machine. Machines listed as WiFi-compatible in 2026: B 990, B 790 ULTRA, B 790 PRO, B 770 QE PRO, B 700 PRO, B 735 PRO, bernette 59 (V9.4). | ● | ● | [1][3] |
| Scan & Send via WiFi to B 990 | See Hooping. | ● | ● | [3] |
| Stitch Selected Design | Stitch out straight from the library. | ● | ● | [2] |

### 15. Visualisation

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Artistic View ("realistic preview", 3D simulation) | Shows the design as sewn thread with simulated depth. (BERNINA does not use the name "TrueView", which is Wilcom's; **not found in the BERNINA manual**.) | ● | ● | [2][3] |
| Show Stitches / Outlines / Needle Points / Connectors / Stumpwork / Bitmap and Vector Artwork, Dim Artwork | Display toggles. | ● | ● | [2] |
| Stitch Player | Plays the stitch-out on screen, in stitch or artistic view. | ● | ● | [2][3] |
| Travel by colour, object, start or end | Step through the design. | ● | ● | [2] |
| Visualize finished articles (Design > Background) | Place the design on a garment or article picture from a library (multi-colour garments) or on your own photo. This is BERNINA's product visualiser. | ● | ● | [2] |
| Overview Window, zoom 1:1, grid, rulers and guides, inch/metric | Viewing aids. | ● | ● | [2][3] |
| Print / Print Preview worksheet | Worksheet with a machine run-time estimate and colour list. | ● | ● | [2][3] |
| Design grade on status bar (V9) | Shows grade A–D. | ● | ● | [1] |
| 4K monitor support, large or small icons with tool names | UI scaling options. | ● | ● | [1][3] |

### 16. Library, cloud and sharing

| Feature | What it does | DP | Cr | Source |
|---|---|---|---|---|
| Embroidery Library | Built-in design browser: search, sort, group, folders, preview pane; print a catalogue; find designs on disk. | ● | ● | [2][3] |
| Home screen (V9) | Tutorials, skill builder, BERNINA blog, webinars, embroidery design shop. | ● | ● | [1] |
| Built-in designs | 930 (DP), 384 (Cr). Commercial use of stitched-out designs is allowed; selling the files is not, unless they are significantly modified. | 930 | 384 | [2][3] |
| Sign in / sign out licence | Use on up to 3 computers with one account; needs online checks. | ● | ● | [1] |
| Cloud storage or sharing in V9 | **None found.** V9 has no cloud library. The earlier **BERNINA Cloud** web app went with BERNINA Toolbox (2017) and appears discontinued. | – | – | [11][12] |
| Connect Content (Corel) | Free and paid graphics through CorelDRAW's Connect docker. | ● | – | [3] |

### 17. Ancillary and companion products

| Product | What it does | Platform / price | Source |
|---|---|---|---|
| BERNINA Cross Stitch | Separate cross-stitch design app bundled with DP. Saves ARX. | Windows, DP only | [2][3] |
| BERNINA Quilter | Separate patchwork quilt design app bundled with DP. Saves ARQ. | Windows, DP only | [2][3] |
| ARTlink 9 | Free viewer and transfer level (see Summary). | Windows, free | [1][6] |
| BERNINA WiFi Connector | Design transfer manager (see Machine connection). | Windows, included | [1] |
| **Stitchout** app | Watch an embroidery in progress on a connected BERNINA machine: live status, colour checklist with stitch counts and times, design view with time left, alerts when it finishes, needs a thread change or needs attention. Monitoring only, no editing. | iPhone (iOS 15+) and visionOS, free; an Android version is mentioned in search snippets (Android 10+), unconfirmed | [9][18] |
| **Bianco by BERNINA** | iPad sketching app: draw with Apple Pencil or a finger using stitch "brushes" (straight and zigzag of several widths); pressure and tilt change the stitch direction; BERNINA thread colours, shapes, handwritten or typed text; send to a BERNINA machine. Export formats not stated. | iPad (iPadOS 16+) and visionOS, free; v1.0 2023-02-15, v3.2 2025-12-12 | [10] |
| BERNINA Toolbox (2017, appears discontinued) | Modular editing, lettering and monogramming software (modules: Editing, Lettering, Lettering Pro, Monogramming, Bundle). Ran on Mac and Windows, with the BERNINA Cloud web app for tablets and browsers. MSRP $249 per module, $349 Lettering Pro, $799 bundle. A bernina.com search snippet says the Bundle is no longer available and V9 replaces it. | Mac, Windows and web | [11][12] |
| BERNINA DesignWorks (older) | CutWork, PaintWork and CrystalWork software suite and tools for older machines (B 580 E, B 750 QE E, B 780 E, B 830 E). | Windows (XP–7 era) | [17] |
| WeAllSew app (2013) | Community and blog app, not a design tool. | iPhone | search result only, unconfirmed |

---

## What was new in V9 (vs V8), and in the 9.x updates

From the release notes [1] and the brochure [3]:

- **V9.0 (2021):** 64-bit; new installation and sign-in licensing (no dongle, up to 3 PCs); Home
  screen; CorelDRAW SE 2021 (DP); new Lettering/Monogramming toolbox; Keyboard Design Collection;
  recommended font sizes; new Appliqué toolbox (Digitize Appliqué with Holes, Convert to Appliqué,
  Combine Appliqué, Export Cutting File in SVG or EXP, separate offsets per component); merged
  CutWork/StumpWork toolbox (Open Cut and Closed Cut, Convert to CutWork Border); auto-digitizing
  improvements (Prepare Bitmap, transparency, Convert Embroidery to Artwork as a bitmap); Branching
  for fills; Apply Closest Join; Weld; Outlines & Offsets for open objects; Rotate Hoop; BERNINA WiFi
  Connector and WiFi Device; design grade on the status bar; 4K support; new fonts, patterns and
  designs (930 designs, about 500 artworks, more than 100 fonts).
- **V9.0.3:** new hoops; fixes to lettering spacing, Magic Wand rotation, the Color PhotoStitch
  slider, VIP format and Quilter ARQ; Windows 11 support for the WiFi Device; machine run-time
  calculation fix.
- **V9.1:** direct WiFi send and receive with the B 790 PRO (open EXP from the machine); new hoops;
  3D Drummond font stitching improved.
- **V9.2:** WiFi with the B 990 (send to the stitch-out screen or the personal folder); open the
  B 990's scanned hoop image; B 990 and Giant hoop; **Keep Hoop Position**; updated Auto Start and
  End and hoop position controls; presser foot #72; Portuguese UI; new Aurora driver.
- **V9.3:** magnetic hoops (VIO 5, 7, 8 and 9 series), Medium Border Clamp, sock hoop inserts,
  presser feet #15 and #72, templates shown in inches.
- **V9.4 (2026):** Giant Magnetic Hoop (B 990), bernette 59 machine with its hoops and WiFi
  "Write to Machine", updated EULA.
- **"Version 10": not found in any public source. Unconfirmed / no announcement.**

## Gaps and caveats

- bernina.com HTML pages (product pages, the V8→V9 difference page, ARTlink, Toolbox, the Stitchout
  page, the BERNINA blog) returned HTTP 403 to our fetcher. Facts from them come from search-result
  snippets and are marked as such. BERNINA-hosted **PDFs** (brochure, release notes, reference
  manual, DesignWorks brochure) were read in full.
- Tier marks for features missing from the brochure chart are left blank (unconfirmed).
- Thread chart brand list, ADX, encryption of ART and an ART spec: unconfirmed.
- Prices: dealer MSRPs; BERNINA's own price list was not reachable.

## Sources

1. BERNINA Embroidery Software Release Notes V9 (header "V9 – August 2026", covers 9.0–9.4), PDF: https://softwarehelp.bernina.com/en/downloads/ReleaseNotes.pdf (also https://softwarehelp.bernina.com/en/downloads/ReleaseNotes-print.pdf)
2. BERNINA Embroidery Software 9 Reference Manual (DesignerPlus and Creator, Rev 6, July 2024), PDF: https://softwarehelp.bernina.com/en/downloads/ReferenceManual.pdf ; online help: https://softwarehelp.bernina.com/en/index.htm
3. BERNINA Embroidery Software 9 brochure, with the DesignerPlus / Creator / Free Trial comparison chart, PDF (© 2024): https://www.bernina.com/bernina/media/products/Software/Embroidery_Software/V9/Brochures/Embroidery-Software-9_Brochure_A4_EN_v2_low-res.pdf (US version: https://www.bernina.com/Bernina/media/products/Software/Embroidery_Software/V9/Brochures/Embroidery-Software-9_Brochure_EN-US_v2_low-res.pdf)
4. BERNINA of Naperville (authorized dealer), software price list: https://berninaofnaperville.com/pages/bernina-embroidery-software
5. The Sewing Studio (UK), DesignerPlus V9 Creator upgrade: https://www.thesewingstudio.co.uk/products/bernina-embroidery-software-9-designerplus-creator-upgrade
6. BERNINA ARTlink 9 page (search snippet only; page returned 403): https://www.bernina.com/Artlink ; product levels in the help: https://softwarehelp.bernina.com/en/MainHelp/PART_Release/introduction/Product_levels.htm
7. Sewing Machine Shop listing (search snippet, unconfirmed): https://sewingmachineshop.com/?p=2297
8. Techjockey listings (search snippets, unconfirmed): https://www.techjockey.com/us/detail/bernina-embroidery-software-9-designerplus , https://www.techjockey.com/compare/drawings-pro-vs-bernina-embroidery-software-9-creator
9. Stitchout app, Apple App Store: https://apps.apple.com/app/id1634379782
10. Bianco by BERNINA, Apple App Store: https://apps.apple.com/sg/app/bianco-by-bernina/id1670888263
11. BERNINA of America introduces BERNINA Toolbox (press release, 22 Aug 2017, mirror): https://www.firmenpresse.de/pressrelease557418/bernina-of-america-introduces-bernina-toolbox-software.html ; original: https://www.bernina.com/en-US/Footer-en-US/Country-Pages/Media/Press-Releases-2017/USA/BoA-Introduces-BERNINA-Toolbox-Software
12. BERNINA Toolbox Bundle page (search snippet: "no longer available"): https://bernina.com/Toolbox ; https://bernina.com/en-IN/Products-IN/BERNINA-products/Software/BERNINA-embroidery-software/BERNINA-Toolbox
13. fileinfo.com, ART version extensions: https://fileinfo.com/extension/art , https://fileinfo.com/extension/art80
14. BERNINA blog post on ART files (search snippet only; page returned 403): https://blog.bernina.com/en/?p=15976
15. Wilcom supported formats ("ART (Bernina OEM) – Read only – up to V6; write via special arrangement"): https://help.wilcom.com/portal/en/kb/articles/supported-formats
16. Pulse Microsystems, Catalog Xpress and ART file support: https://hmsupport.pulsemicro.com/support/solutions/articles/9000238695-catalog-xpress-and-art-file-support
17. BERNINA DesignWorks brochure (CutWork, PaintWork, CrystalWork), PDF: https://www.bernina.com:443/de/Resources/bernina-products/software/designworks/PDF/Brochure_DesignWorks_EN.pdf
18. BERNINA Stitchout App page (search snippet only): https://www.bernina.com/stitchout-app ; WiFi transfer and Stitchout tips page: https://bernina.com/ja-JP/イヘントや体験会-JP/Embroidery-Tips-Tricks/WiFi-Design-Transfer-and-monitoring-via-App
19. BERNINA software compatibility (search snippet: V9 on Windows 10/11, Mac via Boot Camp/Parallels; V8 DesignerPlus discontinued): https://www.bernina.com/en-US/Support-US/Software/BERNINA-Software-Compatibility/BERNINA-Software-Compatibility
20. BERNINA WeAllSew app press release (2013, search snippet only): https://www.bernina.com/en-US/Footer-en-US/Country-Pages/Media/Press-Releases-2013/USA/WASMobileApp
