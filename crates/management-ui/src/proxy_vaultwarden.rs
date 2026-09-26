//! Vaultwarden reverse proxy under a public path prefix on the Axum edge.
//!
//! Day-one product path (operator plan Phase B):
//! - Public prefix default `/vault` (trailing slash normalized)
//! - Upstream default `http://127.0.0.1:8222` (Rocket loopback)
//! - VW login is SoT on this path (no Nostr gate; auth middleware skips prefix)
//! - WebSocket Upgrade headers forwarded; upgrade tunnel when upstream 101
//! - Never log Authorization, Cookie, ADMIN_TOKEN, or request/response bodies
//!
//! Not nginx. Not a new subdomain. Not iframe embedding.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header};
use axum::response::{IntoResponse, Redirect, Response};
use tracing::{debug, warn};

use crate::AppState;

/// Default public path prefix (no trailing slash).
pub const DEFAULT_PUBLIC_PREFIX: &str = "/vault";

/// Default loopback Rocket base (no trailing slash).
pub const DEFAULT_UPSTREAM_BASE: &str = "http://127.0.0.1:8222";

/// Attachment-friendly body cap (bytes) for non-upgrade proxy requests.
pub const DEFAULT_BODY_LIMIT_BYTES: usize = 32 * 1024 * 1024;

/// Runtime knobs for the Vaultwarden path proxy (env / Nix).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultwardenProxyConfig {
    pub enable: bool,
    /// Normalized public prefix without trailing slash (e.g. `/vault`).
    pub public_prefix: String,
    /// Upstream base without trailing slash (e.g. `http://127.0.0.1:8222`).
    pub upstream_base: String,
    /// Max request body bytes for non-upgrade forwards.
    pub body_limit_bytes: usize,
}

impl Default for VaultwardenProxyConfig {
    fn default() -> Self {
        Self {
            enable: false,
            public_prefix: DEFAULT_PUBLIC_PREFIX.to_string(),
            upstream_base: DEFAULT_UPSTREAM_BASE.to_string(),
            body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
        }
    }
}

impl VaultwardenProxyConfig {
    /// Parse from env map (tests) or process env.
    pub fn from_env_map(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let enable = match get("SURMOUNT_VAULTWARDEN_PROXY") {
            Some(v) => env_truthy(&v),
            None => false,
        };
        let public_prefix = match get("SURMOUNT_VAULTWARDEN_PROXY_PREFIX") {
            Some(v) if !v.trim().is_empty() => normalize_public_prefix(&v)?,
            _ => DEFAULT_PUBLIC_PREFIX.to_string(),
        };
        let upstream_base = match get("SURMOUNT_VAULTWARDEN_PROXY_UPSTREAM") {
            Some(v) if !v.trim().is_empty() => normalize_upstream_base(&v)?,
            _ => DEFAULT_UPSTREAM_BASE.to_string(),
        };
        let body_limit_bytes = match get("SURMOUNT_VAULTWARDEN_PROXY_BODY_LIMIT") {
            Some(v) if !v.trim().is_empty() => {
                let n: usize = v
                    .trim()
                    .parse()
                    .map_err(|e| format!("SURMOUNT_VAULTWARDEN_PROXY_BODY_LIMIT invalid: {e}"))?;
                if n < 1024 {
                    return Err(
                        "SURMOUNT_VAULTWARDEN_PROXY_BODY_LIMIT must be >= 1024 bytes".into(),
                    );
                }
                n
            }
            _ => DEFAULT_BODY_LIMIT_BYTES,
        };
        Ok(Self {
            enable,
            public_prefix,
            upstream_base,
            body_limit_bytes,
        })
    }

    pub fn from_env() -> Result<Self, String> {
        Self::from_env_map(|k| std::env::var(k).ok())
    }

    /// True when `path` is exactly the prefix or under it (`/vault`, `/vault/...`).
    pub fn matches_path(&self, path: &str) -> bool {
        path_under_prefix(path, &self.public_prefix)
    }

    /// Same-origin console href for Open vault when proxy is on.
    pub fn same_origin_href(&self) -> String {
        format!("{}/", self.public_prefix.trim_end_matches('/'))
    }
}

fn env_truthy(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Normalize public prefix: leading slash, no trailing slash, no empty path.
pub fn normalize_public_prefix(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("Vaultwarden proxy public prefix must not be empty".into());
    }
    if t.contains("://") || t.contains('?') || t.contains('#') || t.contains(' ') {
        return Err(format!(
            "Vaultwarden proxy public prefix must be a path (got {t:?})"
        ));
    }
    let with_slash = if t.starts_with('/') {
        t.to_string()
    } else {
        format!("/{t}")
    };
    let trimmed = with_slash.trim_end_matches('/').to_string();
    if trimmed.is_empty() || trimmed == "/" {
        return Err("Vaultwarden proxy public prefix must not be '/'".into());
    }
    if trimmed
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.')))
    {
        return Err(format!(
            "Vaultwarden proxy public prefix has unsafe characters: {trimmed:?}"
        ));
    }
    Ok(trimmed)
}

/// Normalize upstream base URL: http(s) only, no trailing slash.
pub fn normalize_upstream_base(raw: &str) -> Result<String, String> {
    let t = raw.trim().trim_end_matches('/').trim();
    if t.is_empty() {
        return Err("Vaultwarden proxy upstream must not be empty".into());
    }
    if !(t.starts_with("http://") || t.starts_with("https://")) {
        return Err(format!(
            "Vaultwarden proxy upstream must be http(s):// (got {t:?})"
        ));
    }
    if t.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Vaultwarden proxy upstream must not contain whitespace/control".into());
    }
    Ok(t.to_string())
}

/// Path is exactly `prefix` or `prefix/...`.
pub fn path_under_prefix(path: &str, prefix: &str) -> bool {
    let p = prefix.trim_end_matches('/');
    path == p || path.starts_with(&format!("{p}/"))
}

/// Strip public prefix for upstream path.
///
/// `/vault` or `/vault/` -> `/`
/// `/vault/api/foo` -> `/api/foo`
/// Returns None when path is outside the prefix.
pub fn strip_public_prefix(path: &str, prefix: &str) -> Option<String> {
    let p = prefix.trim_end_matches('/');
    if path == p {
        return Some("/".to_string());
    }
    let with_slash = format!("{p}/");
    if path == with_slash.as_str() {
        return Some("/".to_string());
    }
    if let Some(rest) = path.strip_prefix(&with_slash) {
        if rest.is_empty() {
            return Some("/".to_string());
        }
        return Some(format!("/{rest}"));
    }
    None
}

/// Build upstream URI from base + stripped path + optional query.
pub fn build_upstream_uri(
    upstream_base: &str,
    stripped_path: &str,
    query: Option<&str>,
) -> Result<String, String> {
    let base = upstream_base.trim_end_matches('/');
    let path = if stripped_path.is_empty() {
        "/"
    } else if stripped_path.starts_with('/') {
        stripped_path
    } else {
        return Err(format!(
            "stripped path must start with / (got {stripped_path:?})"
        ));
    };
    let mut url = format!("{base}{path}");
    if let Some(q) = query
        && !q.is_empty()
    {
        url.push('?');
        url.push_str(q);
    }
    Ok(url)
}

/// True when the client requests a WebSocket upgrade.
pub fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    let upgrade = headers
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);
    if !upgrade {
        return false;
    }
    headers
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .map(|s| {
            s.split(',')
                .any(|p| p.trim().eq_ignore_ascii_case("upgrade"))
        })
        .unwrap_or(false)
}

/// Headers always preserved for WebSocket handshakes (plus non-hop-by-hop).
fn is_websocket_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "upgrade"
            | "connection"
            | "sec-websocket-key"
            | "sec-websocket-version"
            | "sec-websocket-protocol"
            | "sec-websocket-extensions"
            | "sec-websocket-accept"
    )
}

fn is_hop_by_hop_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// Copy request headers for upstream, dropping hop-by-hop.
/// When `preserve_upgrade` is true, keep Upgrade / Connection / Sec-WebSocket-*.
pub fn filter_request_headers(src: &HeaderMap, preserve_upgrade: bool) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in src.iter() {
        let lname = name.as_str();
        if lname.eq_ignore_ascii_case("host") || lname.eq_ignore_ascii_case("content-length") {
            continue;
        }
        if preserve_upgrade && is_websocket_header(lname) {
            out.append(name.clone(), value.clone());
            continue;
        }
        if is_hop_by_hop_name(lname) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}

/// Copy response headers for client, dropping hop-by-hop (except upgrade on 101).
pub fn filter_response_headers(src: &HeaderMap, preserve_upgrade: bool) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in src.iter() {
        let lname = name.as_str();
        if lname.eq_ignore_ascii_case("transfer-encoding")
            || lname.eq_ignore_ascii_case("content-length")
        {
            continue;
        }
        if preserve_upgrade && is_websocket_header(lname) {
            out.append(name.clone(), value.clone());
            continue;
        }
        if is_hop_by_hop_name(lname) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}

/// Redact secret-shaped substrings for any log/error text leaving this module.
pub fn redact_secrets_in_text(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    for needle in ["admin_token=", "authorization:", "cookie:", "bearer "] {
        if let Some(idx) = lower.find(needle) {
            let end = (idx + needle.len()).min(line.len());
            let prefix = &line[..end];
            return format!("{prefix}<redacted>");
        }
    }
    line.to_string()
}

/// Axum catch-all handler for the Vaultwarden public prefix.
pub async fn proxy_handler(State(state): State<Arc<AppState>>, req: Request) -> Response {
    let cfg = &state.config.vaultwarden_proxy;
    if !cfg.enable {
        return StatusCode::NOT_FOUND.into_response();
    }

    let path = req.uri().path().to_string();
    let prefix = cfg.public_prefix.as_str();

    // Normalize bare prefix to trailing slash (same-origin location for VW).
    if path == prefix {
        let loc = format!("{prefix}/");
        return Redirect::permanent(&loc).into_response();
    }

    let Some(stripped) = strip_public_prefix(&path, prefix) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let query = req.uri().query().map(str::to_string);
    let upstream = match build_upstream_uri(&cfg.upstream_base, &stripped, query.as_deref()) {
        Ok(u) => u,
        Err(e) => {
            warn!(error = %e, "vaultwarden_proxy_bad_upstream_uri");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let method = req.method().clone();
    let headers = req.headers().clone();
    let wants_ws = is_websocket_upgrade(&headers);

    if wants_ws {
        return proxy_websocket(state, req, upstream).await;
    }

    proxy_http(state, method, headers, upstream, req).await
}

async fn proxy_http(
    state: Arc<AppState>,
    method: Method,
    headers: HeaderMap,
    upstream: String,
    req: Request,
) -> Response {
    let limit = state.config.vaultwarden_proxy.body_limit_bytes;
    let body = match axum::body::to_bytes(req.into_body(), limit).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body too large for vault proxy",
            )
                .into_response();
        }
    };

    let filtered = filter_request_headers(&headers, false);
    let mut builder = state.http.request(method, &upstream);
    for (name, value) in filtered.iter() {
        builder = builder.header(name, value);
    }

    let sent = builder.body(body.to_vec()).send().await;
    let resp = match sent {
        Ok(r) => r,
        Err(e) => {
            let safe = redact_secrets_in_text(&format!("upstream_error: {e}"));
            warn!(error = %safe, "vaultwarden_proxy_upstream_unreachable");
            return (StatusCode::BAD_GATEWAY, "Vaultwarden upstream unreachable").into_response();
        }
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = resp.headers().clone();
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            let safe = redact_secrets_in_text(&format!("upstream_body: {e}"));
            warn!(error = %safe, "vaultwarden_proxy_upstream_body");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let mut out = Response::builder().status(status);
    let filtered_out = filter_response_headers(&resp_headers, false);
    if let Some(hmap) = out.headers_mut() {
        for (name, value) in filtered_out.iter() {
            hmap.append(name, value.clone());
        }
    }
    out.body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

/// WebSocket upgrade: forward handshake headers; on 101, bi-directional copy.
async fn proxy_websocket(state: Arc<AppState>, req: Request, upstream: String) -> Response {
    let uri: Uri = match upstream.parse() {
        Ok(u) => u,
        Err(e) => {
            warn!(error = %e, "vaultwarden_proxy_ws_bad_uri");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let host = uri.host().unwrap_or("127.0.0.1");
    let port = uri
        .port_u16()
        .unwrap_or(if uri.scheme_str() == Some("https") {
            443
        } else {
            80
        });
    let path_and_query = uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());

    let addr = format!("{host}:{port}");
    let tcp = match tokio::time::timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            let safe = redact_secrets_in_text(&format!("ws_connect: {e}"));
            warn!(error = %safe, "vaultwarden_proxy_ws_connect");
            return (StatusCode::BAD_GATEWAY, "Vaultwarden upstream unreachable").into_response();
        }
        Err(_) => {
            warn!("vaultwarden_proxy_ws_connect_timeout");
            return (StatusCode::BAD_GATEWAY, "Vaultwarden upstream unreachable").into_response();
        }
    };

    let method = req.method().clone();
    let headers = req.headers().clone();
    let filtered = filter_request_headers(&headers, true);

    let mut req_buf = format!("{method} {path_and_query} HTTP/1.1\r\nHost: {host}:{port}\r\n");
    for (name, value) in filtered.iter() {
        if let Ok(v) = value.to_str() {
            req_buf.push_str(name.as_str());
            req_buf.push_str(": ");
            req_buf.push_str(v);
            req_buf.push_str("\r\n");
        }
    }
    if !filtered.contains_key(header::CONNECTION) {
        req_buf.push_str("Connection: Upgrade\r\n");
    }
    if !filtered.contains_key(header::UPGRADE) {
        req_buf.push_str("Upgrade: websocket\r\n");
    }
    req_buf.push_str("\r\n");

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut upstream_stream = tcp;
    if let Err(e) = upstream_stream.write_all(req_buf.as_bytes()).await {
        let safe = redact_secrets_in_text(&format!("ws_write: {e}"));
        warn!(error = %safe, "vaultwarden_proxy_ws_write");
        return StatusCode::BAD_GATEWAY.into_response();
    }

    let mut header_bytes = Vec::with_capacity(1024);
    let mut tmp = [0u8; 256];
    let header_end = loop {
        if header_bytes.len() > 64 * 1024 {
            warn!("vaultwarden_proxy_ws_header_too_large");
            return StatusCode::BAD_GATEWAY.into_response();
        }
        let n = match upstream_stream.read(&mut tmp).await {
            Ok(0) => {
                warn!("vaultwarden_proxy_ws_upstream_eof");
                return StatusCode::BAD_GATEWAY.into_response();
            }
            Ok(n) => n,
            Err(e) => {
                let safe = redact_secrets_in_text(&format!("ws_read: {e}"));
                warn!(error = %safe, "vaultwarden_proxy_ws_read");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        };
        header_bytes.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_header_end(&header_bytes) {
            break pos;
        }
    };

    let header_blob = &header_bytes[..header_end];
    let rest = header_bytes[header_end..].to_vec();
    let header_str = String::from_utf8_lossy(header_blob);
    let mut lines = header_str.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status_code = parse_status_code(status_line).unwrap_or(502);
    let mut resp_headers = HeaderMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((n, v)) = line.split_once(':')
            && let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(n.trim().as_bytes()),
                HeaderValue::from_str(v.trim()),
            )
        {
            resp_headers.append(name, val);
        }
    }

    if status_code != 101 {
        let mut body = rest;
        let mut buf = [0u8; 8192];
        if let Ok(Ok(n)) =
            tokio::time::timeout(Duration::from_millis(200), upstream_stream.read(&mut buf)).await
            && n > 0
        {
            body.extend_from_slice(&buf[..n]);
        }
        let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_GATEWAY);
        let filtered_out = filter_response_headers(&resp_headers, false);
        let mut out = Response::builder().status(status);
        if let Some(hmap) = out.headers_mut() {
            for (name, value) in filtered_out.iter() {
                hmap.append(name, value.clone());
            }
        }
        return out
            .body(Body::from(body))
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
    }

    let filtered_out = filter_response_headers(&resp_headers, true);
    let upgrade = match hyper::upgrade::on(req).await {
        Ok(upgraded) => upgraded,
        Err(e) => {
            let safe = redact_secrets_in_text(&format!("client_upgrade: {e}"));
            warn!(error = %safe, "vaultwarden_proxy_client_upgrade");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    tokio::spawn(async move {
        if let Err(e) = websocket_tunnel(upgrade, upstream_stream, rest).await {
            debug!(
                error = %redact_secrets_in_text(&e.to_string()),
                "vaultwarden_proxy_ws_tunnel_end"
            );
        }
        let _ = &state;
    });

    let mut out = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
    if let Some(hmap) = out.headers_mut() {
        for (name, value) in filtered_out.iter() {
            hmap.append(name, value.clone());
        }
        if !hmap.contains_key(header::UPGRADE) {
            hmap.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        }
        if !hmap.contains_key(header::CONNECTION) {
            hmap.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
        }
    }
    out.body(Body::empty())
        .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn parse_status_code(status_line: &str) -> Option<u16> {
    let mut parts = status_line.split_whitespace();
    let _http = parts.next()?;
    parts.next()?.parse().ok()
}

async fn websocket_tunnel(
    client: hyper::upgrade::Upgraded,
    mut upstream: tokio::net::TcpStream,
    pending_from_upstream: Vec<u8>,
) -> Result<(), std::io::Error> {
    use tokio::io::AsyncWriteExt;

    let mut client = hyper_util::rt::TokioIo::new(client);

    if !pending_from_upstream.is_empty() {
        client.write_all(&pending_from_upstream).await?;
    }

    let (mut client_r, mut client_w) = tokio::io::split(client);
    let (mut up_r, mut up_w) = upstream.split();

    let c2u = async {
        tokio::io::copy(&mut client_r, &mut up_w).await?;
        up_w.shutdown().await
    };
    let u2c = async {
        tokio::io::copy(&mut up_r, &mut client_w).await?;
        client_w.shutdown().await
    };

    tokio::select! {
        r = c2u => { r?; }
        r = u2c => { r?; }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::routing::{any, get};
    use std::net::SocketAddr;
    use std::sync::Mutex;

    #[test]
    fn normalize_public_prefix_defaults_and_rejects() {
        assert_eq!(normalize_public_prefix("/vault").unwrap(), "/vault");
        assert_eq!(normalize_public_prefix("/vault/").unwrap(), "/vault");
        assert_eq!(normalize_public_prefix("vault").unwrap(), "/vault");
        assert!(normalize_public_prefix("/").is_err());
        assert!(normalize_public_prefix("").is_err());
        assert!(normalize_public_prefix("http://evil").is_err());
    }

    #[test]
    fn normalize_upstream_base_http_only() {
        assert_eq!(
            normalize_upstream_base("http://127.0.0.1:8222/").unwrap(),
            "http://127.0.0.1:8222"
        );
        assert!(normalize_upstream_base("ftp://x").is_err());
        assert!(normalize_upstream_base("").is_err());
    }

    #[test]
    fn strip_public_prefix_rewrites() {
        assert_eq!(
            strip_public_prefix("/vault", "/vault").as_deref(),
            Some("/")
        );
        assert_eq!(
            strip_public_prefix("/vault/", "/vault").as_deref(),
            Some("/")
        );
        assert_eq!(
            strip_public_prefix("/vault/api/identity", "/vault").as_deref(),
            Some("/api/identity")
        );
        assert_eq!(strip_public_prefix("/other", "/vault"), None);
        assert_eq!(strip_public_prefix("/vaultwarden", "/vault"), None);
    }

    #[test]
    fn build_upstream_uri_joins_query() {
        let u = build_upstream_uri("http://127.0.0.1:8222", "/api/foo", Some("a=1")).unwrap();
        assert_eq!(u, "http://127.0.0.1:8222/api/foo?a=1");
    }

    #[test]
    fn is_websocket_upgrade_contract() {
        let mut h = HeaderMap::new();
        assert!(!is_websocket_upgrade(&h));
        h.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        h.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
        assert!(is_websocket_upgrade(&h));
    }

    #[test]
    fn filter_request_headers_preserves_upgrade_when_asked() {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, HeaderValue::from_static("services.example"));
        h.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
        h.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
        h.insert(
            HeaderName::from_static("sec-websocket-key"),
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer secret-token-xyz"),
        );
        let kept = filter_request_headers(&h, true);
        assert!(kept.get(header::UPGRADE).is_some());
        assert!(kept.get(header::CONNECTION).is_some());
        assert!(kept.get("sec-websocket-key").is_some());
        assert!(kept.get(header::AUTHORIZATION).is_some());
        assert!(kept.get(header::HOST).is_none());
        let dropped = filter_request_headers(&h, false);
        assert!(dropped.get(header::UPGRADE).is_none());
    }

    #[test]
    fn redact_secrets_never_echoes_admin_token() {
        let line = "failed ADMIN_TOKEN=super-secret-value path=/vault";
        let red = redact_secrets_in_text(line);
        assert!(!red.contains("super-secret-value"), "{red}");
        assert!(red.contains("<redacted>"), "{red}");
    }

    #[test]
    fn proxy_config_from_env_map() {
        let cfg = VaultwardenProxyConfig::from_env_map(|k| match k {
            "SURMOUNT_VAULTWARDEN_PROXY" => Some("1".into()),
            "SURMOUNT_VAULTWARDEN_PROXY_PREFIX" => Some("/vault/".into()),
            "SURMOUNT_VAULTWARDEN_PROXY_UPSTREAM" => Some("http://127.0.0.1:8222/".into()),
            _ => None,
        })
        .unwrap();
        assert!(cfg.enable);
        assert_eq!(cfg.public_prefix, "/vault");
        assert_eq!(cfg.upstream_base, "http://127.0.0.1:8222");
        assert_eq!(cfg.same_origin_href(), "/vault/");
    }

    #[test]
    fn proxy_disabled_by_default() {
        let cfg = VaultwardenProxyConfig::from_env_map(|_| None).unwrap();
        assert!(!cfg.enable);
    }

    /// Named contract: GET under /vault forwards with path rewrite to mock upstream.
    #[tokio::test]
    async fn vault_proxy_get_forwards_and_rewrites_path() {
        let hits = Arc::new(Mutex::new(Vec::<String>::new()));
        let hits_c = hits.clone();
        let mock = Router::new().route(
            "/api/config",
            get(move || {
                let hits_c = hits_c.clone();
                async move {
                    hits_c.lock().unwrap().push("/api/config".into());
                    "vw-ok"
                }
            }),
        );
        let mock_lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = mock_lis.local_addr().unwrap();
        let mock_serve = tokio::spawn(async move {
            axum::serve(mock_lis, mock).await.ok();
        });

        let state = test_proxy_state(true, &format!("http://{mock_addr}"));
        let app = Router::new()
            .route("/vault", any(proxy_handler))
            .route("/vault/", any(proxy_handler))
            .route("/vault/{*rest}", any(proxy_handler))
            .with_state(state);

        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lis.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(lis, app).await.ok();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/vault/api/config"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        assert_eq!(resp.text().await.unwrap(), "vw-ok");
        assert_eq!(
            hits.lock().unwrap().as_slice(),
            &["/api/config".to_string()]
        );

        serve.abort();
        mock_serve.abort();
    }

    /// Named contract: Upgrade + Connection + Sec-WebSocket-* reach upstream.
    #[tokio::test]
    async fn vault_proxy_upgrade_headers_passthrough_contract() {
        let seen = Arc::new(Mutex::new(HeaderMap::new()));
        let seen_c = seen.clone();
        let mock = Router::new().route(
            "/notifications/hub",
            any(move |req: Request| {
                let seen_c = seen_c.clone();
                async move {
                    *seen_c.lock().unwrap() = req.headers().clone();
                    (
                        StatusCode::BAD_REQUEST,
                        "not a real ws server; headers captured",
                    )
                }
            }),
        );
        let mock_lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = mock_lis.local_addr().unwrap();
        let mock_serve = tokio::spawn(async move {
            axum::serve(mock_lis, mock).await.ok();
        });

        let state = test_proxy_state(true, &format!("http://{mock_addr}"));
        let app = Router::new()
            .route("/vault", any(proxy_handler))
            .route("/vault/", any(proxy_handler))
            .route("/vault/{*rest}", any(proxy_handler))
            .with_state(state);

        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lis.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(lis, app).await.ok();
        });

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let req = format!(
            "GET /vault/notifications/hub HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             \r\n"
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = vec![0u8; 4096];
        let _ = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        let h = seen.lock().unwrap().clone();
        assert!(
            h.get(header::UPGRADE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.eq_ignore_ascii_case("websocket"))
                .unwrap_or(false),
            "upstream must see Upgrade: websocket; headers={h:?}"
        );
        assert!(
            h.get("sec-websocket-key").is_some(),
            "upstream must see Sec-WebSocket-Key; headers={h:?}"
        );

        serve.abort();
        mock_serve.abort();
    }

    /// Named contract: upstream down => 502 (not 500 dump).
    #[tokio::test]
    async fn vault_proxy_502_when_upstream_down() {
        let dead = {
            let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let a = lis.local_addr().unwrap();
            drop(lis);
            a
        };
        tokio::time::sleep(Duration::from_millis(20)).await;

        let state = test_proxy_state(true, &format!("http://{dead}"));
        let app = Router::new()
            .route("/vault", any(proxy_handler))
            .route("/vault/", any(proxy_handler))
            .route("/vault/{*rest}", any(proxy_handler))
            .with_state(state);

        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lis.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(lis, app).await.ok();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/vault/"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::BAD_GATEWAY);
        let body = resp.text().await.unwrap();
        assert!(
            !body.contains("ADMIN_TOKEN") && !body.to_ascii_lowercase().contains("password"),
            "502 body must not leak secrets: {body}"
        );

        serve.abort();
    }

    /// Named contract: proxy disabled => 404 on /vault paths.
    #[tokio::test]
    async fn vault_proxy_disabled_is_404() {
        let state = test_proxy_state(false, "http://127.0.0.1:1");
        let app = Router::new()
            .route("/vault", any(proxy_handler))
            .route("/vault/", any(proxy_handler))
            .route("/vault/{*rest}", any(proxy_handler))
            .with_state(state);
        let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = lis.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(lis, app).await.ok();
        });
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/vault/"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);
        serve.abort();
    }

    fn test_proxy_state(enable: bool, upstream: &str) -> Arc<AppState> {
        use crate::config::{AppConfig, OnionSurface};
        use crate::tls::ListenMode;
        use surmount_management_ui::auth::AuthConfig;
        use surmount_management_ui::ban::{BanConfig, BanEnforcement, BanGuard, MemoryBanBackend};

        let config = AppConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 8090)),
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
            vaultwarden_url: Some("https://services.example.test/vault".into()),
            apex_public_root: None,
            static_vhosts: Default::default(),
            extra_mail_hostnames: Vec::new(),
            vaultwarden_proxy: VaultwardenProxyConfig {
                enable,
                public_prefix: "/vault".into(),
                upstream_base: upstream.trim_end_matches('/').to_string(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
            },
            splora_proxy: crate::proxy_vaultwarden::splora::SploraProxyConfig::default(),
            http3: crate::tls::http3::Http3Config::default(),
            rate_limit_max_requests: 0,
            rate_limit_window: Duration::from_secs(60),
            rate_limit_max_keys: 1000,
            ban: BanConfig {
                enforcement: BanEnforcement::Off,
                backend_kind: surmount_management_ui::ban::BanBackendKind::Memory,
                whitelist: vec![],
                state_path: None,
                nft_exec_enabled: false,
                nft_bin: None,
                nft_helper_sock: None,
                nft_helper_bin: None,
            },
            auth: AuthConfig::off(),
            allow_directory_unauthenticated: false,
            allow_public_auth_off: false,
            console_accounts_path: crate::config::unused_console_accounts_path(),
            nwc_store_path: crate::config::unused_nwc_store_path(),
        };
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        Arc::new(AppState {
            http,
            rate_limiter: None,
            ban: BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(MemoryBanBackend::new(vec![])),
            ),
            directory: crate::directory::directory_unavailable(),
            config,
        })
    }
}

/// Splora Unix-socket proxy (nested so the flake git tree sees this module).
///
/// One public Host. Network paths on that Host map to
/// `/run/splora/<instance>.http.sock`. The queue is a separate socket and
/// path. NIP-98 stays in splora. This edge does not add API keys. Not nginx.
/// Not the Electrum newline socket (fail-closed if that socket is configured).
/// Not an explorer: no block, transaction, or address pages.
pub(crate) mod splora {

    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use axum::body::Body;
    use axum::extract::{Request, State};
    use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, header};
    use axum::middleware::Next;
    use axum::response::{IntoResponse, Response};
    use bytes::Bytes;
    use http_body_util::{BodyExt, Full};
    use hyper::body::Incoming;
    use hyper_util::rt::TokioIo;
    use serde::Deserialize;
    use tokio::net::UnixStream;
    use tracing::{debug, warn};

    use crate::AppState;
    use crate::proxy_vaultwarden::{
        filter_request_headers, filter_response_headers, is_websocket_upgrade,
        redact_secrets_in_text,
    };

    /// Instance names this edge will proxy. Host-local map chooses which are live.
    pub const ALLOWED_INSTANCE_NAMES: &[&str] =
        &["mainnet", "testnet3", "testnet4", "mutinynet", "liquid"];

    /// Directory for splora sockets on the edge host.
    pub const DEFAULT_SOCKET_DIR: &str = "/run/splora";

    /// Queue unit socket (not an indexer instance).
    pub const DEFAULT_QUEUE_SOCKET: &str = "/run/splora/queue.sock";

    /// Public path for queue POST `{npub,email}` only.
    pub const DEFAULT_QUEUE_PATH: &str = "/splora/queue";

    /// WebSocket path on each indexer HTTP socket. Exact path only.
    pub const INDEXER_WS_PATH: &str = "/api/v1/ws";

    /// `GET /` hrefs on the one public Host. Not per-network Hosts.
    pub const PUBLIC_PATH_HREFS: &[&str] = &[
        "/api/",
        "/testnet/",
        "/testnet4/",
        "/signet/",
        "/mutinynet/",
        "/liquid/",
    ];

    pub const DEFAULT_BODY_LIMIT_BYTES: usize = 8 * 1024 * 1024;
    pub const QUEUE_BODY_LIMIT_BYTES: usize = 64 * 1024;

    /// One indexer instance behind a Host list and one HTTP Unix socket.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SploraInstance {
        pub name: String,
        pub hosts: Vec<String>,
        pub http_socket: PathBuf,
    }

    /// Runtime knobs for the splora Unix-socket proxy (env / Nix). Default off.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SploraProxyConfig {
        pub enable: bool,
        pub instances: Vec<SploraInstance>,
        pub queue_socket: PathBuf,
        pub queue_path: String,
        pub body_limit_bytes: usize,
        /// Public portal Host (one DNS name). Not an indexer. Env
        /// `SURMOUNT_SPLORA_PORTAL_HOST`. None when the proxy is off or unset.
        pub portal_host: Option<String>,
    }

    impl Default for SploraProxyConfig {
        fn default() -> Self {
            Self {
                enable: false,
                instances: Vec::new(),
                queue_socket: PathBuf::from(DEFAULT_QUEUE_SOCKET),
                queue_path: DEFAULT_QUEUE_PATH.to_string(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host: None,
            }
        }
    }

    impl SploraProxyConfig {
        pub fn from_env() -> Result<Self, String> {
            Self::from_env_map(|k| std::env::var(k).ok())
        }

        pub fn from_env_map(get: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
            if let Some(raw) = get("SURMOUNT_SPLORA_ELECTRUM_SOCKET") {
                let t = raw.trim();
                if !t.is_empty() {
                    return Err(
                        "SURMOUNT_SPLORA_ELECTRUM_SOCKET is set: this edge does not proxy \
                     the Electrum newline Unix socket (fail-closed)"
                            .into(),
                    );
                }
            }

            let enable = match get("SURMOUNT_SPLORA_PROXY") {
                Some(v) => crate::config::parse_env_flag_truthy(&v),
                None => false,
            };

            let socket_dir = match get("SURMOUNT_SPLORA_SOCKET_DIR") {
                Some(v) if !v.trim().is_empty() => v.trim().to_string(),
                _ => DEFAULT_SOCKET_DIR.to_string(),
            };
            refuse_electrum_path(&socket_dir, "SURMOUNT_SPLORA_SOCKET_DIR")?;

            let queue_socket = match get("SURMOUNT_SPLORA_QUEUE_SOCKET") {
                Some(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
                _ => PathBuf::from(DEFAULT_QUEUE_SOCKET),
            };
            refuse_electrum_path(
                queue_socket.to_str().unwrap_or(""),
                "SURMOUNT_SPLORA_QUEUE_SOCKET",
            )?;

            let queue_path = match get("SURMOUNT_SPLORA_QUEUE_PATH") {
                Some(v) if !v.trim().is_empty() => normalize_queue_path(&v)?,
                _ => DEFAULT_QUEUE_PATH.to_string(),
            };

            let instances = match get("SURMOUNT_SPLORA_INSTANCES") {
                Some(v) if !v.trim().is_empty() => parse_instances_json(&v, &socket_dir)?,
                _ => Vec::new(),
            };

            if enable && instances.is_empty() {
                return Err(
                    "SURMOUNT_SPLORA_PROXY is on but SURMOUNT_SPLORA_INSTANCES is empty \
                 (fail-closed; configure a Host -> Unix socket map)"
                        .into(),
                );
            }

            let portal_host = parse_portal_host(get("SURMOUNT_SPLORA_PORTAL_HOST"), enable)?;
            if let Some(ref portal) = portal_host {
                if instances
                    .iter()
                    .any(|inst| inst.hosts.iter().any(|h| h == portal))
                {
                    return Err(format!(
                        "SURMOUNT_SPLORA_PORTAL_HOST={portal} must not also be an indexer Host \
                         (the portal is not a fifth indexer)"
                    ));
                }
            }

            Ok(Self {
                enable,
                instances,
                queue_socket,
                queue_path,
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host,
            })
        }

        /// True when this request is a splora Host, the portal, or the queue path (skip Nostr).
        pub fn should_bypass_auth(&self, host: &str, path: &str) -> bool {
            if !self.enable {
                return false;
            }
            self.is_queue_path(path)
                || self.is_portal_host(host)
                || self.instance_for_host(host).is_some()
        }

        pub fn is_portal_host(&self, host: &str) -> bool {
            if !self.enable {
                return false;
            }
            let Some(portal) = self.portal_host.as_deref() else {
                return false;
            };
            let key = normalize_host(host);
            !key.is_empty() && key == portal
        }

        /// Indexer Hosts plus the portal Host (when set). Empty when the proxy is off.
        pub fn extra_onion_and_redirect_hosts(&self) -> Vec<String> {
            if !self.enable {
                return Vec::new();
            }
            let mut out = Vec::new();
            for inst in &self.instances {
                for host in &inst.hosts {
                    if !out.iter().any(|h| h == host) {
                        out.push(host.clone());
                    }
                }
            }
            if let Some(portal) = self.portal_host.as_ref() {
                if !portal.is_empty() && !out.iter().any(|h| h == portal) {
                    out.push(portal.clone());
                }
            }
            out
        }

        pub fn is_queue_path(&self, path: &str) -> bool {
            path_is_queue(&self.queue_path, path)
        }

        pub fn instance_by_name(&self, name: &str) -> Option<&SploraInstance> {
            self.instances.iter().find(|inst| inst.name == name)
        }

        /// Instance names present in this map. Absent `mainnet` means `/api` is off.
        pub fn enabled_instance_names(&self) -> Vec<&str> {
            self.instances
                .iter()
                .map(|inst| inst.name.as_str())
                .collect()
        }

        pub fn instance_for_host(&self, host: &str) -> Option<&SploraInstance> {
            let key = normalize_host(host);
            if key.is_empty() {
                return None;
            }
            self.instances
                .iter()
                .find(|inst| inst.hosts.iter().any(|h| h == &key))
        }
    }

    fn parse_portal_host(raw: Option<String>, enable: bool) -> Result<Option<String>, String> {
        if !enable {
            return Ok(None);
        }
        let Some(v) = raw else {
            return Ok(None);
        };
        let n = normalize_host(&v);
        if n.is_empty() {
            return Ok(None);
        }
        Ok(Some(n))
    }

    fn path_is_queue(queue_path: &str, path: &str) -> bool {
        let p = queue_path.trim_end_matches('/');
        !p.is_empty() && (path == p || path == format!("{p}/"))
    }

    /// Where one public path goes. No redirect arm: `/signet` and `/mutinynet`
    /// are both proxies to the mutinynet socket.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum SploraPublicRoute {
        Index,
        Queue,
        NotFound,
        Proxy {
            socket_name: &'static str,
            backend_path: String,
            query: Option<String>,
            websocket: bool,
        },
    }

    /// Path routing for the portal Host.
    ///
    /// Network prefix is stripped first. The query string is kept. A
    /// remainder of `/api` or `/api/...` keeps the Splora REST rule: drop a
    /// leading `/api`, except exact `/api/v1/ws`, which stays and is the
    /// only upgrade. Any other remainder (`/block/...`, `/tx/...`,
    /// `/address/...`, `/`, and later UI paths) is forwarded to that network
    /// socket. This repo does not render those pages. Unprefixed `/block`,
    /// `/tx`, and `/address` are not routes here. `/testnet3` is not
    /// `/testnet`. The queue path is not a network prefix. `/api` with no
    /// network prefix is mainnet, and is not proxied when mainnet is absent.
    pub fn route_splora_public(
        path: &str,
        query: Option<&str>,
        queue_path: &str,
        enabled: &[&str],
    ) -> SploraPublicRoute {
        let path = path.split('?').next().unwrap_or(path);
        if path_is_queue(queue_path, path) {
            return SploraPublicRoute::Queue;
        }
        if path == "/" {
            return SploraPublicRoute::Index;
        }
        let kept = match query {
            Some(q) if !q.is_empty() => Some(q.to_string()),
            _ => None,
        };
        if let Some(rest) = strip_named_prefix(path, "/testnet4") {
            return proxy_after_network("testnet4", rest, kept, enabled);
        }
        if let Some(rest) = strip_named_prefix(path, "/testnet") {
            return proxy_after_network("testnet3", rest, kept, enabled);
        }
        if let Some(rest) = strip_named_prefix(path, "/signet") {
            return proxy_after_network("mutinynet", rest, kept, enabled);
        }
        if let Some(rest) = strip_named_prefix(path, "/mutinynet") {
            return proxy_after_network("mutinynet", rest, kept, enabled);
        }
        if let Some(rest) = strip_named_prefix(path, "/liquid") {
            return proxy_after_network("liquid", rest, kept, enabled);
        }
        if path == "/api" || path.starts_with("/api/") {
            return proxy_after_network("mainnet", path, kept, enabled);
        }
        SploraPublicRoute::NotFound
    }

    fn strip_named_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
        if path == prefix {
            return Some("");
        }
        let rest = path.strip_prefix(prefix)?;
        if rest.starts_with('/') {
            Some(rest)
        } else {
            None
        }
    }

    fn proxy_after_network(
        socket_name: &'static str,
        after_network: &str,
        query: Option<String>,
        enabled: &[&str],
    ) -> SploraPublicRoute {
        if !enabled.iter().any(|name| *name == socket_name) {
            return SploraPublicRoute::NotFound;
        }
        let rest_is_api = after_network == "/api" || after_network.starts_with("/api/");
        let backend_path = if rest_is_api {
            backend_path_for_indexer(after_network)
        } else {
            forward_ui_remainder(after_network)
        };
        SploraPublicRoute::Proxy {
            socket_name,
            backend_path,
            query,
            websocket: rest_is_api && after_network == INDEXER_WS_PATH,
        }
    }

    /// Remainder after the network prefix, when it is not REST.
    /// Empty is the prefix itself (`/testnet4`). Not HTML from this repo.
    fn forward_ui_remainder(after_network: &str) -> String {
        if after_network.is_empty() || after_network == "/" {
            "/".to_string()
        } else if after_network.starts_with('/') {
            after_network.to_string()
        } else {
            format!("/{after_network}")
        }
    }

    /// `/api/v1/ws` stays. Other `/api/...` loses the `/api` prefix.
    fn backend_path_for_indexer(path_with_api: &str) -> String {
        if path_with_api == INDEXER_WS_PATH || path_with_api.starts_with("/api/v1/ws/") {
            return path_with_api.to_string();
        }
        match path_with_api.strip_prefix("/api") {
            Some("") => "/".to_string(),
            Some(rest) => rest.to_string(),
            None => path_with_api.to_string(),
        }
    }

    fn html_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    }

    /// GET / HTML for the one public Host. Same-host path list. Not an explorer.
    pub fn portal_index_html(_cfg: &SploraProxyConfig) -> String {
        let mut items = String::new();
        for href in PUBLIC_PATH_HREFS {
            let h = html_escape(href);
            items.push_str(&format!("<li><a href=\"{h}\">{h}</a></li>\n"));
        }
        format!(
            "<!DOCTYPE html>\n\
<html lang=\"en\">\n\
<head><meta charset=\"utf-8\"/><title>Splora</title></head>\n\
<body>\n\
<h1>Splora</h1>\n\
<p>Paths on this host:</p>\n\
<ul>\n{items}</ul>\n\
</body>\n\
</html>\n"
        )
    }

    fn portal_response(cfg: &SploraProxyConfig, method: &Method, path: &str) -> Response {
        let p = path.split('?').next().unwrap_or(path);
        if p != "/" {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
        if *method != Method::GET {
            return (StatusCode::METHOD_NOT_ALLOWED, "GET only").into_response();
        }
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            portal_index_html(cfg),
        )
            .into_response()
    }

    fn refuse_electrum_path(path: &str, label: &str) -> Result<(), String> {
        let lower = path.to_ascii_lowercase();
        if lower.contains("electrum") {
            return Err(format!(
                "{label}={path:?} looks like an Electrum newline socket; this edge \
             does not proxy that protocol (fail-closed)"
            ));
        }
        Ok(())
    }

    /// Leading slash, no trailing slash, not `/`.
    pub fn normalize_queue_path(raw: &str) -> Result<String, String> {
        let t = raw.trim();
        if t.is_empty() {
            return Err("splora queue path must not be empty".into());
        }
        if t.contains("://") || t.contains('?') || t.contains('#') || t.contains(' ') {
            return Err(format!("splora queue path must be a path (got {t:?})"));
        }
        let with_slash = if t.starts_with('/') {
            t.to_string()
        } else {
            format!("/{t}")
        };
        let trimmed = with_slash.trim_end_matches('/').to_string();
        if trimmed.is_empty() || trimmed == "/" {
            return Err("splora queue path must not be '/'".into());
        }
        Ok(trimmed)
    }

    pub fn normalize_host(raw: &str) -> String {
        crate::redirect::host_for_url_authority(raw)
            .trim()
            .to_ascii_lowercase()
    }

    fn allowed_instance_name(name: &str) -> bool {
        ALLOWED_INSTANCE_NAMES.iter().any(|n| *n == name)
    }

    #[derive(Debug, Deserialize)]
    struct InstanceJson {
        #[serde(default)]
        hosts: Vec<String>,
        #[serde(default)]
        socket: String,
    }

    /// JSON object: `{ "mainnet": { "hosts": ["esplora.example.test"], "socket": "..." } }`.
    pub fn parse_instances_json(
        raw: &str,
        socket_dir: &str,
    ) -> Result<Vec<SploraInstance>, String> {
        let value: serde_json::Value = serde_json::from_str(raw.trim()).map_err(|e| {
            format!("SURMOUNT_SPLORA_INSTANCES must be a JSON object instance -> hosts/socket: {e}")
        })?;
        let obj = value.as_object().ok_or_else(|| {
            "SURMOUNT_SPLORA_INSTANCES must be a JSON object (instance name -> config)".to_string()
        })?;
        let mut out = Vec::new();
        let mut seen_hosts: BTreeMap<String, String> = BTreeMap::new();
        for (name, cfg) in obj {
            if !allowed_instance_name(name) {
                return Err(format!(
                    "SURMOUNT_SPLORA_INSTANCES unknown instance {name:?}; allowed: {}",
                    ALLOWED_INSTANCE_NAMES.join(", ")
                ));
            }
            let parsed: InstanceJson = serde_json::from_value(cfg.clone()).map_err(|e| {
                format!(
                    "SURMOUNT_SPLORA_INSTANCES[{name}] must be an object with hosts/socket: {e}"
                )
            })?;
            let socket = if parsed.socket.trim().is_empty() {
                PathBuf::from(format!("{socket_dir}/{name}.http.sock"))
            } else {
                PathBuf::from(parsed.socket.trim())
            };
            refuse_electrum_path(socket.to_str().unwrap_or(""), "splora instance socket")?;
            if !socket.is_absolute() {
                return Err(format!(
                    "SURMOUNT_SPLORA_INSTANCES[{name}].socket must be an absolute path"
                ));
            }
            let mut hosts = Vec::new();
            for h in parsed.hosts {
                let n = normalize_host(&h);
                if n.is_empty() {
                    return Err(format!(
                        "SURMOUNT_SPLORA_INSTANCES[{name}] host must not be empty"
                    ));
                }
                if let Some(other) = seen_hosts.get(&n) {
                    return Err(format!(
                        "SURMOUNT_SPLORA_INSTANCES host {n} is mapped to both {other} and {name}"
                    ));
                }
                seen_hosts.insert(n.clone(), name.clone());
                hosts.push(n);
            }
            // Empty hosts is the path-routing map: one portal Host, no
            // per-network public Host. The instance name still selects the socket.
            out.push(SploraInstance {
                name: name.clone(),
                hosts,
                http_socket: socket,
            });
        }
        Ok(out)
    }

    /// `X-Forwarded-Proto` for the hop to splora (always set; tests fail if omitted).
    pub fn forwarded_proto(listen_is_https: bool) -> &'static str {
        if listen_is_https { "https" } else { "http" }
    }

    /// Axum middleware: Host/path match -> Unix HTTP/1.1 (or WebSocket) proxy.
    pub async fn splora_proxy_middleware(
        State(state): State<Arc<AppState>>,
        request: Request,
        next: Next,
    ) -> Response {
        let cfg = &state.config.splora_proxy;
        if !cfg.enable {
            return next.run(request).await;
        }

        let path = request.uri().path().to_string();
        let host = crate::redirect::request_authority_host(
            header_str(request.headers(), "host"),
            request.uri().host(),
        );

        if cfg.is_queue_path(&path) {
            return proxy_queue(state, request).await;
        }

        if cfg.is_portal_host(&host) {
            let decision = {
                let enabled = cfg.enabled_instance_names();
                route_splora_public(&path, request.uri().query(), &cfg.queue_path, &enabled)
            };
            return match decision {
                SploraPublicRoute::Queue => proxy_queue(state, request).await,
                SploraPublicRoute::Index => portal_response(cfg, request.method(), &path),
                SploraPublicRoute::NotFound => (StatusCode::NOT_FOUND, "not found").into_response(),
                SploraPublicRoute::Proxy {
                    socket_name,
                    backend_path,
                    websocket,
                    ..
                } => {
                    let Some(instance) = cfg.instance_by_name(socket_name).cloned() else {
                        return (StatusCode::NOT_FOUND, "not found").into_response();
                    };
                    proxy_indexer(state, request, instance, host, backend_path, websocket).await
                }
            };
        }

        if let Some(instance) = cfg.instance_for_host(&host).cloned() {
            let backend_path = path.clone();
            let websocket = backend_path == INDEXER_WS_PATH;
            return proxy_indexer(state, request, instance, host, backend_path, websocket).await;
        }

        next.run(request).await
    }

    fn header_str<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
        headers.get(name).and_then(|v| v.to_str().ok())
    }

    async fn proxy_queue(state: Arc<AppState>, req: Request) -> Response {
        if req.method() != Method::POST {
            return (StatusCode::METHOD_NOT_ALLOWED, "POST only").into_response();
        }
        let headers = req.headers().clone();
        let limit = QUEUE_BODY_LIMIT_BYTES;
        let body = match axum::body::to_bytes(req.into_body(), limit).await {
            Ok(b) => b,
            Err(_) => {
                return (StatusCode::PAYLOAD_TOO_LARGE, "queue body too large").into_response();
            }
        };
        if !queue_body_has_npub_and_email(&body) {
            return (
                StatusCode::BAD_REQUEST,
                "queue body must be JSON object with npub and email",
            )
                .into_response();
        }
        let host = header_str(&headers, "host")
            .map(normalize_host)
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| state.config.services_hostname.clone());
        let proto = forwarded_proto(state.config.listen_mode.is_https());
        let path = state.config.splora_proxy.queue_path.clone();
        proxy_unix_http(
            state.config.splora_proxy.queue_socket.as_path(),
            Method::POST,
            &path,
            None,
            &host,
            proto,
            &headers,
            body,
            false,
        )
        .await
    }

    fn queue_body_has_npub_and_email(bytes: &[u8]) -> bool {
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return false;
        };
        let Some(obj) = v.as_object() else {
            return false;
        };
        nonempty_str_field(obj.get("npub")) && nonempty_str_field(obj.get("email"))
    }

    fn nonempty_str_field(v: Option<&serde_json::Value>) -> bool {
        v.and_then(|x| x.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
    }

    async fn proxy_indexer(
        state: Arc<AppState>,
        req: Request,
        instance: SploraInstance,
        host: String,
        backend_path: String,
        websocket: bool,
    ) -> Response {
        let query = req.uri().query().map(str::to_string);
        let method = req.method().clone();
        let headers = req.headers().clone();
        let proto = forwarded_proto(state.config.listen_mode.is_https());
        let wants_ws = websocket && is_websocket_upgrade(&headers);

        if wants_ws {
            return proxy_unix_websocket(
                instance.http_socket,
                req,
                &backend_path,
                query.as_deref(),
                &host,
                proto,
            )
            .await;
        }

        let limit = state.config.splora_proxy.body_limit_bytes;
        let body = match axum::body::to_bytes(req.into_body(), limit).await {
            Ok(b) => b,
            Err(_) => {
                return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
            }
        };

        proxy_unix_http(
            instance.http_socket.as_path(),
            method,
            &backend_path,
            query.as_deref(),
            &host,
            proto,
            &headers,
            body,
            false,
        )
        .await
    }

    /// HTTP/1.1 over a Unix socket. Always sets Host and X-Forwarded-Proto.
    #[allow(clippy::too_many_arguments)]
    pub async fn proxy_unix_http(
        socket: &Path,
        method: Method,
        path: &str,
        query: Option<&str>,
        host: &str,
        forwarded_proto: &str,
        incoming_headers: &HeaderMap,
        body: Bytes,
        preserve_upgrade: bool,
    ) -> Response {
        let uri = match origin_form_uri(path, query) {
            Ok(u) => u,
            Err(e) => {
                warn!(error = %e, "splora_proxy_bad_uri");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        };

        let stream = match tokio::time::timeout(
            Duration::from_secs(10),
            UnixStream::connect(socket),
        )
        .await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let safe = redact_secrets_in_text(&format!("uds_connect: {e}"));
                warn!(error = %safe, socket = %socket.display(), "splora_proxy_unix_connect");
                return (StatusCode::BAD_GATEWAY, "splora upstream unreachable").into_response();
            }
            Err(_) => {
                warn!(socket = %socket.display(), "splora_proxy_unix_connect_timeout");
                return (StatusCode::BAD_GATEWAY, "splora upstream unreachable").into_response();
            }
        };

        let io = TokioIo::new(stream);
        let (mut sender, conn) = match hyper::client::conn::http1::handshake(io).await {
            Ok(pair) => pair,
            Err(e) => {
                let safe = redact_secrets_in_text(&format!("uds_handshake: {e}"));
                warn!(error = %safe, "splora_proxy_unix_handshake");
                return (StatusCode::BAD_GATEWAY, "splora upstream unreachable").into_response();
            }
        };
        tokio::spawn(async move {
            if let Err(e) = conn.await {
                debug!(
                    error = %redact_secrets_in_text(&e.to_string()),
                    "splora_proxy_unix_conn"
                );
            }
        });

        let filtered = filter_request_headers(incoming_headers, preserve_upgrade);
        let mut builder = hyper::Request::builder().method(method).uri(uri);
        for (name, value) in filtered.iter() {
            builder = builder.header(name, value);
        }
        builder = builder.header(header::HOST, host);
        builder = builder.header("x-forwarded-proto", forwarded_proto);

        let outbound = match builder.body(Full::new(body)) {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "splora_proxy_build_request");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        };

        let resp = match sender.send_request(outbound).await {
            Ok(r) => r,
            Err(e) => {
                let safe = redact_secrets_in_text(&format!("uds_send: {e}"));
                warn!(error = %safe, "splora_proxy_unix_send");
                return (StatusCode::BAD_GATEWAY, "splora upstream unreachable").into_response();
            }
        };

        hyper_response_to_axum(resp, preserve_upgrade).await
    }

    async fn hyper_response_to_axum(
        resp: hyper::Response<Incoming>,
        preserve_upgrade: bool,
    ) -> Response {
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = match resp.into_body().collect().await {
            Ok(c) => c.to_bytes(),
            Err(e) => {
                let safe = redact_secrets_in_text(&format!("uds_body: {e}"));
                warn!(error = %safe, "splora_proxy_unix_body");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        };
        let mut out = Response::builder().status(status);
        let filtered_out = filter_response_headers(&headers, preserve_upgrade);
        if let Some(hmap) = out.headers_mut() {
            for (name, value) in filtered_out.iter() {
                hmap.append(name, value.clone());
            }
        }
        out.body(Body::from(bytes))
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
    }

    fn origin_form_uri(path: &str, query: Option<&str>) -> Result<Uri, String> {
        let p = if path.is_empty() { "/" } else { path };
        if !p.starts_with('/') {
            return Err(format!("path must start with / (got {p:?})"));
        }
        let s = match query {
            Some(q) if !q.is_empty() => format!("{p}?{q}"),
            _ => p.to_string(),
        };
        s.parse::<Uri>()
            .map_err(|e| format!("invalid origin-form URI {s:?}: {e}"))
    }

    /// WebSocket upgrade over the same indexer HTTP Unix socket (101 tunnel).
    async fn proxy_unix_websocket(
        socket: PathBuf,
        req: Request,
        path: &str,
        query: Option<&str>,
        host: &str,
        forwarded_proto: &str,
    ) -> Response {
        let path_and_query = match query {
            Some(q) if !q.is_empty() => format!("{path}?{q}"),
            _ => path.to_string(),
        };

        let unix = match tokio::time::timeout(Duration::from_secs(10), UnixStream::connect(&socket))
            .await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                let safe = redact_secrets_in_text(&format!("ws_uds_connect: {e}"));
                warn!(error = %safe, "splora_proxy_ws_connect");
                return (StatusCode::BAD_GATEWAY, "splora upstream unreachable").into_response();
            }
            Err(_) => {
                warn!("splora_proxy_ws_connect_timeout");
                return (StatusCode::BAD_GATEWAY, "splora upstream unreachable").into_response();
            }
        };

        let method = req.method().clone();
        let headers = req.headers().clone();
        let filtered = filter_request_headers(&headers, true);

        let mut req_buf = format!("{method} {path_and_query} HTTP/1.1\r\nHost: {host}\r\n");
        req_buf.push_str("X-Forwarded-Proto: ");
        req_buf.push_str(forwarded_proto);
        req_buf.push_str("\r\n");
        for (name, value) in filtered.iter() {
            if name.as_str().eq_ignore_ascii_case("x-forwarded-proto") {
                continue;
            }
            if let Ok(v) = value.to_str() {
                req_buf.push_str(name.as_str());
                req_buf.push_str(": ");
                req_buf.push_str(v);
                req_buf.push_str("\r\n");
            }
        }
        if !filtered.contains_key(header::CONNECTION) {
            req_buf.push_str("Connection: Upgrade\r\n");
        }
        if !filtered.contains_key(header::UPGRADE) {
            req_buf.push_str("Upgrade: websocket\r\n");
        }
        req_buf.push_str("\r\n");

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut upstream_stream = unix;
        if let Err(e) = upstream_stream.write_all(req_buf.as_bytes()).await {
            let safe = redact_secrets_in_text(&format!("ws_write: {e}"));
            warn!(error = %safe, "splora_proxy_ws_write");
            return StatusCode::BAD_GATEWAY.into_response();
        }

        let mut header_bytes = Vec::with_capacity(1024);
        let mut tmp = [0u8; 256];
        let header_end = loop {
            if header_bytes.len() > 64 * 1024 {
                warn!("splora_proxy_ws_header_too_large");
                return StatusCode::BAD_GATEWAY.into_response();
            }
            let n = match upstream_stream.read(&mut tmp).await {
                Ok(0) => {
                    warn!("splora_proxy_ws_upstream_eof");
                    return StatusCode::BAD_GATEWAY.into_response();
                }
                Ok(n) => n,
                Err(e) => {
                    let safe = redact_secrets_in_text(&format!("ws_read: {e}"));
                    warn!(error = %safe, "splora_proxy_ws_read");
                    return StatusCode::BAD_GATEWAY.into_response();
                }
            };
            header_bytes.extend_from_slice(&tmp[..n]);
            if let Some(pos) = find_header_end(&header_bytes) {
                break pos;
            }
        };

        let header_blob = &header_bytes[..header_end];
        let rest = header_bytes[header_end..].to_vec();
        let header_str = String::from_utf8_lossy(header_blob);
        let mut lines = header_str.split("\r\n");
        let status_line = lines.next().unwrap_or("");
        let status_code = parse_status_code(status_line).unwrap_or(502);
        let mut resp_headers = HeaderMap::new();
        for line in lines {
            if line.is_empty() {
                continue;
            }
            if let Some((n, v)) = line.split_once(':')
                && let (Ok(name), Ok(val)) = (
                    HeaderName::from_bytes(n.trim().as_bytes()),
                    HeaderValue::from_str(v.trim()),
                )
            {
                resp_headers.append(name, val);
            }
        }

        if status_code != 101 {
            let mut body = rest;
            let mut buf = [0u8; 8192];
            if let Ok(Ok(n)) =
                tokio::time::timeout(Duration::from_millis(200), upstream_stream.read(&mut buf))
                    .await
                && n > 0
            {
                body.extend_from_slice(&buf[..n]);
            }
            let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::BAD_GATEWAY);
            let filtered_out = filter_response_headers(&resp_headers, false);
            let mut out = Response::builder().status(status);
            if let Some(hmap) = out.headers_mut() {
                for (name, value) in filtered_out.iter() {
                    hmap.append(name, value.clone());
                }
            }
            return out
                .body(Body::from(body))
                .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response());
        }

        let filtered_out = filter_response_headers(&resp_headers, true);
        let upgrade = match hyper::upgrade::on(req).await {
            Ok(upgraded) => upgraded,
            Err(e) => {
                let safe = redact_secrets_in_text(&format!("client_upgrade: {e}"));
                warn!(error = %safe, "splora_proxy_client_upgrade");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        };

        tokio::spawn(async move {
            if let Err(e) = websocket_unix_tunnel(upgrade, upstream_stream, rest).await {
                debug!(
                    error = %redact_secrets_in_text(&e.to_string()),
                    "splora_proxy_ws_tunnel_end"
                );
            }
        });

        let mut out = Response::builder().status(StatusCode::SWITCHING_PROTOCOLS);
        if let Some(hmap) = out.headers_mut() {
            for (name, value) in filtered_out.iter() {
                hmap.append(name, value.clone());
            }
            if !hmap.contains_key(header::UPGRADE) {
                hmap.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
            }
            if !hmap.contains_key(header::CONNECTION) {
                hmap.insert(header::CONNECTION, HeaderValue::from_static("Upgrade"));
            }
        }
        out.body(Body::empty())
            .unwrap_or_else(|_| StatusCode::BAD_GATEWAY.into_response())
    }

    fn find_header_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
    }

    fn parse_status_code(status_line: &str) -> Option<u16> {
        let mut parts = status_line.split_whitespace();
        let _http = parts.next()?;
        parts.next()?.parse().ok()
    }

    async fn websocket_unix_tunnel(
        client: hyper::upgrade::Upgraded,
        mut upstream: UnixStream,
        pending_from_upstream: Vec<u8>,
    ) -> Result<(), std::io::Error> {
        use tokio::io::AsyncWriteExt;

        let mut client = hyper_util::rt::TokioIo::new(client);

        if !pending_from_upstream.is_empty() {
            client.write_all(&pending_from_upstream).await?;
        }

        let (mut client_r, mut client_w) = tokio::io::split(client);
        let (mut up_r, mut up_w) = upstream.split();

        let c2u = async {
            tokio::io::copy(&mut client_r, &mut up_w).await?;
            up_w.shutdown().await
        };
        let u2c = async {
            tokio::io::copy(&mut up_r, &mut client_w).await?;
            client_w.shutdown().await
        };

        tokio::select! {
            r = c2u => { r?; }
            r = u2c => { r?; }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use axum::Router;
        use axum::extract::Request as AxumRequest;
        use axum::routing::{any, get, post};
        use std::sync::Mutex;

        fn tmp_sock(prefix: &str) -> PathBuf {
            std::env::temp_dir().join(format!(
                "{prefix}-{}-{}.sock",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
        }

        #[test]
        fn electrum_socket_env_fails_closed() {
            let err = SploraProxyConfig::from_env_map(|k| match k {
                "SURMOUNT_SPLORA_ELECTRUM_SOCKET" => {
                    Some("/run/splora/mainnet.electrum.sock".into())
                }
                _ => None,
            })
            .unwrap_err();
            assert!(err.contains("Electrum"), "{err}");
            assert!(
                err.contains("fail-closed") || err.contains("does not proxy"),
                "{err}"
            );
        }

        #[test]
        fn electrum_in_instance_socket_fails_closed() {
            let err = parse_instances_json(
            r#"{"mainnet":{"hosts":["esplora.example.test"],"socket":"/run/splora/mainnet.electrum.sock"}}"#,
            "/run/splora",
        )
        .unwrap_err();
            assert!(err.to_ascii_lowercase().contains("electrum"), "{err}");
        }

        #[test]
        fn unknown_instance_name_fails_closed() {
            let err = parse_instances_json(
            r#"{"regtest":{"hosts":["x.example.test"],"socket":"/run/splora/regtest.http.sock"}}"#,
            "/run/splora",
        )
        .unwrap_err();
            assert!(err.contains("unknown instance"), "{err}");
        }

        #[test]
        fn proxy_disabled_by_default() {
            let cfg = SploraProxyConfig::from_env_map(|_| None).unwrap();
            assert!(!cfg.enable);
            assert!(cfg.instances.is_empty());
        }

        #[test]
        fn enable_without_instances_fails_closed() {
            let err = SploraProxyConfig::from_env_map(|k| match k {
                "SURMOUNT_SPLORA_PROXY" => Some("1".into()),
                _ => None,
            })
            .unwrap_err();
            assert!(err.contains("INSTANCES"), "{err}");
        }

        #[test]
        fn instances_json_maps_hosts_and_default_socket() {
            let cfg = SploraProxyConfig::from_env_map(|k| match k {
            "SURMOUNT_SPLORA_PROXY" => Some("true".into()),
            "SURMOUNT_SPLORA_INSTANCES" => Some(
                r#"{"mainnet":{"hosts":["Esplora.Example.TEST:443"]},"liquid":{"hosts":["liquid.example.test"],"socket":"/run/splora/liquid.http.sock"}}"#
                    .into(),
            ),
            _ => None,
        })
        .unwrap();
            assert!(cfg.enable);
            assert_eq!(cfg.instances.len(), 2);
            let main = cfg.instance_for_host("esplora.example.test").unwrap();
            assert_eq!(main.name, "mainnet");
            assert_eq!(
                main.http_socket,
                PathBuf::from("/run/splora/mainnet.http.sock")
            );
            assert!(cfg.should_bypass_auth("esplora.example.test", "/api/tx/abc"));
            assert!(cfg.should_bypass_auth("services.example.test", "/splora/queue"));
            assert!(!cfg.should_bypass_auth("services.example.test", "/health"));
            assert!(cfg.portal_host.is_none());
        }

        fn four_live_mainnet_off_cfg(portal: &str) -> SploraProxyConfig {
            SploraProxyConfig {
                enable: true,
                instances: vec![
                    SploraInstance {
                        name: "testnet3".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/testnet3.http.sock"),
                    },
                    SploraInstance {
                        name: "testnet4".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/testnet4.http.sock"),
                    },
                    SploraInstance {
                        name: "mutinynet".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/mutinynet.http.sock"),
                    },
                    SploraInstance {
                        name: "liquid".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/liquid.http.sock"),
                    },
                ],
                queue_socket: PathBuf::from(DEFAULT_QUEUE_SOCKET),
                queue_path: DEFAULT_QUEUE_PATH.into(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host: Some(portal.to_string()),
            }
        }

        const FOUR_LIVE: &[&str] = &["testnet3", "testnet4", "mutinynet", "liquid"];

        fn route_four(path: &str, query: Option<&str>) -> SploraPublicRoute {
            route_splora_public(path, query, DEFAULT_QUEUE_PATH, FOUR_LIVE)
        }

        fn expect_proxy(
            route: SploraPublicRoute,
            socket: &str,
            backend: &str,
            query: Option<&str>,
            websocket: bool,
        ) {
            match route {
                SploraPublicRoute::Proxy {
                    socket_name,
                    backend_path,
                    query: got_q,
                    websocket: got_ws,
                } => {
                    assert_eq!(socket_name, socket);
                    assert_eq!(backend_path, backend);
                    assert_eq!(got_q.as_deref(), query);
                    assert_eq!(got_ws, websocket);
                }
                other => panic!("expected proxy to {socket} {backend}, got {other:?}"),
            }
        }

        /// Named contract: each public prefix selects that network socket.
        /// REST drops `/api` (Splora http_front). Mainnet is absent here.
        #[test]
        fn splora_public_prefix_forwards_to_named_socket() {
            expect_proxy(
                route_four("/testnet/api/tx/x", None),
                "testnet3",
                "/tx/x",
                None,
                false,
            );
            expect_proxy(
                route_four("/testnet4/api/v1/blocks/tip/height", None),
                "testnet4",
                "/v1/blocks/tip/height",
                None,
                false,
            );
            expect_proxy(
                route_four("/signet/api/tx/x", None),
                "mutinynet",
                "/tx/x",
                None,
                false,
            );
            expect_proxy(
                route_four("/mutinynet/api/tx/x", None),
                "mutinynet",
                "/tx/x",
                None,
                false,
            );
            expect_proxy(
                route_four("/liquid/api/tx/x", None),
                "liquid",
                "/tx/x",
                None,
                false,
            );
            assert!(matches!(
                route_four("/api/tx/x", None),
                SploraPublicRoute::NotFound
            ));
        }

        /// Named contract: public `/testnet3/...` is not the testnet prefix.
        #[test]
        fn splora_testnet_is_not_testnet3_prefix() {
            assert!(matches!(
                route_four("/testnet3/api/tx/x", None),
                SploraPublicRoute::NotFound
            ));
            assert!(matches!(
                route_four("/testnet3", None),
                SploraPublicRoute::NotFound
            ));
            assert!(matches!(
                route_four("/testnet30/api/x", None),
                SploraPublicRoute::NotFound
            ));
            expect_proxy(
                route_four("/testnet/api/tx/x", None),
                "testnet3",
                "/tx/x",
                None,
                false,
            );
            expect_proxy(
                route_four("/testnet4/api/tx/x", None),
                "testnet4",
                "/tx/x",
                None,
                false,
            );
        }

        /// Named contract: `/signet` and `/mutinynet` share mutinynet. No redirect.
        #[test]
        fn splora_signet_and_mutinynet_share_socket_without_redirect() {
            let signet = route_four("/signet/api/v1/blocks/tip/height", Some("n=1"));
            let mutiny = route_four("/mutinynet/api/v1/blocks/tip/height", Some("n=1"));
            expect_proxy(
                signet.clone(),
                "mutinynet",
                "/v1/blocks/tip/height",
                Some("n=1"),
                false,
            );
            expect_proxy(
                mutiny.clone(),
                "mutinynet",
                "/v1/blocks/tip/height",
                Some("n=1"),
                false,
            );
            assert_eq!(signet, mutiny);
            assert!(!matches!(signet, SploraPublicRoute::NotFound));
        }

        /// Named contract: network prefix strip keeps the query string.
        /// `/testnet/api/v1/tx/abc?x=1` is testnet3, path `/v1/tx/abc`, query `x=1`
        /// (same `/api` strip as Splora `http_front::backend_path`).
        #[test]
        fn splora_query_string_kept_after_prefix_strip() {
            expect_proxy(
                route_four("/testnet/api/v1/tx/abc", Some("x=1")),
                "testnet3",
                "/v1/tx/abc",
                Some("x=1"),
                false,
            );
            expect_proxy(
                route_four("/liquid/api/tx/x", Some("verbose=1&limit=2")),
                "liquid",
                "/tx/x",
                Some("verbose=1&limit=2"),
                false,
            );
            expect_proxy(
                route_four("/testnet4/api/v1/blocks/tip/height", Some("x=1")),
                "testnet4",
                "/v1/blocks/tip/height",
                Some("x=1"),
                false,
            );
        }

        /// Named contract: block, tx, and address paths are forwarded to the
        /// network socket. This repo does not render that HTML. Unprefixed
        /// `/block`, `/tx`, and `/address` stay unmatched so another app on
        /// this router keeps those paths on other hosts.
        #[tokio::test]
        async fn splora_ui_paths_forward_without_html() {
            expect_proxy(
                route_four("/testnet4/tx/abc", Some("x=1")),
                "testnet4",
                "/tx/abc",
                Some("x=1"),
                false,
            );
            assert!(!matches!(
                route_four("/testnet4/tx/abc", Some("x=1")),
                SploraPublicRoute::Index
            ));
            expect_proxy(
                route_four("/testnet4/block/abc", None),
                "testnet4",
                "/block/abc",
                None,
                false,
            );
            expect_proxy(
                route_four("/testnet/address/abc", None),
                "testnet3",
                "/address/abc",
                None,
                false,
            );
            expect_proxy(
                route_four("/liquid/block/abc", None),
                "liquid",
                "/block/abc",
                None,
                false,
            );
            let signet = route_four("/signet/tx/abc", None);
            let mutiny = route_four("/mutinynet/tx/abc", None);
            expect_proxy(signet.clone(), "mutinynet", "/tx/abc", None, false);
            expect_proxy(mutiny.clone(), "mutinynet", "/tx/abc", None, false);
            assert_eq!(signet, mutiny);
            assert!(!matches!(signet, SploraPublicRoute::NotFound));
            expect_proxy(route_four("/testnet4", None), "testnet4", "/", None, false);
            expect_proxy(route_four("/testnet4/", None), "testnet4", "/", None, false);

            for path in [
                "/block/abc",
                "/tx/abc",
                "/address/abc",
                "/block",
                "/tx",
                "/address",
            ] {
                assert!(
                    matches!(route_four(path, None), SploraPublicRoute::NotFound),
                    "{path} must stay unmatched"
                );
                assert!(
                    matches!(
                        route_splora_public(path, None, DEFAULT_QUEUE_PATH, &["mainnet"]),
                        SploraPublicRoute::NotFound
                    ),
                    "{path} must stay unmatched even when mainnet is on"
                );
            }

            let sock = tmp_sock("splora-ui");
            let seen = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
            let seen_c = seen.clone();
            let mock = Router::new().fallback(any(move |req: AxumRequest| {
                let seen_c = seen_c.clone();
                async move {
                    let path = req.uri().path().to_string();
                    let query = req.uri().query().unwrap_or("").to_string();
                    seen_c.lock().unwrap().push((path, query));
                    (
                        StatusCode::OK,
                        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                        "upstream-bytes",
                    )
                }
            }));
            let mock_serve = serve_uds(sock.clone(), mock).await;
            tokio::time::sleep(Duration::from_millis(20)).await;

            let cfg = SploraProxyConfig {
                enable: true,
                instances: vec![
                    SploraInstance {
                        name: "testnet3".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/testnet3.http.sock"),
                    },
                    SploraInstance {
                        name: "testnet4".into(),
                        hosts: vec![],
                        http_socket: sock.clone(),
                    },
                    SploraInstance {
                        name: "mutinynet".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/mutinynet.http.sock"),
                    },
                    SploraInstance {
                        name: "liquid".into(),
                        hosts: vec![],
                        http_socket: PathBuf::from("/run/splora/liquid.http.sock"),
                    },
                ],
                queue_socket: PathBuf::from(DEFAULT_QUEUE_SOCKET),
                queue_path: DEFAULT_QUEUE_PATH.into(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host: Some("splora.surmount.systems".into()),
            };
            let state = test_state_for(cfg, false);
            let app = Router::new()
                .route("/tx/abc", get(|| async { "console-tx" }))
                .fallback(axum::routing::any(|| async { StatusCode::NOT_FOUND }))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    splora_proxy_middleware,
                ))
                .with_state(state);
            let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = lis.local_addr().unwrap();
            let serve = tokio::spawn(async move {
                axum::serve(lis, app).await.ok();
            });
            let client = reqwest::Client::new();

            let tx = client
                .get(format!("http://{addr}/testnet4/tx/abc?x=1"))
                .header("Host", "splora.surmount.systems")
                .send()
                .await
                .unwrap();
            assert_eq!(tx.status(), reqwest::StatusCode::OK);
            let tx_type = tx
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            assert!(
                !tx_type.contains("text/html"),
                "UI forward must not be local HTML, content-type={tx_type}"
            );
            let tx_body = tx.text().await.unwrap();
            assert_eq!(tx_body, "upstream-bytes");
            assert!(!tx_body.contains("<!DOCTYPE html"));
            assert!(!tx_body.contains("esplora.surmount.systems"));
            let hits = seen.lock().unwrap().clone();
            assert_eq!(hits, vec![("/tx/abc".to_string(), "x=1".to_string())]);

            let root = client
                .get(format!("http://{addr}/"))
                .header("Host", "splora.surmount.systems")
                .send()
                .await
                .unwrap();
            assert_eq!(root.status(), reqwest::StatusCode::OK);
            let root_body = root.text().await.unwrap();
            for href in PUBLIC_PATH_HREFS {
                assert!(
                    root_body.contains(&format!("href=\"{href}\"")),
                    "missing {href}: {root_body}"
                );
            }
            assert!(
                !root_body.contains("esplora.surmount.systems"),
                "GET / must not name an esplora Host: {root_body}"
            );
            assert!(!root_body.contains("/block"));
            assert!(!root_body.contains("/tx/"));
            assert!(!root_body.contains("/address"));

            let blocked = client
                .get(format!("http://{addr}/tx/abc"))
                .header("Host", "splora.surmount.systems")
                .send()
                .await
                .unwrap();
            assert_eq!(blocked.status(), reqwest::StatusCode::NOT_FOUND);
            assert_eq!(blocked.text().await.unwrap(), "not found");

            let other = client
                .get(format!("http://{addr}/tx/abc"))
                .header("Host", "services.example.test")
                .send()
                .await
                .unwrap();
            assert_eq!(other.status(), reqwest::StatusCode::OK);
            assert_eq!(other.text().await.unwrap(), "console-tx");

            serve.abort();
            mock_serve.abort();
            let _ = std::fs::remove_file(&sock);
        }

        /// Named contract: only `/{network}/api/v1/ws` (mainnet: `/api/v1/ws`) is an upgrade.
        #[test]
        fn splora_websocket_path_is_upgrade() {
            expect_proxy(
                route_four("/testnet4/api/v1/ws", Some("x=1")),
                "testnet4",
                "/api/v1/ws",
                Some("x=1"),
                true,
            );
            expect_proxy(
                route_four("/signet/api/v1/ws", None),
                "mutinynet",
                "/api/v1/ws",
                None,
                true,
            );
            expect_proxy(
                route_four("/mutinynet/api/v1/ws", None),
                "mutinynet",
                "/api/v1/ws",
                None,
                true,
            );
            expect_proxy(
                route_four("/testnet4/api/v1/ws/extra", None),
                "testnet4",
                "/api/v1/ws/extra",
                None,
                false,
            );
            expect_proxy(
                route_four("/testnet4/api/tx/x", None),
                "testnet4",
                "/tx/x",
                None,
                false,
            );
            assert!(matches!(
                route_four("/api/v1/ws", None),
                SploraPublicRoute::NotFound
            ));
            expect_proxy(
                route_splora_public("/api/v1/ws", None, DEFAULT_QUEUE_PATH, &["mainnet"]),
                "mainnet",
                "/api/v1/ws",
                None,
                true,
            );
        }

        /// Named contract: POST `/splora/queue` is the queue, not a network prefix.
        #[test]
        fn splora_queue_is_not_a_network_prefix() {
            assert!(matches!(
                route_four("/splora/queue", None),
                SploraPublicRoute::Queue
            ));
            assert!(matches!(
                route_four("/splora/queue/", Some("a=1")),
                SploraPublicRoute::Queue
            ));
            assert!(matches!(
                route_four("/splora/queue/api/tx/x", None),
                SploraPublicRoute::NotFound
            ));
            assert!(!matches!(
                route_four("/splora/queue", None),
                SploraPublicRoute::Proxy { .. }
            ));
        }

        /// Named contract: mainnet absent from the instance map => `/api` is 404.
        #[test]
        fn splora_mainnet_api_is_404_when_indexer_disabled() {
            for path in [
                "/api",
                "/api/",
                "/api/tx/x",
                "/api/v1/blocks/tip/height",
                "/api/v1/ws",
            ] {
                assert!(
                    matches!(route_four(path, Some("q=1")), SploraPublicRoute::NotFound),
                    "{path} must be 404 while mainnet is off"
                );
            }
            expect_proxy(
                route_splora_public(
                    "/api/tx/x",
                    Some("q=1"),
                    DEFAULT_QUEUE_PATH,
                    &["mainnet", "testnet4"],
                ),
                "mainnet",
                "/tx/x",
                Some("q=1"),
                false,
            );
            let parsed = parse_instances_json(
                r#"{"testnet4":{"hosts":[]},"liquid":{"hosts":[]}}"#,
                "/run/splora",
            )
            .expect("empty hosts still enable path routing");
            assert!(parsed.iter().all(|inst| inst.hosts.is_empty()));
            assert!(parsed.iter().all(|inst| inst.name != "mainnet"));
        }

        /// Named contract: GET / lists same-host paths and not esplora Hosts.
        #[test]
        fn splora_root_lists_same_host_paths_without_esplora_host() {
            let cfg = four_live_mainnet_off_cfg("splora.surmount.systems");
            let html = portal_index_html(&cfg);
            for href in PUBLIC_PATH_HREFS {
                assert!(
                    html.contains(&format!("href=\"{href}\"")),
                    "missing {href} in {html}"
                );
            }
            assert!(html.contains("href=\"/testnet4/\""));
            assert!(
                !html.contains("esplora.surmount.systems"),
                "path list must not name an esplora Host: {html}"
            );
            assert!(!html.contains("/block"));
            assert!(!html.contains("/tx/"));
            assert!(!html.contains("/address"));
            assert!(cfg.should_bypass_auth("splora.surmount.systems", "/testnet4/api/tx/x"));
            assert!(cfg.instance_for_host("splora.surmount.systems").is_none());
            let extra = cfg.extra_onion_and_redirect_hosts();
            assert_eq!(extra, vec!["splora.surmount.systems".to_string()]);
        }

        /// Named contract: portal GET / is 200 HTML, a same-host path list.
        #[test]
        fn portal_html_lists_same_host_paths_not_esplora_hosts() {
            let cfg = four_live_mainnet_off_cfg("splora.surmount.systems");
            let html = portal_index_html(&cfg);
            assert!(html.contains("href=\"/testnet4/\""));
            assert!(!html.contains("esplora.surmount.systems"), "{html}");
            assert!(cfg.should_bypass_auth("splora.surmount.systems", "/"));
            assert!(cfg.is_portal_host("Splora.surmount.systems"));
            assert!(cfg.instance_for_host("splora.surmount.systems").is_none());
            let extra = cfg.extra_onion_and_redirect_hosts();
            assert!(extra.iter().any(|h| h == "splora.surmount.systems"));
            assert!(
                extra
                    .iter()
                    .all(|h| !h.contains("esplora.surmount.systems")),
                "{extra:?}"
            );
        }

        #[test]
        fn portal_host_colliding_with_indexer_fails_closed() {
            let err = SploraProxyConfig::from_env_map(|k| match k {
                "SURMOUNT_SPLORA_PROXY" => Some("1".into()),
                "SURMOUNT_SPLORA_PORTAL_HOST" => Some("testnet3.esplora.surmount.systems".into()),
                "SURMOUNT_SPLORA_INSTANCES" => {
                    Some(r#"{"testnet3":{"hosts":["testnet3.esplora.surmount.systems"]}}"#.into())
                }
                _ => None,
            })
            .unwrap_err();
            assert!(err.contains("PORTAL_HOST"), "{err}");
            assert!(
                err.contains("fifth indexer") || err.contains("not also"),
                "{err}"
            );
        }

        #[test]
        fn portal_env_ignored_when_proxy_off() {
            let cfg = SploraProxyConfig::from_env_map(|k| match k {
                "SURMOUNT_SPLORA_PORTAL_HOST" => Some("splora.surmount.systems".into()),
                _ => None,
            })
            .unwrap();
            assert!(!cfg.enable);
            assert!(cfg.portal_host.is_none());
            assert!(!cfg.is_portal_host("splora.surmount.systems"));
        }

        /// Named contract: GET / on Host splora.surmount.systems is 200 HTML.
        #[tokio::test]
        async fn splora_portal_get_root_is_200_html_listing_live_hosts() {
            let cfg = four_live_mainnet_off_cfg("splora.surmount.systems");
            let state = test_state_for(cfg, false);
            let app = Router::new()
                .fallback(axum::routing::any(|| async { StatusCode::NOT_FOUND }))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    splora_proxy_middleware,
                ))
                .with_state(state);
            let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = lis.local_addr().unwrap();
            let serve = tokio::spawn(async move {
                axum::serve(lis, app).await.ok();
            });
            let client = reqwest::Client::new();
            let resp = client
                .get(format!("http://{addr}/"))
                .header("Host", "splora.surmount.systems")
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), reqwest::StatusCode::OK);
            let ctype = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            assert!(
                ctype.contains("text/html"),
                "portal GET / must be HTML, content-type={ctype}"
            );
            let body = resp.text().await.unwrap();
            for href in PUBLIC_PATH_HREFS {
                assert!(
                    body.contains(&format!("href=\"{href}\"")),
                    "missing {href}: {body}"
                );
            }
            assert!(
                !body.contains("esplora.surmount.systems"),
                "portal GET / must not name an esplora Host: {body}"
            );
            assert!(!body.contains("/block"));
            assert!(!body.contains("/tx/"));
            assert!(!body.contains("/address"));
            serve.abort();
        }

        fn test_state_for(cfg: SploraProxyConfig, https: bool) -> Arc<AppState> {
            use crate::config::{AppConfig, OnionSurface};
            use crate::tls::ListenMode;
            use std::net::SocketAddr;
            use surmount_management_ui::auth::AuthConfig;
            use surmount_management_ui::ban::{
                BanConfig, BanEnforcement, BanGuard, MemoryBanBackend,
            };

            let listen_mode = if https {
                ListenMode::Https(crate::tls::TlsPaths::new("/tmp/c.pem", "/tmp/k.pem"))
            } else {
                ListenMode::PlainHttp
            };
            let config = AppConfig {
                listen: SocketAddr::from(([127, 0, 0, 1], 8090)),
                http_redirect_listen: None,
                local_cleartext_listen: None,
                listen_mode,
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
                splora_proxy: cfg,
                http3: crate::tls::http3::Http3Config::default(),
                apex_public_root: None,
                static_vhosts: Default::default(),
                extra_mail_hostnames: Vec::new(),
                rate_limit_max_requests: 0,
                rate_limit_window: Duration::from_secs(60),
                rate_limit_max_keys: 1000,
                ban: BanConfig {
                    enforcement: BanEnforcement::Off,
                    backend_kind: surmount_management_ui::ban::BanBackendKind::Memory,
                    whitelist: vec![],
                    state_path: None,
                    nft_exec_enabled: false,
                    nft_bin: None,
                    nft_helper_sock: None,
                    nft_helper_bin: None,
                },
                auth: AuthConfig::off(),
                allow_directory_unauthenticated: false,
                allow_public_auth_off: false,
                console_accounts_path: crate::config::unused_console_accounts_path(),
                nwc_store_path: crate::config::unused_nwc_store_path(),
            };
            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            Arc::new(AppState {
                http,
                rate_limiter: None,
                ban: BanGuard::with_backend(
                    BanEnforcement::Off,
                    Box::new(MemoryBanBackend::new(vec![])),
                ),
                directory: crate::directory::directory_unavailable(),
                config,
            })
        }

        async fn serve_uds(path: PathBuf, app: Router) -> tokio::task::JoinHandle<()> {
            let _ = std::fs::remove_file(&path);
            let lis = tokio::net::UnixListener::bind(&path).unwrap();
            tokio::spawn(async move {
                axum::serve(lis, app).await.ok();
            })
        }

        /// Named contract: GET/POST to an indexer Host forward Host and
        /// X-Forwarded-Proto over HTTP/1.1 Unix. Must fail if proto is omitted.
        #[tokio::test]
        async fn splora_proxy_get_post_forward_host_and_x_forwarded_proto() {
            let sock = tmp_sock("splora-http");
            let seen = Arc::new(Mutex::new(Vec::<(String, HeaderMap, String)>::new()));
            let seen_c = seen.clone();
            let mock = Router::new()
                .route(
                    "/api/blocks/tip/hash",
                    get({
                        let seen_c = seen_c.clone();
                        move |req: AxumRequest| {
                            let seen_c = seen_c.clone();
                            async move {
                                seen_c.lock().unwrap().push((
                                    "GET".into(),
                                    req.headers().clone(),
                                    String::new(),
                                ));
                                "tip-hash"
                            }
                        }
                    }),
                )
                .route(
                    "/api/tx",
                    post({
                        let seen_c = seen_c.clone();
                        move |req: AxumRequest| {
                            let seen_c = seen_c.clone();
                            async move {
                                let headers = req.headers().clone();
                                let body =
                                    axum::body::to_bytes(req.into_body(), 1024).await.unwrap();
                                seen_c.lock().unwrap().push((
                                    "POST".into(),
                                    headers,
                                    String::from_utf8_lossy(&body).into_owned(),
                                ));
                                "txid"
                            }
                        }
                    }),
                );
            let mock_serve = serve_uds(sock.clone(), mock).await;
            tokio::time::sleep(Duration::from_millis(20)).await;

            let cfg = SploraProxyConfig {
                enable: true,
                instances: vec![SploraInstance {
                    name: "mainnet".into(),
                    hosts: vec!["esplora.example.test".into()],
                    http_socket: sock.clone(),
                }],
                queue_socket: PathBuf::from("/tmp/splora-queue-unused.sock"),
                queue_path: DEFAULT_QUEUE_PATH.into(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host: None,
            };
            let state = test_state_for(cfg, true);
            let app = Router::new()
                .fallback(axum::routing::any(|| async { StatusCode::NOT_FOUND }))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    splora_proxy_middleware,
                ))
                .with_state(state);

            let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = lis.local_addr().unwrap();
            let serve = tokio::spawn(async move {
                axum::serve(lis, app).await.ok();
            });

            let client = reqwest::Client::new();
            let get_resp = client
                .get(format!("http://{addr}/api/blocks/tip/hash"))
                .header("Host", "esplora.example.test")
                .send()
                .await
                .unwrap();
            assert_eq!(get_resp.status(), reqwest::StatusCode::OK);
            assert_eq!(get_resp.text().await.unwrap(), "tip-hash");

            let post_resp = client
                .post(format!("http://{addr}/api/tx"))
                .header("Host", "esplora.example.test")
                .body("rawtx")
                .send()
                .await
                .unwrap();
            assert_eq!(post_resp.status(), reqwest::StatusCode::OK);

            tokio::time::sleep(Duration::from_millis(30)).await;
            let hits = seen.lock().unwrap().clone();
            assert_eq!(hits.len(), 2, "expected GET and POST hits, got {hits:?}");
            for (method, headers, _) in &hits {
                let host = headers
                    .get(header::HOST)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                let proto = headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok());
                assert_eq!(host, "esplora.example.test", "Host missing on {method}");
                assert_eq!(
                    proto,
                    Some("https"),
                    "X-Forwarded-Proto must be forwarded (test fails if omitted); {method} headers={headers:?}"
                );
            }

            serve.abort();
            mock_serve.abort();
            let _ = std::fs::remove_file(&sock);
        }

        /// Named contract: queue POST goes only to queue.sock, never indexer.
        #[tokio::test]
        async fn splora_queue_post_only_hits_queue_socket() {
            let idx_sock = tmp_sock("splora-idx");
            let q_sock = tmp_sock("splora-queue");
            let idx_hits = Arc::new(Mutex::new(0u32));
            let q_hits = Arc::new(Mutex::new(Vec::<HeaderMap>::new()));
            let idx_c = idx_hits.clone();
            let q_c = q_hits.clone();

            let idx_app = Router::new().fallback(any({
                let idx_c = idx_c.clone();
                move || {
                    let idx_c = idx_c.clone();
                    async move {
                        *idx_c.lock().unwrap() += 1;
                        "indexer"
                    }
                }
            }));
            let q_app = Router::new().route(
                "/splora/queue",
                post({
                    let q_c = q_c.clone();
                    move |req: AxumRequest| {
                        let q_c = q_c.clone();
                        async move {
                            q_c.lock().unwrap().push(req.headers().clone());
                            "queued"
                        }
                    }
                }),
            );
            let idx_serve = serve_uds(idx_sock.clone(), idx_app).await;
            let q_serve = serve_uds(q_sock.clone(), q_app).await;
            tokio::time::sleep(Duration::from_millis(20)).await;

            let cfg = SploraProxyConfig {
                enable: true,
                instances: vec![SploraInstance {
                    name: "mainnet".into(),
                    hosts: vec!["esplora.example.test".into()],
                    http_socket: idx_sock.clone(),
                }],
                queue_socket: q_sock.clone(),
                queue_path: DEFAULT_QUEUE_PATH.into(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host: None,
            };
            let state = test_state_for(cfg, false);
            let app = Router::new()
                .fallback(axum::routing::any(|| async { StatusCode::NOT_FOUND }))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    splora_proxy_middleware,
                ))
                .with_state(state);
            let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = lis.local_addr().unwrap();
            let serve = tokio::spawn(async move {
                axum::serve(lis, app).await.ok();
            });

            let client = reqwest::Client::new();
            let resp = client
                .post(format!("http://{addr}/splora/queue"))
                .header("Host", "esplora.example.test")
                .header(header::CONTENT_TYPE, "application/json")
                .body(r#"{"npub":"npub1abc","email":"a@example.test"}"#)
                .send()
                .await
                .unwrap();
            assert_eq!(resp.status(), reqwest::StatusCode::OK);
            assert_eq!(resp.text().await.unwrap(), "queued");
            assert_eq!(
                *idx_hits.lock().unwrap(),
                0,
                "queue POST must not hit indexer units"
            );
            let qh = q_hits.lock().unwrap().clone();
            assert_eq!(qh.len(), 1);
            assert_eq!(
                qh[0].get("x-forwarded-proto").and_then(|v| v.to_str().ok()),
                Some("http"),
                "queue hop must set X-Forwarded-Proto; headers={:?}",
                qh[0]
            );

            let bad = client
                .get(format!("http://{addr}/splora/queue"))
                .header("Host", "esplora.example.test")
                .send()
                .await
                .unwrap();
            assert_eq!(bad.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);

            serve.abort();
            idx_serve.abort();
            q_serve.abort();
            let _ = std::fs::remove_file(&idx_sock);
            let _ = std::fs::remove_file(&q_sock);
        }

        /// Named contract: GET /api/v1/ws Upgrade headers reach the indexer HTTP socket.
        #[tokio::test]
        async fn splora_ws_upgrade_headers_reach_indexer_http_socket() {
            let sock = tmp_sock("splora-ws");
            let seen = Arc::new(Mutex::new(HeaderMap::new()));
            let seen_c = seen.clone();
            let mock = Router::new().route(
                "/api/v1/ws",
                any(move |req: AxumRequest| {
                    let seen_c = seen_c.clone();
                    async move {
                        *seen_c.lock().unwrap() = req.headers().clone();
                        (StatusCode::BAD_REQUEST, "not a real ws; headers captured")
                    }
                }),
            );
            let mock_serve = serve_uds(sock.clone(), mock).await;
            tokio::time::sleep(Duration::from_millis(20)).await;

            let cfg = SploraProxyConfig {
                enable: true,
                instances: vec![SploraInstance {
                    name: "testnet4".into(),
                    hosts: vec!["testnet4.esplora.example.test".into()],
                    http_socket: sock.clone(),
                }],
                queue_socket: PathBuf::from("/tmp/splora-queue-unused.sock"),
                queue_path: DEFAULT_QUEUE_PATH.into(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
                portal_host: None,
            };
            let state = test_state_for(cfg, true);
            let app = Router::new()
                .fallback(axum::routing::any(|| async { StatusCode::NOT_FOUND }))
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    splora_proxy_middleware,
                ))
                .with_state(state);
            let lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = lis.local_addr().unwrap();
            let serve = tokio::spawn(async move {
                axum::serve(lis, app).await.ok();
            });

            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let req = "GET /api/v1/ws HTTP/1.1\r\n\
             Host: testnet4.esplora.example.test\r\n\
             Connection: Upgrade\r\n\
             Upgrade: websocket\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             \r\n"
                .to_string();
            stream.write_all(req.as_bytes()).await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await;
            tokio::time::sleep(Duration::from_millis(50)).await;

            let h = seen.lock().unwrap().clone();
            assert!(
                h.get(header::UPGRADE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.eq_ignore_ascii_case("websocket"))
                    .unwrap_or(false),
                "indexer HTTP socket must see Upgrade: websocket; headers={h:?}"
            );
            assert!(
                h.get("sec-websocket-key").is_some(),
                "indexer HTTP socket must see Sec-WebSocket-Key; headers={h:?}"
            );
            assert_eq!(
                h.get("x-forwarded-proto").and_then(|v| v.to_str().ok()),
                Some("https"),
                "WS handshake must set X-Forwarded-Proto; headers={h:?}"
            );

            serve.abort();
            mock_serve.abort();
            let _ = std::fs::remove_file(&sock);
        }
    }
}
