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
            vaultwarden_proxy: VaultwardenProxyConfig {
                enable,
                public_prefix: "/vault".into(),
                upstream_base: upstream.trim_end_matches('/').to_string(),
                body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
            },
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
