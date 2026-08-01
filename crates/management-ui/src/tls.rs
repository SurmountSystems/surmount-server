//! TLS path configuration and rustls HTTPS acceptor for edge mode.
//!
//! Certificate and key material live on the host as deploy secrets
//! (never in git). This module holds paths, validates them, and builds a
//! TLS 1.3-lean rustls `ServerConfig` for in-process HTTPS.
//!
//! **Fail closed:** HTTPS mode does not serve cleartext unless an explicit
//! dangerous escape env is set (default off). Escape is a deliberate override:
//! when set it always forces cleartext under the https label, even if the
//! rustls acceptor is ready. Happy path: escape off + real rustls terminate.
//! See `https_startup_decision`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::pki_types::{pem::PemObject, CertificateDer, PrivateKeyDer};
use rustls::server::ServerConfig;

/// PEM certificate + private key paths for rustls / edge HTTPS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsPaths {
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
}

impl TlsPaths {
    pub fn new(cert_path: impl Into<PathBuf>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
        }
    }

    /// True when both path strings are non-empty (does not touch the filesystem).
    pub fn paths_configured(&self) -> bool {
        !path_is_blank(&self.cert_path) && !path_is_blank(&self.key_path)
    }

    /// Fail-loud check used at startup when HTTPS is required.
    /// Returns Ok(()) only if both files exist as regular files and the
    /// private key is not group/world readable (`mode & 0o077 == 0`).
    pub fn require_files_exist(&self) -> Result<(), String> {
        require_regular_file(&self.cert_path, "TLS certificate")?;
        require_regular_file(&self.key_path, "TLS private key")?;
        require_private_key_mode(&self.key_path)?;
        Ok(())
    }
}

fn path_is_blank(p: &Path) -> bool {
    p.as_os_str().is_empty()
}

fn require_regular_file(path: &Path, label: &str) -> Result<(), String> {
    if path_is_blank(path) {
        return Err(format!("{label} path is empty"));
    }
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() => Ok(()),
        Ok(_) => Err(format!(
            "{label} path is not a regular file: {}",
            path.display()
        )),
        Err(err) => Err(format!(
            "{label} missing or unreadable at {}: {err}",
            path.display()
        )),
    }
}

/// Private key must not be group- or world-readable (`mode & 0o077 == 0`).
fn require_private_key_mode(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|err| {
        format!(
            "TLS private key missing or unreadable at {}: {err}",
            path.display()
        )
    })?;
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(format!(
            "TLS private key at {} must not be group/world readable \
             (mode {:04o}; require owner-only, e.g. 0600)",
            path.display(),
            mode & 0o777
        ));
    }
    Ok(())
}

/// How the management UI / edge should listen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenMode {
    /// Plain HTTP (dev, loopback scaffold, or behind transitional nginx).
    PlainHttp,
    /// HTTPS with host deploy-secret PEM paths.
    Https(TlsPaths),
}

impl ListenMode {
    /// Parse from env-style strings.
    ///
    /// `mode`: "http" | "https" (case-insensitive). Default http.
    /// When https, cert and key paths are required non-empty.
    pub fn from_parts(
        mode: Option<&str>,
        cert_path: Option<&str>,
        key_path: Option<&str>,
    ) -> Result<Self, String> {
        let mode = mode.unwrap_or("http").trim().to_ascii_lowercase();
        match mode.as_str() {
            "http" | "plain" | "plainhttp" => Ok(ListenMode::PlainHttp),
            "https" | "tls" => {
                let cert = cert_path.map(str::trim).filter(|s| !s.is_empty());
                let key = key_path.map(str::trim).filter(|s| !s.is_empty());
                match (cert, key) {
                    (Some(c), Some(k)) => Ok(ListenMode::Https(TlsPaths::new(c, k))),
                    _ => Err(
                        "HTTPS listen mode requires SURMOUNT_TLS_CERT and SURMOUNT_TLS_KEY paths"
                            .into(),
                    ),
                }
            }
            other => Err(format!(
                "unknown SURMOUNT_LISTEN_MODE {other:?}; use http or https"
            )),
        }
    }

    pub fn is_https(&self) -> bool {
        matches!(self, ListenMode::Https(_))
    }
}

/// Whether main may bind a cleartext or TLS listener for this mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpsStartupDecision {
    /// Plain HTTP mode: cleartext bind is correct.
    ServePlainHttp,
    /// HTTPS with in-process rustls acceptor (real TLS terminate).
    ServeHttps,
    /// HTTPS requested but cannot serve safely: refuse to start.
    FailClosed { reason: &'static str },
    /// Explicit operator escape: cleartext under https label (dangerous).
    DangerousCleartextEscape,
}

/// Decide HTTPS startup behavior.
///
/// `rustls_acceptor_ready` is true when in-process TLS is wired (product constant).
/// `allow_cleartext_escape` maps to SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE
/// (default false). Escape is an emergency override: when set, cleartext wins
/// even if the acceptor is ready. Happy path is acceptor ready + escape off.
pub fn https_startup_decision(
    mode: &ListenMode,
    rustls_acceptor_ready: bool,
    allow_cleartext_escape: bool,
) -> HttpsStartupDecision {
    match mode {
        ListenMode::PlainHttp => HttpsStartupDecision::ServePlainHttp,
        // Emergency cleartext under https label (explicit operator opt-in).
        ListenMode::Https(_) if allow_cleartext_escape => {
            HttpsStartupDecision::DangerousCleartextEscape
        }
        ListenMode::Https(_) if rustls_acceptor_ready => HttpsStartupDecision::ServeHttps,
        ListenMode::Https(_) => HttpsStartupDecision::FailClosed {
            reason: "SURMOUNT_LISTEN_MODE=https but in-process rustls acceptor is \
                     unavailable in this build. Use listen mode http for dual-run \
                     behind transitional nginx, or set \
                     SURMOUNT_HTTPS_ALLOW_CLEARTEXT_ESCAPE=1 only for emergency \
                     debugging (always serves cleartext under the https label; \
                     never enable in production).",
        },
    }
}

/// Product has an in-process rustls acceptor.
pub const RUSTLS_ACCEPTOR_READY: bool = true;

/// Build a TLS 1.3-lean rustls server config from host PEM paths.
///
/// - Protocol: TLS 1.3 only (no TLS 1.2 / ancient versions)
/// - Provider: aws-lc-rs (default; includes hybrid PQ KEX preference when the
///   crate feature set enables it, e.g. X25519MLKEM768)
/// - ALPN: h2, http/1.1
///
/// Callers must already have run [`TlsPaths::require_files_exist`].
pub fn load_rustls_server_config(paths: &TlsPaths) -> Result<Arc<ServerConfig>, String> {
    let cert_bytes = std::fs::read(&paths.cert_path).map_err(|err| {
        format!(
            "failed to read TLS certificate at {}: {err}",
            paths.cert_path.display()
        )
    })?;
    let key_bytes = std::fs::read(&paths.key_path).map_err(|err| {
        format!(
            "failed to read TLS private key at {}: {err}",
            paths.key_path.display()
        )
    })?;

    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(&cert_bytes)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            format!(
                "invalid TLS certificate PEM at {}: {err}",
                paths.cert_path.display()
            )
        })?;
    if certs.is_empty() {
        return Err(format!(
            "no certificates found in PEM at {}",
            paths.cert_path.display()
        ));
    }

    let key = PrivateKeyDer::from_pem_slice(&key_bytes).map_err(|err| {
        format!(
            "invalid TLS private key PEM at {}: {err}",
            paths.key_path.display()
        )
    })?;

    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let mut config = ServerConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|err| format!("rustls TLS 1.3 config rejected: {err}"))?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|err| format!("rustls rejected certificate/key pair: {err}"))?;

    // Required by axum-server when building ServerConfig manually.
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(Arc::new(config))
}

/// Test-only helpers shared by `tls` unit tests and main integration tests.
#[cfg(test)]
pub(crate) mod test_support {
    use super::TlsPaths;
    use std::path::{Path, PathBuf};

    /// Removes a temp directory on Drop (always cleans test PEM material).
    pub(crate) struct TempPemDir {
        path: PathBuf,
    }

    impl TempPemDir {
        pub(crate) fn new(prefix: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        pub(crate) fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempPemDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// Self-signed PEMs for tests only (never production secrets). Key mode 0600.
    pub(crate) fn write_temp_self_signed_pems(dir: &Path, common_name: &str) -> TlsPaths {
        use std::os::unix::fs::PermissionsExt;

        std::fs::create_dir_all(dir).unwrap();
        let cert_path = dir.join("cert.pem");
        let key_path = dir.join("key.pem");

        let mut params = rcgen::CertificateParams::new(vec!["localhost".into()]).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, common_name);
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();

        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key_pair.serialize_pem()).unwrap();
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&cert_path, std::fs::Permissions::from_mode(0o644)).unwrap();

        TlsPaths::new(cert_path, key_path)
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{write_temp_self_signed_pems, TempPemDir};
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn default_mode_is_plain_http() {
        let mode = ListenMode::from_parts(None, None, None).unwrap();
        assert_eq!(mode, ListenMode::PlainHttp);
        assert!(!mode.is_https());
    }

    #[test]
    fn https_without_paths_fails() {
        let err = ListenMode::from_parts(Some("https"), None, None).unwrap_err();
        assert!(err.contains("SURMOUNT_TLS_CERT"), "{err}");
    }

    #[test]
    fn https_with_paths_configures_tls() {
        let mode = ListenMode::from_parts(
            Some("HTTPS"),
            Some("/run/surmount-secrets/tls/cert.pem"),
            Some("/run/surmount-secrets/tls/key.pem"),
        )
        .unwrap();
        match mode {
            ListenMode::Https(paths) => {
                assert!(paths.paths_configured());
                assert_eq!(
                    paths.cert_path.as_os_str(),
                    "/run/surmount-secrets/tls/cert.pem"
                );
            }
            ListenMode::PlainHttp => panic!("expected https"),
        }
    }

    #[test]
    fn require_files_exist_fails_loud_when_missing() {
        let paths = TlsPaths::new(
            "/run/surmount-secrets/tls/does-not-exist-cert.pem",
            "/run/surmount-secrets/tls/does-not-exist-key.pem",
        );
        let err = paths.require_files_exist().unwrap_err();
        assert!(
            err.contains("missing") || err.contains("unreadable"),
            "{err}"
        );
    }

    #[test]
    fn require_files_exist_ok_for_temp_files() {
        let dir = TempPemDir::new("surmount-tls-test");
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::File::create(&cert)
            .unwrap()
            .write_all(b"dummy-cert")
            .unwrap();
        std::fs::File::create(&key)
            .unwrap()
            .write_all(b"dummy-key")
            .unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&cert, std::fs::Permissions::from_mode(0o644)).unwrap();

        let paths = TlsPaths::new(&cert, &key);
        assert!(paths.require_files_exist().is_ok());
    }

    #[test]
    fn require_files_exist_rejects_world_readable_key() {
        let dir = TempPemDir::new("surmount-tls-keymode");
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::File::create(&cert)
            .unwrap()
            .write_all(b"dummy-cert")
            .unwrap();
        std::fs::File::create(&key)
            .unwrap()
            .write_all(b"dummy-key")
            .unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o644)).unwrap();

        let paths = TlsPaths::new(&cert, &key);
        let err = paths.require_files_exist().unwrap_err();
        assert!(err.contains("group/world") || err.contains("0o"), "{err}");
    }

    #[test]
    fn require_files_exist_rejects_group_readable_key() {
        let dir = TempPemDir::new("surmount-tls-keymode-g");
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::File::create(&cert)
            .unwrap()
            .write_all(b"dummy-cert")
            .unwrap();
        std::fs::File::create(&key)
            .unwrap()
            .write_all(b"dummy-key")
            .unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o640)).unwrap();

        let paths = TlsPaths::new(&cert, &key);
        let err = paths.require_files_exist().unwrap_err();
        assert!(err.contains("group/world"), "{err}");
    }

    #[test]
    fn https_mode_fail_closed_without_acceptor_or_escape() {
        let mode = ListenMode::Https(TlsPaths::new("/c.pem", "/k.pem"));
        match https_startup_decision(&mode, false, false) {
            HttpsStartupDecision::FailClosed { reason } => {
                assert!(reason.contains("rustls"), "{reason}");
                assert!(
                    reason.contains("unavailable") || reason.contains("acceptor"),
                    "{reason}"
                );
                assert!(!reason.contains("not wired yet"), "{reason}");
            }
            other => panic!("expected FailClosed, got {other:?}"),
        }
    }

    #[test]
    fn https_mode_escape_allows_dangerous_cleartext() {
        let mode = ListenMode::Https(TlsPaths::new("/c.pem", "/k.pem"));
        assert_eq!(
            https_startup_decision(&mode, false, true),
            HttpsStartupDecision::DangerousCleartextEscape
        );
    }

    #[test]
    fn https_mode_acceptor_ready_serves_https() {
        let mode = ListenMode::Https(TlsPaths::new("/c.pem", "/k.pem"));
        assert_eq!(
            https_startup_decision(&mode, true, false),
            HttpsStartupDecision::ServeHttps
        );
        // Escape is emergency override even when acceptor is ready.
        assert_eq!(
            https_startup_decision(&mode, true, true),
            HttpsStartupDecision::DangerousCleartextEscape
        );
    }

    #[test]
    fn product_constant_marks_acceptor_ready() {
        // Constant is the product wire flag; decision must take ServeHttps path.
        let mode = ListenMode::Https(TlsPaths::new("/c.pem", "/k.pem"));
        assert_eq!(
            https_startup_decision(&mode, RUSTLS_ACCEPTOR_READY, false),
            HttpsStartupDecision::ServeHttps
        );
    }

    #[test]
    fn plain_http_always_serves_plain() {
        assert_eq!(
            https_startup_decision(&ListenMode::PlainHttp, false, false),
            HttpsStartupDecision::ServePlainHttp
        );
    }

    #[test]
    fn load_rustls_server_config_rejects_garbage_pem() {
        let dir = TempPemDir::new("surmount-tls-badpem");
        let cert = dir.path().join("cert.pem");
        let key = dir.path().join("key.pem");
        std::fs::write(&cert, b"not-a-cert\n").unwrap();
        std::fs::write(&key, b"not-a-key\n").unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();

        let paths = TlsPaths::new(&cert, &key);
        paths.require_files_exist().unwrap();
        let err = load_rustls_server_config(&paths).unwrap_err();
        assert!(
            err.contains("invalid") || err.contains("no certificates") || err.contains("PEM"),
            "{err}"
        );
    }

    #[test]
    fn load_rustls_server_config_accepts_self_signed_pems() {
        let dir = TempPemDir::new("surmount-tls-goodpem");
        let paths = write_temp_self_signed_pems(dir.path(), "surmount-test");
        paths.require_files_exist().unwrap();
        let cfg = load_rustls_server_config(&paths).expect("load self-signed PEMs");
        assert_eq!(
            cfg.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );
    }
}
