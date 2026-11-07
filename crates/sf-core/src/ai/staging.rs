//! The staging switch for the web app: opening it with `?services=staging` makes it use the
//! staging account service and staging MakerFoundry (so release tests never create accounts or
//! designs on production). The staging addresses are compiled in, exact and HTTPS; no address
//! is ever taken from the page's address. As elsewhere in [`super`], no network code lives here.

use std::sync::atomic::{AtomicBool, Ordering};

/// The staging account service.
pub const CLOUD: &str = "https://cloud-staging-staging.up.railway.app";
/// The staging MakerFoundry.
pub const MAKERFOUNDRY: &str = "https://makerfoundry-test-test.up.railway.app";
/// The staging MakerFoundry's host, the one extra host `?open=` accepts in staging mode.
pub const MAKERFOUNDRY_HOST: &str = "makerfoundry-test-test.up.railway.app";
/// The banner shown while staging is on.
pub const BANNER: &str = "STAGING services (test data)";

static ON: AtomicBool = AtomicBool::new(false);

/// Whether the `services=` value asks for staging: exactly `staging`; anything else is ignored.
pub fn requested(value: Option<&str>) -> bool {
    value == Some("staging")
}

/// Whether this tab is in staging: the page's `services=` value when it asks for it, else what
/// the tab's session remembered (`remembered` is the value kept earlier). An unknown value
/// changes nothing.
pub fn chosen(param: Option<&str>, remembered: Option<&str>) -> bool {
    requested(param) || requested(remembered)
}

/// Turn staging on or off for this tab.
pub fn set(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

/// Whether this tab uses the staging services.
pub fn active() -> bool {
    ON.load(Ordering::Relaxed)
}

/// The account service's address in staging mode.
pub fn cloud() -> Option<&'static str> {
    active().then_some(CLOUD)
}

/// MakerFoundry's address in staging mode.
pub fn makerfoundry() -> Option<&'static str> {
    active().then_some(MAKERFOUNDRY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_exact_word_asks_for_staging() {
        assert!(requested(Some("staging")));
        for no in [None, Some(""), Some("Staging"), Some("production"), Some("staging "), Some("https://evil.example"), Some("prod")] {
            assert!(!requested(no), "{no:?}");
        }
    }

    #[test]
    fn staging_is_remembered_and_unknown_values_change_nothing() {
        assert!(chosen(Some("staging"), None));
        assert!(chosen(None, Some("staging")));
        assert!(chosen(Some("nonsense"), Some("staging")), "an unknown value doesn't switch it off");
        assert!(!chosen(Some("nonsense"), None));
        assert!(!chosen(None, None));
    }

    #[test]
    fn the_addresses_are_https_and_exact() {
        assert!(CLOUD.starts_with("https://") && MAKERFOUNDRY.starts_with("https://"));
        assert_eq!(MAKERFOUNDRY, format!("https://{MAKERFOUNDRY_HOST}"));
    }

    // One test holds the switch (it is process-wide), so tests can't race on it.
    #[test]
    fn production_is_the_default_and_the_switch_changes_both_addresses() {
        set(false);
        assert!(!active() && cloud().is_none() && makerfoundry().is_none());
        set(true);
        assert_eq!((cloud(), makerfoundry()), (Some(CLOUD), Some(MAKERFOUNDRY)));
        set(false);
        assert!(cloud().is_none());
    }
}
