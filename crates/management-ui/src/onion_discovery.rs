//! Clearnet Host -> onion discovery headers (`Onion-Location` + `Alt-Svc`).
//!
//! Loaded once at process start from env / optional map file plus auto-derive
//! from the process-wide onion surface. There is no SIGHUP or file-watch
//! hot-reload; restart the unit after mapping or hostname-file changes.

use std::collections::BTreeMap;
use std::path::Path;

use axum::http::uri::PathAndQuery;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use serde::{Deserialize, Serialize};

use crate::config::{parse_env_flag_truthy, redact_onion_in_text};
use crate::redirect::host_for_url_authority;

/// Canonical header name for onion discovery (HTTP is case-insensitive).
pub static ONION_LOCATION: HeaderName = HeaderName::from_static("onion-location");
/// Canonical Alt-Svc header name.
pub static ALT_SVC: HeaderName = HeaderName::from_static("alt-svc");

/// Default Alt-Svc max-age (seconds) for auto-derived mappings.
pub const DEFAULT_MA_SECONDS: u64 = 86_400;
/// Default onion port for auto-derived mappings.
pub const DEFAULT_ONION_PORT: u16 = 443;
/// Path prefix that selects a clearnet Host on the shared v3 onion.
///
/// Services console stays at `{onion}/` (empty prefix). Apex, www, extra
/// static Hosts, and `mta-sts.{primary}` use `/_o/{clearnet-host}{path}`.
pub const ONION_VHOST_PATH_PREFIX: &str = "/_o";

/// Marker inserted on the local Arti / loopback cleartext socket so discovery
/// headers are not emitted as if that bind were TLS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CleartextSocket;

/// One clearnet host -> onion discovery mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnionMapping {
    pub clearnet_host: String,
    pub onion_host: String,
    pub onion_port: u16,
    pub protocols: Vec<String>,
    pub onion_scheme: String,
    pub ma_seconds: u64,
    /// Pre-computed `{onion_host}:{onion_port}` for Alt-Svc.
    pub onion_authority: String,
    pub onion_location_enabled: bool,
    pub alt_svc_enabled: bool,
    /// Empty for the services console. Otherwise `/_o/{clearnet_host}`.
    pub onion_path_prefix: String,
}

/// Process-wide onion discovery table (immutable after `AppConfig::from_env`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnionDiscoveryConfig {
    pub onion_location_enabled: bool,
    pub alt_svc_enabled: bool,
    mappings: BTreeMap<String, OnionMapping>,
}

/// Machine-readable dump for `GET /api/v1/system` (admin-gated when auth is on).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OnionDiscoveryDump {
    pub onion_location_enabled: bool,
    pub alt_svc_enabled: bool,
    pub mappings: Vec<OnionMapping>,
}

impl OnionDiscoveryConfig {
    /// No mappings. Global header switches default on (emit still needs a map).
    pub fn empty() -> Self {
        Self {
            onion_location_enabled: true,
            alt_svc_enabled: true,
            mappings: BTreeMap::new(),
        }
    }

    pub fn from_mappings(
        onion_location_enabled: bool,
        alt_svc_enabled: bool,
        mappings: impl IntoIterator<Item = OnionMapping>,
    ) -> Self {
        let mut map = BTreeMap::new();
        for m in mappings {
            map.insert(m.clearnet_host.clone(), m);
        }
        Self {
            onion_location_enabled,
            alt_svc_enabled,
            mappings: map,
        }
    }

    pub fn lookup(&self, host: &str) -> Option<&OnionMapping> {
        let key = normalize_clearnet_host(host);
        if key.is_empty() {
            return None;
        }
        self.mappings.get(&key)
    }

    pub fn mappings(&self) -> impl Iterator<Item = &OnionMapping> {
        self.mappings.values()
    }

    pub fn dump(&self) -> OnionDiscoveryDump {
        OnionDiscoveryDump {
            onion_location_enabled: self.onion_location_enabled,
            alt_svc_enabled: self.alt_svc_enabled,
            mappings: self.mappings.values().cloned().collect(),
        }
    }
}

impl OnionMapping {
    /// Build a mapping after validating v3 onion, scheme, port, and protocols.
    /// Returns `None` for invalid input (never panics).
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        clearnet_host: &str,
        onion_host: &str,
        onion_port: u16,
        protocols: Vec<String>,
        onion_scheme: &str,
        ma_seconds: u64,
        onion_location_enabled: bool,
        alt_svc_enabled: bool,
    ) -> Option<Self> {
        let clearnet_host = normalize_clearnet_host(clearnet_host);
        if clearnet_host.is_empty() || host_is_onion(&clearnet_host) {
            return None;
        }
        let onion_host = parse_v3_onion_host(onion_host)?;
        if onion_port == 0 {
            return None;
        }
        let onion_scheme = parse_onion_scheme(onion_scheme)?;
        let protocols = sanitize_protocols(protocols);
        if protocols.is_empty() {
            return None;
        }
        let onion_authority = format!("{onion_host}:{onion_port}");
        Some(Self {
            clearnet_host,
            onion_host,
            onion_port,
            protocols,
            onion_scheme: onion_scheme.to_string(),
            ma_seconds,
            onion_authority,
            onion_location_enabled,
            alt_svc_enabled,
            onion_path_prefix: String::new(),
        })
    }

    /// Auto-derive defaults: port 443, `["h2"]`, ma 86400, both headers on.
    ///
    /// Sets `/_o/{host}` so purple-pill lands on this Host. Callers that
    /// map the services hostname must clear `onion_path_prefix`.
    pub fn auto_derived(clearnet_host: &str, onion_url: &str) -> Option<Self> {
        let onion_host = parse_v3_onion_host(onion_url)?;
        let scheme = scheme_from_onion_url(onion_url);
        let mut mapping = Self::try_new(
            clearnet_host,
            &onion_host,
            DEFAULT_ONION_PORT,
            vec!["h2".into()],
            scheme,
            DEFAULT_MA_SECONDS,
            true,
            true,
        )?;
        mapping.onion_path_prefix = onion_vhost_path_prefix(&mapping.clearnet_host);
        Some(mapping)
    }
}

/// `/_o/{normalized-clearnet-host}` (no trailing slash).
pub fn onion_vhost_path_prefix(clearnet_host: &str) -> String {
    let host = normalize_clearnet_host(clearnet_host);
    format!("{ONION_VHOST_PATH_PREFIX}/{host}")
}

/// Apply console-vs-vhost prefix: services hostname stays onion root.
pub fn apply_onion_surface_prefix(mapping: &mut OnionMapping, services_hostname: &str) {
    let services = normalize_clearnet_host(services_hostname);
    if !services.is_empty() && mapping.clearnet_host == services {
        mapping.onion_path_prefix.clear();
    } else if mapping.onion_path_prefix.is_empty() {
        mapping.onion_path_prefix = onion_vhost_path_prefix(&mapping.clearnet_host);
    }
}

/// v3 onion host: 56 chars of `a-z` / `2-7` plus `.onion`.
/// Accepts a bare host or `http(s)://` URL. Invalid -> `None`.
pub fn parse_v3_onion_host(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let without_scheme = t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
        .or_else(|| t.strip_prefix("HTTPS://"))
        .or_else(|| t.strip_prefix("HTTP://"))
        .unwrap_or(t);
    let host_only = without_scheme.split('/').next().unwrap_or(without_scheme);
    let host_only = host_only.split('?').next().unwrap_or(host_only);
    let host_only = host_only.trim().trim_end_matches('.');
    // Onion labels do not use a numeric port in the host field.
    if host_only.contains(':') {
        return None;
    }
    let host = host_only.to_ascii_lowercase();
    let label = host.strip_suffix(".onion")?;
    if label.len() != 56 {
        return None;
    }
    if !label
        .bytes()
        .all(|b| matches!(b, b'a'..=b'z' | b'2'..=b'7'))
    {
        return None;
    }
    Some(format!("{label}.onion"))
}

pub fn host_is_onion(host: &str) -> bool {
    host_for_url_authority(host)
        .to_ascii_lowercase()
        .ends_with(".onion")
}

pub fn normalize_clearnet_host(host: &str) -> String {
    host_for_url_authority(host).to_ascii_lowercase()
}

fn parse_onion_scheme(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "http" => Some("http"),
        "https" => Some("https"),
        _ => None,
    }
}

fn scheme_from_onion_url(url: &str) -> &'static str {
    if url.trim().to_ascii_lowercase().starts_with("https://") {
        "https"
    } else {
        "http"
    }
}

fn sanitize_protocols(protocols: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for p in protocols {
        let t = p.trim().to_ascii_lowercase();
        if t.is_empty() || t.len() > 16 {
            continue;
        }
        if !t
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        {
            continue;
        }
        if !out.iter().any(|e| e == &t) {
            out.push(t);
        }
    }
    out
}

/// Public Hosts that auto-map to the process-wide v3 onion.
///
/// Apex, `www.{apex}`, services, `mta-sts.{apex}`, plus extra static Hosts.
/// Never mail. Never onion names. De-duplicated, insertion order.
pub fn auto_clearnet_hosts(
    primary_domain: &str,
    services_hostname: &str,
    extra_static_hosts: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<String> {
    let mut out = Vec::new();
    let apex = normalize_clearnet_host(primary_domain);
    if !apex.is_empty() && !host_is_onion(&apex) {
        out.push(apex.clone());
        let www = format!("www.{apex}");
        if www != apex {
            out.push(www);
        }
    }
    let services = normalize_clearnet_host(services_hostname);
    if !services.is_empty() && !host_is_onion(&services) && !out.iter().any(|h| h == &services) {
        out.push(services);
    }
    if !apex.is_empty() && !host_is_onion(&apex) {
        let mta = format!("mta-sts.{apex}");
        if !out.iter().any(|h| h == &mta) {
            out.push(mta);
        }
    }
    let mail = if apex.is_empty() {
        String::new()
    } else {
        format!("mail.{apex}")
    };
    for raw in extra_static_hosts {
        let host = normalize_clearnet_host(raw.as_ref());
        if host.is_empty() || host_is_onion(&host) {
            continue;
        }
        if !mail.is_empty() && host == mail {
            continue;
        }
        if !out.iter().any(|h| h == &host) {
            out.push(host);
        }
    }
    out
}

/// Build the table from an already-resolved onion URL (tests + `from_env`).
#[allow(clippy::too_many_arguments)]
pub fn build_onion_discovery(
    primary_domain: &str,
    services_hostname: &str,
    onion_url: Option<&str>,
    onion_location_enabled: bool,
    alt_svc_enabled: bool,
    extra: impl IntoIterator<Item = OnionMapping>,
    location_disabled_hosts: &[String],
    alt_svc_disabled_hosts: &[String],
    extra_static_hosts: impl IntoIterator<Item = impl AsRef<str>>,
) -> OnionDiscoveryConfig {
    let extra_static: Vec<String> = extra_static_hosts
        .into_iter()
        .map(|h| normalize_clearnet_host(h.as_ref()))
        .filter(|h| !h.is_empty())
        .collect();
    let mut mappings = BTreeMap::new();
    if let Some(url) = onion_url {
        for host in auto_clearnet_hosts(primary_domain, services_hostname, &extra_static) {
            if let Some(mut m) = OnionMapping::auto_derived(&host, url) {
                apply_onion_surface_prefix(&mut m, services_hostname);
                mappings.insert(m.clearnet_host.clone(), m);
            }
        }
    }
    for mut m in extra {
        apply_onion_surface_prefix(&mut m, services_hostname);
        mappings.insert(m.clearnet_host.clone(), m);
    }
    for host in location_disabled_hosts {
        let key = normalize_clearnet_host(host);
        if let Some(m) = mappings.get_mut(&key) {
            m.onion_location_enabled = false;
        }
    }
    for host in alt_svc_disabled_hosts {
        let key = normalize_clearnet_host(host);
        if let Some(m) = mappings.get_mut(&key) {
            m.alt_svc_enabled = false;
        }
    }
    OnionDiscoveryConfig {
        onion_location_enabled,
        alt_svc_enabled,
        mappings,
    }
}

/// Load once at process start. Never panics. Invalid map entries are skipped.
///
/// No hot-reload: callers must restart after env / map-file / hostname-file
/// changes (same as PEMs).
pub fn onion_discovery_from_env(
    primary_domain: &str,
    services_hostname: &str,
    onion_url: Option<&str>,
    extra_static_hosts: &[String],
) -> OnionDiscoveryConfig {
    let onion_location_enabled = env_flag("SURMOUNT_ONION_LOCATION_ENABLED", true);
    let alt_svc_enabled = env_flag("SURMOUNT_ONION_ALT_SVC_ENABLED", true);
    let extra = load_map_file_extras();
    let location_disabled = env_host_list("SURMOUNT_ONION_LOCATION_DISABLED_HOSTS");
    let alt_disabled = env_host_list("SURMOUNT_ONION_ALT_SVC_DISABLED_HOSTS");
    build_onion_discovery(
        primary_domain,
        services_hostname,
        onion_url,
        onion_location_enabled,
        alt_svc_enabled,
        extra,
        &location_disabled,
        &alt_disabled,
        extra_static_hosts,
    )
}

fn env_flag(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => parse_env_flag_truthy(&v),
        Err(_) => default,
    }
}

fn env_host_list(key: &str) -> Vec<String> {
    match std::env::var(key) {
        Ok(s) => s
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(normalize_clearnet_host)
            .filter(|p| !p.is_empty())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn load_map_file_extras() -> Vec<OnionMapping> {
    let path = match std::env::var("SURMOUNT_ONION_MAP_FILE") {
        Ok(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Vec::new();
            }
            t.to_string()
        }
        Err(_) => return Vec::new(),
    };
    load_map_file(Path::new(&path))
}

#[derive(Debug, Deserialize)]
struct OnionMapFile {
    #[serde(default = "default_true")]
    onion_location_enabled: Option<bool>,
    #[serde(default)]
    alt_svc_enabled: Option<bool>,
    #[serde(default)]
    mappings: Vec<OnionMapFileEntry>,
}

fn default_true() -> Option<bool> {
    Some(true)
}

#[derive(Debug, Deserialize)]
struct OnionMapFileEntry {
    clearnet_host: Option<String>,
    onion_host: Option<String>,
    onion_port: Option<u16>,
    #[serde(default)]
    protocols: Option<Vec<String>>,
    onion_scheme: Option<String>,
    ma_seconds: Option<u64>,
    onion_location_enabled: Option<bool>,
    alt_svc_enabled: Option<bool>,
}

fn load_map_file(path: &Path) -> Vec<OnionMapping> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "SURMOUNT_ONION_MAP_FILE unreadable; skipping extra mappings"
            );
            return Vec::new();
        }
    };
    let parsed: OnionMapFile = match serde_json::from_str(&contents) {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "SURMOUNT_ONION_MAP_FILE is not valid JSON; skipping extra mappings"
            );
            return Vec::new();
        }
    };
    let file_ol = parsed.onion_location_enabled.unwrap_or(true);
    let file_alt = parsed.alt_svc_enabled.unwrap_or(true);
    let mut out = Vec::new();
    for entry in parsed.mappings {
        let Some(host) = entry.clearnet_host.as_deref() else {
            continue;
        };
        let Some(onion) = entry.onion_host.as_deref() else {
            continue;
        };
        let protocols = entry.protocols.unwrap_or_else(|| vec!["h2".into()]);
        let scheme = entry.onion_scheme.as_deref().unwrap_or("http");
        let Some(mapping) = OnionMapping::try_new(
            host,
            onion,
            entry.onion_port.unwrap_or(DEFAULT_ONION_PORT),
            protocols,
            scheme,
            entry.ma_seconds.unwrap_or(DEFAULT_MA_SECONDS),
            entry.onion_location_enabled.unwrap_or(true) && file_ol,
            entry.alt_svc_enabled.unwrap_or(true) && file_alt,
        ) else {
            tracing::warn!(
                clearnet_host = %redact_onion_in_text(host),
                "skipping invalid onion discovery map entry"
            );
            continue;
        };
        out.push(mapping);
    }
    out
}

pub fn onion_location_value(mapping: &OnionMapping, uri: &Uri) -> Option<String> {
    let path = uri.path();
    let path = if path.is_empty() { "/" } else { path };
    let prefix = mapping.onion_path_prefix.trim_end_matches('/');
    let mut loc = format!(
        "{}://{}{}{}",
        mapping.onion_scheme, mapping.onion_host, prefix, path
    );
    if let Some(q) = uri.query().filter(|q| !q.is_empty()) {
        loc.push('?');
        loc.push_str(q);
    }
    if loc.contains('\r') || loc.contains('\n') || loc.contains('\0') {
        return None;
    }
    Some(loc)
}

/// Match `/_o/{mapped-host}` or `/_o/{mapped-host}/{rest}` on an onion request.
/// Longest prefix wins. Unknown hosts return None (path stays unrewritten;
/// Axum 404 unless that path is already a console route).
pub fn match_onion_vhost_rewrite(
    path: &str,
    cfg: &OnionDiscoveryConfig,
) -> Option<(String, String)> {
    let path = path.split('?').next().unwrap_or(path);
    let mut best: Option<(usize, String, String)> = None;
    for mapping in cfg.mappings() {
        let prefix = mapping.onion_path_prefix.trim_end_matches('/');
        if prefix.is_empty() {
            continue;
        }
        let (host, new_path) = if path == prefix {
            (mapping.clearnet_host.clone(), "/".to_string())
        } else if let Some(rest) = path.strip_prefix(prefix).filter(|r| r.starts_with('/')) {
            let new_path = if rest.is_empty() {
                "/".to_string()
            } else {
                rest.to_string()
            };
            (mapping.clearnet_host.clone(), new_path)
        } else {
            continue;
        };
        if best.as_ref().is_none_or(|(len, _, _)| prefix.len() > *len) {
            best = Some((prefix.len(), host, new_path));
        }
    }
    best.map(|(_, host, new_path)| (host, new_path))
}

/// Replace URI path, keep query. `new_path` must start with `/`.
pub fn rewrite_uri_path_keep_query(uri: &Uri, new_path: &str) -> Option<Uri> {
    if !new_path.starts_with('/') || new_path.contains('\r') || new_path.contains('\n') {
        return None;
    }
    let pq = match uri.query().filter(|q| !q.is_empty()) {
        Some(q) => format!("{new_path}?{q}"),
        None => new_path.to_string(),
    };
    let path_and_query: PathAndQuery = pq.parse().ok()?;
    // Always origin-form. Keeping scheme/authority (HTTP/2 `:authority` or
    // absolute-form) makes Axum miss `/.well-known/...` routes after rewrite.
    let mut origin = uri.clone().into_parts();
    origin.scheme = None;
    origin.authority = None;
    origin.path_and_query = Some(path_and_query);
    Uri::from_parts(origin).ok()
}

/// Append an Alt-Svc token (comma-separated) so clearnet h3 and onion h2 coexist.
pub(crate) fn merge_alt_svc_token(headers: &mut HeaderMap, extra: &str) -> bool {
    if extra.is_empty() || extra.contains('\n') || extra.contains('\r') {
        return false;
    }
    let merged = match headers.get(&ALT_SVC).and_then(|v| v.to_str().ok()) {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}, {extra}"),
        _ => extra.to_string(),
    };
    let Ok(hv) = HeaderValue::from_str(&merged) else {
        return false;
    };
    headers.insert(ALT_SVC.clone(), hv);
    true
}

pub fn alt_svc_value(mapping: &OnionMapping) -> Option<String> {
    if mapping.protocols.is_empty() {
        return None;
    }
    let authority = &mapping.onion_authority;
    if authority.contains('"') || authority.contains('\\') || authority.contains('\n') {
        return None;
    }
    let mut parts = Vec::with_capacity(mapping.protocols.len());
    for proto in &mapping.protocols {
        parts.push(format!(
            "{proto}=\"{authority}\"; ma={}; persist=1",
            mapping.ma_seconds
        ));
    }
    Some(parts.join(", "))
}

pub fn discovery_eligible(
    listen_is_https: bool,
    cleartext_socket: bool,
    host: &str,
    status: StatusCode,
) -> bool {
    if !listen_is_https || cleartext_socket {
        return false;
    }
    if host_is_onion(host) {
        return false;
    }
    let code = status.as_u16();
    (200..400).contains(&code)
}

/// Log fields for a successful injection (onions redacted).
pub fn onion_discovery_trace_fields(clearnet_host: &str, onion_host: &str) -> (String, String) {
    (
        redact_onion_in_text(clearnet_host),
        redact_onion_in_text(onion_host),
    )
}

/// Insert Onion-Location and/or Alt-Svc when eligible. Never panics.
/// Returns true when at least one header was written.
pub fn apply_onion_discovery_headers(
    headers: &mut HeaderMap,
    cfg: &OnionDiscoveryConfig,
    listen_is_https: bool,
    cleartext_socket: bool,
    host: &str,
    uri: &Uri,
    status: StatusCode,
) -> bool {
    if !discovery_eligible(listen_is_https, cleartext_socket, host, status) {
        return false;
    }
    let Some(mapping) = cfg.lookup(host) else {
        return false;
    };
    let emit_ol = cfg.onion_location_enabled && mapping.onion_location_enabled;
    let emit_alt = cfg.alt_svc_enabled && mapping.alt_svc_enabled;
    if !emit_ol && !emit_alt {
        return false;
    }

    let mut injected = false;
    if emit_ol
        && let Some(val) = onion_location_value(mapping, uri)
        && let Ok(hv) = HeaderValue::from_str(&val)
    {
        headers.insert(ONION_LOCATION.clone(), hv);
        injected = true;
    }
    if emit_alt && let Some(val) = alt_svc_value(mapping) {
        if merge_alt_svc_token(headers, &val) {
            injected = true;
        }
    }
    if injected {
        let (clearnet, onion) = onion_discovery_trace_fields(host, &mapping.onion_host);
        tracing::debug!(
            clearnet_host = %clearnet,
            onion_host = %onion,
            "onion_discovery_headers"
        );
    }
    injected
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const FIXTURE_ONION: &str =
        "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion";

    fn fixture_url() -> String {
        format!("http://{FIXTURE_ONION}")
    }

    fn mapping() -> OnionMapping {
        OnionMapping::auto_derived("example.test", &fixture_url()).unwrap()
    }

    #[test]
    fn parse_v3_rejects_short_long_and_bad_charset() {
        assert!(parse_v3_onion_host("not-an-onion").is_none());
        assert!(parse_v3_onion_host("abc.onion").is_none());
        let mut bad = "a".repeat(56);
        bad.push_str(".onion");
        // 'a' is fine; use '1' (not in base32 onion alphabet).
        let bad_charset = format!("{}1.onion", "a".repeat(55));
        assert!(parse_v3_onion_host(&bad_charset).is_none());
        assert!(parse_v3_onion_host(&format!("{FIXTURE_ONION}.evil")).is_none());
        assert_eq!(
            parse_v3_onion_host(&format!("https://{FIXTURE_ONION}/path")),
            Some(FIXTURE_ONION.to_string())
        );
    }

    #[test]
    fn try_new_rejects_port_zero_and_empty_protocols() {
        assert!(
            OnionMapping::try_new(
                "example.test",
                FIXTURE_ONION,
                0,
                vec!["h2".into()],
                "http",
                60,
                true,
                true,
            )
            .is_none()
        );
        assert!(
            OnionMapping::try_new(
                "example.test",
                FIXTURE_ONION,
                443,
                vec![],
                "http",
                60,
                true,
                true,
            )
            .is_none()
        );
        assert!(
            OnionMapping::try_new(
                "example.test",
                FIXTURE_ONION,
                443,
                vec!["h2".into()],
                "ftp",
                60,
                true,
                true,
            )
            .is_none()
        );
    }

    #[test]
    fn auto_clearnet_hosts_apex_www_and_services() {
        let hosts = auto_clearnet_hosts("Example.TEST", "services.example.test", &[] as &[&str]);
        assert_eq!(
            hosts,
            vec![
                "example.test".to_string(),
                "www.example.test".to_string(),
                "services.example.test".to_string(),
                "mta-sts.example.test".to_string(),
            ]
        );
    }

    #[test]
    fn auto_clearnet_hosts_includes_extra_static_not_mail() {
        let hosts = auto_clearnet_hosts(
            "example.test",
            "services.example.test",
            ["extra.test", "www.extra.test", "mail.example.test"],
        );
        assert!(hosts.contains(&"extra.test".to_string()));
        assert!(hosts.contains(&"www.extra.test".to_string()));
        assert!(hosts.contains(&"mta-sts.example.test".to_string()));
        assert!(
            !hosts.contains(&"mail.example.test".to_string()),
            "mail is not a public site and must not auto-map: {hosts:?}"
        );
    }

    #[test]
    fn match_onion_vhost_rewrite_longest_prefix() {
        let cfg = build_onion_discovery(
            "example.test",
            "services.example.test",
            Some(&fixture_url()),
            true,
            true,
            Vec::new(),
            &[],
            &[],
            ["extra.test"],
        );
        assert_eq!(
            match_onion_vhost_rewrite("/_o/extra.test/", &cfg),
            Some(("extra.test".into(), "/".into()))
        );
        assert_eq!(
            match_onion_vhost_rewrite("/_o/extra.test/about?x=1", &cfg),
            Some(("extra.test".into(), "/about".into()))
        );
        assert_eq!(
            match_onion_vhost_rewrite("/_o/example.test/", &cfg),
            Some(("example.test".into(), "/".into()))
        );
        assert_eq!(
            match_onion_vhost_rewrite("/_o/mta-sts.example.test/.well-known/mta-sts.txt", &cfg),
            Some((
                "mta-sts.example.test".into(),
                "/.well-known/mta-sts.txt".into()
            ))
        );
        let well_known = rewrite_uri_path_keep_query(
            &"/_o/mta-sts.example.test/.well-known/mta-sts.txt"
                .parse()
                .unwrap(),
            "/.well-known/mta-sts.txt",
        )
        .expect("well-known path rewrite must parse");
        assert_eq!(well_known.path(), "/.well-known/mta-sts.txt");
        let full: Uri = "http://127.0.0.1:8090/_o/mta-sts.example.test/.well-known/mta-sts.txt"
            .parse()
            .unwrap();
        assert_eq!(
            full.path(),
            "/_o/mta-sts.example.test/.well-known/mta-sts.txt",
            "http Uri must keep .well-known as a path segment: {full}"
        );
        let full_rw = rewrite_uri_path_keep_query(&full, "/.well-known/mta-sts.txt")
            .expect("full URI well-known rewrite must parse");
        assert_eq!(full_rw.path(), "/.well-known/mta-sts.txt");
        assert_eq!(match_onion_vhost_rewrite("/", &cfg), None);
        assert_eq!(match_onion_vhost_rewrite("/_o/unknown.test/", &cfg), None);
        let services = cfg.lookup("services.example.test").unwrap();
        assert!(
            services.onion_path_prefix.is_empty(),
            "services console stays onion root"
        );
    }

    #[test]
    fn build_skips_invalid_onion_url_without_panic() {
        let cfg = build_onion_discovery(
            "example.test",
            "services.example.test",
            Some("http://not-valid.onion"),
            true,
            true,
            Vec::new(),
            &[],
            &[],
            &[] as &[&str],
        );
        assert_eq!(cfg.mappings().count(), 0);
    }

    #[test]
    fn onion_location_preserves_path_and_query() {
        let m = mapping();
        let uri: Uri = "/mail/inbox?tab=unread".parse().unwrap();
        assert_eq!(
            onion_location_value(&m, &uri).unwrap(),
            format!("http://{FIXTURE_ONION}/_o/example.test/mail/inbox?tab=unread")
        );
        let root: Uri = "/".parse().unwrap();
        assert_eq!(
            onion_location_value(&m, &root).unwrap(),
            format!("http://{FIXTURE_ONION}/_o/example.test/")
        );
    }

    #[test]
    fn alt_svc_h2_persist_and_extra_protocol() {
        let mut m = mapping();
        assert_eq!(
            alt_svc_value(&m).unwrap(),
            format!("h2=\"{FIXTURE_ONION}:443\"; ma=86400; persist=1")
        );
        m.protocols = vec!["h2".into(), "h3".into()];
        assert_eq!(
            alt_svc_value(&m).unwrap(),
            format!(
                "h2=\"{FIXTURE_ONION}:443\"; ma=86400; persist=1, h3=\"{FIXTURE_ONION}:443\"; ma=86400; persist=1"
            )
        );
    }

    #[test]
    fn apply_https_2xx_emits_both_and_disable_flags() {
        let cfg = OnionDiscoveryConfig::from_mappings(true, true, [mapping()]);
        let uri: Uri = "/health?x=1".parse().unwrap();
        let mut headers = HeaderMap::new();
        assert!(apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            true,
            false,
            "example.test",
            &uri,
            StatusCode::OK,
        ));
        assert_eq!(
            headers.get(&ONION_LOCATION).and_then(|v| v.to_str().ok()),
            Some(format!("http://{FIXTURE_ONION}/_o/example.test/health?x=1").as_str())
        );
        assert_eq!(
            headers.get(&ALT_SVC).and_then(|v| v.to_str().ok()),
            Some(format!("h2=\"{FIXTURE_ONION}:443\"; ma=86400; persist=1").as_str())
        );

        let mut headers = HeaderMap::new();
        assert!(!apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            false,
            false,
            "example.test",
            &uri,
            StatusCode::OK,
        ));
        assert!(headers.get(&ONION_LOCATION).is_none());
        assert!(headers.get(&ALT_SVC).is_none());

        let mut headers = HeaderMap::new();
        assert!(!apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            true,
            true,
            "example.test",
            &uri,
            StatusCode::OK,
        ));

        let mut headers = HeaderMap::new();
        assert!(!apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            true,
            false,
            FIXTURE_ONION,
            &uri,
            StatusCode::OK,
        ));

        let mut headers = HeaderMap::new();
        assert!(!apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            true,
            false,
            "mail.example.test",
            &uri,
            StatusCode::OK,
        ));

        let mut headers = HeaderMap::new();
        assert!(!apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            true,
            false,
            "example.test",
            &uri,
            StatusCode::NOT_FOUND,
        ));
        assert!(!apply_onion_discovery_headers(
            &mut headers,
            &cfg,
            true,
            false,
            "example.test",
            &uri,
            StatusCode::INTERNAL_SERVER_ERROR,
        ));

        let ol_off = OnionDiscoveryConfig::from_mappings(false, true, [mapping()]);
        let mut headers = HeaderMap::new();
        assert!(apply_onion_discovery_headers(
            &mut headers,
            &ol_off,
            true,
            false,
            "example.test",
            &uri,
            StatusCode::FOUND,
        ));
        assert!(headers.get(&ONION_LOCATION).is_none());
        assert!(headers.get(&ALT_SVC).is_some());

        let mut per = mapping();
        per.alt_svc_enabled = false;
        let per_cfg = OnionDiscoveryConfig::from_mappings(true, true, [per]);
        let mut headers = HeaderMap::new();
        assert!(apply_onion_discovery_headers(
            &mut headers,
            &per_cfg,
            true,
            false,
            "example.test",
            &uri,
            StatusCode::OK,
        ));
        assert!(headers.get(&ONION_LOCATION).is_some());
        assert!(headers.get(&ALT_SVC).is_none());
    }

    #[test]
    fn dump_reflects_loaded_map() {
        let cfg = build_onion_discovery(
            "example.test",
            "services.example.test",
            Some(&fixture_url()),
            true,
            true,
            Vec::new(),
            &[],
            &[],
            &[] as &[&str],
        );
        let dump = cfg.dump();
        assert!(dump.onion_location_enabled);
        assert!(dump.alt_svc_enabled);
        let hosts: Vec<_> = dump
            .mappings
            .iter()
            .map(|m| m.clearnet_host.as_str())
            .collect();
        assert!(hosts.contains(&"example.test"));
        assert!(hosts.contains(&"www.example.test"));
        assert!(hosts.contains(&"services.example.test"));
        assert!(hosts.contains(&"mta-sts.example.test"));
        assert!(!hosts.contains(&"mail.example.test"));
    }

    #[test]
    fn onion_discovery_trace_fields_redact_onion() {
        let (c, o) = onion_discovery_trace_fields("example.test", FIXTURE_ONION);
        assert_eq!(c, "example.test");
        assert_eq!(o, "<onion-redacted>");
        assert!(!o.contains(FIXTURE_ONION));
    }

    #[test]
    fn malformed_map_file_never_panics() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "surmount-onion-map-bad-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, "{not json").unwrap();
        let extras = load_map_file(&path);
        assert!(extras.is_empty());
        std::fs::write(
            &path,
            r#"{"mappings":[{"clearnet_host":"x.test","onion_host":"nope"}]}"#,
        )
        .unwrap();
        let extras = load_map_file(&path);
        assert!(extras.is_empty());
        let _ = std::fs::remove_file(&path);
    }
}
