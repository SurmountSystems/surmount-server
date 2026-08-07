//! Surmount management UI + Axum HTTPS edge foundation.
//!
//! Serves an Axum HTTP API and Leptos SSR multi-page management console
//! intended for https://services.surmount.systems.
//!
//! Edge foundation (this binary grows into clearnet HTTPS edge):
//! - Plain HTTP (dev / behind transitional nginx) or in-process rustls HTTPS
//! - HTTPS mode loads host PEMs and terminates TLS 1.3 in-process (no cleartext
//!   under the https label unless SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1, which
//!   is a deliberate cleartext override even when the acceptor is ready)
//! - Optional plain HTTP redirect-only listener (`SURMOUNT_REDIRECT_HTTP_TO_HTTPS`
//!   + `SURMOUNT_HTTP_REDIRECT_LISTEN`); no cleartext API on that port
//! - Optional loopback cleartext **full API** listener (`SURMOUNT_LOCAL_CLEARTEXT_LISTEN`)
//!   for local reverse-proxies (Arti HS lean path) when primary is https
//! - In-memory fixed-window rate limit; trusted client IP when peer is loopback
//! - Ban decision layer (whitelist last-used, optional enforce/dry-run; default off)
//!
//! Stalwart remains the mail engine. Auth: Nostr foundation (mode off by default).
//! ACME HTTP-01 on product :80 is parked (see RESIDUAL.md).

mod api;
mod client_ip;
mod config;
mod directory;
mod pages;
mod redirect;
mod tls;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{from_fn, from_fn_with_state, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use serde_json::{json, Value};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::client_ip::client_ip_for_rate_limit;
use crate::config::AppConfig;
use crate::directory::Directory;
use crate::redirect::{
    https_port_for_redirect, redirect_bind_decision, redirect_http_to_https, HttpToHttps,
    RedirectBindDecision, RedirectStatus,
};
use crate::tls::{
    https_startup_decision, load_rustls_server_config, HttpsStartupDecision, RUSTLS_ACCEPTOR_READY,
};
use surmount_management_ui::auth::{
    absolute_request_url, auth_error_kind, baseline_csp, cookie_value, csrf_clear_cookie_header,
    csrf_set_cookie_header, decode_event_b64_or_json, decode_session_cookie, encode_session_cookie,
    is_html_path, is_public_path, new_csp_nonce, new_csrf_token, now_unix,
    parse_event_from_auth_header, session_clear_cookie_header, session_set_cookie_header,
    verify_csrf_double_submit, verify_nip98_event, AuthError, ChallengeResponse, CspNonce,
    SessionPayload, CSRF_COOKIE_NAME, CSRF_HEADER_NAME, SESSION_COOKIE_NAME,
};
use surmount_management_ui::ban::{should_reject_banned, AccessDecision, BanGuard};
use surmount_management_ui::rate_limit::{FixedWindowRateLimiter, RateLimitDecision};

pub struct AppState {
    pub config: AppConfig,
    /// Shared HTTP client for future JMAP / Stalwart admin calls.
    pub http: reqwest::Client,
    pub rate_limiter: Option<FixedWindowRateLimiter<String>>,
    /// Ban/whitelist guard (enforcement default off; lean private).
    pub ban: BanGuard,
    /// Account directory strategy (default: honest empty / unavailable).
    pub directory: Box<dyn Directory>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let config = AppConfig::from_env().map_err(anyhow::Error::msg)?;
    config
        .validate_local_cleartext()
        .map_err(anyhow::Error::msg)
        .context("local cleartext listen config")?;

    // Fail loud when HTTPS is selected but PEM material is missing on host.
    if let Some(paths) = config.tls_paths() {
        paths
            .require_files_exist()
            .map_err(anyhow::Error::msg)
            .context("HTTPS listen mode requires deploy-secret TLS files on host")?;
    }

    let decision = https_startup_decision(
        &config.listen_mode,
        RUSTLS_ACCEPTOR_READY,
        config.https_allow_cleartext_escape,
    );
    match &decision {
        HttpsStartupDecision::FailClosed { reason } => {
            error!(%reason, "refusing to start");
            bail!("{reason}");
        }
        HttpsStartupDecision::DangerousCleartextEscape => {
            warn!(
                "SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE is set: serving cleartext while \
                 listen mode is https. Not for production. Cert path configured (key path omitted from logs)."
            );
        }
        HttpsStartupDecision::ServePlainHttp | HttpsStartupDecision::ServeHttps => {}
    }

    let listen = config.listen;
    let rate_limiter = config.rate_limiter();
    let ban = BanGuard::from_config(config.ban.clone()).map_err(anyhow::Error::msg)?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("build reqwest client")?;
    // Default: unavailable. Mock only when SURMOUNT_DIRECTORY=mock (hermetic).
    // Live Stalwart only when SURMOUNT_DIRECTORY=stalwart + host token (fail-closed).
    let directory = crate::directory::directory_from_env(http.clone(), config.auth.mode)
        .map_err(anyhow::Error::msg)
        .context("directory backend config (SURMOUNT_DIRECTORY)")?;
    let state = Arc::new(AppState {
        http,
        rate_limiter,
        ban,
        directory,
        config,
    });

    let app = build_router(state.clone());

    // Redirect-only plain HTTP (typically :80). Fail-closed on empty allowlist
    // when flag+listen are set; dual-run nginx mutex is Nix-eval only.
    let redirect_decision = redirect_bind_decision(
        state.config.redirect_http_to_https,
        state.config.http_redirect_listen,
        &state.config.redirect_allowed_hosts,
    );
    match &redirect_decision {
        RedirectBindDecision::FailClosed { reason } => {
            error!(%reason, "refusing to start (http redirect bind)");
            bail!("{reason}");
        }
        RedirectBindDecision::Skip { reason } => {
            info!(%reason, "http redirect listener not started");
        }
        RedirectBindDecision::Bind { addr } => {
            info!(%addr, "http redirect listener will bind (redirect-only, no API)");
        }
    }

    dispatch_listen_with_extras(
        decision,
        listen,
        app,
        state.config.tls_paths(),
        redirect_decision,
        state.config.local_cleartext_listen,
        state.clone(),
    )
    .await?;

    Ok(())
}

/// Bind plain HTTP redirect listener (shared by main and tests). Port 0 allowed.
async fn bind_redirect_listener(
    addr: SocketAddr,
) -> anyhow::Result<(tokio::net::TcpListener, SocketAddr)> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("bind http redirect listener {addr}"))?;
    let bound = listener
        .local_addr()
        .context("local_addr of http redirect listener")?;
    Ok((listener, bound))
}

/// Serve redirect-only app on an already-bound listener.
async fn serve_redirect_on_listener(
    listener: tokio::net::TcpListener,
    app: Router,
) -> anyhow::Result<()> {
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("serve http redirect")?;
    Ok(())
}

/// Primary edge + optional concurrent redirect-only and/or local cleartext API.
///
/// `FailClosed` always refuses here (defense in depth; main also bails early).
/// Local cleartext uses the **full** management router (not redirect-only).
async fn dispatch_listen_with_extras(
    decision: HttpsStartupDecision,
    listen: SocketAddr,
    app: Router,
    tls_paths: Option<&crate::tls::TlsPaths>,
    redirect: RedirectBindDecision,
    local_cleartext: Option<SocketAddr>,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    if let RedirectBindDecision::FailClosed { reason } = &redirect {
        bail!("{reason}");
    }

    // Bind extras first so port conflicts fail closed before the primary loop.
    let redirect_bound = match &redirect {
        RedirectBindDecision::Bind { addr } => {
            let (listener, bound) = bind_redirect_listener(*addr).await?;
            info!(%bound, mode = "http-redirect", "surmount-management-ui redirect listening");
            Some((listener, build_redirect_router(state.clone())))
        }
        RedirectBindDecision::Skip { .. } | RedirectBindDecision::FailClosed { .. } => None,
    };

    let local_cleartext_bound = if let Some(addr) = local_cleartext {
        // Re-check constraints (main already validated config; defense in depth).
        if !addr.ip().is_loopback() {
            bail!("local cleartext listen {addr} must be loopback-only");
        }
        if addr == listen {
            bail!("local cleartext listen must differ from primary listen");
        }
        if addr.port() == listen.port() {
            bail!(
                "local cleartext listen {addr} port must differ from primary {listen} \
                 (wildcard/loopback same-port collision)"
            );
        }
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("bind local cleartext API {addr}"))?;
        let bound = listener
            .local_addr()
            .context("local_addr of local cleartext listener")?;
        info!(%bound, mode = "http-local-cleartext", "surmount-management-ui local cleartext API listening");
        // Full API router (same as primary cleartext path), not redirect-only.
        Some((listener, app.clone()))
    } else {
        None
    };

    let primary = dispatch_listen(decision, listen, app, tls_paths);

    match (redirect_bound, local_cleartext_bound) {
        (None, None) => primary.await,
        (Some((r_lis, r_app)), None) => {
            let redirect_serve = serve_redirect_on_listener(r_lis, r_app);
            tokio::select! {
                r = primary => r.context("primary listen")?,
                r = redirect_serve => r?,
            }
            Ok(())
        }
        (None, Some((c_lis, c_app))) => {
            let cleartext_serve = serve_cleartext_on_listener(c_lis, c_app, "http-local-cleartext");
            tokio::select! {
                r = primary => r.context("primary listen")?,
                r = cleartext_serve => r?,
            }
            Ok(())
        }
        (Some((r_lis, r_app)), Some((c_lis, c_app))) => {
            let redirect_serve = serve_redirect_on_listener(r_lis, r_app);
            let cleartext_serve = serve_cleartext_on_listener(c_lis, c_app, "http-local-cleartext");
            tokio::select! {
                r = primary => r.context("primary listen")?,
                r = redirect_serve => r?,
                r = cleartext_serve => r?,
            }
            Ok(())
        }
    }
}

/// Compatibility alias used by older tests (redirect only, no local cleartext).
#[cfg(test)]
async fn dispatch_listen_with_redirect(
    decision: HttpsStartupDecision,
    listen: SocketAddr,
    app: Router,
    tls_paths: Option<&crate::tls::TlsPaths>,
    redirect: RedirectBindDecision,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    dispatch_listen_with_extras(decision, listen, app, tls_paths, redirect, None, state).await
}

/// Shared dispatch used by `main` and integration tests (ServeHttps vs cleartext).
async fn dispatch_listen(
    decision: HttpsStartupDecision,
    listen: SocketAddr,
    app: Router,
    tls_paths: Option<&crate::tls::TlsPaths>,
) -> anyhow::Result<()> {
    match decision {
        HttpsStartupDecision::ServeHttps => {
            let paths = tls_paths.context("HTTPS mode missing TLS paths after config parse")?;
            let tls = rustls_config_from_paths(paths)?;
            serve_https(listen, app, tls).await?;
        }
        HttpsStartupDecision::ServePlainHttp | HttpsStartupDecision::DangerousCleartextEscape => {
            let mode_label = match decision {
                HttpsStartupDecision::DangerousCleartextEscape => "https-cleartext-escape",
                _ => "http",
            };
            serve_cleartext(listen, app, mode_label).await?;
        }
        HttpsStartupDecision::FailClosed { reason } => {
            bail!("{reason}");
        }
    }
    Ok(())
}

fn rustls_config_from_paths(paths: &crate::tls::TlsPaths) -> anyhow::Result<RustlsConfig> {
    let server_config = load_rustls_server_config(paths)
        .map_err(anyhow::Error::msg)
        .context("load rustls ServerConfig from host PEMs")?;
    Ok(RustlsConfig::from_config(server_config))
}

async fn serve_cleartext(listen: SocketAddr, app: Router, mode_label: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("bind {listen}"))?;
    info!(%listen, mode = mode_label, "surmount-management-ui listening");
    serve_cleartext_on_listener(listener, app, mode_label).await
}

/// Serve full management API (cleartext) on an already-bound listener.
async fn serve_cleartext_on_listener(
    listener: tokio::net::TcpListener,
    app: Router,
    mode_label: &str,
) -> anyhow::Result<()> {
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .with_context(|| format!("serve {mode_label}"))?;
    Ok(())
}

/// Bind a nonblocking TCP listener for HTTPS (shared by main and tests).
fn bind_https_listener(listen: SocketAddr) -> anyhow::Result<(std::net::TcpListener, SocketAddr)> {
    let std_listener =
        std::net::TcpListener::bind(listen).with_context(|| format!("bind {listen}"))?;
    std_listener
        .set_nonblocking(true)
        .context("set nonblocking on TLS listener")?;
    let bound = std_listener
        .local_addr()
        .context("local_addr of TLS listener")?;
    Ok((std_listener, bound))
}

/// Serve HTTPS on an already-bound listener with the given axum-server handle.
/// Shared by `serve_https` (production) and integration tests.
async fn serve_https_on_listener(
    std_listener: std::net::TcpListener,
    app: Router,
    tls: RustlsConfig,
    handle: Handle<SocketAddr>,
) -> anyhow::Result<()> {
    axum_server::from_tcp_rustls(std_listener, tls)
        .context("from_tcp_rustls")?
        .handle(handle)
        .serve(app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .context("serve https")?;
    Ok(())
}

async fn serve_https(listen: SocketAddr, app: Router, tls: RustlsConfig) -> anyhow::Result<()> {
    let (std_listener, bound) = bind_https_listener(listen)?;
    info!(%bound, mode = "https", "surmount-management-ui listening");

    let handle = Handle::new();
    let shutdown_handle = handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_handle.graceful_shutdown(Some(Duration::from_secs(10)));
    });

    serve_https_on_listener(std_listener, app, tls, handle).await
}

fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(pages::home))
        .route("/domains", get(pages::domains_page))
        .route("/accounts", get(pages::accounts_page))
        .route("/system", get(pages::system_page))
        .route("/mail", get(pages::mail_page))
        .route("/login", get(pages::login_page))
        .route("/health", get(api::health))
        .route("/api/health", get(api::health))
        .route("/api/v1/domains", get(api::list_domains))
        .route(
            "/api/v1/accounts",
            get(api::list_accounts).post(accounts_create),
        )
        .route("/api/v1/accounts/{id}", patch(accounts_update))
        .route("/api/v1/system", get(api::system_status))
        .route("/api/v1/jmap", post(api::jmap_proxy_placeholder))
        .route("/api/v1/stalwart/status", get(api::stalwart_status))
        .route("/api/v1/auth/challenge", get(auth_challenge))
        .route("/api/v1/auth/session", post(auth_session_create))
        .route("/api/v1/auth/logout", post(auth_logout))
        .route("/api/v1/auth/me", get(auth_me))
        // Auth gate (inner) then rate-limit/ban then structured request log;
        // security headers outermost so 401/403/429 and happy paths all get
        // CSP / nosniff / frame denial. No tower TraceLayer (avoids dumping
        // headers/URIs that may embed onion or secret-shaped paths).
        .layer(from_fn_with_state(state.clone(), auth_middleware))
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(from_fn(request_log_middleware))
        .layer(from_fn(security_headers_middleware))
        .with_state(state)
}

/// Fields the request logger emits for path/query (pure; middleware wiring SoT).
///
/// Path is onion-redacted. Query string content is never returned (presence only)
/// so tokens/onion fragments in `?` do not enter log fields.
fn request_log_path_fields(uri: &axum::http::Uri) -> (String, bool) {
    use crate::config::redact_onion_in_text;
    let path = redact_onion_in_text(uri.path());
    let has_query = uri.query().map(|q| !q.is_empty()).unwrap_or(false);
    (path, has_query)
}

/// Structured request log: method, path (onion-redacted), status, latency.
/// Never logs Authorization, Cookie, bodies, tokens, nsec, or full onion labels.
async fn request_log_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let (path, has_query) = request_log_path_fields(request.uri());
    let start = Instant::now();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let latency_ms = start.elapsed().as_millis() as u64;
    info!(
        method = %method,
        path = %path,
        has_query,
        status,
        latency_ms,
        "http_request"
    );
    response
}

/// Baseline security headers + per-request CSP nonce (for `/login` inline script).
///
/// Injects [`CspNonce`] into request extensions before the handler runs so SSR
/// can emit matching `nonce=` on the login script tag.
async fn security_headers_middleware(mut request: Request, next: Next) -> Response {
    let nonce = match new_csp_nonce() {
        Ok(n) => n,
        Err(_) => {
            // Fail closed on entropy failure: still serve, but without a usable
            // script nonce (login inline script will not match CSP).
            "unavailable".to_string()
        }
    };
    request.extensions_mut().insert(CspNonce(nonce.clone()));
    let mut response = next.run(request).await;
    apply_security_headers(response.headers_mut(), &nonce);
    response
}

/// Apply baseline response hardening headers (idempotent insert).
fn apply_security_headers(headers: &mut HeaderMap, csp_nonce: &str) {
    let csp = baseline_csp(csp_nonce);
    if let Ok(v) = HeaderValue::from_str(&csp) {
        headers.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
}

/// Request scheme for absolute URL building (public_base_url wins when set).
fn request_scheme(state: &AppState) -> &'static str {
    if state.config.listen_mode.is_https() {
        "https"
    } else {
        "http"
    }
}

fn secure_cookies(state: &AppState) -> bool {
    state.config.listen_mode.is_https()
}

async fn auth_challenge(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Json<ChallengeResponse> {
    let host = header_str(&headers, "host").unwrap_or("127.0.0.1");
    let url = absolute_request_url(
        state.config.auth.public_base_url.as_deref(),
        request_scheme(&state),
        host,
        "/api/v1/auth/session",
    );
    Json(ChallengeResponse {
        method: "POST",
        url,
        created_at_window_secs: state.config.auth.nip98_max_skew_secs,
        kind: surmount_management_ui::auth::NIP98_KIND,
        auth_mode: state.config.auth.mode.as_str(),
        note:
            "Sign a kind 27235 event with u=url and method=POST; exchange at /api/v1/auth/session \
               (Authorization: Nostr <base64> or JSON body). nsec never sent to server. \
               Scaffold: rust-nostr verify + HMAC cookie session (Q-AUTH-1 residual).",
    })
}

async fn auth_session_create(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !state.config.auth.mode.is_nostr() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": "auth mode is off"})),
        )
            .into_response();
    }
    let secret = match state.config.auth.session_secret.as_ref() {
        Some(s) if !s.is_empty() => s.as_slice(),
        _ => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"ok": false, "error": "session secret not configured"})),
            )
                .into_response();
        }
    };

    let host = header_str(&headers, "host").unwrap_or("127.0.0.1");
    let expected_url = absolute_request_url(
        state.config.auth.public_base_url.as_deref(),
        request_scheme(&state),
        host,
        "/api/v1/auth/session",
    );

    let event = match resolve_session_event(&headers, &body) {
        Ok(e) => e,
        Err(e) => {
            // Session exchange fail (missing/malformed event) -> ban candidate once.
            log_auth_failure("session_exchange_parse", &e);
            let _ = signal_unauthorized(&state, addr, &headers);
            return auth_fail_json(e);
        }
    };

    let verified = match verify_nip98_event(
        &event,
        &expected_url,
        "POST",
        now_unix(),
        state.config.auth.nip98_max_skew_secs,
        &state.config.auth.allowlist,
    ) {
        Ok(v) => v,
        Err(e) => {
            // Invalid credentials (sig / allowlist / skew) -> ban candidate once.
            log_auth_failure("session_exchange_verify", &e);
            let _ = signal_unauthorized(&state, addr, &headers);
            return auth_fail_json(e);
        }
    };

    let exp = now_unix().saturating_add(state.config.auth.session_ttl_secs);
    let payload = SessionPayload {
        sub: verified.hex_pubkey.clone(),
        exp,
    };
    let cookie_val = match encode_session_cookie(&payload, secret) {
        Ok(c) => c,
        Err(e) => return auth_fail_json(e),
    };
    let csrf_token = match new_csrf_token() {
        Ok(t) => t,
        Err(e) => return auth_fail_json(e),
    };
    let secure = secure_cookies(&state);
    let set_session =
        session_set_cookie_header(&cookie_val, state.config.auth.session_ttl_secs, secure);
    let set_csrf = csrf_set_cookie_header(&csrf_token, state.config.auth.session_ttl_secs, secure);
    let npub = verified
        .npub()
        .unwrap_or_else(|_| verified.hex_pubkey.clone());
    let mut response = (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "npub": npub,
            "csrf": csrf_token,
        })),
    )
        .into_response();
    // Two Set-Cookie values: HttpOnly session + non-HttpOnly CSRF double-submit.
    if let Ok(v) = HeaderValue::from_str(&set_session) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&set_csrf) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    response
}

fn resolve_session_event(headers: &HeaderMap, body: &Bytes) -> Result<nostr::Event, AuthError> {
    if let Some(auth) = header_str(headers, "authorization") {
        if auth.to_ascii_lowercase().starts_with("nostr ") || auth.starts_with("Nostr ") {
            return parse_event_from_auth_header(auth);
        }
    }
    if body.is_empty() {
        return Err(AuthError::Malformed);
    }
    // JSON body: { "event": { ... } } | { "event": "<base64>" } | raw event object
    if let Ok(v) = serde_json::from_slice::<Value>(body) {
        if let Some(ev) = v.get("event") {
            if let Some(s) = ev.as_str() {
                return decode_event_b64_or_json(s);
            }
            return decode_event_b64_or_json(&ev.to_string());
        }
        return decode_event_b64_or_json(&v.to_string());
    }
    // Raw base64 string body
    let s = std::str::from_utf8(body).map_err(|_| AuthError::Malformed)?;
    decode_event_b64_or_json(s)
}

fn auth_fail_json(err: AuthError) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"ok": false, "error": err.to_string()})),
    )
        .into_response()
}

/// Structured auth-failure log: kind + surface only. Never logs cookies, nsec,
/// Authorization material, or event JSON.
fn log_auth_failure(surface: &'static str, err: &AuthError) {
    warn!(
        surface,
        kind = auth_error_kind(err),
        error = %err,
        "auth failure"
    );
}

/// POST logout: cookie-authenticated mutation protected by double-submit CSRF.
///
/// Requires matching `surmount_csrf` cookie and `X-CSRF-Token` header (or JSON
/// body field `csrf`). SameSite=Lax on the session cookie is already set;
/// double-submit is defense-in-depth for cookie-authenticated POSTs.
async fn auth_logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Logout is always cookie-auth; require CSRF unconditionally.
    if let Some(resp) = require_csrf_double_submit(&headers, &body, true) {
        return resp;
    }

    let secure = secure_cookies(&state);
    let clear_session = session_clear_cookie_header(secure);
    let clear_csrf = csrf_clear_cookie_header(secure);
    let mut response = (StatusCode::OK, Json(json!({"ok": true}))).into_response();
    if let Ok(v) = HeaderValue::from_str(&clear_session) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    if let Ok(v) = HeaderValue::from_str(&clear_csrf) {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    response
}

/// Account create mutation: auth coupling (nostr or lab escape) + CSRF when
/// session cookie present. Directory backend performs the create.
async fn accounts_create(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<api::CreateAccountBody>,
) -> Response {
    let body_bytes = Bytes::from(
        serde_json::to_vec(&json!({
            "csrf": body.csrf,
        }))
        .unwrap_or_default(),
    );
    if let Some(resp) = require_csrf_double_submit(&headers, &body_bytes, false) {
        return resp;
    }
    let (status, json) = api::create_account_via_directory(&state, body).await;
    (status, json).into_response()
}

/// Account update mutation (description patch): same auth/CSRF rules as create.
async fn accounts_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<api::UpdateAccountBody>,
) -> Response {
    let body_bytes = Bytes::from(
        serde_json::to_vec(&json!({
            "csrf": body.csrf,
        }))
        .unwrap_or_default(),
    );
    if let Some(resp) = require_csrf_double_submit(&headers, &body_bytes, false) {
        return resp;
    }
    let (status, json) = api::update_account_via_directory(&state, id, body).await;
    (status, json).into_response()
}

/// Optional JSON body `{ "csrf": "..." }` for double-submit when header is absent.
fn csrf_from_json_body(body: &Bytes) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let v: Value = serde_json::from_slice(body).ok()?;
    v.get("csrf")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
}

/// CSRF double-submit for cookie-authenticated mutations.
///
/// - `force=true` (logout): always require cookie+header/body match.
/// - `force=false` (account mutations): require only when session cookie is
///   present (NIP-98-only or lab auth-off paths skip CSRF).
///
/// Returns `Some(error_response)` when CSRF fails; `None` when allowed.
fn require_csrf_double_submit(headers: &HeaderMap, body: &Bytes, force: bool) -> Option<Response> {
    let cookie_hdr = header_str(headers, "cookie");
    let has_session = cookie_hdr
        .and_then(|c| cookie_value(c, SESSION_COOKIE_NAME))
        .is_some();
    if !force && !has_session {
        return None;
    }
    let csrf_cookie = cookie_hdr.and_then(|c| cookie_value(c, CSRF_COOKIE_NAME));
    let csrf_header = header_str(headers, CSRF_HEADER_NAME);
    let csrf_body = csrf_from_json_body(body);
    let submitted = csrf_header.or(csrf_body.as_deref());
    if let Err(e) = verify_csrf_double_submit(csrf_cookie, submitted) {
        return Some(
            (
                StatusCode::FORBIDDEN,
                Json(json!({"ok": false, "error": e.to_string()})),
            )
                .into_response(),
        );
    }
    None
}

async fn auth_me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    match identity_from_request(&state, &headers, &Method::GET, "/api/v1/auth/me") {
        Ok(id) => {
            let npub = id.npub().unwrap_or_else(|_| id.hex_pubkey.clone());
            (
                StatusCode::OK,
                Json(json!({"ok": true, "npub": npub, "hex": id.hex_pubkey})),
            )
                .into_response()
        }
        Err(e) => auth_fail_json(e),
    }
}

/// Resolve identity from session cookie or NIP-98 Authorization header.
fn identity_from_request(
    state: &AppState,
    headers: &HeaderMap,
    method: &Method,
    path_and_query: &str,
) -> Result<surmount_management_ui::auth::VerifiedIdentity, AuthError> {
    if !state.config.auth.mode.is_nostr() {
        return Err(AuthError::AuthOff);
    }
    let secret = state
        .config
        .auth
        .session_secret
        .as_ref()
        .filter(|s| !s.is_empty())
        .ok_or(AuthError::Config)?;

    // 1) Session cookie
    if let Some(cookie_hdr) = header_str(headers, "cookie") {
        if let Some(val) = cookie_value(cookie_hdr, SESSION_COOKIE_NAME) {
            match decode_session_cookie(val, secret, now_unix()) {
                Ok(payload) => {
                    return Ok(surmount_management_ui::auth::VerifiedIdentity {
                        hex_pubkey: payload.sub,
                    });
                }
                Err(AuthError::SessionExpired) | Err(AuthError::SessionInvalid) => {}
                Err(e) => return Err(e),
            }
        }
    }

    // 2) NIP-98 on this request
    let auth_hdr = header_str(headers, "authorization").ok_or(AuthError::Malformed)?;
    let event = parse_event_from_auth_header(auth_hdr)?;
    let host = header_str(headers, "host").unwrap_or("127.0.0.1");
    let url = absolute_request_url(
        state.config.auth.public_base_url.as_deref(),
        request_scheme(state),
        host,
        path_and_query,
    );
    verify_nip98_event(
        &event,
        &url,
        method.as_str(),
        now_unix(),
        state.config.auth.nip98_max_skew_secs,
        &state.config.auth.allowlist,
    )
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    if !state.config.auth.mode.is_nostr() {
        return next.run(request).await;
    }

    let path = request.uri().path().to_string();
    if is_public_path(&path) {
        return next.run(request).await;
    }

    let method = request.method().clone();
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| path.clone());
    let headers = request.headers().clone();

    match identity_from_request(&state, &headers, &method, &path_and_query) {
        Ok(_) => next.run(request).await,
        Err(err) => {
            // Only signal ban when client presented bad credentials (not mere absence).
            // Missing cookie / no Authorization does not ban. See SECURITY.md ban matrix.
            let presented = header_str(&headers, "authorization")
                .map(|h| h.to_ascii_lowercase().starts_with("nostr ") || h.starts_with("Nostr "))
                .unwrap_or(false);
            if presented
                && matches!(
                    err,
                    AuthError::BadSignature
                        | AuthError::NotAllowlisted
                        | AuthError::WrongKind
                        | AuthError::Skew
                        | AuthError::UrlMismatch
                        | AuthError::MethodMismatch
                )
            {
                log_auth_failure("protected_path_bad_nip98", &err);
                let _ = signal_unauthorized(&state, addr, &headers);
            } else if !matches!(err, AuthError::AuthOff) {
                // Gate miss (missing cookie, malformed Nostr, session invalid): log only.
                log_auth_failure("protected_path_gate", &err);
            }

            if is_html_path(&path) {
                let next_q = urlencoding_path(&path_and_query);
                let loc = format!("/login?next={next_q}");
                Redirect::temporary(&loc).into_response()
            } else {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({"ok": false, "error": err.to_string()})),
                )
                    .into_response()
            }
        }
    }
}

/// Minimal path query escape for `next=` (keep `/` and alphanumerics).
fn urlencoding_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Redirect-only router: no management API or HTML on the plain HTTP port.
/// Shares ban/rate middleware so enforce bans apply on :80 upgrade path too
/// (403/429 only; handler still never serves cleartext API bodies).
fn build_redirect_router(state: Arc<AppState>) -> Router {
    Router::new()
        .fallback(http_redirect_handler)
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        .with_state(state)
}

/// Plain HTTP upgrade handler (308 when Host allowlisted; 404 otherwise).
/// Never serves cleartext API bodies on the redirect listener.
async fn http_redirect_handler(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let host = header_str(request.headers(), "host").unwrap_or("");
    let path_and_query = request
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    let https_port = https_port_for_redirect(state.config.listen.port());
    match http_upgrade_location(
        true,
        host,
        path_and_query,
        https_port,
        &state.config.redirect_allowed_hosts,
    ) {
        Some(location) => redirect_response(&location, RedirectStatus::PermanentRedirect308),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn rate_limit_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let key = rate_limit_key(addr, headers);
    let client_ip: std::net::IpAddr = key.parse().unwrap_or_else(|_| addr.ip());

    // Short-circuit enforce bans before rate-limit counters (avoid max_keys
    // pressure under ban storms). Whitelist still checked via backend.
    if state.ban.enforcement.applies_rejects()
        && !state.ban.is_whitelisted(client_ip)
        && state.ban.is_banned(client_ip)
    {
        return (StatusCode::FORBIDDEN, "banned").into_response();
    }

    // Rate limit (or allow-all when disabled), then ban/whitelist decide.
    let rate = match state.rate_limiter.as_ref() {
        Some(limiter) => limiter.check(key.clone(), Instant::now()),
        None => RateLimitDecision::Allowed {
            remaining: u32::MAX,
        },
    };

    let decision = state.ban.decide(client_ip, rate);

    if should_reject_banned(state.ban.enforcement, &decision) {
        return (StatusCode::FORBIDDEN, "banned").into_response();
    }

    match decision {
        AccessDecision::RateLimited { retry_after } => {
            let secs = retry_after.as_secs().max(1);
            (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, secs.to_string())],
                "rate limit exceeded",
            )
                .into_response()
        }
        AccessDecision::Banned => {
            // Dry-run / off: do not 403; fall through (no whitelist touch).
            next.run(request).await
        }
        AccessDecision::Whitelisted => {
            let resp = next.run(request).await;
            // Last-used hygiene: touch on whitelist match (any status on this
            // path; does not require auth residual yet).
            state.ban.on_request_allowed(client_ip);
            resp
        }
        AccessDecision::Allowed => next.run(request).await,
        AccessDecision::BanCandidate { .. } => {
            // Not emitted by ordinary request path; treat as allow.
            next.run(request).await
        }
    }
}

fn rate_limit_key(addr: SocketAddr, headers: &HeaderMap) -> String {
    let x_real = header_str(headers, "x-real-ip");
    let xff = header_str(headers, "x-forwarded-for");
    client_ip_for_rate_limit(addr.ip(), x_real, xff)
}

/// Resolve client IP the same way as rate-limit / ban middleware, then run the
/// ban-layer unauthorized hook ([`BanGuard::signal_unauthorized`]).
///
/// Product surface for BanCandidate is the ban layer; this adapter is the
/// request-context entry for auth failures (bad NIP-98 / session exchange).
/// Missing cookie does not call this. Q-ACL-1 still open for full surface list.
/// Hermetic tests call it directly.
fn signal_unauthorized(
    state: &AppState,
    addr: SocketAddr,
    headers: &HeaderMap,
) -> Result<surmount_management_ui::ban::BanSignalOutcome, String> {
    let key = rate_limit_key(addr, headers);
    let ip = key
        .parse::<std::net::IpAddr>()
        .unwrap_or_else(|_| addr.ip());
    match state.ban.signal_unauthorized(ip) {
        Ok((decision, outcome)) => {
            info!(%ip, ?decision, ?outcome, "ban signal unauthorized");
            Ok(outcome)
        }
        Err(e) => {
            warn!(%ip, error = %e, "ban signal failed");
            Err(e)
        }
    }
}

/// Keep the request-context adapter linked in non-test builds so clippy
/// `-D dead-code` does not drop the Q-ACL-1 call-site surface. Returns a
/// stable symbol name only (no I/O).
#[used]
static SIGNAL_UNAUTHORIZED_HOOK: fn(
    &AppState,
    SocketAddr,
    &HeaderMap,
) -> Result<
    surmount_management_ui::ban::BanSignalOutcome,
    String,
> = signal_unauthorized;

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// Pure-function entry used by unit tests and redirect wiring.
fn http_upgrade_location(
    enabled: bool,
    host: &str,
    path_and_query: &str,
    https_port: Option<u16>,
    allowed_hosts: &[String],
) -> Option<String> {
    match redirect_http_to_https(enabled, host, path_and_query, https_port, allowed_hosts) {
        HttpToHttps::Redirect { location } => Some(location),
        HttpToHttps::PassThrough => None,
    }
}

/// Build a permanent HTTPS upgrade response (308 preferred for method preserve).
fn redirect_response(location: &str, status: RedirectStatus) -> Response {
    match status {
        RedirectStatus::MovedPermanently301 => Redirect::permanent(location).into_response(),
        RedirectStatus::PermanentRedirect308 => (
            StatusCode::PERMANENT_REDIRECT,
            [(header::LOCATION, location.to_string())],
        )
            .into_response(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received");
}

#[cfg(test)]
mod edge_wire_tests {
    use super::*;
    use crate::tls::ListenMode;
    use std::net::{IpAddr, Ipv4Addr};
    use surmount_management_ui::ban::BanEnforcement;

    fn allow(hosts: &[&str]) -> Vec<String> {
        hosts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn http_upgrade_location_wires_redirect_helper() {
        let hosts = allow(&["services.example.test"]);
        let loc = http_upgrade_location(true, "services.example.test", "/health", None, &hosts);
        assert_eq!(loc.as_deref(), Some("https://services.example.test/health"));
        assert_eq!(http_upgrade_location(false, "x", "/", None, &hosts), None);
    }

    #[test]
    fn redirect_response_uses_308_by_default_preference() {
        let resp = redirect_response(
            "https://services.example.test/",
            RedirectStatus::PermanentRedirect308,
        );
        assert_eq!(resp.status(), StatusCode::PERMANENT_REDIRECT);
    }

    #[test]
    fn rate_limit_key_uses_x_real_ip_on_loopback() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "198.51.100.20".parse().unwrap());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4444);
        assert_eq!(rate_limit_key(addr, &headers), "198.51.100.20");
    }

    /// Request-context hook: unauthorized signal uses same IP key as middleware;
    /// whitelist spared; enforce records ban for non-whitelist X-Real-IP.
    #[test]
    fn signal_unauthorized_hook_whitelist_and_enforce_record() {
        use surmount_management_ui::ban::{
            parse_whitelist_cidrs, BanReason, BanSignalOutcome, MemoryBanBackend,
        };

        let wl = parse_whitelist_cidrs("198.51.100.40/32").unwrap();
        let backend = MemoryBanBackend::new(wl);
        let ban = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(backend));
        let state = test_state_with_ban(0, BanEnforcement::Enforce, ban);

        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 443);
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "198.51.100.40".parse().unwrap());
        let out = signal_unauthorized(&state, loopback, &headers).unwrap();
        assert_eq!(out, BanSignalOutcome::SkippedWhitelisted);
        assert!(!state
            .ban
            .is_banned(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 40))));

        headers.insert("x-real-ip", "203.0.113.9".parse().unwrap());
        let out = signal_unauthorized(&state, loopback, &headers).unwrap();
        assert_eq!(
            out,
            BanSignalOutcome::Recorded {
                reason: BanReason::Unauthorized
            }
        );
        assert!(state
            .ban
            .is_banned(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9))));
    }

    #[test]
    fn rate_limit_key_ignores_xff_from_public_peer() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "198.51.100.20".parse().unwrap());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)), 443);
        assert_eq!(rate_limit_key(addr, &headers), "203.0.113.5");
    }

    fn test_state(max: u32) -> Arc<AppState> {
        test_state_with_ban(
            max,
            BanEnforcement::Off,
            BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
        )
    }

    fn test_state_with_ban(max: u32, enforcement: BanEnforcement, ban: BanGuard) -> Arc<AppState> {
        let config = AppConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 8090)),
            http_redirect_listen: None,
            local_cleartext_listen: None,
            listen_mode: ListenMode::PlainHttp,
            redirect_http_to_https: false,
            redirect_allowed_hosts: vec!["services.example.test".into()],
            https_allow_cleartext_escape: false,
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_url: None,
            rate_limit_max_requests: max,
            rate_limit_window: std::time::Duration::from_secs(60),
            rate_limit_max_keys: 1000,
            ban: surmount_management_ui::ban::BanConfig {
                enforcement,
                backend_kind: surmount_management_ui::ban::BanBackendKind::Memory,
                whitelist: vec![],
                state_path: None,
                nft_exec_enabled: false,
                nft_bin: None,
                nft_helper_sock: None,
                nft_helper_bin: None,
            },
            auth: surmount_management_ui::auth::AuthConfig::off(),
            allow_directory_unauthenticated: false,
        };
        let rate_limiter = config.rate_limiter();
        Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter,
            ban,
            directory: crate::directory::directory_unavailable(),
            config,
        })
    }

    fn test_state_with_directory(
        max: u32,
        directory: Box<dyn crate::directory::Directory>,
    ) -> Arc<AppState> {
        let config = AppConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 8090)),
            http_redirect_listen: None,
            local_cleartext_listen: None,
            listen_mode: ListenMode::PlainHttp,
            redirect_http_to_https: false,
            redirect_allowed_hosts: vec!["services.example.test".into()],
            https_allow_cleartext_escape: false,
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_url: None,
            rate_limit_max_requests: max,
            rate_limit_window: std::time::Duration::from_secs(60),
            rate_limit_max_keys: 1000,
            ban: surmount_management_ui::ban::BanConfig {
                enforcement: BanEnforcement::Off,
                backend_kind: surmount_management_ui::ban::BanBackendKind::Memory,
                whitelist: vec![],
                state_path: None,
                nft_exec_enabled: false,
                nft_bin: None,
                nft_helper_sock: None,
                nft_helper_bin: None,
            },
            auth: surmount_management_ui::auth::AuthConfig::off(),
            allow_directory_unauthenticated: false,
        };
        let rate_limiter = config.rate_limiter();
        Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter,
            ban: BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
            directory,
            config,
        })
    }

    fn test_state_nostr(allow_hex: &str, secret: &[u8]) -> Arc<AppState> {
        test_state_nostr_with_ban(
            allow_hex,
            secret,
            BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
        )
    }

    fn test_state_nostr_with_ban(allow_hex: &str, secret: &[u8], ban: BanGuard) -> Arc<AppState> {
        use std::collections::HashSet;
        use surmount_management_ui::auth::{AuthConfig, AuthMode};
        let mut allowlist = HashSet::new();
        allowlist.insert(allow_hex.to_ascii_lowercase());
        let base = test_state(0);
        let mut config = base.config.clone();
        config.auth = AuthConfig {
            mode: AuthMode::Nostr,
            allowlist,
            session_secret: Some(secret.to_vec()),
            session_ttl_secs: 3600,
            public_base_url: None,
            nip98_max_skew_secs: 300,
        };
        Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter: None,
            ban,
            directory: crate::directory::directory_unavailable(),
            config,
        })
    }

    /// Surface audit: 404/501 must **not** auto-ban. `signal_unauthorized` is
    /// wired only for auth session exchange failures and bad presented NIP-98
    /// (not missing cookie; not 404/501). Full Q-ACL surface list still open.
    #[tokio::test]
    async fn surface_audit_404_and_501_do_not_auto_ban() {
        use std::net::Ipv4Addr;
        use surmount_management_ui::ban::MemoryBanBackend;

        let backend = MemoryBanBackend::new(vec![]);
        let ban = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(backend));
        let state = test_state_with_ban(0, BanEnforcement::Enforce, ban);
        let app = build_router(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let base = format!("http://{addr}");

        // Known open stubs / health: 200 (not banned).
        let health = client.get(format!("{base}/health")).send().await.unwrap();
        assert_eq!(health.status(), reqwest::StatusCode::OK);

        // Unknown path: 404. Must not ban the peer.
        let missing = client
            .get(format!("{base}/no-such-route-surface-audit"))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), reqwest::StatusCode::NOT_FOUND);

        // JMAP placeholder: 501. Must not ban.
        let jmap = client
            .post(format!("{base}/api/v1/jmap"))
            .header("content-type", "application/json")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(jmap.status(), reqwest::StatusCode::NOT_IMPLEMENTED);

        // Peer is loopback connect; neither 404 nor 501 should record a ban.
        let peer = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        assert!(
            !state.ban.is_banned(peer),
            "404/501 must not call signal_unauthorized / auto-ban"
        );

        // Hook still linked for future Q-ACL-1 surfaces (not dead-code).
        let hook_addr = SIGNAL_UNAUTHORIZED_HOOK as usize;
        assert!(
            hook_addr != 0,
            "signal_unauthorized hook must remain linked"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: POST /api/v1/jmap is an honest 501 proxy boundary.
    ///
    /// Product is not a JMAP forwarder yet: status 501, stable error code,
    /// message does not claim success, no invented method results. Shared
    /// `build_router` path (not a reimplemented serve). Webmail HTML remains
    /// residual (no /webmail product route). Beyond-501 proxy is parked.
    #[tokio::test]
    async fn jmap_proxy_returns_honest_501_body() {
        let state = test_state(0);
        let app = build_router(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let base = format!("http://{addr}");

        let res = client
            .post(format!("{base}/api/v1/jmap"))
            .header("content-type", "application/json")
            .body(r#"{"using":["urn:ietf:params:jmap:core"],"methodCalls":[]}"#)
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            reqwest::StatusCode::NOT_IMPLEMENTED,
            "JMAP proxy boundary must be 501 until real forward lands"
        );
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(
            body.get("error").and_then(|v| v.as_str()),
            Some("jmap_proxy_not_implemented"),
            "stable honesty error code: {body}"
        );
        let message = body
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        assert!(
            message.contains("not implemented") || message.contains("proxy not"),
            "message must say not implemented: {body}"
        );
        assert!(
            body.get("methodResponses").is_none(),
            "must not invent JMAP methodResponses: {body}"
        );
        assert!(
            body.get("sessionState").is_none(),
            "must not invent JMAP sessionState: {body}"
        );
        assert_eq!(
            body.get("stalwart_url").and_then(|v| v.as_str()),
            Some(state.config.stalwart_url.as_str()),
            "pointer to real Stalwart HTTP config when set"
        );

        // No webmail product route yet (honest 404, not a fake UI).
        let webmail = client.get(format!("{base}/webmail")).send().await.unwrap();
        assert_eq!(
            webmail.status(),
            reqwest::StatusCode::NOT_FOUND,
            "/webmail must not pretend product UI exists"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: auth-failure ban matrix matches code (SECURITY.md).
    ///
    /// | Surface | Condition | signal_unauthorized |
    /// |---------|-----------|---------------------|
    /// | POST session | parse/verify fail | yes |
    /// | protected path | missing cookie | no |
    /// | protected path | Nostr + not allowlisted | yes |
    /// | 404 / 501 | any | no |
    #[tokio::test]
    async fn auth_failure_ban_matrix_signals_and_skips() {
        use std::net::Ipv4Addr;
        use surmount_management_ui::auth::{
            event_to_nostr_authorization, sign_nip98_event, HttpMethod,
        };
        use surmount_management_ui::ban::MemoryBanBackend;

        let secret = b"test-session-secret-ban-matrix!!!!";
        let allow_keys = nostr::Keys::generate();
        let allow_hex = allow_keys.public_key().to_hex();
        let ban = BanGuard::with_backend(
            BanEnforcement::Enforce,
            Box::new(MemoryBanBackend::new(vec![])),
        );
        let state = test_state_nostr_with_ban(&allow_hex, secret, ban);
        let app = build_router(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let base = format!("http://{addr}");

        // Distinct client keys via X-Real-IP (peer is loopback; trusted).
        let ip_session = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10));
        let ip_missing = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 11));
        let ip_404 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 12));
        let ip_501 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 13));
        let ip_bad_nip98 = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 14));

        // 1) Session exchange fail (malformed) -> signal_unauthorized once.
        let sess_fail = client
            .post(format!("{base}/api/v1/auth/session"))
            .header("x-real-ip", ip_session.to_string())
            .header(reqwest::header::AUTHORIZATION, "Nostr not-valid-event")
            .send()
            .await
            .unwrap();
        assert_eq!(sess_fail.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(
            state.ban.is_banned(ip_session),
            "session exchange fail must signal_unauthorized"
        );

        // 2) Missing cookie on protected API -> gate 401, no ban.
        let miss = client
            .get(format!("{base}/api/v1/domains"))
            .header("x-real-ip", ip_missing.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(miss.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(
            !state.ban.is_banned(ip_missing),
            "missing cookie must not ban"
        );

        // 3) Unknown HTML path under mode=nostr: gate redirects to /login (not
        // a ban). True 404 without ban is locked by surface_audit under auth off.
        let unknown_html = client
            .get(format!("{base}/no-such-ban-matrix-path"))
            .header("x-real-ip", ip_404.to_string())
            .send()
            .await
            .unwrap();
        assert_eq!(
            unknown_html.status(),
            reqwest::StatusCode::TEMPORARY_REDIRECT
        );
        assert!(
            !state.ban.is_banned(ip_404),
            "unknown HTML path gate must not ban"
        );

        // 4) JMAP under mode=nostr without credentials: gate 401, no ban.
        // (501 path is reached only when auth is off or after credentials;
        // surface_audit covers 501 no-ban under auth off.)
        let jmap_gate = client
            .post(format!("{base}/api/v1/jmap"))
            .header("x-real-ip", ip_501.to_string())
            .header("content-type", "application/json")
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(jmap_gate.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(
            !state.ban.is_banned(ip_501),
            "gated JMAP without credentials must not ban"
        );

        // 5) Presented NIP-98 not allowlisted on protected path -> ban.
        let evil = nostr::Keys::generate();
        let domains_url = format!("{base}/api/v1/domains");
        let event = sign_nip98_event(
            &evil,
            &domains_url,
            HttpMethod::GET,
            Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        )
        .unwrap();
        let authz = event_to_nostr_authorization(&event);
        let bad = client
            .get(&domains_url)
            .header("x-real-ip", ip_bad_nip98.to_string())
            .header(reqwest::header::AUTHORIZATION, authz)
            .header(reqwest::header::HOST, format!("{addr}"))
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), reqwest::StatusCode::UNAUTHORIZED);
        assert!(
            state.ban.is_banned(ip_bad_nip98),
            "not-allowlisted NIP-98 on protected path must signal_unauthorized"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: auth mode off keeps console open without cookie.
    #[tokio::test]
    async fn auth_mode_off_root_ok_without_cookie() {
        let state = test_state(0);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let res = client.get(format!("http://{addr}/")).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: mode=nostr without cookie gates HTML to login and API to 401; health open.
    #[tokio::test]
    async fn auth_mode_nostr_gates_without_cookie() {
        let secret = b"test-session-secret-for-http-gate!!";
        let k = nostr::Keys::generate();
        let hex = k.public_key().to_hex();
        let state = test_state_nostr(&hex, secret);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let base = format!("http://{addr}");

        let health = client.get(format!("{base}/health")).send().await.unwrap();
        assert_eq!(health.status(), reqwest::StatusCode::OK);

        let root = client.get(format!("{base}/")).send().await.unwrap();
        assert_eq!(root.status(), reqwest::StatusCode::TEMPORARY_REDIRECT);
        let loc = root
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            loc.starts_with("/login"),
            "expected redirect to /login, got {loc}"
        );

        let api = client
            .get(format!("{base}/api/v1/domains"))
            .send()
            .await
            .unwrap();
        assert_eq!(api.status(), reqwest::StatusCode::UNAUTHORIZED);

        let login = client.get(format!("{base}/login")).send().await.unwrap();
        assert_eq!(login.status(), reqwest::StatusCode::OK);

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: NIP-98 session exchange then me + HTML ok with cookie.
    #[tokio::test]
    async fn auth_session_from_nip98_then_me_and_root() {
        use surmount_management_ui::auth::{
            event_to_nostr_authorization, sign_nip98_event, HttpMethod,
        };

        let secret = b"test-session-secret-for-http-sess!!";
        let keys = nostr::Keys::generate();
        let hex = keys.public_key().to_hex();
        let state = test_state_nostr(&hex, secret);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let base = format!("http://{addr}");
        let session_url = format!("{base}/api/v1/auth/session");
        let event = sign_nip98_event(
            &keys,
            &session_url,
            HttpMethod::POST,
            Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        )
        .unwrap();
        let authz = event_to_nostr_authorization(&event);
        let sess = client
            .post(&session_url)
            .header(reqwest::header::AUTHORIZATION, authz)
            .send()
            .await
            .unwrap();
        assert_eq!(sess.status(), reqwest::StatusCode::OK);
        // Extract session cookie manually (no reqwest cookies feature).
        let set_cookie = sess
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .expect("Set-Cookie on session");
        let cookie_pair = set_cookie.split(';').next().unwrap().to_string();
        let _ = sess.text().await;

        let me = client
            .get(format!("{base}/api/v1/auth/me"))
            .header(reqwest::header::COOKIE, &cookie_pair)
            .send()
            .await
            .unwrap();
        assert_eq!(me.status(), reqwest::StatusCode::OK);
        let me_json: serde_json::Value = me.json().await.unwrap();
        assert_eq!(me_json["ok"], true);

        let root = client
            .get(format!("{base}/"))
            .header(reqwest::header::COOKIE, &cookie_pair)
            .send()
            .await
            .unwrap();
        assert_eq!(root.status(), reqwest::StatusCode::OK);

        serve.abort();
        let _ = serve.await;
    }

    #[tokio::test]
    async fn middleware_enforce_returns_403_for_banned_ip() {
        use std::net::Ipv4Addr;
        use surmount_management_ui::ban::{BanBackend, BanReason, MemoryBanBackend};

        let backend = MemoryBanBackend::new(vec![]);
        let banned = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        backend.ban(banned, BanReason::Unauthorized).unwrap();
        let ban = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(backend));
        let state = test_state_with_ban(0, BanEnforcement::Enforce, ban);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/health");
        let r = client.get(&url).send().await.unwrap();
        assert_eq!(r.status(), reqwest::StatusCode::FORBIDDEN);

        // Distinct X-Real-IP from loopback is not the banned peer key unless listed.
        let r2 = client
            .get(&url)
            .header("X-Real-IP", "198.51.100.40")
            .send()
            .await
            .unwrap();
        assert_eq!(r2.status(), reqwest::StatusCode::OK);

        serve.abort();
        let _ = serve.await;
    }

    #[tokio::test]
    async fn middleware_whitelist_bypasses_rate_limit_and_touches_last_used() {
        use std::net::Ipv4Addr;
        use surmount_management_ui::ban::{parse_whitelist_cidrs, MemoryBanBackend};

        let wl = parse_whitelist_cidrs("198.51.100.40/32").unwrap();
        let backend = MemoryBanBackend::new(wl);
        let ban = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(backend));
        // max=1 would 429 a normal client on second hit; whitelist must not.
        let state = test_state_with_ban(1, BanEnforcement::Enforce, ban);
        let backend_ref = state.ban.backend();
        let app = build_router(state.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/health");
        for _ in 0..3 {
            let r = client
                .get(&url)
                .header("X-Real-IP", "198.51.100.40")
                .send()
                .await
                .unwrap();
            assert_eq!(r.status(), reqwest::StatusCode::OK);
        }
        let home = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 40));
        assert!(
            backend_ref.whitelist_last_used(home).is_some(),
            "whitelist last-used must be touched on allowed requests"
        );

        serve.abort();
        let _ = serve.await;
    }

    #[tokio::test]
    async fn redirect_listener_enforce_bans_403_without_open_redirect() {
        use std::net::Ipv4Addr;
        use surmount_management_ui::ban::{BanBackend, BanReason, MemoryBanBackend};

        let backend = MemoryBanBackend::new(vec![]);
        let banned = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        backend.ban(banned, BanReason::Unauthorized).unwrap();
        let ban = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(backend));
        let state = test_state_with_ban(0, BanEnforcement::Enforce, ban);
        let app = build_redirect_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();

        // Banned peer: 403 before Host upgrade (no open redirect body).
        let banned_resp = client
            .get(format!("http://{addr}/health"))
            .header("Host", "services.example.test")
            .send()
            .await
            .unwrap();
        assert_eq!(banned_resp.status(), reqwest::StatusCode::FORBIDDEN);
        let banned_body = banned_resp.text().await.unwrap();
        assert!(
            !banned_body.to_ascii_lowercase().contains("location"),
            "banned must not emit redirect Location body: {banned_body}"
        );

        // Distinct X-Real-IP not banned: allowlisted host still 308 only.
        let ok = client
            .get(format!("http://{addr}/health"))
            .header("Host", "services.example.test")
            .header("X-Real-IP", "198.51.100.55")
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), reqwest::StatusCode::PERMANENT_REDIRECT);

        // Evil Host still 404 (open-redirect contract intact).
        let evil = client
            .get(format!("http://{addr}/api/health"))
            .header("Host", "evil.example")
            .header("X-Real-IP", "198.51.100.55")
            .send()
            .await
            .unwrap();
        assert_eq!(evil.status(), reqwest::StatusCode::NOT_FOUND);

        serve.abort();
        let _ = serve.await;
    }

    #[tokio::test]
    async fn middleware_returns_429_with_retry_after() {
        // Response shape unit-style.
        let limiter: FixedWindowRateLimiter<String> =
            FixedWindowRateLimiter::new(1, std::time::Duration::from_secs(30));
        let t0 = Instant::now();
        assert!(matches!(
            limiter.check("k".to_string(), t0),
            RateLimitDecision::Allowed { .. }
        ));
        let decision = limiter.check("k".to_string(), t0);
        assert!(matches!(decision, RateLimitDecision::Limited { .. }));
        if let RateLimitDecision::Limited { retry_after } = decision {
            let secs = retry_after.as_secs().max(1);
            let resp = (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, secs.to_string())],
                "rate limit exceeded",
            )
                .into_response();
            assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
            assert_eq!(resp.headers().get(header::RETRY_AFTER).unwrap(), "30");
        }

        // Full stack: bind ephemeral + client (uses real ConnectInfo).
        let state = test_state(1);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let url = format!("http://{addr}/health");
        let r1 = client.get(&url).send().await.unwrap();
        assert_eq!(r1.status(), reqwest::StatusCode::OK);
        let r2 = client.get(&url).send().await.unwrap();
        assert_eq!(r2.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
        assert!(r2.headers().get(reqwest::header::RETRY_AFTER).is_some());

        // Separate client IP via X-Real-IP from loopback peer gets own bucket.
        let r3 = client
            .get(&url)
            .header("X-Real-IP", "198.51.100.99")
            .send()
            .await
            .unwrap();
        assert_eq!(r3.status(), reqwest::StatusCode::OK);

        serve.abort();
        let _ = serve.await;
    }

    #[tokio::test]
    async fn https_serves_health_over_tls_with_temp_self_signed_pems() {
        use crate::tls::test_support::{write_temp_self_signed_pems, TempPemDir};
        use crate::tls::{https_startup_decision, ListenMode, RUSTLS_ACCEPTOR_READY};
        use std::os::unix::fs::PermissionsExt;

        // Drop cleans PEMs even on panic/assert fail.
        let dir = TempPemDir::new("surmount-https-itest");
        let paths = write_temp_self_signed_pems(dir.path(), "surmount-https-test");
        paths
            .require_files_exist()
            .expect("temp PEMs must pass checks");

        let key_mode = std::fs::metadata(&paths.key_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(key_mode & 0o077, 0, "key mode {key_mode:04o}");

        // Same decision path as main (escape off + product ready flag).
        let mode = ListenMode::Https(paths.clone());
        let decision = https_startup_decision(&mode, RUSTLS_ACCEPTOR_READY, false);
        assert_eq!(decision, HttpsStartupDecision::ServeHttps);

        // Same PEM → RustlsConfig path as dispatch_listen ServeHttps arm.
        let tls = rustls_config_from_paths(&paths).expect("rustls_config_from_paths");

        let (std_listener, addr) =
            bind_https_listener(SocketAddr::from(([127, 0, 0, 1], 0))).unwrap();

        let state = test_state(0); // rate limit off for this path
        let app = build_router(state);
        let handle = Handle::new();
        let serve_handle = handle.clone();
        // Same accept loop as production serve_https.
        let serve = tokio::spawn(async move {
            serve_https_on_listener(std_listener, app, tls, serve_handle)
                .await
                .ok();
        });

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let https_url = format!("https://{addr}/health");
        let http_url = format!("http://{addr}/health");

        // Retry until accept loop is ready (avoid fixed sleep flake).
        let deadline = Instant::now() + Duration::from_secs(2);
        let resp = loop {
            match client.get(&https_url).send().await {
                Ok(r) => break r,
                Err(e) => {
                    if Instant::now() >= deadline {
                        panic!("HTTPS GET {https_url} failed after retries: {e}");
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        };
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::OK,
            "body={}",
            resp.text().await.unwrap_or_default()
        );

        // Cleartext HTTP to the TLS port must not succeed (no false-green cleartext).
        let plain = client.get(&http_url).send().await;
        assert!(
            plain.is_err(),
            "plain HTTP must fail on TLS listener; got {:?}",
            plain.map(|r| r.status())
        );

        handle.graceful_shutdown(Some(Duration::from_secs(1)));
        let _ = tokio::time::timeout(Duration::from_secs(3), serve).await;
        // dir Drop removes temp PEMs
    }

    /// HTTPS primary + loopback cleartext full API via production
    /// `dispatch_listen_with_extras` (extras-first bind + select!). Redirect-only
    /// stays separate: cleartext local serves /health 200, not 308/404-only.
    #[tokio::test]
    async fn https_plus_local_cleartext_api_serves_health_on_both() {
        use crate::tls::test_support::{write_temp_self_signed_pems, TempPemDir};
        use crate::tls::{https_startup_decision, ListenMode, RUSTLS_ACCEPTOR_READY};

        let dir = TempPemDir::new("surmount-https-local-ct");
        let paths = write_temp_self_signed_pems(dir.path(), "surmount-https-local-ct");
        paths.require_files_exist().expect("temp PEMs");

        let mode = ListenMode::Https(paths.clone());
        let decision = https_startup_decision(&mode, RUSTLS_ACCEPTOR_READY, false);
        assert_eq!(decision, HttpsStartupDecision::ServeHttps);

        // Reserve ephemeral ports then drop so dispatch can bind (same as product path).
        async fn free_port() -> u16 {
            let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().port()
        }
        let https_addr = SocketAddr::from(([127, 0, 0, 1], free_port().await));
        let cleartext_addr = SocketAddr::from(([127, 0, 0, 1], free_port().await));
        assert_ne!(https_addr.port(), cleartext_addr.port());

        let state = test_state(0);
        let app = build_router(state.clone());
        let paths_for_dispatch = paths.clone();
        let serve = tokio::spawn(async move {
            dispatch_listen_with_extras(
                decision,
                https_addr,
                app,
                Some(&paths_for_dispatch),
                RedirectBindDecision::Skip {
                    reason: "test: no redirect listener",
                },
                Some(cleartext_addr),
                state,
            )
            .await
        });

        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let https_url = format!("https://{https_addr}/health");
        let clear_url = format!("http://{cleartext_addr}/health");

        let deadline = Instant::now() + Duration::from_secs(2);
        let https_resp = loop {
            match client.get(&https_url).send().await {
                Ok(r) => break r,
                Err(e) => {
                    if Instant::now() >= deadline {
                        panic!("HTTPS GET via dispatch_listen_with_extras failed: {e}");
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        };
        assert_eq!(https_resp.status(), reqwest::StatusCode::OK);

        let clear_resp = client.get(&clear_url).send().await.expect("cleartext GET");
        assert_eq!(
            clear_resp.status(),
            reqwest::StatusCode::OK,
            "local cleartext must serve full API /health via dispatch extras path"
        );
        let body = clear_resp.text().await.unwrap_or_default();
        assert!(
            body.contains("ok") || body.contains("status") || !body.is_empty(),
            "expected health body, got {body:?}"
        );

        // Plain HTTP to the TLS port must still fail (no false-green cleartext on primary).
        let plain_on_tls = client
            .get(format!("http://{https_addr}/health"))
            .send()
            .await;
        assert!(
            plain_on_tls.is_err(),
            "plain HTTP must fail on TLS listener; got {:?}",
            plain_on_tls.map(|r| r.status())
        );

        serve.abort();
        let _ = tokio::time::timeout(Duration::from_secs(3), serve).await;
    }

    #[test]
    fn dispatch_serve_https_requires_paths() {
        // Decision ServeHttps without paths must fail at dispatch (main wiring).
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(async {
            let app = build_router(test_state(0));
            dispatch_listen(
                HttpsStartupDecision::ServeHttps,
                SocketAddr::from(([127, 0, 0, 1], 0)),
                app,
                None,
            )
            .await
            .unwrap_err()
        });
        let msg = format!("{err:#}");
        assert!(
            msg.contains("TLS paths") || msg.contains("missing"),
            "{msg}"
        );
    }

    #[tokio::test]
    async fn redirect_listener_returns_308_and_no_api() {
        let state = test_state(0);
        let app = build_redirect_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let url = format!("http://{addr}/health");
        let resp = client
            .get(&url)
            .header("Host", "services.example.test")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::PERMANENT_REDIRECT);
        let loc = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        // Primary listen in test_state is 8090, so Location includes :8090.
        assert_eq!(loc, "https://services.example.test:8090/health");

        // Disallowed host: 404, not a cleartext API body.
        let bad = client
            .get(format!("http://{addr}/api/health"))
            .header("Host", "evil.example")
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), reqwest::StatusCode::NOT_FOUND);
        let body = bad.text().await.unwrap();
        assert!(
            !body.contains("ok"),
            "must not serve API on redirect port: {body}"
        );
        assert!(!body.to_ascii_lowercase().contains("domain"));

        // Allowlisted host on /api/* still only redirects (no JSON API).
        let api_redir = client
            .get(format!("http://{addr}/api/v1/domains"))
            .header("Host", "services.example.test")
            .send()
            .await
            .unwrap();
        assert_eq!(
            api_redir.status(),
            reqwest::StatusCode::PERMANENT_REDIRECT,
            "redirect port must not serve cleartext API"
        );

        serve.abort();
        let _ = serve.await;
    }

    #[test]
    fn redirect_bind_decision_wires_from_config_shape() {
        // Mirrors main: flag+listen+hosts => Bind; flag off => Skip.
        let addr = SocketAddr::from(([0, 0, 0, 0], 80));
        let hosts = allow(&["services.example.test"]);
        assert!(matches!(
            redirect_bind_decision(true, Some(addr), &hosts),
            RedirectBindDecision::Bind { .. }
        ));
        assert!(matches!(
            redirect_bind_decision(false, Some(addr), &hosts),
            RedirectBindDecision::Skip { .. }
        ));
        assert!(matches!(
            redirect_bind_decision(true, None, &hosts),
            RedirectBindDecision::Skip { .. }
        ));
        assert!(matches!(
            redirect_bind_decision(true, Some(addr), &[]),
            RedirectBindDecision::FailClosed { .. }
        ));
    }

    #[tokio::test]
    async fn dispatch_fail_closed_refuses_without_primary() {
        // Defense in depth: FailClosed must not fall through to primary-only.
        let state = test_state(0);
        let app = build_router(state.clone());
        let err = dispatch_listen_with_redirect(
            HttpsStartupDecision::ServePlainHttp,
            SocketAddr::from(([127, 0, 0, 1], 0)),
            app,
            None,
            RedirectBindDecision::FailClosed {
                reason: "redirect_http_to_https on but redirect allowlist is empty \
                         (open-redirect-safe refuse; set SURMOUNT_REDIRECT_ALLOWED_HOSTS \
                         or services/primary hostnames)",
            },
            state,
        )
        .await
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("allowlist") || msg.contains("refuse"), "{msg}");
    }

    #[tokio::test]
    async fn dispatch_dual_bind_primary_health_and_redirect_308() {
        // Bind-order + concurrent serve paths used by dispatch_listen_with_redirect.
        let state = test_state(0);
        let redirect_app = build_redirect_router(state.clone());
        let primary_app = build_router(state);

        let (redir_listener, redir_addr) =
            bind_redirect_listener(SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .unwrap();
        let primary_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let primary_addr = primary_listener.local_addr().unwrap();

        let redir_serve = tokio::spawn(async move {
            serve_redirect_on_listener(redir_listener, redirect_app)
                .await
                .ok();
        });
        let primary_serve = tokio::spawn(async move {
            axum::serve(
                primary_listener,
                primary_app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();

        let health = client
            .get(format!("http://{primary_addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(health.status(), reqwest::StatusCode::OK);

        let redir = client
            .get(format!("http://{redir_addr}/health"))
            .header("Host", "services.example.test")
            .send()
            .await
            .unwrap();
        assert_eq!(redir.status(), reqwest::StatusCode::PERMANENT_REDIRECT);
        let loc = redir
            .headers()
            .get(reqwest::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(loc, "https://services.example.test:8090/health");

        // Occupied redirect port: bind fails closed before primary would matter.
        let hold_err = bind_redirect_listener(redir_addr).await;
        assert!(
            hold_err.is_err(),
            "second bind on redirect addr must fail closed"
        );

        redir_serve.abort();
        primary_serve.abort();
        let _ = redir_serve.await;
        let _ = primary_serve.await;
    }

    /// Named contract: admin shell is Leptos SSR (not hand-rolled HTML only)
    /// and health still works on the shared Axum middleware stack.
    #[tokio::test]
    async fn admin_shell_ssr_and_health_via_shared_router() {
        let state = test_state(0);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();

        let health = client
            .get(format!("http://{addr}/health"))
            .send()
            .await
            .unwrap();
        assert_eq!(health.status(), reqwest::StatusCode::OK);
        let health_body = health.text().await.unwrap();
        assert!(
            health_body.contains("\"status\":\"ok\"") || health_body.contains("\"status\": \"ok\""),
            "health JSON body: {health_body}"
        );

        let shell = client.get(format!("http://{addr}/")).send().await.unwrap();
        assert_eq!(shell.status(), reqwest::StatusCode::OK);
        let ct = shell
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            ct.contains("text/html"),
            "admin shell Content-Type should be text/html, got {ct:?}"
        );
        let html = shell.text().await.unwrap();
        assert!(
            html.contains(r#"data-surmount-ssr="leptos""#),
            "admin shell must be Leptos SSR (marker data-surmount-ssr=leptos); got snippet: {}",
            html.chars().take(200).collect::<String>()
        );
        assert!(
            html.contains("Management console"),
            "admin shell should include Management console copy"
        );
        assert!(
            html.contains("example.test"),
            "admin shell should include primary_domain from state"
        );
        assert!(
            html.contains(r#"href="/domains""#),
            "overview must expose SSR nav to /domains"
        );
        assert!(
            !html.to_ascii_lowercase().contains("skeleton"),
            "overview must not ship skeleton branding"
        );

        // Section routes are real HTML pages on the same router.
        for path in ["/domains", "/accounts", "/system", "/mail"] {
            let resp = client
                .get(format!("http://{addr}{path}"))
                .send()
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                reqwest::StatusCode::OK,
                "{path} should be 200"
            );
            let body = resp.text().await.unwrap();
            assert!(
                body.contains(r#"data-surmount-ssr="leptos""#),
                "{path} must be Leptos SSR"
            );
        }

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: accounts JSON does not invent fake live Stalwart accounts.
    #[tokio::test]
    async fn accounts_api_honest_empty_without_directory() {
        let state = test_state(0);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/api/v1/accounts"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert!(
            !body.contains("admin@"),
            "accounts API must not invent admin@; body={body}"
        );
        assert!(
            body.contains("\"source\":\"unavailable\"")
                || body.contains("\"source\": \"unavailable\""),
            "accounts API source unavailable; body={body}"
        );
        assert!(
            body.contains("\"accounts\":[]") || body.contains("\"accounts\": []"),
            "accounts list empty without directory; body={body}"
        );

        let accounts_html = client
            .get(format!("http://{addr}/accounts"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            !accounts_html.contains("admin@"),
            "accounts page must not invent admin@"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: HTTP list_accounts respects injected directory (mock).
    #[tokio::test]
    async fn accounts_api_respects_mock_directory() {
        let state = test_state_with_directory(0, crate::directory::directory_mock());
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/api/v1/accounts"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert!(
            body.contains("\"source\":\"mock\"") || body.contains("\"source\": \"mock\""),
            "mock directory must label source mock; body={body}"
        );
        assert!(
            !body.contains("\"source\":\"stalwart\"") && !body.contains("\"source\": \"stalwart\""),
            "mock must not claim stalwart; body={body}"
        );
        assert!(
            body.contains("fixture-operator@mock.surmount.test")
                || body.contains("mock.surmount.test"),
            "mock fixture principals expected; body={body}"
        );
        assert!(
            !body.contains("\"accounts\":[]") && !body.contains("\"accounts\": []"),
            "mock directory must not return empty accounts; body={body}"
        );

        let accounts_html = client
            .get(format!("http://{addr}/accounts"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            accounts_html.contains("source: mock") || accounts_html.contains("mock"),
            "accounts SSR should reflect mock source; html snippet"
        );
        assert!(
            accounts_html.contains("fixture-operator@mock.surmount.test")
                || accounts_html.contains("mock.surmount.test"),
            "accounts SSR should list mock fixture principals"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: mode=nostr gates accounts API even when directory is injected.
    #[tokio::test]
    async fn auth_mode_nostr_gates_accounts_api_with_directory_injected() {
        let secret = b"test-session-secret-for-dir-gate!!";
        let k = nostr::Keys::generate();
        let hex = k.public_key().to_hex();
        let base = test_state_nostr(&hex, secret);
        let state = Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter: None,
            ban: BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
            directory: crate::directory::directory_mock(),
            config: base.config.clone(),
        });
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/api/v1/accounts"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            reqwest::StatusCode::UNAUTHORIZED,
            "unauthenticated accounts must be 401 under nostr even with mock directory"
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: live Stalwart directory over HTTP lists principals via
    /// injected client (wire mock; no live Stalwart required).
    #[tokio::test]
    async fn accounts_api_respects_live_stalwart_directory() {
        use axum::body::Body;
        use axum::http::{header, StatusCode};
        use axum::response::Response;
        use axum::routing::post;
        use axum::Router;
        use serde_json::json;

        let mock = Router::new().route(
            "/api",
            post(|| async {
                let body = json!({
                    "methodResponses": [
                        ["x:Account/query", {"ids": ["api-1"], "total": 1}, "q1"],
                        ["x:Account/get", {
                            "list": [{
                                "id": "api-1",
                                "name": "ops",
                                "emailAddress": "ops@live.example.test",
                                "@type": "User"
                            }],
                            "notFound": []
                        }, "g1"]
                    ]
                });
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap()
            }),
        );
        let mock_lis = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mock_addr = mock_lis.local_addr().unwrap();
        let mock_serve = tokio::spawn(async move {
            axum::serve(mock_lis, mock).await.ok();
        });

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir =
            crate::directory::directory_stalwart(http, format!("http://{mock_addr}"), "e2e-token");
        let state = test_state_with_directory(0, dir);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/api/v1/accounts"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body = resp.text().await.unwrap();
        assert!(
            body.contains("\"source\":\"stalwart\"") || body.contains("\"source\": \"stalwart\""),
            "live directory must label source stalwart; body={body}"
        );
        assert!(
            body.contains("ops@live.example.test"),
            "live principal expected; body={body}"
        );
        assert!(
            !body.contains("admin@"),
            "must not invent admin@; body={body}"
        );

        serve.abort();
        mock_serve.abort();
        let _ = serve.await;
        let _ = mock_serve.await;
    }

    /// Named contract: live directory with unreachable Stalwart stays empty + stalwart source.
    #[tokio::test]
    async fn accounts_api_live_directory_unreachable_is_empty_not_invented() {
        let dead = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let dead_addr = dead.local_addr().unwrap();
        drop(dead);

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let dir =
            crate::directory::directory_stalwart(http, format!("http://{dead_addr}"), "token");
        let state = test_state_with_directory(0, dir);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });

        let client = reqwest::Client::new();
        let body = client
            .get(format!("http://{addr}/api/v1/accounts"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(
            body.contains("\"source\":\"stalwart\"") || body.contains("\"source\": \"stalwart\""),
            "configured live path keeps source stalwart; body={body}"
        );
        assert!(
            body.contains("\"accounts\":[]") || body.contains("\"accounts\": []"),
            "unreachable live directory must not invent rows; body={body}"
        );
        assert!(
            !body.contains("admin@"),
            "must not invent admin@; body={body}"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: baseline security headers on `/` and `/health`.
    #[tokio::test]
    async fn security_headers_on_root_and_health() {
        let state = test_state(0);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{addr}");

        for path in ["/", "/health"] {
            let res = client.get(format!("{base}{path}")).send().await.unwrap();
            assert_eq!(res.status(), reqwest::StatusCode::OK, "path {path}");
            let h = res.headers();
            let csp = h
                .get(reqwest::header::CONTENT_SECURITY_POLICY)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            assert!(
                csp.contains("default-src 'self'"),
                "{path} CSP missing default-src self: {csp}"
            );
            assert!(
                csp.contains("frame-ancestors 'none'"),
                "{path} CSP missing frame-ancestors: {csp}"
            );
            assert!(
                !csp.contains("cdn.") && !csp.contains("https://"),
                "{path} CSP must not allow third-party hosts: {csp}"
            );
            assert_eq!(
                h.get(reqwest::header::X_CONTENT_TYPE_OPTIONS)
                    .and_then(|v| v.to_str().ok()),
                Some("nosniff"),
                "{path} X-Content-Type-Options"
            );
            assert_eq!(
                h.get(reqwest::header::REFERRER_POLICY)
                    .and_then(|v| v.to_str().ok()),
                Some("no-referrer"),
                "{path} Referrer-Policy"
            );
            assert_eq!(
                h.get(reqwest::header::X_FRAME_OPTIONS)
                    .and_then(|v| v.to_str().ok()),
                Some("DENY"),
                "{path} X-Frame-Options"
            );
        }

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: `/login` HTML script nonce matches CSP (NIP-07 inline).
    #[tokio::test]
    async fn login_page_script_nonce_matches_csp() {
        let secret = b"test-session-secret-for-csp-nonce!!";
        let k = nostr::Keys::generate();
        let hex = k.public_key().to_hex();
        let state = test_state_nostr(&hex, secret);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let res = client
            .get(format!("http://{addr}/login"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        let csp = res
            .headers()
            .get(reqwest::header::CONTENT_SECURITY_POLICY)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let body = res.text().await.unwrap();
        // Extract nonce-… from CSP script-src.
        let marker = "'nonce-";
        let start = csp
            .find(marker)
            .expect("CSP should include script-src nonce");
        let rest = &csp[start + marker.len()..];
        let end = rest.find('\'').expect("nonce closing quote");
        let nonce = &rest[..end];
        assert!(!nonce.is_empty());
        assert!(
            body.contains(&format!("nonce=\"{nonce}\""))
                || body.contains(&format!("nonce='{nonce}'"))
                || body.contains(&format!("nonce={nonce}")),
            "login HTML must carry matching script nonce={nonce}; csp={csp}; body snippet={}",
            body.chars().take(800).collect::<String>()
        );
        assert!(
            !body.to_ascii_lowercase().contains("onclick="),
            "login must not use inline onclick (breaks nonce-only CSP)"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Helper: NIP-98 session exchange; returns (session_pair, csrf_token, cookie_header_blob).
    async fn session_login_cookies(
        client: &reqwest::Client,
        base: &str,
        keys: &nostr::Keys,
    ) -> (String, String, String) {
        use surmount_management_ui::auth::{
            event_to_nostr_authorization, sign_nip98_event, HttpMethod,
        };

        let session_url = format!("{base}/api/v1/auth/session");
        let event = sign_nip98_event(
            keys,
            &session_url,
            HttpMethod::POST,
            Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
        )
        .unwrap();
        let authz = event_to_nostr_authorization(&event);
        let sess = client
            .post(&session_url)
            .header(reqwest::header::AUTHORIZATION, authz)
            .send()
            .await
            .unwrap();
        assert_eq!(sess.status(), reqwest::StatusCode::OK);
        let mut session_pair = None;
        let mut csrf_pair = None;
        for val in sess.headers().get_all(reqwest::header::SET_COOKIE) {
            let s = val.to_str().unwrap_or("");
            let pair = s.split(';').next().unwrap_or("").to_string();
            if pair.starts_with("surmount_session=") {
                session_pair = Some(pair);
            } else if pair.starts_with("surmount_csrf=") {
                csrf_pair = Some(pair);
            }
        }
        let session_pair = session_pair.expect("session Set-Cookie");
        let csrf_pair = csrf_pair.expect("csrf Set-Cookie");
        let body: serde_json::Value = sess.json().await.unwrap();
        let csrf_token = body["csrf"]
            .as_str()
            .expect("session JSON includes csrf")
            .to_string();
        let cookie_blob = format!("{session_pair}; {csrf_pair}");
        (session_pair, csrf_token, cookie_blob)
    }

    /// Named contract: forged logout without CSRF token fails (403).
    #[tokio::test]
    async fn csrf_logout_without_token_forbidden() {
        let secret = b"test-session-secret-for-csrf-out!!";
        let keys = nostr::Keys::generate();
        let hex = keys.public_key().to_hex();
        let state = test_state_nostr(&hex, secret);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{addr}");
        let (session_pair, _csrf, _blob) = session_login_cookies(&client, &base, &keys).await;

        // Session cookie only, no X-CSRF-Token and no csrf cookie → 403.
        let forged = client
            .post(format!("{base}/api/v1/auth/logout"))
            .header(reqwest::header::COOKIE, &session_pair)
            .send()
            .await
            .unwrap();
        assert_eq!(forged.status(), reqwest::StatusCode::FORBIDDEN);
        let err: serde_json::Value = forged.json().await.unwrap();
        assert_eq!(err["ok"], false);
        assert!(
            err["error"]
                .as_str()
                .unwrap_or("")
                .to_ascii_lowercase()
                .contains("csrf"),
            "error should mention CSRF: {err}"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: legitimate logout with double-submit CSRF succeeds.
    #[tokio::test]
    async fn csrf_logout_with_double_submit_succeeds() {
        let secret = b"test-session-secret-for-csrf-ok!!";
        let keys = nostr::Keys::generate();
        let hex = keys.public_key().to_hex();
        let state = test_state_nostr(&hex, secret);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{addr}");
        let (_session_pair, csrf_token, cookie_blob) =
            session_login_cookies(&client, &base, &keys).await;

        let ok = client
            .post(format!("{base}/api/v1/auth/logout"))
            .header(reqwest::header::COOKIE, &cookie_blob)
            .header(CSRF_HEADER_NAME, &csrf_token)
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = ok.json().await.unwrap();
        assert_eq!(body["ok"], true);

        // Wrong token still fails.
        let bad = client
            .post(format!("{base}/api/v1/auth/logout"))
            .header(reqwest::header::COOKIE, &cookie_blob)
            .header(CSRF_HEADER_NAME, "not-the-token")
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), reqwest::StatusCode::FORBIDDEN);

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: account create with auth_mode=off fails closed (no lab escape).
    #[tokio::test]
    async fn account_create_auth_off_fail_closed_without_lab_escape() {
        let state = test_state_with_directory(0, crate::directory::directory_mock());
        assert!(!state.config.allow_directory_unauthenticated);
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{addr}");
        let resp = client
            .post(format!("{base}/api/v1/accounts"))
            .json(&serde_json::json!({
                "name": "alice",
                "domain_id": "d1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::FORBIDDEN);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["ok"], false);
        let err = body["error"].as_str().unwrap_or("").to_ascii_lowercase();
        assert!(
            err.contains("nostr") || err.contains("fail-closed") || err.contains("auth"),
            "expected auth fail-closed note: {body}"
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: lab escape + mock directory allows create without session.
    #[tokio::test]
    async fn account_create_lab_escape_mock_succeeds() {
        let mut state = test_state_with_directory(0, crate::directory::directory_mock());
        // Rebuild with lab escape flag.
        let mut config = state.config.clone();
        config.allow_directory_unauthenticated = true;
        let dir = crate::directory::directory_mock();
        state = Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter: None,
            ban: BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
            directory: dir,
            config,
        });
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{addr}");
        let resp = client
            .post(format!("{base}/api/v1/accounts"))
            .json(&serde_json::json!({
                "name": "labuser",
                "domain_id": "mock-domain",
                "description": "lab"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["source"], "mock");
        assert_eq!(body["account"]["id"], "mock-labuser");
        assert!(
            body["account"]["address"]
                .as_str()
                .unwrap_or("")
                .contains("mock"),
            "mock address label: {body}"
        );
        // List reflects create.
        let list: serde_json::Value = client
            .get(format!("{base}/api/v1/accounts"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(
            list["accounts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "mock-labuser"),
            "list should include created: {list}"
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: cookie session create requires CSRF double-submit.
    #[tokio::test]
    async fn account_create_cookie_auth_requires_csrf() {
        let secret = b"test-session-secret-for-acct-csrf!";
        let keys = nostr::Keys::generate();
        let hex = keys.public_key().to_hex();
        let base_state = test_state_nostr(&hex, secret);
        // Keep nostr; mock directory for hermetic mutation.
        let state = Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter: None,
            ban: BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
            directory: crate::directory::directory_mock(),
            config: base_state.config.clone(),
        });
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let base = format!("http://{addr}");
        let (session_pair, csrf_token, cookie_blob) =
            session_login_cookies(&client, &base, &keys).await;

        // Session cookie only, no CSRF -> 403.
        let forged = client
            .post(format!("{base}/api/v1/accounts"))
            .header(reqwest::header::COOKIE, &session_pair)
            .json(&serde_json::json!({
                "name": "forged",
                "domain_id": "d1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(forged.status(), reqwest::StatusCode::FORBIDDEN);

        // Double-submit succeeds.
        let ok = client
            .post(format!("{base}/api/v1/accounts"))
            .header(reqwest::header::COOKIE, &cookie_blob)
            .header(CSRF_HEADER_NAME, &csrf_token)
            .json(&serde_json::json!({
                "name": "gooduser",
                "domain_id": "d1"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), reqwest::StatusCode::OK);
        let body: serde_json::Value = ok.json().await.unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["source"], "mock");

        // PATCH update with CSRF.
        let id = body["account"]["id"].as_str().unwrap();
        let patched = client
            .patch(format!("{base}/api/v1/accounts/{id}"))
            .header(reqwest::header::COOKIE, &cookie_blob)
            .header(CSRF_HEADER_NAME, &csrf_token)
            .json(&serde_json::json!({ "description": "patched" }))
            .send()
            .await
            .unwrap();
        assert_eq!(patched.status(), reqwest::StatusCode::OK);
        let pbody: serde_json::Value = patched.json().await.unwrap();
        assert_eq!(pbody["ok"], true);

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: unavailable directory create returns 503, not invent success.
    #[tokio::test]
    async fn account_create_unavailable_directory_service_unavailable() {
        let mut config = test_state(0).config.clone();
        config.allow_directory_unauthenticated = true;
        let state = Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter: None,
            ban: BanGuard::with_backend(
                BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
            directory: crate::directory::directory_unavailable(),
            config,
        });
        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .ok();
        });
        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/api/v1/accounts"))
            .json(&serde_json::json!({ "name": "x", "domain_id": "d" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["ok"], false);
        assert_eq!(body["source"], "unavailable");
        assert!(body.get("account").is_none() || body["account"].is_null());
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: request log path helper redacts onion labels (no secret dump).
    #[test]
    fn request_log_path_redacts_onion() {
        use crate::config::redact_onion_in_text;
        let v3 = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx";
        let path = format!("/via/{v3}.onion/accounts");
        let red = redact_onion_in_text(&path);
        assert!(
            red.contains("<onion-redacted>"),
            "path must redact onion: {red}"
        );
        assert!(!red.contains(v3), "full onion label must not remain: {red}");
    }

    /// Named contract: middleware path field builder (exact wiring SoT) redacts onion
    /// and never returns query content (presence flag only).
    #[test]
    fn request_log_path_fields_redacts_onion_and_hides_query() {
        let v3 = "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx";
        let uri: axum::http::Uri = format!("/via/{v3}.onion/accounts?token=secret-value")
            .parse()
            .unwrap();
        let (path, has_query) = request_log_path_fields(&uri);
        assert!(
            path.contains("<onion-redacted>"),
            "middleware path field must redact: {path}"
        );
        assert!(
            !path.contains(v3),
            "full onion must not appear in log path field: {path}"
        );
        assert!(has_query, "query presence should be true");
        assert!(
            !path.contains("token") && !path.contains("secret-value"),
            "query must not leak into path field: {path}"
        );
    }
}
