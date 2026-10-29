//! Memory cards (`embroidery.memoryCards`, `embroidery.memoryCard.save`): older machines read
//! designs from a memory card written by a card writer box (the Magic, Amazing or Ultimate Box)
//! and its own software, which takes a machine file. Choosing the card picks the file format the
//! card's machines read and checks the design against what the card holds.
//!
//! What the cards take is from the public Embird pages on the Ultimate Box
//! (<https://www.embird.net/sw/embird/tutorial/wubox/wubox.htm>,
//! <https://embird.net/manual/0575ultimatebox.htm>): "up to 230,000 stitches to the Amazing/Magic
//! and Brother 512 kilobyte (4 megabit) blank cards", "over 450,000 stitches to the Ultimate
//! Card", the blank cards fitting "Brother, Babylock, Bernina Deco, Simplicity, White, and
//! Viking/Husqvarna" machines, and the two-slot box also taking "the cards for Pfaff and Bernina
//! Artista" machines. StitchFoundry doesn't talk to the box itself: how it talks to a computer is
//! not published.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, cmd, has_doc, str_param};
use crate::EngineError;

const CMD: &str = "embroidery.memoryCard.save";

/// A kind of memory card: id, name, the machines that read it, the format, the stitches it holds
/// (`None`: not published).
struct Card {
    id: &'static str,
    name: &'static str,
    machines: &'static str,
    format: &'static str,
    stitches: Option<u64>,
}

const BROTHER_MACHINES: &str = "Brother, Baby Lock, Bernina Deco, Simplicity, White, Viking/Husqvarna (Brother-made)";

const CARDS: &[Card] = &[
    Card {
        id: "brother-512k",
        name: "Brother 512 KB blank card (Magic, Amazing)",
        machines: BROTHER_MACHINES,
        format: "pes",
        stitches: Some(230_000),
    },
    Card { id: "ultimate", name: "Ultimate Card", machines: BROTHER_MACHINES, format: "pes", stitches: Some(450_000) },
    Card { id: "pfaff", name: "Pfaff card (two-slot box)", machines: "Pfaff", format: "pcs", stitches: None },
];

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "embroidery.memoryCards",
            "Memory Cards",
            [],
            None,
            "{} the memory cards a card writer box writes, with the machines that read them, the file format and the stitches a card holds → {cards: [{id, name, machines, format, stitches?}]}",
            always,
            cards
        ),
        cmd!(
            query "embroidery.memoryCard.save",
            "Save for a Memory Card",
            [],
            None,
            "{card: an id from embroidery.memoryCards, path?: the file to write (without it: the bytes), name?: the design's label} the design as the file the card's machines read, checked against what the card holds, for the card writer's own software to put on the card → {format, bytes, stitches, share: of the card (0–1, when known), warnings, path? | dataBase64}",
            has_doc,
            save
        ),
    ]
}

fn cards(_: &mut Session, _: &Value) -> Result<Value> {
    let list: Vec<Value> = CARDS
        .iter()
        .map(|c| {
            let mut v = json!({ "id": c.id, "name": c.name, "machines": c.machines, "format": c.format });
            if let Some(n) = c.stitches {
                v["stitches"] = json!(n);
            }
            v
        })
        .collect();
    Ok(json!({ "cards": list }))
}

fn save(s: &mut Session, p: &Value) -> Result<Value> {
    let id = str_param(p, "card").ok_or_else(|| bad(CMD, "missing `card`: see embroidery.memoryCards"))?;
    let card = CARDS.iter().find(|c| c.id == id).ok_or_else(|| bad(CMD, format!("no card `{id}`: see embroidery.memoryCards")))?;
    let mut q = json!({ "format": card.format, "center": true });
    if let Some(n) = str_param(p, "name") {
        q["name"] = json!(n);
    }
    let out = super::embroidery::export_machine_file(s, &q)?;
    let stitches = out["stitches"].as_u64().unwrap_or(0);
    if let Some(cap) = card.stitches
        && stitches > cap
    {
        return Err(bad(CMD, format!("the design has {stitches} stitches, more than the {cap} a {} holds", card.name)));
    }
    let mut r = json!({ "format": card.format, "bytes": out["bytes"], "stitches": stitches, "warnings": out["warnings"] });
    if let Some(cap) = card.stitches.filter(|c| *c > 0) {
        r["share"] = json!(stitches as f64 / cap as f64);
    }
    match str_param(p, "path").map(str::trim).filter(|x| !x.is_empty()) {
        Some(path) => {
            let bytes = out["dataBase64"]
                .as_str()
                .and_then(vectorcraft_format::base64_decode)
                .ok_or_else(|| EngineError::Other("the machine file didn't encode".into()))?;
            super::fileio::write_file(path, &bytes)?;
            r["path"] = json!(path);
        }
        None => r["dataBase64"] = out["dataBase64"].clone(),
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_picks_its_format_and_checks_what_it_holds() {
        let mut s = Session::new();
        let list = s.execute("embroidery.memoryCards", &json!({})).unwrap();
        assert_eq!(list["cards"].as_array().unwrap().len(), CARDS.len());
        s.execute("embroidery.sample", &json!({"id": "heart-patch"})).unwrap();
        let r = s.execute("embroidery.memoryCard.save", &json!({"card": "brother-512k"})).unwrap();
        assert_eq!(r["format"], "pes");
        let bytes = vectorcraft_format::base64_decode(r["dataBase64"].as_str().unwrap()).unwrap();
        assert!(bytes.starts_with(b"#PES"), "a PES file");
        let share = r["share"].as_f64().unwrap();
        assert!(share > 0.0 && share < 0.2, "{share}");
        assert_eq!(s.execute("embroidery.memoryCard.save", &json!({"card": "pfaff"})).unwrap()["format"], "pcs");
        let dir = std::env::temp_dir().join(format!("sf-card-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("heart.pes");
        let r = s.execute("embroidery.memoryCard.save", &json!({"card": "ultimate", "path": path.to_string_lossy()})).unwrap();
        assert_eq!(std::fs::read(&path).unwrap().len() as u64, r["bytes"].as_u64().unwrap());
        assert!(r.get("dataBase64").is_none());
        let _ = std::fs::remove_dir_all(&dir);
        for bad in [json!({}), json!({"card": "nope"})] {
            assert!(s.execute("embroidery.memoryCard.save", &bad).is_err(), "{bad}");
        }
    }
}
