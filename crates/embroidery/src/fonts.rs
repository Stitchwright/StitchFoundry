//! Embroidery fonts in Ink/Stitch's font format: a folder with `font.json` (the font's name,
//! sizes, glyph widths and kerning) and SVG art whose layers, named `GlyphLayer-<glyph>`, hold
//! each glyph digitized with Ink/Stitch settings (sewn as [`crate::inkstitch`] reads them).
//! StitchFoundry ships none: users open font folders they have, and each font keeps its own
//! licence. The format comes from Ink/Stitch's public documentation
//! (https://inkstitch.org/docs/font-tools/) and from fonts' own data files, not its code.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use vectorcraft_doc::{Document, Node};

/// Most glyphs one font keeps, characters one piece of lettering sets, and characters a glyph
/// name has (ligatures such as "fi").
pub const MAX_GLYPHS: usize = 5_000;
/// How deep inside other layers glyph layers are looked for.
const MAX_LAYER_DEPTH: usize = 4;
pub const MAX_TEXT: usize = 2_000;
const MAX_NAME: usize = 8;

/// What `font.json` says that lettering needs (the rest is ignored).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct FontInfo {
    pub name: String,
    pub description: String,
    pub font_license: String,
    /// Line height, in font units.
    pub leading: f64,
    /// The height of the font's capitals as digitized (mm).
    pub size: f64,
    /// How far the font scales and still sews well.
    pub min_scale: f64,
    pub max_scale: f64,
    /// "upper" or "lower": text set in that case; else as typed.
    pub letter_case: String,
    /// What a character without a glyph shows.
    pub default_glyph: String,
    /// Advances, in font units: every glyph's (none: its own width), a space's, and glyphs' own.
    pub horiz_adv_x_default: Option<f64>,
    pub horiz_adv_x_space: Option<f64>,
    pub horiz_adv_x: HashMap<String, f64>,
    /// "A B" (or "AB"): how much closer the pair sits, in font units.
    pub kerning_pairs: HashMap<String, f64>,
}

impl FontInfo {
    pub fn parse(json: &str) -> Result<FontInfo, String> {
        let mut f: FontInfo = serde_json::from_str(json).map_err(|e| format!("font.json: {e}"))?;
        if f.name.trim().is_empty() {
            return Err("font.json names no font".into());
        }
        let finite = |v: f64, or: f64| if v.is_finite() { v } else { or };
        f.leading = finite(f.leading, 0.0).max(0.0);
        f.size = finite(f.size, 0.0).max(0.0);
        f.min_scale = finite(f.min_scale, 0.0).max(0.0);
        f.max_scale = finite(f.max_scale, 0.0).max(0.0);
        f.horiz_adv_x.retain(|_, v| v.is_finite());
        f.kerning_pairs.retain(|_, v| v.is_finite());
        Ok(f)
    }
}

/// One glyph's art: a layer of the font's art, its baseline (pt, from the top of its page) and
/// the size of a font unit (pt).
#[derive(Clone, Debug)]
pub struct Glyph {
    pub art: Arc<Node>,
    pub baseline: f64,
    pub unit: f64,
}

impl Glyph {
    /// How wide the glyph's art reaches from its origin (font units).
    fn width(&self) -> f64 {
        self.art.geometric_bounds().map_or(0.0, |b| b.x1.max(0.0) / self.unit.max(1e-9))
    }
}

/// An embroidery font ready to set text.
#[derive(Clone, Debug)]
pub struct EmbFont {
    pub info: FontInfo,
    pub glyphs: HashMap<String, Glyph>,
    /// The folder it was read from (none: given as text, or built in).
    pub folder: Option<String>,
    /// Built into the app ([`BUNDLED_FONTS`]): its licence's text.
    pub built_in: Option<&'static str>,
}

/// A font built into the app (FONT-01): its folder's files, embedded at build time from a
/// checkout of the StitchFoundry fonts repository (`build.rs`, `STITCHFOUNDRY_FONTS_DIR`).
#[derive(Debug)]
pub struct BundledFont {
    /// Its folder's name in that repository (`foundry-block`).
    pub folder: &'static str,
    pub json: &'static str,
    /// Its glyphs: a StitchFoundry document.
    pub glyphs: &'static [u8],
    /// Its licence's text (the SIL Open Font License), which travels with the font.
    pub licence: &'static str,
}

include!(concat!(env!("OUT_DIR"), "/bundled_fonts.rs"));

/// One glyph of set text: its name, how far along its line it starts and which line (font units).
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub glyph: String,
    pub x: f64,
    pub line: usize,
    /// The word it is in, counted through the text (a space or a line break starts the next).
    pub word: usize,
    /// How far the next glyph starts along (font units).
    pub advance: f64,
}

impl EmbFont {
    /// A font from its `font.json` and its art: SVG documents, each with the baseline of its
    /// glyphs (pt from the top of the page) and its font unit (pt).
    pub fn new(json: &str, art: Vec<(Document, f64, f64)>) -> Result<EmbFont, String> {
        let info = FontInfo::parse(json)?;
        let mut glyphs = HashMap::new();
        for (doc, baseline, unit) in art {
            if !(baseline.is_finite() && unit.is_finite() && unit > 0.0) {
                continue;
            }
            // the glyph layers wherever they are: at the top, or inside other layers
            let mut stack: Vec<(&Arc<Node>, usize)> = doc.layers.iter().map(|l| (l, 0)).collect();
            while let Some((l, depth)) = stack.pop() {
                match l.name.as_deref().and_then(|n| n.strip_prefix("GlyphLayer-")) {
                    Some(name) => {
                        if name.is_empty() || name.chars().count() > MAX_NAME || glyphs.len() >= MAX_GLYPHS {
                            continue;
                        }
                        glyphs.entry(name.to_string()).or_insert_with(|| Glyph { art: l.clone(), baseline, unit });
                    }
                    None if depth < MAX_LAYER_DEPTH => stack.extend(l.children().into_iter().flatten().map(|c| (c, depth + 1))),
                    None => {}
                }
            }
        }
        if glyphs.is_empty() {
            return Err(format!("{}: no glyph layers (GlyphLayer-…) in its art", info.name));
        }
        Ok(EmbFont { info, glyphs, folder: None, built_in: None })
    }

    /// The text as the font sets it: in its letter case.
    fn cased(&self, text: &str) -> String {
        match self.info.letter_case.as_str() {
            "upper" => text.to_uppercase(),
            "lower" => text.to_lowercase(),
            _ => text.to_string(),
        }
    }

    /// The width of a space (font units): the font's, else a third of its line height.
    fn space(&self) -> f64 {
        self.info
            .horiz_adv_x_space
            .or_else(|| self.info.horiz_adv_x.get(" ").copied())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(self.info.leading / 3.0)
    }

    /// How far glyph `name` moves the pen on (font units), when the font has it.
    pub fn advance_of(&self, name: &str) -> Option<f64> {
        self.glyphs.get(name).map(|g| self.advance(name, g))
    }

    /// How far glyph `name` moves the pen on (font units).
    fn advance(&self, name: &str, g: &Glyph) -> f64 {
        self.info.horiz_adv_x.get(name).copied().or(self.info.horiz_adv_x_default).unwrap_or_else(|| g.width())
    }

    fn kerning(&self, a: &str, b: &str) -> f64 {
        self.info.kerning_pairs.get(&format!("{a} {b}")).or_else(|| self.info.kerning_pairs.get(&format!("{a}{b}"))).copied().unwrap_or(0.0)
    }

    /// Where each glyph of `text` goes: lines broken at newlines, each glyph the longest name
    /// the text matches (ligatures first), kerned against the one before; characters the font
    /// has no glyph for take its default glyph, else a space.
    pub fn layout(&self, text: &str) -> Vec<Placed> {
        let text: Vec<char> = self.cased(text).chars().take(MAX_TEXT).collect();
        let mut out = Vec::new();
        let (mut x, mut line) = (0.0, 0);
        let mut prev: Option<String> = None;
        // the word the next glyph is in, and whether a space or line break came before it
        let (mut word, mut gap) = (0, false);
        let mut i = 0;
        while i < text.len() {
            let c = text.get(i).copied().unwrap_or(' ');
            if c.is_whitespace() {
                gap = true;
            }
            if c == '\n' {
                (x, line, prev) = (0.0, line + 1, None);
                i += 1;
                continue;
            }
            if gap && !c.is_whitespace() {
                gap = false;
                if !out.is_empty() {
                    word += 1;
                }
            }
            // the longest glyph name the text matches here
            let found = (1..=MAX_NAME.min(text.len() - i)).rev().find_map(|n| {
                let name: String = text.get(i..i + n)?.iter().collect();
                self.glyphs.get(&name).map(|g| (name, g, n))
            });
            match found {
                Some((name, g, n)) => {
                    if let Some(p) = &prev {
                        x -= self.kerning(p, &name);
                    }
                    let advance = self.advance(&name, g);
                    out.push(Placed { glyph: name.clone(), x, line, word, advance });
                    x += advance;
                    prev = Some(name);
                    i += n;
                }
                None => {
                    match self.glyphs.get(&self.info.default_glyph).filter(|_| !c.is_whitespace()) {
                        Some(g) => {
                            let advance = self.advance(&self.info.default_glyph, g);
                            out.push(Placed { glyph: self.info.default_glyph.clone(), x, line, word, advance });
                            x += advance;
                        }
                        None => x += self.space(),
                    }
                    prev = None;
                    i += 1;
                }
            }
        }
        out
    }
}
