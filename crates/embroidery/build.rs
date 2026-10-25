//! The optional StitchFoundry fonts build input (FONT-01;
//! https://github.com/Stitchwright/stitchfoundry-fonts): with `STITCHFOUNDRY_FONTS_DIR=<checkout>`
//! set, every font folder under its `fonts/` (font.json, glyphs.stitchfoundry and the licence
//! beside them) is embedded as `BUNDLED_FONTS`, and the app has them without loading anything;
//! unset, `BUNDLED_FONTS` is empty and fonts are loaded or downloaded as before. Font files never
//! live in this repository. `STITCHFOUNDRY_FONTS_REQUIRED=1` turns a missing or bad checkout into a
//! build error (release builds). Nothing is fetched: the build reads only the local checkout.
//! The web build (wasm32) embeds only the folders `fonts/web.txt` lists, when there is one, so the
//! page stays small to download.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The largest file embedded (bytes): the Foundry fonts are well under a megabyte each.
const MAX_FILE: u64 = 8 << 20;
/// The most fonts embedded.
const MAX_FONTS: usize = 64;

fn main() {
    println!("cargo::rerun-if-env-changed=STITCHFOUNDRY_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=STITCHFOUNDRY_FONTS_REQUIRED");
    let required = std::env::var_os("STITCHFOUNDRY_FONTS_REQUIRED").is_some_and(|v| !v.is_empty());
    let mut entries = String::new();
    match std::env::var_os("STITCHFOUNDRY_FONTS_DIR").filter(|d| !d.is_empty()).map(PathBuf::from) {
        Some(dir) => {
            let dir = from_workspace(dir);
            match fonts(&dir) {
                Ok(e) if e.is_empty() && required => println!("cargo::error=STITCHFOUNDRY_FONTS_DIR={}: no fonts under fonts/", dir.display()),
                Ok(e) => entries = e,
                Err(e) if required => println!("cargo::error=STITCHFOUNDRY_FONTS_DIR={}: {e}", dir.display()),
                Err(e) => println!("cargo::warning=building without the StitchFoundry fonts: STITCHFOUNDRY_FONTS_DIR={}: {e}", dir.display()),
            }
        }
        None if required => println!("cargo::error=STITCHFOUNDRY_FONTS_REQUIRED is set but STITCHFOUNDRY_FONTS_DIR isn't"),
        None => {}
    }
    let src = format!("pub static BUNDLED_FONTS: &[BundledFont] = &[\n{entries}];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("bundled_fonts.rs");
    // unchanged output keeps its time, so nothing above rebuilds for nothing
    if std::fs::read_to_string(&out).is_ok_and(|old| old == src) {
        return;
    }
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// A relative directory is taken from the workspace root, where `cargo` is usually run.
fn from_workspace(dir: PathBuf) -> PathBuf {
    if dir.is_absolute() {
        return dir;
    }
    PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default()).join("../..").join(dir)
}

/// One `BundledFont { .. }` per folder under `dir/fonts` holding font.json and glyphs.stitchfoundry,
/// in name order.
fn fonts(dir: &Path) -> Result<String, String> {
    let root = dir.join("fonts");
    let mut folders: Vec<PathBuf> = std::fs::read_dir(&root)
        .map_err(|e| format!("{}: {e}", root.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("font.json").is_file() && p.join("glyphs.stitchfoundry").is_file())
        .collect();
    folders.sort();
    // the web app: just the fonts `web.txt` names (a folder a line), when the checkout has one
    let web = std::env::var("TARGET").is_ok_and(|t| t.starts_with("wasm32"));
    let list = root.join("web.txt");
    println!("cargo::rerun-if-changed={}", list.display());
    if web && let Ok(text) = std::fs::read_to_string(&list) {
        let wanted: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        folders.retain(|f| f.file_name().and_then(|n| n.to_str()).is_some_and(|n| wanted.contains(&n)));
    }
    let mut out = String::new();
    for f in folders.iter().take(MAX_FONTS) {
        let name = f.file_name().and_then(|n| n.to_str()).ok_or_else(|| format!("{}: not a plain name", f.display()))?;
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(format!("{name}: a font folder's name is letters, digits, - and _"));
        }
        let licence = ["OFL.txt", "LICENSE", "LICENSE.txt"].iter().map(|n| f.join(n)).find(|p| p.is_file());
        let Some(licence) = licence else { return Err(format!("{name}: no licence (OFL.txt) beside the font")) };
        let (json, glyphs) = (f.join("font.json"), f.join("glyphs.stitchfoundry"));
        for p in [&json, &glyphs, &licence] {
            let len = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?.len();
            if len > MAX_FILE {
                return Err(format!("{}: larger than {MAX_FILE} bytes", p.display()));
            }
            println!("cargo::rerun-if-changed={}", p.display());
        }
        let lit = |p: &Path| format!("{:?}", p.display().to_string());
        let _ = writeln!(
            out,
            "    BundledFont {{ folder: {name:?}, json: include_str!({}), glyphs: include_bytes!({}), licence: include_str!({}) }},",
            lit(&json),
            lit(&glyphs),
            lit(&licence)
        );
    }
    Ok(out)
}
