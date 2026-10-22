//! Design from Prompt's desktop services: the HTTPS request to the AI service, and the file that
//! keeps the user's API key. The key lives in its own file next to the settings, readable only by
//! the user, not in the settings: the engine, the control channel and MCP agents can read those.

use std::path::{Path, PathBuf};
use std::time::Duration;

use vectorcraft_ui_egui::{HttpRequest, SecretStore};

/// How long a drawing may take, answer included: as long as the hosted service waits for it, since
/// a request given up on here is still billed when it finishes.
const TIMEOUT: Duration = Duration::from_secs(600);

/// Send `req` and wait for the answer → (HTTP status, body). Proxies come from `HTTPS_PROXY` and
/// friends; certificates are checked against the system's trusted ones.
pub fn post(req: &HttpRequest) -> Result<(u16, String), String> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).timeout_global(Some(TIMEOUT)).http_status_as_error(false).build().into();
    let mut call = agent.post(&req.url);
    for (name, value) in &req.headers {
        call = call.header(*name, value.as_str());
    }
    let mut res = call.send(req.body.as_str()).map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let body = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// Fetch `url` and wait for it → (HTTP status, body of at most `limit` bytes). Proxies and
/// certificates as [`post`].
pub fn get(url: &str, limit: u64) -> Result<(u16, Vec<u8>), String> {
    let tls = ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(Duration::from_secs(120)))
        .http_status_as_error(false)
        .user_agent("StitchFoundry")
        .build()
        .into();
    let mut res = agent.get(url).call().map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let body = res.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| e.to_string())?;
    Ok((status, body))
}

/// Secrets kept as files in a folder of the user's own; a missing one falls back to an environment
/// variable (`ANTHROPIC_API_KEY` for the key).
pub struct SecretFiles {
    /// None: secrets aren't kept (runs without preferences).
    dir: Option<PathBuf>,
}

impl SecretFiles {
    pub fn new(dir: Option<PathBuf>) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> Option<PathBuf> {
        // Names are ours (`anthropic-api-key`): never a path.
        let plain = !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
        self.dir.as_ref().filter(|_| plain).map(|d| d.join(name))
    }
}

/// The environment variable a secret falls back to.
fn env_var(name: &str) -> Option<&'static str> {
    match name {
        "anthropic-api-key" => Some("ANTHROPIC_API_KEY"),
        // a StitchFoundry session (automation, testing)
        "stitchfoundry-session" => Some("STITCHFOUNDRY_SESSION"),
        _ => None,
    }
}

impl SecretStore for SecretFiles {
    fn load(&self, name: &str) -> Option<String> {
        let saved = self.path(name).and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        saved.or_else(|| std::env::var(env_var(name)?).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
    }

    fn save(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        let path = self.path(name).ok_or("secrets aren't kept in this run")?;
        match value {
            None => match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("can't forget it: {e}")),
                _ => Ok(()),
            },
            Some(v) => write_private(&path, v).map_err(|e| format!("can't keep it: {e}")),
        }
    }
}

/// Write `value` to `path`, readable and writable by the user alone (Unix; on Windows the folder,
/// in the user's profile, is theirs).
fn write_private(path: &Path, value: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
        o.mode(0o600);
        // A file made earlier with wider permissions is narrowed first.
        if path.exists() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    o.open(path)?.write_all(value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_kept_privately_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("stitchfoundry-secrets-{}", std::process::id()));
        let store = SecretFiles::new(Some(dir.clone()));
        store.save("test-key", Some("sk-ant-123")).unwrap();
        assert_eq!(store.load("test-key").as_deref(), Some("sk-ant-123"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(dir.join("test-key")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        store.save("test-key", None).unwrap();
        assert_eq!(store.load("test-key"), None);
        store.save("test-key", None).unwrap();
        // names are never paths
        assert!(store.save("../x", Some("v")).is_err());
        assert!(SecretFiles::new(None).save("test-key", Some("v")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
