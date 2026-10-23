# How the machine files are verified

Status: 2026-10-23 (FR-01). A file that a machine misreads costs a user fabric, thread and
trust, so every machine format StitchFoundry writes is checked three ways.

1. **Round trips** (`cargo test -p sf-formats`): every writer's file read back by StitchFoundry's
   own reader gives the stitches that were written (`tests/roundtrip.rs`, golden files, and the
   robustness fuzz for the readers).
2. **An independent reader.** The same files read by pyembroidery 1.5.1
   (<https://github.com/EmbroidePy/pyembroidery>, MIT), a reader StitchFoundry's code doesn't
   share, and compared with StitchFoundry's own stitch list of the design (the JSON export):
   - every needle point, in order, within 0.16 mm (the formats' 0.1 mm units);
   - the design's size, the colour changes and stops (stops become colour changes where a
     format has none), and the trims where the format has a trim command;
   - thread colours: exact for the RGB formats (VP3, XXX, PCS); for the palette formats (PES,
     PEC, JEF, SEW, HUS) exactly the palette colour that looks nearest (CIEDE2000), from
     pyembroidery's own palette tables.
3. **Header fields machines use** (the design's size, stitch and colour counts, the hoop code):
   DST's `ST`, `CO` and extents, JEF's point and colour counts, extents and hoop code, and the
   PEC block's colour count and size (in PEC and PES files), checked against the stitches.

The designs: a fill, a satin column and a run in three threads; pieces in the far corners of a
5×7 hoop (long moves, trims); appliqué (two stops); sixteen threads; a patch (dense round fill,
satin border); a 360 mm design with pieces 340 mm apart and long run stitches; lettering; the
whole 5×7 field. Each in the 13 writable formats: 104 files.

## 2026-10-23 results

All 104 files agree on all three after these fixes:

- **DSB (Barudan):** trims, stops and needle changes were written without the sign bits a zero
  move carries in Barudan's files (`87` for a trim where the files have `E7`); readers that look
  for `E7`, `E8` and `E9`–`F7` stopped at the first trim. U01 shares the records and gets the
  same fix.
- **SEW (older Janome, Elna, Kenmore):** colours were written as JEF palette indices, but SEW has
  a palette of its own (42 of its 79 colours differ): a navy showed as bright blue. SEW now has
  its own table (`sew_chart`).
- **Palette colours:** thread colours now go to the palette colour that looks nearest by
  CIEDE2000 rather than by Lab distance (a mid blue became the much darker Ultramarine; it is now
  Cornflower Blue).

Not checked here, and still to do on real machines: whether each machine model accepts the
files (Brother, Janome, Bernina, Husqvarna Viking/Pfaff, Barudan and Tajima machines), how
machines treat JEF's and PCS's lack of a trim command, and the experimental HUS and SEW writers.
The sew-test set (the same eight designs) is for that.

## Running it again

The scripts live outside the repository (they are Python and use pyembroidery): make the eight
designs with `stitchfoundry-cli run … --cmd embroidery.export --params '{"format": F}'` for the
JSON stitch list and each format, then read each file with `pyembroidery.read` and compare as
above. Run it before a release and after any change under `crates/sf-formats`.
