//! Multi-decoration (phase 4): sequins, beads, rhinestones, cord and boring are stitch types
//! (`embroidery.set {type: sequin|bead|stone|cord|boring}`, their settings by
//! `embroidery.setParams`); these commands give their standard sizes and count what a design uses.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use sf_core::decor::{DecorKind, palette};
use sf_core::model::ObjectKind;

use super::{CommandSpec, Result, Session, always, bad, cmd, has_doc};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.decor.palette",
            "Decoration Sizes",
            [],
            None,
            "{} the standard sizes: sequins (3–9 mm), rhinestones by their SS size (SS6 2 mm … SS30 6.4 mm), seed and bugle beads → {sizes: [{kind, name, size}]}",
            always,
            sizes
        ),
        cmd!(
            query "embroidery.decor.count",
            "Count Decorations",
            [],
            None,
            "{} the sequins, beads and rhinestones the design uses, by kind, size and colour (to order supplies), and the cord's length → {pieces: [{kind, size, color, count}], total, cordMm}",
            has_doc,
            count
        ),
    ]
}

fn sizes(_: &mut Session, _: &Value) -> Result<Value> {
    let v: Vec<Value> = palette().into_iter().map(|(k, n, s)| json!({ "kind": k.id(), "name": n, "size": s })).collect();
    Ok(json!({ "sizes": v }))
}

fn count(s: &mut Session, _: &Value) -> Result<Value> {
    let plan = s.embroidery_plan().ok_or_else(|| bad("embroidery.decor.count", "no document"))?;
    let mut counts: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    let mut cord = 0.0;
    for obj in &plan.design.objects {
        let ObjectKind::Decor(d) = &obj.kind else { continue };
        let color = d.params.color.or_else(|| plan.design.palette.get(obj.color).map(|t| t.color)).map(|c| c.to_hex()).unwrap_or_default();
        match d.params.kind {
            DecorKind::Cord => {
                for (line, closed) in &d.lines {
                    cord += line.windows(2).filter_map(|w| Some(w.first()?.distance(*w.get(1)?))).sum::<f64>();
                    if *closed && let (Some(a), Some(b)) = (line.first(), line.last()) {
                        cord += a.distance(*b);
                    }
                }
            }
            DecorKind::Boring | DecorKind::Print | DecorKind::Foil => {}
            k => {
                for it in &d.items {
                    *counts.entry((k.id().to_string(), format!("{:.1}", it.size), color.clone())).or_default() += 1;
                }
            }
        }
    }
    let total: usize = counts.values().sum();
    let pieces: Vec<Value> = counts
        .into_iter()
        .map(|((kind, size, color), n)| json!({ "kind": kind, "size": size.parse::<f64>().unwrap_or(0.0), "color": color, "count": n }))
        .collect();
    Ok(json!({ "pieces": pieces, "total": total, "cordMm": (cord * 10.0).round() / 10.0 }))
}
