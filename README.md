<p align="center">
  <img alt="StitchFoundry" src="assets/app-icon/stitchfoundry-1024.png" width="148">
</p>

<h1 align="center">StitchFoundry</h1>

<p align="center">
  <b>Open-source machine-embroidery digitizing, by SiteSentrix.</b>
</p>

<p align="center">
  Turn shapes, type and images into stitches, and write the files your embroidery machine reads.
  It runs natively on macOS, Windows, Linux and FreeBSD, and in the browser, and is written in pure
  Rust on top of the VectorCraft vector editor.
</p>

<p align="center">
  <a href="https://github.com/Stitchwright/StitchFoundry/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/Stitchwright/StitchFoundry/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="Status: pre-release" src="https://img.shields.io/badge/status-pre--release-f2546b">
  <img alt="Written in pure Rust" src="https://img.shields.io/badge/pure-Rust-b83a24?logo=rust&logoColor=white">
  <img alt="Runs on macOS, Windows, Linux, FreeBSD and the web" src="https://img.shields.io/badge/runs%20on-macOS%20%C2%B7%20Windows%20%C2%B7%20Linux%20%C2%B7%20FreeBSD%20%C2%B7%20Web-555555">
  <img alt="MCP server for agents" src="https://img.shields.io/badge/agents-MCP%20server-555555">
  <img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-555555">
</p>

> [!NOTE]
> **Pre-release.** Every machine format round-trips in software, and files StitchFoundry writes are
> read correctly by an independent open-source embroidery library, but nothing has been sewn on a
> real machine yet: test-sew before production runs.

## Download

<p align="center">
  <a href="https://github.com/Stitchwright/StitchFoundry/releases/tag/v0.14.0-rc.1"><b>⬇ Download StitchFoundry 0.14.0 (release candidate 1)</b></a>
  (Windows, macOS, Linux; signed) ·
  <a href="https://stitchwright.github.io/StitchFoundry/"><b>Try it in your browser</b></a>
</p>

| System | File to download |
|---|---|
| Windows (64-bit, 32-bit or ARM) | `stitchfoundry-…-windows-x64.msi` (installer) or `…-portable.zip`; `…-x86` and `…-arm64` likewise |
| macOS (Apple silicon and Intel) | `stitchfoundry-…-macos-universal.dmg` |
| Linux (x86_64 or ARM) | `.AppImage` (runs anywhere), `.deb` (Debian, Ubuntu), `.rpm` (Fedora) or `.flatpak` |
| FreeBSD 14 | `stitchfoundry-…-freebsd-x86_64.tar.gz` |

The Windows installers and programs are signed, and the Mac app is signed and notarized by
Apple, so they open without a warning. The browser version is a demo: it can't download
embroidery fonts. Builds of every change since the release, unsigned, are on the
[latest build](https://github.com/Stitchwright/StitchFoundry/releases/tag/latest-build) page
(Windows may warn: **More info › Run anyway**; on macOS right-click › **Open** the first time).

<p align="center">
  <img src="docs/images/stitchfoundry-branching.png" alt="StitchFoundry: thirty satin petals and a fill vine sewn as one thread, with the Stitch Sequence panel" width="100%">
  <br><sub>Branching: thirty satin petals and a fill vine sewn as one thread, ordered in the Stitch Sequence panel</sub>
</p>

## What it does

- **Stitch types on anything you draw.** Any path, shape or type can carry running stitches
  (single, triple, bean, motif), satin and satin columns, tatami and other fills, or appliqué. Edit
  the shape and the stitches follow.
- **Direction placed for you, then yours to change.** Stitch angles are chosen automatically and
  stay editable on the canvas with inclination lines and start and stop beads.
- **Fill textures.** Tatami fills take a texture pattern (diamonds, waves, herringbone, bricks,
  circles, scales) or lines carved in from your own art: a drawn line, a shape's outline like a
  carving stamp, or a curve repeated across the fill.
- **Digitizing tools.** Column A/B digitizing, a radial digitizing widget with satin brushes,
  lettering from any font, and Auto-Digitize for flat-colour images.
- **Design from Prompt and Redraw Image.** Describe a design, or select a logo, and Claude draws
  it as clean flat-colour art, which StitchFoundry digitizes in one step (desktop app, with your
  own Anthropic API key).
- **Stitch-level control.** Edit single stitches and machine functions, split objects, branch
  objects into one thread run, sequence automatically, and keep objects alike with styles.
- **Design checks** for dense areas, long stitches and lettering too small to sew, machine
  profiles for each format, a hoop list, fabrics, thread charts and a stitch player.
- **Machine files:** DST, PES and PEC, JEF, EXP, VP3, XXX and U01, to open and to export.

[`docs/embroidery.md`](docs/embroidery.md) covers it all as you use it.

## Screenshots

<table>
  <tr>
    <td width="50%"><img src="docs/images/stitchfoundry-home.png" alt="The Home screen: start a design for your hoop, first steps, samples and tips"><br><sub><b>Home.</b> Start a design the size of your hoop, follow the first steps or open a sample.</sub></td>
    <td width="50%"><img src="docs/images/stitchfoundry-sample.png" alt="The Sun and Moon sample: a satin-edged sun with a fill centre and a fill crescent moon"><br><sub><b>Stitches on the canvas.</b> The Sun and Moon sample, shown thread by thread as it will sew.</sub></td>
  </tr>
  <tr>
    <td><img src="docs/images/stitchfoundry-applique-and-print.png" alt="Appliqué pieces in gingham, denim and plain fabric, a foil star, a printed band and coil and square-loop runs"><br><sub><b>Appliqué, foil and print.</b> Fabric pieces with their tack-down and cover stitches, a foil star, a printed band and two motif runs.</sub></td>
    <td><img src="docs/images/stitchfoundry-sequins-and-beads.png" alt="A heart filled with sequins, a ring of stones, beads, a couched cord, boring holes and star sequins"><br><sub><b>Sequins, stones, beads and cord.</b> Decorations placed along a line or filling a shape, with their holding stitches.</sub></td>
  </tr>
  <tr>
    <td><img src="docs/images/stitchfoundry-image-to-stitches.png" alt="A placed image with the Embroidery panel previewing it reduced to six thread colours"><br><sub><b>Image to stitches.</b> Choose how many threads a picture becomes and preview it before Auto-Digitize.</sub></td>
    <td><img src="docs/images/stitchfoundry-cap-preview.png" alt="The heart patch sample previewed life size on the front of a navy cap"><br><sub><b>See it on the product.</b> The design life size on a cap, T-shirt, polo, hoodie, tote or towel before you sew it.</sub></td>
  </tr>
  <tr>
    <td colspan="2"><img src="docs/images/stitchfoundry-help.png" alt="The Help window open at Digitizing basics"><br><sub><b>Help built in.</b> Press F1 for the guide, including Digitizing basics: stitch lengths, density, underlay, pull and push compensation.</sub></td>
  </tr>
</table>

## Built on VectorCraft

StitchFoundry includes the full [VectorCraft](https://github.com/storytold/vectorcraft) editor: pen,
shape and brush tools, Pathfinder, type, gradients, live effects and artboards, and SVG, PDF, EPS,
DXF, EMF and raster files. Every action is an engine command, so agents can drive the app over a
JSON control channel and an MCP server the way a person does.

## Free and open source

StitchFoundry is free and open source. Its AI features (Design from Prompt, Redraw Image with
Claude) run on your own Anthropic API key; optional cloud credits for them are planned.

## Build and run

Rust 1.97 or newer.

```sh
cargo run --release -p vectorcraft                          # the app (the program is stitchfoundry)
cargo run --release -p vectorcraft -- --control 7979        # + JSON control channel
cargo run --release -p vectorcraft-cli -- mcp               # MCP server (stdio)
cargo xtask ci                                              # fmt, clippy, tests, layering, wasm, vendor names
```

Japanese fonts come from the optional craft-fonts build input
(`CRAFT_FONTS_DIR="$PWD/../craft-fonts"`); without it, Japanese text uses the system fonts. See
[`docs/development.md`](docs/development.md) for graphics processors, Wayland and other platform notes.

## Docs

- [`docs/embroidery.md`](docs/embroidery.md): embroidery, as you use it
- [`docs/stitchfoundry-spec.md`](docs/stitchfoundry-spec.md): the build spec (architecture, feature
  matrix, roadmap, decisions)
- [`docs/mcp.md`](docs/mcp.md) and [`docs/control-protocol.md`](docs/control-protocol.md): driving it
  from agents
- [`AGENTS.md`](AGENTS.md): rules for contributors (clean-room, assets, no panics, quality gates)

StitchFoundry is written clean-room: behaviour comes from public documentation only, never from
commercial embroidery software.

## Reporting bugs

In the app, **Help → Report a Bug…** opens StitchHub's report page, filled in with the version and your system.
Describe what happened and send it: it becomes an issue in this repository (a public one, so leave out anything
private). You can also [open an issue](https://github.com/Stitchwright/StitchFoundry/issues) yourself. A maintainer can
have Claude work on an issue by labelling it `claude-fix`; Claude opens a pull request for review, and never merges.

## License and credits

StitchFoundry is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Copyright (c) 2026 Bruce Wattendorf (SiteSentrix) and the StitchFoundry contributors.<br>
Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors.

Required notices are in [NOTICE](NOTICE). Bundled fonts, icons, images and other assets keep their
own open licences; each one is listed with its author, source and licence in [ASSETS.md](ASSETS.md).
The app icon is original artwork; its files are in [`assets/app-icon/`](assets/app-icon/README.md).

### Where it comes from

The GitHub repositories StitchFoundry takes code, data or fonts from:

| Repository | What StitchFoundry uses | How |
|---|---|---|
| [storytold/vectorcraft](https://github.com/storytold/vectorcraft) | the whole vector editor StitchFoundry is built on | merged in from time to time (last: version 0.13.1, upstream `aa9c70a7`, 2026-10-19); MIT OR Apache-2.0 |
| [storytold/craft-libs](https://github.com/storytold/craft-libs) | the PDF reader (patched [hayro](https://github.com/LaurenzV/hayro) crates) | fetched at a pinned revision when building; MIT OR Apache-2.0 |
| [storytold/craft-fonts](https://github.com/storytold/craft-fonts) | the Japanese fonts | optional build input (release builds), never copied here; OFL |
| [adobe-fonts/source-sans](https://github.com/adobe-fonts/source-sans), [rsms/inter](https://github.com/rsms/inter), [JetBrains/JetBrainsMono](https://github.com/JetBrains/JetBrainsMono) | the interface fonts | copies in `assets/fonts` (from VectorCraft); OFL, listed in [ASSETS.md](ASSETS.md) |
| [EmbroidePy/pyembroidery](https://github.com/EmbroidePy/pyembroidery) | the Brother PEC and Janome JEF thread colour tables | copied once (version 1.5.1); MIT, notice in [NOTICE](NOTICE) |
| [Stitchwright/stitchfoundry-fonts](https://github.com/Stitchwright/stitchfoundry-fonts) | embroidery fonts | downloaded in the app (Embroidery › Embroidery Fonts › Get Embroidery Fonts…), not in this repository |
| [inkstitch/embroidery-fonts](https://github.com/inkstitch/embroidery-fonts) | embroidery fonts in Ink/Stitch's format | downloaded in the app, not in this repository; each font keeps its own licence |

Ink/Stitch itself is only a reference from its public documentation: none of its (GPL) code is used.
Every other dependency is an ordinary Rust crate from crates.io, listed in `Cargo.lock`.

StitchFoundry is based on [VectorCraft](https://github.com/storytold/vectorcraft) by the ArtCraft team.
It is an independent project, not made, sponsored or endorsed by the ArtCraft team.

<sub>Tajima, Brother, Janome, Melco, Pfaff, Husqvarna Viking, Singer and Barudan are trademarks of their
owners, named only to identify the file formats and hoops StitchFoundry works with. Adobe and Illustrator are
trademarks of Adobe Inc.; StitchFoundry is not affiliated with Adobe.</sub>
