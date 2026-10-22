# BERNINA Embroidery Software 9 against StitchFoundry

Status: 2026-10-22. From public pages only (BERNINA's release notes, reference manual and brochure
PDFs, dealer pages, app-store listings); the full survey with every feature, tier and source is
[`research/bernina-v9.md`](research/bernina-v9.md). Nothing of BERNINA's was installed or run.

## The product

- **Current version: V9** (2021), updated to V9.4 in August 2026 (hoops, the bernette 59). No
  public source mentions a V10.
- **Tiers:** DesignerPlus 9, $2,899 (with CorelDRAW SE, the Cross Stitch and Quilter apps and the
  WiFi Device); Creator 9, $1,299 (no multi-hooping, freehand digitizing, special effects,
  Trapunto, PunchWork, Couching, StumpWork, quilt backgrounds or Keyboard Design Collection);
  ARTlink 9, a free viewer. US dealer prices.
- **Windows only** (a Mac through Boot Camp or Parallels); the licence is checked online every 30
  days; no cloud, no web or mobile digitizer (the iPhone app watches a machine sew, the iPad app
  turns drawings into embroidery).
- **The engine is Wilcom's** (the release notes carry Wilcom's copyright, and Wilcom's EMB is as
  editable as BERNINA's own ART).
- **ART**, BERNINA's design file, has no published layout; other programs read it, only BERNINA's
  software writes it. StitchFoundry won't read or write it unless BERNINA publishes it.

## What BERNINA has that StitchFoundry doesn't

Checked against the spec's feature matrix and the commands; most of BERNINA's list is already in
StitchFoundry (stitch types and fills, carving stamps, trapunto, sculpture runs, jagged edges,
colour blending, puff, appliqué with cutting files, CutWork, keyboard design collections,
monograms, multi-hooping, quilt backgrounds, auto-digitizing, PhotoStitch, Magic Wand, the
visualizer, wreaths and mirror-merge, weld, branching and closest join).

| BERNINA feature | What it is | Worth doing? |
|---|---|---|
| StumpWork | 3D pieces (petals, wings) sewn with a wire round their edge on water-soluble stabilizer, cut out and attached | Done 2026-10-22 (SW-01): Embroidery › Make StumpWork |
| Blackwork outline and fill, Candlewicking | Counted-thread styles: geometric blackwork repeats, and French-knot-like candlewick dots along lines | Done 2026-10-22 (ST-03): the blackwork and candlewick motifs, for motif runs and motif fills |
| Lacework fill | A fill for free-standing lace (open net that holds together on its own) | Done 2026-10-22 (FSL-01): Embroidery › Make Lacework |
| Star fill, Wave fill | A fill whose rows run out from a point like rays, or in waves | Star is close to our radial fill; Wave done 2026-10-22 (CS-08): Embroidery › Wave Fill |
| Morphing | Warp a design (bulge, wave, envelope) | VectorCraft's envelope distort covers shapes; check that it carries through to stitches |
| Eyelets | Round holes cut and satin-finished | Yes: CutWork has the cut; an eyelet object is small |
| Convert Embroidery to Artwork | Stitches back to vectors or a picture | Partly there (Convert Stitches to Objects); a picture export exists |
| Textured Edge | Fill edges made ragged, like fur | Our jagged edges are on satins; extend to fills |
| PunchWork | Felting with BERNINA's punch-needle attachment | Only for machines with that attachment: low |
| Direct WiFi to the B 790 PRO and B 990, the B 990's camera picture of the hoop | Machine tie-ins | Needs BERNINA's protocol: not public |
| Design grade (A–D) | How editable an opened file is, on the status bar | Small: a label on opened files |

## What StitchFoundry has that BERNINA doesn't

As far as BERNINA's public pages show: Mac, Linux and the web as well as Windows; free; AI
(Design from Prompt, Redraw Image, Check My Sew-out); Send to StitchShop and Publish to
MakerFoundry; the free web viewer; job orders with costing; ready-made in-the-hoop projects and
mySewnet-style wizards (Family Tree, Spiro, Endless Border). Not checked: whether BERNINA has team
names or in-the-hoop projects under other names.
