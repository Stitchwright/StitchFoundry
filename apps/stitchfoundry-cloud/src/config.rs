//! The service's settings, read from the environment when it starts. Secrets (the Anthropic and
//! Stripe keys) live only here, set by whoever runs the service, never in the repository.

use std::path::PathBuf;

/// A pack of credits for sale: what it is called, how many credits, and its price in cents.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Pack {
    pub id: String,
    pub credits: u64,
    pub cents: u64,
}

/// What a model costs, in US dollars a million tokens.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelPrice {
    pub model: String,
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Where to listen (`SF_CLOUD_ADDR`, default `127.0.0.1:8080`, behind a TLS proxy).
    pub addr: String,
    /// Where accounts, sessions and the credit ledger are kept (`SF_CLOUD_DATA`).
    pub data_dir: PathBuf,
    /// The service's own Anthropic key (`ANTHROPIC_API_KEY`); without it AI requests are refused.
    pub anthropic_key: Option<String>,
    /// Stripe's secret key (`STRIPE_SECRET_KEY`) and the webhook's signing secret
    /// (`STRIPE_WEBHOOK_SECRET`); without them nothing can be bought.
    pub stripe_key: Option<String>,
    pub stripe_webhook_secret: Option<String>,
    /// The address people reach the service at (`SF_CLOUD_PUBLIC_URL`), for Stripe's return pages.
    pub public_url: String,
    /// Credits a new account starts with (`SF_CLOUD_SIGNUP_CREDITS`, default 100).
    pub signup_credits: u64,
    /// What one credit sells for, in cents (`SF_CLOUD_CENTS_PER_CREDIT`, default 1: a credit is a
    /// cent), and how much over the AI's own cost a request is charged (`SF_CLOUD_MARKUP`, 3).
    pub cents_per_credit: f64,
    pub markup: f64,
    /// The packs on sale (`SF_CLOUD_PACKS`: `id:credits:cents,…`).
    pub packs: Vec<Pack>,
    /// The models requests may name.
    pub allowed: Vec<String>,
    /// What each model a reply may come from costs (a request the model declines can be answered
    /// by another one).
    pub models: Vec<ModelPrice>,
    /// The most tokens one reply may use.
    pub max_tokens: u64,
    /// Sew-out checks free each month (`SF_CLOUD_FREE_CHECKS`, 3) and what a check costs after
    /// them (`SF_CLOUD_CHECK_CREDITS`, 10).
    pub free_checks: u64,
    pub check_credits: u64,
    /// The most the month's free offers may cost the service in AI (`SF_CLOUD_FREE_BUDGET_CENTS`,
    /// 2500: $25): sign-up credits, counted at what they could buy, and free sew-out checks, at what
    /// they used. Once it is reached, new accounts start without credits and checks are paid until
    /// the next month.
    pub free_budget_cents: u64,
    /// The Postgres database (`DATABASE_URL`, as Railway sets it); without it the data folder.
    pub database_url: Option<String>,
    /// Where sew-out reports are emailed (`SF_CLOUD_REPORTS_TO`), through Resend
    /// (`RESEND_API_KEY`), from `SF_CLOUD_REPORTS_FROM` (an address on a domain Resend has
    /// verified); without them reports are only kept.
    pub resend_key: Option<String>,
    pub reports_to: Option<String>,
    pub reports_from: String,
    /// Who account emails (password resets) come from (`SF_CLOUD_MAIL_FROM`, an address on a
    /// domain Resend has verified), through Resend; without `RESEND_API_KEY` there are none.
    pub mail_from: String,
    /// A staging service's test mailbox (`SF_CLOUD_MAIL_CATCHER_URL`, Mailpit's address, e.g.
    /// `http://mailpit.railway.internal:8025`): every email goes there instead of through Resend
    /// ([`crate::mail_catcher`]). Setting it turns email on by itself; `RESEND_API_KEY` is then
    /// ignored, so a real key can never travel with a staging email.
    pub mail_catcher: Option<String>,
    /// The commit this service was built from (`RAILWAY_GIT_COMMIT_SHA`, set by Railway: hex
    /// digits only), shown by `/v1/health` so a staging test run can say which commit passed.
    pub commit: Option<String>,
    /// Sessions made before this time (Unix seconds) end when the service starts
    /// (`SF_CLOUD_END_SESSIONS_BEFORE`: `2026-10-22T09:00:00Z`, a date, or Unix seconds): set it
    /// with new keys after a breach, and everyone signs in again. Later restarts end nothing more.
    pub end_sessions_before: Option<u64>,
    /// Test mode (`SF_CLOUD_TEST_MODE`, off by default), for a staging copy only: mounts
    /// `/v1/test/provision` and `/v1/test/cleanup`, which make and delete verified test accounts
    /// for a QA bot. [`Config::from_env`] refuses to start with it on when the setup looks like
    /// production ([`production_guard`]).
    pub test_mode: bool,
    /// The secret those routes need (`SF_CLOUD_TEST_SECRET`, at least [`MIN_TEST_SECRET`]
    /// characters), sent as `X-Test-Secret`; only read in test mode.
    pub test_secret: Option<String>,
    /// Which copy of the service this is (`SF_CLOUD_ENV`): test mode needs `staging`.
    pub env: Option<String>,
}

/// The shortest test secret accepted.
pub const MIN_TEST_SECRET: usize = 32;
/// The production service's host: test mode never starts with this public address.
pub const PRODUCTION_HOST: &str = "cloud.stitchfoundry.app";
/// What a database's host or name must contain for test mode to use it.
pub const STAGING_MARKER: &str = "staging";

/// Whether `text` turns a switch on (`1`, `true`, `on`, `yes`) or off (`0`, `false`, `off`, `no`,
/// empty); anything else is refused, so a typo never leaves it in a state nobody meant.
fn switch(name: &str, text: Option<String>) -> Result<bool, String> {
    match text.as_deref().map(|t| t.trim().to_ascii_lowercase()).as_deref() {
        None | Some("" | "0" | "false" | "off" | "no") => Ok(false),
        Some("1" | "true" | "on" | "yes") => Ok(true),
        Some(_) => Err(format!("{name} is on (1, true) or off (0, false)")),
    }
}

/// The host of a public address (`https://Cloud.Example.com:443/` → `cloud.example.com`).
fn url_host(url: &str) -> String {
    let u = url.trim().to_ascii_lowercase();
    let rest = u.split_once("://").map_or(u.as_str(), |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host_port.split(':').next().unwrap_or_default();
    host.trim_end_matches('.').to_string()
}

/// Why test mode can't start with these settings, if it can't. It fails closed: test mode starts
/// only on an explicit staging setup, and never when anything looks like production.
///
/// - `SF_CLOUD_ENV` must be `staging`.
/// - `SF_CLOUD_TEST_SECRET` must be at least [`MIN_TEST_SECRET`] characters.
/// - `SF_CLOUD_PUBLIC_URL` must not be the production address (`cloud.stitchfoundry.app`, in any
///   case, scheme, port or trailing slash).
/// - `STRIPE_SECRET_KEY` must not be a live key (`sk_live_…`, `rk_live_…`).
/// - Railway's `RAILWAY_ENVIRONMENT_NAME`, when set, must not be `production`.
/// - The database: production keeps its data in Postgres (`DATABASE_URL`), and a staging copy's
///   database could hold a copy of the same data, so nothing in the data tells them apart.
///   `DATABASE_URL`, when set, must therefore name a staging database: its host or its database
///   name must contain `staging` (on Railway, name the staging database's service
///   `postgres-staging`, whose private host is then `postgres-staging.railway.internal`; or name
///   the database `stitchfoundry_staging`). An address that can't be read is refused. Without
///   `DATABASE_URL` (the data folder, which production doesn't use) this check passes.
pub fn production_guard(cfg: &Config, env: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
    let refuse = |why: &str| Err(format!("SF_CLOUD_TEST_MODE is on but {why}: test mode is for a staging copy only, never production"));
    match cfg.env.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("staging") => {}
        None => return refuse("SF_CLOUD_ENV isn't set (test mode needs SF_CLOUD_ENV=staging)"),
        Some(_) => return refuse("SF_CLOUD_ENV isn't staging"),
    }
    if cfg.test_secret.as_ref().is_none_or(|s| s.chars().count() < MIN_TEST_SECRET) {
        return refuse(&format!("SF_CLOUD_TEST_SECRET is missing or shorter than {MIN_TEST_SECRET} characters"));
    }
    if url_host(&cfg.public_url) == PRODUCTION_HOST {
        return refuse("SF_CLOUD_PUBLIC_URL is the production address");
    }
    if cfg.stripe_key.as_deref().is_some_and(|k| k.starts_with("sk_live_") || k.starts_with("rk_live_")) {
        return refuse("STRIPE_SECRET_KEY is a live key");
    }
    if env("RAILWAY_ENVIRONMENT_NAME").is_some_and(|e| e.trim().eq_ignore_ascii_case("production")) {
        return refuse("RAILWAY_ENVIRONMENT_NAME is production");
    }
    if let Some(url) = &cfg.database_url {
        let Ok(db) = url.parse::<postgres::Config>() else { return refuse("DATABASE_URL can't be read") };
        let marked = |s: &str| s.to_ascii_lowercase().contains(STAGING_MARKER);
        let hosts = db.get_hosts();
        let host_marked = !hosts.is_empty() && hosts.iter().all(|h| matches!(h, postgres::config::Host::Tcp(h) if marked(h)));
        if !host_marked && !db.get_dbname().is_some_and(marked) {
            return refuse("DATABASE_URL doesn't name a staging database (its host or database name must contain \"staging\")");
        }
    }
    Ok(())
}

/// `text` as Unix seconds: digits, or a UTC date `YYYY-MM-DD` with an optional `THH:MM[:SS]` and
/// `Z`.
pub fn parse_time(text: &str) -> Option<u64> {
    let t = text.trim();
    if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
        return t.parse().ok();
    }
    let t = t.strip_suffix('Z').unwrap_or(t);
    let (date, time) = t.split_once('T').unwrap_or((t, "00:00:00"));
    let d: Vec<i64> = date.split('-').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let h: Vec<i64> = time.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    let ([y, m, day], [hh, mm, rest @ ..]) = (d.as_slice(), h.as_slice()) else { return None };
    let ss = rest.first().copied().unwrap_or(0);
    if !((1970..=9999).contains(y)
        && (1..=12).contains(m)
        && (1..=31).contains(day)
        && (0..24).contains(hh)
        && (0..60).contains(mm)
        && (0..61).contains(&ss))
    {
        return None;
    }
    // days from 1970-01-01 to the date (Howard Hinnant's days_from_civil)
    let (y, m) = if *m <= 2 { (y - 1, m + 9) } else { (*y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

/// What stands in for Resend's key while emails go to a test mailbox: it turns email on and is
/// never sent anywhere ([`crate::mail_catcher`] leaves the key behind).
pub const MAIL_CATCHER_KEY: &str = "mail-catcher";

/// The default packs, as the owner priced them (2026-10-18): $10, $25 and $50.
pub const DEFAULT_PACKS: &str = "small:1000:1000,medium:2750:2500,large:6000:5000";

impl Config {
    /// The settings in `env` (`name → value`), defaults for the rest.
    pub fn from_env(env: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let num = |name: &str, default: f64| -> Result<f64, String> {
            match env(name) {
                None => Ok(default),
                Some(v) => v.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).ok_or_else(|| format!("{name} isn't a number")),
            }
        };
        let packs = parse_packs(&env("SF_CLOUD_PACKS").unwrap_or_else(|| DEFAULT_PACKS.to_string()))?;
        let cents_per_credit = num("SF_CLOUD_CENTS_PER_CREDIT", 1.0)?;
        if cents_per_credit <= 0.0 {
            return Err("SF_CLOUD_CENTS_PER_CREDIT must be more than 0".into());
        }
        let nonempty = |name: &str| env(name).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let test_mode = switch("SF_CLOUD_TEST_MODE", env("SF_CLOUD_TEST_MODE"))?;
        let mail_catcher = nonempty("SF_CLOUD_MAIL_CATCHER_URL");
        let cfg = Config {
            // Railway (and most container hosts) give the port in PORT
            addr: env("SF_CLOUD_ADDR").or_else(|| nonempty("PORT").map(|p| format!("0.0.0.0:{p}"))).unwrap_or_else(|| "127.0.0.1:8080".into()),
            data_dir: PathBuf::from(env("SF_CLOUD_DATA").unwrap_or_else(|| "stitchfoundry-cloud-data".into())),
            anthropic_key: nonempty("ANTHROPIC_API_KEY"),
            stripe_key: nonempty("STRIPE_SECRET_KEY"),
            stripe_webhook_secret: nonempty("STRIPE_WEBHOOK_SECRET"),
            public_url: env("SF_CLOUD_PUBLIC_URL").unwrap_or_else(|| "http://127.0.0.1:8080".into()).trim_end_matches('/').to_string(),
            signup_credits: num("SF_CLOUD_SIGNUP_CREDITS", 100.0)? as u64,
            cents_per_credit,
            markup: num("SF_CLOUD_MARKUP", 3.0)?.max(1.0),
            packs,
            allowed: vec!["claude-opus-5-5".into()],
            models: vec![
                ModelPrice { model: "claude-opus-5-5".into(), input: 4.0, output: 20.0, cache_read: 0.20, cache_write: 5.0 },
                ModelPrice { model: "claude-opus-5".into(), input: 5.0, output: 25.0, cache_read: 0.50, cache_write: 6.25 },
                ModelPrice { model: "claude-opus-4-8".into(), input: 5.0, output: 25.0, cache_read: 0.50, cache_write: 6.25 },
            ],
            max_tokens: 16_000,
            free_checks: num("SF_CLOUD_FREE_CHECKS", 3.0)? as u64,
            check_credits: num("SF_CLOUD_CHECK_CREDITS", 10.0)? as u64,
            free_budget_cents: num("SF_CLOUD_FREE_BUDGET_CENTS", 2500.0)? as u64,
            database_url: nonempty("DATABASE_URL"),
            // (with a test mailbox, email is on and no Resend key is kept)
            resend_key: if mail_catcher.is_some() { Some(MAIL_CATCHER_KEY.into()) } else { nonempty("RESEND_API_KEY") },
            reports_to: nonempty("SF_CLOUD_REPORTS_TO"),
            reports_from: nonempty("SF_CLOUD_REPORTS_FROM").unwrap_or_else(|| "StitchFoundry <reports@stitchfoundry.app>".into()),
            mail_from: nonempty("SF_CLOUD_MAIL_FROM").unwrap_or_else(|| "StitchFoundry <accounts@stitchfoundry.app>".into()),
            mail_catcher,
            commit: nonempty("RAILWAY_GIT_COMMIT_SHA").filter(|c| (7..=64).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_hexdigit())),
            end_sessions_before: match nonempty("SF_CLOUD_END_SESSIONS_BEFORE") {
                None => None,
                Some(v) => Some(parse_time(&v).ok_or("SF_CLOUD_END_SESSIONS_BEFORE is a UTC time like 2026-10-22T09:00:00Z")?),
            },
            test_mode,
            // (only read in test mode; with it off the routes don't exist)
            test_secret: if test_mode { nonempty("SF_CLOUD_TEST_SECRET") } else { None },
            env: nonempty("SF_CLOUD_ENV"),
        };
        if cfg.test_mode {
            production_guard(&cfg, &env)?;
        }
        Ok(cfg)
    }

    /// What model `name` costs, if the service knows it.
    pub fn model(&self, name: &str) -> Option<&ModelPrice> {
        self.models.iter().find(|m| m.model == name)
    }

    /// The dearest prices of all the models a reply may come from (for holding credits).
    pub fn dearest(&self) -> ModelPrice {
        let max = |f: fn(&ModelPrice) -> f64| self.models.iter().map(f).fold(0.0, f64::max);
        ModelPrice {
            model: String::new(),
            input: max(|m| m.input),
            output: max(|m| m.output),
            cache_read: max(|m| m.cache_read),
            cache_write: max(|m| m.cache_write),
        }
    }
}

/// `id:credits:cents` packs, apart by commas.
pub fn parse_packs(s: &str) -> Result<Vec<Pack>, String> {
    let mut packs = Vec::new();
    for item in s.split(',').map(str::trim).filter(|i| !i.is_empty()) {
        let parts: Vec<&str> = item.split(':').map(str::trim).collect();
        let [id, credits, cents] = parts[..] else {
            return Err(format!("a pack is id:credits:cents, not {item:?}"));
        };
        let credits = credits.parse::<u64>().ok().filter(|c| *c > 0).ok_or_else(|| format!("{item:?}: credits must be a whole number above 0"))?;
        let cents = cents.parse::<u64>().ok().filter(|c| *c >= 50).ok_or_else(|| format!("{item:?}: the price is in cents, 50 or more"))?;
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(format!("{item:?}: a pack's id is letters, digits and -"));
        }
        packs.push(Pack { id: id.to_string(), credits, cents });
    }
    if packs.is_empty() {
        return Err("SF_CLOUD_PACKS names no pack".into());
    }
    Ok(packs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_to_end_sessions_before_are_read() {
        assert_eq!(parse_time("2026-10-22T09:00:00Z"), Some(1_792_659_600));
        assert_eq!(parse_time("2026-10-22"), Some(1_792_627_200));
        assert_eq!(parse_time("1970-01-01T00:00Z"), Some(0));
        assert_eq!(parse_time(" 1792659600 "), Some(1_792_659_600));
        for bad in ["", "yesterday", "2026-13-01", "2026-10-22T25:00", "10/22/2026"] {
            assert_eq!(parse_time(bad), None, "{bad}");
        }
        let env = |k: &str| (k == "SF_CLOUD_END_SESSIONS_BEFORE").then(|| "2026-10-22".to_string());
        assert_eq!(Config::from_env(env).unwrap().end_sessions_before, Some(1_792_627_200));
        assert!(Config::from_env(|k| (k == "SF_CLOUD_END_SESSIONS_BEFORE").then(|| "soon".to_string())).is_err());
    }

    #[test]
    fn a_test_mailbox_turns_email_on_and_drops_resends_key() {
        let c = Config::from_env(|_| None).unwrap();
        assert!(c.mail_catcher.is_none() && c.resend_key.is_none());
        let c = Config::from_env(|k| match k {
            "SF_CLOUD_MAIL_CATCHER_URL" => Some(" http://mailpit.railway.internal:8025 ".into()),
            "RESEND_API_KEY" => Some("re_live_secret".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(c.mail_catcher.as_deref(), Some("http://mailpit.railway.internal:8025"));
        assert_eq!(c.resend_key.as_deref(), Some(MAIL_CATCHER_KEY));
    }

    #[test]
    fn the_commit_is_read_only_when_it_is_one() {
        let commit = |v: &str| Config::from_env(|k| (k == "RAILWAY_GIT_COMMIT_SHA").then(|| v.to_string())).unwrap().commit;
        assert_eq!(commit(" 84f911b7a0 ").as_deref(), Some("84f911b7a0"));
        for bad in ["", "abc", "not-a-commit", "84f911b7a0<script>"] {
            assert_eq!(commit(bad), None, "{bad}");
        }
    }

    #[test]
    fn settings_come_from_the_environment_with_defaults() {
        let c = Config::from_env(|_| None).unwrap();
        assert_eq!(c.addr, "127.0.0.1:8080");
        assert_eq!(c.signup_credits, 100);
        assert_eq!(c.free_budget_cents, 2500);
        assert_eq!(c.packs.len(), 3);
        assert!(c.anthropic_key.is_none() && c.stripe_key.is_none());
        let c = Config::from_env(|k| match k {
            "ANTHROPIC_API_KEY" => Some(" sk-ant-x ".into()),
            "SF_CLOUD_PACKS" => Some("one:100:199".into()),
            "SF_CLOUD_PUBLIC_URL" => Some("https://cloud.example/".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(c.anthropic_key.as_deref(), Some("sk-ant-x"));
        assert_eq!(c.packs, [Pack { id: "one".into(), credits: 100, cents: 199 }]);
        assert_eq!(c.public_url, "https://cloud.example");
        assert!(Config::from_env(|k| (k == "SF_CLOUD_MARKUP").then(|| "lots".to_string())).is_err());
        assert!(parse_packs("a:0:100").is_err() && parse_packs("a:10:10").is_err() && parse_packs("a b:10:100").is_err() && parse_packs("").is_err());
    }

    /// CLOUD-13: a staging setup test mode starts with (each test below changes one thing).
    fn staging(k: &str) -> Option<String> {
        match k {
            "SF_CLOUD_TEST_MODE" => Some("1".into()),
            "SF_CLOUD_ENV" => Some("staging".into()),
            "SF_CLOUD_TEST_SECRET" => Some("0123456789abcdef0123456789abcdef".into()),
            "SF_CLOUD_PUBLIC_URL" => Some("https://cloud-staging.stitchfoundry.app".into()),
            "STRIPE_SECRET_KEY" => Some("sk_test_x".into()),
            "DATABASE_URL" => Some("postgresql://postgres:pw@postgres-staging.railway.internal:5432/railway".into()),
            _ => None,
        }
    }

    /// `staging` with `name` set to `value` (or removed with `None`).
    fn staging_with(name: &'static str, value: Option<&'static str>) -> impl Fn(&str) -> Option<String> {
        move |k: &str| if k == name { value.map(str::to_string) } else { staging(k) }
    }

    fn refused(env: impl Fn(&str) -> Option<String>, why: &str) {
        let e = Config::from_env(env).unwrap_err();
        assert!(e.contains(why) && e.contains("staging copy only"), "{e}");
    }

    #[test]
    fn test_mode_is_off_by_default_and_starts_on_a_staging_setup() {
        let c = Config::from_env(|_| None).unwrap();
        assert!(!c.test_mode && c.test_secret.is_none());
        // off, everything else is as before: the secret isn't even read
        let c = Config::from_env(staging_with("SF_CLOUD_TEST_MODE", Some("off"))).unwrap();
        assert!(!c.test_mode && c.test_secret.is_none());
        let c = Config::from_env(staging).unwrap();
        assert!(c.test_mode);
        assert_eq!(c.test_secret.as_deref(), Some("0123456789abcdef0123456789abcdef"));
        // the database's name may carry the marker instead of its host; the data folder passes
        assert!(Config::from_env(staging_with("DATABASE_URL", Some("postgres://u:p@db.internal/stitchfoundry_staging"))).unwrap().test_mode);
        assert!(Config::from_env(staging_with("DATABASE_URL", Some("host=staging-db.internal user=u dbname=sf"))).unwrap().test_mode);
        assert!(Config::from_env(staging_with("DATABASE_URL", None)).unwrap().test_mode);
        // a typo is refused rather than guessed
        assert!(Config::from_env(staging_with("SF_CLOUD_TEST_MODE", Some("maybe"))).is_err());
    }

    #[test]
    fn test_mode_refuses_without_sf_cloud_env() {
        refused(staging_with("SF_CLOUD_ENV", None), "SF_CLOUD_ENV isn't set");
    }

    #[test]
    fn test_mode_refuses_an_env_other_than_staging() {
        refused(staging_with("SF_CLOUD_ENV", Some("production")), "SF_CLOUD_ENV isn't staging");
        refused(staging_with("SF_CLOUD_ENV", Some("prod")), "SF_CLOUD_ENV isn't staging");
    }

    #[test]
    fn test_mode_refuses_a_missing_or_short_secret() {
        refused(staging_with("SF_CLOUD_TEST_SECRET", None), "SF_CLOUD_TEST_SECRET");
        refused(staging_with("SF_CLOUD_TEST_SECRET", Some("   ")), "SF_CLOUD_TEST_SECRET");
        refused(staging_with("SF_CLOUD_TEST_SECRET", Some("0123456789abcdef0123456789abcde")), "shorter than 32");
    }

    #[test]
    fn test_mode_refuses_the_production_address() {
        for url in [
            "https://cloud.stitchfoundry.app",
            "https://cloud.stitchfoundry.app/",
            "HTTPS://Cloud.StitchFoundry.App//",
            "https://cloud.stitchfoundry.app:443/v1",
            "http://cloud.stitchfoundry.app",
        ] {
            refused(staging_with("SF_CLOUD_PUBLIC_URL", Some(url)), "SF_CLOUD_PUBLIC_URL is the production address");
        }
    }

    #[test]
    fn test_mode_refuses_a_live_stripe_key() {
        refused(staging_with("STRIPE_SECRET_KEY", Some("sk_live_abc")), "live key");
        refused(staging_with("STRIPE_SECRET_KEY", Some("rk_live_abc")), "live key");
    }

    #[test]
    fn test_mode_refuses_a_database_not_named_staging() {
        for url in [
            "postgresql://postgres:pw@postgres.railway.internal:5432/railway",
            // the marker in the password or user doesn't count: only the host or the database's name
            "postgres://staging:staging@db.internal/stitchfoundry",
            "host=db.internal user=staging dbname=sf",
            "not a database address ://",
        ] {
            refused(staging_with("DATABASE_URL", Some(url)), "DATABASE_URL");
        }
    }

    #[test]
    fn test_mode_refuses_railways_production_environment() {
        let env = |k: &str| if k == "RAILWAY_ENVIRONMENT_NAME" { Some("production".to_string()) } else { staging(k) };
        refused(env, "RAILWAY_ENVIRONMENT_NAME is production");
    }
}
