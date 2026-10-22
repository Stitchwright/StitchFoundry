# StitchFoundry v1: the launch gap list

Status: 2026-10-22. Where StitchFoundry stands against the launch checklist (lettering, formats,
simulator, thread charts, hoops, fabrics, AI, appliqué and in-the-hoop, batch, estimator, web
viewer, StitchShop, library) and against what Hatch, Embrilliance and mySewnet Embroidery offer,
judged only from their public pages (never by running their software). The ranked work is in
[`gaps.md`](gaps.md); this page is the launch view of it.

## The checklist

| Launch item | State | What's missing |
|---|---|---|
| Lettering and monograms | Mostly done | TrueType/OpenType fonts sew as satin; monograms, words on arcs and circles, per-letter editing and kerning. Built-in embroidery fonts (FONT-01) done 2026-10-22: the Foundry fonts are bundled from Stitchwright/stitchfoundry-fonts. |
| File formats | Mostly done | Reads and writes PES, DST, JEF, EXP, VP3, XXX, PEC, U01, PCS, TAP, DSB; reads DSZ. Missing: **HUS** (MF-04), VIP, SEW, ART. |
| Stitch simulator | Done | 3D thread view, playback with speed, fabric colour, garment previews. |
| Thread charts | Partial | Generic, Brother, Janome; nearest-colour matching; CSV import. Missing: **Isacord, Madeira, Robison-Anton, Sulky** (waiting on the makers' permission). |
| Hoops | Done | About 60 hoops by maker, chosen by My Machine. |
| Fabric presets | Done | 9 presets setting density, underlay and pull compensation. |
| AI photo-to-stitch | Done | Design from Prompt with Effort (Quick, Normal, Thorough; the cost shown before and after), Thorough's research and stitch check, Redraw Image, Auto-Digitize, Remove Background with the Cut-Out Brush, Magnetic Lasso, Photo Stitch. Make a Design turns a photo into stitches in one step, its background cut away (a setting). The AI test jobs (`evals/ai`) measure it against the owner's photos. |
| Appliqué / in-the-hoop | Partial | Appliqué is complete. In-the-hoop has 13 projects (pillow fronts, mug rug, coaster, ornament, key fob, luggage tag, bookmark, zipper pouch, pencil case, card holder, phone case, book cover). Missing: **more items** (passport holders), more sizes of each, and styles, each with styles, a blank and printable steps. |
| Batch personalization | Done | Team Names from CSV, layouts, worksheets, patches. |
| Job estimator | Done | Stitch count, time, thread and bobbin, quotes, job orders, worksheets. Barcodes still to be checked with a real scanner. |
| Free web viewer | Done (VIEW-01) | A link with `?view` opens the design read-only: its size, stitches, threads and sewing time, with **Get StitchFoundry Free** and **Edit It Here**. |
| Send to StitchShop | Planned (SHOP-01) | StitchHub's inbox is built; StitchFoundry's side is planned in [`stitchshop.md`](stitchshop.md): File › Send to StitchShop… and from a job order, with the design's numbers and a preview. |
| Shared library | Partial | Local library; Publish to MakerFoundry. Sharing comes later, with StitchHub. |

From mySewnet, apart from in-the-hoop: split and multi-hoop, QuickFont, photo stitch, Encore's
repeats (Repeat Design: round a circle, in a row, round the hoop, along a shape), Spiro and Endless
(Endless Border: repeats re-hooped along start and end marks), the Quilt Block wizard's quilting
round a design (Quilt Background: stipple or echo, a margin kept clear) are in, and Name Changer's
job is Team Names' (one file per name, the lettering's place, size and arc kept). PhotoStitch's sepia, black and white and colour tile styles are in. Missing: the Family Tree wizard
(HV-01).

## The owner's decisions

- **Built-in fonts** (2026-10-21): yes, made from OFL fonts and kept in craft-fonts, never in this
  repository.
- **HUS** (2026-10-21): add it, marked experimental; the owner tests it on a Husqvarna.
- **Thread charts** (2026-10-21): ask the makers for permission (the letter is drafted); until
  then users import charts themselves.
- **Photos in Make a Design** (2026-10-21): the background is cut away by default, a setting.
- **Badges and logos** (2026-10-21): real things are drawn without their makers' badges.

## The v1 order

1. Send to StitchShop (SHOP-01, [`stitchshop.md`](stitchshop.md)).
2. The rest of in-the-hoop (HV-01): Family Tree and the passport holder.
3. The web viewer's call to action.
4. Thread charts, as permission comes.

Done since the list was drawn up: built-in fonts (FONT-01), the one-step photo to embroidery (PH-01), Effort and the AI's
cost (AI-05), HUS (MF-04), the key fob, luggage tag, bookmark, zipper pouch, pencil case, card holder, phone case and book cover, Repeat Design, Spiro, Endless Border, Quilt Background and the sepia, black and white and colour tile photo stitches (HV-01), the web viewer
(VIEW-01), research and the stitch check (AI-04), the AI test jobs (AI-EVAL-01).

## Sources

[mySewnet Platinum](https://www.mysewnet.com/en-us/products-and-plans/embroidery-software-platinum),
[Project-in-the-hoop Wizard guide](https://www.mysewnet.com/media/documents/Project-in-the-hoop-Wizard.pdf),
[mySewnet help: Welcome Screen](https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Welcome_Screen.htm),
[mySewnet Embroidery features](https://mysewnet.com/en-il/embroidery/embroidery-digitizing-software/embroidery-software-features),
[mySewnet help: PhotoStitch Assistant](https://softwarehelp.mysewnet.com/MSM/140/Embroidery/Topics/PP3M_Emb_22_Port_Wiz.htm),
[mySewnet help: Photos for Color Tile PhotoStitch](https://softwarehelp.mysewnet.com/MSW/100/Embroidery/Topics/Photos_for_Color_Tile_PhotoStitch.htm),
[mySewnet Embroidery Reference Guide](https://cms.prod.mysewnet.com/media/documents/mySewnetEmbroidery_RefGuide_Win140.pdf).
