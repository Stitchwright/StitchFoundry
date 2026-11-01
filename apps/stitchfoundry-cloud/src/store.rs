//! Accounts, sessions, credits and sew-out checks, kept in memory and written after each change,
//! either to a Postgres database (`DATABASE_URL`, as on Railway) or to the data folder:
//! the state written whole (`sf_state`, or `state.json` through a temporary file so a crash never
//! leaves half a file), and the ledger, every credit added or spent, only ever added to
//! (`sf_ledger`, or `ledger.jsonl`). Shared sew-out checks (photos and rounds) go with them
//! (`sf_shared_*`, or `shared/<check>/`). A single process serves everyone, so one lock is enough.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: u64,
    pub email: String,
    /// The password's Argon2id hash (PHC string).
    pub password: String,
    /// Credits to spend.
    pub credits: u64,
    /// Credits held for requests on their way. Never written: a request's hold lives only as long
    /// as the process serving it, so after a restart (a redeploy) nothing stays held.
    #[serde(skip)]
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

/// A sew-out check (see `sewout.rs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub account: u64,
    /// The month it counts in (`"2026-10"`).
    pub month: String,
    pub created: u64,
    /// The requests it has had.
    pub requests: u32,
    /// Whether it was one of the month's free checks.
    pub free: bool,
    /// Kept to improve StitchFoundry, with the user's leave.
    #[serde(default)]
    pub shared: bool,
    /// The earlier check it follows up ("did the fix work?").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_up: Option<String>,
    /// The credits its requests cost, all told (given back when the user reports it).
    #[serde(default)]
    pub spent: u64,
}

/// A password-reset link on its way: whose, and until when it works.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reset {
    pub account: u64,
    pub expires: u64,
}

/// A sew-out report someone sent (Report to StitchFoundry): whose, and when. Its photos and
/// text are kept as a shared check's are, under its id.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub account: u64,
    pub created: u64,
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
    /// Sew-out checks by id.
    #[serde(default)]
    checks: BTreeMap<String, Check>,
    /// What the free offers cost the service each month (cents, by `"2026-10"`).
    #[serde(default)]
    free_spent: BTreeMap<String, u64>,
    #[serde(default)]
    reports: BTreeMap<String, Report>,
    /// Password-reset links by the SHA-256 of their token (the token itself is never kept).
    #[serde(default)]
    resets: BTreeMap<String, Reset>,
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

/// Shared checks kept in memory: each check's files (name, bytes) and rounds.
type SharedInMemory = Mutex<BTreeMap<String, (Vec<(String, Vec<u8>)>, Vec<String>)>>;

/// Where the store is written.
enum Backend {
    /// Nowhere (tests); shared checks are kept in memory.
    Memory(SharedInMemory),
    Dir(PathBuf),
    Db(Box<Mutex<postgres::Client>>),
}

pub struct Store {
    backend: Backend,
    state: State,
}

/// The tables in a Postgres database.
const SCHEMA: &str = "\
CREATE TABLE IF NOT EXISTS sf_state (id INT PRIMARY KEY, data TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sf_ledger (id BIGSERIAL PRIMARY KEY, at BIGINT NOT NULL, account BIGINT NOT NULL, entry TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sf_shared_files (check_id TEXT NOT NULL, name TEXT NOT NULL, data BYTEA NOT NULL, PRIMARY KEY (check_id, name));
CREATE TABLE IF NOT EXISTS sf_shared_rounds (id BIGSERIAL PRIMARY KEY, check_id TEXT NOT NULL, line TEXT NOT NULL);";

fn db_error(e: postgres::Error) -> StoreError {
    StoreError::Io(format!("database: {e}"))
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
        Ok(Store { backend: Backend::Dir(dir.to_path_buf()), state })
    }

    /// The store in the Postgres database at `url` (its tables made when missing). Railway's
    /// private network needs no TLS; elsewhere, reach the database over a private network too.
    pub fn open_db(url: &str) -> Result<Store, StoreError> {
        let mut client = postgres::Client::connect(url, postgres::NoTls).map_err(db_error)?;
        client.batch_execute(SCHEMA).map_err(db_error)?;
        let row = client.query_opt("SELECT data FROM sf_state WHERE id = 1", &[]).map_err(db_error)?;
        let state = match row {
            Some(r) => {
                let data: String = r.try_get(0).map_err(db_error)?;
                serde_json::from_str(&data).map_err(|e| StoreError::Io(format!("sf_state: {e}")))?
            }
            None => State::default(),
        };
        Ok(Store { backend: Backend::Db(Box::new(Mutex::new(client))), state })
    }

    /// A store that keeps nothing on disk (tests).
    pub fn in_memory() -> Store {
        Store { backend: Backend::Memory(Mutex::default()), state: State::default() }
    }

    fn save(&self) -> Result<(), StoreError> {
        match &self.backend {
            Backend::Memory(_) => Ok(()),
            Backend::Dir(dir) => {
                let bytes = serde_json::to_vec(&self.state).map_err(|e| StoreError::Io(e.to_string()))?;
                let tmp = dir.join("state.json.tmp");
                std::fs::write(&tmp, bytes).map_err(|e| StoreError::Io(e.to_string()))?;
                std::fs::rename(&tmp, dir.join("state.json")).map_err(|e| StoreError::Io(e.to_string()))
            }
            Backend::Db(c) => {
                let data = serde_json::to_string(&self.state).map_err(|e| StoreError::Io(e.to_string()))?;
                let mut c = c.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                c.execute("INSERT INTO sf_state (id, data) VALUES (1, $1) ON CONFLICT (id) DO UPDATE SET data = EXCLUDED.data", &[&data])
                    .map_err(db_error)?;
                Ok(())
            }
        }
    }

    fn log(&mut self, e: Entry) -> Result<(), StoreError> {
        let recent = self.state.recent.entry(e.account).or_default();
        recent.push(e.clone());
        if recent.len() > RECENT {
            let extra = recent.len() - RECENT;
            recent.drain(..extra);
        }
        let line = serde_json::to_string(&e).map_err(|err| StoreError::Io(err.to_string()))?;
        match &self.backend {
            Backend::Memory(_) => {}
            Backend::Dir(dir) => {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(dir.join("ledger.jsonl"))
                    .map_err(|err| StoreError::Io(err.to_string()))?;
                writeln!(f, "{line}").map_err(|err| StoreError::Io(err.to_string()))?;
            }
            Backend::Db(c) => {
                let mut c = c.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let (at, account) = (e.at as i64, e.account as i64);
                c.execute("INSERT INTO sf_ledger (at, account, entry) VALUES ($1, $2, $3)", &[&at, &account, &line]).map_err(db_error)?;
            }
        }
        Ok(())
    }

    // ---------- the free offers' budget ----------

    /// What the free offers have cost the service in `month` (cents).
    pub fn free_spent(&self, month: &str) -> u64 {
        self.state.free_spent.get(month).copied().unwrap_or(0)
    }

    /// Count `cents` more against `month`'s free offers.
    pub fn add_free_spent(&mut self, month: &str, cents: u64) -> Result<(), StoreError> {
        if cents == 0 {
            return Ok(());
        }
        let spent = self.state.free_spent.entry(month.to_string()).or_insert(0);
        *spent = spent.saturating_add(cents);
        self.save()
    }

    // ---------- sew-out checks ----------

    pub fn check(&self, id: &str) -> Option<&Check> {
        self.state.checks.get(id)
    }

    /// Notes a free check before its first request goes out, so that requests made at the same
    /// time count it ([`Store::checks_in`]) and can't all take the month's last free check.
    /// Nothing changes when the check exists already.
    pub fn start_check(&mut self, id: &str, check: Check) -> Result<(), StoreError> {
        if self.state.checks.contains_key(id) {
            return Ok(());
        }
        self.state.checks.insert(id.to_string(), Check { requests: 0, spent: 0, ..check });
        self.save()
    }

    /// Forgets a check [`Store::start_check`] noted whose first request didn't go through (it
    /// has had no requests), giving its free slot back.
    pub fn drop_unstarted_check(&mut self, id: &str) -> Result<(), StoreError> {
        if self.state.checks.get(id).is_some_and(|c| c.requests == 0) {
            self.state.checks.remove(id);
            self.save()?;
        }
        Ok(())
    }

    /// The checks `account` started in `month`.
    pub fn checks_in(&self, account: u64, month: &str) -> usize {
        self.state.checks.values().filter(|c| c.account == account && c.month == month).count()
    }

    /// A new check, or another request of one (its count up, shared once asked).
    pub fn note_check(&mut self, id: &str, check: Check) -> Result<(), StoreError> {
        let c = self.state.checks.entry(id.to_string()).or_insert(Check { requests: 0, spent: 0, ..check.clone() });
        c.requests = c.requests.saturating_add(1);
        c.shared |= check.shared;
        c.spent = c.spent.saturating_add(check.spent);
        self.save()
    }

    /// The ids of `account`'s shared checks (all of them, for deleting).
    pub fn shared_checks(&self, account: u64) -> Vec<String> {
        self.state.checks.iter().filter(|(_, c)| c.account == account && c.shared).map(|(id, _)| id.clone()).collect()
    }

    /// Keep a shared check's `files` (once each) and a round's `line`.
    pub fn keep_shared(&self, check: &str, files: &[(String, Vec<u8>)], line: &str) -> Result<(), StoreError> {
        let io = |e: std::io::Error| StoreError::Io(e.to_string());
        match &self.backend {
            Backend::Memory(m) => {
                let mut m = m.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let (f, lines) = m.entry(check.to_string()).or_default();
                for (name, bytes) in files {
                    if !f.iter().any(|(n, _)| n == name) {
                        f.push((name.clone(), bytes.clone()));
                    }
                }
                lines.push(line.to_string());
            }
            Backend::Dir(dir) => {
                let d = dir.join("shared").join(check);
                std::fs::create_dir_all(&d).map_err(io)?;
                for (name, bytes) in files {
                    let p = d.join(name);
                    if !p.exists() {
                        std::fs::write(p, bytes).map_err(io)?;
                    }
                }
                let mut f = std::fs::OpenOptions::new().create(true).append(true).open(d.join("check.jsonl")).map_err(io)?;
                writeln!(f, "{line}").map_err(io)?;
            }
            Backend::Db(c) => {
                let mut c = c.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                for (name, bytes) in files {
                    c.execute(
                        "INSERT INTO sf_shared_files (check_id, name, data) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
                        &[&check, name, bytes],
                    )
                    .map_err(db_error)?;
                }
                c.execute("INSERT INTO sf_shared_rounds (check_id, line) VALUES ($1, $2)", &[&check, &line]).map_err(db_error)?;
            }
        }
        Ok(())
    }

    /// A shared check's files' names and its rounds (tests; reading the collection).
    pub fn shared(&self, check: &str) -> (Vec<String>, Vec<String>) {
        match &self.backend {
            Backend::Memory(m) => {
                let m = m.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                m.get(check).map(|(f, l)| (f.iter().map(|(n, _)| n.clone()).collect(), l.clone())).unwrap_or_default()
            }
            Backend::Dir(dir) => {
                let d = dir.join("shared").join(check);
                let mut names: Vec<String> = std::fs::read_dir(&d)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .filter(|n| n != "check.jsonl")
                    .collect();
                names.sort();
                let lines = std::fs::read_to_string(d.join("check.jsonl")).unwrap_or_default().lines().map(str::to_string).collect();
                (names, lines)
            }
            Backend::Db(c) => {
                let mut c = c.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                let names = c
                    .query("SELECT name FROM sf_shared_files WHERE check_id = $1 ORDER BY name", &[&check])
                    .map(|rows| rows.iter().filter_map(|r| r.try_get::<_, String>(0).ok()).collect())
                    .unwrap_or_default();
                let lines = c
                    .query("SELECT line FROM sf_shared_rounds WHERE check_id = $1 ORDER BY id", &[&check])
                    .map(|rows| rows.iter().filter_map(|r| r.try_get::<_, String>(0).ok()).collect())
                    .unwrap_or_default();
                (names, lines)
            }
        }
    }

    /// Delete the shared checks `ids` (their photos and rounds; the checks stay counted, no longer
    /// shared) → how many there were.
    pub fn delete_shared(&mut self, ids: &[String]) -> Result<usize, StoreError> {
        let mut n = 0;
        for id in ids {
            let Some(c) = self.state.checks.get_mut(id).filter(|c| c.shared) else { continue };
            c.shared = false;
            n += 1;
            self.forget_kept(id)?;
        }
        self.save()?;
        Ok(n)
    }

    /// Remove what was kept under `id` (a shared check's or a report's photos and text).
    fn forget_kept(&self, id: &str) -> Result<(), StoreError> {
        match &self.backend {
            Backend::Memory(m) => {
                m.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(id);
            }
            Backend::Dir(dir) => {
                let d = dir.join("shared").join(id);
                if d.exists() {
                    std::fs::remove_dir_all(d).map_err(|e| StoreError::Io(e.to_string()))?;
                }
            }
            Backend::Db(c) => {
                let mut c = c.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                c.execute("DELETE FROM sf_shared_files WHERE check_id = $1", &[&id]).map_err(db_error)?;
                c.execute("DELETE FROM sf_shared_rounds WHERE check_id = $1", &[&id]).map_err(db_error)?;
            }
        }
        Ok(())
    }

    // ---------- sew-out reports ----------

    /// Keep report `id`: its `files` and its `text` (as a shared check's), and whose it is.
    pub fn keep_report(&mut self, id: &str, report: Report, files: &[(String, Vec<u8>)], text: &str) -> Result<(), StoreError> {
        self.keep_shared(id, files, text)?;
        self.state.reports.insert(id.to_string(), report);
        self.save()
    }

    /// The ids of `account`'s reports.
    pub fn reports(&self, account: u64) -> Vec<String> {
        self.state.reports.iter().filter(|(_, r)| r.account == account).map(|(id, _)| id.clone()).collect()
    }

    /// How many reports `account` sent since `since`.
    pub fn reports_since(&self, account: u64, since: u64) -> usize {
        self.state.reports.values().filter(|r| r.account == account && r.created >= since).count()
    }

    /// Delete `account`'s reports (their photos and text) → how many there were.
    pub fn delete_reports(&mut self, account: u64) -> Result<usize, StoreError> {
        let ids = self.reports(account);
        for id in &ids {
            self.forget_kept(id)?;
            self.state.reports.remove(id);
        }
        self.save()?;
        Ok(ids.len())
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

    /// A new password-reset link for `account`, working until `RESET_TTL` from `now`. An account
    /// has one link at a time: asking again replaces the earlier one.
    pub fn add_reset(&mut self, token_hash: String, account: u64, now: u64) -> Result<(), StoreError> {
        self.state.resets.retain(|_, r| r.account != account && r.expires > now);
        self.state.resets.insert(token_hash, Reset { account, expires: now.saturating_add(crate::auth::RESET_TTL) });
        self.save()
    }

    /// The account a reset link is for, while it works (it isn't used up by asking).
    pub fn reset_for(&self, token_hash: &str, now: u64) -> Option<u64> {
        self.state.resets.get(token_hash).filter(|r| r.expires > now).map(|r| r.account)
    }

    /// Ends every session made before `t` (Unix seconds) → how many ended.
    pub fn end_sessions_before(&mut self, t: u64) -> Result<usize, StoreError> {
        let before = self.state.sessions.len();
        self.state.sessions.retain(|_, s| s.created >= t);
        let ended = before - self.state.sessions.len();
        if ended > 0 {
            self.save()?;
        }
        Ok(ended)
    }

    /// Uses a reset link: the account gets `password_hash` as its password and every session it
    /// had ends (whoever knew the old password is signed out). `Ok(None)` when the link has
    /// expired or was used.
    pub fn use_reset(&mut self, token_hash: &str, password_hash: String, now: u64) -> Result<Option<u64>, StoreError> {
        let Some(account) = self.reset_for(token_hash, now) else { return Ok(None) };
        let a = self.state.accounts.get_mut(&account).ok_or(StoreError::NoAccount)?;
        a.password = password_hash;
        self.state.resets.retain(|_, r| r.account != account);
        self.state.sessions.retain(|_, s| s.account != account);
        self.save()?;
        Ok(Some(account))
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

    /// Credits held when the service stops (a redeploy mid-request) aren't held after it starts
    /// again: the request that held them is gone, so all the credits can be spent.
    #[test]
    fn held_credits_are_not_held_after_a_restart() {
        let dir = std::env::temp_dir().join(format!("sf-cloud-store-held-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let id = {
            let mut s = Store::open(&dir).unwrap();
            let id = s.create("a@b.c", "hash".into(), 100, 1000).unwrap();
            s.reserve(id, 80).unwrap();
            // any later change writes the state, the hold with it if it were kept
            s.add_session("t1".into(), id, 1001).unwrap();
            id
        };
        let mut s = Store::open(&dir).unwrap();
        assert_eq!(s.account(id).unwrap().reserved, 0, "no request is on its way after a restart");
        assert_eq!(s.reserve(id, 100), Ok(()), "every credit can be held again");
        // a state written by an earlier version, with the hold in it, is read without it
        let old = std::fs::read_to_string(dir.join("state.json")).unwrap().replace("\"credits\":100", "\"credits\":100,\"reserved\":80");
        assert!(old.contains("\"reserved\":80"));
        std::fs::write(dir.join("state.json"), old).unwrap();
        let s = Store::open(&dir).unwrap();
        assert_eq!(s.account(id).unwrap().reserved, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// After a breach: sessions made before the time end, later ones go on.
    #[test]
    fn sessions_made_before_a_time_end() {
        let mut s = Store::in_memory();
        let id = s.create("a@b.c", "pw".into(), 0, 0).unwrap();
        s.add_session("old".into(), id, 1000).unwrap();
        s.add_session("new".into(), id, 5000).unwrap();
        assert_eq!(s.end_sessions_before(3000).unwrap(), 1);
        assert_eq!((s.session("old", 6000), s.session("new", 6000)), (None, Some(id)));
        assert_eq!(s.end_sessions_before(3000).unwrap(), 0, "a restart ends nothing more");
    }

    #[test]
    fn a_reset_link_works_once_for_an_hour_and_ends_the_sessions() {
        let mut s = Store::in_memory();
        let id = s.create("a@b.c", "old".into(), 0, 0).unwrap();
        let other = s.create("d@e.f", "theirs".into(), 0, 0).unwrap();
        s.add_session("t1".into(), id, 0).unwrap();
        s.add_session("t2".into(), other, 0).unwrap();
        s.add_reset("r1".into(), id, 0).unwrap();
        assert_eq!(s.reset_for("r1", 10), Some(id));
        assert_eq!(s.reset_for("r1", crate::auth::RESET_TTL), None, "an hour");
        assert_eq!(s.use_reset("r1", "new".into(), crate::auth::RESET_TTL).unwrap(), None);
        assert_eq!(s.account(id).unwrap().password, "old", "an expired link changes nothing");
        // asking again replaces the earlier link
        s.add_reset("r2".into(), id, 100).unwrap();
        s.add_reset("r3".into(), id, 200).unwrap();
        assert_eq!(s.reset_for("r2", 300), None);
        assert_eq!(s.use_reset("r3", "new".into(), 300).unwrap(), Some(id));
        assert_eq!(s.account(id).unwrap().password, "new");
        assert_eq!(s.session("t1", 300), None, "signed out everywhere");
        assert_eq!(s.session("t2", 300), Some(other), "other accounts stay signed in");
        assert_eq!(s.use_reset("r3", "again".into(), 301).unwrap(), None, "once");
    }

    /// The Postgres store, with a database to try it on (`SF_CLOUD_TEST_DATABASE_URL`, whose
    /// StitchFoundry tables it empties); skipped without one.
    #[test]
    fn the_database_keeps_accounts_credits_and_shared_checks() {
        let Ok(url) = std::env::var("SF_CLOUD_TEST_DATABASE_URL") else { return };
        let mut c = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        c.batch_execute("DROP TABLE IF EXISTS sf_state, sf_ledger, sf_shared_files, sf_shared_rounds").unwrap();
        let id = {
            let mut s = Store::open_db(&url).unwrap();
            let id = s.create("a@b.c", "hash".into(), 100, 1000).unwrap();
            s.add_session("t1".into(), id, 1000).unwrap();
            assert!(s.add_credits(id, 1000, "purchase", "cs_1", "small pack", 1001).unwrap());
            let check =
                Check { account: id, month: "2026-10".into(), created: 1000, requests: 0, free: true, shared: true, follow_up: None, spent: 0 };
            s.note_check("1-aaaaaaaa", check).unwrap();
            s.keep_shared("1-aaaaaaaa", &[("image-1.jpg".into(), b"hello".to_vec())], "{\"said\":[\"overlap\"]}").unwrap();
            s.keep_shared("1-aaaaaaaa", &[("image-1.jpg".into(), b"again".to_vec())], "{}").unwrap();
            id
        };
        let mut s = Store::open_db(&url).unwrap();
        assert_eq!(s.account(id).unwrap().credits, 1100);
        assert_eq!(s.session("t1", 2000), Some(id));
        assert_eq!(s.checks_in(id, "2026-10"), 1);
        let (files, rounds) = s.shared("1-aaaaaaaa");
        assert_eq!(files, ["image-1.jpg"]);
        assert_eq!(rounds.len(), 2);
        let n: i64 = c.query_one("SELECT count(*) FROM sf_ledger", &[]).unwrap().get(0);
        assert_eq!(n, 2, "signup and purchase");
        let photo: Vec<u8> = c.query_one("SELECT data FROM sf_shared_files", &[]).unwrap().get(0);
        assert_eq!(photo, b"hello", "kept once");
        assert_eq!(s.delete_shared(&["1-aaaaaaaa".into()]).unwrap(), 1);
        assert!(s.shared("1-aaaaaaaa").0.is_empty());
        assert!(s.shared_checks(id).is_empty());
    }
}
