# Send to StitchShop (SHOP-01): the plan

Status: planned, 2026-10-22. StitchHub's side is built (the "StitchFoundry inbox": `src/inbox.mjs` and
the README's section of that name in [Stitchwright/stitchhub](https://github.com/Stitchwright/stitchhub),
on the branch `claude/happy-einstein-mmdl83` at the time of writing), tested against stand-ins only.
This page is StitchFoundry's side: what it sends, from where, and its answers to the hub's questions.

## What the user does

**File › Send to StitchShop…**, and **Send to StitchShop** on a job order (Embroidery › Job Orders):
pick the shop (when the account has more than one), check the format and what goes with it, send.
The design is sent as DST unless the job or the user chooses another format, with what StitchFoundry
knows about it (the `meta` below) and, once the hub has a slot for it, a preview picture. The status
says it arrived, with **Take It Back** while no shop has taken it.

## How it works

Every request carries the Foundry Account session (`Authorization: Bearer sfs_…`) the app already
holds after signing in to cloud.stitchfoundry.app. The hub asks the Foundry Account service whose it
is (`POST /v1/account`, its numeric `id`) and keeps nothing of it.

1. `GET {hub}/v1/inbox/shops` → the account's shops, for the picker (left out with one shop).
2. `POST {hub}/v1/inbox/put {fileName, size, sha256, meta, toShopId?}` → `{id, url, headers}`.
3. `PUT` the bytes to `url` with exactly those headers and `Content-Length: size`.
4. `POST {hub}/v1/inbox/done {id}`: only then does the file appear to the shop.
5. `POST {hub}/v1/inbox/retract {id}` for Take It Back (409: a shop has taken it already).

- **The hub's address** is a setting (Preferences › StitchFoundry, `stitchshop-hub`), by default
  `https://hub.stitchbench.app`; a self-hosted hub has its own. HTTPS only (or this computer).
- **The file name** is the design's (or the job's number and design), made to fit
  `^[A-Za-z0-9][A-Za-z0-9 ._()+-]{0,119}$`; **size** 1 byte to 10 MB, checked before sending.
- **Errors** are said in words: 401 sign in again; **404 "link your shop first: in StitchShop,
  Settings › Foundry Account"** (or the shop chosen isn't the account's, or the hub has its inbox
  off); 413 the inbox is full (25 files or 100 MB waiting); 429 try again later; 502/503 the hub or
  the service didn't answer.
- **Where the code goes**, as for Publish to MakerFoundry (PUB-01): the requests made and read in
  `sf_core::ai::stitchshop` (no network code), the sending in the app, a command
  (`embroidery.sendToStitchShop`) for the control channel and MCP, the web app too (the bucket's
  signed address must allow the web app's origin: a CORS rule for the hub's owner).
- **Tests**: the request sequence against a stand-in hub, as `test/inbox.mjs` does from the hub's
  side; every error answer; a file name that has to be cleaned; a 10 MB limit.

## What `meta` carries

All of these, from the design (`embroidery.info`) and, when sent from a job order, the job:

| Key | From |
|---|---|
| `job` | the job order's number (J-0001…) |
| `customer`, `garment`, `placement`, `due` (yyyy-mm-dd), `quantity` | the job order |
| `notes` (500 characters) | the job order's notes, else the design's notes |
| `program`, `version` | "StitchFoundry" and the app's version |
| `stitches`, `colors`, `widthMm`, `heightMm` | the design: always sent, whatever the format |
| `runMinutes` | the design's sewing time, rounded up |
| `threads` | the threads in sewing order, by name and code |

## Answers to the hub's questions

**1. Job file.** StitchFoundry has job orders but no job file: they live in the app's preferences
(`embroidery.jobs`, `embroidery.job.create` and the rest), with `number` (J-0001…), `customer`,
`design` (the design's file), `quantity`, `due` (YYYY-MM-DD), `garment`, `placement` (left-chest,
right-chest, full-front, full-back, cap-front, sleeve, yoke, pocket or words), `notes`, `status`
(open, production, done, cancelled) and `created`. `meta` can be exactly those with the design's
numbers, in the hub's keys (the table above). Fields worth adding to the hub's list, all of which
StitchFoundry has: **`design`** (the design's name), **`format`** (dst, pes…), **`hoop`** (the hoop's
name), **`fabric`** (the fabric preset), **`trims`**, **`colorChanges`** (with stops, distinct from
`colors`: a design can use one thread twice), **`threadM`** and **`bobbinM`** (thread and bobbin
used, metres, for costing).

**2. DST accuracy.** The header fields are written from the records (`crates/sf-formats/src/dst.rs`):
`ST:` is the number of records, every one, jumps, trim jumps and the end record included, so it is
more than the stitch count; `CO:` the colour changes and stops; `+X: -X: +Y: -Y:` the extents from
the start point in 0.1 mm, and `AX: AY:` the end point. A colour change is a record with flag 0xC3,
and a stop is the same code (DST has no other: it reads back as a colour change). What a counter
should know: **a trim is three small jumps that cancel out** (+0.2/+0.2, −0.4/−0.4, +0.2/+0.2 mm),
the usual way for DST; moves longer than 12.1 mm are split into several jump records, and one longer
than 24.2 mm (three jumps or more) gets a trim before it, since machines cut there anyway; the end is
`00 00 F3`. So stitches are the records with 0x03 in byte 3, not `ST:`.

**3. Preview picture.** Yes, please add a preview slot. StitchFoundry makes a PNG of the stitches as
they sew (`embroidery.exportImage`, on the fabric's colour) and, with the product visualizer, the
design on a garment photo. Either fits 2 MB as a PNG or JPEG.

**Corrections:** none. `POST /v1/account` answers `{id, email, credits, …}` with `id` a number, the
account's lasting one, as the hub expects.
