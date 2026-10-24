# Tajima DG16 by Pulse against StitchFoundry

Status: 2026-10-24 (PU-05, PU-10, PU-12, PU-18 and satin PU-04 done the same day). From public pages only: Pulse's level comparison
(<https://embroidery.pulsemicro.com/dg15-levels.html>), its product pages
(<https://dgml14.pulsemicro.com/pulse-products.htm>,
<https://dgml14.pulsemicro.com/pulse-products-12reasonsforpulse.htm>,
<https://dgml14.pulsemicro.com/pulse-products-everythingsequin.htm>), the DG16 release notes
(<https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG16>, 16.5.7683, 2026-04-22),
the DG16 launch press release
(<https://www.printing.org/content/2018/10/09/hirsch-solutions-offers-new-tajima-dg16-by-pulse-software>)
and PulseID's page (<https://www.pulsemicro.com/pulseid>). Nothing of Pulse's was installed or run.

## The product

- **Tajima DG16 by Pulse** (2018; 16.5.7683 in April 2026), Windows only, licensed with a security
  device. Levels, each with everything below it: Composer (lettering and editing), Creator
  (digitizing), Illustrator Extreme (vector tools), Artist Plus (effects), Maestro (everything).
- Native design file **PXF**, Tajima's **TBF**; neither layout is published. StitchFoundry won't
  read or write them unless they are.
- Around it: **PulseID** (order-driven personalization and production automation, with
  e-commerce plug-ins), design management (Librarian, Design Manager), networking (Passport,
  machine status, production reports) and direct machine connections (Tajima LAN, Tajima Connect,
  serial; Barudan, Brother, SWF, Toyota, ZSK).

## Already in StitchFoundry

Checked against the commands and stitch parameters. Most of Pulse's list is here already:

- **Every level's essentials:** reading and writing the machine formats, resize, rotate and skew,
  vector (Bézier) digitizing, Illustrator/SVG/PDF/EPS artwork, stitch editing, worksheets with
  barcodes, recipes (fabrics), several thread palettes (colorways), the sew-out player, a global
  view, tabs, workspaces, a step-by-step guide (Make a Design, simple mode).
- **Composer:** TrueType to satin lettering, letter-by-letter changes, kerning, lettering
  templates, patches, monograms, lettering on curves, outline editing, AutoDigitizer, garment
  previews (Product Visualizer, Garment Background), automatic underlay, closest-point connections,
  the cost estimate.
- **Creator:** tracing, satin paths, runs (single, bean, backstitch, stem, blanket, zigzag), fills
  with holes, cross stitch, manual stitches, slicing (Split Object, Automatic Knife), carving
  stamps and 40 programmed fill patterns, jagged edges, automatic appliqué, alignment.
- **Illustrator Extreme:** stitch type conversion for any segment, pen and drawing tools, image
  vectorizing (Image Trace), branching, motif runs and fills, contour fills, overlap removal,
  borders round text and fills (Outline Around, Add Border).
- **Artist Plus:** automatic overlap, radial fills, colour blends (Blend Colors, Blend from
  Gradient), density profiles (`spacing_profile`), circular copies (Repeat Design along a circle),
  turning fills, line carving (Carve Split Lines), custom cornering, random satin edges.
- **Maestro:** automatic breakup (Automatic Knife), automatic start and stop, automatic
  sequencing, spiral and wave fills, the font creator, kerning pairs, push and pull compensation.
- **Sequins and chenille:** sequins along lines and filling shapes, sizes in turn, chain and moss
  chenille, compound chenille.
- **Release 16.5:** Remove Islands (Fill Holes, Remove Holes), adding lines to multi-line text.

## What Pulse has that StitchFoundry doesn't

Ranked by how much the people who'd move to StitchFoundry miss them. Ids for the spec's roadmap.

| Id | Feature (Pulse's name) | What it does | Size |
|---|---|---|---|
| PU-01 | **Fur Stitch** (Maestro) | Fills an area with irregular, overlapping runs of varied length, like fur or hair | M |
| PU-02 | **Fractal, Cascade and Revolving fills** (Maestro); **Fireworks**, **Satin Swirl** | Decorative fills: self-similar curves, rows that step down, rows that turn round a centre, bursts of rays | M |
| PU-03 | **Elastic carving** (Artist Plus) | Carving lines that stretch with the shape, as turning does | S |
| PU-04 | **Variable density and stitch length along a satin or run** (Maestro) | Satin denser or sparser along its length, runs whose stitch length changes along them | S. **Satin done** (`spacing_along`); runs still to do |
| PU-05 | **Random density for satin** (Artist Plus) | Satin rows spaced unevenly on purpose, for texture | **Done** (`random_density`) |
| PU-06 | **Sequin extras**: eccentric sequins (off-centre hole), up to 12 sequin types in one design (colour change sequins), "north drop" tack-down, sequin fonts, the Sequin Paintbrush (drag over sequins to change their colour) | For shops with sequin heads | M |
| PU-07 | **Chenille extras**: Spiro, Lattice, Swirl and Dual Swirl chenille fills, chenille fonts | For shops with chenille heads | M |
| PU-08 | **Bury Stitches** | Connecting stitches driven down under the fabric so they don't show | S; Pulse's public pages say too little to build it from: needs a public description first |
| PU-09 | **Speed steps** | Machine speed changes inside a design (slower for small lettering or metallic thread) | S; only Tajima's own TBF carries them, and its layout isn't published |
| PU-10 | **ColorIT** | Lighten, darken or tint a design's colours in one step | **Done** (Recolour Threads: Lighter, Darker, Tinted) |
| PU-11 | **WordIT** | A word collage filling a simple shape (a heart of names) | M |
| PU-12 | **Kerning classes** (16.5) | Kern a letter against a whole group of letters at once when making a font | **Done** (Kerning Class, Kern All) |
| PU-13 | **Autodigitizer: capped satin ends, satin columns down to 1 mm** (16.5) | Cleaner small lettering and narrow columns from pictures | S |
| PU-14 | **Fonts: 170+ included, 150+ to buy; small lettering fonts (4 mm)** | StitchFoundry's built-in fonts come from craft-fonts (OFL), far fewer | L (fonts) |
| PU-15 | **Direct machine connection**: Tajima LAN and Tajima Connect, Tajima serial, Barudan, SWF, Toyota, ZSK; machine status and production reports | Commercial shops send designs over the network and see what each head is doing | L; the protocols aren't published (owner's call, below) |
| PU-16 | **PulseID-style personalization automation**: orders read from an online shop (Shopify, WooCommerce), rules pick the size, colour and font, designs pulled by barcode | Shops that sell personalized goods online | L; with MakerFoundry |
| PU-17 | **Web design catalog, e-mailing designs and worksheets** | Share a design or its proof sheet with a customer in one step | S |
| PU-18 | **Short stitch cleaning in the viewer** (Ambassador) | Remove stitches too short to sew from a machine file | **Done** (Remove Short Stitches) |

## For the owner to decide

- **PU-15, machine networking.** Tajima Connect, Tajima LAN and Barudan's protocols aren't
  published. Building them clean-room means the machine makers' published SDKs or documentation,
  or a partnership; without that, StitchFoundry keeps to USB sticks, folders and network shares
  (which every one of these machines also reads).
- **PU-14, fonts.** More fonts means more OFL fonts digitized into craft-fonts (FONT-01's way) and
  sewn out; it is the biggest single piece of work on this list.
- **PU-16, personalization automation.** Belongs with MakerFoundry's shop as much as the app.

## Suggested order

1. Small and widely useful: PU-10 ColorIT, PU-08 Bury Stitches, PU-09 speed steps, PU-18 short
   stitch cleaning, PU-05 random satin density, PU-04 variable density and length, PU-12 kerning
   classes, PU-13 autodigitizer columns, PU-17 sharing.
2. Look of the work: PU-01 Fur Stitch, PU-02 decorative fills, PU-03 elastic carving, PU-11 WordIT.
3. Specialty heads: PU-06 sequins, PU-07 chenille.
4. The owner's calls: PU-14 fonts, PU-15 networking, PU-16 automation.
