//! Recolouring the design (CW-06): its threads changed colour for colour, by a map or a scheme
//! (grey, inverted, all in one hue, the hue turned), matched to a thread chart if asked, the art
//! repainted to match; one undo step.

use serde_json::{Value, json};
use sf_core::Rgb;
use sf_core::attr::StitchType;
use sf_core::threads::Thread;
use vectorcraft_color::Color;
use vectorcraft_doc::{NodeId, NodeKind};

use super::{CommandSpec, EngineError, Result, Session, bad, bool_or, cmd, has_doc, str_param};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.recolor",
        "Recolour Threads",
        [],
        None,
        "{map?: [{from: \"#rrggbb\", to: \"#rrggbb\"}] (each thread of that colour), scheme?: grayscale|invert|mono|hue|lighten|darken|tint (instead of a map: grey by lightness, inverted, every thread in `color`'s hue at its own lightness, the hue turned `degrees`, every thread lighter or darker by `amount`, or mixed `amount` toward `color`), color?: \"#rrggbb\" (mono, tint), degrees?: -360–360 (hue), amount?: % 1–100 (lighten, darken, tint: 20), chart?: a thread chart to match the new colours to, ids?, all?: bool (the selection's stitched objects; all of them with `all` or nothing selected), paint?: bool (true: repaint the art)} recolour the design's threads, one undo step → {changed, colors: [{from, to}]}",
        has_doc,
        recolor
    )]
}

/// `c` as hue (0–360), saturation and lightness (0–1).
fn to_hsl(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (f64::from(c.r) / 255.0, f64::from(c.g) / 255.0, f64::from(c.b) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-9 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-9);
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s.min(1.0), l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let v = |t: f64| ((t + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb::new(v(r), v(g), v(b))
}

/// The colour a scheme gives `c`; `amount` (0–1) is how far lighten, darken and tint go.
fn scheme(name: &str, c: Rgb, tone: Rgb, degrees: f64, amount: f64) -> Rgb {
    match name {
        // ColorIT-style: lightness towards white or black, or the colour mixed towards `tone`
        "lighten" => {
            let (h, s, l) = to_hsl(c);
            from_hsl(h, s, l + (1.0 - l) * amount)
        }
        "darken" => {
            let (h, s, l) = to_hsl(c);
            from_hsl(h, s, l * (1.0 - amount))
        }
        "tint" => c.mix(tone, amount),
        "grayscale" => {
            let y = (0.2126 * f64::from(c.r) + 0.7152 * f64::from(c.g) + 0.0722 * f64::from(c.b)).round().clamp(0.0, 255.0) as u8;
            Rgb::new(y, y, y)
        }
        "invert" => Rgb::new(255 - c.r, 255 - c.g, 255 - c.b),
        "mono" => {
            let (h, s, _) = to_hsl(tone);
            let (_, _, l) = to_hsl(c);
            from_hsl(h, s, l)
        }
        _ => {
            let (h, s, l) = to_hsl(c);
            from_hsl(h + degrees, s, l)
        }
    }
}

fn recolor(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.recolor";
    let hex = |v: &Value, what: &str| v.as_str().and_then(Rgb::from_hex).ok_or_else(|| bad(CMD, format!("`{what}` is \"#rrggbb\"")));
    let map: Vec<(Rgb, Rgb)> = match p.get("map") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a.iter().take(256).map(|m| Ok((hex(&m["from"], "map.from")?, hex(&m["to"], "map.to")?))).collect::<Result<_>>()?,
        Some(_) => return Err(bad(CMD, "`map` is a list of {from, to}")),
    };
    let scheme_name = str_param(p, "scheme");
    if let Some(n) = scheme_name
        && !["grayscale", "invert", "mono", "hue", "lighten", "darken", "tint"].contains(&n)
    {
        return Err(bad(CMD, "`scheme` is grayscale, invert, mono, hue, lighten, darken or tint"));
    }
    if map.is_empty() && scheme_name.is_none() {
        return Err(bad(CMD, "give a `map` of colours or a `scheme`"));
    }
    let tone = match (scheme_name, p.get("color")) {
        (Some("mono" | "tint"), Some(v)) => hex(v, "color")?,
        (Some(name @ ("mono" | "tint")), None) => return Err(bad(CMD, format!("`{name}` needs a `color`"))),
        _ => Rgb::new(0, 0, 0),
    };
    let degrees = match p.get("degrees") {
        None | Some(Value::Null) => 180.0,
        Some(v) => v.as_f64().filter(|d| (-360.0..=360.0).contains(d)).ok_or_else(|| bad(CMD, "`degrees` is -360 to 360"))?,
    };
    let amount = match p.get("amount") {
        None | Some(Value::Null) => 0.2,
        Some(v) => v.as_f64().filter(|a| (1.0..=100.0).contains(a)).ok_or_else(|| bad(CMD, "`amount` is 1 to 100 (%)"))? / 100.0,
    };
    let chart = match str_param(p, "chart") {
        None => None,
        Some(name) => Some(super::embroidery_charts::find_chart(s, name).ok_or_else(|| bad(CMD, format!("unknown thread chart `{name}`")))?),
    };
    let repaint = bool_or(p, "paint", true);
    let everything = bool_or(p, "all", false) || (p.get("ids").is_none() && s.doc()?.selection.objects.is_empty());
    let ids: Vec<NodeId> = if everything {
        vectorcraft_embroidery::stitched_nodes(&s.doc()?.doc).iter().map(|n| n.id).collect()
    } else {
        super::embroidery::stitched_targets(s, p)?
    };
    // each object's thread and the one it becomes
    let mut colors: Vec<(Rgb, Rgb)> = Vec::new();
    let plans: Vec<(NodeId, Thread)> = {
        let d = &s.doc()?.doc;
        ids.iter()
            .filter_map(|id| {
                let n = d.node(*id)?;
                let e = n.embroidery.as_deref()?;
                if matches!(e.stitch, StitchType::Branch) || matches!(n.kind, NodeKind::Group { .. } | NodeKind::Layer { .. }) {
                    return None;
                }
                let now = vectorcraft_embroidery::node_thread(n, e);
                let to = match map.iter().find(|(from, _)| *from == now.color) {
                    Some((_, to)) => *to,
                    None => match scheme_name {
                        Some(name) if map.is_empty() => scheme(name, now.color, tone, degrees, amount),
                        _ => return None,
                    },
                };
                let thread = match &chart {
                    Some(c) => c.threads.get(c.nearest(to)).cloned().unwrap_or_else(|| Thread::new(to, "")),
                    None => Thread::new(to, ""),
                };
                if thread.color == now.color && thread.name == now.name {
                    return None;
                }
                if !colors.iter().any(|(f, _)| *f == now.color) {
                    colors.push((now.color, thread.color));
                }
                Some((*id, thread))
            })
            .collect()
    };
    if plans.is_empty() {
        return Err(bad(CMD, "no thread of the design changes"));
    }
    let new = super::embroidery::new_stitching(s);
    s.edit("Recolour Threads", |d, _| {
        for (id, t) in &plans {
            let c = Color::rgb8(t.color.r, t.color.g, t.color.b);
            super::embroidery::put_thread(d.node_mut(*id).ok_or(EngineError::NoNode(*id))?, &new, Some(t.clone()), repaint.then_some(c));
        }
        Ok(())
    })?;
    let colors: Vec<Value> = colors.iter().map(|(f, t)| json!({ "from": f.to_hex(), "to": t.to_hex() })).collect();
    Ok(json!({ "changed": plans.len(), "colors": colors }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_goes_there_and_back() {
        for c in [Rgb::new(0xd6, 0x28, 0x28), Rgb::new(0x26, 0x46, 0x53), Rgb::new(0, 0, 0), Rgb::new(255, 255, 255), Rgb::new(0x80, 0x80, 0x80)] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            assert!((i32::from(back.r) - i32::from(c.r)).abs() <= 1 && (i32::from(back.g) - i32::from(c.g)).abs() <= 1, "{c:?} → {back:?}");
        }
        assert_eq!(scheme("hue", Rgb::new(255, 0, 0), Rgb::new(0, 0, 0), 120.0, 0.0), Rgb::new(0, 255, 0));
        assert_eq!(scheme("invert", Rgb::new(255, 0, 10), Rgb::new(0, 0, 0), 0.0, 0.0), Rgb::new(0, 255, 245));
    }

    /// PU-10 (Pulse's ColorIT): lighter, darker, or mixed towards a colour, by an amount.
    #[test]
    fn lighten_darken_and_tint() {
        let red = Rgb::new(200, 40, 40);
        let k = Rgb::new(0, 0, 0);
        let (_, _, l) = to_hsl(red);
        let (_, _, up) = to_hsl(scheme("lighten", red, k, 0.0, 0.5));
        let (_, _, down) = to_hsl(scheme("darken", red, k, 0.0, 0.5));
        assert!((up - (l + (1.0 - l) * 0.5)).abs() < 0.01 && (down - l * 0.5).abs() < 0.01, "{l} {up} {down}");
        assert_eq!(scheme("lighten", red, k, 0.0, 1.0), Rgb::new(255, 255, 255));
        assert_eq!(scheme("darken", red, k, 0.0, 1.0), Rgb::new(0, 0, 0));
        assert_eq!(scheme("tint", red, Rgb::new(0, 0, 200), 0.0, 0.5), red.mix(Rgb::new(0, 0, 200), 0.5));
        let mut s = Session::new();
        s.execute("embroidery.newDesign", &json!({})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 50, "y": 50, "width": 100, "height": 60})).unwrap();
        s.execute("embroidery.set", &json!({"type": "fill"})).unwrap();
        s.execute("embroidery.setThread", &json!({"color": "#c82828"})).unwrap();
        let r = s.execute("embroidery.recolor", &json!({"scheme": "lighten", "amount": 40, "all": true})).unwrap();
        assert_eq!(r["changed"], 1, "{r}");
        assert!(s.execute("embroidery.recolor", &json!({"scheme": "tint"})).is_err(), "tint needs a colour");
        assert!(s.execute("embroidery.recolor", &json!({"scheme": "darken", "amount": 0})).is_err());
    }
}
