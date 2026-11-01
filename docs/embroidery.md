# StitchFoundry: embroidery digitizing

StitchFoundry is this fork's embroidery mode: machine-embroidery digitizing built into the
vector editor. Any path, compound path or type object can carry a **stitch type**. The stitches
are generated from the artwork, shown on the canvas as shaded thread, and written out as machine
files (DST, PES, JEF, …). Edit the shape and the stitches follow; give it a different stitch type
and the art stays the same.

Everything is a command (`embroidery.*`), so the same features work from the menus and panels,
the control channel, the CLI and the MCP server.

- [Quick start](#quick-start)
- [Make a Design](#make-a-design)
- [Simple mode](#simple-mode)
- [In-the-hoop projects](#in-the-hoop-projects)
- [Quilting block by block](#quilting-block-by-block)
- [My Machine](#my-machine)
- [Digitizing basics](#digitizing-basics)
- [Stitch types](#stitch-types)
- [Stitch direction: automatic, then editable](#stitch-direction-automatic-then-editable)
- [The digitizing widget and brush](#the-digitizing-widget-and-brush)
- [Lettering](#lettering)
- [Patches](#patches)
- [Auto-Digitize](#auto-digitize)
- [Design from Prompt](#design-from-prompt)
- [Redraw Image with Claude](#redraw-image-with-claude)
- [Ask About Embroidery](#ask-about-embroidery)
- [Check My Sew-out](#check-my-sew-out)
- [Foundry Account](#foundry-account)
- [Publish to MakerFoundry](#publish-to-makerfoundry)
- [Send to StitchShop](#send-to-stitchshop)
- [Design settings: hoop, fabric, threads](#design-settings-hoop-fabric-threads)
- [Stitch Sequence and the stitch player](#stitch-sequence-and-the-stitch-player)
- [Machine files](#machine-files)
- [Commands](#commands)
- [How it is built](#how-it-is-built)
- [Clean-room notes](#clean-room-notes)
- [Known limits](#known-limits)

## Quick start

The **Home screen** (shown when no design is open, or with the Home button) starts a design the
size of your hoop (4×4, 5×7, 6×10, 8×8 or 8×12; `embroidery.newDesign {hoop}`), opens a sample
design to look at and sew, each shown as a picture of its stitches with what it shows when you
point at it (`embroidery.samples`, `embroidery.sample {id}`): a heart patch (a satin border round
a fill), sun and moon (clipart as fills), an appliqué star, a spiro flower (spirograph loops), a
tartan heart, a lace star (free-standing lace), a wavy leaf (rows following a wave, blending two
greens), a mug rug (an in-the-hoop project) and a quilt block (a heart stippled round). It also
walks through a first design in four steps with a link to the help on each, and shows a tip. **Help › StitchFoundry Help…** (F1) opens this guide in the app, on the topic about the tool
in hand, with a search; its **Tips** tab (Help › Tips) has short hints, the tool's first. The
guide is the same text as this file (`help.topics`, `help.topic`, `help.forTool`).

**Updates.** Once a day the desktop app asks GitHub for StitchFoundry's releases (the request
carries nothing about you or your designs) and, when one is newer, the Home screen says so with
**Download**, which opens the release's page, and **Not now**, which hides it until the next
version. A release candidate (a test release, such as 0.14.0-rc.1) is offered only to someone
running one; drafts and the unsigned Latest build never are. **Help › Check for Updates** asks at
once and says in the status bar what it found; **Help › Check for Updates Automatically** turns the
daily check off (`ui.checkForUpdates {automatic?}`). The web app is always the newest and doesn't
ask.

1. Draw shapes, place a logo, or type text.
2. Select them and choose **Embroidery › Create › Convert Artwork to Embroidery**. Filled areas become
   satin (if no part is thicker than 7 mm) or tatami fill, and strokes become satin lines, bean
   runs or runs, depending on their width. To pick a type yourself, use **Embroidery › Stitch Type**
   or the **Embroidery** panel.
3. Check the result on the canvas: stitches are drawn as shaded thread over a fabric-tinted
   artboard. Choose a hoop in the Embroidery panel to see the sewing field.
4. Fix the stitch direction where needed with the **Inclination tool** (below).
5. Use **File › Export › Machine Embroidery File** (or **Embroidery › Export Machine File**) and
   pick a format.

## Digitizing basics

Digitizing is planning how a machine sews art: which stitches cover each part, how long and how
close they are, what goes under them, and the order the needle takes, so the design sews cleanly,
without needless trims, on the fabric it is for. StitchFoundry makes these choices for you and
shows them; this is what they mean and where to change them.

**Three stitch kinds.** A **run** is a line of single stitches: outlines, details and the travel
between parts. **Satin** zigzags across a narrow column, its threads lying side by side for a
smooth, raised look: borders and lettering. A **fill** (tatami) covers an area in rows of shorter
stitches, staggered so no lines show. See [Stitch types](#stitch-types).

**Stitch length.** The machine needs some room between needle holes, and long threads on top can
catch.

- Runs: about 2–2.5 mm sews smoothly (the default 2.5 mm). Under about 0.5 mm the needle keeps
  going into the same spot and can cut or break the thread.
- On clothing, keep any stitch under about 7 mm, since longer ones snag when worn and washed. On
  bags, towels and other items up to about 12 mm is fine; beyond that most machines trim.
- Satin: columns from about 1 mm to 6–7 mm wide. Narrower satin sinks into the fabric (sew it
  as a run); wider satin is split (**Auto split**) or sewn as a fill.
- Fills: stitches of about 4 mm (the default). Much shorter fill stitches make the fabric ripple
  after washing.

**Check Design** flags all of these, with **Sewn on clothing** choosing 7 or 12.1 mm
([Design settings](#design-settings-hoop-fabric-threads)).

**Density** is the gap between rows or zigzags: 0.4 mm by default. Closer for pile (towels,
fleece) so the stitches sit on top of the loops; more open for leather and vinyl, where too many
holes cut the material. Fabric presets set it.

**Underlay** is the stitching sewn first, under the stitches you see: it holds the fabric down
and gives the top stitches something to sit on.

- A **centre run** goes down the middle of a column.
- An **edge run** (contour) goes just inside the outline.
- A **zigzag** goes back and forth across a column.
- A sparse **tatami** goes across a fill at right angles to its rows.

Knits and pile want the most (an edge run and a zigzag); light and see-through fabrics the
least. Each one is switched in the Embroidery panel, and fabric presets choose them.

**Compensation.** Thread pulls the fabric in across the stitches and pushes it out along them.
So a satin sewn exactly as drawn comes out narrower and longer. **Pull compensation** widens
satin and lengthens fill rows to make up for it; **push compensation** shortens satin at its
open ends and keeps fill rows back from the edges across them.

**Order and registration.** Parts sewn in a sensible order meet where they should: what lies
behind first, outlines last over the fills they edge, each part starting near where the last one
ended. Badly ordered designs show gaps and misaligned outlines. **Auto Sequence**, the start and
stop points of each object, and branching (several parts sewn as one path, the thread carried
under later stitching) plan it ([Stitch Sequence](#stitch-sequence-and-the-stitch-player)).

**Colour changes and trims.** Every trim stops the machine to cut, move and start again, and every
colour change needs the operator. Sew a colour's parts together, and travel under stitching still
to come instead of trimming. An extra colour change is sometimes worth it to sew in a better
order. **Merge Colours** and **Sort by Thread Color** help.

**Fabric recipes.** The best density, underlay and compensation depend on the fabric, so the
**Fabric** presets (T-shirt, polo, fleece, towel, denim, leather, cap, silk, woven) set them all
at once, with the stabiliser to hoop under it. Make your own for the fabrics you sew most.

## Stitch types

| Type | What it sews | Main settings |
|---|---|---|
| Run | single run along the path | stitch length, passes |
| Bean run | triple run (each stitch three times) | stitch length, passes |
| Backstitch | hand-sewn look: each stitch goes forward two steps and back one | stitch length, passes |
| Stem stitch | rope-like outline: backstitch with slanted stitches | stitch length, width, side, passes |
| Blanket, zigzag, motif | E-stitch prongs, an open zigzag or a repeating motif along the path (wave, scallop, diamond, cross, feather, heart, star, leaf, loop, zigzag, E-stitch, coil, square loop, blackwork, candlewick, or one drawn for the design), motifs growing or shrinking along it when graded | stitch length, width, width at end, spacing, flip side |
| Redwork | the lines of a path, compound path, type or redwork group sewn as one line that ends where it starts, the way between lines retraced along them | run style (single, triple, backstitch, stem stitch), stitch length |
| Hand stitch | a run that looks sewn by hand: thick stitches (each sewn three times) with short thin ones between them, lengths varying a little | stitch length, gap, passes |
| Sculpture run | a thick-thin line: groups of triple stitches in turn with groups of single ones | triple and single stitch lengths, how many of each, passes |
| String stitch | string art (as in Paghadi work): long straight stitches spanning part of the line, overlapping; on curves they draw a curve inside it, round a circle a ring | chord length, spacing, passes |
| Satin | a column; on a closed shape, satin that turns along it | density, pull compensation (one value, or one per side), auto spacing, short stitches, auto split, preserve long stitches, tatami stitches (a tatami border), jagged edges, underlay (incl. double zigzag) |
| Tatami fill | rows of staggered stitches at the best angle, or turning along inclination lines | density, stitch length, row offset, angle, randomised needle points, gradient density, spacing profile, colour blends, texture, split lines, trapunto, ragged edge, underlay |
| Satin fill | satin rows across the shape | density, pull compensation |
| Contour / Spiral / Radial / Cross / Meander fill | rows following the outline (all the way in, or only so many rings: an offset fill), a spiral, rays from the centre, cross stitches on a grid (full X, half, upright + or double), or a flowing stipple line that never crosses itself | spacing, stitch length, rings (contour), cross style and cell size (cross), randomness (meander) |
| Motif fill | rows of motifs across the shape, kept inside it, each row shifted by the row offset | motif, motif size, row spacing, stitch length, row offset, angle |
| Appliqué | placement run, stop, tack-down, stop, cover satin | cover width and density |
| Manual | a stitch at every anchor point | none |

Satin lines along an open path (also called steil) take a width and a **width profile**:
constant, tapered, calligraphy (a nib angle) or pressure (from a pen). A closed path painted as a
stroke with no fill is sewn the same way, round and round: a **satin border** round the shape (a
filled closed shape is satin across its area instead).

**Column A/B:** the **Column A/B tool** (Embroidery toolbar) digitizes a satin column of any
shape the way digitizers do: click a point on one side of the column, then across on the other
side, then on the first side again, and so on; Alt-click makes a smooth point. Each pair of points
sets the stitch angle there, so the stitches turn with the column (the pairs become inclination
lines you can edit later). Every click is its own step (Undo takes it back), Backspace removes the
last point, Enter or Escape ends the column. To **continue digitizing** a column later, select it
and pick the tool again: clicks add to it. An open path (a run or a satin line) continues from
its end with the Pen tool, keeping its stitch type.

**Tatami borders:** a satin column, line or border with *Tatami stitches* on is sewn in tatami rows
instead: every stitch across is split into pieces of the *Stitch length* (4 mm to start with),
shifted a quarter from row to row. Use it for borders and columns too wide for satin.

**Smart corners:** where a satin line turns sharply, rows that simply turned with it would pile up
inside the corner. *Corners* sets how the two sides meet: **Automatic** (the default) lets rows
turn round gentle corners (up to 45°), keeps each side's rows square and meets them on the miter
line at medium ones (up to 110°), and caps the tip of sharper ones; **Turn**, **Miter** and
**Capped** use one method everywhere. Tight curves never fold the inner side of the column back
over itself.

**Passes:** any run style can be sewn up to 9 times over the same needle points; 2 passes is a
backtrack, which ends where it started.

**Motifs:** a motif run repeats one of fifteen motifs along its line (wave, scallop, diamond,
cross, feather, heart, star, leaf, loop, zigzag, E-stitch, coil, square loop, blackwork and
candlewick; zigzag and E-stitch make zigzag and E-stitch fills as motif fills, coil and square
loop the looped, chenille look regular machines can sew, along a line or filling a shape).
**Blackwork** is a stepped key across the line, a counted-thread look as a border or, as a motif
fill, an all-over pattern; **candlewick** sews a knot of short crossing stitches at each repeat
with the line between, the look of candlewicking's French knots. With *Graded* on, the motifs
grow or shrink from *Width* at the start to *Width at end*, their repeats keeping their
proportions. Motifs, blanket prongs and their grading follow the line the way it was drawn,
whichever end sewing starts from (the closest join). A **motif fill** sews rows of motifs across a shape, half a motif inside its outline;
the row offset shifts each row along by that fraction of a motif (0.5 for a brick layout).
*Paired with* makes a paired motif fill: a second motif sewn in turn with the first along every
row (`motif_pair`), each in the same place on every row. *3D effect* sculpts a motif fill
(`warp`): **Globe out** makes a dome, the motifs largest in the middle and smaller and closer
together toward the edge; **Globe in** makes a bowl, the motifs smallest in the middle; and
**Perspective** makes the fill recede, its motifs and rows smaller toward the far side, which
*Direction* points to (90°: up). *Strength* (0 to 0.8) sets how much the motifs change; strong
effects make small stitches where motifs shrink.
**My Motifs:** *Keep Motif* (beside the motif list, or `embroidery.motif.keep {name}`) keeps one
of the design's motifs for any design. The motif lists show My Motifs after the design's own;
applying one adds it to the design. `embroidery.motif.library` lists them and
`embroidery.motif.forget {name}` takes one out; they are kept with the preferences, and
resetting the preferences keeps them.
**Embroidery › Motifs › Place Motif** places one motif (10 mm long) in the middle of the artboard as a path
sewn in a single run: move, scale and reshape it like any art. The **Motif tool** (in the
Embroidery tool group) places one wherever you click, or drag across the canvas for one as long as
the drag and turned its way; pick the motif, built-in or one of the design's, in the Embroidery
panel while the tool is active.

**Motif columns:** a satin column or satin line can be sewn as motifs instead of satin (*Sew
as: Motifs*, `motif_column`): along its middle, each motif as wide as the column where it sits (a
tenth less, so it stays inside) and *Motif length* times as long, turning with the column's
rungs. On a tapered or calligraphic line the motifs grow and shrink with it (a graded motif
column). Motifs stand upright along a column drawn left to right, as on a motif run.

**Rings:** **Embroidery › Motifs › Place Ring** places a 20 mm ring, 3 mm wide, in the middle of the
artboard: a **Satin Ring** (a satin border round a circle), a **Filled Ring** (a fill between two
circles) or a **Bean Ring** (a bean run round a circle). It is an ordinary circle afterwards:
move, scale and reshape it. `embroidery.placeRing` takes the `stitch`, the `diameter`, the
ring's `width` and where it goes (`at`).

**Your own motifs:** draw a motif as a path, left to right the way it should sew along a
left-to-right line, select it and choose **Embroidery › Motifs › Make Motif**. An open path runs from its
first point to its last, so the motif starts and ends on the line between them; a closed shape
(or a path ending near where it starts) lies on the middle of its height and is sewn once round
from its left side, then on round to its right side. The motif keeps the drawn proportions and
belongs to the design (it is saved with it): the *Motif* list of motif runs and motif fills
offers it after the built-in motifs. Choosing it makes a run as high as the drawing's proportions
make it for its spacing, and spaces a motif fill's rows so the motifs don't overlap; Place Motif
and Motif Border take it too. Making a motif again under the same name (`embroidery.motif.make
{name}`) changes it in everything that sews it. *Delete Motif* takes it out of the design; objects
sewing it keep their copy. All motifs, built-in or drawn, sew the same way up along runs, in
motif fills and when placed: upright along a line drawn left to right (*Flip* turns them over).

**Spacing profiles and colour blends** (tatami): *Spacing profile* sets how the rows space out
across the shape (in the direction the rows stack): **Even**, **Accordion** (dense, sparse,
dense, sparse, dense: a shaded, ribbed look), **Fade out** and **Fade in** (the rows thin out to
nothing across the shape). The density changes evenly between the profile's points. Below the
presets, the profile's curve shows the rows' density across the shape (up is denser; the line is
the fill's own density; along the bottom, no rows): drag a point to reshape it, double-click to
add one (16 at most), right-click one to take it out; each change is one undo step. Agents can set
any profile (`spacing_profile: [[t, spacing], …]`, 0 for no rows). **Make Blend** (with a colour
picked in *Blend into*) turns the fill into stacked layers, one per colour, each densest across
its own band and fading to nothing at the next colour's, so the thread colours mix across the
rows; the layers together sew about as much as the fill did, and only the first keeps the
underlay. The layers stay in the one object (Multi Blend): select it, move it or reshape it and
every layer follows, each sewn in its own thread one after the other; *Blend layers* in the panel
shows their colours and **Remove Blend** sews it plain again (`setParams {blend: []}`).
`embroidery.makeBlend` takes up to six colours, and `separate: true` makes the layers objects of
their own instead. Turn the fill's angle to turn the blend.

**Blend from Gradient:** a tatami fill painted with a linear gradient (Gradient panel, or an SVG
gradient) shows **Blend from Gradient** in the panel. It sews the gradient's colours as a Multi
Blend that runs the way the gradient does: the rows turn across the gradient, and each colour is
densest where its stop is and fades to nothing at the next stop's, so uneven stops blend unevenly
too. Up to six colours (a gradient with more is sampled evenly); the art keeps its gradient
(`embroidery.gradientBlend {ids?, spacing?}`). Like Ink/Stitch's linear gradient fill, written for
StitchFoundry.

**Tartan Fill:** **Embroidery › Tartan Fill** fills the selected closed shapes with a woven check.
The sett, a run of coloured stripes (our own navy, green, black and gold to start with), is laid
down the shape and across it and repeated to fill it; by default it runs there and back, as most
tartans do. Each colour becomes one fill each way: the warp first, its rows running down, then
the weft on top, its rows running across, both open (0.8 mm apart) so where stripes cross both
threads show and the colours mix the way a weave's do. The fills take the shape's place in one
group; one Undo puts it back (`embroidery.tartan {sett?: [{color, width}], mirror?, scale?}`).

**Cutwork:** machines with cutting needles cut the fabric with blades that each face one way, so
each needle cuts only lines running within its range of directions. **Embroidery › Cutwork** splits
the selected lines for them: each stretch becomes a running-stitch line in its needle's colour,
named after it (needle 1 cuts from 0° to 45°, needle 2 from 45° to 90° and so on, either way along
the line; tiny jogs stay with the stretch before), grouped by needle so each needle cuts all its
stretches in one go (`embroidery.cutwork {needles?: 1-4, stitch?}`).

**Routing satins:** to sew several satin columns as one path without trims, the way Ink/Stitch's
auto-route does, select them and use **Embroidery › Shape › Branch** (see Stitch Sequence below): it picks the
order and carries the thread from one to the next under the ones still to come.

**Outlines and borders:** **Embroidery › Add to Design › Add Outline** sews an outline round the selected
stitched shapes, lines and type, after them and in the top one's thread: a **Satin Outline** or
**Bean Outline** following them 1 mm out, or a **Rectangle**, **Oval**, **Rounded** or **Motif
Border** framing them. A line is outlined round the band its stitches cover (a satin line as wide
as it is sewn, a run as if 1 mm wide), a closed line or border round its outside. `embroidery.addOutline` takes the shape (`offset`, `rectangle`, `oval`, `rounded`),
how far out (`offset`, mm), the stitch (`run`, `bean`, `satin`, `motif`), its `width` and `motif`.
The outline is an ordinary path afterwards: reshape it or change its stitch type.

**Repeats and envelopes:** stitched objects inside a repeat (Object › Repeat: radial, grid or
mirror) are sewn once per copy, where each copy shows: wreaths, borders and arrays from one
motif. Selecting the source object shows every copy's stitches. Stitched objects inside an
envelope (Object › Envelope Distort) are sewn bent through it, like the art; their inclination
lines and start and stop beads don't bend with them. The same goes for the live effects that
reshape a path (Effect › Warp, and Distort & Transform: Pucker & Bloat, Roughen, Twist, Zig Zag,
Tweak, Transform…): the stitches follow the shape as it shows, and taking the effect off sews the
plain shape again. Effect › Stylize and the other raster effects don't change what is sewn.

**Laydown:** **Embroidery › Add to Design › Add Laydown** puts an open tatami (rows 2.5 mm apart, at 45°, no
underlay, travel along its outline) under the selected stitched shapes and type, 1.5 mm beyond
them, sewn first in the bottom one's thread. On towels, fleece and knits it flattens the pile so
the design sits on top instead of sinking in. Lines get no laydown. The command's `margin`,
`spacing` and `angle` change it; afterwards it is an ordinary fill.

**Hand stitch** (a run style): stitches about *Stitch length* long, each sewn forward, back and
forward again so it lies thick, joined by single stitches about *Gap* long that hardly show; each
length varies a little at random (the same way every time), so the line looks sewn by hand. It
starts and ends with a thick stitch.

**Sculpture run** (a run style): a thick-thin line. *Triple stitches* stitches *Triple stitch
length* long, each sewn forward, back and forward again, in turn with *Single stitches* stitches
*Single stitch length* long (three and one to start with); all lengths are scaled a little so
the line ends on a whole stitch.

**Cross fill styles:** *Cross style* sews each cell of a cross fill as a **full cross** (an X:
the "/" diagonals along the row and the "\" diagonals coming back), a **half cross** (the "/"
diagonals only), an **upright cross** (a + : the middle line with a bar up and down each cell,
and the middle line again coming back) or a **double cross** (an X with a + over it). *Cell size*
sets the grid.

**Jagged edges** (satin): the stitches on one side (*Side 1* is the first rail), the other or
both end short of the edge by a random amount up to *Jag depth*, for fur, grass and feathers. A
stitch always keeps at least a fifth of the column's width; the underlay isn't jagged.

**Trapunto** (tatami): travel between the parts of the fill keeps to its outline instead of
running along its rows, crossing the fill only where nothing else reaches (round a hole). Use it
with an open density (2 mm and more), where travel along the rows would show between them.

**Ragged edge** (tatami, Textured Edge in other programs): each row's ends stop short of the
outline by a random amount up to the depth set (mm, 0 for a clean edge; at most 20, and never more
than two fifths of the row from each end), for fur, grass or feathers. The same rows come out each
time. Turn the edge-run underlay off as well when the outline shouldn't show between the ends.

**Satin details:** *pull compensation* can differ per side (side 1 is the first rail), for a
column lying on fabric on one side and on other stitching on the other. *Preserve long stitches*
keeps a stitch longer than the machine's longest stitch whole: the frame moves in steps without a
needle drop. Satin on closed shapes (including lettering) uses the same settings as a satin column.

**Ties:** every object follows the design's tie-in and tie-off setting unless its own *Tie in* or
*Tie off* says *On* or *Off*. The design's *Tie style* (Machine settings) is **Single** (one stitch
out and back), **Double** (twice, the default) or **Triangle** (out, across and back, for fabric
that frays or stretches), *Tie length* long. **Omit last stitch** leaves out an object's last
needle point, where it would sew back over the object's edge or into the next one.

**Remove underlying stitching:** an object with this option on cuts what it covers out of every
fill sewn before it (lettering on a background fill, a patch on a jacket back), so thread doesn't
stack up where nobody sees it. The fills below still run *Overlap* mm (1 mm by default) in under
its edge, so no gap opens when the fabric pulls. A fill cut in two sews as two pieces; one covered
completely sews nothing. Moving or reshaping the object updates the cut.

Underlay is automatic by default (centre run, edge run, zigzag or tatami, chosen by width) and can
be set by hand. Choosing a **Fabric** in the Embroidery panel sets density, pull compensation and
underlay for every stitched object to suit that fabric.

**Push compensation** (`push_comp`, satin and fills; 0 by default, fabric presets set it): stitches
pull the fabric in across them (pull compensation widens them for it) and push it out along
them. With push compensation a satin column stops that much short at each open end, and a fill's
rows stop that much short of the outline across them, so the sewn shape comes out its drawn size.

## Stitch direction: automatic, then editable

Every stitched object chooses its stitch direction automatically, and the result is stored as
ordinary editable data:

- **Fills** get the straight angle that sews with the fewest breaks. Long, thin shapes and satin
  shapes get **inclination lines** instead: lines drawn across the shape that the stitches follow,
  so the direction turns smoothly from one line to the next (turning satin, turning tatami).
- To place inclination lines automatically, the two ends of the column are found and its sides
  are paired by dynamic time warping. Shapes whose pairing would run along rather than across
  them fall back to a straight angle.
- Satin needs simple columns, so shapes are cut open first: each hole is slit where the shape is
  thinnest (an "O" becomes two columns), and sharp inside corners where strokes meet are cut too
  (a "T" becomes three columns). The slits are 0.02 mm wide, too narrow to show in the stitches.

A shape given the **Satin** stitch type gets its inclination lines at once, placed as Auto
Direction places them (about every one and a half column widths, and wherever the column turns by
more than 20°), in the same undo step: they show on the selected satin with any tool (the
Embroidery panel's **Inclination lines** display, on by default) and can be dragged straight away.
Satin lines drawn as strokes turn along their path and need none.

Editing:

- **Inclination tool** (Embroidery section of the toolbar, **Embroidery › Stitch Direction ›
  Inclination Tool** or Shift+I): drag across a shape to add a line, drag a line's ends to move
  them, click a line and press Delete to remove it.
- **Embroidery › Stitch Direction › Use Line as Direction**: select stitched shapes and lines drawn
  across them (with the Pen, the Line Segment tool or any path tool, or brought in with the art);
  each line becomes a direction line of the shapes it crosses and the drawn line goes, in one undo
  step (`embroidery.lineAsDirection {ids?, keep?}`). Several lines across one shape make its
  stitches turn from one to the next.
- The **green bead** (where sewing starts) and the **red bead** (where it stops) of the selected
  object can be dragged too. **Reset Start and Stop** in the panel restores the automatic closest
  joins. A fill honours both: its underlay runs out from the start and back to it, then the top
  stitching begins at the start and sews towards the stop, the piece of the shape that can end
  nearest the stop sewn last, so it finishes there instead of travelling back over sewn rows.
- **Embroidery › Stitch Direction › Auto Direction** places the automatic lines again; **Embroidery › Stitch
  Direction › Clear Inclination Lines** goes back to the automatic direction.
- Inclination lines, beads and fill angles move, rotate, scale and mirror with their object.
- A click selects a stitched object where its stitches are, not only on its art: inside a fill,
  appliqué or filled satin even when the art has no fill colour, across the whole width of a satin
  line drawn on a hairline, and on an imported machine file's thin stitch lines, so it never
  passes through to the object behind.

**Reverse Stitch Direction** (Embroidery menu) swaps where the selected objects start and stop
sewing.

**Swap Satin Sides** (Embroidery menu, `embroidery.swapSides`; the `swap_sides` setting) starts the
selected satin columns' zigzag on their other side, so each column also ends on the other side:
a shorter way on to what is sewn next. Each side keeps its own settings (twin pull compensation,
jagged edges). Choose it again to swap back.

**Convert to Satin Column** (Embroidery menu, `embroidery.toColumn`) turns a closed shape that
sews as turning satin (long and narrow, without holes: a stroke of a letter, a stem, a leaf)
into a satin column between two rails, as the Column A/B tool makes them: its two sides become
open paths you can edit node by node, meeting at a point where its ends were round, and the way
its stitches turned stays as inclination lines. The satin settings carry over (from a satin fill,
its satin column settings).

**Holes:** **Embroidery › Shape › Remove Holes** deletes the holes of the selected shapes (all of them, or
with the `maxArea` parameter only small ones, handy after tracing). **Fill Holes** adds an object
in each hole, sewn right after its shape, so a hole can be sewn in another thread. To add a hole,
draw a shape over a stitched object, select both and choose **Minus Front** (Window ›
Pathfinder). The hole is cut out and the object keeps its stitch settings. Pathfinder operations
carry stitch settings the same way they carry appearance.

## Fill textures and split lines

A tatami fill sews its rows with the needle points staggered a little from row to row, which
reads as a flat, even surface. Where the needle points line up instead, they show as lines in
the fill: an embossed texture without another thread.

- **Texture** (Embroidery panel, tatami fills): a pattern tiled over the fill, with a needle point
  wherever a row crosses one of its lines (programmed splits). **Diamonds**, **Waves**,
  **Herringbone**, **Bricks**, **Circles**, **Scales**, **Hexagons**, **Chevrons**, **Zigzag**,
  **Ogee**, **Basket weave**, **Stars**, **Hearts** and **Leaves**; *Texture size* is the width of one
  pattern tile (to start with, a size that suits the stitch length), *Texture angle* turns the
  pattern against the stitch rows. Between the pattern's lines the stitches run straight to the
  next line, broken up only where the gap is longer than a long stitch, so the pattern reads
  clearly. The pattern stays put on the page while the shape changes.
- **Split lines** (drawn splits, carving stamps, flexi splits): draw paths over a tatami fill,
  select them with the fill and choose **Embroidery › Split Lines › Carve Split Lines**. The fill keeps its usual
  stitching and gains a needle point where each row crosses a line. A closed path presses in its
  whole outline, like a carving stamp (a star, a heart, a letter's outline); open paths draw
  lines. The paths are used up unless the command's `keep` parameter is on. When every selected
  object is a tatami fill, the front-most one is the stamp for the others.
  With the `repeat` parameter (mm), each line is copied that far apart across the whole fill:
  draw one wave across a shape for wavy lines all over it (flexi splits).
- **Contour split lines:** **Embroidery › Split Lines › Contour Split Lines** gives the selected tatami fills
  split lines that follow their outlines: each outline, holes too, repeated inward every 4 mm
  (`embroidery.contourSplits {spacing}`) until the shape runs out, so the fill shows rings along
  its edge.
- **Carving stamps:** **Embroidery › Split Lines › Carve Stamp** presses a stamp from the library into the
  selected tatami fills, in the middle of each and 60% as wide as its smaller side: star, heart,
  flower, leaf (with its veins), spiral, sun, moon, snowflake, paw print, swirl, shell,
  butterfly, clover, feather, rings and diamond, all drawn for StitchFoundry. The stamp becomes
  split lines like any carving (`embroidery.carveStamp` takes `at`, `size` and `angle`;
  `embroidery.stamps` lists the library).
- Split lines show dashed on the selected fill, move, turn and
  scale with it, and stay its own: **Make Properties Current** and styles leave them out.
  **Embroidery › Split Lines › Clear Split Lines** (or the button in the panel) takes them out again.
- **Embossed turning satin:** a satin fill that turns (two inclination lines or more) takes a
  texture too. The pattern is laid along the column, its length along the column's middle and
  its width across it, so it bends with the turning; each satin stitch gets a needle point where
  it crosses the pattern's lines (none closer than 0.8 mm to another or to the edge). Set it with
  the same *Texture* rows in the Embroidery panel (`embroidery.setParams {params: {texture}}`).
- Otherwise textures and split lines change only straight tatami rows: straight satin fills,
  turning tatami and underlay ignore them, and a textured tatami shape doesn't turn along long
  thin shapes on its own.
- **Flow lines** (Florentine and liquid effects): draw one or two lines across or beside a
  tatami fill, select them with it and choose **Embroidery › Stitch Direction › Fill Along Lines**. With one line,
  every row is a copy of it moved across the fill, so the stitches follow its curve; with two,
  the rows change shape evenly from the first line to the second, so they can fan out, narrow
  or swing round. Draw the two the same way round and keep them clear of each other; rows
  crowd where the lines come together. The lines carry on straight past their ends, and a
  pair that would stretch the shape out of all proportion leaves the rows straight. Textures
  and split lines follow the rows; underlay stays straight. Flow lines show dashed on the
  selected fill, move with it, and take precedence over inclination lines;
  **Embroidery › Stitch Direction › Clear Flow Lines** (or the button in the panel) gives straight rows back.
- **Wave Fill** (Embroidery › Wave Fill, `embroidery.waveFill {ids?, waves?, height?, angle?}`)
  makes the flow line for you: the selected shapes become tatami fills whose rows rise and fall
  across them `waves` times (3), `height` mm from crest to trough (5), running at `angle`.

## The digitizing widget and brush

The **Digitizing Widget** (Embroidery menu, Embroidery panel, or picking the **Embroidery Brush**
tool) is a dial in the canvas's lower-left corner:

- the **inner ring** picks what to set: Brush, Width, Density, Length or Shape;
- the **outer ring** shows that setting's values; click one to use it;
- the **centre** shows the thread colour (click to cycle; the panel has a full colour picker).

Brushes: Run, Bean, Steil, Tapered, Calligraphy, Pressure, Appliqué, Fill and Satin. Shapes:
Free (freehand), Line, Circle, Square, Rounded, Triangle, Diamond, Hexagon, Star, Heart and Arrow
(drag to size; Shift keeps proportions). With the widget open, the Embroidery panel also offers:

- **New strokes: Sewn last / Before the selection / After the selection**, which digitizes into
  the middle of the sewing order;
- **Widget values**, the widths, densities and lengths the outer ring offers (comma-separated, up to
  12 each).

Stroke ends snap to anchors and guides while Snap to Point or Smart Guides is on.

## Appliqué covers

An appliqué's cover stitch is satin, blanket, an open zigzag or a triple run (or none, for raw
edges), *Cover width* wide. **Partial cover** (`partial: true`) leaves the cover out wherever an
object sewn later lies over the patch's edge: that object holds the fabric there, and the cover
under it would only add bulk.

The canvas shows each appliqué's fabric under its stitches (*Fabric colour*; *Fabric pattern*:
plain, stripes, check, dots, denim or felt, woven with the *Pattern colour*), so a design can be
judged with the fabrics it will use.

**Combine Appliqué** (`embroidery.applique.combine`) makes the selected pieces one compound path
sewn together (`combine: true`): every piece's placement line, one stop to lay all the fabric,
every tack-down, one stop to trim, then every cover; without it each piece is sewn whole, two
stops each. Pieces of one compound path combine with *Sew the pieces together* in the panel.
**Prepare Appliqué** (`embroidery.applique.prepare {close?, breakUp?}`) makes pieces ready to
sew: open outlines are closed, and where pieces overlap, each is cut away under the pieces in
front of it, so every piece lies flat in its own layer.

## Click to stitch

The **Click-to-Stitch tool** (in the Embroidery tools) turns a region of a placed image into a
stitched object with one click: the region is the pixels joined to the spot clicked whose colour
is within *Tolerance* of it (a magic wand), and it becomes an object in its colour, stitched as the
tool's *Stitch as* setting says (in the Embroidery panel while the tool is chosen):

- **Fill**: tatami over the region, its holes left open (holes under 16 pixels are filled);
- **Fill without holes**: tatami over the whole region;
- **Satin**: satin across the region, turning with it;
- **Outline**: a running stitch round its edge;
- **Centre line**: running stitches along its middle, for lines and lettering drawn in the image.

With **Contiguous** off (`contiguous: false`) a click takes every patch of about that colour across
the image, joined to the spot or not (a colour range): all of a parrot's yellow in one object.

The same is `embroidery.clickStitch {at, as?, tolerance?, contiguous?, minHole?, id?}`. Each click is
one undo step; the image stays under the stitches.

Where a region's colour runs into its neighbours, draw round it with the **Magnetic Lasso tool**
instead: click round the part of the photo you want, and between the clicks the border snaps to
the edges in the photo near the path the pointer took (the "live wire" of Mortensen and Barrett's
*Intelligent Scissors*, 1995). Click the first point again, double-click or press **Enter** to
close the shape; **Backspace** takes the last point back and **Escape** starts over. The shape
gets the photo's average colour inside it and is stitched as *Stitch as* says (Fill, Satin,
Outline, or None for a plain path to work on). *Width* is how far from the pointer's path, in the
photo's pixels, an edge is looked for (10), and *Contrast* how strong a step in colour has to be
to count as one (10 %). The same is `embroidery.magneticLasso {points, guide?, width?, contrast?,
as?, id?}`, one undo step.

## Fill region

CAD drawings open with File › Open or File › Place: DXF (ASCII or binary, any version) and, in
the desktop app, DWG (AutoCAD 2000 and later). A DWG is turned into DXF first by acadrust
(MPL-2.0), in a helper copy of the app with a two-minute limit, so a damaged drawing fails to open
without taking the app down; the web app asks for a DXF instead.

For line drawings, such as a DXF from a CAD program or a colouring-book outline, the **Fill Region
tool** (in the Embroidery tools, with the paint-bucket cursor) fills an area with one click. Click
inside an area that lines enclose and it becomes a new closed shape above the lines, sewn as the
tool's *Stitch as* setting says (Fill or Satin) in the current fill colour. The lines aren't
changed, and a line across the area bounds it, as a CAD program's boundary pick does.

- The selected lines and shapes bound the area; with nothing selected, every line does.
- **Close gaps up to** (1 mm; 0 to 25): lines that stop short of each other by up to this much
  still enclose the area, as lines from CAD often do. When a click finds no closed area, raise it.

The same is `embroidery.fillRegion {point, ids?, gap?, type?, color?}`. Each click is one undo step.

## Outlines round art

**Embroidery › Outline Around** (`embroidery.outlineAround {gap?, width?, kind?, color?}`)
draws one outline round the selected stitched art as it sews (lettering, shapes and lines
together; pieces closer than about four times the gap share one outline), its holes left out:
bubble lettering, a sticker or patch border. It is `gap` (2 mm)
from the art, a satin `width` (3 mm) wide, or a single or triple run (`kind: run|triple`), in
`color` (near black), and is sewn after everything else; it is an ordinary path to change after.

Two more kinds, after Ink/Stitch's knockdown fill (https://inkstitch.org/docs/fill-tools/):
**knockdown** (`kind: knockdown`) fills that area with light tatami (1.2 mm rows at 45°) sewn
first, under the design, to press a towel's or fleece's pile flat; **emboss** (`kind: emboss`)
fills a rectangle or circle (`shape`) reaching `extend` (5 mm) beyond the design with light
tatami, leaving the design's area out, so the design stands up from a stitched background.

Two quilting kinds (Embroidery › Add to Design › Echo Quilting and Background Quilting): **echo**
(`kind: echo`) sews `count` (3) running-stitch outlines round the art, the first `gap` (2 mm) from
it and each `spacing` (3 mm) further out, the way a quilter echoes an appliqué; **stipple**
(`kind: stipple`) fills a rectangle or circle (`shape`, `extend`) round the art with a meander,
leaving the art's area out, sewn first so the design sits on a quilted background.

## Scatter copies

**Embroidery › Scatter Copies** (`embroidery.scatter {count?, gap?, spread?, seed?, turn?}`)
strews copies of a motif over a shape: select the motif (one object or several) and a closed
shape larger than it, and the copies go inside the shape, `gap` (2 mm) apart and clear of its
edge, at random (`spread: random`, the same each time for the same `seed`) or evenly on a
triangular grid (`even`), up to `count` (20), each turned at random with `turn`. They keep the
motif's stitching and land in a group above the shape, which stays (delete it, or give it a
fill to sew behind them).

## Repeat Design

**Embroidery › Repeat Design** (`embroidery.repeat {along?, repeats?, position?, radius?, gap?,
margin?, corners?}`) repeats the selected design (one object or several) and puts the copies in
its place, in a group in sewing order:

- `along: circle` (the default): `repeats` (8; 2 to 360) copies round a circle, clockwise from the
  top, each turned to face out, the circle just large enough to keep them `gap` (2 mm) apart unless
  a `radius` is given;
- `line`: in a row, left to right, `gap` apart;
- `frame`: round the inside of the hoop, `margin` (5 mm) from its edge, as many as fit a gap apart,
  turned to follow each side; `corners: false` leaves the corners empty;
- `shape`: along the outline of a closed shape selected with the design (the larger), which stays.

`position: mirrored` mirrors every other copy (best with an even number). A negative gap overlaps
the copies, by half a copy's width at most. It works like Encore in mySewnet
([public help](https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Encore.htm)).

## Endless Border

**Embroidery › Endless Border** (`embroidery.endless {direction?, marker?, sides?, repeat?, gap?}`)
makes the selected design a border sewn hooping after hooping, down a towel or along a table
runner. The design is repeated along the way the fabric moves through the hoop (`direction`: up
in a tall hoop, left in a wide one, or as set) as many times as fit (`repeat: false` for one),
`gap` apart. Start marks come first, on the side the fabric moves toward, and end marks last, on
the other, each in a thread of its own so the machine stops: sew it, move the fabric on and hoop
it again, and at the start marks line the needle up with the end marks just sewn. The end marks
are a small L of long stitches whose corner is the point (`marker: corner`), one long stitch onto
it (`single`, for fabric that marks), a few stitches on it (`tieoff`) or three stitches in a line
ending at it (`line`); on both sides of the border (`sides: both`, to see it went in straight) or
the first only. Pick out the marks when the border is done. It works like Endless Embroidery in
mySewnet ([public help](https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Endless_Embroidery.htm)).

## Quilt Background

**Embroidery › Quilt Background** (`embroidery.quiltBackground {fill?, margin?, spacing?, stitch?,
width?, height?, color?}`) quilts round the selected design, for a quilt block or a framed
piece: a `margin` (3 mm) is left unstitched round the design, and the rest of the hoop (3 mm in
from its edge), or a block `width` × `height` round the design, is quilted in one running line,
`spacing` (4 mm) between its lines, `stitch` (2 mm) long. `fill: stipple` (the default) is a
wandering meander; `echo` is lines following the design's outline outward and the block's edge
inward. The quilting is sewn first, under the design, in a thread of its own named Quilting (the
design's first colour unless `color` is given). It works like the quilting round an embroidery in
mySewnet's Quilt Block wizard
([public help](https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Quilt_Block_Wizard_Examples.htm)).

## Spiro

**Embroidery › Spiro** (`embroidery.spiro {size?, petals?, shape?, spacing?, layers?, stitch?,
colors?, random?, seed?}`) draws a spirograph pattern in the middle of the hoop, `size` (80 mm)
across, as lines sewn in running, double or triple stitch (`stitch`, triple by default). `petals`
(7) sets the loops round it, `shape` how far round each loop reaches before it turns (1 to petals
− 1; one that shares a factor with the petals is moved to the nearest that doesn't, so each layer
is one closed line), `spacing` how far the loops swing in (1: they meet in the middle). `layers`
(2) repeats it turned between the petals, a thread colour each (`colors`, or six of our own in
turn). `random: true` chooses the petals, shape and spacing for you, the same for the same `seed`.

## StumpWork

**Embroidery › Make StumpWork** (`embroidery.stumpwork {ids?, gauge?, cover?, coverWidth?, color?,
fill?}`) turns the selected closed shapes (a group's too) into stumpwork pieces: raised, free-standing petals,
leaves or wings with a wire round their edge, sewn on organza over water-soluble stabilizer, cut
out and bent to shape. Each piece sews in four steps, each its own thread named for what to do,
so the machine stops before it:

1. **Wire track**: a double run along the edge, where the wire goes (its thread also says to hoop
   organza over water-soluble stabilizer).
2. **Wire**: lay the wire on the track with its tails at the start and hold it; a zigzag just
   wider than the wire tacks it down. The wire is given by its gauge (`gauge`, AWG 28 by default,
   24 to 34; a bigger number a thinner wire).
3. **The piece's fill** (a tatami fill if it had none; `fill: false` leaves it open).
4. **Cover**: a satin (or blanket, `cover`) over the wire, 2.5 mm wide (`coverWidth`, at least the
   wire's width and a bit), in the piece's colour or `color`. In the fill's colour it follows the
   fill with no stop.

The finishing goes in the design's notes, which the worksheet prints: cut each piece out close to
its edge, rinse the stabilizer away, let it dry, bend the wire to shape, and push the tails through
the fabric where the piece goes. One undo takes a piece back to its shape. Behaviour from
BERNINA's public help on its stumpwork tools.

## Eyelets

**Embroidery › Make Eyelets** (`embroidery.eyelet {ids?, diameter?, width?, color?}`) makes holes
bound with satin, for lacing, ribbon or broderie anglaise. With nothing selected it makes a round
one in the middle of the design, its hole `diameter` mm across (4); selected closed shapes become
eyelets of their own shape. Each sews in two steps, each its own thread so the machine stops
between them: the hole's edge stitched round twice to hold the fabric, then (after you take the
hoop off and cut the hole open inside that stitching) a satin binding `width` mm wide (1.5) over the
cut edge. On the round eyelet the satin lies just outside the hole, overlapping the cut edge, so
the hole stays the size asked. One undo takes them back.

## Lacework (free-standing lace)

**Embroidery › Make Lacework** (`embroidery.lacework {ids?, pattern?, spacing?, edge?, color?}`)
turns the selected closed shapes (a group's too) into free-standing lace: open rows crossing at the
pattern's angles, which lock where they cross so the lace holds once the stabilizer is gone, and a
satin edge round it. **Diamond** crosses rows at 45° and 135°, **grid** at 0° and 90°,
**triangle** at 0°, 60° and 120°, **star** all four; `spacing` sets the rows apart (2.5 mm, 1.5 to
6) and `edge` the satin's width (3 mm, or 0 for none). It is all one thread with no stop, in the
shape's colour or `color`. The design's notes say how to sew it: two layers of fibrous
water-soluble stabilizer, the same thread in the bobbin, then rinse the stabilizer away and pin the
lace flat to dry. One undo takes it back. Behaviour from BERNINA's public help on its Lacework fill.

## Family Tree

**Embroidery › Create › Family Tree…** makes a family chart: each name lettered in satin, framed
if you like, partners side by side joined by a line, and each family joined to its parents. Type
the family as an outline, one person a line, each generation indented under the one before:

```
John Green + Mary Green
  Anna Green + Tom Hill
    Lucy Hill
    Sam Hill
  Peter Green
```

- **Descendants** (the default): the first person at the top (`Name + Partner`, or `&`, for a
  couple), their children below them, and their children below each, the oldest first. A chart
  holds four generations: down to the great-grandchildren.
- **Ancestors**: the person at the bottom, their father and mother indented under them in the
  outline (they sit above on the chart), and each one's parents under them, up to the
  great-grandparents. Two parents a person.
- **Names**: as typed, or the given names only (the first word of each).
- **Letter height** (7 mm), a **frame** round each name (none, box, rounded or oval, triple
  stitch), and the **connectors** (running stitch, satin or none; **Double** sews two lines side
  by side). Names, frames and connectors are each their own thread.

The chart goes in the middle of the hoop, made smaller to fit it if it is too big, down to 4 mm
letters; if it still doesn't fit, the status bar says so. It is one group, and one undo takes it
back. The window keeps the outline on this computer for next time; nothing is sent anywhere.
From the control channel or MCP: `embroidery.familyTree {people, chart?, names?, height?, color?,
frame?, frameColor?, connectors?, double?, connectorColor?, fit?}` → `{id, people, generations,
heightMm, widthMm, chartHeightMm, fits}`, and `ui.familyTree` opens the window.

## Cross-stitch patterns

A cross-stitch pattern is drawn square by square on a grid, the way counted cross stitch is
charted, and sews as X stitches that sit exactly in their squares.

- **Embroidery › Create › Cross Stitch › New Cross-Stitch Pattern…** (`embroidery.crossStitch.new {cols?,
  rows?, cell?, style?, at?}`) puts an empty grid on the page: 40 × 40 squares of 2.5 mm unless
  you ask for others (1 to 300 squares a side, 1 to 10 mm a square). Its thin grey lines show
  where the squares are and are never sewn.
- **The Cross Stitch tool** (in the Embroidery tools) paints squares: click one, or drag across
  several, in the tool's colour. *Erase* (or holding Alt) clears them instead, and *Brush* paints
  a block of 1 to 9 squares across. A square has one colour: painting it again changes it. A
  whole stroke is one step, so Undo takes it back. The Embroidery panel shows the tool's colour
  (the pattern's colours are one click away), Erase and Brush.
- **Fill a patch** (in the Embroidery panel, with the Cross Stitch tool) makes a click fill every
  square touching the one clicked that is like it (all empty, or all one colour, side by side,
  not corner to corner) in the tool's colour, or clear them with Alt
  (`embroidery.crossStitch.fill {at? | cell?, color?, erase?}`): a background filled in one go.
- **Backstitch lines** (in the Embroidery panel, with the Cross Stitch tool): drag from one corner
  of the squares to another to draw a backstitch outline, straight or slanted, in the tool's
  colour; Alt-click a line to take it away (`embroidery.crossStitch.backstitch {from?, to? | at?,
  color?, erase?}`). Each colour's lines are one object, joined where they meet, sewn last (over
  the crosses) as a running stitch a square long, so the needle goes in at the corners. A line
  drawn again in another colour changes colour.
- **Replace … With the Thread Colour** swaps one colour of the selected pattern for the tool's
  colour everywhere (`embroidery.crossStitch.replace {from, to?, erase?}`); when the pattern has
  that colour already, the two become one.
- **Fabric** sets the square size by the fabric's count: 11-, 14-, 16- or 18-count Aida (a 14-count
  square is 25.4 / 14 = 1.81 mm). `count` works wherever `cell` does (`new`, `fromImage`, `set`).
- **Embroidery › Create › Cross Stitch › Cross Stitch from Image…** (`embroidery.crossStitch.fromImage
  {cols?, colors?, cell?, style?, background?}`) turns the selected picture into a pattern over it:
  60 squares across and 8 colours unless you choose, each colour matched to a thread of the
  design's chart, stray single squares tidied, and the colour round the edges (the background)
  left unsewn unless `background` is on. Then paint over it to fix anything.
- Each colour is one object in the pattern's group, sewn row by row: going along a row it sews
  the "/" half of each X, coming back the "\\" half, so the top stitches all lie the same way,
  as hand cross stitch should. Lightest colours sew first.
- `embroidery.crossStitch.set {cell?, style?}` changes a pattern's square size (it grows or shrinks
  from its top left corner) or how squares sew: full X, half (one diagonal), upright (+) or
  double (X and +). `embroidery.crossStitch.info` lists its size and each colour's squares.
- Move the pattern as a whole (select the group); its squares and stitches move with it. Resize
  it with the square size rather than by scaling.
- **Embroidery › Create › Cross Stitch › Print Cross-Stitch Chart…** (or **Print Chart…** in the
  panel; `embroidery.crossStitch.chart {page?, square?, colors?}`) prints the pattern as a counted
  chart to stitch from by hand or to check against: a symbol in every square (each colour its
  own; up to 64 told apart by shape and fill) on its colour, heavier lines every 10 squares with
  their numbers along the top and left, as many A4 or Letter pages as it takes (each page says
  which columns and rows it shows), backstitch drawn over the squares in its colours, then a key:
  each symbol with its colour, thread and number of stitches, each backstitch colour and its lines, and the pattern's size in stitches, as sewn and on 14-count Aida. It opens as a new
  document to print or export as PDF; `colors: false` prints black symbols on white to save
  ink.

## Clipart

The Embroidery panel's **Clipart** section places simple motifs drawn for StitchFoundry (heart,
star, sparkle, crown, paw, leaf, flower, sun, moon, lightning, shield, snowflake, music note,
anchor, check mark, arrow) digitized in the design chart's threads, as Design from Prompt's art
is (`embroidery.clipart.insert {name, size?, at?, color?}`; `color` puts every part in one
colour). Designs of your own come in from the Design Library.

## Design templates

The Embroidery panel's **Design Templates** section (shown also with no document open) starts a
design from a template (`embroidery.template.new {name}`): **Left chest logo**, **Cap front**,
**Full back**, **Sleeve** and **Towel border** set the page, the hoop and a guide round the area
such designs fill; **Round patch 75 mm** and **Name tape 100 × 30** also have their satin border
digitized. **Save as Template** keeps the active design as a template of your own
(`embroidery.template.save`): its page, layers, art, hoop, fabric, machine and settings, opened
again as a new untitled design.

## Design library

The Embroidery panel's **Design Library** section shows a folder's designs as thumbnails of their
stitches: *Choose Folder…*, then *Find* by name and *Sort* by name, newest, most stitches or
largest; hover one for its size, stitches, colours and file, click it to open it, and *Print
Catalogue* makes a catalogue sheet of the designs found. Underneath,
`embroidery.library.scan {folder, query?, recursive?, sort?}` lists the machine embroidery files
in a folder and its subfolders (DST, EXP, JEF, PES, PEC, VP3, XXX, U01, PCS, TAP, DSB, DSZ): each with its
size, stitches, colours, threads, date and an outline of its stitches for a thumbnail; `query`
finds names and folders, and `sort` orders them by name, date, stitches or size. What a file says
is kept while it is unchanged, so looking again is quick; a file that can't be read is listed
with why. `embroidery.library.catalogue {folder, query?, columns?, page?}` prints them: a
catalogue sheet of thumbnails in their colours, each with its name, size, stitches, colours and
format, as many pages as it takes.

**Batch convert** (`embroidery.library.convert {format, folder?, files?, dir?, replace?}`) writes
a folder's designs (found as the library finds them, `query` and `recursive` too) or the ones
listed in `files` in another machine format: each beside the file it came from or into `dir`,
named as it was (a number added when the name is taken, unless `replace`). Designs already in
that format are skipped (in the Design Library section: **Convert to**, then **Convert All**
for the designs found), the files read are left as they are, and a file that can't be read is
listed with why. ZSK DSZ designs can be converted from but not to. A format without colours
(DST, EXP, TAP, U01, DSB) gives the converted design
standard threads.

## How much the tools show

The Home screen asks **How do you work?** (also **Window › Experience**), and each answer picks a
workspace and how much the Embroidery panel shows:

- **New to embroidery** (simple mode, below): a few large labelled buttons in place of the
  toolbar, **Your Design** in place of the panels, and short File, Edit, View and Help menus.
- **I digitize designs** (Digitizing): every digitizing tool with rulers, the control bar and the
  Layers panel; the font creator, team names, buttonholes and exports, but not job orders or batch
  output.
- **I run an embroidery business** (Embroidery Business): everything, job orders and batch output
  included.

**Everything** (the start) shows every section in the Essentials workspace. Each is a command too:
`window.experience {level: beginner|digitizer|business|all}`.

## Lining things up

While something is dragged with **View › Smart Guides** on (the default), the hoop's centre lines
show faintly across the sewing field, so it is always clear where the middle is.

- **Centre to centre wins:** a selection whose middle comes near the hoop's centre, or another
  object's centre, lands on it, even when one of its edges is a little nearer a line of its own.
- **Crosshair:** when the middles meet, a line runs both ways through them (across the whole hoop
  for its centre), both centres are ringed, and a label says **page center** (the hoop's centre),
  **centered** (on another object), or **page center line** / **center line** when only one way
  lines up. This works whatever point the object was grabbed by.
- **Edges** still line up with other objects' edges and the hoop's edges, with a line and
  **align**.

Hold the guides back with **View › Smart Guides** (Ctrl+U); **Edit › Preferences › Smart
Guides** turns the lines, the labels and spacing guides on or off and picks their colour.

## Make a Design

**Make a Design, Step by Step** on the Home screen (also **File › Make a Design…**, the
Embroidery menu and the first button in simple mode) walks through a whole design in seven
steps, with Back to change an answer:

1. **What would you like to make?** A name or some words, a shape, an appliqué, a picture, an
   in-the-hoop project or quilting for a tile (below), or a sample to try.
2. **Which hoop will you use?** My Machine's hoops (or the usual ones); the design is made to fit
   it. A sample comes with its own hoop.
3. **The words, the shape or the picture.** Words: what it should say and how tall the capital
   letters are (20–30 mm suits a towel, 10–15 mm a shirt). A shape or an appliqué: pick one; an
   appliqué piece stops for the fabric, tacks it down, stops to trim and sews the edge. A
   picture: choose one, and it is turned into stitches at most four fifths of the hoop (simple
   pictures with a few flat colours work best). A photo (told from a drawing by its many colours)
   is prepared first: with **Remove Background** on (the default) its subject is cut out of the
   background, the way **Remove Background** does it, then its colours are made flat (8 unless
   asked) and it is digitized in fewer, larger shapes than a drawing. Turn **Remove Background**
   off to stitch the whole photo, its background too (a landscape, a garden). **Add a white
   border round it** puts a 4 mm white band with a satin edge round the picture, like a sticker
   or a patch (see [Borders round a design](#borders-round-a-design)); the picture is made
   smaller to leave room for it. **Make It** builds
   the design (`embroidery.makeDesign {what, hoop?, text?, height?, shape?, path?, colors?,
   photo?, removeBackground?, border?, sample?}`; `photo` overrides the guess, `border` is the
   band's width in mm, 0 for none); a
   mistake (no words typed, a picture that can't be read) shows on the step.
4. **What fabric will you sew it on?** Woven, piqué (polo shirt), jersey (T-shirt), fleece or
   sweatshirt, towel, denim or canvas, leather or vinyl, a structured cap, or silk: the stitches'
   density, compensation and underlay are set for it (`embroidery.applyFabric`). A stretchy
   T-shirt needs much more holding than a towel. **Another fabric, or not sure** leaves the
   design as it is.
5. **Your thread colours**, in sewing order: click one to match a thread you have
   (`embroidery.recolor`).
6. **Watch it sew**: the sewing time, a warning if it is bigger than the hoop, and **Watch It Sew**.
7. **Save it**: **Save for My Machine** writes the machine's file to its USB stick; **Save
   Design…** keeps a copy to change later.

`ui.makeDesign` opens the guide.

## Simple mode

For someone who would rather not learn a drawing program. The left column has big buttons, each
with a one-line hint:

- **Make a Design…**: the step-by-step guide (above).
- **Select**: click a part of the design to choose it, drag it to move it.
- **Add Words…**: type a name or a few words and how tall they should be (8–80 mm, inches shown
  too); they land in the middle of the hoop as smooth satin letters in the current thread
  (`embroidery.addWords {text, height, color?}`).
- **Add a Shape…**: a heart, star, flower, leaf, sun, moon, paw, crown or snowflake, ready to sew
  in the middle of the hoop (`embroidery.addShape {name, size?}`).
- **Add Appliqué…**: the same shapes as appliqué pieces: the machine stops for you to lay the
  fabric, then sews it down (`embroidery.addShape {name, applique: true}`).
- **Add a Picture…**: places a photo or drawing; **Turn Into Stitches** then digitizes it in your
  thread colours.
- **Watch It Sew** plays the design stitch by stitch; **Undo** takes back the last change and
  **Redo** puts it back.
- **Start Over…** asks first, then clears everything in the design, keeping its hoop
  (`embroidery.startOver`); Undo still brings it back.
- **Save for My Machine** (below) writes the design for the machine.

**Your Design** on the right shows, with nothing chosen, the design's size (mm and inches),
stitches, thread colours and sewing time, and warns in red when it is bigger than the hoop. With a
part chosen it asks how to **Sew it as** (Filled, Smooth edge (satin), Just the outline, Appliqué
or Not sewn), its thread colour, **Smaller**, **Bigger** and **Delete**. The menus keep only File,
Edit, View and Help items a beginner needs; **Help › Show Everything** (or the link under the
buttons) goes back to the full program.

Simple mode also keeps the words plain and the screen calm: the document tab shows just the
design's name, the keyboard hints under the canvas are hidden, and three **A** buttons under the
column set the **Text size** of the whole window (normal, larger, largest: the UI Scaling
preference, 1×, 1.25× and 1.5×).

**Saved automatically.** Every few minutes (the Data Recovery interval), a design with changes is
saved on its own (`file.keepSafe`): in place when it is a StitchFoundry design already, else as
a new design in **My Designs** (Documents/StitchFoundry Designs, or the `myDesignsFolder`
preference): "My design 1", "My design 2"…, or the name of the machine file it was opened from,
which is never written over. Your Design shows where it went. Data Recovery's copies still cover
a crash in between.

## In-the-hoop projects

An in-the-hoop project is sewn whole, or nearly, in the hoop: the machine stops at each step for
you to lay, tape or trim fabric. **Make a Design › An in-the-hoop project** (or
`embroidery.ith.make {project, hoop?, text?, shape?, height?, color?, size?, edge?}`) makes one
with your words and a shape in the middle, in the smallest of My Machine's hoops it fits, turned the
right way round. Most pieces come **smaller** or **larger** (`size: small|large`, four fifths or
five quarters of their own size; the hoop grows to fit), all but the pillow fronts, the book cover,
the phone case and the passport holder, which fit what goes in them. The pieces finished in the
hoop with nothing sewn into their edge (the mug rug, coaster, ornament, key fob, luggage tag and
bookmark) take an **edge**: square corners, rounded corners, scallops all round, or round
(`edge: square|rounded|scalloped|round`), the placement line, tack-downs and satin edge all
following it.

- **Little pillow front, 8″ square** and **9½ × 5½″**: fronts for the little seasonal bench
  pillows that sit on a small bench and change with the months (forms of those two sizes). The
  hoop sews a panel as big as a 5×7-class hoop allows: a trim line, the design and a triple-stitch
  frame. Border strips and an envelope back, cut to the sizes given, make up the rest on the
  sewing machine.
- **Mug rug**, **coaster** and **round ornament**: finished in the hoop. A placement line; batting
  and top fabric tacked down; the design; the backing taped under the hoop and tacked on, with a
  stop to trim the layers; a satin edge all round.
- **Key fob, 1¼ × 5″**, **luggage tag, 2¾ × 4½″** and **bookmark, 2 × 6½″**: finished in the hoop
  the same way. The key fob and the tag come with their ribbon loop (for a split key ring, or round
  a bag's handle; `ribbon: false` leaves it out). On the narrow ones, the fob and the bookmark, the
  design is sized by the strip's width and the words turn to run along it, reading upwards.
- **Zipper pouch, 6 × 4½″** (from a 5×7 hoop): sewn inside out and turned. The outline and a
  zipper line 1″ below the top; the front fabric laid with its top edge on the line and tacked;
  the top strip laid with its bottom edge on the line and tacked; the design on the front, below
  the zipper; the zipper laid face up over the line and both its tapes sewn down, covering the
  fabrics' edges; then, with the zipper open halfway, the back laid face down and sewn all round.
  Trim ¼″ outside, turn it out through the zipper. It needs a nylon coil zipper (the needle sews
  over its ends), and takes no ribbon or vinyl.
- **Pencil case, 8 × 3½″** (from a 6×10-class hoop): the zipper pouch made long, for a 10″ zipper
  or longer.
- **Card holder, 4 × 3″**: finished in the hoop like the coaster, with a slip pocket over its
  lower half. After the design (in the upper part, since sewing through the pocket would close
  it), a line marks where the pocket's fold goes; the pocket fabric, folded in half, is laid with
  its fold on the line and tacked along its sides and bottom; the backing and the satin edge close
  them, and the fold stays open for cards.
- **Phone case, 3½ × 6½″** (from a 5×7 hoop): sewn inside out and turned. The outline; the front
  fabric, its top folded under ½″, laid on it and tacked; the design; then the back fabric, its top
  folded under too, laid face down and sewn along the sides and bottom only. Trim, turn it out
  through the open top and topstitch the top. No ribbon or vinyl.
- **Passport holder, 4 × 5¾″** (from a 5×7 hoop): sewn as the phone case is, sized for a passport
  of the usual size (125 × 88 mm) slid in closed through the open top, with ½″ of room across and
  ¾″ in length for its thickness. Leave out the fleece if it fits tight.
- **Book cover, for a 5½ × 8½″ book**: the hoop sews the front panel like a pillow front (a trim
  line, the design, a frame); borders bring it to the cover's size, a back and spine piece is
  joined to it, and a lining and two folded flaps make it up. The sizes are for a book ¾″ thick.

Each step is its own thread, named for what to do ("Placement: lay batting, then top fabric over
the line", "Backing: tape the backing fabric under the hoop", "Edge: satin all round"), so the
machine stops between steps and its screen says what comes next. The supplies with their cut
sizes (inches and millimetres, ¼″ seams) and the steps are kept as the design's notes, printed on
its worksheet: in simple mode **Print the Steps…** under Your Design.

**Ribbon, vinyl and a button** (`ribbon`, `vinyl`, `button`; tick boxes in Make a Design):
- **Ribbon**: on a piece finished in the hoop, a hanging loop at the top: a mark shows where the
  folded ribbon's ends go (the loop out beyond the edge, taped out of the way), then the machine
  tacks the ends down and the backing covers them. On a pillow front, a ribbon band across the panel
  below the design: a line for its top edge, then both its edges sewn down.
- **Clear vinyl on top** (pieces finished in the hoop): after the design, lay clear vinyl over the
  piece and tape it outside the stitching; it is tacked with long stitches, which don't tear it,
  for a wipe-clean mug rug or coaster.
- **A button mark**: a small cross where a button is sewn on by hand afterwards.

Each is a step of its own, with its own named thread, and goes into the supplies (ribbon width and
length, vinyl size, the button).

**Cutting templates** (`embroidery.ith.templates`, or **Print the Cutting Templates…** at the end
of Make a Design): every piece to cut (top fabric, batting, backing, vinyl, the trimmed panel, the
borders, the envelope back) drawn life size on Letter pages with its name and size, a 1″ square on
each page to check the print is at 100%; a piece bigger than the page is drawn smaller with its
true size written.

**Designs you bought.** Your Design lists any design's steps, its threads in sewing order by the
names the file gives them, so an in-the-hoop design from a kit reads as its steps; and Smaller or
Bigger asks first on a design that stops for fabric (or carries a project's cut sizes), since a new
size would no longer match fabric cut to size. Projects and wording here are our own; we don't
copy any company's designs, files or instructions.

## Quilting block by block

Quilting a whole quilt on an embroidery machine, one hooping at a time, with clear marking tiles
(the kind sold as tile quilting systems): lay a tile the size of the block on the quilt, mark its
middle, crosshairs, up arrow, sides and corners with a water-soluble pen, hoop the quilt sandwich
with the marks centred, and quilt the design that size. **Make a Design › Quilting for a tile**
(or `embroidery.quiltBlock {width, height, unit?, pattern?: stipple|swirls|loops|feathers|pebbles|waves|spiral|crosshatch, spacing?, stitch?, color?, hoop?}`)
makes that design:

- the block's rectangle quilted edge to edge in **one running-stitch thread** (no trims), in the
  smallest of My Machine's hoops it fits, turned the block's way round, top up;
- patterns: **Stipple** (a wandering meander), **Swirls** (a spiral in and its echo back out,
  swirl after swirl), **Loops** (rows of round cursive loops), **Feathers** (rows of teardrop
  plumes, each leaning back over the one before), **Pebbles** (round pebbles of mixed sizes,
  each sewn round once and half again to the next), **Waves**, **Squares spiral** and **Crosshatch**;
  the row patterns turn at the block's sides, so each is still one line;
- every block keeps its lines half a line-spacing in from its edge, so with the same pattern and
  **Lines apart** for every block, blocks of any size side by side read as one continuous quilting
  with no lines to match up.

**A block**, **A border strip** or **A corner**: border strips are long and narrow (1 × 4″ to
3 × 10″); every pattern fits one, its motifs squeezed to the strip's height when the strip is
narrower than they are. A corner is the square where two strips meet: the border's row bends round
a quarter circle, in from the right edge's middle and out through the bottom's, so the border turns
the corner (made for the quilt's top left corner; turn the hoop, or the design on the machine, for
the others; `corner: true`).

**In the middle**: words, a shape, or a shape sewn as appliqué in the middle of a block, sewn first;
the quilting goes round it, `clearance` (4 mm) away: a stipple or spiral leaves its outline out,
and a row pattern that would cross it follows its outline instead and carries on, so it is still
one line (`text`, `shape`, `applique`, `clearance`). **Make Every Size…** (`embroidery.quiltBlock.set {folder, pattern, kind?: blocks|borders|corners, sizes?, spacing?, format?}`)
writes one machine file per block size to a folder or the USB stick, named by size ("Stipple 5x7
in.vp3", "Border Swirls 7x1.5 in.vp3", "Corner Swirls 2x2 in.vp3"): the usual sizes from 2 × 2″ to 9 × 14″, or the ones you list, so you pick the file that
matches the size written on the tile. Sizes no hoop of yours takes are left out and said. The
design's notes say how to mark and hoop. Patterns and wording are our own.

## Chenille

For chenille machines, which sew thick yarn without a bobbin thread, any stitched object can be
chain or moss (Embroidery › Special Machines › Chenille › Chain, Moss or Lockstitch, or the Embroidery panel's **Chenille** row;
`embroidery.chenille.set {stitch: chain|moss|none, height?, pattern?}`):

- **Chain** sews link-like loops on top: outlines, borders, monograms and light fills. The
  object keeps its stitch type (a run is usual).
- **Moss** sews a dense looped pile for fills. Its loops follow the **coil** or **square loop**
  pattern: a fill becomes a motif fill and a run a motif run in that pattern. Satin can't be moss.
- **Needle height** (1 to 9, the machine's own levels): lower for chain and outlines (3 unless
  set), higher for moss fills (6).
- **Compound Chenille…** (Embroidery › Special Machines › Chenille, or the panel with a fill selected;
  `embroidery.chenille.compound {before?, after?, offset?, pattern?, height?, borderHeight?}`)
  makes the shape a moss fill with chain borders sewn before it and after it (1 each unless set, up
  to 3), following its outline inward `offset` apart (1.5 mm). The borders tie the moss in, as
  chenille has no bobbin thread to hold it.
- Machine files stop wherever the colour, the stitch (chain or moss) or the needle height
  changes, one stop each, as chenille machines that work with single stop codes take them; the
  machine starts in chain. The machine can't tell those stops apart, so **Chenille stops** in the
  Embroidery panel (`embroidery.chenille.stops`) lists what to set at each one. Tajima's own multi-stop chenille code tables aren't publicly described, so they aren't
  written.
- The canvas shows chenille objects with their ordinary stitches: chain and moss loops are formed
  by the machine.

Public descriptions: https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Chenille/chenille_basics/chenille_basics-3.htm,
https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Chenille/chenille_output/Output_to_Tajima_chenille_machines.htm.

## Longarm quilting

Computerized longarm quilting machines sew one unbroken line across the quilt: they can't trim
and jump the way an embroidery machine does. **Embroidery › Special Machines › Longarm Quilting** makes and checks
such lines, drawn as ordinary lines (a stroke, no fill), and writes them for the quilting
software that runs the machine:

- **Check Continuous Line** (`quilt.check {tolerance?}`) says in the status bar whether the
  selected lines are one continuous line, or how many joins they need and the longest.
- **Join into One Line** (`quilt.join`) chains the selected lines and shapes' outlines into one,
  from the leftmost end, each time on to the nearest end (a closed shape is sewn all the way round
  from its nearest point); the joins are straight runs.
- **Start and End on the Edges** (`quilt.edges {height?}`) leads the line in from the left edge
  of its box and out to the right edge at the height it starts at, so copies set side by side run
  on from one to the next: the motif for rows and borders.
- **Edge-to-Edge Rows** (`quilt.pantograph {width, height, row?, nest?}`): the selected line
  repeated across a quilt in rows, each row one continuous line; every other row starts half a
  repeat along so the rows nest. The menu has the usual quilt sizes (Crib 36 × 52″ to King 108 ×
  108″); the Embroidery panel's **Longarm Quilting** section takes any size and row spacing.
- **Borders and corners** (the panel's section; `quilt.border {length, depth, down?}`): the
  line fitted a whole number of times along a border, and with a corner leg the border turns a
  quarter at its end and runs down the side.
- **Continuous Fill** (`quilt.fill {kind, spacing?}`): **Stipple** and **Meander** (a line that
  never crosses itself, paths about `spacing` apart, 6 mm unless set), **Echo** (rings following
  the shape inward, joined into one line) inside each selected closed shape, and **Stitch in the
  Ditch** along all the selected shapes' outlines as one line.
- **Export Quilting Lines** (`quilt.export {format}`, File via `file.export.quilt`): the selected
  lines, else every line, as **HP-GL PLT** (40 units a millimetre), **DXF** (R12 polylines in mm,
  single stroke) or **SVG** (polylines in mm). The machines' own formats (QLI, HQF, IQP, BQM, CQP,
  PAT, SSD) have no public description, so they aren't written; quilting software reads DXF, SVG
  and PLT and writes them.

## Large designs

Designs of 100,000 stitches and more stay quick to work with: a change to one object restitches
only that object, the canvas redraws only the objects whose stitches changed, and panning and
zooming move what is already drawn. Zoomed out, where a thread is thinner than two screen pixels,
TrueView draws each stitch as a flat strip in its thread's colour (its shading would not show).
On the desktop, shapes are converted to stitches on several processor cores at once.
`stitchfoundry-cli perf --embroidery 120` times a 155,000-stitch design (planning, a change, a
move, statistics, DST and VP3 files) against its budgets.

## Embroidery in the web app

The web app (the same program, running in a browser with WebGPU or WebGL2) makes designs the way
the desktop app does: Make a Design, words, shapes, appliqué, in-the-hoop projects, quilting tiles,
Watch It Sew and My Machine all work. What changes is where files go, since a browser page can't
write to folders or a USB stick:

- **Save for My Machine** downloads the design in My Machine's format (VP3 for the Designer
  Diamond deLuxe), named after the design; copy it to the machine's USB stick;
- **Save Design** downloads the `.stitchfoundry` design file, to open again later;
- **Make Every Size** (quilting tiles) downloads one zip holding a file for each size
  (`embroidery.quiltBlock.set` with no `folder`);
- printing (the steps, cutting templates) goes through the browser's print dialog;
- Keep Safe's automatic saving to My Designs is desktop only; the web app keeps recovery copies in
  the browser's storage instead.

**The free viewer** (VIEW-01): add `&view` to a link that opens a design (`?open=…&view`, a design on
MakerFoundry) and the web app shows just the design, with nothing to edit: its name, its
size, stitch count and sewing time, and its threads in sewing order (point at one for its name).
**Get StitchFoundry Free** goes to stitchfoundry.app; **Edit It Here** opens the full app with the
design already open. `ui.viewer {"on": true|false}` switches it from the control channel.

**Tablets** (TB-01): the web app runs on iPad and Android tablets in Safari or Chrome. The first
time it opens on a touch tablet (a touch screen whose main pointer is a finger, so touch-screen
laptops aren't counted) it starts in simple mode: the big buttons on the left and Your Design in
place of the side panels, which leave too little room for the design on a tablet. Choosing another
way of working (Window › Experience, or *Show everything*) is kept from then on. Pinch zooms and two
fingers pan the canvas. Saving downloads the machine file, to copy to a USB stick.

## Life size on screen

**View › Calibrate Life Size…** shows a 100 mm ruler and the outline of a bank card (85.6 ×
53.98 mm). Hold a real ruler or card against the screen and change the size until they match,
then **Use This Size**: View › Actual Size (Ctrl/Cmd+1) shows designs life size from then on, so
you can judge lettering heights and stitch lengths as they will sew. The measurement is kept as
the `screenPointsPerInch` preference (General) and turns on **Display Print Size at 100% Zoom**;
`ui.calibrateLifeSize {pointsPerInch}` sets it directly. Calibrate again on another screen.

## Worksheets

**Embroidery › Worksheet** (`embroidery.worksheet {kind?, customer?, notes?}`) makes a worksheet
for the design as a new A4 document, to print (File › Print) or export as PDF:

- the design drawn in its thread colours, life size when it fits (else scaled, the scale said);
- its size, stitches, colours, colour changes, trims, thread (top and bobbin) and sewing time at
  the machine's speed, and its hoop, fabric and machine;
- its colours in sewing order: a swatch, the thread's code, name and brand, stitches and length;
- a barcode of the job at the top right (`barcode: code128|code39|none`, Code 128 unless said):
  the design's name, or `barcodeText`, to scan at the machine or the counter;
- the design's colorways, each with its threads colour by colour, the one in use marked
  (`colorways: false` leaves them out);
- the notes given, else the design's own **Design notes** (set in the panel's Design section,
  or `embroidery.settings {notes}`, and kept with the design), wrapped to the page; an
  **approval sheet** (`kind: approval`) adds the customer and a line to sign.
- a **pattern** (`kind: pattern`) is the design over the whole page with one line of figures;
- a **placement template** (`kind: template`) is the design life size, with lines through its
  middle to the page's edges, an arrow for which way is up and a 100 mm bar to check the print
  wasn't scaled: cut it out or lay it on the garment to mark where the hoop goes;
- `placement: left-chest|right-chest|full-front|full-back|cap-front|sleeve|yoke|pocket` prints
  the usual size and place for it (typical values: garments and customers vary);
- on A4 or US Letter paper (`page: a4|letter`); a logo or anything else can be added to the sheet
  like to any document (File › Place).

**Worksheet setup** (`embroidery.worksheet.setup {company?, details?, logo?, hide?}`) makes the
worksheets the shop's own: its name and details (address, phone, web) in every worksheet's
footer, its logo (an image file) at the bottom right, and the sections left out (`figures`,
`colors`, `colorways`, `notes`, `barcode`); `hide` on a worksheet overrides it for that one.

The worksheet's text is in English for now.

## Job orders

The Embroidery panel's **Job Orders** section keeps customers' orders: **New Job for This Design**
(`embroidery.job.create {customer, design?, quantity?, due?, garment?, placement?, notes?}`) numbers
it (J-0001 on) and remembers the active design's file; each open job shows its status (open, in
production, done, cancelled: `embroidery.job.update`) and a **Worksheet** button
(`embroidery.job.worksheet`): the design opened if it isn't already, and a worksheet with the
customer, the job's number as its barcode, the quantity, due date, garment, placement, notes and
the cost for the quantity. `embroidery.job.output {id, dir?, profile?…}` writes the job's files as
Batch Output does, named after the job's number. `embroidery.jobs {status?, query?}` lists them.

## Batch output

`embroidery.output {dir, formats?, worksheet?, design?, name?, replace?}` writes everything a job
needs to a folder in one step: the design's machine files in each format asked for (the design's
machine's when none are), its worksheet as a PDF (`name-worksheet.pdf`; `worksheet:
production|approval|pattern|template|none`) and the design itself (`.stitchfoundry`; `design:
false` leaves it out). The files are named alike: when any name is taken, a number is added to
all of them (`Logo-2.dst`, `Logo-2-worksheet.pdf`…) unless `replace` is set. The documents open
stay as they are.

In the Embroidery panel, **Batch Output** ticks the formats, chooses the worksheet and whether
the design goes too, then **Write to Folder…**; **Write Again** writes to the same folder. A
profile chosen there fills the choices in; **Save Profile** keeps them under the name typed.

**Output profiles** keep a batch by name: `embroidery.output.saveProfile {name, formats?,
worksheet?, design?, dir?}`, listed by `embroidery.output.profiles` and forgotten with
`embroidery.output.deleteProfile`. `embroidery.output {profile}` uses one, its folder too; what
the request says wins over it.

## Product visualizer

**Embroidery › Product Visualizer** (`embroidery.visualize {product?, color?, placement?}`) shows
the design, life size and in its thread colours, on a product drawn in the product's colour: a
T-shirt, polo, hoodie, cap, tote bag or towel. Caps come in four styles: a structured ball cap
(`cap`), a snapback with a flat bill (`snapback`), a trucker cap with a mesh back (`trucker`) and
a low, unstructured dad cap (`dadcap`, whose front takes designs up to about 100 mm). It goes
where such designs usually go (`placement`: left or right chest, full front, full back, back
yoke, sleeve or pocket on garments; the front of a cap; the middle of a bag; above a towel's border), seen from the back
for the full back and yoke. A design wider than usual there is warned of. With `image` it goes
on a photo of the customer's own product instead: `imageWidth` says how many mm the photo shows
across, `at` where the design's middle goes. The mock-up opens as a new document, to show the
customer, print or export (PNG, PDF).

As placement aids, `areas: true` draws the usual work areas of the side shown on the product,
dashed and named with their sizes (left and right chest, full front, sleeve; full back and yoke
from the back; a cap's front, a bag's middle, a towel's band above its border). On shirts and
hoodies the caption, and `belowNeckMm`, say how far below the neckline the design starts, the
way placements are measured on a garment. Printed life-size placement templates come from the
worksheet (`kind: template`), and a basting run round the design from the design settings.

## The lettering kiosk

**Embroidery › Create › Lettering Kiosk** (`embroidery.kiosk {on: true}`) turns the app into a simple
screen for counter staff: one panel and the design, nothing else (no menus, panels or
keyboard shortcuts). They pick a template (a name
tape, a cap front, a left chest…), type the name, pick a font and a thread, and **Make Design**
(`embroidery.kiosk.order {text, template?, font?, color?, size?}`) starts a design from the
template with the name set in the font, centred in the template's area and made as large as fits
(or the size asked for), in the thread. **Show on Product** puts it on a T-shirt, polo, hoodie,
ball cap, snapback, trucker or dad cap, bag or towel (the Product Visualizer); **Send to Machine** queues it for the shop's machine
folder, **Save Machine File…** saves it; **Next Name** starts again.

The shop chooses what the counter offers (`embroidery.kiosk {templates, fonts, colors, folder,
format}`: empty lists offer everything) and can lock the kiosk with a PIN of 4 to 12 digits
(**Lock** on the kiosk screen, or `pin`): while it is on and locked, changing it or **Leave
Kiosk** takes the PIN. The kiosk stays on across restarts, so a counter computer starts in it. The
PIN keeps the full app out of reach by mistake; it is not a security barrier, since the
preferences file can be edited.

## Online orders

**Embroidery › Online Orders…** reads the orders from an online shop and makes each personalized
item into a design: the name or text the customer typed, in the font and thread colour the rules
give, fitted to the rule's template. Each item also gets a job order (the shop's order number,
customer and quantity), its machine files and a worksheet with a barcode, all in the orders folder
(My Designs/Orders unless another is chosen). Shopify and WooCommerce are read directly (the keys
are kept in the system's password store); Etsy and other shops through their CSV order export.
Each order line is made once; **Check every 15 minutes while open** keeps checking. **Find a job**
takes a worksheet's barcode, a job number or the shop's order number. Rules: the first whose
product text is in the item's name or SKU makes it; an order field named font or colour overrides
the rule's. Setting up each shop: [`online-orders.md`](online-orders.md). Commands:
`embroidery.orders.setup`, `embroidery.orders.import {source, data, dryRun?}`,
`embroidery.job.find {code}`.

## Sequins, beads, rhinestones, cord and boring

Seven stitch types decorate instead of stitching a fill (Embroidery panel › Stitch type, or
`embroidery.set {type: sequin|bead|stone|cord|boring|print|foil}`); their settings change with the panel or
`embroidery.setParams`:

- **Sequins**: round, square, star, heart or flower sequins (`shape`, `size` mm across), placed
  along the line (`layout: run`), packed inside the shape (`fill`: a honeycomb or a grid, rows at
  `angle`, sewn back and forth), round the inside of its edge (`border`) or one at each point
  drawn (`manual`). `spacing` sets the gap between them (negative overlaps them: minus half the
  size makes a half-overlapping run), `sizes` takes several sizes in turn (multi-sequin), `flip`
  shows every other one's back, and `color` their own colour. Each is held by fixing stitches
  from its hole over the edge (`fixing`: single, the way the run goes, under the next sequin;
  double; three; or a cross).
- **Beads**: seed or bugle beads along the line, in the shape or at points; each sewn through
  and back so it lies flat.
- **Rhinestones**: stones (SS6 to SS30, by size in mm) along the line, round the edge, packed
  inside the shape or at points, round or square. They aren't sewn and add nothing to the
  machine file; export their template (**Cut File**, below) to cut, brush the stones into and press.
  Setting type to rhinestones makes bling lettering and bling borders.
- **Cord**: a cord couched along the line with a zigzag a little wider than it (`size`: the
  cord's width).
- **Print** and **Foil**: the shape shown printed, flat in its colour, or as pressed foil with
  a metallic sheen, under the embroidery: virtual decoration for mixed-media designs. They aren't
  sewn and add nothing to the machine file.
- **Boring**: each closed shape a cut-work hole: its outline run, a stop to cut it, then a
  satin edge round it (`size`: the edge's width).

**Cut File** (Embroidery menu, `embroidery.cutFile {what?, format?, mirror?, path?, open?}`) writes
what is cut rather than sewn, in millimetres, as SVG or DXF (R12 lines and circles, for cutters
and laser tables): each rhinestone a hole 0.3 mm larger than its stone (the rhinestone template,
to cut in template material, brush the stones into and press), and each appliqué piece's
outline (to cut the fabric by hand or on a cutter). `what` takes `stones`, `applique` or `all`;
`mirror` flips it for material cut from its back (heat-transfer flock, fabric with fusible
backing). `open: true` opens it as a new document, life size, to print as a cutting template.
It also lists the stones by size and colour, to order and to set.

The machine stops before each sequin, cord and boring object (`stop`), for the operator to load
the sequins, lay the cord or cut the holes: the codes that drive sequin, cording and boring
devices on multi-head machines aren't publicly documented, so they aren't written. The canvas
shows the sequins, beads, stones, cord and holes as they will look, the stitches over them
(stones over everything). **Count Decorations** (`embroidery.decor.count`) lists the sequins,
beads and stones by kind, size and colour, and the cord's length, to order supplies;
`embroidery.decor.palette` gives the standard sizes.

## Merging colours

**Merge Colours** (`embroidery.colors.merge {tolerance?, sequence?}`) makes the design's threads of
the same colour, or near enough to pass for it (`tolerance`, a ΔE: 3 by default, 0 for only the
very same colour), one thread: each becomes the one sewn first, its name and code kept. The design
is then sequenced again (as Auto Sequence, keeping objects that overlap in their stacking order) so
the merged colours' blocks join: fewer colour changes at the machine. An object given its own
colour block keeps its stop. It is one undo step, and says how many threads and colour blocks
there were before and after.

## Thread charts of your own

Besides the built-in charts, the Design section keeps charts of your own: **Import Chart…** reads
a CSV file (`embroidery.chart.import {name, path | text}`), from a supplier's list or a shop's
stock: a header naming the columns (`color`, `colour` or `hex`, or `r`, `g` and `b`; `code` or
`number`; `name` or `description`), or rows of a code, a name and a `#rrggbb` colour, with
commas, semicolons or tabs between. It reads a GIMP palette (`.gpl`) too, the format many
thread charts are shared in: a colour a line as red, green and blue and a name, a number at the
start or end of the name (`1800 Black`) taken as the thread's code. **Export Chart…** writes the chosen chart as CSV
(`embroidery.chart.export`, `code,name,color`). `embroidery.chart.save {name, threads?}` makes a
chart thread by thread, or from the design's threads when none are given. Charts of your own are
used like the built-in ones: to pick a thread, to match threads to a chart and to recolour.

## Recolouring threads

**Recolour Threads** in the Design section (`embroidery.recolor`) changes the design's threads at
once, the art repainted to match, in one undo step: by a `map` of colours (`[{from, to}]`, every
thread of a colour), or by a `scheme`: the hue turned (`degrees`), every thread in one colour's
hue at its own lightness (`mono`, `color`), grey by lightness, or inverted. With `chart` (the
panel uses the design's) each new colour becomes that chart's closest thread. It works on the
selection's stitched objects, or all of them when nothing is selected. Colorways keep several
recolourings of one design side by side.

## Hoops

The Embroidery panel's Design section picks the design's hoop from the standard ones (Brother,
Janome, Bernina, Husqvarna Viking, Pfaff creative, Tajima, cap, sleeve and jacket-back frames, and
magnetic frames by size; 60 in all, their sewing fields from the makers' public product pages) and
one's own. **Smallest That Fits** (`embroidery.hoop.auto {brand?, turn?, special?}`)
chooses the smallest hoop that holds the design, turned a quarter when that fits better (`turn:
false` keeps hoops as they are; cap, sleeve and jacket-back frames only with `special`; `brand`
keeps to hoops whose names have it). **Rotate Hoop** (`embroidery.hoop.rotate`) turns the hoop a
quarter. **In the hoop** chooses where the design sits: centred (as machines centre designs) or at a
fixed place, x and y mm from the hoop's centre (`embroidery.settings {hoopPosition: [x, y] |
"centre"}`): the machine files are written with the design there, and the hoop outline and the
fit check follow. **Custom Hoops** keeps hoops of one's own by name with their field
(`embroidery.hoop.saveCustom {name, width, height, shape?}`, `.deleteCustom`), listed with the
standard ones everywhere a hoop is chosen (`embroidery.hoops` lists them all, with whether the
design fits each).

## Garment photo

**Garment Photo…** in the Design section (`embroidery.background {path, width?, at?}`) puts a
photo of the garment behind the design while it is digitized, life size (`width`: the mm the
photo shows across, 500), its middle on the design's: on a locked template layer below the
current one, so it is never stitched, printed or exported with the design. Delete the layer to
take it away. The fabric colour still tints the artboard under the stitches.

## Multi-hooping

A design larger than the hoop is sewn in several hoopings: `embroidery.multiHoop {hoop?, width?,
height?, overlap?, dir?, format?, marks?}` lays a grid of hoopings over it that overlap by
`overlap` (10 mm), puts each object, whole, in the first hooping that holds it, and writes each
hooping as a machine file of its own (`name-1`, `name-2`…, each centred on the hoop) into `dir`.
Each hooping first sews **registration crosses** in the overlaps it shares with its neighbours,
in a colour of its own: re-hoop the fabric so the next hooping's crosses land on the ones already
sewn, then sew on. Without `dir` it only plans (which object goes where). An object larger than any hooping is split (`split: false` refuses it instead): each
stitch is sewn in the hooping whose middle is nearest it, and each piece is tied in and off and
trimmed, so the pieces meet in the overlaps; a stitch longer than half the overlap is refused
(widen the overlap).

`sheets: true` also opens a document of a page per hooping to print and keep by the machine:
the hooping's stitches in their colours inside the hoop's field (life size when it fits the
page), its place in the grid, its file and how to line the next one up.

A **multi-position hoop** (Multi-position 130 × 300 with three 130 × 180 positions, 200 × 360
with two 200 × 200) is hooped once: with one, the frame's positions are the hoopings, each a
machine file to sew with the frame at that position, and no registration marks are sewn.

The marks are set with the design (`embroidery.settings {registration: {shape, size, perOverlap,
color}}`): crosses, rings or targets (a ring with a cross), how far they reach (3 mm), how many
in each overlap (2; 1 to 4) and their thread's colour (light grey).

## Stitch image

**File › Export › Machine Embroidery File › Stitch Image PNG** (`embroidery.exportImage {path?,
ppmm?, background?, margin?, view?}`) draws the design's stitches as lit thread, as the TrueView
display shows them (`view: flat` for flat colour), and writes a PNG: for a website, a quote or a
catalogue. `ppmm` sets the pixels a millimetre (10; up to 40 for print), `background` white, a
colour or `transparent`, `margin` the mm round the design (3).

**Sew-out Animation GIF** (the same menu, or **Save as Animation…** under the stitch player;
`embroidery.exportAnimation {path?, frames?, seconds?, hold?, ppmm?, background?, margin?, view?}`)
writes the design sewing itself, stitch by stitch in sewing order as the stitch player shows it, as
an animated GIF that loops: for a website, a listing or a message. It has 60 frames (10 to 240)
over 6 seconds (2 to 60; from the player, its speed sets it), then holds the finished design for 2
seconds before it starts again, at 4 pixels a millimetre (up to 12).

**Sew-out Video MP4** (the same menu, or **Save as Video…** beside it; `format: "mp4"`, or a
`.mp4` path) writes the same as an MP4 video, which phones and social sites play where they won't
take a GIF: H.264, Baseline profile, every frame a key frame, encoded by the pure-Rust `less-avc`
(MIT OR Apache-2.0) and put in an MP4 file written by StitchFoundry from ISO/IEC 14496-12 and -15.
Its frames are uncompressed inside, so a six-second clip is about 10–20 MB; sites recompress it on
upload. H.264 is a patented format, whose licence has asked nothing for the first 100,000 copies of
an encoder a year (the owner's decision, 2026-10-24, to use it).

**Embroidery › Print Stitch View** (`embroidery.printView {scale?, page?, view?}`) opens the
stitch view as pages to print (File › Print) or export: life size over as many A4 or Letter pages
as it takes (the pages touching, to tape together), or fitted to one page (`scale: fit`).

## Stitch lists and G-code

Besides the machine formats, `embroidery.export` (and Batch Output, Send to Machine) writes the
stitches as open text StitchFoundry defines, to read into a spreadsheet or a script or to drive
a home-built machine:

- `csv`: a line a stitch, `index,command,x,y,thread,color,name` (mm from the design's centre, y
  down; command `stitch`, `jump`, `trim`, `color`, `stop` or `end`; thread from 1);
- `json`: `{format: "stitchfoundry-stitches", version: 1, units: "mm", name, threads: [{color,
  name, code, chart}], stitches: [[x, y, command]…]}`;
- `gcode`: millimetres, absolute (`G21 G90`); a jump is `G0`, a stitch a `G1` move then a needle
  cycle: the Z axis advances by `zStep` (1; 0 for none) and/or `stitchCommand` is written (for a
  needle relay, say `M3`); a thread change or stop writes a comment naming the next thread and
  `colorCommand` (`M0`, a pause); a trim a comment and `trimCommand`; y counts up unless
  `flipY: false`; `feed` sets the stitching speed (1200 mm/min).

These are exports only: they aren't read back.

## My Machine

**File › My Machine…** (also on the Embroidery menu and the Home screen) asks once which machine
you have: Husqvarna Viking Designer Diamond deLuxe, other Husqvarna Viking models, Pfaff creative,
Brother or Baby Lock (home or multi-needle), Janome or Elna, Bernina, Singer, a commercial
multi-needle machine (DST), or another machine (DST). It also asks whether to measure in
millimetres or inches, and which folder or USB stick designs go to. From then on:

- the Home screen and Make a Design offer that machine's hoops, and a new design uses its first
  hoop when none is named (`embroidery.newDesign`). For the Designer Diamond deLuxe they are the
  three it comes with, Crown 260 × 200, Royal 360 × 200 and Splendid Square 120 × 120 mm (as
  Husqvarna Viking's feature sheet lists them), then the optional 240 × 150, Majestic 360 × 350,
  Quilter's 200 × 200, Mega Endless 260 × 150, Small Square 80 × 80 and 100 × 100 mm;
- **My own hoops**, at the bottom of the window, adds a hoop that isn't listed: its name, width and
  height (in the units chosen) and square or oval. It joins the machine's hoops on the Home screen,
  in Make a Design and everywhere a hoop is chosen, and Remove takes it away again
  (`embroidery.hoop.saveCustom`, `.deleteCustom`);
- new designs and exports use the machine's format (VP3 for Husqvarna Viking, PES for Brother,
  JEF for Janome, EXP for Bernina, XXX for Singer, DST otherwise);
- **File › Save for My Machine** writes the design in that format to the chosen folder in one step
  (`embroidery.send`), asking for a folder first when none is chosen or the USB stick isn't
  plugged in. In the web app, which has no folders, it downloads the file instead: copy it to the
  USB stick. When nobody has said which fabric the design goes on, it asks first (**Which
  fabric?**, the same choices as Make a Design) and sets the stitches up for it; **not sure**
  saves it as it is and isn't asked again for that design.
- After it saves in the desktop app, **Saved for your machine** shows where the file went, with
  **Eject USB Stick** (so the stick can be pulled out without losing the file: macOS's `diskutil
  eject`, udisks or GIO on Linux, the Explorer's Eject on Windows) and **Show in Folder**. The
  same is `ui.ejectMachineFolder {folder?}`; a folder that isn't on a removable drive says so.
- On a Mac, StitchFoundry is the default app for machine files (DST, PES, JEF, EXP, VP3 and the
  rest) unless another app claims them, so double-clicking one in the Finder opens it here.

Commands: `embroidery.myMachine` (the machine, its format, hoops and folder, one's own hoops
(`own`), and the list to choose from), `embroidery.myMachine.set {machine: name|null}`, `ui.myMachine`, `ui.saveForMyMachine
{folder?, fabric?}` (with no fabric, a design that has none answers `{askFabric: true}` and the
question opens).

## Send queue

**Send When Ready** in the Send to Machine section (`embroidery.queue.add {folder?, format?,
name?}`) queues the design for the chosen machine folder: written at once when the folder is
there, else kept until it is (a USB stick plugged in, a share back on the network); while
designs wait the panel looks every few seconds and sends them in the order they were queued
(`embroidery.queue.run`). The queue is kept across sessions; `embroidery.queue` lists it with
what was sent where, and **Clear Sent** (`embroidery.queue.remove {done: true}`) tidies it. The
machines are reached through the folders they read: their makers' network protocols aren't
publicly documented.

## Export history

Every export that writes files is remembered for the session, newest first: File › Export,
Send to Machine, Batch Output, multi-hooping and batch convert, each with the document it came
from, the files it wrote, or why it failed (`export.history`). **Export Again** (`export.again
{index}`) runs one again as it was asked, from the active document, writing the same files
again; the Embroidery panel's **Exports** section lists the latest with an **Again** button.
Exports are encoded and written off the UI thread while you go on working when **Background
Export** is on (Preferences › File Handling).

## Send to machine

**Embroidery › Send to Machine** (`embroidery.send`) writes the design's machine file (in its
machine's format, as Export Machine File does) straight into the folder the machine reads: a USB
stick, the machine's own folder, or a network share (a hot folder). Add the folder once in the
Embroidery panel's *Send to Machine* section (*Add Machine Folder…*); it is remembered, the last
used first, and *Send* writes the file there under the design's name (a number is added when a
file of that name is there already; `replace: true` writes over it). The folder is checked first:
when it isn't there (the stick taken out, the share unreachable) the send stops and says so.
`embroidery.send.targets` lists the folders and whether each is there now. Machines that only take
designs over their maker's own wireless link are not reached directly; send to a folder they read.

### Memory cards

Older machines read designs from a memory card, written by a card writer box (the Magic, Amazing or
Ultimate Box) and the box's own software. **Embroidery › Save for a Memory Card…**
(`ui.saveForMemoryCard`; `embroidery.memoryCards`, `embroidery.memoryCard.save {card, path?}`) asks
which card the machine takes and saves the design in the format its machines read, ready for the
box's software to put on the card:

| Card | Machines | Saved as | Holds |
|---|---|---|---|
| Brother 512 KB blank card (Magic, Amazing) | Brother, Baby Lock, Bernina Deco, Simplicity, White, Viking/Husqvarna (Brother-made) | PES | up to 230,000 stitches |
| Ultimate Card | the same | PES | over 450,000 stitches |
| Pfaff card (two-slot box) | Pfaff | PCS | not published |

A design with more stitches than the card holds isn't saved, and the window says how much of the
card it takes, with the warnings Export Machine File gives (a design larger than the hoop, thread
colours the format shows as their nearest). What the cards take is from the public Embird pages on
the Ultimate Box (<https://www.embird.net/sw/embird/tutorial/wubox/wubox.htm>,
<https://embird.net/manual/0575ultimatebox.htm>). StitchFoundry doesn't write to the box itself: how
it talks to a computer isn't published. Bernina Artista cards, which the two-slot box also writes,
take a format StitchFoundry doesn't write yet.

## Lettering

**Embroidery › Add Words…** (in every way of working) asks what to say and how tall, and adds the
words in the middle of the design, sewn as satin. With a line selected (any path: draw one with
the Pen, Curvature or Arc tool, or use a shape's outline), **Along the selected line** makes the
words follow it, centred along it; words longer than the line are made smaller to fit. Without
a line, **Straight**, **Arc Up** (a rainbow), **Arc Down** (a smile, read the right way up) or
**Circle** (round the middle, along the top) sets the words on a curve made for them
(`embroidery.addWords {text, height?, color?, path? | shape?: straight|arcUp|arcDown|circle}`).

In an embroidery design (one with a hoop), type made with the Type tools, including Type on a
Path, sews at once as satin, in the same undo step; type in an ordinary document stays type until
it is given a stitch type.

Type objects are stitched straight from their glyph outlines, so there are no embroidery fonts
to install, and editing the text, font or size restitches it. Small lettering becomes turning
satin columns, with counters and stroke joins cut so each piece turns cleanly. Lettering too thick
for satin becomes tatami.

Each letter's pieces are sewn as one path: the next piece is always one that touches what is
left of the letter, and the thread runs on hidden under the pieces still to come, so a letter is
trimmed only when the needle leaves it. A stroke that has to end where it began runs a line of
stitches up its middle first and comes back over it. Satin is never narrower than 0.8 mm, so the
pointed tips of serifs aren't lost to stitches shorter than the machine sews. (The first real
sew-out, block letters on a T-shirt, had 275 trims; the same design now has about 100, nearly all
of them between letters.)

## Auto-Digitize

Select a placed image and choose **Embroidery › Create › Auto-Digitize Image** (or use the Embroidery panel
when an image is selected). The panel's **Thread colors** slider (2 to 32) shows at once what that
many colours look like: a small preview of the image reduced the way Auto-Digitize reduces it
(the background it leaves out shown as a light checker), the thread each colour becomes (hover a
swatch for its name, code and how much of the image it covers) and how many thread changes that
means, so you can compare counts before digitizing (`embroidery.image.preview {colors,
removeBackground}`). The same count is used by Prepare Image and colour Photo Stitch, both a
button away. The image is traced into flat colour shapes with the given number of
thread colours, the background and specks under 1 mm² are dropped (the shape round a dropped speck closes over it, so no fabric shows through), and each shape gets a stitch
type: satin for narrow parts, tatami at its best angle for the rest. The design is sewn colour by
colour, nearest shape first. It is scaled to fit the hoop if one is set (or to 100 mm if it is
larger than 200 mm). The image stays underneath as a reference. One undo step undoes it all.

Smart design options: shapes no thicker than `details` (1.2 mm) are fine lines and details, sewn
as a run round them instead of a satin too narrow to sew, after everything else so they lie on
top (the shapes under them close up beneath them); a dark line (a stripe, a whisker) is sewn down
its middle as a bean stitch and kept however small its area once it is 1.5 mm long, and a thin
sliver of another colour shorter than 3 mm is left to the shapes round it, which close over it; `outline: design` puts a satin outline
(`outlineWidth`, 1.5 mm) round the whole design and `outline: shapes` one round each filled
shape, in the design's darkest thread (or `outlineColor`); `chart` matches every thread to the
nearest in a thread chart. Choose how much to keep with `removeBackground` (the colour round the
image's edge) and `minArea` (specks).

### Preparing an image

Photos from a phone or camera place like any image (JPEG, PNG, WebP, TIFF and the rest). HEIC
photos (an iPhone's default) open only in builds made with the `heif` feature: elsewhere StitchFoundry
says to save the photo as JPEG (on an iPhone, Settings › Camera › Formats › Most Compatible takes
photos as JPEG).

**Embroidery › Prepare Image** (`embroidery.image.prepare {colors?, smooth?, clean?, replace?}`)
makes a photo or a scanned logo ready to digitize: a median filter smooths it while keeping its
edges (`smooth`, a radius in pixels), its colours are reduced to the main ones (`colors`, 8 by
default; 0 keeps them all), and specks and ragged edges take their neighbours' colour (`clean`,
how many passes). The prepared copy goes over the original and is selected, ready for
Auto-Digitize or Click to Stitch; `replace` swaps the original for it.

**Remove Background** (Embroidery menu, or the button beside Prepare Image;
`embroidery.image.removeBackground {box?, keep?, remove?, replace?}`) cuts the subject of a photo
out from what is behind it: a pet, a person, a bird in front of leaves. It needs no colour to pick:
it learns the colours of the subject and of the background and splits the picture between them
along its edges (the GrabCut method Rother, Kolmogorov and Blake published in 2004, written for
StitchFoundry). Given no box and no marks it also looks for the subject itself (Select Subject: the
parts of the picture that stand out from its edges seed the cut, and the edge is cut again at full
size; adapted from PhotoCraft's, which follows published saliency methods) and keeps what either
finds. The box round the subject is the whole photo less a thin border, or `box` (fractions of the
image); `keep` and `remove` mark spots it got wrong (`[[x, y, radius]]`, fractions of the
image) and run it again. The cut-out copy goes over the photo with the background transparent, so
Auto-Digitize, Prepare Image and Photo Stitch leave it out; `replace` swaps the photo for it. It
works on a copy of the photo 240 pixels across and smooths the edge when it scales back up, so a
photo of any size takes a second or two.

Below the button, **Sensitivity** (0 to 100, `sensitivity`) chooses how readily a part is taken for
background: higher takes more away, lower keeps more of the subject. The picture under it shows the
photo and its cut-out side by side: drag the line across it to compare before and after; it is
made again, small, when you let go of the slider (`embroidery.image.cutoutPreview`).

The **Cut-Out Brush** tool (Embroidery toolbar) mends a cut-out: paint over a part it left out (the
feathers above a parrot's beak, the same green as the leaves behind them) to put it back, or hold
Alt (or set its mode to Remove) and paint over background it kept to take it away. When you let go
the cut-out is made again round the stroke, one undo step per stroke
(`embroidery.image.refineCutout {keep?, remove?, radius?, sensitivity?}`, points in the document);
the cut-out still holds the whole photo, its background only transparent, so nothing is lost.

### Borders round a design

A cut-out photo sewn on its own can look lost on the fabric; a border round it, like the white
band round a die-cut sticker or the edge of a patch, holds it together. **Border round the
subject** (Embroidery panel, above Auto-Digitize Image) adds one when the image is digitized
(`embroidery.autoDigitize {…, border?, borderColor?, borderEdge?}`), and **Add Border**
(Embroidery menu, or the button beside it; `embroidery.addBorder {ids?, width?, color?, smooth?,
edge?, edgeWidth?, edgeColor?}`) puts one round stitched shapes already there: the selection, or
a digitized photo's group.

The border follows the design's outline grown by **Border width** (`width`, 1 to 30 mm, 3 by
default), its corners and notches rounded (`smooth`, 0 to 15 mm, 2 by default) so the edge is a
smooth line rather than every feather's tip. It is a fill in its own colour (`color`, white by
default) with a hole the design's shape, reaching 0.5 mm under the design so no fabric shows
between them; it is sewn first, so the design lies on top of it. **Satin edge** (`edge`: `satin`,
`run` or `none`) sews a satin column round its outside (`edgeWidth` mm wide, 1.5 by default) in
the design's darkest thread unless `edgeColor` names one. It is one undo step, and gives the
border's id and its size.

**Image Colours** (`embroidery.image.colors {colors?, chart?}`) lists an image's main colours,
the most used first, each with how much of the image it covers and the nearest thread in the
design's chart (or the one named).

## Photo stitch

Select a placed photo and choose **Embroidery › Photo Stitch** (`embroidery.photoStitch {mode}`)
to sew it over the image as ordinary objects you can edit:

- **Lines** (`mode: lines`): parallel lines `spacing` mm apart (1.6) at `angle`, each zigzagging
  across itself as wide as the photo is dark there, so dark parts sew dense and light parts
  straight: one line of needle points in one thread (`color`, black).
- **Coral** (`mode: coral`): points scattered as densely as the photo is dark (`density`, points
  a mm² where it is darkest), sewn as one meandering run from each point to the nearest one not
  yet sewn; a gap over 5 mm starts a new run.
- **Colour** (`mode: color`): the photo's main colours (`colors`, 6), matched to the chart's
  threads, on a grid of `cell` mm; each colour's region becomes a short-stitch tatami fill, the
  lightest sewn first, its angle turned colour to colour. The background (the colour covering
  most of the photo's edges) is left unsewn unless `background` is on.

- **Sepia** and **Black and White** (`mode: sepia`, `mode: mono`): as Colour, but by lightness
  alone, in shades of brown (7, `colors`) or grey (11) from light to dark, each matched to the
  chart's nearest thread. Good contrast and a plain or cut-away background work best.
- **Colour Tile** (`mode: tile`): the photo as small tiles (`shape`: square, diamond or hexagon;
  `size` mm across, 4; `gap` between them, 0.3 mm), each in one of the photo's main colours
  (`colors`, 8) matched to the chart; every colour's tiles are one fill, the lightest sewn first.
  Fine detail doesn't show at this scale: strong areas of colour do.

The Colour, Sepia, Black and White and Colour Tile modes leave the background out (`background`
to sew it). `invert` sews the light parts instead (a light thread on a dark fabric) and `contrast`
sharpens (above 1) or softens the tones. Prepare Image first to cut noise in a busy photo.

## Design from Prompt

Describe a design in words and Claude, Anthropic's AI model, draws artwork for it, which
StitchFoundry then digitizes. Choose **Embroidery › Create › Design from Prompt…** (or the button in the
Embroidery panel), describe the design ("a cartoon fox head, orange and white"), and set its size
(the longest side, in mm: 90% of the hoop's shorter side by default, else 100 mm) and the most
thread colours it may use. **Generate** sends the request, and the dialog shows how long Claude
has been drawing; **Cancel** stops waiting. If something goes wrong (a refused key, a busy
service, a drawing that grew too detailed), the dialog says so and stays open to try again.

Claude is asked for what digitizes well: filled shapes in flat colours, no part narrower than
1.5 mm stitched, no shading or tiny details, drawn back to front. The drawing arrives as one group
named after the description. Its shapes get stitch types as with Convert Artwork to Embroidery
(satin for narrow parts, tatami for the rest), and each one cuts what it covers out of the fills
sewn before it (Remove Underlying Stitching, with a 0.4 mm overlap so small details on top don't
stack thread), so stacked shapes don't pile up. They are sewn in the order they were drawn, with
shapes of one colour brought together where the stacking allows (Auto Sequence, within the group),
so there are fewer colour changes. Its colours become the design chart's closest threads. Images, invisible
shapes and specks under 1 mm² are left out. One undo step undoes it all. As with any automatic
digitizing, check the stitches (Check Design, the stitch player) before you sew. Claude won't copy
someone else's logo, trademark or character: the dialog says so, and an original design described
in your own words draws fine.

**A picture from another AI.** **Picture made by** chooses who makes the picture: **Claude**
(the default) draws the flat shapes itself; **Google Nano Banana**, **OpenAI GPT Image**, **Flux
(fal)**, **Grok Imagine** (xAI, `grok-imagine-image-quality`) or **OpenRouter** (one key for many
image models) paint a picture first, with your own key
for that service (its row shows when it is chosen, with **Get a key**; kept as that service's own
secret file, as the Anthropic key is), and **Model** names another of its models (empty: its
current one). The picture is never stitched as it is: when it comes, Claude redraws it as flat
shapes in the thread colours asked for, as Redraw Image does, and that is placed and digitized, so
even a painterly picture sews with few thread changes. The dialog says "Making the picture…" then
"Drawing the design…". Redrawing needs Claude, so an Anthropic key (or Foundry credits) is
needed too. The picture is billed to your account with that service
(`ui.designFromPrompt {picture?: google|openai|fal|openrouter, model?}`).

**Effort and what it costs.** **Effort** chooses how hard Claude works on a drawing: **Quick**
thinks briefly and costs least, **Normal** is the model's own default and **Thorough** thinks
longest and does two things more. It may search the web (up to two searches) for what the
description names, when it is a real thing (a dog breed, a truck or RV model, a building), so its
shape, proportions and markings are right; real things are drawn without their makers' badges or
logos. And before the drawing is placed it goes back to Claude as it will stitch
(`embroidery.artPreview`, a picture of the stitches drawn in a scratch document), to be checked
against the description once: Claude answers OK, or with a corrected drawing, which is placed
instead (the status line says so). Only the description, the drawing and its stitches go back,
nothing else from your design, and not the search results: the check is a request of its own,
without searching, so they aren't paid for twice. Thorough costs several times what Normal does. The choice is kept for next time (`embroidery.aiEffort {effort?: quick|normal|thorough}`)
and goes to the Messages API as its `effort` (low, medium, high). Before anything is sent the
dialog shows what a drawing should cost: in dollars on your own key, at Anthropic's published
prices for the model ($4 a million tokens sent, $20 a million back, $10 a thousand searches), or,
when you pay with StitchFoundry credits, in credits at StitchFoundry's price (the service's rate,
which includes its markup, so the number is what comes off your balance), with the most it could
cost if the reply ran to its limit; Thorough carries a warning.
The typical figures are estimates until the AI test jobs (`evals/ai/README.md`) measure them. Once
the design is placed, the status line says what it took, from the reply itself: its tokens and
searches, and what you paid for them: dollars on your own key, credits at StitchFoundry's price
when you pay with credits.

**Your own API key.** Design from Prompt uses your Anthropic API key, and each design is billed to
your Anthropic account; get a key in the [Anthropic Console](https://console.anthropic.com/settings/keys).
Paste it into the dialog. With **Remember the key on this computer**, once Anthropic accepts the
key it is kept in a file of its own, `anthropic-api-key`, next to StitchFoundry's settings and
readable by you alone: not in the settings themselves, which the control channel and MCP agents
can read. Turn the option off and Generate to forget a kept key. Without a kept key, the `ANTHROPIC_API_KEY` environment variable is used.

Only your description, the size and the colour count are sent (and, to redraw an image, the
image: see below), to `api.anthropic.com` over HTTPS, and only when you click Generate; nothing
else from your design leaves the computer. Certificates are checked against the system's trusted
ones, and a proxy set in the usual environment variables (`HTTPS_PROXY`…) is used. In the web
app the browser sends the request straight to Anthropic (nothing passes through StitchFoundry's
site), and Remember the key keeps it in this browser's storage for the site. Agents don't need
it: an agent that draws can hand its own SVG to `embroidery.placeArt`, which does the rest.

## Redraw Image with Claude

Select a placed logo or image and choose **Embroidery › Create › Redraw Image with Claude…** (or **Redraw
with Claude…** in the Embroidery panel, next to Auto-Digitize). Claude redraws it as clean
flat-colour art in the same dialog as Design from Prompt, with the same API key: it keeps the
image's composition, shapes, lettering and colours, leaves out its background (unless it is part
of the design, like a badge shape), merges shades into at most the thread colours asked for, and
thickens or drops what is too fine to stitch. Write anything to change in the box ("leave out the
tagline", "make the letters bolder"); it is optional. Only redraw artwork you own or have
permission to use.

The drawing lands on the image: its frame is the image's place, sized to the size asked about the
image's centre (by default the image's own size, brought into the hoop, as Auto-Digitize does). It
is then digitized as Design from Prompt's drawings are: stitch types, underlying fills cut, the
chart's threads and same-coloured shapes together, in one undo step. The image stays underneath to
compare with. Unlike Auto-Digitize, which traces the pixels, Claude draws the shapes anew, so
blurry, small or photographed logos come out clean; check the lettering and the details against
the image before you sew.

The image is sent as a PNG (a JPEG when that would be too large), no larger than 1568 pixels on its
longest side, which is as much as the model looks at.

## Ask About Embroidery

Choose **Embroidery › Ask About Embroidery…** (also in the Help menu, and in simple mode's Help
menu) and type a question about machine embroidery or about StitchFoundry: which stabilizer suits
a T-shirt, why the thread keeps breaking, how to make small letters sew cleaner, where a feature
is. Press Enter (Shift+Enter starts a new line) or **Ask**; Claude's answer appears in the window,
in plain words, naming StitchFoundry's menu items where one does what you need. Ask a follow-up
and the whole conversation goes along (up to 30 questions); **New Conversation** starts over.
Nothing of it is saved.

**Changing the design.** Ask for a change ("make the border satin", "fewer trims please", "make
the letters 15 mm tall") and Claude does it with StitchFoundry's own commands. It looks at the
design by itself (Design Information, Check Design, an object's settings), but every change waits
for you: the window lists what Claude wants to do, as the menus name it, with **Allow** and
**Don't Allow**. Each change is one step that Edit › Undo takes back. Claude can't open, save,
export or send files, change preferences, install anything, or use the app's windows: for those
it tells you where to go.

With **Include my design** on (the default), a short summary of the design open now goes with each
question, so "why does this design have so many trims?" can be answered: its size, stitches,
trims, jumps, colour changes and sewing time, its threads in order, the hoop (and whether the
design fits), fabric, machine and file format, what Check Design finds, and its objects in sewing
order with their stitch types and sizes (the selected ones marked). What Claude looks at with its
commands goes too; the design's file is never sent.

It uses the same Anthropic API key as Design from Prompt, billed to your Anthropic account; the key
row shows while there is none (or after **Change key**). Answers can be wrong: try a test sew
before an important piece. It works in the desktop and the web app, as Design from Prompt does,
and agents can open it with a question (`ui.askAboutEmbroidery {question}`).

## Check My Sew-out

Sewn it and something doesn't look right? Choose **Embroidery › Check My Sew-out…** (also in the
Help menu and simple mode), add photos of the finished piece and press **Check My Sew-out**:

- **Photos**: the front, the back, and close-ups of anything that looks wrong, up to six. A ruler
  or a coin in the photo shows the size. Phone photos are turned upright and made smaller before
  they go; iPhone HEIC photos can't be read: send them as JPEG (Settings › Camera › Formats ›
  Most Compatible).
- **About the sewing**: the fabric (as the design has it, or another), the stabilizer, the
  needle, and what happened while it sewed (the top thread broke, the bobbin ran out or didn't
  cut, the hoop was knocked, the needle broke, the machine stopped), plus anything else in your
  own words. Your machine comes from My Machine.
- **Include the design that was sewn**: with the design open, a picture of how it should look,
  drawn from its own stitches, and the same summary Ask About Embroidery sends go along, so the
  design can be fixed, not only explained.

Claude compares the photos with how the design should look and answers in the Ask About Embroidery
window: what sewed well, then each problem, where it is, its likely cause and the fix, split into
what the design can fix (overlap where shapes meet, compensation, density, underlay, the fabric
setting, stitch direction, trims) and what the machine or the sewer can (tension, needle, bobbin,
threading, stabilizer, hooping). It reads the back too: top thread showing there or nests point
to tension and threading, not the design. It offers to make the design fixes, and makes each one
only after you **Allow** it, as Ask About Embroidery does; ask follow-up questions in the same
window. Try a fix on a scrap of the same fabric before the real piece.

It uses your own Anthropic API key, as Design from Prompt and Ask do, or your StitchFoundry
credits (below): then the first three checks each month are free, and a check after them costs 10
credits, its follow-up questions and fixes included (`ui.checkSewout {photos?}`). With your own key,
the photos and the design go to Anthropic for the check and StitchFoundry keeps nothing. With
credits you can tick **Share this check to help improve StitchFoundry**: the photos, what was found
and what was changed are kept, privately, so the software can learn from real sew-outs; and after
sewing the fix, **This is the sew-out after the fix from my last check** tells it whether the fix
worked. **Delete My Shared Sew-out Checks** in Foundry Account removes them.

### Report to StitchFoundry

When a check turns up something StitchFoundry itself could do better, **Report to StitchFoundry…**
(in the Ask window under a check's answer) sends it to the people who make it. **Have Claude Write
It** asks Claude, in the same conversation, for a short report: a title, what went wrong, whether
StitchFoundry's stitches, settings or advice were part of the cause or it was the machine, thread,
fabric or hooping, and what the software could do differently. Edit it as you like, add a note,
and choose what goes with it: the photos, the conversation with Claude, a summary of the design.
Nothing is sent until you tick **I agree to send this to SiteSentrix…** and press **Send Report**;
it needs a Foundry Account (any account, whichever way the check was paid), and an account
can send five a day (`ui.reportSewout {title?, report?, note?, consent?, draft?, send?}`). A check you report is on us: the credits it cost (the check, its follow-up questions and Claude's draft) come back to your account, once a check.

A report is used only to improve StitchFoundry: it is never shared, sold or published. It is kept
privately and emailed to StitchFoundry's makers, who may reply to your account's email; **Delete My
Shared Sew-out Checks** in Foundry Account deletes your reports too.

## Foundry Account

**Embroidery › Foundry Account…** (also in the Help menu) is the other way to pay for the AI
features: instead of your own Anthropic API key, Foundry credits.

- **Sign in** with your email and password, or **Create Account** (a new account starts with free
  credits). The password is sent once and forgotten; the app keeps only the sign-in, on this
  computer, as it keeps the API key.
- The window shows your **credits**, the free sew-out checks left this month, and the packs to
  **Buy…**: the payment page opens in your browser, on Stripe's secure site (your card never
  reaches StitchFoundry); afterwards press **Refresh**.
- **Pay for the AI features with my Foundry credits**, on once you sign in, sends Design from
  Prompt, Redraw Image, Ask About Embroidery and Check My Sew-out through StitchFoundry: they
  stop asking for an API key and say "Paid with Foundry credits (N left)". Turn it off to use
  your own key again. **Sign Out** ends the sign-in here and at the service.
- Running out says so ("Not enough Foundry credits: buy more in Foundry Account");
  nothing is charged for a request that fails.

Until StitchFoundry's service opens, the window says accounts aren't open yet. The **Server** row
names another service (one you run yourself: `docs/cloud.md`), and so does the
`STITCHFOUNDRY_CLOUD_URL` environment variable (`ui.account`).

## Publish to MakerFoundry

[MakerFoundry](https://makerfoundry.app) is SiteSentrix's place to share and sell embroidery
designs, and it uses the same Foundry Account. **File › Publish to MakerFoundry…** sends the open
design there as a **draft** in your account:

1. Sign in to your Foundry Account (the title bar's **Sign In**). The command needs a design with
   stitches, and a sign-in.
2. Pick the machine format (**PES** unless you choose another: DST, JEF, EXP, VP3, XXX, PEC or
   U01) and, if you checked a sew-out, whether its photos go along (**Include the photos**).
3. Press **Publish to MakerFoundry**. The design is written in that format and sent with your
   sign-in; MakerFoundry checks the sign-in with Foundry Accounts for this one request and doesn't
   keep it. It reads the file as it reads an upload on its site, saves the photos again without
   their location or camera details, and keeps the design as a draft.
4. The draft's page opens in your browser. Add the title, description, tags, licence and price
   there, and publish it: publishing (and agreeing to MakerFoundry's Seller Terms) always happens
   on the site. If the browser isn't signed in to MakerFoundry, it asks for your Foundry Account
   first and then comes back to the draft. A draft you don't want is deleted from **Your designs**
   on the site.

If MakerFoundry refuses the design, the window shows its reason as it wrote it: a file it can't
read, too many designs today (50 a day, 500 kept), or the service busy. An ended sign-in opens
Foundry Account to sign in again. A design over MakerFoundry's limits (4 MB for the machine file,
64 MB with its six photos at most) is refused before anything is sent ("The design is too big for
MakerFoundry").

The window's **Server** row sends to another MakerFoundry (one you run yourself, for testing:
`http://127.0.0.1:8080` works on this computer), and so does the `MAKERFOUNDRY_URL` environment
variable; anything else must be an `https://` address. From the control channel or MCP:
`embroidery.publishToMakerFoundry {format?, photos?, server?, send?}` (`send: true` publishes at
once) → `{open, signedIn, format, photos, server, sending, url, error}`. The web app sends from the
browser, which MakerFoundry allows for stitchfoundry.app.

## Send to StitchShop

**File › Send to StitchShop…**, or **Send to StitchShop** on a job order (Embroidery panel › Job
Orders), sends the open design to the shop that will sew it. It waits in the shop's inbox in
StitchShop or StitchBench until the shop takes it into an order. It goes through StitchHub, by the
Foundry Account you and the shop share, so the shop has to have linked its StitchHub account to
that Foundry Account first (in the shop's program, Settings › Foundry Account).

1. Sign in to your Foundry Account. The command needs a design with stitches and a sign-in.
2. Pick the shop when your account has more than one (**Any of my shops** lets each of them see
   it), and the machine format: **DST** unless you choose another (PES, JEF, EXP, VP3, XXX, PEC or
   U01). **Include a picture of the stitches** sends a PNG of the design as it sews, which the shop
   uses as the order's mock-up when the order has none.
3. Press **Send to StitchShop**. The design is written in that format and sent with its details:
   its name, size, stitches (needle stitches only, counted as StitchShop counts a DST), colours and
   the threads in sewing order, colour changes, trims, thread and bobbin used, sewing time, hoop
   and fabric. From a job order, its number, customer, garment, placement, due date, quantity and
   notes go too, and the file is named after the job (`J-0001 Rose crest.dst`).
4. The window says which inbox it is in. **Take It Back** removes it while no shop has taken it.

The file and the picture go straight to StitchHub's file store at addresses the hub signs for their
exact size and contents; the hub checks your sign-in with Foundry Accounts and doesn't keep it.
A file waits 30 days. The hub's limits are 10 MB for the file, 2 MB for the picture (it is made
smaller until it fits), and 25 files or 100 MB waiting for an account. Over 10 MB, the design is
refused before anything is sent. The hub's messages are shown as it wrote them: no StitchHub
account linked to this Foundry Account yet, the inbox full, too many tries. An ended sign-in
opens Foundry Account.

The window's **Server** row sends to another hub (a self-hosted one, or one on this computer for
testing: `http://127.0.0.1:8787`), and so does the `STITCHSHOP_HUB_URL` environment variable;
anything else must be an `https://` address. From the control channel or MCP:
`embroidery.sendToStitchShop {format?, job?, shop?, preview?, hub?, send?, retract?}` → `{open,
signedIn, format, job, preview, hub, shops, shop, sending, sent, retracted, error}`. The web app
sends from the browser, which needs the hub's file store to allow stitchfoundry.app (a CORS rule
for the hub's owner).

### Open in StitchFoundry

A design page on MakerFoundry can open its embroidery file straight in StitchFoundry's web app:
the link is the web app's address with `?open=` and the file's address on MakerFoundry, for example
`https://stitchwright.github.io/StitchFoundry/?open=https%3A%2F%2Fmakerfoundry.app%2Fapi%2Fv1%2Fdesigns%2F42%2Ffile%2Frose.pes`.
The design opens as a new document, as if the file had been dropped on the window; the status bar
says so while it loads, and says why when it can't. When MakerFoundry refuses the file, its own
sentence is shown as it is ("That design isn't on MakerFoundry.", "This design is for sale: buy it
on MakerFoundry, then download it there."); anything else, or an answer without MakerFoundry's
sentence, shows "Couldn't open the design from MakerFoundry: …". The `open=` part leaves the page's
address as soon as it's read, so reloading the page or sharing its address doesn't open the design
again (the other parameters, such as `view`, stay).
Only files on makerfoundry.app (or www.makerfoundry.app) over `https://` are fetched, without
cookies, up to 32 MB; any other address is refused before anything is fetched. The file's name
comes from the end of its address (or the `Content-Disposition` header when the site shares it),
and its extension says how it's read, so the address should end in the file's name (`…/rose.pes`).
MakerFoundry has to allow the web app's site (`https://stitchwright.github.io`) to fetch the file.

## Design settings: hoop, fabric, threads

The Embroidery panel's **Design** section shows the stitch count, size, colours, trims, sewing
time and thread used, and whether the design fits the hoop. Its settings:

- **Hoop**: the sewing field, drawn on the canvas centred on the design (machine files are
  centred too); dashed red when the design doesn't fit. Choosing a hoop makes the page its size,
  about the page's middle, so the design stays put (`embroidery.settings {hoop, page?: false}`
  keeps the page; templates keep their own page with a margin round the hoop).
- **Fabric**: presets (recipes) that set density, fill stitch length, pull and push compensation
  and underlay (the satin underlay type: automatic, centre run, edge run, zigzag or an edge run
  with a zigzag over it, the support knits and pile need; and whether fills get an edge run, which
  leather and vinyl, where every hole shows, go without), whether the design is sewn on clothing
  (worn and washed, so Check Design flags long stitches from 7 mm, not 12.1 mm: T-shirts, polos,
  fleece and silk are; towels and leather aren't), and say what to hoop under it: a **Stabiliser** (tear-away for stable wovens, caps and towels; cut-away or a no-show
  mesh for knits, which stretch; wash-away for sheer work) and a water-soluble topping for pile
  fabrics, printed on worksheets too. **Fabric settings**
  below it shows the chosen fabric's values; **Save as New Fabric** makes a copy you can change
  (the design keeps its own fabrics, and changes apply to the design at once), and **Delete
  Fabric** removes one.
- **Overlap where shapes touch** (in Machine settings; 0.5 mm): a fill sewn before a shape it
  touches reaches that far in under it, so the fabric doesn't show between them when the stitches
  pull it in, as it did above a truck's bumper on the first sew-out, on a T-shirt. With the pull
  compensation the stitches add, that makes about 1 mm, as digitizers leave it. Only where the
  later shape covers it (it stays hidden), not into a shape lying over it already, nor between a
  letter's own strokes; 0 sews shapes as drawn (`embroidery.settings {overlapTouching}`).
- **Machine**: the machine the design is for. The standard profiles are named by the file format
  their machines read (DST, EXP and U01 for commercial machines; PES, JEF, VP3 and XXX for home
  machines); profiles of your own (say, *Diamond deLuxe*, writing VP3) follow them. A machine sets
  the longest stitch and frame move and the speed and trim time used for time estimates, and
  **Export** writes its format unless you pick another. **Machine settings** below it change those
  values for this design, along with the shortest stitch, how long a jump must be to be trimmed,
  the colour change time and the design's tie-ins and tie-offs. **Save to Machine** keeps them in
  the machine's profile, so the next design that picks it starts with them (all of them, ties
  too); **Reset Machine** puts a changed standard profile back to how it ships. **Save as New
  Machine** makes a profile of your own from them, in the design's format; for your own profiles
  the section also has their **Name**, their file **Format** and **Delete Machine**. Profiles are
  kept with your preferences, not in the design: a design keeps the name and format of its
  machine, so it still exports in that format on a computer without the profile.
- **Start at** and **End at**: where sewing starts (the first object entered at its point
  closest to the design's centre, a corner or the middle of a side) and where the frame goes when
  the design is done, after a trim: the centre, say, to take the hoop off with the needle clear,
  or a corner to line up the next hooping (`embroidery.settings {start, end}`).
- **Basting first**: a basting run, long running stitches (6 mm) sewn before everything else in
  the first thread and without ties, holding the fabric to the stabiliser, to pick out after.
  *Basting round* the design (*Basting margin* out from it, 5 mm) or the hoop (that far in from
  its edge, the hoop centred on the design); it is written into the machine file
  (`embroidery.settings {basting: true | {hoop?, margin?, length?} | false}`).
- **Estimate cost**: what sewing the design costs, shown under its figures (*Cost a piece*) and on
  worksheets. *Cost settings*: a price per 1000 stitches, the machine's time an hour (shared by
  the pieces its *Machine heads* sew at once), thread per 1000 m (top and bobbin) and a set-up
  charge an order, in a currency; `embroidery.cost {quantity}` prices an order, the set-up spread
  over its pieces (`embroidery.settings {cost: {perThousand?, perHour?, threadPerKm?, setup?,
  heads?, currency?}}`).
- **Colorway**: other thread schemes for the same design, a shirt in navy and one in white.
  *New Colorway* copies the threads in use; each of the design's colours (in the order it first
  uses them) then gets a thread of the colorway's own, from the colour buttons below. The
  colorway picked is what the stitches show, machine files carry and worksheets print (named
  there); *The design's own* goes back to the threads set on the objects, and the art keeps its
  colours throughout (`embroidery.colorway.list|add|activate|setThread|rename|remove`).
- **Thread chart**: the chart the panel's thread list picks from. Threads keep their name and
  code; PES and JEF files use the nearest colour of the machine's own palette. **Match Threads to
  Chart** (also in the Embroidery menu, for the selection) gives every stitched object the chart's
  closest thread and recolours its art: use it on a design opened from a machine file, whose
  colours name no threads.
- **Show stitches** (Hidden / Lines / TrueView), **Artwork** (Normal / Dimmed / Hidden),
  connectors, needle points, the selection's **start and stop** beads and its **inclination
  lines** (shown with any tool, not only the Inclination tool).
- **Repeats across** and **Repeats down** (up to 10 each) show the whole design repeated in a
  grid on the canvas, **Gap between repeats** apart, to preview a repeated layout (a border, an
  all-over pattern); only the design itself is sewn (`ui.embroideryRepeats`).

Thread colour follows the object's paint colour unless set in the panel. **Select › Same ›
Thread Color** and **Stitch Type** select every stitched object like the selected one.

**Check Design** (Embroidery menu, or the button in the panel's Design section) looks for spots
where thread piles up (very dense), stitches long enough to catch (from 7 mm on clothing, which
is worn and washed, or 12.1 mm on bags, towels and other items: **Sewn on clothing** in Check
settings), objects with many stitches under 0.5 mm (the needle sews in one spot and can break the
thread), satin narrower than 1 mm (it sinks into the fabric: sew it as a run), tatami fills with
stitches under 4 mm (short fill stitches ripple after washing), tiny objects, lettering under
about 5 mm, objects that make no stitches and a design larger than its hoop. Click a finding to
select its objects and bring the spot into view. **Check settings** under the button sets each
limit for this design; 0 turns a rule off.

Density is measured as layers of thread over 2 × 2 mm squares: a tatami fill is about 1.2 layers
with its underlay, a satin column 2.5 to 3.5. Spots over 4.5 layers are flagged: four fills stacked,
satin columns crossing, or a column piling thread up inside a corner. Lettering, borders over fills
and two overlapping fills stay under it.

**Density Map** (Embroidery menu, or *Density map* with the panel's display options) colours the
whole design by the same measure, square millimetre by square millimetre: blue where the thread
lies one layer deep, green and yellow deeper, red as deep as Check Design calls dense (its limit
in Check settings). `embroidery.densityMap {cell?}` returns the layers cell by cell.

## Properties for new objects and styles

With nothing selected, the Embroidery panel shows **Properties for New Objects**: pick a stitch
type and set its density, underlay, pull compensation and the rest. Objects you then give that
stitch type (with the Stitch Type list or Convert Artwork to Embroidery) take them. With
an object selected, **Make Current** (also Embroidery › Stitches › Make Properties Current) makes its
properties the ones new objects of its type take. **Reset to Defaults** goes back to the built-in
values. These are saved with the design, so a template carries them to new designs. **Save as
Default** keeps them for every design (with the app's preferences): a pull compensation of 0.7,
say, set once, is what every new satin in every new design takes, until the design sets its own
or **Clear Default** forgets it (`embroidery.current.saveAsDefault {type?}`,
`embroidery.current.clearDefault {type?}`).

**Embroidery styles** are named sets of stitch properties. In the **Embroidery Styles** panel
(Window › Embroidery Styles), the new-style button saves the selected object's properties as a
style, and clicking a style applies it to the selection. Objects keep their thread, shape,
inclination lines and start and stop points, and follow the style: **Redefine Embroidery Style**
(panel menu) gives the style the selected object's properties and updates every object that
follows it. A "+" marks an object changed since it took its style. The Embroidery panel's
**Style** list shows and changes the selection's style; **Break Link to Embroidery Style**
leaves objects as they are but no longer following it.

## Editing single stitches

The **Stitch Edit tool** (Embroidery toolbar, or **Edit Stitches** in the panel) shows the needle
points of the selected stitched object. Click a point to pick it (Shift adds or removes), drag
across empty canvas to pick the points inside, drag picked points to move them, Alt-click a stitch
to add a needle point there, and press Delete to remove the picked ones.

The first edit **freezes** the object: its stitches are kept as they are, and changing its shape no
longer regenerates them (moving, scaling or rotating the object still carries them along). The panel
then says the stitches were edited by hand; **Regenerate Stitches** (panel or Embroidery menu) drops
the edits. **Freeze Stitches** freezes without editing.

**Split Here** (panel, with one needle point picked) splits an object's stitches in two at that
point: the object keeps the stitches before it, and a copy sewn right after it takes the rest, both
frozen. Use it to sew part of an object in another colour, or to put something between its parts.

**Machine functions:** a trim or a stop can be put after any stitch, and any stitch made a jump
(the frame moves there without sewing), with `embroidery.function.insert`; `embroidery.function.clear`
takes them out again. **Stop after** in the panel stops the machine after an object without
freezing it (to place appliqué fabric, or change thread on a single-needle machine). **Own colour
block** gives an object a colour block of its own: a colour change before it even when the object
before is in the same colour (split colour blocks: say, a dark red sewn as two blocks so a
multi-needle machine can take it from two needles, or so it stops there; `ownBlock` in
`embroidery.setParams`).

The **Stitch List** panel (Window or Embroidery menu) lists every record of the design in sewing
order: number, function (stitch, jump, trim, colour change, stop), position, length and the object
that sews it. Click a row to play the design up to it and select its object; while the stitch player
runs, the list follows it.

## Stitch Sequence and the stitch player

The **Stitch Sequence** panel lists the stitched objects in sewing order, grouped by thread, with
stitch counts. Drag a row onto another to sew it just before or after that one, or use the context
menu (sew earlier, later, first, last); Shift- or Ctrl-click selects several. Each row's eye hides
or shows its object and its lock locks it; hidden objects are listed at the end, not sewn, so they
can be shown again. **Sort by Thread Color** regroups the design to sew each thread colour in one
go. The **player** redraws the design stitch by stitch, as the machine will sew it; its **travel**
buttons step back and forth by stitch, object, colour, trim or machine function, selecting the
object reached.

**Fewer trims.** Between two objects of one thread, each object ends at its point nearest the
next one; when they touch or overlap, the thread runs to the next one in small running stitches
hidden under it (it sews over them) instead of a trim, up to 40 mm. Objects apart are trimmed
between as before (`embroidery.settings {hideTravel: false}` trims between all of them).

**Embroidery › Colors and Order › Auto Sequence** reorders the stitched objects of each layer or group for the
fewest colour changes and then the shortest way from one object to the next, keeping objects
that overlap in their stacking order (the one on top is still sewn last).

**Embroidery › Shape › Branch** sews two or more selected objects as one path, without trims: they
become a *branch* group. The objects are sewn closest-first (overlapping ones in stacking
order), each ends where the objects still to sew cover it, and the thread runs to the next one
as small running stitches routed inside those later objects, so it is sewn over and hidden.
Where nothing covers the way, the thread is trimmed as usual. **Break Apart** turns a branch
back into separate objects; on a stitched path or compound path made of separate parts it
makes one object per part, each with the same stitching (a hole stays with its part).

**Embroidery › Add to Design › Add Buttonholes** places a straight, rounded or keyhole buttonhole (15 mm slit,
4 mm wide) in the middle of the artboard: satin columns bordering a slit that is cut open after
sewing (0.6 mm is left open for the cut). Each buttonhole is a group of ordinary stitched paths,
sewn in order: a run round it to hold the fabric, one side, the far end (a bartack, a satin end
rounded round the slit, or a keyhole eye), the other side and a bartack at the near end. Move,
turn and resize it like any art. `embroidery.addButtonholes` places a string of them too
(`count`, evenly spaced from `at` to `to`), with the slit `length`, the `width` and an `angle`. The Embroidery panel's **Buttonholes** section adds one or a row
of them in the middle of the artboard: style, length, width, how many, how far apart (middle to
middle) and an angle, the row running square to the slits.

**Embroidery › Shape › Automatic Knife** cuts the selected stitched shapes round their holes and where
they branch (at sharp inside corners, like the arms of a T) into simpler pieces, each its own
object with the same stitching and its own automatic direction, a single angle or turning;
the pieces overlap by 0.3 mm along the cuts (`overlap`) and become a branch group, sewn as one
path. Edit each piece's direction like any object's; Break Apart separates the group.

**Embroidery › Shape › Redwork** sews the selected line art (paths, compound paths and type) the way
hand redwork is sewn: as one line that starts and ends at the same point, so there is one tie-in
and one tie-off. Every line is sewn once in the run style (*Single*, *Triple*, *Backstitch* or
*Stem stitch*); to get from one line to the next the needle retraces lines already sewn as a
single run on top, choosing the shortest ways, so no travel shows off the lines. Lines that
cross, touch, or end within 0.3 mm of another line meet there; parts that don't touch are
joined by travel or a trim. The art becomes a *redwork* group in the first thread given to its lines (else the first
line's stroke colour), and
its own stitching (a fill, say) is left out while it is in the group; the panel sets the run
style and stitch length. **Break Apart** gives the art back as it was. A single path, compound
path or type can be redwork on its own too (stitch type *Redwork*): its subpaths are sewn as
one line.

## Machine files

Designs save as `.stitchfoundry` files, which keep everything. Saved as SVG instead, a design
with stitched objects carries its native document inside the SVG (`preserveEditing`, on by
default for them), so it reopens with every stitch type and setting and File › Save keeps
writing the same SVG; only with `preserveEditing: false` is the SVG plain art. A machine file
opened (DST, PES…) asks for a name on the first Save, so the machine file isn't overwritten with
a design; export machine files with **Export Machine File**.

| Format | Extension | Colours | Open | Export |
|---|---|---|---|---|
| Tajima | .dst | no (colour changes only) | yes | yes |
| Brother | .pes, .pec | PEC palette (nearest) | yes | yes |
| Janome | .jef | JEF palette (nearest) | yes | yes |
| Melco | .exp | no | yes | yes |
| Pfaff | .vp3 | RGB with names | yes | yes |
| Singer | .xxx | RGB | yes | yes |
| Barudan | .u01 | no (needle numbers) | yes | yes |
| Pfaff (older machines; experimental) | .pcs | RGB, 16 at most | yes | yes |
| Husqvarna Viking (experimental) | .hus | HUS palette (nearest) | yes | yes |
| Janome, Elna, Kenmore (older machines; experimental) | .sew | JEF palette (nearest) | yes | yes |
| Pfaff (older machines) | .vip | RGB | yes | no |
| Happy | .tap | no (DST records) | yes | yes |

- **Before a file goes to the machine** (File › Export, Save for My Machine, Send), it comes
  with what the machine may refuse or show differently, in the status bar (`warnings` in
  `embroidery.export` and `embroidery.send`):
  - the design is larger than its hoop's sewing field (where it is placed in it), or than the
    largest hoop the format names (JEF: 200 × 200 mm);
  - the format stores no thread colours (DST, EXP, TAP, DSB, U01): the machine shows its own, so
    sew by the colour list;
  - a palette format (Brother PES and PEC, Janome JEF and SEW, Husqvarna HUS) can only store its
    machines' own colours: threads further than 12 (CIEDE2000) from all of them are named with
    the one the machine will show;
  - stops written as colour changes, where the format has no stop of its own.

  Limits a format can't hold refuse the file instead (PCS: 16 colours; DST: 999 colour changes).
  Thread colours go to a palette as the one that looks nearest by CIEDE2000 (Sharma, Wu and
  Dalal, 2005), which picks better than plain Lab distance among saturated blues, purples and
  dark reds. How each writer is checked: [`format-verification.md`](format-verification.md).
- Opening or placing a machine file turns each thread into a path of **Manual** stitches (one
  subpath per run between trims), which you can edit, move, scale and recolour.
- **Embroidery › Convert Stitches to Objects** (`embroidery.stitchesToObjects {ids?}`) then
  recognises those stitches from the way the needle moves: where it swings from side to side
  stitch after stitch it is a satin column (its two sides become the column's rails, with stitch
  angles where it turned), where it sews rows of several stitches back and forth side by side it
  is a tatami fill (the rows' outline, angle, spacing and stitch length), and the rest are runs.
  They become ordinary objects in the same thread, in the same order, in the manual stitches'
  place (one undo step), so a design whose source is lost can be resized and sewn again with its
  stitches made afresh. Underlay is recognised as runs or fills of its own; delete those you
  don't want, since the new objects make their own.
- The Embroidery panel's **Design** figures show the **design grade**, how editable the design
  is by where it came from (the grades BERNINA's software shows,
  <https://softwarehelp.bernina.com/en/MainHelp/PART_Basics/assets/Design_files.htm>): **A** for a
  design made here, every object keeping its stitch settings; **D** for a machine file opened
  as stitches only; **C** once its stitches are converted to objects. A design keeps its grade
  when saved and opened again; placing a machine file into a design made here leaves it grade A.
  BERNINA's grade B (a design read from another program's outline file) has no counterpart, as
  no such files are read. `embroidery.info` gives it as `grade` and `openedFrom`.
- HUS is **experimental**. Reading follows the MIT-licensed pyembroidery project's reader,
  which reads real Husqvarna files, decompression included. Writing has no public reference: the
  three streams are stored in blocks that code every byte as itself, which the format's own
  decoding reads, but no machine has sewn one yet. Colours are the nearest of the 29 in the HUS
  palette.
- SEW, the format of older Janome, Elna and Kenmore machines, is **experimental**. Its colour
  table (JEF's palette) and JEF's stitch records follow pyembroidery's and libembroidery's
  readers; the preview picture Janome's own files keep before the stitches is written blank, and
  no machine has sewn one of ours yet. A SEW file has no signature: it is recognised by its
  extension, or by a colour table that fits the Janome palette.
- Pfaff **VIP** (older Pfaff machines, between PCS and VP3) is **read only**: HUS's compressed
  streams with an RGB colour table, unscrambled with libembroidery's key. Nobody has published a
  writer machines accept, and machines that read VIP read HUS or VP3 too: export one of those.
- PCS is **experimental**: its layout comes from public descriptions of the format, and no
  machine has sewn one yet. It holds 16 colours at most, positions in 1/6 mm, and no trim (a
  trim is written as a move to where the needle is). TAP is DST's records without the header.
- Barudan **DSB** is a DST header over U01's records: read and written. ZSK **DSZ** (a DST header
  over records of its own, with trims, stops and needle changes) is **read only**: it opens and
  converts to the other formats, but isn't written, as no public description covers writing it.
  Neither stores colours. Both layouts come from the format notes of the MIT-licensed
  pyembroidery project; no machine has sewn a DSB we wrote yet.
- Long stitches and jumps are split to the format's limit. Trims are written natively where the
  format has them; DST uses the common three-jump convention.
- VP3 writes each move as one long record that ends where the next stitch starts. Programs
  differ on whether that record is a jump or a stitch; read either way, the needle goes down only
  where it would anyway.
- The format code (`crates/sf-formats`) round-trips every format and survives random and
  truncated input. Files it writes were read back by an independent open-source embroidery
  library with the same stitches, trims, colour changes, extents and colours (that library counts
  each VP3 move as a stitch, reads JEF without trims and U01 colour changes as needle numbers),
  and files that library wrote open here with the right size, colours and structure. **They have not been sewn on real machines yet**, so
  test-sew before production runs.

## Ink/Stitch designs

An SVG made with Ink/Stitch, the embroidery extension for Inkscape, opens stitched the way its
Ink/Stitch settings say (they are kept with the art and saved with it): dashed strokes run, with
their stitch length, bean stitch and repeats; satin columns sew between their two rails, their
rungs becoming inclination lines; other strokes become satin lines as wide as the stroke; and
filled shapes fill, with their fill method (contour, meander and circular fills as contour,
meander and spiral fills, the others tatami), angle, row spacing, stitch length, stagger and
underlay. Art without settings of its own is sewn with the defaults, as Ink/Stitch would. A fill
with a stroke sews the fill only. Commands (trims, stops) and the newer stitch types
(ripple, tartan, gradient fills…) come in as their nearest StitchFoundry stitching. The mapping
was written from Ink/Stitch's public documentation, not its code.

## Compare Designs

Embroidery › Compare Designs… puts two designs side by side: each one a machine file (any
format StitchFoundry reads) or the design open now. It shows a picture of each and the same
figures for both: stitches, trims, jumps, colour changes, stops, width and height, top and bobbin
thread, sewing time, average and longest stitch, and how many stitches are under 1 mm (they can
break threads and needles) or over 7 mm (they can snag). Each row says how much the second
design differs from the first, and differences of 10% or more are highlighted. Use it to check a
design against another made from the same art, here or in another program, or two versions of a
design, before sewing both (`embroidery.compare {a, b}`; each side `"design"`, `{path}` or
`{dataBase64, name}`).

**One over the Other** draws both on one picture instead: at the same scale, centred on each
other, the first design in blue and the second in red, so a part one of them leaves out, a fill
that goes the other way or a design that came out larger shows at a glance
(`ui.compareDesigns {overlay: true}`).

## Embroidery fonts

Lettering can use embroidery fonts in Ink/Stitch's font format: a folder with `font.json` and
its glyphs' art (`ltr.svg`, or SVGs in an `ltr` folder), each glyph a layer named
`GlyphLayer-<glyph>` digitized with Ink/Stitch settings. Official builds come with five fonts
of our own built in, digitized from openly licensed typefaces (FONT-01): **Foundry Block**,
**Foundry Varsity**, **Foundry Script**, **Foundry Slab** and **Foundry Sans**, listed without
loading anything (`builtIn` in `embroidery.fonts`, with their licence's text, the SIL Open Font
License). They are generated and not yet sew-tested: sew a test at the size you use first. More
fonts download from **Get Embroidery Fonts…** (the Font Library): the 131 Ink/Stitch
fonts in the StitchFoundry fonts collection and Ink/Stitch's own library, each under its own
licence. Or load font folders you have (`embroidery.font.load {path}`), and
set text with them (`embroidery.lettering {text, font, size, at}`): each glyph's art sewn the way
its settings say, placed with the font's advances and kerning, lines its leading apart, ligatures
first, in the font's letter case, characters it lacks shown as its default glyph. `size` is the
capitals' height in mm; outside the sizes the font says it sews well at, the lettering comes with
a warning. Font folders loaded are remembered: later sessions load them again the first time
lettering needs a font (*Forget Font* in the panel, or `embroidery.font.forget {name}`, unloads one
and forgets its folder; a folder that has gone is skipped with a warning). The lettering itself is stitched art, a
group per glyph, saved with the design. The lettering's group remembers what it says, in what font,
how big and where: `embroidery.lettering.edit {text?, font?, size?, leading?}` sets it again where
it is now, moved, turned and scaled as it is (scaling it changes its size), as long as its font is
loaded; the glyphs' own stitching can be changed like any art's, until it is set again.
**Embroidery › Shape › Break Apart** turns it into its glyphs, no longer lettering. In the Embroidery panel, **Embroidery Lettering**
does the same: *Load Embroidery Font…* picks a font folder, then choose the font, type the text,
set its size (the sizes the font sews well at are shown, marked ▲ as a warning while the size typed is outside them) and *Place Lettering* puts it in the
middle of the artboard. With a lettering selected, the section shows what it says, in what font and
how big: change any of them and *Update Lettering* sets it again where it is. *Baseline* bends the
lettering round a circle (*Radius*): **Arch** over its top, **Valley** along its bottom, glyphs
upright to it (`arc`: the radius in mm, negative along the bottom). **Vertical** sets the letters
one under another, each centred, extra lines side by side (`vertical`). *Place Along Selected
Line* sets the text along any line or shape you have drawn, from its start, each letter upright to
it (`path`: its id, or true for the selected one); the line is kept with the lettering, which
can then be moved or scaled as a whole, and `path: null` sets it straight again. *Spiral* winds it in round
the circle, the radius shrinking that many mm a turn (negative: winding out; `spiral`), for text
that runs on round and round; lettering placed from the panel
is centred on the artboard (`align`: left, centre or right on `at`).

One letter at a time: with a lettering selected, the section's *Letter* rows move a letter along
its line or down from it, scale it or turn it about the middle of its baseline (*Reset Letter* puts
it back); so does `embroidery.lettering.letter {index, dx?, dy?, scale?, angle?}`, or, with no
`index`, the letter selected inside the lettering. The changes are kept when the lettering is set
again (a letter is known by its place in the text) and move, scale and turn with it; a letter's
stitching can be changed like any art's until it is set again.

How lettering is sewn (*Sew order*, *Join* and *Trims* in the section; `order`, `lines`, `join`
and `trim` on `embroidery.lettering` and `.edit`):

- **Sew order**: the letters left to right, right to left, or **centre out**: the middle letter
  first, then the letters back to the first, then on from the middle to the last, so a cap's
  front panel is held where it matters most. `lines: up` sews the lines from the last up.
- **Join**: *Closest* starts each letter where it is closest to where the last stopped; *Along
  the baseline* starts each letter at its baseline's start and stops it at the end (start and stop
  points on its first and last pieces), so the thread runs on along the baseline, hidden in a
  towel's pile.
- **Trims**: where the jumps need them (the machine's settings), or after each letter, each word
  or each line.

These follow the public descriptions of lettering sequencing and joins, for example
https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Lettering/lettering_advanced/lettering_advanced-15.htm
and https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Lettering/lettering_advanced/lettering_advanced-16.htm.

**Get Embroidery Fonts…** (Embroidery menu, and the Embroidery Lettering section; desktop app
only, `ui.fontLibrary`) lists the fonts of an embroidery font library and downloads one with a
click: Ink/Stitch's font library (github.com/inkstitch/embroidery-fonts, about 140 fonts, each
under its own licence, kept in its folder), and StitchFoundry's own once it is published
(github.com/Stitchwright/stitchfoundry-fonts; until then the window says it isn't). Search the
list by name, open a font's *Preview* in the browser, and *Download*: its files go into the
**Embroidery Fonts** folder next to the settings (a font downloaded again replaces its copy) and
it is loaded and chosen for lettering, remembered like any font folder. **Download All** fetches every font not downloaded yet
(the whole Ink/Stitch library is about 110 MB), one after another, each loaded as it arrives, with
a progress bar and *Stop*. Names and paths the
library sends are checked and sizes capped before anything is written. The web demo has no
server to keep fonts on, so it doesn't offer downloads.

## Warped lettering

Lettering takes the envelopes and warps any art does (**Object › Envelope Distort › Make with
Warp**: arc, arch, bulge, flag, wave, fish, rise, fisheye, inflate, squeeze, twist): its stitches
bend with the letters, and it stays lettering, so it can be set again with new text inside the
envelope (`embroidery.lettering.edit`). Moderate bends sew best; strong ones stretch satin
columns wide or crowd them.

## Lettering stitches and small text

*Letters sewn* (`stitch`) sews lettering as the font digitizes it, all **satin** (its filled
parts sewn as satin across them) or all **fill** (each letter one tatami fill, the shape of what
it sews: for large, wide letters). *Small text* (`small: auto|on|off`; auto: capitals under
6 mm) sews small lettering cleanly: satin over a centre run only, half the pull compensation and
short stitches round curves; fills with a light edge-run underlay and shorter stitches; runs in
short stitches. Both stay with the lettering when it is set again.

## Appliqué lettering

*Appliqué letters* in the lettering section (`applique: "#rrggbb"` on `embroidery.lettering` and
`embroidery.lettering.edit`) makes each letter a patch of fabric that colour: one shape, the
outline of what the letter sews (its fills and satin columns together), sewn as appliqué: a
placement line and a stop to lay the fabric, a tack-down and a stop to trim it, then the cover.
The appliqué settings can be changed per letter like any appliqué; the lettering stays lettering,
and turning it off again brings back the font's own stitching. Bold, wide fonts suit it best.

## 3D puff

**Embroidery › 3D Puff** (`embroidery.puff {ids?, on?, cap?}`) sews the selected objects' satin
over foam: dense (0.16 mm), with no underlay, auto split or short stitches (they would flatten
or cut the foam inside the column), the sides pushed out 0.35 mm (the foam pushes the thread out
rather than pulling it in), and a cap across each end of a column, its stitches reaching 0.5 mm
(`cap`) past the end so the foam is cut and covered there too. Chosen again on satin that is all
puff already, it turns it back to plain satin. Lettering takes it too (`puff: true` on
`embroidery.lettering` and `.edit`, *3D puff* in the lettering section): its glyphs' satin is puff,
and stays so when the lettering is set again. A filled shape made puff is sewn as satin across
it over the foam (made plain again, it fills again); other pieces are left as they are.

**Embroidery › Sew Puff Last** (`embroidery.puff.last`) orders a design the way puff is sewn:
the flat parts first, then the puff. In each layer (and plain group), the objects that are all
puff move after the others, in the order they were in, and the last piece sewn before the first
of them gets a stop, so the machine pauses for the foam to be laid over what is already sewn.

To sew it: give the puff its own thread colour so the machine stops before it, lay the foam
(2–4 mm, in the thread's colour) over the area, sew, then tear the foam away and heat it to shrink
the fibres left. Columns 3 to 7 mm wide puff best: narrower ones don't hold the foam, wider ones
let it bulge out. These follow public descriptions of the practice:
https://wilcom.com/resources/blog/qa-from-3d-puff-embroidery-webinar and
https://hatchembroidery.com/resources/blog/how-to-digitize-for-3d-puffy-foam-machine-embroidery.

## Making embroidery fonts

**Embroidery › Embroidery Fonts › New Embroidery Font** starts the font creator: a new document
with one artboard per glyph (A–Z, a–z, 0–9 and `.,!?'-&`; `embroidery.fontDoc.new {name, chars,
size, descent}` takes others, ligatures such as "fi" too), each named after its glyph and as wide
as its advance, with guides at the baseline and the capitals' height (25 mm unless you say). Digitize
each glyph on its artboard with the usual tools; resize an artboard to change how far the next glyph
starts. `embroidery.fontDoc.setProps` sets the font's name, line spacing, space width, letter case,
the sizes it sews well at, description and licence; `embroidery.fontDoc.kern {pair, value}` sets a
pair's kerning; `embroidery.fontDoc.addGlyphs` adds glyphs; `embroidery.fontDoc.info` lists the
glyphs and which still need art. **Save Embroidery Font…** writes the font folder (font.json and
glyphs.stitchfoundry, the glyphs in StitchFoundry's own format with their stitching) and loads it
for lettering at once; the folder is remembered like any font folder.

A font can also start from type: `embroidery.fontDoc.fromType {family, style, chars, size}` makes
a font document from an installed TrueType or OpenType family, each glyph its outline at the
family's own advance, digitized as Convert Artwork would (satin where the strokes are thin enough,
fill where they're wide), ready to refine and save. Check the type's licence allows it.

Designs on keys (a keyboard collection): select a design in a font document and *Assign* it to a
character in the Font Creator section (`embroidery.fontDoc.assign {glyph}`): it moves onto that
glyph's artboard (one is made if the font hasn't the glyph), its bottom on the baseline, the
artboard widened to hold it. Save the font and type with the designs like any lettering.

Any loaded font, Ink/Stitch's or one made here, opens as a font document to refine
(`embroidery.font.edit {name}`): its glyphs on their artboards at their advances, its properties
and kerning; saving it replaces the loaded font of that name (rename it with
`embroidery.fontDoc.setProps` to keep both). `embroidery.fontDoc.merge {font, chars}` merges
fonts: another font's glyphs this one hasn't yet come in below the others, scaled so their
capitals are as tall.

The Embroidery panel's **Font Creator** section does all this: *New Embroidery Font*, *New Font
from Type* (pick the family) or *Edit Font* (a loaded one) to start; in a font document, the font's
name, line spacing, space, letter case and the sizes it sews well at, its glyphs (those still to
digitize dim; click one to go to its artboard), glyphs to add, kerning pairs, merging another
font's glyphs, *Try in a New Document* and *Save Embroidery Font…*.

## Monograms

The Embroidery panel's **Monogram** section sets one to three initials in the lettering font:
**Classic** makes the middle one of three larger, each letter centred on the monogram's middle; **In
a line** sets them one size on one baseline. A border goes round them: a satin circle, diamond or
square, or a circle of scallops (`embroidery.monogram {letters, font, size, style, border, at}`).
Each letter is lettering of its own, set again like any (`embroidery.lettering.edit`), and the
whole monogram is one group.

More monogram choices: the **stacked** style sets the letters one above the other; borders
also come as an **oval**, **octagon**, **heart** or **shield**, and in up to four **rings** 3 mm
apart (`rings`); an **ornament** either side of the letters, mirrored (hearts, stars, dots as
satin, swirls as a triple run; `ornament`), is drawn in code and goes inside the border.

**Templates**: *Template* picks a look from the gallery (Classic circle, Double ring, Wedding
heart, Royal shield, Diamond trio, Scalloped, Starry oval, Square block, Stacked crest) or one of
your own; *Save as Template* keeps the look now set (style, border, rings, ornament, size) under a
name, kept with your preferences for any design, and *Delete Template* removes one of yours
(`embroidery.monogram.templates`, `.saveTemplate`, `.deleteTemplate`; `embroidery.monogram
{template}`, anything given with it winning).

## Team names

One design, many names (a team's shirts): set the name in an embroidery font, select it, and in the
Embroidery panel's **Team Names** section type the names, one a line (a line break in a name sets
two lines), or *Import Names…* from a CSV table (its first column; `embroidery.teamNames.read
{csv | path, column, header}` reads any column, commas or semicolons, quoted fields). *Export One
File per Name…* writes a machine file for each into a folder: the design with the lettering set
to that name, named after it (`embroidery.teamNames.export {names, dir, format, prefix}`). The
design itself is left as it is. *Lay Out Together* puts every name in one design, in rows and
columns a gap apart, each still lettering, for a large hoop or patches cut apart after
(`embroidery.teamNames.layout {names, columns, gap}`); *Names Worksheet* lists each name with
its size, stitches and sewing time, and the totals for the order
(`embroidery.teamNames.worksheet {names, page}`).

## Patches

Badges and name patches for a company's workers, a club or a team, laid out so a whole hoop of
them sews at once. In the Embroidery panel's **Patches** section (shown also with no document
open) choose the shape (circle, oval, rectangle, rounded rectangle or shield), its size, the
satin edge's width (0 for none) and colour, a background fill (or none, so the fabric shows) and
the colour of the words. Type words for every patch, or a list of names, one a line: a patch for
each worker, their name on it (a line break in a name sets two lines; long names are made
smaller to fit).

*Make Patches* starts a new design holding a hoopful of them in rows, 5 mm apart
(`embroidery.patches.make`). Every patch's background is sewn first, then every patch's words,
then every edge, so a hoopful has three thread changes however many patches it holds. With no
hoop named, several patches go in My Machine's largest hoop (one goes in the smallest that holds
it). When the names need more than one hoopful, the status line says how many; *Export
Patches…* writes every hoopful as a machine file of its own (`patches-1.vp3`, `patches-2.vp3`…,
in My Machine's format) and `patches-names.txt`, listing the names in each hoopful
(`embroidery.patches.export {…, dir, format, prefix}`). *Fill Hoop With Copies* repeats the open
design across its hoop in rows, as many as fit, then sorts the stitching by thread so each colour
is sewn once for all the copies (`embroidery.patches.fillHoop {gap, count}`, one undo step).
Sew the patches on stiff backing (or felt or twill with a cut-away stabilizer) and cut them out
round the edge.

## Commands

Every feature is an engine command. A few of the main ones (see `document.commands` or the MCP
tool list for all parameters):

| Command | Does |
|---|---|
| `embroidery.patches.make {shape, width, height, border, borderColor, background, text \| names, textColor, count, hoop, gap, hoopful}`, `embroidery.patches.export {…, dir, format, prefix}`, `embroidery.patches.fillHoop {gap, count}` | patches: a hoopful as a new design (a name each), every hoopful written as machine files, the open design repeated across its hoop |
| `embroidery.set {type}` | give the selection a stitch type (`run`, `bean`, `blanket`, `satin`, `fill`, `satinfill`, `contour`, `spiral`, `radial`, `cross`, `meander`, `applique`, `manual`, `redwork`, `none`) |
| `embroidery.setParams {params}` | change stitch parameters (camelCase names, mm) |
| `embroidery.setThread {color, name?, code?, chart?}` | set the thread |
| `embroidery.convert` | stitch types from the artwork's look |
| `embroidery.autoDirection` / `addGuide` / `moveGuide` / `removeGuide` / `clearGuides` / `setBeads` | stitch direction and start/stop |
| `embroidery.setParams {params: {texture: {pattern, size?, angle?} \| null}}` | a tatami fill's texture (programmed splits): `diamonds`, `waves`, `herringbone`, `bricks`, `circles`, `scales` |
| `embroidery.carveSplits {ids?, repeat?, keep?}`, `embroidery.setSplits {id, splits}`, `embroidery.clearSplits {ids?}` | split lines in tatami fills: carved from other art (a stamp's outline, lines, lines repeated across the fill), set from points, cleared |
| `embroidery.contourSplits {ids?, spacing?}` | split lines following tatami fills' outlines inward |
| `embroidery.carveStamp {stamp, ids?, at?, size?, angle?}`, `embroidery.stamps` | press a carving stamp from the library into tatami fills; list the library |
| `embroidery.fillAlongLines {ids?, keep?}`, `embroidery.setFlowLines {id, lines}`, `embroidery.clearFlowLines {ids?}` | flow lines in tatami fills: rows copying one line or flowing from one to another, taken from other art, set from points, cleared |
| `embroidery.placeMotif {motif, size?, at?, angle?}` | one motif (built-in or the design's) as a path sewn in a single run |
| `embroidery.motif.make {name?, ids?}`, `embroidery.motif.apply {name, ids?}`, `embroidery.motif.list`, `embroidery.motif.delete {name}` | the design's own motifs: made from the selected path, sewn by the selected runs and fills (paths not stitched yet become motif runs or fills), listed with how many objects sew each, removed |
| `embroidery.addLaydown {ids?, margin?, spacing?, angle?}` | an open fill sewn first under the selected shapes and type (pile fabrics) |
| `embroidery.makeBlend {colors, ids?, spacing?, separate?}` | a tatami fill as stacked layers blending 2 to 6 thread colours across its rows, in the one object (or one object per layer) |
| `embroidery.addOutline {ids?, shape?, offset?, stitch?, width?, motif?}` | a stitched outline or border round the selected shapes, lines and type |
| `embroidery.exportAnimation {path?, frames?, seconds?, hold?, ppmm?}` | the design sewing itself as a looping animated GIF |
| `embroidery.memoryCards`, `embroidery.memoryCard.save {card, path?}` | the memory cards a card writer box writes, and the design saved in the format a card's machines read, checked against what it holds |
| `embroidery.addBorder {ids?, width?, color?, smooth?, edge?, edgeWidth?, edgeColor?}` | a sticker- or patch-like band of fill round a design's outline, sewn first, with a satin or run edge |
| `embroidery.placeRing {stitch?, diameter?, width?, at?}` | a satin, filled or bean ring |
| `embroidery.draw {brush, points \| stamp, …}` | a brush stroke, as the widget draws it |
| `embroidery.column {points: [[x, y, curve?]…], id?}` | a Column A/B satin column from points on its two sides in turn; with `id`, continue that column |
| `embroidery.autoDigitize {colors, …}` | trace a selected image into stitches |
| `embroidery.placeArt {svg, size?, at?, frame?, name?, minArea?}` | place flat-colour SVG art and digitize it, as Design from Prompt does with Claude's drawing (its linked files are never read); `frame` fits the SVG's page into a rectangle, as Redraw Image does over the image |
| `embroidery.settings {hoop, fabric, chart, machine, …}`, `embroidery.applyFabric {name}` | design settings |
| `embroidery.machine.list`, `embroidery.machine.save {name, newName?, format?, fromDesign?, …}`, `embroidery.machine.delete {name}` | machine profiles: change the standard ones, make, rename and delete your own |
| `embroidery.crossStitch.new {cols?, rows?, cell?, style?, at?}`, `.paint {at? \| cells?, color?, erase?, size?}`, `.fromImage {cols?, colors?, background?}`, `.set {cell?, style?}`, `.info` | cross-stitch patterns: an empty grid, squares painted or erased, a picture as a pattern, square size and style, the colours and their squares |
| `embroidery.sortByColor` | sew each colour in one go |
| `embroidery.autoSequence {ids?}` | reorder for the fewest colour changes and the shortest travel, overlaps kept; with `ids`, only inside those groups |
| `embroidery.matchThreads {chart?, all?}` | the chart's closest thread for each stitched object |
| `embroidery.makeCurrent`, `embroidery.current`, `embroidery.current.setParams {type, params}`, `embroidery.current.reset {type?}` | the properties new objects take |
| `embroidery.style.new {name?}` / `apply {name}` / `redefine {name}` / `unlink` / `rename {name, to}` / `duplicate` / `delete` / `list` | embroidery styles |
| `embroidery.fabric.save {name, from?, …}`, `embroidery.fabric.delete {name}` | fabrics made for the design |
| `embroidery.branch {ids?}`, `embroidery.breakApart {ids?}` | sew objects as one path; separate a branch, a redwork group or a many-part object |
| `embroidery.addButtonholes {style?, length?, width?, angle?, at?, to?, count?}` | straight, rounded or keyhole buttonholes: one, or a string evenly spaced from `at` to `to` |
| `embroidery.knife {ids?, overlap?}` | cut stitched shapes round their holes and at their branches into pieces that each turn their own way, a branch group |
| `embroidery.redwork {ids?, kind?}` | sew line art as redwork: one line ending where it starts, the way between lines retraced along them (`kind`: `run`, `bean`, `backstitch`, `stemstitch`) |
| `embroidery.reverse`, `embroidery.removeHoles {maxArea?}`, `embroidery.fillHoles` | sewing direction and holes |
| `select.same.thread`, `select.same.stitchType` | select like objects |
| `embroidery.check` | sewing problems (dense spots, long stitches, tiny objects, small lettering, hoop) |
| `embroidery.stitches {id}`, `embroidery.stitches.edit {id, ops}` | an object's own stitches; move, delete and insert them |
| `embroidery.freeze`, `embroidery.regenerate` | keep stitches as they are for editing; drop the edits |
| `embroidery.stitches.split {id, at}` | split an object's stitches in two |
| `embroidery.function.insert {id, at, kind}`, `embroidery.function.clear {id, at}` | trims, stops and jumps at single stitches |
| `embroidery.info`, `embroidery.plan` | statistics; the stitches themselves |
| `embroidery.export {format}` | a machine file (`dataBase64`) |

From the control channel (`stitchfoundry --control 7979`, one JSON object per line):

```json
{"id": 1, "method": "engine.execute", "params": {"command": "shape.ellipse", "params": {"x": 50, "y": 50, "width": 200, "height": 200}}}
{"id": 2, "method": "engine.execute", "params": {"command": "embroidery.set", "params": {"type": "fill"}}}
{"id": 3, "method": "engine.execute", "params": {"command": "embroidery.export", "params": {"format": "dst"}}}
```

## How it is built

| Crate | Layer | Role |
|---|---|---|
| `sf-core` | standalone | geometry, the embroidery model, the stitch engine (runs, satin, fills, underlay, appliqué, connectors, ties, trims), inclination lines and turning, brushes, threads, hoops, fabrics; Design from Prompt's and Ask About Embroidery's requests and the reading of their replies (`ai`, `ai::ask`) |
| `sf-formats` | 1 | machine file readers and writers |
| `vectorcraft-embroidery` | 2 | the document side: converts paths and type to embroidery objects (millimetres), plans the whole document in paint order, caches unchanged objects |
| `vectorcraft-engine` | 5 | the `embroidery.*` commands and machine-file open/place |
| `vectorcraft-tools` | 4 | the Embroidery Brush and Inclination tools |
| `vectorcraft-ui-egui` | 6 | canvas overlay (TrueView mesh, hoop, beads, player), panels, widget, menus, the Design from Prompt dialog, the Ask About Embroidery window |
| `vectorcraft` (the desktop app) | 7 | sends Design from Prompt's and Ask About Embroidery's requests (ureq, rustls) and keeps the API key |
| `vectorcraft-web` (the web app) | 7 | sends them with the browser's fetch and keeps the key in the browser's local storage |

Documents store embroidery as an optional attribute on each node (`Node.embroidery`) and on
the document (`Document.embroidery`: hoop, fabric, chart). Older documents and documents without
embroidery load and save as before. Stitch order is paint order (bottom first). Positions in the
document are points; stitch settings are millimetres.

## Clean-room notes

The embroidery features are written from scratch from public descriptions of how digitizing
works and from the behaviour users expect of digitizing software. No code, assets or file
samples were taken from commercial digitizers or from open-source embroidery projects (some of
which are GPL). The machine file code was written from general format knowledge and is tested by
round trips and robustness tests in the repository.

## Known limits

- Machine files have not been sewn on real machines yet (see above).
- Machine translations of the new interface text into the 11 interface languages want a review by
  native speakers who digitize. In Russian and Ukrainian, the summary counts ("4 colors") use one
  plural form for every number.
- Auto-Digitize works best on flat-colour logos; photos and gradients need tracing by hand
  first.
- Design from Prompt and Redraw Image work with your own Anthropic API key only (StitchFoundry
  accounts and AI credits aren't built yet). The drawing follows the
  description or the image loosely and can use more colours than asked (colours close enough to
  share a thread are merged), and a drawing is billed even when it comes out unusable.
- Branching orders objects closest-first rather than searching for the best tour, and only
  carries the thread across under objects of the branch sewn later.
- Appliqué is one object (placement, tack-down, cover). Splitting an appliqué where pieces overlap
  ("break up") and closing open appliqué shapes are not built yet.
