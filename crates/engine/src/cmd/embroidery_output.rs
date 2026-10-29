//! Batch output (OUT-02, NEW-10): a design's machine files in several formats, its worksheet as a
//! PDF and the design itself, written to a folder in one step, as asked or by an output profile
//! kept by name.

use serde_json::{Value, json};

use super::embroidery::{MACHINE_EXTS, TEXT_EXTS};
use super::{CommandSpec, EngineError, Result, Session, always, bad, bool_or, cmd, has_doc, str_param};

/// Output profiles kept at most.
const MAX_PROFILES: usize = 50;
/// The worksheets a batch can print.
const WORKSHEETS: [&str; 5] = ["production", "approval", "pattern", "template", "none"];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.output",
            "Batch Output",
            [],
            None,
            "{dir?: the folder to write to (the profile's), profile?: an output profile's name (any case), formats?: [dst|exp|jef|pes|pec|vp3|xxx|u01|pcs|tap|dsb…] (the profile's, else the design's machine, else dst), worksheet?: production|approval|pattern|template|none (the profile's, else production), design?: bool (the profile's, else true: the design itself, .stitchfoundry), name?: what the files are called (the design's), replace?: bool (false: a number is added to all of them when any name is taken)} batch output: the design's machine files, its worksheet as a PDF (`name-worksheet.pdf`) and the design, written to a folder in one step; the documents open stay as they are → {dir, name, files: [{path, kind: machine|worksheet|design, format, bytes}], stitches}",
            has_doc,
            output
        ),
        cmd!(
            query "embroidery.output.profiles",
            "Output Profiles",
            [],
            None,
            "{} the output profiles kept (embroidery.output.saveProfile) → {profiles: [{name, formats, worksheet, design, dir?}]}",
            always,
            profiles
        ),
        cmd!(
            "embroidery.output.saveProfile",
            "Save Output Profile",
            [],
            None,
            "{name, formats?: [ext…] (empty: the design's machine format when used), worksheet?: production|approval|pattern|template|none (production), design?: bool (true), dir?: the folder it writes to} an output profile kept by name for embroidery.output, replacing one of that name; 50 at most → {name}",
            always,
            save_profile
        ),
        cmd!(
            "embroidery.output.deleteProfile",
            "Delete Output Profile",
            [],
            None,
            "{name} forget an output profile → {name}",
            always,
            delete_profile
        ),
    ]
}

/// What a batch writes, kept by name (`embroidery.output.saveProfile`).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OutputProfile {
    pub name: String,
    /// Machine formats by extension; empty: the design's machine's.
    #[serde(default)]
    pub formats: Vec<String>,
    /// The worksheet's kind, or `none`.
    pub worksheet: String,
    /// Whether the design itself is written too.
    pub design: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

impl OutputProfile {
    fn json(&self) -> Value {
        json!({ "name": self.name, "formats": self.formats, "worksheet": self.worksheet, "design": self.design, "dir": self.dir })
    }
}

/// `formats` as machine extensions, checked, each once (none when not given).
fn formats_param(p: &Value, cmd: &str) -> Result<Option<Vec<String>>> {
    let list: Vec<&Value> = match p.get("formats") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(_)) => p.get("formats").into_iter().collect(),
        Some(Value::Array(a)) => a.iter().collect(),
        Some(_) => return Err(bad(cmd, "`formats` is a list of machine formats")),
    };
    let mut out: Vec<String> = Vec::new();
    for v in list {
        let ext = v.as_str().map(|f| f.trim().trim_start_matches('.').to_ascii_lowercase()).unwrap_or_default();
        if !MACHINE_EXTS.contains(&ext.as_str()) && !TEXT_EXTS.contains(&ext.as_str()) {
            return Err(bad(cmd, format!("unknown format `{ext}`: one of {}, {}", MACHINE_EXTS.join(", "), TEXT_EXTS.join(", "))));
        }
        if !out.contains(&ext) {
            out.push(ext);
        }
    }
    Ok(Some(out))
}

fn worksheet_param(p: &Value, cmd: &str) -> Result<Option<String>> {
    match str_param(p, "worksheet") {
        None => Ok(None),
        Some(w) if WORKSHEETS.contains(&w) => Ok(Some(w.to_string())),
        Some(_) => Err(bad(cmd, "`worksheet` is production, approval, pattern, template or none")),
    }
}

fn find_profile<'a>(s: &'a Session, name: &str) -> Option<&'a OutputProfile> {
    s.prefs.embroidery_output_profiles.iter().find(|o| o.name.eq_ignore_ascii_case(name.trim()))
}

fn output(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.output";
    let profile = match str_param(p, "profile") {
        None => None,
        Some(name) => Some(find_profile(s, name).cloned().ok_or_else(|| bad(CMD, format!("no output profile is named `{name}`")))?),
    };
    // what is asked wins over the profile, the profile over the defaults
    let formats = match formats_param(p, CMD)? {
        Some(f) => f,
        None => match profile.as_ref().map(|o| o.formats.clone()).filter(|f| !f.is_empty()) {
            Some(f) => f,
            // the design's machine reads this format
            None => {
                let machine = s.doc()?.doc.embroidery.as_deref().and_then(|e| e.machine_file_format(&s.prefs.embroidery_machines));
                vec![machine.unwrap_or_else(|| "dst".to_string())]
            }
        },
    };
    let worksheet = worksheet_param(p, CMD)?.or_else(|| profile.as_ref().map(|o| o.worksheet.clone())).unwrap_or_else(|| "production".into());
    let design = match p.get("design") {
        None | Some(Value::Null) => profile.as_ref().is_none_or(|o| o.design),
        Some(v) => v.as_bool().ok_or_else(|| bad(CMD, "`design` is true or false"))?,
    };
    if formats.is_empty() && worksheet == "none" && !design {
        return Err(bad(CMD, "nothing to write: ask for a format, a worksheet or the design"));
    }
    let dir = match str_param(p, "dir").map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => d.to_string(),
        None => profile.as_ref().and_then(|o| o.dir.clone()).ok_or_else(|| bad(CMD, "missing `dir`: the folder to write to"))?,
    };
    if !super::embroidery::folder_ready(&dir) {
        return Err(bad(CMD, format!("{dir} isn't there")));
    }
    let stem = match str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => super::fileio::file_stem(&s.doc()?.doc.title),
    };
    let stem = super::embroidery_lettering::safe_name(&stem);
    // every file is named alike: a number added to all of them when any name is taken
    let base = std::path::Path::new(&dir);
    let mut names: Vec<(String, &str, String)> = formats.iter().map(|f| (format!("{{}}.{f}"), "machine", f.clone())).collect();
    if design {
        names.push((format!("{{}}.{}", vectorcraft_format::SAVE_EXTENSION), "design", vectorcraft_format::SAVE_EXTENSION.to_string()));
    }
    if worksheet != "none" {
        names.push(("{}-worksheet.pdf".to_string(), "worksheet", "pdf".to_string()));
    }
    let path_of = |pattern: &str, stem: &str| base.join(pattern.replace("{}", stem)).to_string_lossy().to_string();
    let mut name = stem.clone();
    if !bool_or(p, "replace", false) {
        let mut k = 2;
        while names.iter().any(|(pat, ..)| super::embroidery::file_there(std::path::Path::new(&path_of(pat, &name)))) && k < 1000 {
            name = format!("{stem}-{k}");
            k += 1;
        }
    }
    let mut files = Vec::new();
    let mut stitches = Value::Null;
    for (pat, kind, format) in &names {
        let path = path_of(pat, &name);
        let bytes = match *kind {
            "machine" => {
                let out = super::embroidery::export_machine_file(s, &json!({ "format": format, "name": name, "center": true }))?;
                stitches = out["stitches"].clone();
                let bytes = out["dataBase64"]
                    .as_str()
                    .and_then(vectorcraft_format::base64_decode)
                    .ok_or_else(|| EngineError::Other("the machine file didn't encode".into()))?;
                super::fileio::write_file(&path, &bytes)?;
                json!(bytes.len())
            }
            "design" => s.execute("document.export", &json!({ "path": path, "format": "vectorcraft" }))?["bytes"].clone(),
            _ => {
                // the worksheet opens as a document of its own: exported, then closed again
                let back = s.active_index();
                s.execute("embroidery.worksheet", &json!({ "kind": worksheet }))?;
                let sheet = s.active_index();
                let r = s.execute("document.export", &json!({ "path": path, "format": "pdf" }));
                if let Some(i) = sheet
                    && sheet != back
                {
                    s.close_document(i);
                }
                if let Some(b) = back {
                    s.set_active(b);
                }
                r?["bytes"].clone()
            }
        };
        files.push(json!({ "path": path, "kind": kind, "format": format, "bytes": bytes }));
    }
    Ok(json!({ "dir": super::fileio::absolute_path(&dir), "name": name, "files": files, "stitches": stitches }))
}

fn profiles(s: &mut Session, _: &Value) -> Result<Value> {
    let list: Vec<Value> = s.prefs.embroidery_output_profiles.iter().map(OutputProfile::json).collect();
    Ok(json!({ "profiles": list }))
}

fn save_profile(s: &mut Session, p: &Value) -> Result<Value> {
    const SAVE: &str = "embroidery.output.saveProfile";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(SAVE, "missing `name`"))?;
    let name: String = name.chars().take(80).collect();
    let design = match p.get("design") {
        None | Some(Value::Null) => true,
        Some(v) => v.as_bool().ok_or_else(|| bad(SAVE, "`design` is true or false"))?,
    };
    let t = OutputProfile {
        name: name.clone(),
        formats: formats_param(p, SAVE)?.unwrap_or_default(),
        worksheet: worksheet_param(p, SAVE)?.unwrap_or_else(|| "production".into()),
        design,
        dir: str_param(p, "dir").map(str::trim).filter(|d| !d.is_empty()).map(str::to_string),
    };
    let own = &mut s.prefs.embroidery_output_profiles;
    let full = own.len() >= MAX_PROFILES;
    match own.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&name)) {
        Some(o) => *o = t,
        None if full => return Err(bad(SAVE, format!("{MAX_PROFILES} output profiles at most: delete one first"))),
        None => own.push(t),
    }
    Ok(json!({ "name": name }))
}

fn delete_profile(s: &mut Session, p: &Value) -> Result<Value> {
    const DELETE: &str = "embroidery.output.deleteProfile";
    let name = str_param(p, "name").map(str::trim).ok_or_else(|| bad(DELETE, "missing `name`"))?.to_string();
    let own = &mut s.prefs.embroidery_output_profiles;
    let before = own.len();
    own.retain(|t| !t.name.eq_ignore_ascii_case(&name));
    if own.len() == before {
        return Err(bad(DELETE, format!("no output profile is named `{name}`")));
    }
    Ok(json!({ "name": name }))
}
