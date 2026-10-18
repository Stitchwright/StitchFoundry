//! Accounts, sessions and credits, kept in memory and written to the data folder after each
//! change: `state.json` (written whole, through a temporary file, so a crash never leaves half a
//! file) and `ledger.jsonl`, every credit added or spent, one line each, only ever added to. A
//! single process serves everyone, so one lock is enough; a database replaces this when the
//! service outgrows one machine.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: u64,
    pub email: String,
    /// The password's Argon2id hash (PHC string).
    pub password: String,
    /// Credits to spend.
    pub credits: u64,
    /// Credits held for requests on their way.
    #[serde(default)]
    pub reserved: u64,
    pub created: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub account: u64,
    pub created: u64,
    pub last_used: u64,
}

/// One change of an account's credits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub at: u64,
    pub account: u64,
    /// `signup`, `purchase`, `ai` or `grant`.
    pub kind: String,
    /// Added (positive) or spent (negative).
    pub credits: i64,
    /// The balance after it.
    pub balance: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    next_id: u64,
    accounts: BTreeMap<u64, Account>,
    /// Sessions by the SHA-256 of their token (the token itself is never kept).
    sessions: BTreeMap<String, Session>,
    /// Stripe events and checkout sessions already credited.
    paid: BTreeSet<String>,
    /// The latest entries of each account (the whole history is in the ledger file).
    #[serde(default)]
    recent: BTreeMap<u64, Vec<Entry>>,
}

/// The most recent entries kept for an account's history.
const RECENT: usize = 50;
/// Sessions last this long unused (seconds): 90 days.
pub const SESSION_TTL: u64 = 90 * 24 * 3600;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StoreError {
    #[error("an account with that email already exists")]
    Taken,
    #[error("no such account")]
    NoAccount,
    #[error("not enough credits")]
    NotEnough,
    #[error("couldn't save: {0}")]
    Io(String),
}

pub struct Store {
    dir: Option<PathBuf>,
    state: State,
}

impl Store {
    /// The store in folder `dir` (made when missing), read from what it holds.
    pub fn open(dir: &Path) -> Result<Store, StoreError> {
        std::fs::create_dir_all(dir).map_err(|e| StoreError::Io(e.to_string()))?;
        let path = dir.join("state.json");
        let state = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| StoreError::Io(format!("{}: {e}", path.display())))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => return Err(StoreError::Io(e.to_string())),
        };
        Ok(Store { dir: Some(dir.to_path_buf()), state })
    }

    /// A store that keeps nothing on disk (tests).
    pub fn in_memory() -> Store {
        Store { dir: None, state: State::default() }
    }

    fn save(&self) -> Result<(), StoreError> {
        let Some(dir) = &self.dir else { return Ok(()) };
        let bytes = serde_json::to_vec(&self.state).map_err(|e| StoreError::Io(e.to_string()))?;
        let tmp = dir.join("state.json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| StoreError::Io(e.to_string()))?;
        std::fs::rename(&tmp, dir.join("state.json")).map_err(|e| StoreError::Io(e.to_string()))
    }

    fn log(&mut self, e: Entry) -> Result<(), StoreError> {
        let recent = self.state.recent.entry(e.account).or_default();
        recent.push(e.clone());
        if recent.len() > RECENT {
            let extra = recent.len() - RECENT;
            recent.drain(..extra);
        }
        if let Some(dir) = &self.dir {
            let line = serde_json::to_string(&e).map_err(|err| StoreError::Io(err.to_string()))?;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("ledger.jsonl"))
                .map_err(|err| StoreError::Io(err.to_string()))?;
            writeln!(f, "{line}").map_err(|err| StoreError::Io(err.to_string()))?;
        }
        Ok(())
    }

    pub fn account(&self, id: u64) -> Option<&Account> {
        self.state.accounts.get(&id)
    }

    pub fn by_email(&self, email: &str) -> Option<&Account> {
        self.state.accounts.values().find(|a| a.email == email)
    }

    pub fn recent(&self, id: u64) -> &[Entry] {
        self.state.recent.get(&id).map_or(&[], Vec::as_slice)
    }

    /// A new account with `credits` to start.
    pub fn create(&mut self, email: &str, password_hash: String, credits: u64, now: u64) -> Result<u64, StoreError> {
        if self.by_email(email).is_some() {
            return Err(StoreError::Taken);
        }
        self.state.next_id += 1;
        let id = self.state.next_id;
        self.state.accounts.insert(id, Account { id, email: email.to_string(), password: password_hash, credits, reserved: 0, created: now });
        if credits > 0 {
            self.log(Entry { at: now, account: id, kind: "signup".into(), credits: credits as i64, balance: credits, note: String::new() })?;
        }
        self.save()?;
        Ok(id)
    }

    pub fn add_session(&mut self, token_hash: String, account: u64, now: u64) -> Result<(), StoreError> {
        // drop sessions gone stale on the way
        self.state.sessions.retain(|_, s| now.saturating_sub(s.last_used) < SESSION_TTL);
        self.state.sessions.insert(token_hash, Session { account, created: now, last_used: now });
        self.save()
    }

    /// The account a session token's hash stands for, while the session lasts.
    pub fn session(&mut self, token_hash: &str, now: u64) -> Option<u64> {
        let s = self.state.sessions.get_mut(token_hash)?;
        if now.saturating_sub(s.last_used) >= SESSION_TTL {
            return None;
        }
        // (kept in memory; written with the next change)
        s.last_used = now;
        Some(s.account)
    }

    pub fn end_session(&mut self, token_hash: &str) -> Result<(), StoreError> {
        self.state.sessions.remove(token_hash);
        self.save()
    }

    /// Credits added (a purchase or a grant), once for each `key` (a Stripe event or checkout).
    /// `Ok(false)` when that key was credited already.
    pub fn add_credits(&mut self, account: u64, credits: u64, kind: &str, key: &str, note: &str, now: u64) -> Result<bool, StoreError> {
        if !key.is_empty() && self.state.paid.contains(key) {
            return Ok(false);
        }
        let a = self.state.accounts.get_mut(&account).ok_or(StoreError::NoAccount)?;
        a.credits = a.credits.saturating_add(credits);
        let balance = a.credits;
        if !key.is_empty() {
            self.state.paid.insert(key.to_string());
        }
        self.log(Entry { at: now, account, kind: kind.into(), credits: credits as i64, balance, note: note.into() })?;
        self.save()?;
        Ok(true)
    }

    /// Holds `credits` of the account's for a request on its way: refused when the credits not held
    /// already don't cover it.
    pub fn reserve(&mut self, account: u64, credits: u64) -> Result<(), StoreError> {
        let a = self.state.accounts.get_mut(&account).ok_or(StoreError::NoAccount)?;
        if a.credits.saturating_sub(a.reserved) < credits {
            return Err(StoreError::NotEnough);
        }
        a.reserved += credits;
        Ok(())
    }

    /// The request held `reserved` is done and cost `spent` (at most what was held): the hold goes,
    /// what it cost is taken.
    pub fn settle(&mut self, account: u64, reserved: u64, spent: u64, note: &str, now: u64) -> Result<u64, StoreError> {
        let a = self.state.accounts.get_mut(&account).ok_or(StoreError::NoAccount)?;
        a.reserved = a.reserved.saturating_sub(reserved);
        let spent = spent.min(a.credits);
        a.credits -= spent;
        let balance = a.credits;
        if spent > 0 {
            self.log(Entry { at: now, account, kind: "ai".into(), credits: -(spent as i64), balance, note: note.into() })?;
            self.save()?;
        }
        Ok(balance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_sessions_and_credits_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("sf-cloud-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let id = {
            let mut s = Store::open(&dir).unwrap();
            let id = s.create("a@b.c", "hash".into(), 50, 1000).unwrap();
            assert_eq!(s.create("a@b.c", "h".into(), 0, 1000), Err(StoreError::Taken));
            s.add_session("t1".into(), id, 1000).unwrap();
            assert!(s.add_credits(id, 500, "purchase", "cs_1", "small pack", 1001).unwrap());
            assert!(!s.add_credits(id, 500, "purchase", "cs_1", "small pack", 1002).unwrap(), "once a checkout");
            id
        };
        let mut s = Store::open(&dir).unwrap();
        assert_eq!(s.account(id).unwrap().credits, 550);
        assert_eq!(s.session("t1", 2000), Some(id));
        assert_eq!(s.session("t1", 2000 + SESSION_TTL), None, "a session unused for 90 days ends");
        let ledger = std::fs::read_to_string(dir.join("ledger.jsonl")).unwrap();
        assert_eq!(ledger.lines().count(), 2);
        assert_eq!(s.recent(id).len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn credits_are_held_for_a_request_then_only_what_it_cost_is_taken() {
        let mut s = Store::in_memory();
        let id = s.create("a@b.c", "h".into(), 100, 0).unwrap();
        s.reserve(id, 80).unwrap();
        assert_eq!(s.reserve(id, 30), Err(StoreError::NotEnough), "80 of 100 held already");
        assert_eq!(s.settle(id, 80, 12, "ask", 1).unwrap(), 88);
        assert_eq!(s.account(id).unwrap().reserved, 0);
        // a failed request costs nothing
        s.reserve(id, 50).unwrap();
        assert_eq!(s.settle(id, 50, 0, "ask", 2).unwrap(), 88);
        assert_eq!(s.recent(id).iter().filter(|e| e.kind == "ai").count(), 1);
    }
}
