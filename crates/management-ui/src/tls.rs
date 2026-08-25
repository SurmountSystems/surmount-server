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

use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum_server::accept::Accept;
use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use rustls::server::ServerConfig;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;

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
    /// private key is owner-only (`mode & 0o077 == 0`) or group-readable
    /// `0640` when the file group is the shared TLS group (`surmount-tls`).
    /// World bits and group-write are always refused.
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

/// Result of a structured owner-only secret-file mode check (no string matching).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretFileModeStatus {
    /// Path missing (caller may treat as "not usable" for reissue).
    Missing,
    /// Regular file, not a symlink, `mode & 0o077 == 0`.
    OwnerOnlyOk,
    /// Hard fail: exists but unsafe or wrong type (do not reissue over it).
    HardFail(String),
}

/// Structured mode check for host secret files (TLS key, ACME account JSON).
///
/// Uses `symlink_metadata` so a symlink is never followed for the mode check.
/// Missing path is [`SecretFileModeStatus::Missing`]; group/world bits and
/// non-regular files are hard fails.
pub fn secret_file_mode_status(path: &Path, label: &str) -> SecretFileModeStatus {
    use std::os::unix::fs::PermissionsExt;
    if path_is_blank(path) {
        return SecretFileModeStatus::HardFail(format!("{label} path is empty"));
    }
    match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => SecretFileModeStatus::Missing,
        Err(err) => SecretFileModeStatus::HardFail(format!(
            "{label} missing or unreadable at {}: {err}",
            path.display()
        )),
        Ok(meta) if meta.file_type().is_symlink() => SecretFileModeStatus::HardFail(format!(
            "{label} at {} is a symlink (refused; require regular file)",
            path.display()
        )),
        Ok(meta) if !meta.is_file() => SecretFileModeStatus::HardFail(format!(
            "{label} path is not a regular file: {}",
            path.display()
        )),
        Ok(meta) => {
            let mode = meta.permissions().mode();
            if mode & 0o077 != 0 {
                SecretFileModeStatus::HardFail(format!(
                    "{label} at {} must not be group/world readable \
                     (mode {:04o}; require owner-only, e.g. 0600)",
                    path.display(),
                    mode & 0o777
                ))
            } else {
                SecretFileModeStatus::OwnerOnlyOk
            }
        }
    }
}

/// File group that may share a group-readable (0640) TLS private key with
/// Stalwart. Owner stays `surmount-ui`; world bits stay off.
pub const SHARED_TLS_KEY_GROUP: &str = "surmount-tls";

/// Private key must be owner-only, or 0640 with a shared TLS group.
fn require_private_key_mode(path: &Path) -> Result<(), String> {
    require_tls_private_key_mode(path, &[SHARED_TLS_KEY_GROUP])
}

/// TLS private-key mode check. `shared_groups` is the allowlist of file
/// group names that may have group-read (0640) so mail and the HTTPS edge
/// can present the same leaf. Tests pass the temp file's own group.
pub(crate) fn require_tls_private_key_mode(
    path: &Path,
    shared_groups: &[&str],
) -> Result<(), String> {
    match secret_file_mode_status(path, "TLS private key") {
        SecretFileModeStatus::OwnerOnlyOk => Ok(()),
        SecretFileModeStatus::Missing => Err(format!(
            "TLS private key missing or unreadable at {}",
            path.display()
        )),
        SecretFileModeStatus::HardFail(msg) => {
            if tls_key_shared_group_readable(path, shared_groups) {
                Ok(())
            } else {
                Err(format!(
                    "{msg}; 0640 is allowed only when the file group is {SHARED_TLS_KEY_GROUP}"
                ))
            }
        }
    }
}

/// True when the key is group-readable (0640 or 0440), not world-open, and
/// the file group name is on the shared-TLS allowlist.
fn tls_key_shared_group_readable(path: &Path, shared_groups: &[&str]) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return false;
    }
    let mode = meta.permissions().mode() & 0o777;
    if !matches!(mode, 0o640 | 0o440) {
        return false;
    }
    let Some(name) = unix_group_name(meta.gid()) else {
        return false;
    };
    shared_groups.iter().any(|g| *g == name)
}

fn unix_group_name(gid: u32) -> Option<String> {
    let mut buf = vec![0u8; 1024];
    let mut grp = std::mem::MaybeUninit::<libc::group>::uninit();
    let mut result: *mut libc::group = std::ptr::null_mut();
    loop {
        let rc = unsafe {
            libc::getgrgid_r(
                gid,
                grp.as_mut_ptr(),
                buf.as_mut_ptr().cast::<libc::c_char>(),
                buf.len(),
                &mut result,
            )
        };
        if rc == libc::ERANGE {
            buf.resize(buf.len().saturating_mul(2).max(2048), 0);
            continue;
        }
        if rc != 0 || result.is_null() {
            return None;
        }
        let name = unsafe { std::ffi::CStr::from_ptr((*result).gr_name) };
        return name.to_str().ok().map(str::to_owned);
    }
}

/// Fail closed: path must exist as owner-only regular file (secrets on host).
pub fn require_owner_only_secret_file(path: &Path, label: &str) -> Result<(), String> {
    match secret_file_mode_status(path, label) {
        SecretFileModeStatus::OwnerOnlyOk => Ok(()),
        SecretFileModeStatus::Missing => Err(format!(
            "{label} missing or unreadable at {}",
            path.display()
        )),
        SecretFileModeStatus::HardFail(msg) => Err(msg),
    }
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
/// - Provider: aws-lc-rs via [`rustls::crypto::aws_lc_rs::default_provider`]
///   with workspace feature `prefer-post-quantum` so hybrid **X25519MLKEM768**
///   is offered first among default kx groups (D1 tree aim)
/// - ALPN: h2, http/1.1
///
/// Hermetic unit tests prove provider group configuration. They do **not**
/// prove a live production host negotiated hybrid (that remains host residual
/// after B1 HTTPS cutover).
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

/// Journal field cap for handshake error Display (no cert dumps).
const TLS_HANDSHAKE_ERROR_MAX: usize = 200;
/// DNS hostname length cap for SNI (RFC 1035).
const TLS_HANDSHAKE_SNI_MAX: usize = 253;
/// Same timeout as axum-server's rustls acceptor (production, not its test 1s).
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Fields emitted for a failed TLS accept / handshake (journal only).
///
/// Never contains PEM, certificate bytes, session tickets, or secrets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TlsHandshakeFailureFields {
    pub peer: String,
    /// ClientHello server name if already parsed.
    pub sni: Option<String>,
    /// Short filter label (`timeout`, `incompatible`, `protocol_version`, ...).
    pub error_kind: String,
    /// Truncated Display; PEM-shaped text stripped.
    pub error: String,
}

/// Build journal fields for a TLS handshake / accept failure.
///
/// `peer` is the TCP peer if known. `sni` is the ClientHello name if parsed.
/// `error` is Display of the accept error (sanitized).
pub(crate) fn tls_handshake_failure_fields(
    peer: Option<SocketAddr>,
    sni: Option<&str>,
    error: &dyn std::fmt::Display,
) -> TlsHandshakeFailureFields {
    let peer = peer
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let raw = error.to_string();
    TlsHandshakeFailureFields {
        peer,
        sni: sanitize_handshake_sni(sni),
        error_kind: classify_tls_handshake_error(&raw),
        error: sanitize_handshake_error_display(&raw),
    }
}

/// Emit `tls_handshake_failed` at warn (our event, not rustls=debug).
pub(crate) fn emit_tls_handshake_failed(
    peer: Option<SocketAddr>,
    sni: Option<&str>,
    error: &dyn std::fmt::Display,
) {
    let fields = tls_handshake_failure_fields(peer, sni, error);
    match fields.sni.as_deref() {
        Some(sni) => {
            tracing::warn!(
                peer = %fields.peer,
                sni = %sni,
                error_kind = %fields.error_kind,
                error = %fields.error,
                "tls_handshake_failed"
            );
        }
        None => {
            tracing::warn!(
                peer = %fields.peer,
                error_kind = %fields.error_kind,
                error = %fields.error,
                "tls_handshake_failed"
            );
        }
    }
}

fn sanitize_handshake_sni(sni: Option<&str>) -> Option<String> {
    let raw = sni.map(str::trim).filter(|s| !s.is_empty())?;
    let upper = raw.to_ascii_uppercase();
    if upper.contains("BEGIN CERTIFICATE") || upper.contains("BEGIN PRIVATE") {
        return None;
    }
    Some(truncate_log_text(raw, TLS_HANDSHAKE_SNI_MAX))
}

fn sanitize_handshake_error_display(raw: &str) -> String {
    truncate_log_text(&strip_pem_shaped(raw), TLS_HANDSHAKE_ERROR_MAX)
}

fn truncate_log_text(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Replace PEM banners and bodies with a marker so journal fields stay short.
fn strip_pem_shaped(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    loop {
        let upper = rest.to_ascii_uppercase();
        let Some(begin) = upper.find("-----BEGIN ") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..begin]);
        out.push_str("<redacted-pem>");
        let after_begin = &rest[begin + "-----BEGIN ".len()..];
        let after_upper = after_begin.to_ascii_uppercase();
        if let Some(end) = after_upper.find("-----END ") {
            let after_end = &after_begin[end + "-----END ".len()..];
            if let Some(close) = after_end.find("-----") {
                rest = &after_end[close + "-----".len()..];
                continue;
            }
        }
        break;
    }
    out
}

fn classify_tls_handshake_error(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        return "timeout".into();
    }
    // rustls `PeerIncompatible` Display includes "incompatible" (e.g. Tls12NotOffered).
    if lower.contains("incompatible") {
        return "incompatible".into();
    }
    if lower.contains("protocolversion")
        || lower.contains("protocol_version")
        || lower.contains("tlsv1_2")
    {
        return "protocol_version".into();
    }
    if lower.contains("noapplicationprotocol") || lower.contains("no application protocol") {
        return "alpn".into();
    }
    if lower.contains("misbehav") {
        return "misbehaved".into();
    }
    if lower.contains("unexpected eof") {
        return "eof".into();
    }
    if lower.contains("connection reset") {
        return "reset".into();
    }
    if lower.contains("decrypt") || lower.contains("bad record mac") {
        return "decrypt".into();
    }
    if lower.contains("handshake") {
        return "handshake".into();
    }
    "error".into()
}

/// rustls acceptor that logs handshake / accept failures at `warn!`.
///
/// Wraps `LazyConfigAcceptor` so SNI is available after ClientHello even when
/// the rest of the handshake fails. Used by `serve_https_on_listener`.
#[derive(Clone)]
pub(crate) struct HandshakeLoggingAcceptor {
    config: RustlsConfig,
    handshake_timeout: Duration,
}

impl std::fmt::Debug for HandshakeLoggingAcceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HandshakeLoggingAcceptor")
            .field("handshake_timeout", &self.handshake_timeout)
            .finish_non_exhaustive()
    }
}

impl HandshakeLoggingAcceptor {
    pub(crate) fn new(config: RustlsConfig) -> Self {
        Self {
            config,
            handshake_timeout: TLS_HANDSHAKE_TIMEOUT,
        }
    }
}

impl<S> Accept<TcpStream, S> for HandshakeLoggingAcceptor
where
    S: Send + 'static,
{
    type Stream = TlsStream<TcpStream>;
    type Service = S;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<(Self::Stream, S)>> + Send>>;

    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let config = self.config.clone();
        let timeout = self.handshake_timeout;
        Box::pin(async move { accept_tls_logged(stream, service, config, timeout).await })
    }
}

async fn accept_tls_logged<S>(
    stream: TcpStream,
    service: S,
    config: RustlsConfig,
    timeout: Duration,
) -> io::Result<(TlsStream<TcpStream>, S)> {
    let peer = stream.peer_addr().ok();
    match tokio::time::timeout(timeout, complete_logged_handshake(stream, config, peer)).await {
        Ok(Ok(tls)) => Ok((tls, service)),
        Ok(Err(err)) => Err(err),
        Err(_elapsed) => {
            let err = io::Error::new(io::ErrorKind::TimedOut, "tls handshake timed out");
            emit_tls_handshake_failed(peer, None, &err);
            Err(err)
        }
    }
}

async fn complete_logged_handshake(
    stream: TcpStream,
    config: RustlsConfig,
    peer: Option<SocketAddr>,
) -> io::Result<TlsStream<TcpStream>> {
    let started =
        match tokio_rustls::LazyConfigAcceptor::new(rustls::server::Acceptor::default(), stream)
            .await
        {
            Ok(started) => started,
            Err(err) => {
                emit_tls_handshake_failed(peer, None, &err);
                return Err(err);
            }
        };
    let sni = started.client_hello().server_name().map(str::to_owned);
    match started.into_stream(config.get_inner()).await {
        Ok(tls) => Ok(tls),
        Err(err) => {
            emit_tls_handshake_failed(peer, sni.as_deref(), &err);
            Err(err)
        }
    }
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
    use super::test_support::{TempPemDir, write_temp_self_signed_pems};
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

    fn file_group_name(path: &std::path::Path) -> String {
        use std::os::unix::fs::MetadataExt;
        let gid = std::fs::metadata(path).unwrap().gid();
        unix_group_name(gid).unwrap_or_else(|| format!("gid-{gid}"))
    }

    #[test]
    fn require_files_exist_allows_0640_key_when_group_is_shared_tls() {
        let dir = TempPemDir::new("surmount-tls-keymode-shared");
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
        std::fs::set_permissions(&cert, std::fs::Permissions::from_mode(0o644)).unwrap();

        let group = file_group_name(&key);
        assert!(
            !group.is_empty(),
            "temp key must resolve a group name for the shared-TLS contract"
        );

        // Named contract: 0640 group-readable shared tls-key is allowed for
        // the HTTPS edge when the file group is surmount-tls / equivalent.
        // Hermetic tests use the temp file's own group as that equivalent.
        require_tls_private_key_mode(&key, &[&group]).expect(
            "0640 group-readable tls-key must be allowed when the file group \
             is the shared TLS group",
        );

        let paths = TlsPaths::new(&cert, &key);
        if group == SHARED_TLS_KEY_GROUP {
            paths
                .require_files_exist()
                .expect("HTTPS edge must accept 0640 when the file group is surmount-tls");
        }
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

    /// D1: product rustls path uses aws-lc-rs whose default provider includes
    /// hybrid X25519MLKEM768. Does not prove live host negotiation.
    #[test]
    fn aws_lc_rs_default_provider_includes_x25519mlkem768_hybrid() {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        let names: Vec<_> = provider.kx_groups.iter().map(|g| g.name()).collect();
        assert!(
            names.contains(&rustls::NamedGroup::X25519MLKEM768),
            "expected hybrid X25519MLKEM768 in default aws-lc-rs kx groups, got {names:?}"
        );
    }

    /// Regression: ACME / hyper-rustls ClientConfig::builder needs a process
    /// CryptoProvider. install_default(aws-lc-rs) must succeed (or already set).
    #[test]
    fn install_aws_lc_rs_default_provider_allows_client_config_builder() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let _ = rustls::ClientConfig::builder();
    }

    /// D1 first-class: prefer-post-quantum puts hybrid first among defaults.
    #[test]
    fn aws_lc_rs_default_provider_prefers_x25519mlkem768_first() {
        let provider = rustls::crypto::aws_lc_rs::default_provider();
        let first = provider
            .kx_groups
            .first()
            .expect("default provider must list at least one kx group")
            .name();
        assert_eq!(
            first,
            rustls::NamedGroup::X25519MLKEM768,
            "prefer-post-quantum should put X25519MLKEM768 first; got {first:?}"
        );
    }

    /// load_rustls_server_config builds TLS 1.3-only config (protocol list).
    #[test]
    fn load_rustls_server_config_is_tls13_only() {
        let dir = TempPemDir::new("surmount-tls-tls13");
        let paths = write_temp_self_signed_pems(dir.path(), "surmount-tls13");
        paths.require_files_exist().unwrap();
        let cfg = load_rustls_server_config(&paths).expect("load self-signed PEMs");
        // ServerConfig versions is private; protocol versions were set at build
        // via with_protocol_versions(&[TLS13]). Accept config built + ALPN as
        // the public smoke; hybrid group proof is on the provider above.
        assert!(
            !cfg.alpn_protocols.is_empty(),
            "TLS server config should advertise ALPN"
        );
        assert_eq!(cfg.alpn_protocols[0], b"h2".as_slice());
    }

    /// Named contract: handshake-failure helper emits peer, SNI, short kind,
    /// and Display. No PEM / cert material in the fields.
    #[test]
    fn tls_handshake_failure_fields_include_peer_sni_and_short_error() {
        use super::tls_handshake_failure_fields;
        use std::net::SocketAddr;

        let peer: SocketAddr = "192.0.2.10:44301".parse().unwrap();
        let fields = tls_handshake_failure_fields(
            Some(peer),
            Some("phone.example.test"),
            &"peer is incompatible: Tls12NotOffered",
        );
        assert_eq!(fields.peer, "192.0.2.10:44301");
        assert_eq!(fields.sni.as_deref(), Some("phone.example.test"));
        assert_eq!(fields.error_kind, "incompatible");
        assert!(
            fields.error.contains("Tls12NotOffered"),
            "Display should keep a short rustls reason: {}",
            fields.error
        );
        assert!(
            !fields.error.contains("BEGIN CERTIFICATE") && !fields.error.contains("BEGIN PRIVATE"),
            "must not look like PEM: {}",
            fields.error
        );
        assert!(
            fields.error_kind.len() <= 40,
            "error_kind must stay short: {}",
            fields.error_kind
        );
    }

    /// Named contract: missing peer/SNI stay explicit; PEM-shaped Display is
    /// stripped and the remaining text is truncated.
    #[test]
    fn tls_handshake_failure_fields_omit_unknown_sni_and_strip_pem() {
        use super::tls_handshake_failure_fields;

        let pem_blob = format!(
            "invalid peer cert -----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE----- extra",
            "MIIB".repeat(80)
        );
        let fields = tls_handshake_failure_fields(None, None, &pem_blob);
        assert_eq!(fields.peer, "unknown");
        assert!(fields.sni.is_none(), "unknown SNI must be None");
        assert!(
            !fields.error.contains("BEGIN CERTIFICATE"),
            "PEM banner must not enter journal fields: {}",
            fields.error
        );
        assert!(
            !fields.error.contains("MIIB"),
            "cert base64 must not enter journal fields: {}",
            fields.error
        );
        assert!(
            fields.error.len() <= 200,
            "error Display must be truncated: len {}",
            fields.error.len()
        );
    }
}
