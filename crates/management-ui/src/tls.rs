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

/// TCP HTTPS ALPN: HTTP/2 then HTTP/1.1. QUIC uses h3 only (see `http3`).
pub const TCP_ALPN_PROTOCOLS: &[&[u8]] = &[b"h2", b"http/1.1"];

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
    load_rustls_server_config_with_alpn(paths, TCP_ALPN_PROTOCOLS)
}

/// Same PEM + TLS 1.3 builder with an explicit ALPN list (TCP vs QUIC).
pub(crate) fn load_rustls_server_config_with_alpn(
    paths: &TlsPaths,
    alpn: &[&[u8]],
) -> Result<Arc<ServerConfig>, String> {
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

    config.alpn_protocols = alpn.iter().map(|p| p.to_vec()).collect();

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

/// HTTP/3 QUIC listener (nested so the flake git tree sees this module).
///
/// HTTP/3 (QUIC) listener next to TCP HTTPS on the Axum edge.
/// Same rustls PEMs as TCP :443. QUIC ALPN is **h3 only**. TCP ALPN stays
/// `h2` + `http/1.1`. The same Axum `Router` is served; splora backends stay
/// HTTP/1.1 over Unix sockets. Clearnet Alt-Svc advertises `h3=":{port}"`
/// only after the QUIC listener is bound. Onion `h2` Alt-Svc stays separate
/// (merged, never replaced).
///
/// Stack: quinn + h3, via axum-h3's production quinn backend (`h3-util`
/// feature `quinn`). See [axum-h3](https://crates.io/crates/axum-h3)
/// (accessed: 2026-08-31).
pub(crate) mod http3 {

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
        let mut config =
            crate::tls::load_rustls_server_config_with_alpn(paths, &[b"h3".as_slice()])?;
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
        use crate::proxy_vaultwarden::splora::{
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
                use surmount_management_ui::ban::{BanConfig, BanEnforcement};
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

            let endpoint = bind_quic_endpoint(SocketAddr::from(([127, 0, 0, 1], 0)), &paths)
                .expect("bind quic");
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
            let mut endpoint =
                quinn::Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0))).unwrap();
            endpoint.set_default_client_config(client_config);
            let conn = endpoint
                .connect(addr, "localhost")
                .unwrap()
                .await
                .expect("quic connect");
            let h3_conn = h3_quinn::Connection::new(conn);
            let (mut driver, mut send_request) = h3::client::new(h3_conn).await.unwrap();
            let driver_task = tokio::spawn(async move {
                let _ =
                    std::future::poll_fn(|cx| std::pin::Pin::new(&mut driver).poll_close(cx)).await;
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
}
