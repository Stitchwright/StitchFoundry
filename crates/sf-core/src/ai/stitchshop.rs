//! Send to StitchShop (SHOP-01): the design sent to the shop that will sew it, through the
//! StitchHub inbox (the README's "StitchFoundry inbox" in Stitchwright/stitchhub; the plan is
//! `docs/stitchshop.md`). The requests carry the Foundry Account's session token, which the hub
//! checks with the account service and never keeps. The file and its preview go straight to the
//! hub's file store at the addresses it signs; the hub only sees their sizes and hashes. As
//! elsewhere in [`super`], no network code lives here.

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use super::{ArtError, HttpRequest, Method, cloud};

/// The hub's address unless another is set.
pub const DEFAULT_HUB: &str = "https://hub.stitchbench.app";
/// The hub's limits: a design file, and its preview picture (bytes).
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
/// The longest message from the hub shown (characters).
const MAX_MESSAGE: usize = 500;
/// The longest file name the hub takes (characters).
const MAX_NAME: usize = 120;

/// One of the account's shops.
#[derive(Clone, Debug, PartialEq)]
pub struct Shop {
    pub id: String,
    pub name: String,
    /// StitchShop or StitchBench.
    pub program: String,
}

/// A design to send: its machine file, a preview picture and what StitchFoundry knows about it.
#[derive(Clone, Debug, Default)]
pub struct Offer {
    /// As the hub takes it ([`file_name`]).
    pub file_name: String,
    pub file: Vec<u8>,
    /// A PNG of the stitches.
    pub preview: Option<Vec<u8>>,
    /// [`meta`].
    pub meta: Value,
    /// The shop it is for; none: any of the account's.
    pub to_shop: Option<String>,
}

/// Where the hub said to upload a file: its signed address and the headers to send with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
}

/// The hub's answer to an offer: the item's id and where its file (and preview) go.
#[derive(Clone, Debug, PartialEq)]
pub struct Slots {
    pub id: String,
    pub file: Slot,
    pub preview: Option<Slot>,
}

/// The address to send to: `typed` (the setting) when given, else the hub's own; HTTPS, or plain
/// HTTP on this computer for testing ([`cloud::clean_server`]).
pub fn hub(typed: &str) -> Result<String, ArtError> {
    let typed = typed.trim();
    cloud::clean_server(if typed.is_empty() { DEFAULT_HUB } else { typed }).ok_or(ArtError::NoStitchHub)
}

/// `name` made into a file name the hub takes: letters, digits, spaces and `. _ ( ) + -`, starting
/// with a letter or digit, 120 characters at most, keeping its extension.
pub fn file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) if !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()) => (s, Some(e)),
        _ => (base, None),
    };
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || " ._()+-".contains(c) {
                c
            } else if c.is_alphanumeric() {
                '_'
            } else {
                ' '
            }
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
    let room = MAX_NAME - ext.map_or(0, |e| e.len() + 1);
    let stem: String = cleaned.chars().take(room).collect();
    let stem = stem.trim_end();
    let stem = if stem.is_empty() { "Design" } else { stem };
    match ext {
        Some(e) => format!("{stem}.{e}"),
        None => stem.to_string(),
    }
}

/// The needle stitches in a DST file: its records with neither the jump nor the colour change bit
/// in their third byte, as StitchShop counts them (the header's `ST:` counts every record).
pub fn dst_stitches(file: &[u8]) -> Option<usize> {
    let body = file.get(512..)?;
    let mut n = 0usize;
    for r in body.chunks_exact(3) {
        let flags = r.get(2).copied().unwrap_or(0);
        if flags == 0xF3 {
            break;
        }
        if flags & 0xC0 == 0 {
            n += 1;
        }
    }
    Some(n)
}

/// What goes with the file, in the hub's keys (`docs/stitchshop.md` › What `meta` carries): from
/// `info` (`embroidery.info`'s answer), the job order `job` (`embroidery.jobs`' entry) when sent
/// from one, the format sent and its bytes `file`. Each value is held to what the hub keeps.
pub fn meta(info: &Value, job: Option<&Value>, design: &str, format: &str, file: &[u8], version: &str) -> Value {
    let mut m = Map::new();
    let text = |v: &Value, max: usize| -> Option<Value> {
        let s: String = v.as_str()?.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        (!s.is_empty()).then(|| Value::from(s.chars().take(max).collect::<String>()))
    };
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v {
            m.insert(k.to_string(), v);
        }
    };
    put("program", Some("StitchFoundry".into()));
    put("version", text(&Value::from(version), 120));
    put("design", text(&Value::from(design), 120));
    put("format", Some(format.to_ascii_lowercase().into()).filter(|f: &Value| f.as_str().is_some_and(|f| (2..=8).contains(&f.len()))));
    put("hoop", text(&info["hoop"], 120));
    put("fabric", text(&info["fabric"], 120));
    let stitches = if format.eq_ignore_ascii_case("dst") { dst_stitches(file) } else { None };
    put("stitches", stitches.map(Value::from).or_else(|| info["stitches"].as_u64().map(Value::from)));
    let threads: Vec<Value> =
        info["colors"].as_array().map(|c| c.iter().filter_map(|c| text(&c["name"], 60)).take(100).collect()).unwrap_or_default();
    if !threads.is_empty() {
        put("colors", Some(threads.len().into()));
        put("threads", Some(threads.into()));
    }
    let changes = info["colorChanges"].as_u64().unwrap_or(0).saturating_add(info["stops"].as_u64().unwrap_or(0));
    put("colorChanges", Some(changes.min(1000).into()));
    put("trims", info["trims"].as_u64().map(|t| t.min(100_000).into()));
    let tenth = |v: &Value| v.as_f64().filter(|v| v.is_finite() && *v >= 0.0).map(|v| Value::from((v * 10.0).round() / 10.0));
    put("widthMm", tenth(&info["widthMm"]));
    put("heightMm", tenth(&info["heightMm"]));
    put("threadM", tenth(&info["threadM"]));
    put("bobbinM", tenth(&info["bobbinM"]));
    put("runMinutes", info["seconds"].as_f64().filter(|s| s.is_finite() && *s >= 0.0).map(|s| Value::from((s / 60.0).ceil() as u64)));
    put("notes", text(&info["notes"], 500));
    if let Some(job) = job {
        put("job", text(&job["number"], 120));
        put("customer", text(&job["customer"], 120));
        put("garment", text(&job["garment"], 120));
        put("placement", text(&job["placement"], 120));
        put("due", text(&job["due"], 10));
        put("quantity", job["quantity"].as_u64().filter(|q| *q >= 1).map(Value::from));
        if let Some(n) = text(&job["notes"], 500) {
            put("notes", Some(n));
        }
    }
    Value::Object(m)
}

/// The SHA-256 of `bytes` in hex, as the hub takes it.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn signed_in(token: &str) -> Result<&str, ArtError> {
    let token = token.trim();
    if token.starts_with(cloud::TOKEN_PREFIX) { Ok(token) } else { Err(ArtError::SignIn) }
}

/// `GET /v1/inbox/shops`: the account's shops, to pick one.
pub fn shops(hub: &str, token: &str) -> Result<HttpRequest, ArtError> {
    let hub = cloud::clean_server(hub).ok_or(ArtError::NoStitchHub)?;
    let token = signed_in(token)?;
    Ok(HttpRequest {
        method: Method::Get,
        url: format!("{hub}/v1/inbox/shops"),
        headers: vec![("authorization", format!("Bearer {token}"))],
        ..Default::default()
    })
}

/// `POST /v1/inbox/put`: asks for the addresses to upload `offer`'s file and preview to; refused
/// here, without sending, when it is over the hub's limits.
pub fn put(hub: &str, token: &str, offer: &Offer) -> Result<HttpRequest, ArtError> {
    let hub = cloud::clean_server(hub).ok_or(ArtError::NoStitchHub)?;
    let token = signed_in(token)?;
    if offer.file.is_empty()
        || offer.file.len() > MAX_FILE_BYTES
        || offer.preview.as_ref().is_some_and(|p| p.is_empty() || p.len() > MAX_PREVIEW_BYTES)
    {
        return Err(ArtError::TooBigForStitchShop);
    }
    let mut body = json!({
        "fileName": file_name(&offer.file_name),
        "size": offer.file.len(),
        "sha256": sha256_hex(&offer.file),
        "meta": offer.meta,
    });
    if let Some(shop) = &offer.to_shop {
        body["toShopId"] = shop.clone().into();
    }
    if let Some(p) = &offer.preview {
        body["preview"] = json!({ "size": p.len(), "sha256": sha256_hex(p), "type": "image/png" });
    }
    cloud::request(&hub, "/v1/inbox/put", Some(token), &body)
}

/// The PUT of `bytes` to the address `slot` the hub signed.
pub fn upload(slot: &Slot, bytes: &[u8]) -> HttpRequest {
    HttpRequest { method: Method::Put, url: slot.url.clone(), headers: slot.headers.clone(), bytes: Some(bytes.to_vec()), ..Default::default() }
}

/// `POST /v1/inbox/done`: the file (and preview) are uploaded; only now does a shop see it.
pub fn done(hub: &str, token: &str, id: &str) -> Result<HttpRequest, ArtError> {
    let hub = cloud::clean_server(hub).ok_or(ArtError::NoStitchHub)?;
    cloud::request(&hub, "/v1/inbox/done", Some(signed_in(token)?), &json!({ "id": id }))
}

/// `POST /v1/inbox/retract`: Take It Back, before a shop has taken it.
pub fn retract(hub: &str, token: &str, id: &str) -> Result<HttpRequest, ArtError> {
    let hub = cloud::clean_server(hub).ok_or(ArtError::NoStitchHub)?;
    cloud::request(&hub, "/v1/inbox/retract", Some(signed_in(token)?), &json!({ "id": id }))
}

/// The hub's answer with HTTP status `status` and body `body` as a value, or what to tell the
/// person (the hub's own sentence when it gives one).
fn checked(status: u16, body: &str) -> Result<Value, ArtError> {
    let v: Option<Value> = serde_json::from_str(body).ok();
    if !(200..300).contains(&status) {
        let message = v.as_ref().and_then(|v| v["error"].as_str()).map(shown).filter(|m| !m.is_empty());
        return Err(match (status, message) {
            (401, m) => ArtError::StitchHubSignIn(m.unwrap_or_default()),
            (_, Some(m)) => ArtError::StitchHub(m),
            (s, None) => ArtError::StitchHubFailed(s),
        });
    }
    v.ok_or(ArtError::NoAnswer)
}

/// The shops in the answer to [`shops`].
pub fn shops_answer(status: u16, body: &str) -> Result<Vec<Shop>, ArtError> {
    let v = checked(status, body)?;
    let list = v["shops"].as_array().ok_or(ArtError::NoAnswer)?;
    Ok(list
        .iter()
        .take(50)
        .filter_map(|s| {
            let id = s["id"].as_str().map(str::to_string).or_else(|| s["id"].as_u64().map(|n| n.to_string()))?;
            Some(Shop {
                id: shown(&id),
                name: shown(s["name"].as_str().unwrap_or_default()),
                program: shown(s["program"].as_str().unwrap_or_default()),
            })
        })
        .collect())
}

/// The addresses in the answer to [`put`]; `preview`: whether one was asked for.
pub fn put_answer(status: u16, body: &str, preview: bool) -> Result<Slots, ArtError> {
    let v = checked(status, body)?;
    let id = v["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 200).ok_or(ArtError::NoAnswer)?;
    let file = slot(&v).ok_or(ArtError::NoAnswer)?;
    let preview = if preview { Some(slot(&v["preview"]).ok_or(ArtError::NoAnswer)?) } else { None };
    Ok(Slots { id: id.to_string(), file, preview })
}

/// An upload address: HTTPS (or this computer), and only the headers an upload needs.
fn slot(v: &Value) -> Option<Slot> {
    let url = v["url"].as_str()?;
    let base = url.split(['?', '#']).next()?;
    cloud::clean_server(base)?;
    if url.len() > 8000 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    if v["method"].as_str().is_some_and(|m| m != "PUT") {
        return None;
    }
    let mut headers = Vec::new();
    if let Some(h) = v["headers"].as_object() {
        for (k, val) in h {
            let name: &'static str = match k.to_ascii_lowercase().as_str() {
                "content-type" => "content-type",
                "x-amz-checksum-sha256" => "x-amz-checksum-sha256",
                "x-amz-sdk-checksum-algorithm" => "x-amz-sdk-checksum-algorithm",
                _ => continue,
            };
            let val = val.as_str().filter(|s| s.len() <= 200 && !s.chars().any(|c| c.is_control()))?;
            headers.push((name, val.to_string()));
        }
    }
    Some(Slot { url: url.to_string(), headers })
}

/// The file store's answer to an [`upload`].
pub fn upload_answer(status: u16, _body: &str) -> Result<(), ArtError> {
    if (200..300).contains(&status) { Ok(()) } else { Err(ArtError::UploadFailed(status)) }
}

/// The hub's answer to [`done`].
pub fn done_answer(status: u16, body: &str) -> Result<(), ArtError> {
    checked(status, body).map(|_| ())
}

/// The hub's answer to [`retract`]: whether it was still there to take back.
pub fn retract_answer(status: u16, body: &str) -> Result<bool, ArtError> {
    Ok(checked(status, body)?["retracted"].as_u64().unwrap_or(0) > 0)
}

/// A message from the hub as it is shown: one line, not too long.
fn shown(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_MESSAGE).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "sfs_0123456789abcdef";

    fn offer() -> Offer {
        Offer {
            file_name: "Rose crest.dst".into(),
            file: b"LA:rose".to_vec(),
            preview: Some(b"\x89PNG".to_vec()),
            meta: json!({"design": "Rose crest"}),
            to_shop: Some("shop_1".into()),
        }
    }

    #[test]
    fn the_requests_in_turn() {
        let r = shops("https://hub.stitchbench.app/", TOKEN).unwrap();
        assert_eq!((r.method, r.url.as_str()), (Method::Get, "https://hub.stitchbench.app/v1/inbox/shops"));
        assert_eq!(r.headers, vec![("authorization", format!("Bearer {TOKEN}"))]);

        let r = put("https://hub.stitchbench.app", TOKEN, &offer()).unwrap();
        assert_eq!((r.method, r.url.as_str()), (Method::Post, "https://hub.stitchbench.app/v1/inbox/put"));
        let b: Value = serde_json::from_str(&r.body).unwrap();
        assert_eq!(b["fileName"], "Rose crest.dst");
        assert_eq!(b["size"], 7);
        assert_eq!(b["sha256"], sha256_hex(b"LA:rose"));
        assert_eq!(b["toShopId"], "shop_1");
        assert_eq!(b["preview"], json!({"size": 4, "sha256": sha256_hex(b"\x89PNG"), "type": "image/png"}));
        assert_eq!(b["meta"], json!({"design": "Rose crest"}));
        let mut any = offer();
        (any.to_shop, any.preview) = (None, None);
        let b: Value = serde_json::from_str(&put("https://hub.stitchbench.app", TOKEN, &any).unwrap().body).unwrap();
        assert!(b.get("toShopId").is_none() && b.get("preview").is_none());

        let slot = Slot { url: "https://store.example/inbox/1/a?X-Amz-Signature=ab".into(), headers: vec![("x-amz-checksum-sha256", "q83v".into())] };
        let r = upload(&slot, b"LA:rose");
        assert_eq!((r.method, r.url.as_str(), r.bytes.as_deref()), (Method::Put, slot.url.as_str(), Some(&b"LA:rose"[..])));
        assert_eq!(r.headers, slot.headers);

        let r = done("https://hub.stitchbench.app", TOKEN, "it_1").unwrap();
        assert_eq!((r.url.as_str(), r.body.as_str()), ("https://hub.stitchbench.app/v1/inbox/done", r#"{"id":"it_1"}"#));
        let r = retract("http://127.0.0.1:8787", TOKEN, "it_1").unwrap();
        assert_eq!(r.url, "http://127.0.0.1:8787/v1/inbox/retract");

        // signed out, or another kind of key: nothing is sent
        assert_eq!(shops("https://hub.stitchbench.app", ""), Err(ArtError::SignIn));
        assert_eq!(put("https://hub.stitchbench.app", "sk-ant-x", &offer()), Err(ArtError::SignIn));
        assert_eq!(shops("http://hub.stitchbench.app", TOKEN), Err(ArtError::NoStitchHub));
        assert_eq!(hub("").unwrap(), DEFAULT_HUB);
        assert_eq!(hub(" http://localhost:8787/ ").unwrap(), "http://localhost:8787");
        assert_eq!(hub("hub.example"), Err(ArtError::NoStitchHub));
    }

    #[test]
    fn limits_are_checked_before_sending() {
        let mut o = offer();
        o.file = vec![0; MAX_FILE_BYTES];
        assert!(put(DEFAULT_HUB, TOKEN, &o).is_ok());
        o.file.push(0);
        assert_eq!(put(DEFAULT_HUB, TOKEN, &o), Err(ArtError::TooBigForStitchShop));
        o.file = Vec::new();
        assert_eq!(put(DEFAULT_HUB, TOKEN, &o), Err(ArtError::TooBigForStitchShop));
        let mut o = offer();
        o.preview = Some(vec![0; MAX_PREVIEW_BYTES + 1]);
        assert_eq!(put(DEFAULT_HUB, TOKEN, &o), Err(ArtError::TooBigForStitchShop));
    }

    #[test]
    fn file_names_are_made_to_fit() {
        let ok = |s: &str| {
            let mut c = s.chars();
            c.next().is_some_and(|c| c.is_ascii_alphanumeric())
                && s.chars().count() <= MAX_NAME
                && s.chars().all(|c| c.is_ascii_alphanumeric() || " ._()+-".contains(c))
        };
        for (raw, want) in [
            ("Rose crest.dst", "Rose crest.dst"),
            ("C:\\designs\\J-0001 Rovers FC.dst", "J-0001 Rovers FC.dst"),
            ("Café & Bar #2.pes", "Caf_ Bar 2.pes"),
            ("  ..hidden.dst", "hidden.dst"),
            ("___.dst", "Design.dst"),
            ("", "Design"),
            ("logo:v2/final?.jef", "final.jef"),
        ] {
            assert_eq!(file_name(raw), want, "{raw}");
            assert!(ok(&file_name(raw)), "{raw}");
        }
        let long = file_name(&format!("{}.dst", "a".repeat(300)));
        assert_eq!(long.len(), MAX_NAME);
        assert!(long.ends_with(".dst") && ok(&long));
    }

    #[test]
    fn dst_stitches_are_the_records_without_jump_or_colour_bits() {
        let mut f = vec![b' '; 512];
        f.extend([0, 0, 0x03, 1, 0, 0x03, 0, 0, 0x83, 0, 0, 0xC3, 2, 0, 0x03, 0, 0, 0xF3, 9, 9, 0x03]);
        assert_eq!(dst_stitches(&f), Some(3));
        assert_eq!(dst_stitches(&f[..100]), None);
    }

    #[test]
    fn meta_carries_the_design_and_the_job_in_the_hubs_keys() {
        let info = json!({
            "stitches": 9000, "trims": 4, "colorChanges": 2, "stops": 1, "widthMm": 80.04, "heightMm": 60.96,
            "threadM": 41.23, "bobbinM": 13.6, "seconds": 601.0, "hoop": "Brother 100 × 100 (4×4)", "fabric": "Pique polo",
            "notes": "Club crest", "colors": [{"name": "Madeira 1147 Red"}, {"name": "Madeira 1000 Black"}, {"name": "Madeira 1147 Red"}],
        });
        let job = json!({"number": "J-0001", "customer": "Rovers FC", "garment": "Navy polo", "placement": "left-chest", "due": "2026-11-01", "quantity": 24, "notes": ""});
        let m = meta(&info, Some(&job), "Rose crest", "PES", b"#PES", "0.14.0");
        assert_eq!(
            m,
            json!({
                "program": "StitchFoundry", "version": "0.14.0", "design": "Rose crest", "format": "pes", "hoop": "Brother 100 × 100 (4×4)",
                "fabric": "Pique polo", "stitches": 9000, "colors": 3, "threads": ["Madeira 1147 Red", "Madeira 1000 Black", "Madeira 1147 Red"],
                "colorChanges": 3, "trims": 4, "widthMm": 80.0, "heightMm": 61.0, "threadM": 41.2, "bobbinM": 13.6, "runMinutes": 11,
                "notes": "Club crest", "job": "J-0001", "customer": "Rovers FC", "garment": "Navy polo", "placement": "left-chest",
                "due": "2026-11-01", "quantity": 24,
            })
        );
        // a DST's stitches are counted in the file, as StitchShop counts them
        let mut dst = vec![b' '; 512];
        dst.extend([0, 0, 0x03, 0, 0, 0x83, 0, 0, 0xF3]);
        assert_eq!(meta(&info, None, "Rose", "dst", &dst, "0.14.0")["stitches"], 1);
        // the job's own notes win; no job, no job keys
        let with_notes = json!({"number": "J-2", "notes": "Gold on the left"});
        assert_eq!(meta(&info, Some(&with_notes), "Rose", "dst", &dst, "1")["notes"], "Gold on the left");
        assert!(meta(&info, None, "Rose", "dst", &dst, "1").get("job").is_none());
        // nothing stitched yet
        assert_eq!(
            meta(&json!({}), None, "", "dst", b"", "1"),
            json!({"program": "StitchFoundry", "version": "1", "format": "dst", "colorChanges": 0})
        );
    }

    #[test]
    fn answers_and_their_messages() {
        let shops =
            shops_answer(200, r#"{"shops": [{"id": "s1", "name": "Main St", "program": "StitchShop"}, {"id": 7, "name": "Back\nroom"}]}"#).unwrap();
        assert_eq!(shops[0], Shop { id: "s1".into(), name: "Main St".into(), program: "StitchShop".into() });
        assert_eq!((shops[1].id.as_str(), shops[1].name.as_str()), ("7", "Back room"));

        let ok = json!({
            "id": "it_1", "url": "https://store.example/inbox/9/it_1?X-Amz-Signature=ab", "method": "PUT", "expiresIn": 900,
            "headers": {"x-amz-checksum-sha256": "q83v", "x-evil": "1"},
            "preview": {"url": "https://store.example/inbox/9/it_1.preview?s=1", "method": "PUT", "headers": {"content-type": "image/png", "x-amz-checksum-sha256": "AAAA"}},
        })
        .to_string();
        let s = put_answer(200, &ok, true).unwrap();
        assert_eq!(s.id, "it_1");
        assert_eq!(s.file.headers, vec![("x-amz-checksum-sha256", "q83v".to_string())], "only the headers an upload needs");
        assert_eq!(s.preview.unwrap().headers.len(), 2);
        assert_eq!(put_answer(200, &ok, false).unwrap().preview, None);
        // an address that isn't https isn't uploaded to; a preview asked for must come back
        for bad in [
            json!({"id": "it_1", "url": "http://store.example/x"}),
            json!({"id": "it_1", "url": "javascript:alert(1)"}),
            json!({"id": "it_1", "url": "https://store.example/a b"}),
            json!({"id": "it_1", "url": "https://store.example/x", "method": "POST"}),
            json!({"url": "https://store.example/x"}),
        ] {
            assert_eq!(put_answer(200, &bad.to_string(), false), Err(ArtError::NoAnswer), "{bad}");
        }
        assert_eq!(put_answer(200, &json!({"id": "it_1", "url": "https://s.example/x"}).to_string(), true), Err(ArtError::NoAnswer));

        let said = |m: &str| json!({"error": m}).to_string();
        let linked = "No StitchHub account is linked to this Foundry Account yet. In your shop's Settings, Foundry Account, link it, or sign in at the hub's account page with it.";
        assert_eq!(put_answer(404, &said(linked), false), Err(ArtError::StitchHub(linked.into())));
        assert_eq!(
            put_answer(413, &said("25 files are already waiting. Take some into orders (or retract them) first."), false).map_err(|e| e.to_string()),
            Err("25 files are already waiting. Take some into orders (or retract them) first.".into())
        );
        assert_eq!(
            shops_answer(401, &said("Sign in to StitchFoundry again.")),
            Err(ArtError::StitchHubSignIn("Sign in to StitchFoundry again.".into()))
        );
        assert_eq!(shops_answer(503, "<html>"), Err(ArtError::StitchHubFailed(503)));
        assert_eq!(upload_answer(200, ""), Ok(()));
        assert_eq!(upload_answer(403, "<Error>SignatureDoesNotMatch</Error>"), Err(ArtError::UploadFailed(403)));
        assert_eq!(done_answer(200, r#"{"ok": true}"#), Ok(()));
        assert_eq!(done_answer(409, &said("The file isn't in the store.")), Err(ArtError::StitchHub("The file isn't in the store.".into())));
        assert_eq!(retract_answer(200, r#"{"ok": true, "retracted": 1}"#), Ok(true));
        assert_eq!(retract_answer(200, r#"{"ok": true, "retracted": 0}"#), Ok(false));
        assert_eq!(
            retract_answer(409, &said("A shop has already taken that file into an order.")),
            Err(ArtError::StitchHub("A shop has already taken that file into an order.".into()))
        );
    }
}
