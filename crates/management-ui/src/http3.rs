//! HTTP/3 (QUIC) listener next to TCP HTTPS on the Axum edge.
//!
//! Same rustls PEMs as TCP :443. QUIC ALPN is **h3 only**. TCP ALPN stays
//! `h2` + `http/1.1`. The same Axum `Router` is served; splora backends stay
//! HTTP/1.1 over Unix sockets. Clearnet Alt-Svc advertises `h3=":{port}"`
//! only after the QUIC listener is bound. Onion `h2` Alt-Svc stays separate
//! (merged, never replaced).
//!
//! Stack: quinn + h3, via axum-h3's production quinn backend (`h3-util`
//! feature `quinn`). See [axum-h3](https://crates.io/crates/axum-h3)
//! (accessed: 2026-08-31).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;
use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum_h3::H3Router;
use h3_util::quinn::H3QuinnAcceptor;
use quinn::crypto::rustls::QuicServerConfig;
use rustls::ServerConfig;
use tracing::info;

#[cfg(test)]
use crate::onion_discovery::ALT_SVC;
use crate::onion_discovery::{host_is_onion, merge_alt_svc_token};
use crate::tls::TlsPaths;

/// HTTP/3 listen knobs. `bound` is set true only after the QUIC endpoint
/// actually listens (Alt-Svc must not fire without that).
#[derive(Debug, Clone)]
pub struct Http3Config {
    pub enable: bool,
    pub bound: Arc<AtomicBool>,
    /// Bound UDP port once listening (0 = not bound). Used in Alt-Svc.
    pub bound_port: Arc<std::sync::atomic::AtomicU16>,
}

impl PartialEq for Http3Config {
    fn eq(&self, other: &Self) -> bool {
        self.enable == other.enable
            && self.bound.load(Ordering::SeqCst) == other.bound.load(Ordering::SeqCst)
            && self.bound_port.load(Ordering::SeqCst) == other.bound_port.load(Ordering::SeqCst)
    }
}

impl Eq for Http3Config {}

impl Default for Http3Config {
    fn default() -> Self {
        Self {
            enable: false,
            bound: Arc::new(AtomicBool::new(false)),
            bound_port: Arc::new(std::sync::atomic::AtomicU16::new(0)),
        }
    }
}

impl Http3Config {
    pub fn from_env_map(
        get: impl Fn(&str) -> Option<String>,
        listen_is_https: bool,
    ) -> Result<Self, String> {
        let enable = match get("SURMOUNT_HTTP3") {
            Some(v) => crate::config::parse_env_flag_truthy(&v),
            None => listen_is_https,
        };
        if enable && !listen_is_https {
            return Err(
                "SURMOUNT_HTTP3 requires SURMOUNT_LISTEN_MODE=https (QUIC uses the same PEMs; fail-closed)"
                    .into(),
            );
        }
        Ok(Self {
            enable,
            bound: Arc::new(AtomicBool::new(false)),
            bound_port: Arc::new(std::sync::atomic::AtomicU16::new(0)),
        })
    }

    pub fn listener_bound(&self) -> bool {
        self.bound.load(Ordering::SeqCst)
    }

    pub fn mark_bound(&self, port: u16) {
        self.bound_port.store(port, Ordering::SeqCst);
        self.bound.store(true, Ordering::SeqCst);
    }
}

/// rustls ServerConfig for QUIC: TLS 1.3, ALPN **h3 only**.
pub fn load_rustls_quic_server_config(paths: &TlsPaths) -> Result<Arc<ServerConfig>, String> {
    let mut config = crate::tls::load_rustls_server_config_with_alpn(paths, &[b"h3".as_slice()])?;
    // quinn requires this for QUIC 0-RTT plumbing on the rustls config.
    Arc::make_mut(&mut config).max_early_data_size = u32::MAX;
    Ok(config)
}

/// Alt-Svc token for the bound QUIC listener. None when not bound.
pub fn h3_alt_svc_value(cfg: &Http3Config) -> Option<String> {
    if !cfg.listener_bound() {
        return None;
    }
    let port = cfg.bound_port.load(Ordering::SeqCst);
    if port == 0 {
        return None;
    }
    Some(format!("h3=\":{port}\"; ma=86400"))
}

/// Insert or merge clearnet `h3` Alt-Svc. Never overwrites onion `h2`.
/// Does not advertise h3 without a bound listener.
pub fn apply_h3_alt_svc(
    headers: &mut HeaderMap,
    cfg: &Http3Config,
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
    if !(200..400).contains(&code) {
        return false;
    }
    let Some(val) = h3_alt_svc_value(cfg) else {
        return false;
    };
    merge_alt_svc_token(headers, &val)
}

/// Bind UDP QUIC on `listen` (same IP:port as TCP HTTPS) and serve `app`.
pub async fn serve_http3(
    listen: SocketAddr,
    app: Router,
    paths: &TlsPaths,
    cfg: &Http3Config,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    let endpoint = bind_quic_endpoint(listen, paths)?;
    let bound = endpoint.local_addr().context("QUIC endpoint local_addr")?;
    cfg.mark_bound(bound.port());
    info!(%bound, mode = "http3-quic", "surmount-management-ui QUIC listening");
    let acceptor = H3QuinnAcceptor::new(endpoint);
    H3Router::from(app)
        .serve_with_shutdown(acceptor, shutdown)
        .await
        .map_err(|e| anyhow::anyhow!("HTTP/3 serve: {e}"))?;
    Ok(())
}

pub fn bind_quic_endpoint(
    listen: SocketAddr,
    paths: &TlsPaths,
) -> anyhow::Result<h3_quinn::quinn::Endpoint> {
    let rustls = load_rustls_quic_server_config(paths)
        .map_err(anyhow::Error::msg)
        .context("load rustls QUIC ServerConfig (ALPN h3)")?;
    let quic = QuicServerConfig::try_from((*rustls).clone())
        .map_err(|e| anyhow::anyhow!("QUIC rustls config rejected: {e}"))?;
    let server = quinn::ServerConfig::with_crypto(Arc::new(quic));
    let endpoint = h3_quinn::quinn::Endpoint::server(server, listen)
        .with_context(|| format!("bind QUIC UDP {listen}"))?;
    Ok(endpoint)
}

/// Fail closed when HTTP/3 is required and bind fails.
pub fn require_http3_bind(enable: bool, listen_is_https: bool) -> Result<(), String> {
    if enable && !listen_is_https {
        return Err("HTTP/3 is enabled but listen mode is not https (fail-closed)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proxy_splora::{
        DEFAULT_BODY_LIMIT_BYTES, DEFAULT_QUEUE_PATH, SploraInstance, SploraProxyConfig,
        splora_proxy_middleware,
    };
    use crate::tls::test_support::{TempPemDir, write_temp_self_signed_pems};
    use axum::Router;
    use axum::extract::Request as AxumRequest;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::routing::get;
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::pem::PemObject;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    #[test]
    fn quic_alpn_is_h3_only() {
        let dir = TempPemDir::new("surmount-quic-alpn");
        let paths = write_temp_self_signed_pems(dir.path(), "surmount-quic");
        paths.require_files_exist().unwrap();
        let cfg = load_rustls_quic_server_config(&paths).expect("quic pems");
        assert_eq!(cfg.alpn_protocols, vec![b"h3".to_vec()]);
    }

    #[test]
    fn tcp_alpn_unchanged_h2_http11() {
        let dir = TempPemDir::new("surmount-tcp-alpn");
        let paths = write_temp_self_signed_pems(dir.path(), "surmount-tcp");
        paths.require_files_exist().unwrap();
        let cfg = crate::tls::load_rustls_server_config(&paths).expect("tcp pems");
        assert_eq!(
            cfg.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );
    }

    #[test]
    fn alt_svc_absent_when_quic_not_bound() {
        let cfg = Http3Config {
            enable: true,
            ..Http3Config::default()
        };
        assert!(h3_alt_svc_value(&cfg).is_none());
        let mut headers = HeaderMap::new();
        let injected = apply_h3_alt_svc(
            &mut headers,
            &cfg,
            true,
            false,
            "services.example.test",
            StatusCode::OK,
        );
        assert!(!injected);
        assert!(headers.get(&ALT_SVC).is_none());
    }

    #[test]
    fn alt_svc_h3_when_bound_and_merges_with_onion_h2() {
        let cfg = Http3Config::default();
        cfg.mark_bound(443);
        assert_eq!(
            h3_alt_svc_value(&cfg).as_deref(),
            Some("h3=\":443\"; ma=86400")
        );
        let mut headers = HeaderMap::new();
        assert!(apply_h3_alt_svc(
            &mut headers,
            &cfg,
            true,
            false,
            "services.example.test",
            StatusCode::OK,
        ));
        merge_alt_svc_token(
            &mut headers,
            "h2=\"abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion:443\"; ma=86400; persist=1",
        );
        let alt = headers.get(&ALT_SVC).and_then(|v| v.to_str().ok()).unwrap();
        assert!(alt.contains("h3=\":443\""), "{alt}");
        assert!(alt.contains("h2=\""), "{alt}");
        assert!(
            alt.contains("onion:443") || alt.contains(".onion:443"),
            "{alt}"
        );
    }

    #[test]
    fn alt_svc_h3_not_on_onion_host() {
        let cfg = Http3Config::default();
        cfg.mark_bound(443);
        let mut headers = HeaderMap::new();
        let injected = apply_h3_alt_svc(
            &mut headers,
            &cfg,
            true,
            false,
            "abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion",
            StatusCode::OK,
        );
        assert!(!injected);
        assert!(headers.get(&ALT_SVC).is_none());
    }

    #[test]
    fn http3_env_defaults_on_with_https_and_refuses_plain() {
        let on = Http3Config::from_env_map(|_| None, true).unwrap();
        assert!(on.enable);
        let off = Http3Config::from_env_map(|_| None, false).unwrap();
        assert!(!off.enable);
        let err = Http3Config::from_env_map(
            |k| match k {
                "SURMOUNT_HTTP3" => Some("1".into()),
                _ => None,
            },
            false,
        )
        .unwrap_err();
        assert!(err.contains("https"), "{err}");
    }

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

    /// Named contract: hermetic h3 GET/POST carries Host + X-Forwarded-Proto
    /// through the same Axum router onto a fake Unix indexer.
    #[tokio::test]
    async fn http3_get_post_host_and_x_forwarded_proto_to_fake_uds() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let sock = tmp_sock("splora-h3");
        let seen = Arc::new(Mutex::new(Vec::<HeaderMap>::new()));
        let seen_c = seen.clone();
        let mock = Router::new()
            .route(
                "/api/blocks/tip/hash",
                get({
                    let seen_c = seen_c.clone();
                    move |req: AxumRequest| {
                        let seen_c = seen_c.clone();
                        async move {
                            seen_c.lock().unwrap().push(req.headers().clone());
                            "tip-h3"
                        }
                    }
                }),
            )
            .route(
                "/api/tx",
                axum::routing::post({
                    let seen_c = seen_c.clone();
                    move |req: AxumRequest| {
                        let seen_c = seen_c.clone();
                        async move {
                            seen_c.lock().unwrap().push(req.headers().clone());
                            "txid-h3"
                        }
                    }
                }),
            );
        let _ = std::fs::remove_file(&sock);
        let uds = tokio::net::UnixListener::bind(&sock).unwrap();
        let mock_serve = tokio::spawn(async move {
            axum::serve(uds, mock).await.ok();
        });
        tokio::time::sleep(Duration::from_millis(20)).await;

        let dir = TempPemDir::new("surmount-h3-live");
        let paths = write_temp_self_signed_pems(dir.path(), "localhost");
        paths.require_files_exist().unwrap();

        let splora = SploraProxyConfig {
            enable: true,
            instances: vec![SploraInstance {
                name: "mainnet".into(),
                hosts: vec!["esplora.example.test".into()],
                http_socket: sock.clone(),
            }],
            queue_socket: PathBuf::from("/tmp/splora-h3-queue-unused.sock"),
            queue_path: DEFAULT_QUEUE_PATH.into(),
            body_limit_bytes: DEFAULT_BODY_LIMIT_BYTES,
        };
        let http3_cfg = Http3Config {
            enable: true,
            ..Http3Config::default()
        };

        let mut state_cfg = {
            use crate::config::{AppConfig, OnionSurface};
            use crate::tls::ListenMode;
            use surmount_management_ui::auth::AuthConfig;
            use surmount_management_ui::ban::{
                BanConfig, BanEnforcement, BanGuard, MemoryBanBackend,
            };
            AppConfig {
                listen: SocketAddr::from(([127, 0, 0, 1], 0)),
                http_redirect_listen: None,
                local_cleartext_listen: None,
                listen_mode: ListenMode::Https(paths.clone()),
                redirect_http_to_https: false,
                redirect_allowed_hosts: vec!["esplora.example.test".into()],
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
                splora_proxy: splora,
                http3: http3_cfg.clone(),
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
            }
        };
        // Same Arc flags as the listener we are about to bind.
        state_cfg.http3 = http3_cfg.clone();

        let state = Arc::new(crate::AppState {
            http: reqwest::Client::new(),
            rate_limiter: None,
            ban: surmount_management_ui::ban::BanGuard::with_backend(
                surmount_management_ui::ban::BanEnforcement::Off,
                Box::new(surmount_management_ui::ban::MemoryBanBackend::new(vec![])),
            ),
            directory: crate::directory::directory_unavailable(),
            config: state_cfg,
        });

        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .fallback(axum::routing::any(|| async { StatusCode::NOT_FOUND }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                splora_proxy_middleware,
            ))
            .with_state(state.clone());

        let endpoint =
            bind_quic_endpoint(SocketAddr::from(([127, 0, 0, 1], 0)), &paths).expect("bind quic");
        let bound = endpoint.local_addr().unwrap();
        http3_cfg.mark_bound(bound.port());
        let serve = tokio::spawn(async move {
            let acceptor = H3QuinnAcceptor::new(endpoint);
            let _ = H3Router::from(app).serve(acceptor).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        h3_request(
            bound,
            dir.path(),
            "GET",
            "https://esplora.example.test/api/blocks/tip/hash",
        )
        .await;
        h3_request(
            bound,
            dir.path(),
            "POST",
            "https://esplora.example.test/api/tx",
        )
        .await;

        tokio::time::sleep(Duration::from_millis(80)).await;
        let hits = seen.lock().unwrap().clone();
        assert!(
            hits.len() >= 2,
            "expected GET and POST on fake UDS, got {}",
            hits.len()
        );
        for headers in &hits {
            assert_eq!(
                headers.get(header::HOST).and_then(|v| v.to_str().ok()),
                Some("esplora.example.test"),
                "Host must be forwarded over h3->UDS; headers={headers:?}"
            );
            assert_eq!(
                headers
                    .get("x-forwarded-proto")
                    .and_then(|v| v.to_str().ok()),
                Some("https"),
                "X-Forwarded-Proto must be forwarded over h3->UDS (fail if omitted); headers={headers:?}"
            );
        }

        serve.abort();
        mock_serve.abort();
        let _ = std::fs::remove_file(&sock);
    }

    async fn h3_request(addr: SocketAddr, pem_dir: &std::path::Path, method: &str, uri: &str) {
        let cert_pem = std::fs::read(pem_dir.join("cert.pem")).unwrap();
        let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&cert_pem)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let mut roots = rustls::RootCertStore::empty();
        for c in certs {
            roots.add(c).unwrap();
        }
        let mut client_crypto = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_crypto.alpn_protocols = vec![b"h3".to_vec()];
        client_crypto.enable_early_data = true;
        let client_config = quinn::ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto).unwrap(),
        ));
        let mut endpoint = quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0))).unwrap();
        endpoint.set_default_client_config(client_config);
        let conn = endpoint
            .connect(addr, "localhost")
            .unwrap()
            .await
            .expect("quic connect");
        let h3_conn = h3_quinn::Connection::new(conn);
        let (mut driver, mut send_request) = h3::client::new(h3_conn).await.unwrap();
        let driver_task = tokio::spawn(async move {
            let _ = driver.await;
        });
        let mut builder = axum::http::Request::builder().method(method).uri(uri);
        builder = builder.header("host", "esplora.example.test");
        let req = builder.body(()).unwrap();
        let mut stream = send_request.send_request(req).await.unwrap();
        if method == "POST" {
            stream
                .send_data(bytes::Bytes::from_static(b"rawtx"))
                .await
                .ok();
        }
        stream.finish().await.ok();
        let _resp = stream.recv_response().await;
        driver_task.abort();
    }
}
