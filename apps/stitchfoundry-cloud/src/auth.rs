//! Signing in: passwords kept only as Argon2id hashes, session tokens of 32 random bytes kept
//! only as their SHA-256, and a limit on wrong passwords so they can't be guessed quickly.

use std::collections::HashMap;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use sha2::{Digest, Sha256};

/// The shortest and longest password accepted.
pub const PASSWORD_LEN: (usize, usize) = (8, 200);
/// How long a password-reset link works (seconds): an hour, once.
pub const RESET_TTL: u64 = 3600;
/// Wrong passwords allowed for one email within [`WINDOW`] seconds; also the reset emails one
/// address may be sent in that time.
pub const MAX_FAILURES: usize = 5;
pub const WINDOW: u64 = 15 * 60;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// `email` tidied (trimmed, lower case), if it looks like an address.
pub fn clean_email(email: &str) -> Option<String> {
    let e = email.trim().to_lowercase();
    let (user, domain) = e.split_once('@')?;
    let ok = !user.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && e.len() <= 254
        && !e.chars().any(|c| c.is_whitespace() || c.is_control());
    ok.then_some(e)
}

/// Why `password` can't be used, if it can't.
pub fn check_password(password: &str) -> Result<(), String> {
    let n = password.chars().count();
    if n < PASSWORD_LEN.0 {
        return Err(format!("a password needs at least {} characters", PASSWORD_LEN.0));
    }
    if n > PASSWORD_LEN.1 {
        return Err(format!("a password has at most {} characters", PASSWORD_LEN.1));
    }
    Ok(())
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| format!("no randomness: {e}"))?;
    Ok(b)
}

/// The Argon2id hash of `password`, with a random salt.
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::encode_b64(&random::<16>()?).map_err(|e| e.to_string())?;
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| e.to_string())
}

/// Whether `password` is the one `hash` was made from.
pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(password.as_bytes(), &h).is_ok())
}

/// A new session token (given to the app) and its hash (kept).
pub fn new_token() -> Result<(String, String), String> {
    let token = format!("sfs_{}", hex(&random::<32>()?));
    let hash = token_hash(&token);
    Ok((token, hash))
}

/// A new password-reset token (emailed in a link) and its hash (kept).
pub fn new_reset_token() -> Result<(String, String), String> {
    let token = format!("sfr_{}", hex(&random::<32>()?));
    let hash = token_hash(&token);
    Ok((token, hash))
}

/// Whether `token` has the shape of a reset token (so it can go into a page as it is).
pub fn is_reset_token(token: &str) -> bool {
    token.strip_prefix("sfr_").is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The SHA-256 of a token, as kept.
pub fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// Wrong passwords lately, by email.
#[derive(Default)]
pub struct Throttle {
    failures: HashMap<String, Vec<u64>>,
}

impl Throttle {
    /// Whether `email` may try a password now.
    pub fn allowed(&mut self, email: &str, now: u64) -> bool {
        let list = self.failures.entry(email.to_string()).or_default();
        list.retain(|t| now.saturating_sub(*t) < WINDOW);
        list.len() < MAX_FAILURES
    }

    pub fn failed(&mut self, email: &str, now: u64) {
        self.failures.entry(email.to_string()).or_default().push(now);
        // keep the map from growing without end
        if self.failures.len() > 100_000 {
            self.failures.retain(|_, l| l.iter().any(|t| now.saturating_sub(*t) < WINDOW));
        }
    }

    pub fn succeeded(&mut self, email: &str) {
        self.failures.remove(email);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_are_hashed_and_checked() {
        let h = hash_password("correct horse").unwrap();
        assert!(h.starts_with("$argon2id$") && !h.contains("correct"));
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong horse!", &h));
        assert!(!verify_password("x", "not a hash"));
        assert_ne!(hash_password("correct horse").unwrap(), h, "a salt each time");
        assert!(check_password("short").is_err() && check_password("long enough").is_ok());
    }

    #[test]
    fn emails_tokens_and_the_throttle() {
        assert_eq!(clean_email("  Denise@Example.COM "), Some("denise@example.com".into()));
        for bad in ["", "no-at", "@x.com", "a@nodot", "a@.com", "a b@c.com"] {
            assert_eq!(clean_email(bad), None, "{bad}");
        }
        let (t, h) = new_token().unwrap();
        assert!(t.starts_with("sfs_") && t.len() == 68);
        assert_eq!(token_hash(&t), h);
        assert_ne!(new_token().unwrap().0, t);
        let (r, rh) = new_reset_token().unwrap();
        assert!(is_reset_token(&r) && r.len() == 68 && token_hash(&r) == rh);
        for bad in ["", "sfr_", "sfs_00", "sfr_<script>", &t, &format!("{r}0")] {
            assert!(!is_reset_token(bad), "{bad}");
        }
        let mut th = Throttle::default();
        for i in 0..MAX_FAILURES as u64 {
            assert!(th.allowed("a@b.c", i));
            th.failed("a@b.c", i);
        }
        assert!(!th.allowed("a@b.c", 10), "five wrong in a row: wait");
        assert!(th.allowed("a@b.c", 10 + WINDOW), "until the window passes");
        assert!(th.allowed("other@b.c", 10));
    }
}
