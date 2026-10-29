//! The worksheet setup (RP-01): the shop's name and details in each worksheet's footer, its logo
//! beside them, and the sections it leaves out; kept across sessions.

use serde_json::{Value, json};

use super::{CommandSpec, Result, Session, always, bad, cmd};

/// The sections a worksheet can leave out.
pub const SECTIONS: [&str; 5] = ["figures", "colors", "colorways", "notes", "barcode"];

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "embroidery.worksheet.setup",
        "Worksheet Setup",
        [],
        None,
        "{company?: the shop's name, details?: address, phone, web (a line or two), logo?: an image file (PNG, JPEG…; \"\" for none), hide?: [figures|colors|colorways|notes|barcode]} what every worksheet carries: the company and its details in the footer with the logo beside them, and the sections it leaves out; without params, the setup as it is → {company, details, logo, hide}",
        always,
        setup
    )]
}

/// What every worksheet carries (`embroidery.worksheet.setup`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SheetSetup {
    pub company: String,
    pub details: String,
    /// The logo's image file.
    pub logo: String,
    /// Sections left out.
    pub hide: Vec<String>,
}

impl SheetSetup {
    pub fn is_empty(&self) -> bool {
        *self == SheetSetup::default()
    }
}

fn setup(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.worksheet.setup";
    let mut st = s.prefs.embroidery_sheet_setup.clone();
    let text = |k: &str, max: usize| p.get(k).and_then(Value::as_str).map(|t| t.trim().chars().take(max).collect::<String>());
    if let Some(t) = text("company", 120) {
        st.company = t;
    }
    if let Some(t) = text("details", 300) {
        st.details = t;
    }
    if let Some(t) = text("logo", 2000) {
        st.logo = t;
    }
    match p.get("hide") {
        None | Some(Value::Null) => {}
        Some(Value::Array(a)) => {
            let mut hide = Vec::new();
            for v in a {
                let id = v.as_str().filter(|id| SECTIONS.contains(id)).ok_or_else(|| bad(CMD, format!("`hide` takes {}", SECTIONS.join(", "))))?;
                if !hide.iter().any(|h: &String| h == id) {
                    hide.push(id.to_string());
                }
            }
            st.hide = hide;
        }
        Some(_) => return Err(bad(CMD, "`hide` is a list of sections")),
    }
    s.prefs.embroidery_sheet_setup = st.clone();
    Ok(json!({ "company": st.company, "details": st.details, "logo": st.logo, "hide": st.hide }))
}
