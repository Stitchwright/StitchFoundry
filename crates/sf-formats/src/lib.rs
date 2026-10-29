//! # StitchFoundry machine embroidery file formats
//!
//! Readers and writers for the files embroidery machines sew from. Every format
//! has a reader ([`read`]) and all but ZSK DSZ and Pfaff VIP a writer ([`write()`]); [`detect`] sniffs a file's
//! format from its bytes and [`read_any`] combines detection with reading.
//!
//! | Format | Machines | Colours | Native commands |
//! |---|---|---|---|
//! | [`Format::Dst`] | Tajima and most commercial machines | no | stitch, jump, colour change/stop, end (trims are a jump convention) |
//! | [`Format::Exp`] | Melco / Bernina | no | stitch, jump, trim, colour change |
//! | [`Format::Jef`] | Janome / Elna / Kenmore | Janome palette | stitch, jump, colour change, end (trims are a zero-length jump) |
//! | [`Format::Pes`] | Brother / Babylock / Deco (PES v1 wrapping a PEC block) | Brother palette | stitch, jump, trim, colour change, end |
//! | [`Format::Pec`] | Brother (bare PEC block) | Brother palette | stitch, jump, trim, colour change, end |
//! | [`Format::Vp3`] | Pfaff / Husqvarna Viking | RGB + thread names | stitch, jump, trim, one block per colour |
//! | [`Format::Xxx`] | Singer | RGB | stitch, jump, trim, colour change, end |
//! | [`Format::U01`] | Barudan | no | stitch, jump, trim, stop, needle change, end |
//! | [`Format::Pcs`] | Pfaff (experimental) | RGB, 16 at most | stitch, move, colour change (trims are a move where the needle is) |
//! | [`Format::Hus`] | Husqvarna Viking (experimental) | Husqvarna palette | stitch, jump, trim, colour change, end |
//! | [`Format::Sew`] | older Janome / Elna / Kenmore (experimental) | Janome palette | as JEF |
//! | [`Format::Vip`] | older Pfaff (read only) | RGB | as HUS |
//! | [`Format::Tap`] | Happy | no | as DST |
//! | [`Format::Dsb`] | Barudan (a DST header, U01's records) | no | as U01 |
//! | [`Format::Dsz`] | ZSK (read only) | no | stitch, jump, trim, stop, needle change |
//!
//! ## Coordinates and preparation
//!
//! A [`Pattern`] holds absolute positions in millimetres with +y **down**.
//! Machine files store relative moves, mostly in 0.1 mm and mostly with +y
//! **up**: writers scale and flip, readers flip back, so a write → read round
//! trip is the identity up to the file's resolution. Positions are rounded
//! absolutely (every move is the difference of two rounded absolute
//! positions), so long designs never drift.
//!
//! Every writer first runs the shared preparation ([`prepare`] shows its
//! result): leading trims and colour changes are dropped, redundant commands are
//! merged (a trim or stop with nothing sewn since the last one, empty colour
//! blocks, a stop right before a colour change), the design is centred on the
//! file origin ([`WriteOptions::center`]), a jump from the origin to the first
//! stitch is added, stitches and jumps longer than the format allows are split
//! into equal steps, stops become colour changes (repeating the thread) in
//! formats without a stop command, and exactly one end record closes the design.
//! [`normalize`] is the format-independent part of that step.
//!
//! ## Robustness
//!
//! Readers parse untrusted bytes: they never panic, cap what they allocate and
//! return a [`FormatError`] for anything they cannot make sense of.
//!
//! The byte layouts were written from the publicly known descriptions of these
//! formats. Each format's module (`src/dst.rs`, `src/pec.rs`, …) documents its
//! layout; where a byte's meaning is uncertain it says so, the writer uses the
//! most common value and the reader ignores it.

#![forbid(unsafe_code)]
mod bytes;
mod dsb;
mod dst;
mod dsz;
mod exp;
mod hus;
mod jef;
mod pcs;
mod pec;
mod pes;
mod plan;
mod sew;
mod tap;
mod u01;
mod vip;
mod vp3;
mod xxx;

pub use plan::normalize;

use sf_core::Pattern;

/// A machine embroidery file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    /// Tajima `.dst`: 512-byte header, 3-byte balanced-ternary records
    /// (±12.1 mm). A trim is written as three small jumps that cancel out, and
    /// any run of three or more jumps reads back as a trim (the writer adds a
    /// trim before such runs, as machines cut there anyway).
    Dst,
    /// Melco expanded `.exp`: 2-byte records (±12.7 mm) with `0x80` escapes;
    /// trims are `80 80 07 00`.
    Exp,
    /// Janome `.jef`: header with the Janome palette indices, 2-byte records
    /// (±12.7 mm) with `0x80` escapes; a trim is a zero-length move.
    Jef,
    /// Brother `.pes`, written as version 1 wrapping a PEC block; trims are
    /// the trim flag of the PEC long form. Read: any version.
    Pes,
    /// Brother `.pec`: `#PEC0001` and a PEC block (palette indices, stitches,
    /// 48×38 thumbnails).
    Pec,
    /// Pfaff / Husqvarna Viking `.vp3`: one block per colour with RGB and
    /// thread names, positions in µm, 2-byte stitches (±12.7 mm); trims are
    /// `80 03`.
    Vp3,
    /// Singer `.xxx`: 2-byte records (±12.4 mm) with `0x7F` escapes and an RGB
    /// colour table; trims are `7F 03`.
    Xxx,
    /// Barudan `.u01`: 3-byte records (±12.7 mm) with trim, stop and needle
    /// change functions.
    U01,
    /// Husqvarna Viking `.hus` (experimental): a palette-indexed header and three compressed
    /// streams (commands, dx, dy) of one byte a record.
    Hus,
    /// Janome `.sew` (experimental), older Janome, Elna and Kenmore machines: a palette-indexed
    /// colour table, then JEF's records from 0x1D78.
    Sew,
    /// Pfaff `.vip` (read only): HUS's layout, another signature and a scrambled RGB colour table.
    Vip,
    /// Pfaff `.pcs` (experimental): an RGB table of up to 16 colours, then 9-byte records at
    /// absolute positions in 1/6 mm; a trim is a move to where the needle is.
    Pcs,
    /// Happy `.tap`: DST's records without its header.
    Tap,
    /// Barudan `.dsb`: a DST header, then U01's records.
    Dsb,
    /// ZSK `.dsz` (read only): a DST header, then 3-byte records (y, x, control) with trim,
    /// stop and needle change commands.
    Dsz,
}

impl Format {
    /// Every format StitchFoundry writes (and reads).
    pub const ALL: [Format; 13] = [
        Format::Dst,
        Format::Exp,
        Format::Jef,
        Format::Pes,
        Format::Pec,
        Format::Vp3,
        Format::Xxx,
        Format::U01,
        Format::Pcs,
        Format::Tap,
        Format::Dsb,
        Format::Hus,
        Format::Sew,
    ];

    /// The formats StitchFoundry reads but doesn't write.
    pub const READ_ONLY: [Format; 2] = [Format::Dsz, Format::Vip];

    /// Whether [`write()`] can write the format.
    pub fn writable(self) -> bool {
        !Format::READ_ONLY.contains(&self)
    }

    /// File extension, lower case without the dot (`"dst"`).
    pub fn extension(self) -> &'static str {
        match self {
            Format::Dst => "dst",
            Format::Exp => "exp",
            Format::Jef => "jef",
            Format::Pes => "pes",
            Format::Pec => "pec",
            Format::Vp3 => "vp3",
            Format::Xxx => "xxx",
            Format::U01 => "u01",
            Format::Pcs => "pcs",
            Format::Tap => "tap",
            Format::Dsb => "dsb",
            Format::Dsz => "dsz",
            Format::Hus => "hus",
            Format::Sew => "sew",
            Format::Vip => "vip",
        }
    }

    /// Human readable name (`"Tajima DST"`).
    pub fn name(self) -> &'static str {
        match self {
            Format::Dst => "Tajima DST",
            Format::Exp => "Melco EXP",
            Format::Jef => "Janome JEF",
            Format::Pes => "Brother PES",
            Format::Pec => "Brother PEC",
            Format::Vp3 => "Pfaff VP3",
            Format::Xxx => "Singer XXX",
            Format::U01 => "Barudan U01",
            Format::Pcs => "Pfaff PCS",
            Format::Tap => "Happy TAP",
            Format::Dsb => "Barudan DSB",
            Format::Dsz => "ZSK DSZ",
            Format::Hus => "Husqvarna HUS",
            Format::Sew => "Janome SEW",
            Format::Vip => "Pfaff VIP",
        }
    }

    /// The format for a file extension: case-insensitive, with or without the
    /// leading dot; a file name works too (`"logo.DST"`).
    pub fn from_extension(ext: &str) -> Option<Format> {
        let ext = ext.trim();
        let ext = ext.rsplit('.').next().unwrap_or(ext);
        Format::ALL.into_iter().chain(Format::READ_ONLY).find(|f| f.extension().eq_ignore_ascii_case(ext))
    }

    /// Whether the format stores thread colours. DST, EXP and U01 only store
    /// colour changes; their readers colour the blocks from a generic palette.
    pub fn has_colors(self) -> bool {
        !matches!(self, Format::Dst | Format::Exp | Format::U01 | Format::Tap | Format::Dsb | Format::Dsz)
    }

    /// Whether the format has a stop command distinct from a colour change.
    /// Elsewhere a [`sf_core::Cmd::Stop`] is written as a colour change to the
    /// same thread (and reads back as a colour change).
    pub fn supports_stop(self) -> bool {
        matches!(self, Format::U01 | Format::Dsb | Format::Dsz)
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A calendar date and time (stored by JEF files).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Timestamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Timestamp {
    /// The UTC date and time `secs` seconds after 1970-01-01.
    pub fn from_unix(secs: u64) -> Timestamp {
        let days = (secs / 86_400) as i64;
        let rem = secs % 86_400;
        // Days → civil date (proleptic Gregorian), counting eras of 400 years from 0000-03-01.
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        Timestamp {
            year: year.clamp(0, 9_999) as u16,
            month: month as u8,
            day: day as u8,
            hour: (rem / 3_600) as u8,
            minute: (rem / 60 % 60) as u8,
            second: (rem % 60) as u8,
        }
    }

    /// The current UTC date and time (a fixed date where the platform has no clock).
    pub fn now() -> Timestamp {
        #[cfg(not(target_arch = "wasm32"))]
        if let Ok(d) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            return Timestamp::from_unix(d.as_secs());
        }
        Timestamp { year: 2000, month: 1, day: 1, hour: 0, minute: 0, second: 0 }
    }
}

/// Options for [`write()`].
#[derive(Clone, Debug, PartialEq)]
pub struct WriteOptions {
    /// Design label stored by formats that have one (DST and PES/PEC `LA:`
    /// label). Empty: the pattern's own name.
    pub name: String,
    /// Move the design so the centre of its stitches sits on the file origin,
    /// which machines treat as the hoop centre / needle start (default). Off:
    /// the pattern's own origin is the file origin.
    pub center: bool,
    /// Creation time stored by JEF files; `None` stores the current time.
    pub timestamp: Option<Timestamp>,
    /// Keep every stop, also stops in a row (a chenille design's), instead of one.
    pub every_stop: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions { name: String::new(), center: true, timestamp: None, every_stop: false }
    }
}

impl WriteOptions {
    /// Options with a design label.
    pub fn named(name: impl Into<String>) -> Self {
        WriteOptions { name: name.into(), ..WriteOptions::default() }
    }

    /// The label to store: the option's name, else the pattern's.
    pub(crate) fn label<'a>(&'a self, pattern: &'a Pattern) -> &'a str {
        if self.name.is_empty() { &pattern.name } else { &self.name }
    }
}

/// Why a file could not be read or written.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The file ends in the middle of a structure.
    #[error("the file is truncated")]
    Truncated,
    /// The file does not start like a file of this format.
    #[error("not a {0} file")]
    BadMagic(Format),
    /// The file is damaged or uses something this reader does not understand.
    #[error("invalid embroidery file: {0}")]
    Invalid(String),
    /// There are no stitches (to write, or in the file).
    #[error("the design has no stitches")]
    Empty,
    /// The design exceeds the format's limits (size, record or colour count),
    /// or the file holds more records than a reader accepts.
    #[error("the design is too large for this file format")]
    TooLarge,
    /// [`read_any`] could not tell which format the bytes are.
    #[error("unrecognised embroidery file format")]
    Unknown,
    /// [`write()`] was asked for a format StitchFoundry only reads.
    #[error("{0} files can be opened but not written")]
    ReadOnly(Format),
}

/// Encodes `pattern` as a `format` file.
/// Open text exports of a design's stitches: CSV and JSON stitch lists, G-code.
pub mod textual;

pub fn write(format: Format, pattern: &Pattern, opts: &WriteOptions) -> Result<Vec<u8>, FormatError> {
    match format {
        Format::Dst => dst::write(pattern, opts),
        Format::Exp => exp::write(pattern, opts),
        Format::Jef => jef::write(pattern, opts),
        Format::Pes => pes::write(pattern, opts),
        Format::Pec => pec::write_file(pattern, opts),
        Format::Vp3 => vp3::write(pattern, opts),
        Format::Xxx => xxx::write(pattern, opts),
        Format::U01 => u01::write(pattern, opts),
        Format::Pcs => pcs::write(pattern, opts),
        Format::Tap => tap::write(pattern, opts),
        Format::Dsb => dsb::write(pattern, opts),
        Format::Hus => hus::write(pattern, opts),
        Format::Sew => sew::write(pattern, opts),
        Format::Dsz | Format::Vip => Err(FormatError::ReadOnly(format)),
    }
}

/// Decodes a `format` file.
pub fn read(format: Format, bytes: &[u8]) -> Result<Pattern, FormatError> {
    match format {
        Format::Dst => dst::read(bytes),
        Format::Exp => exp::read(bytes),
        Format::Jef => jef::read(bytes),
        Format::Pes => pes::read(bytes),
        Format::Pec => pec::read_file(bytes),
        Format::Vp3 => vp3::read(bytes),
        Format::Xxx => xxx::read(bytes),
        Format::U01 => u01::read(bytes),
        Format::Pcs => pcs::read(bytes),
        Format::Tap => tap::read(bytes),
        Format::Dsb => dsb::read(bytes),
        Format::Dsz => dsz::read(bytes),
        Format::Hus => hus::read(bytes),
        Format::Sew => sew::read(bytes),
        Format::Vip => vip::read(bytes),
    }
}

/// The pattern exactly as [`write()`] encodes it for `format`, in millimetres
/// with +y down: centred, split into the format's longest moves, rounded to the
/// file's resolution, with the format's command conventions applied (stops as
/// colour changes, DST trims before long jump runs) and the threads the file
/// will hold (palette colours for PES/PEC/JEF, a generic palette for formats
/// without colours) and the label the file stores. Reading the written file
/// gives this pattern back.
pub fn prepare(format: Format, pattern: &Pattern, opts: &WriteOptions) -> Result<Pattern, FormatError> {
    let (plan, cfg) = match format {
        Format::Dst => dst::plan(pattern, opts)?,
        Format::Exp => exp::plan(pattern, opts)?,
        Format::Jef => jef::plan(pattern, opts)?,
        Format::Pes | Format::Pec => pec::plan(pattern, opts)?,
        Format::Vp3 => vp3::plan(pattern, opts)?,
        Format::Xxx => xxx::plan(pattern, opts)?,
        Format::U01 => u01::plan(pattern, opts)?,
        Format::Pcs => pcs::plan(pattern, opts)?,
        Format::Tap => tap::plan(pattern, opts)?,
        Format::Dsb => dsb::plan(pattern, opts)?,
        Format::Hus => hus::plan(pattern, opts)?,
        Format::Sew => sew::plan(pattern, opts)?,
        Format::Dsz | Format::Vip => return Err(FormatError::ReadOnly(format)),
    };
    let mut out = plan.to_pattern(&cfg);
    // The label as the file stores it.
    out.name = match format {
        Format::Dst | Format::Dsb | Format::Pes | Format::Pec => bytes::ascii_label(opts.label(pattern), 16).trim_end().to_string(),
        Format::Exp
        | Format::Jef
        | Format::Vp3
        | Format::Xxx
        | Format::U01
        | Format::Pcs
        | Format::Tap
        | Format::Dsz
        | Format::Hus
        | Format::Sew
        | Format::Vip => String::new(),
    };
    Ok(out)
}

/// Guesses a file's format from its content: magic bytes for PES, PEC, VP3
/// and DST (a DST header followed by Barudan or ZSK records is DSB or DSZ), header sanity for JEF and XXX, the record structure for U01 and
/// EXP (the last resort: almost any even-length data is valid EXP).
pub fn detect(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"#PES") {
        Some(Format::Pes)
    } else if bytes.starts_with(b"#PEC") {
        Some(Format::Pec)
    } else if bytes.starts_with(b"%vsm%") {
        Some(Format::Vp3)
    } else if hus::sniff(bytes) {
        Some(Format::Hus)
    } else if vip::sniff(bytes) {
        Some(Format::Vip)
    } else if dsb::sniff(bytes) {
        Some(Format::Dsb)
    } else if dsz::sniff(bytes) {
        Some(Format::Dsz)
    } else if dst::sniff(bytes) {
        Some(Format::Dst)
    } else if jef::sniff(bytes) {
        Some(Format::Jef)
    } else if xxx::sniff(bytes) {
        Some(Format::Xxx)
    } else if pcs::sniff(bytes) {
        Some(Format::Pcs)
    } else if tap::sniff(bytes) {
        Some(Format::Tap)
    } else if u01::sniff(bytes) {
        Some(Format::U01)
    } else if sew::sniff(bytes) {
        Some(Format::Sew)
    } else if exp::sniff(bytes) {
        Some(Format::Exp)
    } else {
        None
    }
}

/// Reads a file of unknown format. A format recognised by its magic bytes
/// (PES, PEC, VP3, DST and the DST-headed DSB and DSZ) wins; otherwise the extension hint
/// (`"dst"`, `".PES"` or a file name) is tried first and, if it is missing, unknown or fails to
/// read, the format detected from the content. A `.dsb` or `.dsz` hint goes before a DST
/// header, since those files carry one over records of their own.
pub fn read_any(bytes: &[u8], ext_hint: Option<&str>) -> Result<(Format, Pattern), FormatError> {
    let detected = detect(bytes);
    let magic = detected.filter(|f| matches!(f, Format::Pes | Format::Pec | Format::Vp3 | Format::Dst | Format::Dsb | Format::Dsz));
    let hinted = ext_hint.and_then(Format::from_extension);
    let order = if matches!(hinted, Some(Format::Dsb | Format::Dsz)) { [hinted, magic, detected] } else { [magic, hinted, detected] };
    let mut tried: Vec<Format> = Vec::with_capacity(3);
    let mut first_err = None;
    for f in order.into_iter().flatten() {
        if tried.contains(&f) {
            continue;
        }
        tried.push(f);
        match read(f, bytes) {
            Ok(p) => return Ok((f, p)),
            Err(e) => {
                first_err.get_or_insert(e);
            }
        }
    }
    Err(first_err.unwrap_or(FormatError::Unknown))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::Cmd;

    #[test]
    fn extensions_round_trip() {
        for f in Format::ALL.into_iter().chain(Format::READ_ONLY) {
            assert_eq!(Format::from_extension(f.extension()), Some(f));
            assert_eq!(Format::from_extension(&format!(".{}", f.extension().to_uppercase())), Some(f));
            assert_eq!(Format::from_extension(&format!("design.{}", f.extension())), Some(f));
        }
        assert_eq!(Format::from_extension("svg"), None);
        assert_eq!(Format::from_extension(""), None);
    }

    /// D1: DSB carries a DST header over U01 records: it reads back as DSB, label included,
    /// both by content and by extension, and DST's own files stay DST.
    #[test]
    fn dsb_is_told_apart_from_dst() {
        let mut p = Pattern::new();
        p.name = "badge".into();
        p.threads = vec![sf_core::Thread::default(), sf_core::Thread::default()];
        for (x, y, cmd) in
            [(0.0, 0.0, Cmd::Stitch), (8.0, 0.0, Cmd::Stitch), (8.0, 0.0, Cmd::Trim), (8.0, 0.0, Cmd::ColorChange), (8.0, 6.0, Cmd::Stitch)]
        {
            p.stitches.push(sf_core::Stitch::new(x, y, cmd));
        }
        let dsb = write(Format::Dsb, &p, &WriteOptions::default()).unwrap();
        assert!(dsb.starts_with(b"LA:badge"));
        assert_eq!(detect(&dsb), Some(Format::Dsb));
        let (f, back) = read_any(&dsb, None).unwrap();
        assert_eq!((f, back.name.as_str()), (Format::Dsb, "badge"));
        assert!(back.stitches.iter().any(|s| s.cmd == Cmd::Trim) && back.stitches.iter().any(|s| s.cmd == Cmd::ColorChange));
        assert_eq!(read_any(&dsb, Some("badge.dsb")).unwrap().0, Format::Dsb);
        let dst = write(Format::Dst, &p, &WriteOptions::default()).unwrap();
        assert_eq!(detect(&dst), Some(Format::Dst));
        assert!(matches!(write(Format::Dsz, &p, &WriteOptions::default()), Err(FormatError::ReadOnly(Format::Dsz))));
        assert!(!Format::Dsz.writable() && Format::Dsb.writable());
    }

    #[test]
    fn unix_dates() {
        assert_eq!(Timestamp::from_unix(0), Timestamp { year: 1970, month: 1, day: 1, hour: 0, minute: 0, second: 0 });
        // 2000-02-29 12:34:56 (a leap day).
        assert_eq!(Timestamp::from_unix(951_827_696), Timestamp { year: 2000, month: 2, day: 29, hour: 12, minute: 34, second: 56 });
        // 2026-10-10 00:00:00.
        assert_eq!(Timestamp::from_unix(1_791_590_400), Timestamp { year: 2026, month: 10, day: 10, hour: 0, minute: 0, second: 0 });
    }
}
