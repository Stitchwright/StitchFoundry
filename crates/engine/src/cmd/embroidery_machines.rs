//! Machine profiles (`embroidery.machine.*`): the standard ones, one per file format, as the
//! user changed them, and the user's own. They are kept with the preferences, not in a design:
//! a design names the machine it is for (`embroidery.settings {machine}`).

use serde_json::{Value, json};
use sf_core::machines::{self, MachineAdvanced, MachineProfile};
use sf_core::model::TieStyle;

use super::{CommandSpec, Result, Session, always, bad, cmd, str_param};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.machine.list",
            "Machine Profiles",
            [],
            None,
            "{} the machine profiles: the standard ones, one per file format and named by it (dst, exp, u01, pes, jef, vp3, xxx), as you changed them, then your own → {machines: [{name, label, format, kind: commercial|home, standard, edited (a standard one changed), maxStitch, maxJump (mm), speed (stitches per minute), trimSeconds, advanced?: {minStitch, trimThreshold, tieIn, tieOff, tieLength, tieStyle, colorChangeSeconds}}]}",
            always,
            list
        ),
        cmd!(
            "embroidery.machine.save",
            "Save Machine Profile",
            [],
            None,
            "{name, newName?, format?, fromDesign?, maxStitch?, maxJump?, speed?, trimSeconds?, advanced?: {minStitch?, trimThreshold?, tieIn?, tieOff?, tieLength?, tieStyle?, colorChangeSeconds?} | null} change a machine profile, or make one of your own: a new name with `format` (dst|exp|u01|pes|jef|vp3|xxx; it starts from that format's profile). fromDesign: the values of the design's machine settings first. A standard profile keeps its name, format and kind, and is as it ships again once its values are; the design using a profile follows its new name and format → {name, format, standard, edited, created}",
            always,
            save
        ),
        cmd!(
            "embroidery.machine.delete",
            "Delete Machine Profile",
            [],
            None,
            "{name} delete one of your machine profiles, or put a standard one back to how it ships (designs keep their settings) → {name, reset}",
            always,
            delete
        ),
    ]
}

/// A profile as the commands show it.
pub(crate) fn machine_json(m: &MachineProfile, saved: &[MachineProfile]) -> Value {
    json!({
        "name": m.name, "label": m.label(), "format": m.format, "kind": m.kind.id(),
        "standard": m.is_standard(), "edited": machines::is_edited(saved, &m.name),
        "maxStitch": m.max_stitch, "maxJump": m.max_jump, "speed": m.speed, "trimSeconds": m.trim_seconds,
        "advanced": m.advanced,
    })
}

fn list(s: &mut Session, _p: &Value) -> Result<Value> {
    let saved = &s.prefs.embroidery_machines;
    Ok(json!({ "machines": machines::profiles(saved).iter().map(|m| machine_json(m, saved)).collect::<Vec<_>>() }))
}

fn formats_list() -> String {
    machines::formats().collect::<Vec<_>>().join(", ")
}

/// Whether the name `n` belongs to a profile other than the one at `own` in `all`.
fn taken(all: &[MachineProfile], n: &str, own: Option<&str>) -> bool {
    all.iter().any(|m| m.name.eq_ignore_ascii_case(n.trim()) && own.is_none_or(|o| !m.name.eq_ignore_ascii_case(o)))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.machine.save";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?.to_string();
    let saved = s.prefs.embroidery_machines.clone();
    let all = machines::profiles(&saved);
    let existing = machines::find(&saved, &name);
    let format = str_param(p, "format").map(str::trim);
    if let Some(f) = format
        && machines::standard(f).is_none()
    {
        return Err(bad(CMD, format!("unknown format `{f}`: one of {}", formats_list())));
    }
    let mut m = match &existing {
        Some(m) if m.is_standard() => {
            if format.is_some_and(|f| !f.eq_ignore_ascii_case(&m.format)) {
                return Err(bad(CMD, format!("`{}` is the standard profile for {} files and keeps its format", m.name, m.format.to_uppercase())));
            }
            m.clone()
        }
        Some(m) => {
            let mut m = m.clone();
            if let Some(f) = format.and_then(machines::standard) {
                (m.format, m.kind) = (f.format, f.kind);
            }
            m
        }
        // one of your own: it starts from the profile of its format
        None => {
            let f =
                format.ok_or_else(|| bad(CMD, format!("no machine profile is named `{name}`: give a `format` to make one ({})", formats_list())))?;
            let base = machines::find(&saved, f).ok_or_else(|| bad(CMD, format!("unknown format `{f}`")))?;
            MachineProfile { name: name.clone(), ..base }
        }
    };
    if p.get("fromDesign").and_then(Value::as_bool) == Some(true) {
        let settings = s.doc()?.doc.embroidery.as_deref().map(|e| e.settings.clone()).unwrap_or_default();
        m = m.with_settings(&settings);
    }
    let num = |key: &str, slot: &mut f64, min: f64, max: f64, from: &Value| -> Result<()> {
        if let Some(v) = from.get(key) {
            *slot = v.as_f64().filter(|x| (min..=max).contains(x)).ok_or_else(|| bad(CMD, format!("`{key}` is a number from {min} to {max}")))?;
        }
        Ok(())
    };
    num("maxStitch", &mut m.max_stitch, 1.0, 50.0, p)?;
    num("maxJump", &mut m.max_jump, 1.0, 50.0, p)?;
    num("speed", &mut m.speed, 50.0, 3000.0, p)?;
    num("trimSeconds", &mut m.trim_seconds, 0.0, 60.0, p)?;
    match p.get("advanced") {
        None => {}
        Some(Value::Null) => m.advanced = None,
        Some(a @ Value::Object(_)) => {
            let mut adv: MachineAdvanced = m.advanced.take().unwrap_or_default();
            num("minStitch", &mut adv.min_stitch, 0.0, 5.0, a)?;
            num("trimThreshold", &mut adv.trim_threshold, 0.0, 50.0, a)?;
            num("tieLength", &mut adv.tie_length, 0.1, 5.0, a)?;
            num("colorChangeSeconds", &mut adv.color_change_seconds, 0.0, 600.0, a)?;
            for (key, slot) in [("tieIn", &mut adv.tie_in), ("tieOff", &mut adv.tie_off)] {
                if let Some(v) = a.get(key) {
                    *slot = v.as_bool().ok_or_else(|| bad(CMD, format!("`advanced.{key}` is true or false")))?;
                }
            }
            if let Some(v) = a.get("tieStyle") {
                let id = v.as_str().unwrap_or_default();
                adv.tie_style =
                    TieStyle::ALL.into_iter().find(|t| t.id() == id).ok_or_else(|| bad(CMD, "`advanced.tieStyle` is single, double or triangle"))?;
            }
            m.advanced = Some(adv);
        }
        Some(_) => {
            return Err(bad(CMD, "`advanced` is {minStitch?, trimThreshold?, tieIn?, tieOff?, tieLength?, tieStyle?, colorChangeSeconds?} or null"));
        }
    }
    let old_name = m.name.clone();
    if let Some(n) = str_param(p, "newName").map(str::trim).filter(|n| *n != old_name) {
        if m.is_standard() {
            return Err(bad(CMD, format!("`{old_name}` is a standard profile and keeps its name: save it as a new one instead")));
        }
        if n.is_empty() {
            return Err(bad(CMD, "`newName` is empty"));
        }
        if machines::standard(n).is_some() || taken(&all, n, Some(&old_name)) {
            return Err(bad(CMD, format!("a machine profile named `{n}` exists")));
        }
        m.name = n.to_string();
    }
    // keep it: a standard profile as it ships needs no copy
    let list = &mut s.prefs.embroidery_machines;
    let at = list.iter().position(|x| x.name.eq_ignore_ascii_case(&old_name));
    let shipped = machines::standard(&m.name).is_some_and(|std| std == m);
    match (at, shipped) {
        (Some(i), true) => {
            list.remove(i);
        }
        (Some(i), false) => list[i] = m.clone(),
        (None, true) => {}
        (None, false) => list.push(m.clone()),
    }
    // the design using it follows its new name and format
    let cfg = s.doc().ok().and_then(|d| d.doc.embroidery.as_deref().cloned());
    if let Some(cfg) = cfg
        && cfg.machine.eq_ignore_ascii_case(&old_name)
        && !m.is_standard()
        && (cfg.machine != m.name || cfg.machine_format != m.format)
    {
        let (name, format) = (m.name.clone(), m.format.clone());
        s.edit("Embroidery Settings", |d, _| {
            let cfg = d.embroidery.get_or_insert_with(Box::default);
            (cfg.machine, cfg.machine_format) = (name, format);
            Ok(())
        })?;
    }
    let edited = machines::is_edited(&s.prefs.embroidery_machines, &m.name);
    Ok(json!({ "name": m.name, "format": m.format, "standard": m.is_standard(), "edited": edited, "created": existing.is_none() }))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.machine.delete";
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `name`"))?.to_string();
    let standard = machines::standard(&name).is_some();
    let list = &mut s.prefs.embroidery_machines;
    let Some(i) = list.iter().position(|m| m.name.eq_ignore_ascii_case(&name)) else {
        return Err(bad(
            CMD,
            if standard {
                format!("`{name}` is a standard profile as it ships: nothing to reset")
            } else {
                format!("no machine profile is named `{name}`")
            },
        ));
    };
    let gone = list.remove(i);
    Ok(json!({ "name": if standard { name.to_lowercase() } else { gone.name }, "reset": standard }))
}
