# What's new in StitchFoundry

Release notes for each version, newest first, written for the people who use StitchFoundry. The
release workflow copies a version's section into its GitHub release (`docs/releasing.md`).

## 0.14.0

The first full release of 0.14. It has everything in the 0.14.0-rc.1 test release below (Simple
mode, Online Orders, 39 built-in fonts, Save for a Memory Card and more), plus:

- **Open in StitchFoundry from MakerFoundry**: when a design can't be opened, the web app now
  shows MakerFoundry's own explanation, and the link is cleared from the address bar.
- **Help › Suggest a Feature…** lets you send us an idea, next to Report a Bug.
- **The web app** no longer stops when it checks for updates.
- **Drawing and text**, from the VectorCraft editor StitchFoundry is built on: steadier Shape
  Builder and Join, angle constraints for arrow keys and Shift-drags, better Character and
  Paragraph panels, and SVG, PDF and EPS files that keep more of what you made.

As always, test-sew a design before using it on a garment that matters.

## 0.14.0-rc.1 (test release)

Everything since 0.13.1-rc.2. A test release: please tell us what doesn't work, and test-sew
designs before using them on a garment that matters.

### Easier for beginners

- **Simple mode for people new to computers.** Save for My Machine finds the USB stick you have
  plugged in and saves straight onto it, with one big button per stick when there are several.
  The bottom bar shows See It All, Closer and Farther instead of numbers. Add a Picture opens your
  Pictures folder and puts the picture straight in. Threads are called by colour ("Red", "Sky
  Blue") instead of codes, and sewing times read "7 min" or "4 h 31 min".
- **Light or dark screen**, chosen next to Text size.
- **Big text sizes work everywhere.** Windows taller than the screen scroll, so Done and Next are
  always in view.
- **Plainer words** on Home and in the hints.
- **Make a Design** no longer stops when no hoop or machine is chosen: it uses the everyday 5×7
  hoop. Words too long for the hoop are made smaller to fit, and it says so. A very big design
  says how long it will take to sew.
- **Nine sample designs**, shown as pictures on Home.
- **The app tells you when a newer StitchFoundry is out** (a card on Home, and Help › Check for
  Updates).

### Online orders (new)

- **Embroidery › Online Orders…** reads orders from your Shopify or WooCommerce shop, or a CSV
  export from Etsy or any other shop. It makes each personalized item into a design: the
  customer's name or text, in the font and thread colour your rules choose, fitted to a template.
  Each item also gets a job order, its machine files and a worksheet with a barcode.
- Each order line is made once. It can check every 15 minutes while the window is open.
- **Find a job** by scanning a worksheet's barcode, or by typing the job or order number.
- Setup guide, step by step for each shop: [docs/online-orders.md](https://github.com/Stitchwright/StitchFoundry/blob/main/docs/online-orders.md).

### Fonts

- **39 built-in embroidery fonts** (the Foundry fonts): scripts, serifs, sans, display, western,
  gothic and a 5 mm font for small text, made from openly licensed typefaces. Together with the
  131 free fonts a click away (Get Embroidery Fonts), that's 170. The new fonts were checked on
  screen; please sew a test before relying on them.
- **Kerning classes** in the font creator: kern a letter against a whole group at once.

### Saving and sharing

- **Save for a Memory Card**: pick your machine's brand and it uses the right format and limits.
- **Eject USB Stick** after Save for My Machine. On macOS, StitchFoundry can be the default app
  for machine files.
- **Watch it sew as a video**: save the sew-out as a looping GIF or an MP4.
- **Send to StitchShop**: the design, its preview and details go to your StitchShop inbox (and
  Take It Back).
- **Free web viewer**: open a design from a link in the browser, with Edit It Here and Get
  StitchFoundry Free.
- **Husqvarna HUS** and **Janome SEW** are read and written, and **Pfaff VIP** is read (all
  experimental).
- **Machine files checked against an independent reader**, with fixes, and a warning when a
  format can't keep something.

### Photos and artwork

- **Remove Background** for photos, with **Select Subject**, a Cut-Out Brush and a before-and-after
  preview. Make a Design cuts a photo's subject out first (a setting).
- **Magnetic Lasso**: draw round part of a photo and the shape follows its edges.
- **Click to Stitch** can take every area of a colour, not only the one clicked.
- **Auto-Digitize**: no fabric showing through between colours, and thin dark lines sewn as runs
  on top.
- **Borders round digitized photos.**
- **New photo stitch styles**: sepia, black and white, and colour tiles.
- **DWG drawings** open on the desktop, and **binary DXF** opens. The **Fill Region** tool makes a
  fill from a click between loose lines.
- **Warp and Distort effects** are sewn as the art shows.
- **Design from Prompt**: an Effort setting with the cost shown before and after. Thorough
  drawings look up what they draw and are checked as they stitch.

### Projects and wizards

- **More in-the-hoop projects**: zipper pouch, pencil case, card holder, phone case, passport
  holder, book cover, key fob, luggage tag and bookmark. Projects come in more sizes, with
  rounded, scalloped or round edges.
- **Endless Border**, **Spiro**, **Repeat Design**, **Quilt Background** and **Family Tree**
  wizards.

### Stitching

- **New stitch effects**: Make Eyelets (the hole, a stop to cut it, a satin binding), Make
  Lacework (free-standing lace), StumpWork pieces, Wave Fill, ragged edges on fills, and
  blackwork and candlewick motifs.
- **Recolour Threads**: Lighter, Darker and Tinted, in one step.
- **Remove Short Stitches** from any machine file.
- **Satin columns** can be denser in some places than others, or spaced unevenly on purpose for
  texture.

### From VectorCraft (the drawing app StitchFoundry is built on)

About 200 improvements to drawing, type and file import:
- Objects on Path.
- A new Simplify slider.
- Measurement labels while drawing.
- Rotate View dial.
- Better rulers in inches.
- More Sketch and Artistic effects.
- Live text kept in placed PDFs.
- Saved workspaces keep their panel widths.
- Many fixes.

### Not in this build

- **HEIC (iPhone) photos** are left out of official builds for now, because of the HEVC patents
  in some countries. Convert them to JPEG first.
