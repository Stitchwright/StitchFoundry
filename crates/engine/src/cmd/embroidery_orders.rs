//! Online orders (ORD-01; Pulse's PulseID does this): orders read from an online shop (Shopify,
//! WooCommerce) or an orders CSV export (Etsy and any other shop), each personalized line item made
//! into a design by the first rule matching its product, with a job order, machine files and a
//! worksheet whose barcode finds the job again. The host app fetches the orders (it has the
//! network and the shop's keys); this reads what it fetched and makes the designs.
//!
//! The shops' formats are as their public API documentation gives them: Shopify's GraphQL Admin
//! API `orders` query (https://shopify.dev/docs/api/admin-graphql/latest/queries/orders, line items'
//! `customAttributes`: https://shopify.dev/docs/api/admin-graphql/latest/objects/LineItem) and
//! WooCommerce's REST API `GET /wp-json/wc/v3/orders`
//! (https://woocommerce.github.io/woocommerce-rest-api-docs/#list-all-orders, line items'
//! `meta_data`). Everything read is untrusted: sizes and counts are capped.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, bool_or, cmd, str_param};

/// The most orders and line items read at once, and characters kept of a field.
const MAX_ORDERS: usize = 500;
const MAX_ITEMS: usize = 2000;
const MAX_FIELD: usize = 500;
/// The most rules, and order lines remembered as made.
const MAX_RULES: usize = 100;
const MAX_DONE: usize = 5000;
/// The most bytes of an orders export read.
const MAX_INPUT: usize = 20 << 20;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "embroidery.orders.setup",
            "Online Orders Setup",
            [],
            None,
            "{shop?: shopify|woocommerce|csv, store?: the shop's address (shopify: name.myshopify.com; woocommerce: https://example.com), folder?: where the designs and machine files go (My Designs/Orders), formats?: [dst|pes|jef|exp|vp3|…] machine files for each order (none: the design and worksheet only), rules?: [{product?: text in the product's name or SKU (empty: any), template?: a design template, font?: an embroidery font, color?: \"#rrggbb\" or a thread colour's name, size?: mm, textFields?: [the order fields holding the text (empty: any field named like name, text, monogram or personalization)], fontField?: the field holding a font choice (font), colorField?: the field holding a thread colour (colour or color)}] (in order: the first that matches an item makes it; replaces the rules), forget?: bool (forget which order lines were made)} how online orders are made into designs; without params, as set → {shop, store, folder, formats, rules, made}",
            always,
            setup
        ),
        cmd!(
            "embroidery.orders.import",
            "Make Designs from Orders",
            [],
            None,
            "{source: shopify|woocommerce|csv, data: what the shop gave (shopify: the GraphQL orders answer; woocommerce: the orders list; csv: an orders export with a header row), dryRun?: bool (only say what would be made)} each personalized line item not made before, by the first rule matching it: a design from the rule's template with the item's text in its font and colour, saved in the orders folder, with a job order (the shop's order number and customer), its machine files and a worksheet; items without text, or no rule matching, are listed to look at → {made: [{order, customer, product, text, job, design}], skipped: [{order, product, why}], orders}",
            always,
            import
        ),
        cmd!(
            "embroidery.job.find",
            "Find Job",
            [],
            None,
            "{code: a job number (J-0007, as a worksheet's barcode reads) or a shop's order number (#1001)} the job order it names → {job}",
            always,
            find_job
        ),
    ]
}

/// Online orders: where they come from, the rules making them into designs, and the order lines
/// made already. Kept in the preferences; the shop's keys are kept by the host app's secret store,
/// never here.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OrderSetup {
    pub shop: String,
    pub store: String,
    pub folder: String,
    pub formats: Vec<String>,
    pub rules: Vec<OrderRule>,
    /// The order lines made, as `shop:order:line`, oldest first.
    pub done: Vec<String>,
}

impl OrderSetup {
    pub fn is_empty(&self) -> bool {
        *self == OrderSetup::default()
    }
}

/// A rule: the items it takes, and the design it makes of them.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct OrderRule {
    pub product: String,
    pub template: String,
    pub font: String,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub text_fields: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub font_field: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub color_field: String,
}

impl OrderRule {
    /// Whether the rule takes an item of `product` with `sku`.
    fn takes(&self, product: &str, sku: &str) -> bool {
        let want = self.product.trim().to_lowercase();
        want.is_empty() || want == "*" || product.to_lowercase().contains(&want) || (!sku.is_empty() && sku.to_lowercase() == want)
    }
}

/// An order's line item, as any shop gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderItem {
    /// `shop:order:line`, to know it again.
    pub key: String,
    pub order: String,
    pub customer: String,
    pub product: String,
    pub sku: String,
    pub quantity: u32,
    /// The item's fields: the shop's personalization (Shopify's custom attributes, WooCommerce's
    /// item meta data, a CSV's other columns).
    pub fields: Vec<(String, String)>,
}

/// `v` as text (numbers and booleans written out), at most [`MAX_FIELD`] characters.
fn text(v: &Value) -> String {
    let t = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    };
    t.trim().chars().take(MAX_FIELD).collect()
}

/// A count read from an order (1 when missing, at most 10 000).
fn count(v: &Value) -> u32 {
    v.as_u64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).map_or(1, |n| n.clamp(1, 10_000) as u32)
}

/// Shopify's GraphQL `orders` answer: `{data: {orders: {edges: [{node: order}]}}}`, each order with
/// `name`, `customer {displayName}` (or `shippingAddress {name}`, `email`) and `lineItems {edges:
/// [{node: {id, title, name, sku, quantity, unfulfilledQuantity, customAttributes: [{key, value}]}}]}`.
pub fn shopify_items(answer: &Value) -> Vec<OrderItem> {
    let edges = |v: &Value| v["edges"].as_array().map(|e| e.iter().map(|e| e["node"].clone()).collect::<Vec<_>>()).unwrap_or_default();
    let mut out = Vec::new();
    for o in edges(&answer["data"]["orders"]).iter().take(MAX_ORDERS) {
        let order = text(&o["name"]);
        let customer = [&o["customer"]["displayName"], &o["shippingAddress"]["name"], &o["billingAddress"]["name"], &o["email"]]
            .into_iter()
            .map(text)
            .find(|t| !t.is_empty())
            .unwrap_or_default();
        for li in edges(&o["lineItems"]).iter() {
            if out.len() >= MAX_ITEMS {
                return out;
            }
            // what is left to make: unfulfilled units when Shopify says
            let quantity = if li["unfulfilledQuantity"].is_number() {
                li["unfulfilledQuantity"].as_u64().unwrap_or(0)
            } else {
                u64::from(count(&li["quantity"]))
            };
            if quantity == 0 {
                continue;
            }
            let fields = li["customAttributes"]
                .as_array()
                .map(|a| a.iter().take(50).map(|f| (text(&f["key"]), text(&f["value"]))).filter(|(k, _)| !k.is_empty()).collect())
                .unwrap_or_default();
            let line = text(&li["id"]);
            out.push(OrderItem {
                key: format!("shopify:{}:{line}", text(&o["id"])),
                order: order.clone(),
                customer: customer.clone(),
                product: [text(&li["title"]), text(&li["name"])].into_iter().find(|t| !t.is_empty()).unwrap_or_default(),
                sku: text(&li["sku"]),
                quantity: quantity.min(10_000) as u32,
                fields,
            });
        }
    }
    out
}

/// WooCommerce's orders list: `[{id, number, billing: {first_name, last_name}, line_items: [{id,
/// name, sku, quantity, meta_data: [{key, value, display_key?, display_value?}]}]}]`. Meta data whose
/// key starts with `_` is WooCommerce's own and left out.
pub fn woocommerce_items(orders: &Value) -> Vec<OrderItem> {
    let mut out = Vec::new();
    for o in orders.as_array().map(Vec::as_slice).unwrap_or_default().iter().take(MAX_ORDERS) {
        let number = [text(&o["number"]), text(&o["id"])].into_iter().find(|t| !t.is_empty()).unwrap_or_default();
        let name = |a: &Value| format!("{} {}", text(&a["first_name"]), text(&a["last_name"])).trim().to_string();
        let customer = [name(&o["shipping"]), name(&o["billing"]), text(&o["billing"]["company"]), text(&o["billing"]["email"])]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or_default();
        for li in o["line_items"].as_array().map(Vec::as_slice).unwrap_or_default() {
            if out.len() >= MAX_ITEMS {
                return out;
            }
            let fields = li["meta_data"]
                .as_array()
                .map(|m| {
                    m.iter()
                        .take(50)
                        .filter(|f| !text(&f["key"]).starts_with('_'))
                        .map(|f| {
                            let key = [text(&f["display_key"]), text(&f["key"])].into_iter().find(|t| !t.is_empty()).unwrap_or_default();
                            let value = [text(&f["display_value"]), text(&f["value"])].into_iter().find(|t| !t.is_empty()).unwrap_or_default();
                            (key, value)
                        })
                        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            out.push(OrderItem {
                key: format!("woocommerce:{}:{}", text(&o["id"]), text(&li["id"])),
                order: format!("#{number}"),
                customer: customer.clone(),
                product: text(&li["name"]),
                sku: text(&li["sku"]),
                quantity: count(&li["quantity"]),
                fields,
            });
        }
    }
    out
}

/// The fields of one CSV line (RFC 4180 quotes; commas, semicolons or tabs between fields).
fn csv_line(line: &str, sep: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == sep && !quoted => out.push(std::mem::take(&mut cur).trim().chars().take(MAX_FIELD).collect()),
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().chars().take(MAX_FIELD).collect());
    out
}

/// The records of a CSV text: lines, except where a quoted field holds a line break.
fn csv_records(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quotes = 0usize;
    for line in text.lines() {
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
        quotes += line.matches('"').count();
        if quotes.is_multiple_of(2) {
            out.push(std::mem::take(&mut cur));
            quotes = 0;
            if out.len() > MAX_ITEMS {
                break;
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// An orders CSV export with a header row (Etsy's, Shopify's, a spreadsheet's): the order number,
/// customer, product, SKU and quantity columns found by their names; every other column a field.
/// Etsy's `Variations` column ("Personalization:Rose, Colour:Navy") gives a field a pair.
pub fn csv_items(text: &str) -> Vec<OrderItem> {
    let records = csv_records(text.trim_start_matches('\u{feff}'));
    let Some((head, rows)) = records.split_first() else { return Vec::new() };
    let sep = [',', ';', '\t'].into_iter().max_by_key(|c| head.matches(*c).count()).unwrap_or(',');
    let names: Vec<String> = csv_line(head, sep).iter().map(|n| n.to_lowercase()).collect();
    let col = |want: &[&str]| want.iter().find_map(|w| names.iter().position(|n| n == w));
    let order = col(&["order", "order id", "order number", "order no", "order #", "name"]);
    let customer = col(&["customer", "customer name", "buyer", "full name", "ship name", "shipping name", "billing name"]);
    let product = col(&["product", "product name", "item", "item name", "lineitem name", "title"]);
    let sku = col(&["sku", "lineitem sku", "item sku"]);
    let quantity = col(&["quantity", "qty", "lineitem quantity"]);
    let known = [order, customer, product, sku, quantity];
    let mut out = Vec::new();
    for (i, row) in rows.iter().enumerate().take(MAX_ITEMS) {
        let f = csv_line(row, sep);
        let get = |c: Option<usize>| c.and_then(|c| f.get(c)).cloned().unwrap_or_default();
        if f.iter().all(|v| v.is_empty()) {
            continue;
        }
        let mut fields = Vec::new();
        for (c, v) in f.iter().enumerate() {
            let Some(name) = names.get(c) else { continue };
            if known.contains(&Some(c)) || v.is_empty() {
                continue;
            }
            if name == "variations" || name == "options" {
                // "Personalization:Rose, Colour:Navy"
                for pair in v.split(',') {
                    if let Some((k, val)) = pair.split_once(':') {
                        fields.push((k.trim().to_string(), val.trim().to_string()));
                    }
                }
            } else {
                fields.push((name.clone(), v.clone()));
            }
        }
        let number = get(order);
        let number = if number.is_empty() { format!("row {}", i + 2) } else { number };
        out.push(OrderItem {
            key: format!("csv:{number}:{}:{}", get(product), i + 2),
            order: if number.starts_with('#') || number.starts_with("row") { number.clone() } else { format!("#{number}") },
            customer: get(customer),
            product: get(product),
            sku: get(sku),
            quantity: count(&Value::String(get(quantity))),
            fields,
        });
    }
    out
}

/// Whether a field named `key` holds text to sew (a name, initials, a message).
fn text_field(key: &str) -> bool {
    let k = key.to_lowercase();
    let says = ["name", "text", "personali", "monogram", "initial", "line", "wording", "message", "embroider"].iter().any(|w| k.contains(w));
    let other = ["font", "colour", "color", "thread", "size", "style", "placement", "email", "date", "phone"].iter().any(|w| k.contains(w));
    says && !other
}

/// The text `item` asks for, by `rule`'s fields (else any field named like text), up to four
/// lines.
fn item_text(item: &OrderItem, rule: &OrderRule) -> String {
    let take = |k: &str| {
        if rule.text_fields.is_empty() { text_field(k) } else { rule.text_fields.iter().any(|f| f.trim().eq_ignore_ascii_case(k.trim())) }
    };
    item.fields
        .iter()
        .filter(|(k, _)| take(k))
        .flat_map(|(_, v)| v.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect::<Vec<_>>())
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The value of the field `want` names (or, with `want` empty, of one named like `guesses`).
fn field<'a>(item: &'a OrderItem, want: &str, guesses: &[&str]) -> Option<&'a str> {
    item.fields
        .iter()
        .find(|(k, _)| {
            let k = k.to_lowercase();
            if want.trim().is_empty() { guesses.iter().any(|g| k.contains(g)) } else { k == want.trim().to_lowercase() }
        })
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.trim().is_empty())
}

/// A thread colour from an order or a rule: `#rrggbb`, else a generic thread's name ("Navy").
fn colour(name: &str) -> Option<String> {
    let name = name.trim();
    if let Some(c) = sf_core::Rgb::from_hex(name).filter(|_| name.starts_with('#')) {
        return Some(c.to_hex());
    }
    let chart = sf_core::threads::generic_chart();
    let n = name.to_lowercase();
    chart
        .threads
        .iter()
        .find(|t| t.name.to_lowercase() == n)
        .or_else(|| chart.threads.iter().find(|t| !n.is_empty() && (t.name.to_lowercase().contains(&n) || n.contains(&t.name.to_lowercase()))))
        .map(|t| t.color.to_hex())
}

/// A loaded embroidery font a customer's choice names: exactly, else one whose name holds it.
fn font_named(s: &Session, want: &str) -> Option<String> {
    let w = want.trim().to_lowercase();
    if w.is_empty() {
        return None;
    }
    let names: Vec<&String> = s.embroidery_fonts.iter().map(|f| &f.info.name).collect();
    names.iter().find(|n| n.to_lowercase() == w).or_else(|| names.iter().find(|n| n.to_lowercase().contains(&w))).map(|n| n.to_string())
}

fn setup(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.orders.setup";
    let mut o = s.prefs.embroidery_orders.clone();
    if let Some(shop) = str_param(p, "shop") {
        let shop = shop.trim().to_lowercase();
        if !["shopify", "woocommerce", "csv"].contains(&shop.as_str()) {
            return Err(bad(CMD, "`shop` is shopify, woocommerce or csv"));
        }
        o.shop = shop;
    }
    if let Some(store) = str_param(p, "store") {
        o.store = store.trim().chars().take(300).collect();
    }
    if let Some(folder) = str_param(p, "folder") {
        o.folder = folder.trim().chars().take(2000).collect();
    }
    if let Some(f) = p.get("formats") {
        let list = f.as_array().ok_or_else(|| bad(CMD, "`formats` is a list: [\"dst\", \"pes\"]"))?;
        let mut formats = Vec::new();
        for v in list.iter().take(20) {
            let id = v.as_str().map(|f| f.trim().to_lowercase()).ok_or_else(|| bad(CMD, "a format is a name: dst, pes, jef…"))?;
            if !super::embroidery::MACHINE_EXTS.contains(&id.as_str()) {
                return Err(bad(CMD, format!("`{id}` isn't a machine format StitchFoundry writes")));
            }
            if !formats.contains(&id) {
                formats.push(id);
            }
        }
        o.formats = formats;
    }
    if let Some(r) = p.get("rules") {
        let list = r.as_array().ok_or_else(|| bad(CMD, "`rules` is a list of {product, template, font, color, size}"))?;
        if list.len() > MAX_RULES {
            return Err(bad(CMD, format!("{MAX_RULES} rules at most")));
        }
        let mut rules = Vec::new();
        for v in list {
            let mut rule: OrderRule = serde_json::from_value(v.clone()).map_err(|e| bad(CMD, format!("a rule: {e}")))?;
            let cut = |t: &mut String| *t = t.trim().chars().take(200).collect();
            for t in [&mut rule.product, &mut rule.template, &mut rule.font, &mut rule.color, &mut rule.font_field, &mut rule.color_field] {
                cut(t);
            }
            rule.text_fields.truncate(10);
            rule.text_fields.iter_mut().for_each(cut);
            if !rule.color.is_empty() && colour(&rule.color).is_none() {
                return Err(bad(CMD, format!("`{}` isn't a colour: \"#rrggbb\" or a thread colour's name", rule.color)));
            }
            if rule.size.is_some_and(|v| !(3.0..=200.0).contains(&v)) {
                return Err(bad(CMD, "a rule's `size` is 3 to 200 mm"));
            }
            rules.push(rule);
        }
        o.rules = rules;
    }
    if bool_or(p, "forget", false) {
        o.done.clear();
    }
    let made = o.done.len();
    s.prefs.embroidery_orders = o.clone();
    Ok(json!({ "shop": o.shop, "store": o.store, "folder": o.folder, "formats": o.formats, "rules": o.rules, "made": made }))
}

/// The folder orders go in: the setup's, else My Designs/Orders.
fn orders_folder(s: &Session) -> Option<std::path::PathBuf> {
    let set = s.prefs.embroidery_orders.folder.trim();
    if !set.is_empty() {
        return Some(set.into());
    }
    super::embroidery_safe::designs_folder(s).map(|d| d.join("Orders"))
}

fn import(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.orders.import";
    let source = str_param(p, "source").map(str::to_lowercase).ok_or_else(|| bad(CMD, "missing `source`: shopify, woocommerce or csv"))?;
    let data = p.get("data").ok_or_else(|| bad(CMD, "missing `data`: what the shop gave"))?;
    // the shop's answer as given, or as the text of it
    let parsed;
    let data = match data {
        Value::String(t) if source != "csv" => {
            if t.len() > MAX_INPUT {
                return Err(bad(CMD, "the orders are larger than 20 MB"));
            }
            parsed = serde_json::from_str::<Value>(t).map_err(|e| bad(CMD, format!("the shop's answer isn't JSON: {e}")))?;
            &parsed
        }
        v => v,
    };
    let items = match source.as_str() {
        "shopify" => {
            // A field the app may not read (the customer, without read_customers) comes back as
            // an error beside the orders: only an answer without orders fails.
            if let Some(e) = data["errors"].as_array().and_then(|e| e.first()).filter(|_| data["data"]["orders"].is_null()) {
                return Err(bad(CMD, format!("Shopify said: {}", text(&e["message"]))));
            }
            shopify_items(data)
        }
        "woocommerce" => {
            if let Some(m) = data["message"].as_str().filter(|_| !data.is_array()) {
                return Err(bad(CMD, format!("WooCommerce said: {}", m.chars().take(MAX_FIELD).collect::<String>())));
            }
            woocommerce_items(data)
        }
        "csv" => {
            let t = data.as_str().ok_or_else(|| bad(CMD, "`data` is the CSV file's text"))?;
            if t.len() > MAX_INPUT {
                return Err(bad(CMD, "the orders are larger than 20 MB"));
            }
            csv_items(t)
        }
        _ => return Err(bad(CMD, "`source` is shopify, woocommerce or csv")),
    };
    let dry = bool_or(p, "dryRun", false);
    let setup = s.prefs.embroidery_orders.clone();
    let folder = orders_folder(s);
    if !dry && folder.is_none() {
        return Err(bad(CMD, "no folder for the orders: set one (embroidery.orders.setup {folder})"));
    }
    if !dry && let Some(f) = &folder {
        std::fs::create_dir_all(f).map_err(|e| bad(CMD, format!("can't make the orders folder {}: {e}", f.display())))?;
    }
    // the fonts remembered from earlier sessions, loaded the first time
    if s.embroidery_fonts.is_empty() {
        // with none loaded, each item says so below
        let _ = s.execute("embroidery.fonts", &json!({}));
    }
    let orders: std::collections::BTreeSet<&str> = items.iter().map(|i| i.order.as_str()).collect();
    let mut made = Vec::new();
    let mut skipped = Vec::new();
    let mut done = Vec::new();
    for item in &items {
        if setup.done.contains(&item.key) {
            continue;
        }
        let skip = |why: &str| json!({ "order": item.order, "product": item.product, "why": why });
        let Some(rule) = setup.rules.iter().find(|r| r.takes(&item.product, &item.sku)) else {
            skipped.push(skip("no rule matches this product"));
            continue;
        };
        let text = item_text(item, rule);
        if text.is_empty() {
            skipped.push(skip("no text to sew in the order"));
            continue;
        }
        let font = field(item, &rule.font_field, &["font"])
            .and_then(|f| font_named(s, f))
            .or_else(|| font_named(s, &rule.font))
            .or_else(|| s.embroidery_fonts.first().map(|f| f.info.name.clone()));
        let Some(font) = font else {
            skipped.push(skip("no embroidery font is loaded"));
            continue;
        };
        let color = field(item, &rule.color_field, &["colour", "color", "thread"])
            .and_then(colour)
            .or_else(|| colour(&rule.color))
            .unwrap_or_else(|| "#1d3557".to_string());
        let template = if rule.template.trim().is_empty() { "Name tape 100 × 30".to_string() } else { rule.template.clone() };
        if dry {
            made.push(json!({ "order": item.order, "customer": item.customer, "product": item.product, "text": text, "font": font, "color": color, "template": template, "quantity": item.quantity }));
            continue;
        }
        match make(s, item, &text, template, font, color, rule.size, folder.as_deref(), &setup.formats) {
            Ok(v) => {
                made.push(v);
                done.push(item.key.clone());
            }
            Err(e) => skipped.push(skip(&e.to_string())),
        }
    }
    if !done.is_empty() {
        let o = &mut s.prefs.embroidery_orders;
        o.done.extend(done);
        let over = o.done.len().saturating_sub(MAX_DONE);
        o.done.drain(..over);
    }
    Ok(json!({ "made": made, "skipped": skipped, "orders": orders.len() }))
}

/// One item made: its design (from the template, saved in the orders folder), a job order, the
/// machine files and the worksheet; the design closed again.
#[allow(clippy::too_many_arguments)]
fn make(
    s: &mut Session,
    item: &OrderItem,
    text: &str,
    template: String,
    font: String,
    color: String,
    size: Option<f64>,
    folder: Option<&std::path::Path>,
    formats: &[String],
) -> Result<Value> {
    let folder = folder.ok_or_else(|| bad("embroidery.orders.import", "no folder for the orders"))?;
    let docs = s.documents().len();
    let r = super::embroidery_kiosk::lettered(s, text, template, font, color, size);
    let result = r.and_then(|r| {
        let first = text.lines().next().unwrap_or("design");
        let name = super::embroidery_lettering::safe_name(&format!("{} {first}", item.order.trim_start_matches('#')));
        let path = folder.join(format!("{name}.stitchfoundry"));
        s.execute("file.saveAs", &json!({ "path": path.to_string_lossy(), "format": "vectorcraft" }))?;
        let notes = format!("Order {} · {} · {}", item.order, item.product, text.replace('\n', " / "));
        let customer = if item.customer.is_empty() { item.order.clone() } else { item.customer.clone() };
        let job = s.execute(
            "embroidery.job.create",
            &json!({ "customer": customer, "design": path.to_string_lossy(), "quantity": item.quantity, "garment": item.product, "notes": notes }),
        )?;
        let mut out = json!({ "id": job["id"], "dir": folder.to_string_lossy(), "worksheet": true });
        if !formats.is_empty() {
            out["formats"] = json!(formats);
        }
        s.execute("embroidery.job.output", &out)?;
        Ok(json!({
            "order": item.order, "customer": customer, "product": item.product, "text": text, "font": r["font"], "color": r["color"],
            "job": job["number"], "design": path.to_string_lossy(),
        }))
    });
    // the designs made for it closed again, whatever happened
    while s.documents().len() > docs {
        if s.execute("file.close", &json!({})).is_err() {
            break;
        }
    }
    result
}

fn find_job(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.job.find";
    let code = str_param(p, "code").map(str::trim).filter(|c| !c.is_empty()).ok_or_else(|| bad(CMD, "missing `code`: a job or order number"))?;
    let code: String = code.chars().take(100).collect();
    let jobs = &s.prefs.embroidery_jobs;
    // a job number (a barcode scanner types it), else the shop's order number in its notes
    let order = format!("Order {} ", if code.starts_with('#') { code.clone() } else { format!("#{code}") });
    let job = jobs
        .iter()
        .find(|j| j.number.eq_ignore_ascii_case(&code))
        .or_else(|| jobs.iter().rev().find(|j| j.notes.starts_with(&order)))
        .ok_or_else(|| bad(CMD, format!("no job order {code}")))?;
    let found = serde_json::to_value(job).unwrap_or_default();
    Ok(json!({ "job": found }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written from Shopify's public GraphQL Admin API reference (orders, LineItem).
    fn shopify_answer() -> Value {
        json!({"data": {"orders": {"edges": [
            {"node": {"id": "gid://shopify/Order/1", "name": "#1001", "customer": {"displayName": "Ann Lee"},
             "lineItems": {"edges": [
                {"node": {"id": "gid://shopify/LineItem/11", "title": "Personalized Towel", "sku": "TOWEL-1", "quantity": 2, "unfulfilledQuantity": 2,
                          "customAttributes": [{"key": "Name", "value": "Rose"}, {"key": "Thread colour", "value": "Navy"}, {"key": "Font", "value": "Script"}]}},
                {"node": {"id": "gid://shopify/LineItem/12", "title": "Gift Card", "sku": "", "quantity": 1, "unfulfilledQuantity": 1, "customAttributes": []}},
                {"node": {"id": "gid://shopify/LineItem/13", "title": "Personalized Towel", "sku": "TOWEL-1", "quantity": 1, "unfulfilledQuantity": 0,
                          "customAttributes": [{"key": "Name", "value": "Shipped"}]}}
             ]}}}
        ]}}})
    }

    #[test]
    fn shopify_woocommerce_and_csv_orders_are_read() {
        let s = shopify_items(&shopify_answer());
        assert_eq!(s.len(), 2, "fulfilled items are left out: {s:?}");
        assert_eq!((s[0].order.as_str(), s[0].customer.as_str(), s[0].quantity), ("#1001", "Ann Lee", 2));
        assert_eq!(s[0].fields[0], ("Name".to_string(), "Rose".to_string()));
        // written from WooCommerce's public REST API docs (orders, line_items, meta_data)
        let w = woocommerce_items(&json!([
            {"id": 727, "number": "727", "billing": {"first_name": "Bea", "last_name": "Cruz"},
             "line_items": [{"id": 315, "name": "Baby Blanket", "sku": "BB", "quantity": 1,
                             "meta_data": [{"key": "_reduced_stock", "value": "1"}, {"key": "baby_name", "display_key": "Baby's name", "value": "Leo", "display_value": "Leo"}]}]}
        ]));
        assert_eq!((w[0].order.as_str(), w[0].customer.as_str()), ("#727", "Bea Cruz"));
        assert_eq!(w[0].fields, vec![("Baby's name".to_string(), "Leo".to_string())], "WooCommerce's own meta data left out");
        // an Etsy-style export: Variations hold the personalization
        let c = csv_items("Order ID,Buyer,Item Name,Quantity,Variations\n2001,Cara Diaz,Name Bib,3,\"Personalization:Mia, Colour:Pink\"\n\n");
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].order.as_str(), c[0].customer.as_str(), c[0].product.as_str(), c[0].quantity), ("#2001", "Cara Diaz", "Name Bib", 3));
        assert_eq!(c[0].fields, vec![("Personalization".to_string(), "Mia".to_string()), ("Colour".to_string(), "Pink".to_string())]);
        // a quoted field over two lines, semicolons between fields
        let c = csv_items("Order;Product;Text\n5;Pillow;\"Grandma\nRose\"\n");
        assert_eq!(c[0].fields, vec![("text".to_string(), "Grandma\nRose".to_string())]);
        // junk reads as nothing, never a crash
        assert!(shopify_items(&json!({"data": 5})).is_empty() && woocommerce_items(&json!({"x": 1})).is_empty() && csv_items("").is_empty());
    }

    #[test]
    fn rules_text_fonts_and_colours() {
        let item = &shopify_items(&shopify_answer())[0];
        let any = OrderRule::default();
        assert_eq!(item_text(item, &any), "Rose", "the fields named like text, not the font or colour");
        assert!(any.takes("anything", ""));
        let towel = OrderRule { product: "towel".into(), ..Default::default() };
        assert!(towel.takes("Personalized Towel", "") && !towel.takes("Gift Card", ""));
        assert!(OrderRule { product: "TOWEL-1".into(), ..Default::default() }.takes("x", "towel-1"), "or by SKU");
        let named = OrderRule { text_fields: vec!["thread colour".into()], ..Default::default() };
        assert_eq!(item_text(item, &named), "Navy", "the rule's own fields");
        assert_eq!(colour("Navy").as_deref(), Some("#0b1f44"));
        assert_eq!(colour("#ff0000").as_deref(), Some("#ff0000"));
        assert_eq!(colour("royal"), colour("Royal Blue"));
        assert_eq!(field(item, "", &["colour", "color"]), Some("Navy"));
        assert_eq!(field(item, "Font", &[]), Some("Script"));
    }

    #[test]
    fn orders_become_designs_jobs_and_files_once() {
        let mut s = Session::new();
        let dir = std::env::temp_dir().join(format!("sf-orders-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        // a font of the test's own: no built-in fonts in a plain build
        s.execute("embroidery.fontDoc.fromType", &json!({"family": "Source Sans 3", "chars": "RoseAnLbcdefghijklmnpqtuvwxyz", "size": 20})).unwrap();
        s.execute("embroidery.fontDoc.save", &json!({})).unwrap();
        s.execute("file.close", &json!({})).ok();
        s.execute(
            "embroidery.orders.setup",
            &json!({"shop": "shopify", "folder": dir.to_string_lossy(), "formats": ["dst"], "rules": [{"product": "towel", "template": "Name tape 100 × 30", "color": "Red"}]}),
        )
        .unwrap();
        let open = s.documents().len();
        let dry = s.execute("embroidery.orders.import", &json!({"source": "shopify", "data": shopify_answer(), "dryRun": true})).unwrap();
        assert_eq!(dry["made"][0]["color"], "#0b1f44", "the order's colour comes before the rule's: {dry}");
        assert_eq!(s.documents().len(), open);
        let r = s.execute("embroidery.orders.import", &json!({"source": "shopify", "data": shopify_answer().to_string()})).unwrap();
        assert_eq!(r["made"].as_array().unwrap().len(), 1, "{r}");
        assert_eq!(r["skipped"][0]["why"], "no rule matches this product", "{r}");
        assert_eq!(s.documents().len(), open, "the design is closed again");
        let job = r["made"][0]["job"].as_str().unwrap().to_string();
        let design = r["made"][0]["design"].as_str().unwrap();
        assert!(std::path::Path::new(design).exists(), "{design}");
        let files: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(files.iter().any(|f| f.ends_with(".dst")), "{files:?}");
        // the job: the customer, the quantity, the order in its notes; found by barcode or order
        let found = s.execute("embroidery.job.find", &json!({"code": job})).unwrap();
        assert_eq!((found["job"]["customer"].as_str(), found["job"]["quantity"].as_u64()), (Some("Ann Lee"), Some(2)), "{found}");
        assert_eq!(s.execute("embroidery.job.find", &json!({"code": "1001"})).unwrap()["job"]["number"], job);
        // again: nothing made twice
        let again = s.execute("embroidery.orders.import", &json!({"source": "shopify", "data": shopify_answer()})).unwrap();
        assert!(again["made"].as_array().unwrap().is_empty(), "{again}");
        // bad input fails cleanly
        assert!(s.execute("embroidery.orders.import", &json!({"source": "shopify", "data": "{not json"})).is_err());
        assert!(s.execute("embroidery.orders.import", &json!({"source": "shopify", "data": {"errors": [{"message": "Access denied"}]}})).is_err());
        let partial = json!({"data": {"orders": {"edges": []}}, "errors": [{"message": "Access denied for customer field."}]});
        assert!(s.execute("embroidery.orders.import", &json!({"source": "shopify", "data": partial})).is_ok());
        assert!(s.execute("embroidery.orders.setup", &json!({"rules": [{"color": "plaid"}]})).is_err());
        assert!(s.execute("embroidery.orders.setup", &json!({"formats": ["svg"]})).is_err());
        assert!(s.execute("embroidery.job.find", &json!({"code": "J-9999"})).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
