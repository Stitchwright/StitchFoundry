//! Events the page's own code can't read. Password managers and browser autofill send `keydown`,
//! `keyup` and `keypress` events that aren't keyboard events (a plain `Event` with no `key`), and
//! `input` events with no `inputType`. eframe and our own key listener read those fields as text,
//! and a missing one stopped the app in the middle of signing up. They carry no key the app could
//! use, so they're dropped before any other listener sees them: the guard listens on the window in
//! the capture phase and is added before eframe and [`crate::keys`] add theirs. The autofilled text
//! itself still arrives, through the field's own value.

/// The field an event of this kind must carry as text for the app to read it, if it's one the
/// guard checks.
pub(crate) fn required_text_field(kind: &str) -> Option<&'static str> {
    match kind {
        "keydown" | "keyup" | "keypress" => Some("key"),
        "input" => Some("inputType"),
        _ => None,
    }
}

/// The kinds of event the guard checks.
pub(crate) const GUARDED: [&str; 4] = ["keydown", "keyup", "keypress", "input"];

/// Drop malformed key and input events before the app's listeners see them. Call before
/// `eframe::WebRunner::start` and [`crate::keys::keep_app_keys`].
#[cfg(target_arch = "wasm32")]
pub(crate) fn drop_malformed_events() {
    use wasm_bindgen::JsCast as _;
    let Some(window) = web_sys::window() else { return };
    for kind in GUARDED {
        let Some(field) = required_text_field(kind) else { continue };
        let guard = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            let readable = js_sys::Reflect::get(&e, &wasm_bindgen::JsValue::from_str(field)).is_ok_and(|v| v.is_string());
            if !readable {
                e.stop_immediate_propagation();
            }
        });
        if let Err(err) = window.add_event_listener_with_callback_and_bool(kind, guard.as_ref().unchecked_ref(), true) {
            log::error!("couldn't guard {kind} events: {err:?}");
        }
        // The guard lives as long as the page.
        guard.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_events_need_a_key_and_input_events_an_input_type() {
        for kind in ["keydown", "keyup", "keypress"] {
            assert_eq!(required_text_field(kind), Some("key"), "{kind}");
        }
        assert_eq!(required_text_field("input"), Some("inputType"));
        assert!(GUARDED.iter().all(|kind| required_text_field(kind).is_some()));
    }

    #[test]
    fn other_events_are_left_alone() {
        for kind in ["paste", "pointerdown", "focus", "compositionend", "beforeinput", ""] {
            assert_eq!(required_text_field(kind), None, "{kind}");
        }
    }
}
