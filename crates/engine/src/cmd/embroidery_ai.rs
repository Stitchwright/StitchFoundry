//! The AI features' Effort (AI-05): how hard the model works on Design from Prompt and Redraw
//! Image, kept in preferences. Higher effort thinks longer, which costs more; the dialog shows
//! the estimate (`sf_core::ai::effort`).

use serde_json::{Value, json};
use sf_core::ai::Effort;

use super::*;

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        query "embroidery.aiEffort",
        "AI Effort",
        [],
        None,
        "{effort?: quick|normal|thorough} how hard the AI features work (normal: the model's own default; quick costs least; thorough thinks longest, at about twice normal's cost), kept for next time; without params, the effort set → {effort}",
        always,
        ai_effort
    )]
}

/// The effort set in `prefs` (normal when none or unknown).
pub fn effort(prefs: &crate::Prefs) -> Effort {
    Effort::from_id(&prefs.embroidery_ai_effort).unwrap_or_default()
}

fn ai_effort(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "embroidery.aiEffort";
    if let Some(v) = p.get("effort").filter(|v| !v.is_null()) {
        let e = v.as_str().and_then(Effort::from_id).ok_or_else(|| bad(CMD, "`effort` is quick, normal or thorough"))?;
        s.prefs.embroidery_ai_effort = if e == Effort::Normal { String::new() } else { e.id().to_string() };
    }
    Ok(json!({ "effort": effort(&s.prefs).id() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_effort_is_kept_and_checked() {
        let mut s = Session::new();
        assert_eq!(s.execute("embroidery.aiEffort", &json!({})).unwrap()["effort"], "normal");
        assert_eq!(s.execute("embroidery.aiEffort", &json!({"effort": "Thorough"})).unwrap()["effort"], "thorough");
        assert_eq!(effort(&s.prefs), Effort::Thorough);
        assert!(s.execute("embroidery.aiEffort", &json!({"effort": "max"})).is_err());
        assert_eq!(s.execute("embroidery.aiEffort", &json!({})).unwrap()["effort"], "thorough");
        s.execute("embroidery.aiEffort", &json!({"effort": "normal"})).unwrap();
        assert!(s.prefs.embroidery_ai_effort.is_empty(), "normal is the default: nothing kept");
    }
}
