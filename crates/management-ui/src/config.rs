//! Runtime configuration from environment (set by systemd unit / Nix module).

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::acme::AcmeConfig;
use crate::mta_sts::MtaStsMode;
use crate::proxy_vaultwarden::VaultwardenProxyConfig;
use crate::proxy_vaultwarden::splora::SploraProxyConfig;
use crate::tls::http3::Http3Config;
use crate::tls::{ListenMode, TlsPaths};
use surmount_management_ui::auth::{AuthConfig, AuthMode, resolve_allowlist};
use surmount_management_ui::ban::{BanConfig, ban_config_from_env};
use surmount_management_ui::rate_limit::{DEFAULT_MAX_KEYS, FixedWindowRateLimiter};

/// Unique unused map path for hermetic tests. Never a host secret.
#[cfg(test)]
pub fn unused_console_accounts_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "surmount-console-accounts-unused-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// Unique unused NWC store path for hermetic tests. Never a host secret.
#[cfg(test)]
pub fn unused_nwc_store_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "surmount-nwc-unused-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ))
}

/// Hermetic `example.test` fixture. Tests mutate copies.
#[cfg(test)]
impl AppConfig {
    pub fn example_test() -> Self {
        use crate::tls::ListenMode;
        use surmount_management_ui::ban::{BanBackendKind, BanEnforcement};
        Self {
            listen: "127.0.0.1:8090".parse().unwrap(),
            http_redirect_listen: None,
            local_cleartext_listen: None,
            listen_mode: ListenMode::PlainHttp,
            redirect_http_to_https: false,
            redirect_allowed_hosts: vec!["services.example.test".into()],
            https_allow_cleartext_escape: false,
            acme: crate::acme::AcmeConfig::default(),
            mta_sts_mode: crate::mta_sts::MtaStsMode::Off,
            mta_sts_max_age: 86_400,
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_surface: OnionSurface::NotProvisioned,
            onion_url: None,
            onion_discovery: crate::onion_discovery::OnionDiscoveryConfig::empty(),
            vaultwarden_url: None,
            vaultwarden_proxy: crate::proxy_vaultwarden::VaultwardenProxyConfig::default(),
            splora_proxy: crate::proxy_vaultwarden::splora::SploraProxyConfig::default(),
            http3: crate::tls::http3::Http3Config::default(),
            apex_public_root: None,
            static_vhosts: BTreeMap::new(),
            extra_mail_hostnames: Vec::new(),
            rate_limit_max_requests: 0,
            rate_limit_window: Duration::from_secs(60),
            rate_limit_max_keys: 1000,
            ban: BanConfig {
                enforcement: BanEnforcement::Off,
                backend_kind: BanBackendKind::Memory,
                whitelist: vec![],
                state_path: None,
                nft_exec_enabled: false,
                nft_bin: None,
                nft_helper_sock: None,
                nft_helper_bin: None,
            },
            auth: surmount_management_ui::auth::AuthConfig::off(),
            allow_directory_unauthenticated: false,
            allow_public_auth_off: false,
            console_accounts_path: unused_console_accounts_path(),
            nwc_store_path: unused_nwc_store_path(),
        }
    }
}

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
    /// In-process ACME (default off). See `crate::acme`.
    pub acme: AcmeConfig,
    /// MTA-STS policy mode (default off). See `crate::mta_sts`.
    pub mta_sts_mode: MtaStsMode,
    /// RFC max_age for MTA-STS policy body when mode is enabled.
    pub mta_sts_max_age: u64,
    pub primary_domain: String,
    pub mail_hostname: String,
    pub services_hostname: String,
    pub stalwart_url: String,
    /// Structured onion surface (configured / hostname missing / not provisioned).
    /// Never invents a live .onion; only host material or lab override.
    pub onion_surface: OnionSurface,
    /// Operator-published onion URL when [`OnionSurface::Configured`]; else None.
    /// Convenience for links; same as `onion_surface.url()`.
    pub onion_url: Option<String>,
    /// Clearnet Host -> onion discovery (`Onion-Location` + `Alt-Svc`).
    /// Loaded once at process start. No SIGHUP or file-watch hot-reload;
    /// restart the unit after mapping or hostname-file changes.
    pub onion_discovery: crate::onion_discovery::OnionDiscoveryConfig,
    /// Operator-published Vaultwarden URL for console link (domain C human vault).
    /// Never invented; empty env = unset. No admin token, never log secrets.
    pub vaultwarden_url: Option<String>,
    /// Optional Axum path reverse-proxy to loopback Vaultwarden (default off).
    pub vaultwarden_proxy: VaultwardenProxyConfig,
    /// Optional Host -> Unix-socket reverse-proxy for splora indexers (default off).
    pub splora_proxy: SploraProxyConfig,
    /// HTTP/3 QUIC listener (default on when listen mode is https).
    pub http3: Http3Config,
    /// Host directory for the public apex/www static site. None = coming soon.
    /// Env `SURMOUNT_APEX_PUBLIC_ROOT` (empty/unset = None). Served only when
    /// that directory contains `index.html`.
    pub apex_public_root: Option<PathBuf>,
    /// Extra clearnet Host -> document root (not apex/www, not services).
    /// Env `SURMOUNT_STATIC_VHOSTS` (JSON object hostname -> root) or
    /// `SURMOUNT_STATIC_VHOSTS_FILE` (same JSON on disk). Empty = none.
    /// Do not overload `SURMOUNT_APEX_PUBLIC_ROOT` for these names.
    pub static_vhosts: BTreeMap<String, PathBuf>,
    /// Extra mail Hosts that are not `mail_hostname` and not static vhost keys
    /// (for example `mail.cryptoquick.com` on the production leaf).
    /// Env `SURMOUNT_EXTRA_MAIL_HOSTNAMES` (comma list). Empty = none.
    /// Do not parse `/etc/surmount/mail-domains.txt` as source of truth.
    pub extra_mail_hostnames: Vec<String>,
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
    /// Lab escape: when true, public primary listen may run with
    /// `auth_mode=off`. Never production default. Env
    /// `SURMOUNT_ALLOW_PUBLIC_AUTH_OFF`. Prefer loopback/private binds for
    /// auth-off lab instead of this flag.
    pub allow_public_auth_off: bool,
    /// Surmount console account map (optional npub + Administrator/User).
    /// Env `SURMOUNT_CONSOLE_ACCOUNTS`; default
    /// `/var/lib/surmount/console/accounts.json`. Not the Nostr allowlist.
    pub console_accounts_path: PathBuf,
    /// Contributor NWC URI store (Domain B). Env `SURMOUNT_NWC_STORE`; default
    /// `/var/lib/surmount/secrets/ui/nwc.json`. Never nsec. Never git.
    pub nwc_store_path: PathBuf,
}

/// Product onion status for console + `GET /api/v1/system`.
///
/// Host path is real Arti HS material under `surmount.artiHiddenService`, not a
/// local demo. Lab overrides (`SURMOUNT_ONION_URL` / hostname file) remain for
/// tests but are not the primary operator story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum OnionSurface {
    /// Hostname or URL present; safe to show as operator-published address.
    Configured { url: String },
    /// Module/env pointed at a path, but hostname material is missing/empty.
    HostnameMissing {
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        hs_state_dir: Option<String>,
    },
    /// No onion surface configured for this process (Arti HS not provisioned).
    NotProvisioned,
}

impl OnionSurface {
    pub fn not_provisioned() -> Self {
        Self::NotProvisioned
    }

    /// Build from an optional URL (fixtures / tests). None = not provisioned.
    pub fn from_url_opt(url: Option<String>) -> Self {
        match url {
            Some(url) => Self::Configured { url },
            None => Self::NotProvisioned,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match self {
            Self::Configured { url } => Some(url.as_str()),
            Self::HostnameMissing { .. } | Self::NotProvisioned => None,
        }
    }

    /// Stable API slug: `configured` | `hostname_missing` | `not_provisioned`.
    pub fn status_slug(&self) -> &'static str {
        match self {
            Self::Configured { .. } => "configured",
            Self::HostnameMissing { .. } => "hostname_missing",
            Self::NotProvisioned => "not_provisioned",
        }
    }
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

/// Walk an Arti HS state directory for a `hostname` or `*.onion` file.
///
/// Bounded depth/file count so a mis-pointed tree cannot hang the process.
/// Does not invent an address; only reads host material if present.
pub fn onion_url_from_hs_state_dir(dir: &Path) -> Option<String> {
    const MAX_DEPTH: u32 = 4;
    const MAX_FILES: usize = 64;
    let mut stack: Vec<(PathBuf, u32)> = vec![(dir.to_path_buf(), 0)];
    let mut seen = 0usize;
    while let Some((path, depth)) = stack.pop() {
        if seen >= MAX_FILES {
            break;
        }
        let entries = match std::fs::read_dir(&path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for ent in entries.flatten() {
            if seen >= MAX_FILES {
                break;
            }
            let p = ent.path();
            let ft = match ent.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if ft.is_dir() {
                if depth < MAX_DEPTH {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            seen += 1;
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if (name == "hostname" || name.ends_with(".onion"))
                && let Some(url) = onion_url_from_file(&p)
            {
                return Some(url);
            }
        }
    }
    None
}

/// Resolve structured onion surface from env + host paths.
///
/// Priority: `SURMOUNT_ONION_URL` (lab/override) when non-empty; else readable
/// `SURMOUNT_ONION_HOSTNAME_FILE`; else walk `SURMOUNT_ONION_HS_STATE_DIR` for
/// hostname material (real Arti HS dir). Path set but empty/unreadable =>
/// [`OnionSurface::HostnameMissing`]. Nothing set =>
/// [`OnionSurface::NotProvisioned`]. Never invents a live onion.
pub fn resolve_onion_surface_from_env() -> OnionSurface {
    if let Ok(v) = env::var("SURMOUNT_ONION_URL")
        && let Some(n) = normalize_onion_url(&v)
    {
        return OnionSurface::Configured { url: n };
    }

    let hostname_file = env::var("SURMOUNT_ONION_HOSTNAME_FILE")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let hs_state_dir = env::var("SURMOUNT_ONION_HS_STATE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(ref p) = hostname_file
        && let Some(url) = onion_url_from_file(Path::new(p))
    {
        return OnionSurface::Configured { url };
    }
    if let Some(ref d) = hs_state_dir
        && let Some(url) = onion_url_from_hs_state_dir(Path::new(d))
    {
        return OnionSurface::Configured { url };
    }

    if hostname_file.is_some() || hs_state_dir.is_some() {
        return OnionSurface::HostnameMissing {
            path: hostname_file,
            hs_state_dir,
        };
    }
    OnionSurface::NotProvisioned
}

/// Normalize an operator Vaultwarden URL for display and external links.
///
/// Accepts `http(s)://…`, bare `host:port`, or whitespace-padded forms.
/// Empty / whitespace-only -> `None`. Bare host becomes `http://…`.
/// Refuses non-http(s) schemes (`javascript:`, `data:`, `file:`, …),
/// mid-string whitespace / control chars, and Environment=-unsafe characters.
/// Does not invent a live vault; does not read admin tokens.
pub fn normalize_vaultwarden_url(raw: &str) -> Option<String> {
    let t = raw.trim().trim_end_matches('/').trim();
    if t.is_empty() {
        return None;
    }
    // Env / HTML href hygiene: no control chars, whitespace mid-string, or
    // shell/env breakers after the outer trim.
    if t.chars().any(|c| {
        c.is_control()
            || c.is_whitespace()
            || matches!(c, '"' | '\'' | '`' | '$' | ';' | '\\' | '\0')
    }) {
        return None;
    }
    let normalized = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else if t.contains("://") {
        // Other schemes (javascript:, data:, file:, …) refused.
        return None;
    } else {
        format!("http://{t}")
    };
    // Charset aligned with Nix management-ui vaultwardenUrlShapeOk.
    if !normalized.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                ':' | '/'
                    | '.'
                    | '_'
                    | '~'
                    | '-'
                    | '%'
                    | '@'
                    | '&'
                    | '='
                    | '+'
                    | '['
                    | ']'
                    | '?'
                    | '#'
            )
    }) {
        return None;
    }
    Some(normalized)
}

/// Resolve Vaultwarden URL from `SURMOUNT_VAULTWARDEN_URL` only.
/// Empty / unset = residual not configured. Configured marker is non-empty URL.
pub fn resolve_vaultwarden_url_from_env() -> Option<String> {
    match env::var("SURMOUNT_VAULTWARDEN_URL") {
        Ok(v) => normalize_vaultwarden_url(&v),
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
                // Default open-redirect allowlist: services (operator console),
                // apex + www (public COMING SOON; same-host HTTPS upgrade),
                // mail name, MTA-STS policy host.
                vec![
                    services_hostname.clone(),
                    primary_domain.clone(),
                    format!("www.{primary_domain}"),
                    mail_hostname.clone(),
                    format!("mta-sts.{primary_domain}"),
                ]
            });

        // MTA-STS skeleton: default off until public HTTPS policy host is ready.
        let mta_sts_mode = match env::var("SURMOUNT_MTA_STS_MODE") {
            Ok(s) => MtaStsMode::parse(&s)?,
            Err(_) => MtaStsMode::Off,
        };
        let mta_sts_max_age = env::var("SURMOUNT_MTA_STS_MAX_AGE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(86_400);

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

        // Extra static Hosts first so onion auto-map can include them.
        // JSON object hostname -> root. Env wins over file.
        let static_vhosts = static_vhosts_from_env()?;
        let extra_mail_hostnames = extra_mail_hostnames_from_env();
        let redirect_allowed_hosts =
            union_static_vhost_hosts(redirect_allowed_hosts, &static_vhosts);
        let extra_onion_hosts: Vec<String> = static_vhosts.keys().cloned().collect();

        // Onion: structured surface from env + host HS paths. Never invent.
        let onion_surface = resolve_onion_surface_from_env();
        let onion_url = onion_surface.url().map(str::to_string);
        // Discovery map is start-of-process only (no hot-reload).
        let onion_discovery = crate::onion_discovery::onion_discovery_from_env(
            &primary_domain,
            &services_hostname,
            onion_url.as_deref(),
            &extra_onion_hosts,
        );
        // Vaultwarden: operator-published URL only. Never invent; no admin token.
        let vaultwarden_url = resolve_vaultwarden_url_from_env();
        // Path proxy to loopback Rocket (default off). Fail-closed on bad prefix/upstream.
        let vaultwarden_proxy = VaultwardenProxyConfig::from_env()?;
        let splora_proxy = SploraProxyConfig::from_env()?;
        let http3 = Http3Config::from_env_map(|k| env::var(k).ok(), listen_mode.is_https())?;
        let redirect_allowed_hosts = union_splora_hosts(redirect_allowed_hosts, &splora_proxy);

        // Public apex/www document root. Empty/unset = coming-soon fallback.
        let apex_public_root = match env::var("SURMOUNT_APEX_PUBLIC_ROOT") {
            Ok(s) => {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(t))
                }
            }
            Err(_) => None,
        };

        let auth = auth_config_from_env()?;
        // Prefer bind detection: public primary + auth off fails closed.
        // Lab-only SURMOUNT_ALLOW_PUBLIC_AUTH_OFF=1 for intentional public+off tests.
        let allow_public_auth_off = env_bool("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF", false);
        auth.validate_for_listen(listen, allow_public_auth_off)?;

        // Single SoT with directory process-start coupling (same env + truthy parser).
        let allow_directory_unauthenticated =
            crate::directory::directory_allow_unauthenticated_from_env();

        // ACME default-off; validate() fails closed when enable is incomplete.
        let acme = AcmeConfig::from_env()?;

        let cfg = Self {
            listen,
            http_redirect_listen,
            local_cleartext_listen,
            listen_mode,
            redirect_http_to_https,
            redirect_allowed_hosts,
            https_allow_cleartext_escape: env_bool("SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE", false),
            acme,
            mta_sts_mode,
            mta_sts_max_age,
            primary_domain,
            mail_hostname,
            services_hostname,
            stalwart_url: env::var("SURMOUNT_STALWART_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8081".into()),
            onion_surface,
            onion_url,
            onion_discovery,
            vaultwarden_url,
            vaultwarden_proxy,
            splora_proxy,
            http3,
            apex_public_root,
            static_vhosts,
            extra_mail_hostnames,
            rate_limit_max_requests,
            rate_limit_window: Duration::from_secs(rate_limit_window_secs.max(1)),
            rate_limit_max_keys: rate_limit_max_keys.max(1),
            ban,
            auth,
            allow_directory_unauthenticated,
            allow_public_auth_off,
            console_accounts_path:
                surmount_management_ui::console_accounts::console_accounts_path_from_env(),
            nwc_store_path: surmount_management_ui::nwc::nwc_store_path_from_env(),
        };
        // ACME enable requires HTTPS listen + absolute TLS paths (mirrors Nix).
        cfg.validate_acme_listen()?;
        Ok(cfg)
    }

    /// When ACME is enabled, require HTTPS listen mode and absolute cert/key paths.
    ///
    /// Env-only ACME under plain HTTP would otherwise no-op silently at startup
    /// (`ensure_tls_material` only runs when `tls_paths()` is Some). Fail closed.
    pub fn validate_acme_listen(&self) -> Result<(), String> {
        if !self.acme.enable {
            return Ok(());
        }
        let Some(paths) = self.tls_paths() else {
            return Err(
                "SURMOUNT_ACME_ENABLE requires SURMOUNT_LISTEN_MODE=https and \
                 absolute SURMOUNT_TLS_CERT / SURMOUNT_TLS_KEY paths (fail-closed; \
                 ACME under plain HTTP is not supported)"
                    .into(),
            );
        };
        if !paths.cert_path.is_absolute() {
            return Err(format!(
                "SURMOUNT_TLS_CERT must be an absolute host path when ACME is enabled \
                 (got {:?}; fail-closed)",
                paths.cert_path
            ));
        }
        if !paths.key_path.is_absolute() {
            return Err(format!(
                "SURMOUNT_TLS_KEY must be an absolute host path when ACME is enabled \
                 (got {:?}; fail-closed)",
                paths.key_path
            ));
        }
        Ok(())
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

/// Extra mail Hosts from `SURMOUNT_EXTRA_MAIL_HOSTNAMES` (comma list).
/// Empty or unset is none. Names are trimmed and lowercased. Duplicates drop.
pub fn extra_mail_hostnames_from_env() -> Vec<String> {
    extra_mail_hostnames_from_csv(
        env::var("SURMOUNT_EXTRA_MAIL_HOSTNAMES")
            .ok()
            .as_deref()
            .unwrap_or(""),
    )
}

/// Parse a comma-separated extra-mail hostname list (same rules as env).
pub fn extra_mail_hostnames_from_csv(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for part in raw.split(',') {
        let host = part.trim().to_ascii_lowercase();
        if host.is_empty() {
            continue;
        }
        if seen.insert(host.clone()) {
            out.push(host);
        }
    }
    out
}

/// Parse extra static vhosts from `SURMOUNT_STATIC_VHOSTS` (JSON object) or
/// `SURMOUNT_STATIC_VHOSTS_FILE` (same JSON). Env wins when non-empty.
pub fn static_vhosts_from_env() -> Result<BTreeMap<String, PathBuf>, String> {
    if let Ok(raw) = env::var("SURMOUNT_STATIC_VHOSTS") {
        let t = raw.trim();
        if !t.is_empty() {
            return parse_static_vhosts_json(t);
        }
    }
    match env::var("SURMOUNT_STATIC_VHOSTS_FILE") {
        Ok(p) => {
            let path = p.trim();
            if path.is_empty() {
                return Ok(BTreeMap::new());
            }
            let body = std::fs::read_to_string(path)
                .map_err(|e| format!("SURMOUNT_STATIC_VHOSTS_FILE={path:?} is unreadable: {e}"))?;
            parse_static_vhosts_json(&body)
        }
        Err(_) => Ok(BTreeMap::new()),
    }
}

/// JSON object: `{ "extra.test": "/var/lib/surmount/static-sites/extra" }`.
/// Keys are lowercased DNS hostnames (port stripped). Values are document roots.
pub fn parse_static_vhosts_json(raw: &str) -> Result<BTreeMap<String, PathBuf>, String> {
    let value: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| {
        format!("SURMOUNT_STATIC_VHOSTS must be a JSON object hostname -> root: {e}")
    })?;
    let obj = value.as_object().ok_or_else(|| {
        "SURMOUNT_STATIC_VHOSTS must be a JSON object (hostname -> document root)".to_string()
    })?;
    let mut out = BTreeMap::new();
    for (host, root_val) in obj {
        let host = normalize_static_vhost_hostname(host)?;
        let root = root_val.as_str().ok_or_else(|| {
            format!("SURMOUNT_STATIC_VHOSTS[{host:?}] must be a string document-root path")
        })?;
        let root = root.trim();
        if root.is_empty() {
            return Err(format!(
                "SURMOUNT_STATIC_VHOSTS[{host:?}] document root must not be empty"
            ));
        }
        if root.contains('\0') {
            return Err(format!(
                "SURMOUNT_STATIC_VHOSTS[{host:?}] document root must not contain NUL"
            ));
        }
        out.insert(host, PathBuf::from(root));
    }
    Ok(out)
}

/// Lowercase Host, strip port, refuse empty / control / path characters.
pub fn normalize_static_vhost_hostname(raw: &str) -> Result<String, String> {
    let host = crate::redirect::host_for_url_authority(raw)
        .trim()
        .to_ascii_lowercase();
    if host.is_empty() {
        return Err("static vhost hostname must not be empty".into());
    }
    if host.len() > 253 {
        return Err(format!("static vhost hostname too long ({})", host.len()));
    }
    if host.contains("..") || host.starts_with('.') || host.ends_with('.') {
        return Err(format!("static vhost hostname refused: {host}"));
    }
    if !host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
    {
        return Err(format!(
            "static vhost hostname has invalid characters: {host}"
        ));
    }
    if !host.contains('.') {
        return Err(format!(
            "static vhost hostname must be a DNS name with a dot: {host}"
        ));
    }
    Ok(host)
}

/// Ensure configured extra Hosts can :80-upgrade even when the operator set
/// `SURMOUNT_REDIRECT_ALLOWED_HOSTS` without listing them.
/// Ensure splora public Hosts can :80-upgrade when the proxy is on.
pub fn union_splora_hosts(mut allowed: Vec<String>, splora: &SploraProxyConfig) -> Vec<String> {
    if !splora.enable {
        return allowed;
    }
    for inst in &splora.instances {
        for host in &inst.hosts {
            if !allowed
                .iter()
                .any(|a| crate::redirect::host_is_allowlisted(host, std::slice::from_ref(a)))
            {
                allowed.push(host.to_string());
            }
        }
    }
    allowed
}

pub fn union_static_vhost_hosts(
    mut allowed: Vec<String>,
    vhosts: &BTreeMap<String, PathBuf>,
) -> Vec<String> {
    for host in vhosts.keys() {
        if !allowed
            .iter()
            .any(|a| crate::redirect::host_is_allowlisted(host, std::slice::from_ref(a)))
        {
            allowed.push(host.clone());
        }
    }
    allowed
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

    /// Edition 2024: `set_var`/`remove_var` are unsafe (process-global).
    /// Call only under `EnvGuard` (serialized tests).
    fn set_env(key: &str, val: impl AsRef<std::ffi::OsStr>) {
        // SAFETY: EnvGuard mutex serializes process env mutation in this module's tests.
        unsafe { std::env::set_var(key, val) }
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
            "SURMOUNT_MTA_STS_MODE",
            "SURMOUNT_MTA_STS_MAX_AGE",
            "SURMOUNT_ACME_ENABLE",
            "SURMOUNT_ACME_DIRECTORY",
            "SURMOUNT_ACME_EMAIL",
            "SURMOUNT_ACME_DOMAINS",
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH",
            "SURMOUNT_ACME_CHALLENGE",
            "SURMOUNT_ACME_DNS_PROVIDER",
            "SURMOUNT_ACME_DNS_HOOK",
            "SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS",
            "SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY",
            "SURMOUNT_PRIMARY_DOMAIN",
            "SURMOUNT_MAIL_HOSTNAME",
            "SURMOUNT_SERVICES_HOSTNAME",
            "SURMOUNT_STALWART_URL",
            "SURMOUNT_ONION_URL",
            "SURMOUNT_ONION_HOSTNAME_FILE",
            "SURMOUNT_ONION_HS_STATE_DIR",
            "SURMOUNT_ONION_MAP_FILE",
            "SURMOUNT_ONION_LOCATION_ENABLED",
            "SURMOUNT_ONION_ALT_SVC_ENABLED",
            "SURMOUNT_ONION_LOCATION_DISABLED_HOSTS",
            "SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS",
            "SURMOUNT_VAULTWARDEN_URL",
            "SURMOUNT_VAULTWARDEN_PROXY",
            "SURMOUNT_SPLORA_PROXY",
            "SURMOUNT_SPLORA_INSTANCES",
            "SURMOUNT_SPLORA_SOCKET_DIR",
            "SURMOUNT_SPLORA_QUEUE_SOCKET",
            "SURMOUNT_SPLORA_QUEUE_PATH",
            "SURMOUNT_SPLORA_ELECTRUM_SOCKET",
            "SURMOUNT_HTTP3",
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
            "SURMOUNT_ALLOW_PUBLIC_AUTH_OFF",
            "SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED",
            "SURMOUNT_APEX_PUBLIC_ROOT",
            "SURMOUNT_STATIC_VHOSTS",
            "SURMOUNT_STATIC_VHOSTS_FILE",
            "SURMOUNT_EXTRA_MAIL_HOSTNAMES",
            "SURMOUNT_CONSOLE_ACCOUNTS",
            "SURMOUNT_NWC_STORE",
        ] {
            // SAFETY: EnvGuard mutex serializes process env mutation in this module's tests.
            unsafe { std::env::remove_var(k) }
        }
    }

    #[test]
    fn splora_proxy_and_http3_default_off_on_plain_http() {
        let _g = EnvGuard::acquire();
        let cfg = AppConfig::from_env().unwrap();
        assert!(!cfg.splora_proxy.enable);
        assert!(!cfg.http3.enable);
        assert!(!cfg.http3.listener_bound());
    }

    #[test]
    fn apex_public_root_from_env_trim_empty_unset() {
        let _g = EnvGuard::acquire();
        assert_eq!(AppConfig::from_env().unwrap().apex_public_root, None);
        set_env("SURMOUNT_APEX_PUBLIC_ROOT", "   ");
        assert_eq!(AppConfig::from_env().unwrap().apex_public_root, None);
        set_env("SURMOUNT_APEX_PUBLIC_ROOT", "/var/lib/surmount/public-site");
        assert_eq!(
            AppConfig::from_env().unwrap().apex_public_root.as_deref(),
            Some(std::path::Path::new("/var/lib/surmount/public-site"))
        );
    }

    /// Named contract: extra mail Hosts come from a comma env list.
    #[test]
    fn extra_mail_hostnames_from_comma_env() {
        let _g = EnvGuard::acquire();
        assert!(
            AppConfig::from_env()
                .unwrap()
                .extra_mail_hostnames
                .is_empty()
        );
        set_env(
            "SURMOUNT_EXTRA_MAIL_HOSTNAMES",
            " mail.cryptoquick.com,MAIL.cryptoquick.com, mail.other.test ",
        );
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(
            cfg.extra_mail_hostnames,
            vec![
                "mail.cryptoquick.com".to_string(),
                "mail.other.test".to_string()
            ]
        );
    }

    /// Named contract: extra static Hosts come from JSON env; keys lowercased.
    #[test]
    fn static_vhosts_from_json_env_and_union_allowlist() {
        let _g = EnvGuard::acquire();
        assert!(AppConfig::from_env().unwrap().static_vhosts.is_empty());
        set_env(
            "SURMOUNT_STATIC_VHOSTS",
            r#"{"Extra.TEST":"/var/lib/surmount/static-sites/extra","www.extra.test":"/var/lib/surmount/static-sites/extra"}"#,
        );
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(
            cfg.static_vhosts.get("extra.test").map(|p| p.as_path()),
            Some(std::path::Path::new("/var/lib/surmount/static-sites/extra"))
        );
        assert_eq!(
            cfg.static_vhosts.get("www.extra.test").map(|p| p.as_path()),
            Some(std::path::Path::new("/var/lib/surmount/static-sites/extra"))
        );
        assert!(
            cfg.redirect_allowed_hosts.iter().any(|h| h == "extra.test"),
            "default :80 allowlist must include extra static Host: {:?}",
            cfg.redirect_allowed_hosts
        );
        assert!(
            cfg.redirect_allowed_hosts
                .iter()
                .any(|h| h == "www.extra.test"),
            "default :80 allowlist must include www alias: {:?}",
            cfg.redirect_allowed_hosts
        );
        assert!(
            crate::redirect::redirect_http_to_https(
                true,
                "extra.test",
                "/",
                None,
                &cfg.redirect_allowed_hosts,
                &cfg.primary_domain,
                &cfg.services_hostname,
            ) == crate::redirect::HttpToHttps::Redirect {
                location: "https://extra.test/".into()
            }
        );
    }

    #[test]
    fn static_vhosts_invalid_json_fails_closed() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_STATIC_VHOSTS", "[1,2]");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("JSON object"), "{err}");
    }

    #[test]
    fn auth_mode_default_off() {
        let _g = EnvGuard::acquire();
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.auth.mode, AuthMode::Off);
        assert!(cfg.auth.allowlist.is_empty());
    }

    /// Named contract: console map path comes from env or the host default.
    #[test]
    fn console_accounts_path_from_env_or_default() {
        let _g = EnvGuard::acquire();
        assert_eq!(
            AppConfig::from_env()
                .unwrap()
                .console_accounts_path
                .as_os_str(),
            std::ffi::OsStr::new("/var/lib/surmount/console/accounts.json")
        );
        set_env(
            "SURMOUNT_CONSOLE_ACCOUNTS",
            "/tmp/surmount-console-accounts-fixture.json",
        );
        assert_eq!(
            AppConfig::from_env()
                .unwrap()
                .console_accounts_path
                .as_os_str(),
            std::ffi::OsStr::new("/tmp/surmount-console-accounts-fixture.json")
        );
    }

    #[test]
    fn auth_mode_nostr_requires_session_secret() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_AUTH_MODE", "nostr");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("SESSION_SECRET") || err.contains("session"),
            "{err}"
        );
    }

    #[test]
    fn auth_mode_nostr_with_secret_ok() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_AUTH_MODE", "nostr");
        set_env("SURMOUNT_SESSION_SECRET", "dev-only-test-secret");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.auth.mode, AuthMode::Nostr);
        assert!(!cfg.auth.session_secret.as_ref().unwrap().is_empty());
    }

    /// Named contract: public primary listen + authMode=off fails at from_env
    /// (start-time refuse). Loopback default still allows auth-off.
    #[test]
    fn public_listen_auth_off_is_config_error() {
        let _g = EnvGuard::acquire();
        // Default listen is loopback; auth-off remains OK.
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.auth.mode, AuthMode::Off);
        assert!(!cfg.allow_public_auth_off);

        set_env("SURMOUNT_LISTEN", "0.0.0.0:443");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("AUTH_MODE") || err.contains("public") || err.contains("off"),
            "{err}"
        );

        set_env("SURMOUNT_LISTEN", "203.0.113.10:443");
        let err2 = AppConfig::from_env().unwrap_err();
        assert!(
            err2.contains("AUTH_MODE") || err2.contains("public"),
            "{err2}"
        );

        // Lab escape for intentional public+off (tests only).
        set_env("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF", "1");
        let cfg_lab = AppConfig::from_env().unwrap();
        assert!(cfg_lab.allow_public_auth_off);
        assert_eq!(cfg_lab.auth.mode, AuthMode::Off);

        // Public + nostr is OK when secret present.
        // SAFETY: EnvGuard serializes env mutation in this module.
        unsafe { std::env::remove_var("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF") };
        set_env("SURMOUNT_AUTH_MODE", "nostr");
        set_env("SURMOUNT_SESSION_SECRET", "dev-only-test-secret");
        let cfg_nostr = AppConfig::from_env().unwrap();
        assert_eq!(cfg_nostr.auth.mode, AuthMode::Nostr);
        assert!(cfg_nostr.listen.ip().is_unspecified() || !cfg_nostr.listen.ip().is_loopback());
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

        set_env("SURMOUNT_AUTH_MODE", "nostr");
        // Synthetic HMAC material only. Fail-closed product still requires this.
        set_env("SURMOUNT_SESSION_SECRET", "dev-only-test-secret");
        set_env("SURMOUNT_NOSTR_ALLOWLIST_FILE", file.to_str().unwrap());
        let cfg = AppConfig::from_env().unwrap();
        assert!(
            allowlist_contains(&cfg.auth.allowlist, &file_hex),
            "file allowlist should load when env empty"
        );

        set_env("SURMOUNT_NOSTR_ALLOWLIST", &env_hex);
        set_env("SURMOUNT_SESSION_SECRET", "dev-only-test-secret");
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
        assert!(!cfg.acme.enable, "ACME must default off");
        assert!(cfg.rate_limiter().is_some());
        assert_eq!(cfg.ban.enforcement, BanEnforcement::Off);
        assert!(
            cfg.redirect_allowed_hosts
                .iter()
                .any(|h| h == "services.surmount.systems")
        );
        assert!(
            cfg.redirect_allowed_hosts
                .iter()
                .any(|h| h == "surmount.systems"),
            "default allowlist includes apex for public COMING SOON surface"
        );
        assert!(
            cfg.redirect_allowed_hosts
                .iter()
                .any(|h| h == "www.surmount.systems"),
            "default allowlist includes www for public COMING SOON surface"
        );
        assert!(
            cfg.redirect_allowed_hosts
                .iter()
                .any(|h| h == "mta-sts.surmount.systems"),
            "default allowlist includes MTA-STS policy host"
        );
        assert_eq!(cfg.mta_sts_mode, crate::mta_sts::MtaStsMode::Off);
        assert_eq!(cfg.mta_sts_max_age, 86_400);
    }

    #[test]
    fn mta_sts_mode_from_env() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_MTA_STS_MODE", "testing");
        set_env("SURMOUNT_MTA_STS_MAX_AGE", "3600");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(cfg.mta_sts_mode, crate::mta_sts::MtaStsMode::Testing);
        assert_eq!(cfg.mta_sts_max_age, 3600);
    }

    #[test]
    fn acme_enable_incomplete_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_ACME_ENABLE", "1");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("ACME") || err.contains("DOMAINS") || err.contains("domains"),
            "{err}"
        );
    }

    #[test]
    fn acme_enable_under_plain_http_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_ACME_ENABLE", "1");
        set_env("SURMOUNT_ACME_DOMAINS", "services.example.test");
        set_env("SURMOUNT_ACME_EMAIL", "ops@example.test");
        set_env(
            "SURMOUNT_ACME_DIRECTORY",
            "https://acme-staging-v02.api.letsencrypt.org/directory",
        );
        set_env(
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH",
            "/run/surmount-secrets/acme/account.json",
        );
        // Default listen mode is plain HTTP.
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("https") || err.contains("HTTPS") || err.contains("LISTEN_MODE"),
            "{err}"
        );
    }

    #[test]
    fn acme_enable_with_https_absolute_paths_ok() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN_MODE", "https");
        set_env("SURMOUNT_TLS_CERT", "/run/surmount-secrets/tls/cert.pem");
        set_env("SURMOUNT_TLS_KEY", "/run/surmount-secrets/tls/key.pem");
        set_env("SURMOUNT_ACME_ENABLE", "1");
        set_env("SURMOUNT_ACME_DOMAINS", "services.example.test");
        set_env("SURMOUNT_ACME_EMAIL", "ops@example.test");
        set_env(
            "SURMOUNT_ACME_DIRECTORY",
            "https://acme-staging-v02.api.letsencrypt.org/directory",
        );
        set_env(
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH",
            "/run/surmount-secrets/acme/account.json",
        );
        set_env("SURMOUNT_ACME_DNS_PROVIDER", "mock");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.acme.enable);
        assert!(cfg.listen_mode.is_https());
    }

    #[test]
    fn ban_invalid_enforcement_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_BAN_ENFORCEMENT", "sometimes");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("SURMOUNT_BAN_ENFORCEMENT"), "{err}");
    }

    #[test]
    fn ban_enforce_nft_exec_without_bin_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_BAN_ENFORCEMENT", "enforce");
        set_env("SURMOUNT_BAN_BACKEND", "nft");
        set_env("SURMOUNT_BAN_NFT_EXEC", "1");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("NFT_BIN"),
            "{err}"
        );
    }

    #[test]
    fn ban_invalid_nft_exec_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_BAN_NFT_EXEC", "maybe");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("SURMOUNT_BAN_NFT_EXEC"), "{err}");
    }

    #[test]
    fn https_mode_requires_tls_paths() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN_MODE", "https");
        let err = AppConfig::from_env().unwrap_err();
        assert!(err.contains("SURMOUNT_TLS_CERT"), "{err}");
    }

    #[test]
    fn https_mode_loads_tls_paths() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN_MODE", "https");
        set_env("SURMOUNT_TLS_CERT", "/run/surmount-secrets/tls/cert.pem");
        set_env("SURMOUNT_TLS_KEY", "/run/surmount-secrets/tls/key.pem");
        set_env("SURMOUNT_REDIRECT_HTTP_TO_HTTPS", "true");
        set_env("SURMOUNT_HTTP_REDIRECT_LISTEN", "127.0.0.1:8080");
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
        set_env("SURMOUNT_RATE_LIMIT_MAX", "0");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.rate_limiter().is_none());
    }

    #[test]
    fn cleartext_escape_env_defaults_off() {
        let _g = EnvGuard::acquire();
        assert!(!AppConfig::from_env().unwrap().https_allow_cleartext_escape);
        set_env("SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE", "1");
        assert!(AppConfig::from_env().unwrap().https_allow_cleartext_escape);
    }

    #[test]
    fn invalid_http_redirect_listen_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_REDIRECT_HTTP_TO_HTTPS", "true");
        set_env("SURMOUNT_HTTP_REDIRECT_LISTEN", "not-a-socket");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("SURMOUNT_HTTP_REDIRECT_LISTEN") && err.contains("not-a-socket"),
            "{err}"
        );
    }

    #[test]
    fn empty_http_redirect_listen_is_none() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_HTTP_REDIRECT_LISTEN", "   ");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.http_redirect_listen.is_none());
    }

    #[test]
    fn local_cleartext_listen_parses_loopback() {
        let _g = EnvGuard::acquire();
        // Public primary + auth-off is refused; lab escape for this socket-shape test.
        set_env("SURMOUNT_LISTEN", "0.0.0.0:443");
        set_env("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF", "1");
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
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
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "  ");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.local_cleartext_listen.is_none());
    }

    #[test]
    fn local_cleartext_invalid_is_config_error() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "not-a-socket");
        let err = AppConfig::from_env().unwrap_err();
        assert!(
            err.contains("SURMOUNT_LOCAL_CLEARTEXT_LISTEN") && err.contains("not-a-socket"),
            "{err}"
        );
    }

    #[test]
    fn local_cleartext_must_be_loopback() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN", "127.0.0.1:443");
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "0.0.0.0:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("loopback"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_from_primary() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN", "127.0.0.1:8090");
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("differ") && err.contains("LISTEN"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_port_from_wildcard_primary() {
        // 0.0.0.0:8090 + 127.0.0.1:8090 looks like different SocketAddrs but
        // Linux bind collides. Fail closed at validate (not only at bind).
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN", "0.0.0.0:8090");
        set_env("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF", "1");
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("port") && err.contains("8090"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_from_redirect() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN", "0.0.0.0:443");
        set_env("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF", "1");
        set_env("SURMOUNT_HTTP_REDIRECT_LISTEN", "127.0.0.1:8080");
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8080");
        let cfg = AppConfig::from_env().unwrap();
        let err = cfg.validate_local_cleartext().unwrap_err();
        assert!(err.contains("differ") && err.contains("REDIRECT"), "{err}");
    }

    #[test]
    fn local_cleartext_must_differ_port_from_redirect() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_LISTEN", "0.0.0.0:443");
        set_env("SURMOUNT_ALLOW_PUBLIC_AUTH_OFF", "1");
        set_env("SURMOUNT_HTTP_REDIRECT_LISTEN", "0.0.0.0:8090");
        set_env("SURMOUNT_LOCAL_CLEARTEXT_LISTEN", "127.0.0.1:8090");
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
        set_env("SURMOUNT_ONION_HOSTNAME_FILE", file.to_str().unwrap());
        set_env("SURMOUNT_ONION_URL", format!("{env_onion}.onion"));
        let cfg = AppConfig::from_env().unwrap();
        let expected = format!("http://{env_onion}.onion");
        assert_eq!(cfg.onion_url.as_deref(), Some(expected.as_str()));
        assert_eq!(cfg.onion_surface.status_slug(), "configured");
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
        set_env("SURMOUNT_ONION_HOSTNAME_FILE", file.to_str().unwrap());
        let cfg = AppConfig::from_env().unwrap();
        let expected = format!("http://{file_onion}.onion");
        assert_eq!(cfg.onion_url.as_deref(), Some(expected.as_str()));
        assert_eq!(cfg.onion_surface.status_slug(), "configured");
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
        assert_eq!(cfg.onion_surface.status_slug(), "not_provisioned");
        assert_eq!(cfg.onion_surface, OnionSurface::NotProvisioned);
        assert_eq!(cfg.onion_discovery.mappings().count(), 0);
    }

    /// Named contract: configured onion auto-derives apex, www, services, and mta-sts mappings.
    #[test]
    fn onion_discovery_auto_derive_from_onion_url() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_PRIMARY_DOMAIN", "example.test");
        set_env("SURMOUNT_SERVICES_HOSTNAME", "services.example.test");
        set_env("SURMOUNT_ONION_URL", format!("http://{SAMPLE_V3}.onion"));
        let cfg = AppConfig::from_env().unwrap();
        let hosts: Vec<_> = cfg
            .onion_discovery
            .mappings()
            .map(|m| m.clearnet_host.as_str())
            .collect();
        assert!(hosts.contains(&"example.test"));
        assert!(hosts.contains(&"www.example.test"));
        assert!(hosts.contains(&"services.example.test"));
        assert!(hosts.contains(&"mta-sts.example.test"));
        assert!(!hosts.contains(&"mail.example.test"));
        let apex = cfg.onion_discovery.lookup("example.test").unwrap();
        assert_eq!(apex.onion_port, 443);
        assert_eq!(apex.protocols, vec!["h2".to_string()]);
        assert_eq!(apex.onion_scheme, "http");
        assert_eq!(apex.ma_seconds, 86_400);
        assert_eq!(apex.onion_path_prefix, "/_o/example.test");
        let services = cfg.onion_discovery.lookup("services.example.test").unwrap();
        assert!(
            services.onion_path_prefix.is_empty(),
            "services console stays onion root"
        );
    }

    /// Named contract: extra static Hosts auto-map; mail does not.
    #[test]
    fn onion_discovery_auto_maps_extra_static_vhosts() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_PRIMARY_DOMAIN", "example.test");
        set_env("SURMOUNT_SERVICES_HOSTNAME", "services.example.test");
        set_env("SURMOUNT_ONION_URL", format!("http://{SAMPLE_V3}.onion"));
        set_env(
            "SURMOUNT_STATIC_VHOSTS",
            r#"{"extra.test":"/tmp/extra","www.extra.test":"/tmp/extra"}"#,
        );
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.onion_discovery.lookup("extra.test").is_some());
        assert!(cfg.onion_discovery.lookup("www.extra.test").is_some());
        assert!(cfg.onion_discovery.lookup("mta-sts.example.test").is_some());
        assert!(cfg.onion_discovery.lookup("mail.example.test").is_none());
        let extra = cfg.onion_discovery.lookup("extra.test").unwrap();
        assert_eq!(extra.onion_path_prefix, "/_o/extra.test");
    }

    /// Named contract: invalid v3 onion is rejected for discovery (no panic, no map).
    #[test]
    fn onion_discovery_invalid_v3_is_skipped() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_PRIMARY_DOMAIN", "example.test");
        set_env("SURMOUNT_SERVICES_HOSTNAME", "services.example.test");
        set_env("SURMOUNT_ONION_URL", "http://not-a-v3.onion");
        let cfg = AppConfig::from_env().unwrap();
        assert!(cfg.onion_url.is_some());
        assert_eq!(cfg.onion_discovery.mappings().count(), 0);
    }

    /// Named contract: global disable flags load from env.
    #[test]
    fn onion_discovery_global_disable_from_env() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_PRIMARY_DOMAIN", "example.test");
        set_env("SURMOUNT_SERVICES_HOSTNAME", "services.example.test");
        set_env("SURMOUNT_ONION_URL", format!("https://{SAMPLE_V3}.onion"));
        set_env("SURMOUNT_ONION_LOCATION_ENABLED", "0");
        set_env("SURMOUNT_ONION_ALT_SVC_ENABLED", "false");
        let cfg = AppConfig::from_env().unwrap();
        assert!(!cfg.onion_discovery.onion_location_enabled);
        assert!(!cfg.onion_discovery.alt_svc_enabled);
        let apex = cfg.onion_discovery.lookup("example.test").unwrap();
        assert_eq!(apex.onion_scheme, "https");
    }

    /// Named contract: path set but missing material => hostname_missing (not invent).
    #[test]
    fn onion_surface_hostname_missing_when_file_absent() {
        let _g = EnvGuard::acquire();
        let missing = std::env::temp_dir().join(format!(
            "surmount-onion-absent-{}-hostname",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&missing);
        set_env(
            "SURMOUNT_ONION_HOSTNAME_FILE",
            missing.to_str().expect("utf8 temp path"),
        );
        let surface = resolve_onion_surface_from_env();
        assert_eq!(surface.status_slug(), "hostname_missing");
        assert!(surface.url().is_none());
        match surface {
            OnionSurface::HostnameMissing { path, .. } => {
                assert_eq!(path.as_deref(), missing.to_str());
            }
            other => panic!("expected HostnameMissing, got {other:?}"),
        }
    }

    /// Named contract: walk HS state dir finds nested hostname (real Arti layout).
    #[test]
    fn onion_surface_from_hs_state_dir_walk() {
        let _g = EnvGuard::acquire();
        let dir =
            std::env::temp_dir().join(format!("surmount-onion-hs-walk-{}", std::process::id()));
        let nested = dir.join("keystore").join("hss").join("nick");
        let _ = std::fs::create_dir_all(&nested);
        let host_onion = "d".repeat(56);
        std::fs::write(nested.join("hostname"), format!("{host_onion}.onion\n")).unwrap();
        set_env("SURMOUNT_ONION_HS_STATE_DIR", dir.to_str().unwrap());
        let surface = resolve_onion_surface_from_env();
        let expected = format!("http://{host_onion}.onion");
        assert_eq!(surface, OnionSurface::Configured { url: expected });
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Named contract: empty HS dir with path set is hostname_missing, not configured.
    #[test]
    fn onion_surface_hs_state_dir_empty_is_hostname_missing() {
        let _g = EnvGuard::acquire();
        let dir =
            std::env::temp_dir().join(format!("surmount-onion-hs-empty-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        set_env("SURMOUNT_ONION_HS_STATE_DIR", dir.to_str().unwrap());
        let surface = resolve_onion_surface_from_env();
        assert_eq!(surface.status_slug(), "hostname_missing");
        assert!(surface.url().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Named contract: Vaultwarden URL unset by default (no invented vault).
    #[test]
    fn vaultwarden_url_default_unset() {
        let _g = EnvGuard::acquire();
        let cfg = AppConfig::from_env().unwrap();
        assert!(
            cfg.vaultwarden_url.is_none(),
            "must not invent vaultwarden url: {:?}",
            cfg.vaultwarden_url
        );
    }

    /// Named contract: SURMOUNT_VAULTWARDEN_URL normalizes bare host:port.
    #[test]
    fn vaultwarden_url_from_env_normalizes() {
        let _g = EnvGuard::acquire();
        set_env("SURMOUNT_VAULTWARDEN_URL", "127.0.0.1:8222");
        let cfg = AppConfig::from_env().unwrap();
        assert_eq!(
            cfg.vaultwarden_url.as_deref(),
            Some("http://127.0.0.1:8222")
        );
        set_env("SURMOUNT_VAULTWARDEN_URL", "https://vault.example.test/");
        let cfg2 = AppConfig::from_env().unwrap();
        assert_eq!(
            cfg2.vaultwarden_url.as_deref(),
            Some("https://vault.example.test")
        );
    }

    #[test]
    fn normalize_vaultwarden_url_empty() {
        assert_eq!(normalize_vaultwarden_url("  "), None);
        assert_eq!(normalize_vaultwarden_url(""), None);
    }

    /// Named contract: non-http(s) schemes and env breakers are refused.
    #[test]
    fn normalize_vaultwarden_url_rejects_bad_scheme_and_charset() {
        assert_eq!(normalize_vaultwarden_url("javascript:alert(1)"), None);
        assert_eq!(normalize_vaultwarden_url("data:text/html,hi"), None);
        assert_eq!(normalize_vaultwarden_url("file:///etc/passwd"), None);
        assert_eq!(normalize_vaultwarden_url("http://evil\ninjected=1"), None);
        assert_eq!(normalize_vaultwarden_url("http://evil;rm"), None);
        assert_eq!(
            normalize_vaultwarden_url("https://vault.example.test"),
            Some("https://vault.example.test".into())
        );
        assert_eq!(
            normalize_vaultwarden_url("http://127.0.0.1:8222"),
            Some("http://127.0.0.1:8222".into())
        );
    }

    #[test]
    fn empty_onion_file_is_unset() {
        let _g = EnvGuard::acquire();
        let dir = std::env::temp_dir().join(format!("surmount-onion-empty-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("hostname");
        std::fs::write(&file, "   \n\n").unwrap();
        set_env("SURMOUNT_ONION_HOSTNAME_FILE", file.to_str().unwrap());
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
