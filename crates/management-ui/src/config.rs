//! Runtime configuration from environment (set by systemd unit / Nix module).

use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use crate::tls::{ListenMode, TlsPaths};
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
    /// Max requests per client key per window (0 disables limiter).
    pub rate_limit_max_requests: u32,
    pub rate_limit_window: Duration,
    pub rate_limit_max_keys: usize,
    /// Ban/whitelist subsystem (default enforcement off; lean private).
    pub ban: BanConfig,
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
            rate_limit_max_requests,
            rate_limit_window: Duration::from_secs(rate_limit_window_secs.max(1)),
            rate_limit_max_keys: rate_limit_max_keys.max(1),
            ban,
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

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => default,
    }
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
        ] {
            std::env::remove_var(k);
        }
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
        assert!(
            err.contains("port") && err.contains("8090"),
            "{err}"
        );
    }

    #[test]
    fn local_cleartext_must_differ_from_redirect() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "0.0.0.0:443");
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "127.0.0.1:8080");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8080");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(
            err.contains("differ") && err.contains("REDIRECT"),
            "{err}"
        );
    }

    #[test]
    fn local_cleartext_must_differ_port_from_redirect() {
        let _g = EnvGuard::acquire();
        std::env::set_var("SURMOUNT_LISTEN", "0.0.0.0:443");
        std::env::set_var("SURMOUNT_HTTP_REDIRECT_LISTEN", "0.0.0.0:8090");
        std::env::set_var("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(
            err.contains("port") && err.contains("REDIRECT"),
            "{err}"
        );
    }
}
