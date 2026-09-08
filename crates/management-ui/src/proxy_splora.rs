//! Splora (Esplora-compatible indexer) reverse proxy over Unix sockets.
//!
//! Public Hosts map to `/run/splora/<instance>.http.sock` on the edge host.
//! The queue unit is a separate socket and path. NIP-98 stays in splora;
//! this edge does not add API keys. Not nginx. Not the Electrum newline
//! protocol socket (fail-closed if that socket is configured).

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
    filter_request_headers, filter_response_headers, is_websocket_upgrade, redact_secrets_in_text,
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

/// WebSocket path on each indexer HTTP socket.
pub const INDEXER_WS_PATH: &str = "/api/v1/ws";

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
}

impl Default for SploraProxyConfig {
    fn default() -> Self {
        Self {
            enable: false,
            instances: Vec::new(),
            queue_socket: PathBuf::from(DEFAULT_QUEUE_SOCKET),
            queue_path: DEFAULT_QUEUE_PATH.to_string(),
            body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
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

        Ok(Self {
            enable,
            instances,
            queue_socket,
            queue_path,
            body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
        })
    }

    /// True when this request is a splora Host or the queue path (skip Nostr).
    pub fn should_bypass_auth(&self, host: &str, path: &str) -> bool {
        if !self.enable {
            return false;
        }
        self.is_queue_path(path) || self.instance_for_host(host).is_some()
    }

    pub fn is_queue_path(&self, path: &str) -> bool {
        let p = self.queue_path.trim_end_matches('/');
        path == p || path == format!("{p}/")
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
pub fn parse_instances_json(raw: &str, socket_dir: &str) -> Result<Vec<SploraInstance>, String> {
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
            format!("SURMOUNT_SPLORA_INSTANCES[{name}] must be an object with hosts/socket: {e}")
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
        if hosts.is_empty() {
            return Err(format!(
                "SURMOUNT_SPLORA_INSTANCES[{name}] must list at least one Host"
            ));
        }
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

    if let Some(instance) = cfg.instance_for_host(&host).cloned() {
        return proxy_indexer(state, request, instance, host).await;
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
) -> Response {
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(str::to_string);
    let method = req.method().clone();
    let headers = req.headers().clone();
    let proto = forwarded_proto(state.config.listen_mode.is_https());
    let wants_ws = path == INDEXER_WS_PATH && is_websocket_upgrade(&headers);

    if wants_ws {
        return proxy_unix_websocket(
            instance.http_socket,
            req,
            &path,
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
        &path,
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

    let stream =
        match tokio::time::timeout(Duration::from_secs(10), UnixStream::connect(socket)).await {
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

    let unix =
        match tokio::time::timeout(Duration::from_secs(10), UnixStream::connect(&socket)).await {
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
            "SURMOUNT_SPLORA_ELECTRUM_SOCKET" => Some("/run/splora/mainnet.electrum.sock".into()),
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
    }

    fn test_state_for(cfg: SploraProxyConfig, https: bool) -> Arc<AppState> {
        use crate::config::{AppConfig, OnionSurface};
        use crate::tls::ListenMode;
        use std::net::SocketAddr;
        use surmount_management_ui::auth::AuthConfig;
        use surmount_management_ui::ban::{BanConfig, BanEnforcement, BanGuard, MemoryBanBackend};

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
            http3: crate::http3::Http3Config::default(),
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
                            let body = axum::body::to_bytes(req.into_body(), 1024).await.unwrap();
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
        let req = format!(
            "GET /api/v1/ws HTTP/1.1\r\n\
             Host: testnet4.esplora.example.test\r\n\
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
