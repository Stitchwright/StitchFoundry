//! Stitch properties beyond one object: the properties new objects take ("current
//! properties", the digitizing presets), styles (named property sets objects follow)
//! and fabrics made for the design.

use serde_json::{Map, Value, json};
use sf_core::attr::{DocEmbroidery, EmbStyle, Embroidery, StitchType};
use sf_core::fabrics::Fabric;
use vectorcraft_doc::{Document, NodeId};

use super::embroidery::{ids_json, patched, stitch_type, stitched_targets};
use super::{CommandSpec, Result, Session, always, bad, cmd, has_doc, has_selection, str_param};
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.current",
            "Current Properties",
            [],
            None,
            "{} the properties new objects take, by stitch type (the types `embroidery.set` takes): the design's own, and the defaults saved for every design (used where the design has none) → {current: {type: properties}, defaults: {type: properties}}",
            has_doc,
            current
        ),
        cmd!(
            "embroidery.makeCurrent",
            "Make Properties Current",
            [],
            None,
            "{ids?} new objects of the first selected object's stitch type take its stitch properties (not its thread, shape or direction lines) → {type}",
            has_selection,
            make_current
        ),
        cmd!(
            "embroidery.current.setParams",
            "New Object Properties",
            [],
            None,
            "{type, params} change the properties new objects of a stitch type take (camelCase names, mm, as embroidery.setParams); a run style change files them under that style's type → {type}",
            has_doc,
            current_set_params
        ),
        cmd!(
            "embroidery.current.reset",
            "Reset Current Properties",
            [],
            None,
            "{type?} new objects of a stitch type (else of every type) take the default properties again → {reset}",
            has_doc,
            current_reset
        ),
        cmd!(
            "embroidery.current.saveAsDefault",
            "Save as Default",
            [],
            None,
            "{type?} the design's Properties for New Objects (of one stitch type, else of every type set) become the defaults for every design, kept with the app's preferences → {saved: [type]}",
            has_doc,
            save_default
        ),
        cmd!(
            "embroidery.current.clearDefault",
            "Clear Default",
            [],
            None,
            "{type?} forget the defaults saved for every design (of one stitch type, else all): new objects take the built-in properties again → {cleared}",
            always,
            clear_default
        ),
        cmd!(
            query "embroidery.style.list",
            "Embroidery Styles",
            [],
            None,
            "{} the design's embroidery styles → {styles: [{name, type, uses}]}",
            has_doc,
            style_list
        ),
        cmd!(
            "embroidery.style.new",
            "New Embroidery Style",
            [],
            None,
            "{name?, ids?} a style holding the first selected stitched object's properties; that object follows it → {name}",
            has_selection,
            style_new
        ),
        cmd!(
            "embroidery.style.apply",
            "Apply Embroidery Style",
            [],
            None,
            "{name, ids?} the selected objects take the style's properties (keeping their thread, shape and direction lines) and follow it → {ids}",
            has_selection,
            style_apply
        ),
        cmd!(
            "embroidery.style.redefine",
            "Redefine Embroidery Style",
            [],
            None,
            "{name, ids?} the style takes the first selected stitched object's properties, and every object following it changes with it → {updated}",
            has_selection,
            style_redefine
        ),
        cmd!(
            "embroidery.style.unlink",
            "Break Link to Embroidery Style",
            [],
            None,
            "{ids?} the selected objects keep their properties but no longer follow a style → {ids}",
            has_selection,
            style_unlink
        ),
        cmd!("embroidery.style.duplicate", "Duplicate Embroidery Style", [], None, "{name} → {name}", has_doc, style_duplicate),
        cmd!("embroidery.style.rename", "Rename Embroidery Style", [], None, "{name, to} → {name}", has_doc, style_rename),
        cmd!(
            "embroidery.style.delete",
            "Delete Embroidery Style",
            [],
            None,
            "{name} objects following the style keep its properties but no longer follow it → {released}",
            has_doc,
            style_delete
        ),
        cmd!(
            "embroidery.fabric.save",
            "Save Fabric",
            [],
            None,
            "{name, description?, satinSpacing?, fillSpacing?, fillStitchLength?, pullComp?, pushComp?, fillUnderlay?, fillEdgeRun?, underlaySpacing?, satinUnderlay?: auto|center-run|edge-run|zigzag|edge-zigzag, garment?: bool|null (sewn on clothing: Check Design's long stitches from 7 mm, else 12.1 mm), topping?, stabilizer?: tear-away|cut-away|no-show-mesh|wash-away, from?} make or change a fabric of the design (mm); values not given come from the fabric `from`, else the one of that name, else the standard woven settings → {name}",
            has_doc,
            fabric_save
        ),
        cmd!("embroidery.fabric.delete", "Delete Fabric", [], None, "{name} delete a fabric made for the design", has_doc, fabric_delete),
    ]
}

fn config(s: &Session) -> Result<DocEmbroidery> {
    Ok(s.doc()?.doc.embroidery.as_deref().cloned().unwrap_or_default())
}

fn name_param<'a>(p: &'a Value, cmd: &str) -> Result<&'a str> {
    str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(cmd, "missing `name`"))
}

/// The first selected stitched object (groups: their paths) that has properties to
/// reuse, and its embroidery.
fn source(s: &Session, p: &Value, cmd: &str) -> Result<(NodeId, Embroidery)> {
    let ids = stitched_targets(s, p)?;
    let d = &s.doc()?.doc;
    ids.iter()
        .find_map(|id| {
            let e = d.node(*id)?.embroidery.as_deref()?;
            (!matches!(e.stitch, StitchType::Manual | StitchType::Branch | StitchType::Lettering(_))).then(|| (*id, e.clone()))
        })
        .ok_or_else(|| bad(cmd, "select an object stitched with a stitch type (not manual stitches)"))
}

/// Every node in the document that follows the style `name`.
fn followers(d: &Document, name: &str) -> Vec<NodeId> {
    let mut out = Vec::new();
    d.walk(|n| {
        if n.embroidery.as_deref().and_then(|e| e.style.as_deref()) == Some(name) {
            out.push(n.id);
        }
    });
    out
}

/// `base`, or `base 2`, `base 3`… : the first not taken by a style.
fn free_name(cfg: &DocEmbroidery, base: &str) -> String {
    if cfg.style(base).is_none() {
        return base.to_string();
    }
    (2..).map(|k| format!("{base} {k}")).find(|n| cfg.style(n).is_none()).unwrap_or_default()
}

fn current(s: &mut Session, _p: &Value) -> Result<Value> {
    let cfg = config(s)?;
    let map: Map<String, Value> = cfg.current.iter().map(|(k, e)| (k.clone(), serde_json::to_value(e).unwrap_or(Value::Null))).collect();
    let defaults: Map<String, Value> =
        s.prefs.embroidery_default_properties.iter().map(|(k, e)| (k.clone(), serde_json::to_value(e).unwrap_or(Value::Null))).collect();
    Ok(json!({ "current": map, "defaults": defaults }))
}

fn save_default(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.current.saveAsDefault";
    let key = str_param(p, "type").map(|t| typed(t, CMD).map(|st| st.type_name())).transpose()?;
    let cfg = config(s)?;
    let mut saved = Vec::new();
    for (k, e) in &cfg.current {
        if key.is_none_or(|want| want == k) && e.stitch.type_name() == k {
            s.prefs.embroidery_default_properties.insert(k.clone(), e.properties());
            saved.push(k.clone());
        }
    }
    if saved.is_empty() {
        return Err(bad(CMD, "set Properties for New Objects first (embroidery.current.setParams or Make Current)"));
    }
    Ok(json!({ "saved": saved }))
}

fn clear_default(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.current.clearDefault";
    let key = str_param(p, "type").map(|t| typed(t, CMD).map(|st| st.type_name())).transpose()?;
    let defaults = &mut s.prefs.embroidery_default_properties;
    let before = defaults.len();
    match key {
        Some(k) => {
            defaults.remove(k);
        }
        None => defaults.clear(),
    }
    Ok(json!({ "cleared": before - defaults.len() }))
}

fn make_current(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.makeCurrent";
    let (_, e) = source(s, p, CMD)?;
    let key = e.stitch.type_name();
    s.edit("Make Properties Current", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).current.insert(key.to_string(), e.properties());
        Ok(())
    })?;
    Ok(json!({ "type": key }))
}

/// The stitch type a type name (or alias) names, unless it has no properties.
fn typed(name: &str, cmd: &str) -> Result<StitchType> {
    match stitch_type(name) {
        Some(Some(st)) if !matches!(st, StitchType::Manual | StitchType::Branch | StitchType::Lettering(_)) => Ok(st),
        Some(_) => Err(bad(cmd, format!("`{name}` has no properties to set"))),
        None => Err(bad(cmd, format!("unknown stitch type `{name}`"))),
    }
}

fn current_set_params(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.current.setParams";
    let st = typed(str_param(p, "type").ok_or_else(|| bad(CMD, "missing `type`"))?, CMD)?;
    let patch = p.get("params").filter(|v| v.is_object()).ok_or_else(|| bad(CMD, "missing object `params`"))?;
    let key = st.type_name();
    let start = super::embroidery::current_props(s, key).unwrap_or_else(|| Embroidery::new(st));
    let e = patched(&start, patch).map_err(|m| bad(CMD, m))?.properties();
    let key = e.stitch.type_name();
    if matches!(e.stitch, StitchType::Manual | StitchType::Branch | StitchType::Lettering(_)) {
        return Err(bad(CMD, "the stitch type cannot change here"));
    }
    s.edit("New Object Properties", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).current.insert(key.to_string(), e);
        Ok(())
    })?;
    Ok(json!({ "type": key }))
}

fn current_reset(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.current.reset";
    let key = str_param(p, "type").map(|t| typed(t, CMD).map(|st| st.type_name())).transpose()?;
    let cfg = config(s)?;
    let reset = match key {
        Some(k) => usize::from(cfg.current.contains_key(k)),
        None => cfg.current.len(),
    };
    if reset > 0 {
        s.edit("Reset Current Properties", |d, _| {
            let cfg = d.embroidery.get_or_insert_with(Box::default);
            match key {
                Some(k) => {
                    cfg.current.remove(k);
                }
                None => cfg.current.clear(),
            }
            Ok(())
        })?;
    }
    Ok(json!({ "reset": reset }))
}

fn style_list(s: &mut Session, _p: &Value) -> Result<Value> {
    let d = &s.doc()?.doc;
    let cfg = d.embroidery.as_deref().cloned().unwrap_or_default();
    let styles: Vec<Value> = cfg
        .styles
        .iter()
        .map(|st| json!({ "name": st.name, "type": st.properties.stitch.type_name(), "uses": followers(d, &st.name).len() }))
        .collect();
    Ok(json!({ "styles": styles }))
}

fn style_new(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.new";
    let (id, e) = source(s, p, CMD)?;
    let cfg = config(s)?;
    let name = match str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) if cfg.style(n).is_some() => return Err(bad(CMD, format!("there is a style named `{n}` already"))),
        Some(n) => n.to_string(),
        None => free_name(&cfg, &format!("Style {}", cfg.styles.len() + 1)),
    };
    s.edit("New Embroidery Style", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).styles.push(EmbStyle { name: name.clone(), properties: e.properties() });
        if let Some(e) = d.node_mut(id).ok_or(EngineError::NoNode(id))?.embroidery.as_deref_mut() {
            e.style = Some(name.clone());
        }
        Ok(())
    })?;
    Ok(json!({ "name": name }))
}

/// The style named in the parameters.
fn style_param(s: &Session, p: &Value, cmd: &str) -> Result<EmbStyle> {
    let name = name_param(p, cmd)?;
    config(s)?.style(name).cloned().ok_or_else(|| bad(cmd, format!("no style named `{name}`")))
}

fn style_apply(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.apply";
    let style = style_param(s, p, CMD)?;
    let ids = stitched_targets(s, p)?;
    if ids.is_empty() {
        return Err(bad(CMD, "select a path, shape or type to stitch"));
    }
    s.edit("Apply Embroidery Style", |d, _| {
        for id in &ids {
            let n = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?;
            let e = n.embroidery.get_or_insert_with(|| Box::new(style.properties.clone()));
            e.take_properties(&style.properties);
            e.style = Some(style.name.clone());
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}

fn style_redefine(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.redefine";
    let style = style_param(s, p, CMD)?;
    let (src, e) = source(s, p, CMD)?;
    let props = e.properties();
    let mut ids = followers(&s.doc()?.doc, &style.name);
    if !ids.contains(&src) {
        ids.push(src);
    }
    s.edit("Redefine Embroidery Style", |d, _| {
        let cfg = d.embroidery.get_or_insert_with(Box::default);
        if let Some(st) = cfg.styles.iter_mut().find(|st| st.name == style.name) {
            st.properties = props.clone();
        }
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.take_properties(&props);
                e.style = Some(style.name.clone());
            }
        }
        Ok(())
    })?;
    Ok(json!({ "updated": ids.len() }))
}

fn style_unlink(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.unlink";
    let ids: Vec<NodeId> = {
        let d = &s.doc()?.doc;
        stitched_targets(s, p)?
            .into_iter()
            .filter(|id| d.node(*id).and_then(|n| n.embroidery.as_deref()).is_some_and(|e| e.style.is_some()))
            .collect()
    };
    if ids.is_empty() {
        return Err(bad(CMD, "the selection follows no style"));
    }
    s.edit("Break Link to Embroidery Style", |d, _| {
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.style = None;
            }
        }
        Ok(())
    })?;
    Ok(ids_json(&ids))
}

fn style_duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.duplicate";
    let style = style_param(s, p, CMD)?;
    let name = free_name(&config(s)?, &format!("{} copy", style.name));
    s.edit("Duplicate Embroidery Style", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).styles.push(EmbStyle { name: name.clone(), properties: style.properties.clone() });
        Ok(())
    })?;
    Ok(json!({ "name": name }))
}

fn style_rename(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.rename";
    let style = style_param(s, p, CMD)?;
    let to = str_param(p, "to").map(str::trim).filter(|n| !n.is_empty()).ok_or_else(|| bad(CMD, "missing `to`"))?.to_string();
    if to == style.name {
        return Ok(json!({ "name": to }));
    }
    if config(s)?.style(&to).is_some() {
        return Err(bad(CMD, format!("there is a style named `{to}` already")));
    }
    let ids = followers(&s.doc()?.doc, &style.name);
    s.edit("Rename Embroidery Style", |d, _| {
        if let Some(st) = d.embroidery.get_or_insert_with(Box::default).styles.iter_mut().find(|st| st.name == style.name) {
            st.name = to.clone();
        }
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.style = Some(to.clone());
            }
        }
        Ok(())
    })?;
    Ok(json!({ "name": to }))
}

fn style_delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.style.delete";
    let style = style_param(s, p, CMD)?;
    let ids = followers(&s.doc()?.doc, &style.name);
    s.edit("Delete Embroidery Style", |d, _| {
        d.embroidery.get_or_insert_with(Box::default).styles.retain(|st| st.name != style.name);
        for id in &ids {
            if let Some(e) = d.node_mut(*id).ok_or(EngineError::NoNode(*id))?.embroidery.as_deref_mut() {
                e.style = None;
            }
        }
        Ok(())
    })?;
    Ok(json!({ "released": ids.len() }))
}

/// A fabric's settings as the commands name them.
pub(crate) fn fabric_json(f: &Fabric, custom: bool) -> Value {
    json!({
        "name": f.name, "description": f.description, "custom": custom,
        "satinSpacing": f.satin_spacing, "fillSpacing": f.fill_spacing, "fillStitchLength": f.fill_stitch_length,
        "pullComp": f.pull_comp, "fillUnderlay": f.fill_underlay, "underlaySpacing": f.underlay_spacing, "topping": f.topping,
        "stabilizer": f.stabilizer.id(), "advice": sf_core::fabrics::stabilizer_advice(f),
        "pushComp": f.push_comp, "satinUnderlay": f.satin_underlay.id(), "fillEdgeRun": f.fill_edge_run, "garment": f.garment,
    })
}

fn fabric_save(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fabric.save";
    let name = name_param(p, CMD)?.to_string();
    if sf_core::fabrics::find(&name).is_some() {
        return Err(bad(CMD, format!("`{name}` is a built-in fabric: save the changes under another name")));
    }
    let cfg = config(s)?;
    let mut f = match str_param(p, "from") {
        Some(from) => cfg.fabric(from).ok_or_else(|| bad(CMD, format!("unknown fabric `{from}`")))?,
        None => cfg.fabric(&name).unwrap_or_default(),
    };
    f.name = name.clone();
    if let Some(d) = str_param(p, "description") {
        f.description = d.to_string();
    }
    let num = |key: &str, slot: &mut f64, min: f64, max: f64| -> Result<()> {
        if let Some(v) = p.get(key) {
            let x = v.as_f64().filter(|x| (min..=max).contains(x)).ok_or_else(|| bad(CMD, format!("`{key}` is a number from {min} to {max}")))?;
            *slot = x;
        }
        Ok(())
    };
    num("satinSpacing", &mut f.satin_spacing, 0.1, 3.0)?;
    num("fillSpacing", &mut f.fill_spacing, 0.1, 3.0)?;
    num("fillStitchLength", &mut f.fill_stitch_length, 0.5, 12.0)?;
    num("pullComp", &mut f.pull_comp, 0.0, 2.0)?;
    num("underlaySpacing", &mut f.underlay_spacing, 0.3, 10.0)?;
    num("pushComp", &mut f.push_comp, 0.0, 5.0)?;
    if let Some(v) = p.get("satinUnderlay") {
        f.satin_underlay = v
            .as_str()
            .and_then(sf_core::fabrics::SatinUnderlayRecipe::from_id)
            .ok_or_else(|| bad(CMD, "`satinUnderlay` is auto, center-run, edge-run, zigzag or edge-zigzag"))?;
    }
    match p.get("garment") {
        None => {}
        Some(Value::Null) => f.garment = None,
        Some(v) => f.garment = Some(v.as_bool().ok_or_else(|| bad(CMD, "`garment` is true, false or null"))?),
    }
    if let Some(v) = p.get("stabilizer") {
        f.stabilizer = v
            .as_str()
            .and_then(sf_core::fabrics::Stabilizer::from_id)
            .ok_or_else(|| bad(CMD, "`stabilizer` is tear-away, cut-away, no-show-mesh or wash-away"))?;
    }
    for (key, slot) in [("fillUnderlay", &mut f.fill_underlay), ("topping", &mut f.topping), ("fillEdgeRun", &mut f.fill_edge_run)] {
        if let Some(v) = p.get(key) {
            *slot = v.as_bool().ok_or_else(|| bad(CMD, format!("`{key}` is true or false")))?;
        }
    }
    s.edit("Save Fabric", |d, _| {
        let cfg = d.embroidery.get_or_insert_with(Box::default);
        match cfg.fabrics.iter_mut().find(|x| x.name == f.name) {
            Some(x) => *x = f.clone(),
            None => cfg.fabrics.push(f.clone()),
        }
        Ok(())
    })?;
    Ok(json!({ "name": name }))
}

fn fabric_delete(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.fabric.delete";
    let name = name_param(p, CMD)?.to_string();
    if !config(s)?.fabrics.iter().any(|f| f.name == name) {
        return Err(bad(CMD, format!("no fabric of the design is named `{name}`")));
    }
    s.edit("Delete Fabric", |d, _| {
        let cfg = d.embroidery.get_or_insert_with(Box::default);
        cfg.fabrics.retain(|f| f.name != name);
        if cfg.fabric == name {
            cfg.fabric.clear();
        }
        Ok(())
    })?;
    Ok(json!({ "name": name }))
}
