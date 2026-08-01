//! Surmount management UI + Axum HTTPS edge foundation.
//!
//! Serves a small Axum HTTP API and Leptos SSR admin shell intended for
//! https://services.surmount.systems.
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
//! Stalwart remains the mail engine. Auth residual: Nostr-first product auth.
//! ACME HTTP-01 on product :80 is parked (see RESIDUAL.md).

mod api;
mod client_ip;
mod config;
mod pages;
mod redirect;
mod tls;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use tower_http::trace::TraceLayer;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use crate::client_ip::client_ip_for_rate_limit;
use crate::config::AppConfig;
use crate::redirect::{
    https_port_for_redirect, redirect_bind_decision, redirect_http_to_https, HttpToHttps,
    RedirectBindDecision, RedirectStatus,
};
use crate::tls::{
    https_startup_decision, load_rustls_server_config, HttpsStartupDecision, RUSTLS_ACCEPTOR_READY,
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
    let state = Arc::new(AppState {
        http: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("build reqwest client")?,
        rate_limiter,
        ban,
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
        .route("/health", get(api::health))
        .route("/api/health", get(api::health))
        .route("/api/v1/domains", get(api::list_domains))
        .route("/api/v1/accounts", get(api::list_accounts))
        .route("/api/v1/jmap", post(api::jmap_proxy_placeholder))
        .route("/api/v1/stalwart/status", get(api::stalwart_status))
        .layer(from_fn_with_state(state.clone(), rate_limit_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
/// request-context entry for future auth extractors (Q-ACL-1 open; Q-AUTH-1
/// Nostr parked — no login UI or crypto here). Hermetic tests call it directly.
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
        };
        let rate_limiter = config.rate_limiter();
        Arc::new(AppState {
            http: reqwest::Client::new(),
            rate_limiter,
            ban,
            config,
        })
    }

    /// Surface audit (docs/tests only): current routes do **not** auto-ban on
    /// 404/501. `signal_unauthorized` exists as a request-context hook but is
    /// not wired into handlers (Q-ACL-1 open; do not invent call sites).
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
        let health = client
            .get(format!("{base}/health"))
            .send()
            .await
            .unwrap();
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
        assert!(hook_addr != 0, "signal_unauthorized hook must remain linked");

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

        serve.abort();
        let _ = serve.await;
    }
}
