//! Mistakes made safe (E6): Start Over empties the design in one step Undo takes back, and Keep
//! Safe saves a beginner's design on its own — in place when it is a design file of ours, else as
//! a new design in the My Designs folder — so closing it or a crash never loses it.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, bad, cmd, fileio, has_doc, str_param};

/// The extension Keep Safe saves with.
const EXT: &str = "stitchfoundry";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.startOver",
            "Start Over",
            [],
            None,
            "{} remove everything in the design (its hoop and settings stay), one step Undo takes back → {removed}",
            has_doc,
            start_over
        ),
        cmd!(
            "file.keepSafe",
            "Keep Design Safe",
            [],
            None,
            "{folder?: the folder for new designs (the myDesignsFolder preference when not given; the app gives My Designs: Documents/StitchFoundry Designs)} save the design if it has changes: in place when it was opened from or saved as a StitchFoundry design, else as a new design in the folder (\"My design 1.stitchfoundry\", the next free number, or the opened file's name) that it then keeps → {saved: path | null}",
            has_doc,
            keep_safe
        ),
    ]
}

fn start_over(s: &mut Session, _p: &Value) -> Result<Value> {
    let n = {
        let d = &s.doc()?.doc;
        let mut n = 0usize;
        d.walk(|node| n += usize::from(!matches!(node.kind, vectorcraft_doc::NodeKind::Layer { .. })));
        n
    };
    if n == 0 {
        return Ok(json!({ "removed": 0 }));
    }
    s.execute("select.all", &json!({}))?;
    s.execute("edit.clear", &json!({}))?;
    Ok(json!({ "removed": n }))
}

/// The My Designs folder: the preference, else Documents/StitchFoundry Designs in the home folder.
pub fn designs_folder(s: &Session) -> Option<std::path::PathBuf> {
    let pref = s.prefs.my_designs_folder.trim();
    if !pref.is_empty() {
        return Some(pref.into());
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::Path::new(&home).join("Documents").join("StitchFoundry Designs"))
}

fn keep_safe(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "file.keepSafe";
    let (dirty, path, format, asks) = {
        let st = s.doc()?;
        (st.is_dirty(), st.path.clone(), st.format, st.converted || st.recovered)
    };
    if !dirty {
        return Ok(json!({ "saved": Value::Null }));
    }
    // a design file of ours: saved where it is
    if let Some(path) = path.as_deref().filter(|_| format == "vectorcraft" && !asks) {
        s.execute("document.save", &json!({}))?;
        return Ok(json!({ "saved": path }));
    }
    // the folder is given (the app gives My Designs) or set as a preference: never guessed here,
    // so a call with no folder writes nowhere it wasn't asked to
    let folder = match str_param(p, "folder") {
        Some(f) => std::path::PathBuf::from(f),
        None if !s.prefs.my_designs_folder.trim().is_empty() => std::path::PathBuf::from(s.prefs.my_designs_folder.trim()),
        None => return Err(bad(CMD, "missing `folder`: where to keep new designs (My Designs)")),
    };
    std::fs::create_dir_all(&folder).map_err(|e| bad(CMD, format!("can't make the folder {}: {e}", folder.display())))?;
    // an opened machine file keeps its name; a new design is My design N
    let stem = path.as_deref().map(|p| fileio::file_stem(p.rsplit(['/', '\\']).next().unwrap_or(p)));
    let target = match stem.filter(|s| !s.trim().is_empty()) {
        Some(stem) => free_name(&folder, &stem, false),
        None => free_name(&folder, "My design", true),
    }
    .ok_or_else(|| bad(CMD, "too many designs with that name in the folder"))?;
    let target = target.to_string_lossy().to_string();
    s.execute("file.saveAs", &json!({ "path": target, "format": "vectorcraft" }))?;
    Ok(json!({ "saved": target }))
}

/// `<stem>.stitchfoundry` (numbered: `<stem> 1`, `<stem> 2`… when `numbered`, or once taken).
fn free_name(folder: &std::path::Path, stem: &str, numbered: bool) -> Option<std::path::PathBuf> {
    if !numbered {
        let p = folder.join(format!("{stem}.{EXT}"));
        if !p.exists() {
            return Some(p);
        }
    }
    (1..10_000).map(|i| folder.join(format!("{stem} {i}.{EXT}"))).find(|p| !p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// E6: Start Over empties the design and Undo brings it all back.
    #[test]
    fn start_over_empties_the_design_and_undo_brings_it_back() {
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Husqvarna 240 × 150"})).unwrap();
        s.execute("embroidery.addWords", &json!({"text": "Ann"})).unwrap();
        s.execute("embroidery.addShape", &json!({"name": "Star"})).unwrap();
        let before = s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap();
        assert!(before > 100);
        let r = s.execute("embroidery.startOver", &json!({})).unwrap();
        assert!(r["removed"].as_u64().unwrap() >= 2, "{r}");
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"], 0);
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.info", &json!({})).unwrap()["stitches"].as_u64().unwrap(), before);
        // nothing to remove: nothing happens
        s.execute("embroidery.startOver", &json!({})).unwrap();
        assert_eq!(s.execute("embroidery.startOver", &json!({})).unwrap()["removed"], 0);
    }

    /// E6: Keep Safe saves a new design in the folder, then in place; an unchanged design isn't
    /// saved again.
    #[test]
    fn keep_safe_saves_new_designs_in_the_folder_then_in_place() {
        let dir = std::env::temp_dir().join(format!("sf-keep-safe-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let folder = dir.to_string_lossy().to_string();
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({"hoop": "Husqvarna 240 × 150"})).unwrap();
        s.execute("embroidery.addWords", &json!({"text": "Bo"})).unwrap();
        let r = s.execute("file.keepSafe", &json!({"folder": folder})).unwrap();
        let first = r["saved"].as_str().unwrap().to_string();
        assert!(first.ends_with("My design 1.stitchfoundry"), "{r}");
        assert!(std::fs::metadata(&first).unwrap().len() > 100);
        // unchanged: not saved again
        assert!(s.execute("file.keepSafe", &json!({"folder": folder})).unwrap()["saved"].is_null());
        // changed: saved in place
        s.execute("embroidery.addShape", &json!({"name": "Heart"})).unwrap();
        assert_eq!(s.execute("file.keepSafe", &json!({"folder": folder})).unwrap()["saved"], first.as_str());
        // a second new design takes the next number
        s.execute("embroidery.newDesign", &json!({"hoop": "Husqvarna 240 × 150"})).unwrap();
        s.execute("embroidery.addShape", &json!({"name": "Sun"})).unwrap();
        let second = s.execute("file.keepSafe", &json!({"folder": folder})).unwrap();
        assert!(second["saved"].as_str().unwrap().ends_with("My design 2.stitchfoundry"), "{second}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
