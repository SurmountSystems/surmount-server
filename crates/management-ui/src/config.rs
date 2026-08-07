//! Runtime configuration from environment (set by systemd unit / Nix module).

use std::env;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

use crate::tls::{ListenMode, TlsPaths};
use surmount_management_ui::auth::{resolve_allowlist, AuthConfig, AuthMode};
use surmount_management_ui::ban::{ban_config_from_env, BanConfig};
use surmount_management_ui::rate_limit::{FixedWindowRateLimiter, DEFAULT_MAX_KEYS};

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub listen: SocketAddr,
    /// Optional separate bind for plain HTTP redirect-only listener (:80 path).
    /// None = disabled. Used with `redirect_http_to_https` (see redirect_bind_decision).
    pub http_redirect_listen: Option<SocketAddr>,
    /// Optional loopback cleartext bind for the **full** management API (not redirect-only).
    /// Used when primary is https and a local reverse-proxy (Arti lean path) needs plain HTTP.
    /// None = disabled. Must differ from `listen` and `http_redirect_listen`; loopback only.
    pub local_cleartext_listen: Option<SocketAddr>,
    pub listen_mode: ListenMode,
    /// When true with `http_redirect_listen`, main binds a redirect-only plain HTTP listener.
    pub redirect_http_to_https: bool,
    /// Hostnames allowed when building HTTP->HTTPS Location (open-redirect guard).
    pub redirect_allowed_hosts: Vec<String>,
    /// Dangerous escape: allow cleartext bind when listen_mode is https (default false).
    pub https_allow_cleartext_escape: bool,
    pub primary_domain: String,
    pub mail_hostname: String,
    pub services_hostname: String,
    pub stalwart_url: String,
    /// Operator-published onion URL for display (env or hostname file). Never invented.
    /// Normalized to `http://….onion` when a bare hostname is provided.
    pub onion_url: Option<String>,
    /// Max requests per client key per window (0 disables limiter).
    pub rate_limit_max_requests: u32,
    pub rate_limit_window: Duration,
    pub rate_limit_max_keys: usize,
    /// Ban/whitelist subsystem (default enforcement off; lean private).
    pub ban: BanConfig,
    /// Nostr auth scaffold (default off for local `just dev`).
    pub auth: AuthConfig,
    /// Lab escape: when true, live directory list/mutations may run with
    /// `auth_mode=off`. Never production default. Env
    /// `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED`.
    pub allow_directory_unauthenticated: bool,
}

/// Normalize an operator onion string for display and links.
///
/// Accepts `http://….onion`, bare `….onion`, or whitespace-padded forms.
/// Empty / whitespace-only -> `None`. Bare hostname becomes `http://…`.
pub fn normalize_onion_url(raw: &str) -> Option<String> {
    let t = raw.trim().trim_end_matches('/').trim();
    if t.is_empty() {
        return None;
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        return Some(t.to_string());
    }
    // Bare hostname (or path-less host). Prefer http:// for Tor Browser links.
    Some(format!("http://{t}"))
}

/// Read onion from a host file (first non-empty line, trimmed). Empty file = unset.
pub fn onion_url_from_file(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let line = contents
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    normalize_onion_url(line)
}

/// Resolve onion URL: `SURMOUNT_ONION_URL` wins when non-empty; else readable
/// non-empty `SURMOUNT_ONION_HOSTNAME_FILE`. Unreadable file = unset.
pub fn resolve_onion_url_from_env() -> Option<String> {
    if let Ok(v) = env::var("SURMOUNT_ONION_URL") {
        if let Some(n) = normalize_onion_url(&v) {
            return Some(n);
        }
    }
    match env::var("SURMOUNT_ONION_HOSTNAME_FILE") {
        Ok(p) => {
            let p = p.trim();
            if p.is_empty() {
                None
            } else {
                onion_url_from_file(Path::new(p))
            }
        }
        Err(_) => None,
    }
}

/// Redact v3 (56) / legacy (16) onion labels in log or error text.
/// Mirrors `surmount-e2e` failure-tail redaction: never print full addresses.
/// Scans the whole string: non-label `.onion` substrings are left as-is and
/// the scan continues so later real labels are still redacted.
pub fn redact_onion_in_text(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(rel) = rest.find(".onion") {
        let end_rel = rel + ".onion".len();
        let before = &rest[..rel];
        let start_rel = before
            .rfind(|c: char| !matches!(c, 'a'..='z' | '2'..='7' | 'A'..='Z'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let candidate = &before[start_rel..];
        if candidate.len() == 56 || candidate.len() == 16 {
            out.push_str(&rest[..start_rel]);
            out.push_str("<onion-redacted>");
            rest = &rest[end_rel..];
        } else {
            // Not a v3/legacy label: keep through this `.onion`, continue after.
            out.push_str(&rest[..end_rel]);
            rest = &rest[end_rel..];
        }
    }
    out.push_str(rest);
    out
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let listen = env::var("SURMOUNT_LISTEN")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 8080)));

        let listen_mode = ListenMode::from_parts(
            env::var("SURMOUNT_LISTEN_MODE").ok().as_deref(),
            env::var("SURMOUNT_TLS_CERT").ok().as_deref(),
            env::var("SURMOUNT_TLS_KEY").ok().as_deref(),
        )?;

        let redirect_http_to_https = env_bool("SURMOUNT_REDIRECT_HTTP_TO_HTTPS", false);
        // Non-empty value must parse as SocketAddr; do not silently Skip bind.
        let http_redirect_listen = match env::var("SURMOUNT_HTTP_REDIRECT_LISTEN") {
            Err(_) => None,
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.parse::<SocketAddr>().map_err(|e| {
                        format!(
                            "SURMOUNT_HTTP_REDIRECT_LISTEN={t:?} is not a valid socket address: {e}"
                        )
                    })?)
                }
            }
        };

        // Full API cleartext (Arti / local rproxy). Empty/unset = disabled.
        let local_cleartext_listen = match env::var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN") {
            Err(_) => None,
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(t.parse::<SocketAddr>().map_err(|e| {
                        format!(
                            "SURMOUNT_LOCAL_CLEARTEXT_LISTEN={t:?} is not a valid socket address: {e}"
                        )
                    })?)
                }
            }
        };

        let primary_domain =
            env::var("SURMOUNT_PRIMARY_DOMAIN").unwrap_or_else(|_| "surmount.systems".into());
        let mail_hostname =
            env::var("SURMOUNT_MAIL_HOSTNAME").unwrap_or_else(|_| "mail.surmount.systems".into());
        let services_hostname = env::var("SURMOUNT_SERVICES_HOSTNAME")
            .unwrap_or_else(|_| format!("services.{primary_domain}"));

        let redirect_allowed_hosts = env::var("SURMOUNT_REDIRECT_ALLOWED_HOSTS")
            .ok()
            .map(|s| {
                s.split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                vec![
                    services_hostname.clone(),
                    primary_domain.clone(),
                    format!("www.{primary_domain}"),
                    mail_hostname.clone(),
                ]
            });

        let rate_limit_max_requests = env::var("SURMOUNT_RATE_LIMIT_MAX")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);
        let rate_limit_window_secs = env::var("SURMOUNT_RATE_LIMIT_WINDOW_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60u64);
        let rate_limit_max_keys = env::var("SURMOUNT_RATE_LIMIT_MAX_KEYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_KEYS);

        // Invalid ban env fails closed (not optional skip).
        let ban = ban_config_from_env(|k| env::var(k).ok())?;

        // Onion: env wins; else hostname file. Never invent a live onion.
        let onion_url = resolve_onion_url_from_env();

        let auth = auth_config_from_env()?;
        auth.validate()?;

        // Single SoT with directory process-start coupling (same env + truthy parser).
        let allow_directory_unauthenticated =
            crate::directory::directory_allow_unauthenticated_from_env();

        Ok(Self {
            listen,
            http_redirect_listen,
            local_cleartext_listen,
            listen_mode,
            redirect_http_to_https,
            redirect_allowed_hosts,
            https_allow_cleartext_escape: env_bool("SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE", false),
            primary_domain,
            mail_hostname,
            services_hostname,
            stalwart_url: env::var("SURMOUNT_STALWART_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
            onion_url,
            rate_limit_max_requests,
            rate_limit_window: Duration::from_secs(rate_limit_window_secs.max(1)),
            rate_limit_max_keys: rate_limit_max_keys.max(1),
            ban,
            auth,
            allow_directory_unauthenticated,
        })
    }

    /// Validate local cleartext bind constraints (loopback, not primary, not redirect).
    /// Called from main before bind so misconfig fails closed without a half-open listener.
    ///
    /// Port-level collision is rejected even when IPs differ: Linux cannot bind
    /// `0.0.0.0:P` and `127.0.0.1:P` at once (EADDRINUSE).
    pub fn validate_local_cleartext(&self) -> Result<(), String> {
        let Some(lc) = self.local_cleartext_listen else {
            return Ok(());
        };
        if !lc.ip().is_loopback() {
            return Err(format!(
                "SURMOUNT_LOCAL_CLEARTEXT_LISTEN={lc} must be loopback-only (local cleartext API; fail-closed)"
            ));
        }
        if lc == self.listen {
            return Err(format!(
                "SURMOUNT_LOCAL_CLEARTEXT_LISTEN={lc} must differ from primary SURMOUNT_LISTEN (fail-closed)"
            ));
        }
        // Same port as primary (any IP): wildcard primary would collide on bind.
        if lc.port() == self.listen.port() {
            return Err(format!(
                "SURMOUNT_LOCAL_CLEARTEXT_LISTEN={lc} port must differ from primary \
                 SURMOUNT_LISTEN={} (Linux cannot bind wildcard and loopback on the same port; fail-closed)",
                self.listen
            ));
        }
        if let Some(redir) = self.http_redirect_listen {
            if lc == redir {
                return Err(format!(
                    "SURMOUNT_LOCAL_CLEARTEXT_LISTEN={lc} must differ from SURMOUNT_HTTP_REDIRECT_LISTEN \
                     (redirect-only port is not a cleartext API; fail-closed)"
                ));
            }
            if lc.port() == redir.port() {
                return Err(format!(
                    "SURMOUNT_LOCAL_CLEARTEXT_LISTEN={lc} port must differ from \
                     SURMOUNT_HTTP_REDIRECT_LISTEN={redir} (fail-closed)"
                ));
            }
        }
        Ok(())
    }

    /// Build an in-memory limiter when max > 0.
    pub fn rate_limiter(&self) -> Option<FixedWindowRateLimiter<String>> {
        if self.rate_limit_max_requests == 0 {
            None
        } else {
            Some(FixedWindowRateLimiter::with_max_keys(
                self.rate_limit_max_requests,
                self.rate_limit_window,
                self.rate_limit_max_keys,
            ))
        }
    }

    pub fn tls_paths(&self) -> Option<&TlsPaths> {
        match &self.listen_mode {
            ListenMode::Https(p) => Some(p),
            ListenMode::PlainHttp => None,
        }
    }
}

/// SoT for truthy env flags (`1` / `true` / `yes` / `on`, case-insensitive).
///
/// Used by [`env_bool`] and directory lab-escape
/// (`SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED`) so config and process-start
/// coupling never drift on accepted tokens.
pub fn parse_env_flag_truthy(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => parse_env_flag_truthy(&v),
        Err(_) => default,
    }
}

/// Auth env surface (scaffold). Default mode off so `just dev` stays open.
fn auth_config_from_env() -> Result<AuthConfig, String> {
    let mode = match env::var("SURMOUNT_AUTH_MODE") {
        Ok(v) => AuthMode::parse(&v)?,
        Err(_) => AuthMode::Off,
    };

    // Env allowlist wins when non-empty; else optional file; empty = fail-closed.
    let allowlist_raw = env::var("SURMOUNT_NOSTR_ALLOWLIST").unwrap_or_default();
    let allowlist_file = env::var("SURMOUNT_NOSTR_ALLOWLIST_FILE").ok();
    let allowlist = resolve_allowlist(&allowlist_raw, allowlist_file.as_deref())?;

    let session_secret = match env::var("SURMOUNT_SESSION_SECRET") {
        Ok(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                // Accept raw string or hex; store raw UTF-8 bytes of the env value
                // (operator can use openssl rand -hex 32 as opaque key material).
                Some(t.as_bytes().to_vec())
            }
        }
        Err(_) => None,
    };

    let session_ttl_secs = env::var("SURMOUNT_SESSION_TTL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(86_400u64)
        .max(60);

    let public_base_url = env::var("SURMOUNT_PUBLIC_BASE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let nip98_max_skew_secs = env::var("SURMOUNT_NIP98_MAX_SKEW_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300u64)
        .max(1);

    Ok(AuthConfig {
        mode,
        allowlist,
        session_secret,
        session_ttl_secs,
        public_base_url,
        nip98_max_skew_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tls::ListenMode;
    use std::sync::{Mutex, MutexGuard};
    use surmount_management_ui::ban::BanEnforcement;

    // Serialize env-mutating tests in this process.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn acquire() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            clear_surmount_env();
            Self { _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            clear_surmount_env();
        }
    }

    fn clear_surmount_env() {
        for k in [
            "SURMOUNT_LISTEN",
            "SURMOUNT_LISTEN_MODE",
            "SURMOUNT_TLS_CERT",
            "SURMOUNT_TLS_KEY",
            "SURMOUNT_REDIRECT_HTTP_TO_HTTPS",
            "SURMOUNT_HTTP_REDIRECT_LISTEN",
            "SURMOUNT_LOCAL_CLEARTEXT_LISTEN",
            "SURMOUNT_REDIRECT_ALLOWED_HOSTS",
            "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE",
            "SURMOUNT_PRIMARY_DOMAIN",
            "SURMOUNT_MAIL_HOSTNAME",
            "SURMOUNT_SERVICES_HOSTNAME",
            "SURMOUNT_STALWART_URL",
            "SURMOUNT_ONION_URL",
            "SURMOUNT_ONION_HOSTNAME_FILE",
            "SURMOUNT_RATE_LIMIT_MAX",
            "SURMOUNT_RATE_LIMIT_WINDOW_SECS",
            "SURMOUNT_RATE_LIMIT_MAX_KEYS",
            "SURMOUNT_BAN_ENFORCEMENT",
            "SURMOUNT_BAN_BACKEND",
            "SURMOUNT_BAN_WHITELIST",
            "SURMOUNT_BAN_STATE_PATH",
            "SURMOUNT_BAN_NFT_EXEC",
            "SURMOUNT_BAN_NFT_BIN",
            "SURMOUNT_BAN_NFT_HELPER",
            "SURMOUNT_BAN_NFT_HELPER_SOCK",
            "SURMOUNT_AUTH_MODE",
            "SURMOUNT_NOSTR_ALLOWLIST",
            "SURMOUNT_NOSTR_ALLOWLIST_FILE",
            "SURMOUNT_SESSION_SECRET",
            "SURMOUNT_SESSION_TTL_SECS",
            "SURMOUNT_PUBLIC_BASE_URL",
            "SURMOUNT_NIP98_MAX_SKEW_SECS",
        ] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn auth_mode_default_off() {
        let _g = EnvGuard::acquire();
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.auth.mode, AuthMode::Off);
        assert!(cfg.auth.allowlist.is_empty());
    }

    #[test]
    fn auth_mode_nostr_requires_session_secret() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_AUTH_MODE", "nostr");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("SESSION_SECRET") || err.contains("session"),
            "{err}"
        );
    }

    #[test]
    fn auth_mode_nostr_with_secret_ok() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_AUTH_MODE", "nostr");
        std::env::set_var("SURMOUNT_SESSION_SECRET", "dev-only-test-secret");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.auth.mode, AuthMode::Nostr);
        assert!(!cfg.auth.session_secret.as_ref().unwrap().is_empty());
    }

    /// Named contract: allowlist file loads when env empty; env wins when set.
    #[test]
    fn nostr_allowlist_file_and_env_precedence() {
        use surmount_management_ui::auth::allowlist_contains;

        let _g = EnvGuard::acquire();
        let keys = nostr::Keys::generate();
        let file_hex = keys.public_key().to_hex();
        let env_keys = nostr::Keys::generate();
        let env_hex = env_keys.public_key().to_hex();
        let dir =
            std::env::temp_dir().join(format!("surmount-cfg-allowlist-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("allow.txt");
        std::fs::write(&file, format!("{file_hex}\n")).unwrap();

        std::env::set_var("SURMOUNT_AUTH_MODE", "nostr");
        std::env::set_var("SURMOUNT_SESSION_SECRET", "dev-only-test-secret");
        std::env::set_var("SURMOUNT_NOSTR_ALLOWLIST_FILE", file.to_str().unwrap());
        let cfg = AppConfig::from_env().unwrap();
        assert!(
            allowlist_contains(&cfg.auth.allowlist, &file_hex),
            "file allowlist should load when env empty"
        );

        std::env::set_var("SURMOUNT_NOSTR_ALLOWLIST", &env_hex);
        let cfg2 = AppConfig::from_env().unwrap();
        assert!(
            allowlist_contains(&cfg2.auth.allowlist, &env_hex),
            "env must win over file"
        );
        assert!(
            !allowlist_contains(&cfg2.auth.allowlist, &file_hex),
            "file keys must not mix when env wins"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_listen_is_loopback_8080() {
        let _g = EnvGuard::acquire();
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.listen.port(), 8080);
        assert!(cfg.listen.ip().is_loopback());
        assert_eq!(cfg.primary_domain, "surmount.systems");
        assert_eq!(cfg.listen_mode, ListenMode::PlainHttp);
        assert!(!cfg.redirect_http_to_https);
        assert!(!cfg.https_allow_cleartext_escape);
        assert!(cfg.rate_limiter().is_some());
        assert_eq!(cfg.ban.enforcement, BanEnforcement::Off);
        assert!(cfg
            .redirect_allowed_hosts
            .iter()
            .any(|h| h == "services.surmount.systems"));
    }

    #[test]
    fn ban_invalid_enforcement_is_config_error() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_BAN_ENFORCEMENT", "sometimes");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("SURMOUNT_BAN_ENFORCEMENT"), "{err}");
    }

    #[test]
    fn ban_enforce_nft_exec_without_bin_is_config_error() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_BAN_ENFORCEMENT", "enforce");
        std::env::set_var("SURMOUNT_BAN_BACKEND", "nft");
        std::env::set_var("SURMOUNT_BAN_NFT_EXEC", "1");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("NFT_BIN"),
            "{err}"
        );
    }

    #[test]
    fn ban_invalid_nft_exec_is_config_error() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_BAN_NFT_EXEC", "maybe");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("SURMOUNT_BAN_NFT_EXEC"), "{err}");
    }

    #[test]
    fn https_mode_requires_tls_paths() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN_MODE", "https");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("SURMOUNT_TLS_CERT"), "{err}");
    }

    #[test]
    fn https_mode_loads_tls_paths() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN_MODE", "https");
        std::env::set_var("SURMOUNT_TLS_CERT", "/run/surmount-secrets/tls/cert.pem");
        std::env::set_var("SURMOUNT_TLS_KEY", "/run/surmount-secrets/tls/key.pem");
        std::env::set_var("SURMOUNT_REDIRECT_HTTP_TO_HTTPS", "true");
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "127.0.0.1:8080");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.listen_mode.is_https());
        assert!(cfg.redirect_http_to_https);
        assert_eq!(
            cfg.http_redirect_listen.unwrap().to_string(),
            "127.0.0.1:8080"
        );
    }

    #[test]
    fn rate_limit_zero_disables_limiter() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_RATE_LIMIT_MAX", "0");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.rate_limiter().is_none());
    }

    #[test]
    fn cleartext_escape_env_defaults_off() {
        let _g = EnvGuard::acquire();
        assert!(!AppConfig::from_env().unwrap().https_allow_cleartext_escape);
        std::env::set_var("SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE", "1");
        assert!(AppConfig::from_env().unwrap().https_allow_cleartext_escape);
    }

    #[test]
    fn invalid_http_redirect_listen_is_config_error() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_REDIRECT_HTTP_TO_HTTPS", "true");
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "not-a-socket");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("SURMOUNT_HTTP_REDIRECT_LISTEN") && err.contains("not-a-socket"),
            "{err}"
        );
    }

    #[test]
    fn empty_http_redirect_listen_is_none() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "   ");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.http_redirect_listen.is_none());
    }

    #[test]
    fn local_cleartext_listen_parses_loopback() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "0.0.0.0:443");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(
            cfg.local_cleartext_listen.unwrap().to_string(),
            "127.0.0.1:8090"
        );
        cfg.validate_local_cleartext().unwrap();
    }

    #[test]
    fn local_cleartext_empty_is_none() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "  ");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.local_cleartext_listen.is_none());
    }

    #[test]
    fn local_cleartext_invalid_is_config_error() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "not-a-socket");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("SURMOUNT_LOCAL_CLEARTEXT_LISTEN") && err.contains("not-a-socket"),
            "{err}"
        );
    }

    #[test]
    fn local_cleartext_must_be_loopback() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "127.0.0.1:443");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "0.0.0.0:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_from_primary() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "127.0.0.1:8090");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("differ") && err.contains("LISTEN"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_port_from_wildcard_primary() {
        // 0.0.0.0:8090 + 127.0.0.1:8090 looks like different SocketAddrs but
        // Linux bind collides. Fail closed at validate (not only at bind).
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "0.0.0.0:8090");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("port") && err.contains("8090"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_from_redirect() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "0.0.0.0:443");
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "127.0.0.1:8080");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8080");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("differ") && err.contains("REDIRECT"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_port_from_redirect() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "0.0.0.0:443");
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "0.0.0.0:8090");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("port") && err.contains("REDIRECT"), "{err}");
    }

    // v3 onion label length is 56 base32 chars (a-z, 2-7).
    const SAMPLE_V3: &str = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx";

    /// Named contract: bare .onion becomes http:// link target; empty is unset.
    #[test]
    fn normalize_onion_url_bare_and_empty() {
        assert_eq!(normalize_onion_url("  "), None);
        let bare = format!("{SAMPLE_V3}.onion");
        assert_eq!(
            normalize_onion_url(&bare),
            Some(format!("http://{SAMPLE_V3}.onion"))
        );
        assert_eq!(
            normalize_onion_url(&format!("http://{SAMPLE_V3}.onion/")),
            Some(format!("http://{SAMPLE_V3}.onion"))
        );
    }

    /// Named contract: SURMOUNT_ONION_URL wins; unset by default (no invented onion).
    #[test]
    fn onion_url_from_env_wins_over_file() {
        let _g = EnvGuard::acquire();
        let dir = std::env::temp_dir().join(format!("surmount-onion-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("hostname");
        let file_onion = "b".repeat(56);
        let env_onion = "a".repeat(56);
        std::fs::write(&file, format!("{file_onion}.onion\n")).unwrap();
        std::env::set_var("SURMOUNT_ONION_HOSTNAME_FILE", file.to_str().unwrap());
        std::env::set_var("SURMOUNT_ONION_URL", format!("{env_onion}.onion"));
        let cfg = AppConfig::from_env().unwrap();
        let expected = format!("http://{env_onion}.onion");
        assert_eq!(cfg.onion_url.as_deref(), Some(expected.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn onion_url_from_file_when_env_unset() {
        let _g = EnvGuard::acquire();
        let dir = std::env::temp_dir().join(format!("surmount-onion-file-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("hostname");
        let file_onion = "c".repeat(56);
        std::fs::write(&file, format!("{file_onion}.onion\n")).unwrap();
        std::env::set_var("SURMOUNT_ONION_HOSTNAME_FILE", file.to_str().unwrap());
        let cfg = AppConfig::from_env().unwrap();
        let expected = format!("http://{file_onion}.onion");
        assert_eq!(cfg.onion_url.as_deref(), Some(expected.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn onion_url_default_unset() {
        let _g = EnvGuard::acquire();
        let cfg = AppConfig::from_env().unwrap();
        assert!(
            cfg.onion_url.is_none(),
            "must not invent onion: {:?}",
            cfg.onion_url
        );
    }

    #[test]
    fn empty_onion_file_is_unset() {
        let _g = EnvGuard::acquire();
        let dir = std::env::temp_dir().join(format!("surmount-onion-empty-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("hostname");
        std::fs::write(&file, "   \n\n").unwrap();
        std::env::set_var("SURMOUNT_ONION_HOSTNAME_FILE", file.to_str().unwrap());
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.onion_url.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Named contract: log/error tails must not print full onion labels.
    #[test]
    fn redact_onion_in_text_masks_v3() {
        let onion = format!("{SAMPLE_V3}.onion");
        let line = format!("fetch failed for http://{onion}/health");
        let red = redact_onion_in_text(&line);
        assert!(
            red.contains("<onion-redacted>"),
            "expected redaction marker: {red}"
        );
        assert!(
            !red.contains("abcdefghijklmnopqrstuvwxyz234567"),
            "must not leak onion body: {red}"
        );
    }

    /// Named contract: continue past non-label `.onion` and redact later v3 labels.
    #[test]
    fn redact_onion_in_text_skips_non_label_and_masks_all_v3() {
        let o1 = format!("{SAMPLE_V3}.onion");
        let o2_label = "z".repeat(56);
        let o2 = format!("{o2_label}.onion");
        // "not.onion" is not 16/56 base32; must not stop the scan.
        let line = format!("note not.onion then http://{o1}/a and {o2} end");
        let red = redact_onion_in_text(&line);
        assert!(
            red.contains("not.onion"),
            "non-label .onion should remain: {red}"
        );
        assert_eq!(
            red.matches("<onion-redacted>").count(),
            2,
            "both v3 labels should redact: {red}"
        );
        assert!(
            !red.contains(&SAMPLE_V3[..20]) && !red.contains(&o2_label[..20]),
            "must not leak onion bodies: {red}"
        );
    }

    #[test]
    fn redact_onion_in_text_masks_legacy_16() {
        let legacy = format!("{}.onion", "a".repeat(16));
        let red = redact_onion_in_text(&format!("via {legacy}"));
        assert!(red.contains("<onion-redacted>"), "{red}");
        assert!(!red.contains(&"a".repeat(16)), "{red}");
    }

    /// Named contract: truthy flag SoT includes `on` (same set as historic env_bool).
    #[test]
    fn parse_env_flag_truthy_accepts_on_and_common_tokens() {
        assert!(parse_env_flag_truthy("on"));
        assert!(parse_env_flag_truthy("ON"));
        assert!(parse_env_flag_truthy("1"));
        assert!(parse_env_flag_truthy("true"));
        assert!(parse_env_flag_truthy("yes"));
        assert!(!parse_env_flag_truthy("off"));
        assert!(!parse_env_flag_truthy("0"));
        assert!(!parse_env_flag_truthy("false"));
        assert!(!parse_env_flag_truthy(""));
    }
}
