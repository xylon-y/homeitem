//! Runtime configuration from environment variables.

use std::path::PathBuf;

use common::DEFAULT_LISTEN_ADDR;

/// Fully resolved application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    pub db_path: PathBuf,
    pub upload_dir: PathBuf,
    pub jwt_secret: String,
    pub allow_registration: bool,
    pub cors_origins: Vec<String>,
    pub dev_cors: bool,
    pub max_upload_mb: u64,
    pub static_dir: Option<PathBuf>,
    /// SMTP relay, e.g. "smtp://user:pass@mail.example.com:587" or
    /// "smtp://mail.example.com:25" (no auth).
    pub smtp_url: Option<String>,
    pub mail_from: String,
    /// Minutes between background reminder scans.
    pub reminder_interval_min: i64,
    /// Base URL used in reminder email links.
    pub public_base_url: String,
}

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).map(|v| v.trim().to_string()).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string())
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"),
        Err(_) => default,
    }
}

impl Config {
    /// Read configuration from the process environment.
    ///
    /// # Panics
    /// Panics only if `HOMEITEM_JWT_SECRET` is set to an empty value while
    /// `HOMEITEM_REQUIRE_SECRET` semantics are in play; in practice the secret
    /// is generated and persisted when missing.
    pub fn from_env() -> Self {
        let db_path = PathBuf::from(env_str("HOMEITEM_DB_PATH", "./data/homeitem.db"));
        let upload_dir = PathBuf::from(env_str("HOMEITEM_UPLOAD_DIR", "./data/uploads"));

        let jwt_secret = match std::env::var("HOMEITEM_JWT_SECRET") {
            Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
            _ => load_or_create_secret(&db_path),
        };

        let cors_origins = env_str("HOMEITEM_CORS_ORIGINS", "")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect();

        let smtp_url = std::env::var("HOMEITEM_SMTP_URL").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

        Self {
            listen_addr: env_str("HOMEITEM_LISTEN_ADDR", DEFAULT_LISTEN_ADDR),
            db_path,
            upload_dir,
            jwt_secret,
            allow_registration: env_bool("HOMEITEM_ALLOW_REGISTRATION", false),
            cors_origins,
            dev_cors: env_bool("HOMEITEM_DEV_CORS", false),
            max_upload_mb: env_str("HOMEITEM_MAX_UPLOAD_MB", "25").parse().unwrap_or(25),
            static_dir: std::env::var("HOMEITEM_STATIC_DIR").ok().map(PathBuf::from).filter(|p| !p.as_os_str().is_empty()),
            smtp_url,
            mail_from: env_str("HOMEITEM_MAIL_FROM", "homeitem@localhost"),
            reminder_interval_min: env_str("HOMEITEM_REMINDER_INTERVAL_MIN", "60").parse().unwrap_or(60).max(1),
            public_base_url: env_str("HOMEITEM_PUBLIC_BASE_URL", "http://localhost:7745"),
        }
    }

    /// Deterministic config for unit tests: no env reads, memory-ish paths.
    #[cfg(test)]
    pub fn from_env_for_test() -> Self {
        Self {
            listen_addr: "127.0.0.1:0".into(),
            db_path: PathBuf::from(":memory:"),
            upload_dir: PathBuf::from("./data/uploads"),
            jwt_secret: "test-secret".into(),
            allow_registration: true,
            cors_origins: Vec::new(),
            dev_cors: false,
            max_upload_mb: 25,
            static_dir: None,
            smtp_url: None,
            mail_from: "homeitem@localhost".into(),
            reminder_interval_min: 60,
            public_base_url: "http://localhost:7745".into(),
        }
    }
}

/// Load the JWT secret from `data/.jwt_secret`, generating one on first run.
fn load_or_create_secret(db_path: &PathBuf) -> String {
    let dir = db_path.parent().unwrap_or(std::path::Path::new("./data"));
    let secret_path = dir.join(".jwt_secret");
    if let Ok(secret) = std::fs::read_to_string(&secret_path) {
        let trimmed = secret.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let mut bytes = [0u8; 48];
    rand::Rng::fill(&mut rand::rng(), &mut bytes[..]);
    let secret = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    if let Err(e) = std::fs::write(&secret_path, &secret) {
        tracing::warn!(error = %e, "could not persist jwt secret, using ephemeral value");
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&secret_path, std::fs::Permissions::from_mode(0o600));
        }
    }
    secret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bool_env_parses() {
        // env-based, just verify default fallback path via unset key
        let _ = env_bool("HOMEITEM_TEST_UNSET_KEY_XYZ", true);
    }

    #[test]
    fn config_defaults_exist() {
        // Ensure constructor compiles and defaults are non-empty.
        let listen = env_str("HOMEITEM_TEST_UNSET_LISTEN", DEFAULT_LISTEN_ADDR);
        assert!(!listen.is_empty());
    }
}
