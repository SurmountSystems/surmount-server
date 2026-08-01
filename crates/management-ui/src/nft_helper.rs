//! Least-privilege nft ban helper protocol + client (scaffold).
//!
//! Product split:
//! - **management-ui** owns ban *decisions* (Allow/Whitelist/RateLimited/Banned/
//!   BanCandidate) and durable app state. It does **not** need CAP_NET_ADMIN.
//! - **surmount-nft-ban-helper** holds CAP_NET_ADMIN only when run as a
//!   **socket-activated oneshot** unit (AmbientCapabilities on that unit).
//!   The UI speaks a narrow JSON protocol over a Unix domain socket.
//!
//! **Why not child setcap spawn?** The UI unit keeps `NoNewPrivileges=true`
//! (and on privileged ports `CapabilityBoundingSet=[CAP_NET_BIND_SERVICE]`).
//! `PR_SET_NO_NEW_PRIVS` blocks children from gaining file capabilities or
//! setuid. Spawning `/run/wrappers/bin/...` from the UI **cannot** elevate.
//! Product path is UDS -> separate helper unit, not UI child exec.
//!
//! Hermetic tests: mock [`crate::ban::NftExec`] inside apply; process-spawn
//! client against fake scripts (tests only); UDS client against a local
//! listener thread. No root, no real nft in CI.
//!
//! Fail-closed: missing sock/helper, timeout, protocol error, or `ok:false`
//! must surface as Err. DryRun must not invoke the helper. Q-ACL not invented.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ban::{nft_add_ban_args, nft_remove_ban_args, NftExec};

/// Wire op name accepted by the helper binary.
pub const OP_ADD_BAN: &str = "add_ban";
pub const OP_REMOVE_BAN: &str = "remove_ban";
pub const OP_PING: &str = "ping";

/// Default client I/O timeout (fail-closed; avoids hung Axum worker).
pub const DEFAULT_HELPER_TIMEOUT: Duration = Duration::from_secs(2);

/// Request line (JSON object) from UI -> helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperRequest {
    pub op: String,
    /// Required for `add_ban`; ignored for `ping`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
}

impl HelperRequest {
    pub fn add_ban(ip: IpAddr) -> Self {
        Self {
            op: OP_ADD_BAN.into(),
            ip: Some(ip.to_string()),
        }
    }

    pub fn remove_ban(ip: IpAddr) -> Self {
        Self {
            op: OP_REMOVE_BAN.into(),
            ip: Some(ip.to_string()),
        }
    }

    pub fn ping() -> Self {
        Self {
            op: OP_PING.into(),
            ip: None,
        }
    }

    pub fn to_json_line(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("serialize helper request: {e}"))
    }

    pub fn from_json_line(raw: &str) -> Result<Self, String> {
        let t = raw.trim();
        if t.is_empty() {
            return Err("empty helper request (fail-closed)".into());
        }
        serde_json::from_str(t).map_err(|e| format!("parse helper request (fail-closed): {e}"))
    }
}

/// Response line (JSON object) from helper -> UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl HelperResponse {
    pub fn success() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn failure(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(msg.into()),
        }
    }

    pub fn to_json_line(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("serialize helper response: {e}"))
    }

    pub fn from_json_line(raw: &str) -> Result<Self, String> {
        let t = raw.trim();
        if t.is_empty() {
            return Err("empty helper response (fail-closed)".into());
        }
        serde_json::from_str(t).map_err(|e| format!("parse helper response (fail-closed): {e}"))
    }

    /// Treat non-ok or missing parse as Err (caller must not claim ban applied).
    pub fn into_result(self) -> Result<(), String> {
        if self.ok {
            Ok(())
        } else {
            Err(self
                .error
                .unwrap_or_else(|| "helper returned ok=false without error".into()))
        }
    }
}

/// Pure apply: validate request and optionally run nft via [`NftExec`].
///
/// Used by the helper binary and hermetic unit tests (RecordingNftExec).
pub fn apply_helper_request(req: &HelperRequest, exec: &dyn NftExec) -> HelperResponse {
    match req.op.as_str() {
        OP_PING => HelperResponse::success(),
        OP_ADD_BAN => {
            let Some(ip_s) = req.ip.as_deref() else {
                return HelperResponse::failure("add_ban requires ip (fail-closed)");
            };
            let ip: IpAddr = match ip_s.parse() {
                Ok(ip) => ip,
                Err(e) => {
                    return HelperResponse::failure(format!("add_ban bad ip {ip_s:?}: {e}"));
                }
            };
            let args = nft_add_ban_args(ip);
            match exec.run_nft(&args) {
                Ok(()) => HelperResponse::success(),
                // Crash-window re-signal: element already in set is success.
                Err(e) if crate::ban::nft_element_already_present(&e) => HelperResponse::success(),
                Err(e) => HelperResponse::failure(format!("nft add-element failed: {e}")),
            }
        }
        OP_REMOVE_BAN => {
            let Some(ip_s) = req.ip.as_deref() else {
                return HelperResponse::failure("remove_ban requires ip (fail-closed)");
            };
            let ip: IpAddr = match ip_s.parse() {
                Ok(ip) => ip,
                Err(e) => {
                    return HelperResponse::failure(format!("remove_ban bad ip {ip_s:?}: {e}"));
                }
            };
            let args = nft_remove_ban_args(ip);
            match exec.run_nft(&args) {
                Ok(()) => HelperResponse::success(),
                // Idempotent lab unban: absent element is success (mirror EEXIST-on-add).
                Err(e) if crate::ban::nft_element_already_absent(&e) => HelperResponse::success(),
                Err(e) => HelperResponse::failure(format!("nft delete-element failed: {e}")),
            }
        }
        other => HelperResponse::failure(format!(
            "unknown helper op {other:?} (fail-closed); allowed: {OP_ADD_BAN}, {OP_REMOVE_BAN}, {OP_PING}"
        )),
    }
}

/// Serve one request/response over a duplex stream (UDS or systemd Accept=yes fd).
pub fn serve_helper_once(
    stream: &mut (impl Read + Write),
    exec: &dyn NftExec,
) -> Result<HelperResponse, String> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(Read::by_ref(stream));
        reader
            .read_line(&mut line)
            .map_err(|e| format!("read helper request: {e}"))?;
    }
    let req = HelperRequest::from_json_line(&line)?;
    let resp = apply_helper_request(&req, exec);
    let out = resp.to_json_line()?;
    writeln!(stream, "{out}").map_err(|e| format!("write helper response: {e}"))?;
    stream
        .flush()
        .map_err(|e| format!("flush helper response: {e}"))?;
    Ok(resp)
}

/// Parse CLI argv for the helper binary (no shell).
///
/// Forms:
/// - `ping`
/// - `add-ban <ip>`
/// - `remove-ban <ip>` (lab unban / cleanup; idempotent if element absent)
/// - `apply-json` (single JSON object on stdin; process tests)
/// - `apply-systemd-socket` (connected socket on fd 0; Accept=yes oneshot)
pub fn request_from_argv(args: &[String]) -> Result<HelperRequest, String> {
    let mut it = args.iter();
    let cmd = it.next().map(|s| s.as_str()).ok_or_else(|| {
        "helper requires a command (ping|add-ban|remove-ban|apply-json|apply-systemd-socket)"
            .to_string()
    })?;
    match cmd {
        "ping" => {
            if it.next().is_some() {
                return Err("ping takes no arguments (fail-closed)".into());
            }
            Ok(HelperRequest::ping())
        }
        "add-ban" | "add_ban" => {
            let ip = it
                .next()
                .ok_or_else(|| "add-ban requires <ip> (fail-closed)".to_string())?;
            if it.next().is_some() {
                return Err("add-ban takes exactly one ip argument (fail-closed)".into());
            }
            Ok(HelperRequest {
                op: OP_ADD_BAN.into(),
                ip: Some(ip.clone()),
            })
        }
        "remove-ban" | "remove_ban" | "unban" => {
            let ip = it
                .next()
                .ok_or_else(|| "remove-ban requires <ip> (fail-closed)".to_string())?;
            if it.next().is_some() {
                return Err("remove-ban takes exactly one ip argument (fail-closed)".into());
            }
            Ok(HelperRequest {
                op: OP_REMOVE_BAN.into(),
                ip: Some(ip.clone()),
            })
        }
        "apply-json" => {
            if it.next().is_some() {
                return Err("apply-json takes no argv; JSON on stdin (fail-closed)".into());
            }
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| format!("read helper stdin: {e}"))?;
            HelperRequest::from_json_line(&buf)
        }
        "apply-systemd-socket" => {
            // Handled specially in main (stream I/O); not a HelperRequest parse.
            Err("apply-systemd-socket is stream mode (internal)".into())
        }
        other => Err(format!(
            "unknown helper command {other:?} (fail-closed); use \
             ping|add-ban|remove-ban|apply-json|apply-systemd-socket"
        )),
    }
}

/// How the UI reaches the privileged helper.
#[derive(Debug, Clone)]
enum HelperTransport {
    /// Product path: UDS to socket-activated oneshot (not UI child; NNP-safe).
    UnixSocket(PathBuf),
    /// Test / unsupported under UI NNP: spawn helper process (no elevation).
    Process(PathBuf),
}

/// UI-side client. Prefer [`HelperNftClient::unix_socket`] on product hosts.
///
/// Does **not** hold CAP_NET_ADMIN. Fail-closed on connect/IO/timeout/protocol.
#[derive(Debug, Clone)]
pub struct HelperNftClient {
    transport: HelperTransport,
    /// Optional absolute nft path (process transport only; UDS unit has its env).
    nft_bin: Option<PathBuf>,
    timeout: Duration,
}

impl HelperNftClient {
    /// Product path: connect to absolute UDS (socket-activated helper).
    pub fn unix_socket(sock: impl Into<PathBuf>) -> Self {
        Self {
            transport: HelperTransport::UnixSocket(sock.into()),
            nft_bin: None,
            timeout: DEFAULT_HELPER_TIMEOUT,
        }
    }

    /// Process spawn (hermetic tests; **not** viable under UI NoNewPrivileges).
    pub fn process(bin: impl Into<PathBuf>) -> Self {
        Self {
            transport: HelperTransport::Process(bin.into()),
            nft_bin: None,
            timeout: DEFAULT_HELPER_TIMEOUT,
        }
    }

    /// Backward-compatible alias for process transport (tests).
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self::process(bin)
    }

    pub fn with_nft_bin(mut self, nft_bin: Option<PathBuf>) -> Self {
        self.nft_bin = nft_bin;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn sock_path(&self) -> Option<&Path> {
        match &self.transport {
            HelperTransport::UnixSocket(p) => Some(p.as_path()),
            HelperTransport::Process(_) => None,
        }
    }

    pub fn bin(&self) -> Option<&Path> {
        match &self.transport {
            HelperTransport::Process(p) => Some(p.as_path()),
            HelperTransport::UnixSocket(_) => None,
        }
    }

    /// Run `add_ban` against the helper. Err means ban must not stick in memory.
    pub fn add_ban(&self, ip: IpAddr) -> Result<(), String> {
        self.roundtrip(&HelperRequest::add_ban(ip))
    }

    /// Run `remove_ban` against the helper (lab unban / cleanup).
    /// Absent element is treated as success by the helper apply path.
    pub fn remove_ban(&self, ip: IpAddr) -> Result<(), String> {
        self.roundtrip(&HelperRequest::remove_ban(ip))
    }

    pub fn ping(&self) -> Result<(), String> {
        self.roundtrip(&HelperRequest::ping())
    }

    fn roundtrip(&self, req: &HelperRequest) -> Result<(), String> {
        match &self.transport {
            HelperTransport::UnixSocket(sock) => self.roundtrip_unix(sock, req),
            HelperTransport::Process(bin) => self.roundtrip_process(bin, req),
        }
    }

    fn roundtrip_unix(&self, sock: &Path, req: &HelperRequest) -> Result<(), String> {
        if sock.as_os_str().is_empty() {
            return Err("nft helper sock path is empty (fail-closed)".into());
        }
        if !sock.is_absolute() {
            return Err(format!(
                "nft helper sock must be absolute (fail-closed); got {}",
                sock.display()
            ));
        }

        #[cfg(unix)]
        {
            use std::os::unix::net::UnixStream;
            let mut stream = UnixStream::connect(sock).map_err(|e| {
                format!(
                    "connect nft helper sock {}: {e} (fail-closed)",
                    sock.display()
                )
            })?;
            stream
                .set_read_timeout(Some(self.timeout))
                .map_err(|e| format!("helper sock read timeout set: {e}"))?;
            stream
                .set_write_timeout(Some(self.timeout))
                .map_err(|e| format!("helper sock write timeout set: {e}"))?;

            let payload = req.to_json_line()?;
            writeln!(stream, "{payload}")
                .map_err(|e| format!("write helper sock: {e} (fail-closed)"))?;
            stream
                .flush()
                .map_err(|e| format!("flush helper sock: {e}"))?;

            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .map_err(|e| format!("read helper sock: {e} (fail-closed)"))?;
            let resp = HelperResponse::from_json_line(&line)?;
            resp.into_result()
                .map_err(|e| format!("nft helper refused (fail-closed): {e}"))
        }

        #[cfg(not(unix))]
        {
            let _ = (sock, req);
            Err("nft helper UDS requires unix (fail-closed)".into())
        }
    }

    fn roundtrip_process(&self, bin: &Path, req: &HelperRequest) -> Result<(), String> {
        if bin.as_os_str().is_empty() {
            return Err("nft helper bin path is empty (fail-closed)".into());
        }
        if !bin.is_absolute() {
            return Err(format!(
                "nft helper bin must be absolute (fail-closed); got {}",
                bin.display()
            ));
        }
        if !bin.exists() {
            return Err(format!(
                "nft helper missing at {} (fail-closed)",
                bin.display()
            ));
        }

        let payload = req.to_json_line()?;
        let mut cmd = Command::new(bin);
        cmd.arg("apply-json")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(nft) = &self.nft_bin {
            cmd.env("SURMOUNT_BAN_NFT_BIN", nft);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("spawn nft helper {}: {e} (fail-closed)", bin.display()))?;

        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "helper stdin missing (fail-closed)".to_string())?;
            stdin
                .write_all(payload.as_bytes())
                .map_err(|e| format!("write helper stdin: {e}"))?;
        }

        // Best-effort wait with timeout via thread join (no wait_timeout on all targets).
        let timeout = self.timeout;
        let handle = std::thread::spawn(move || child.wait_with_output());
        let started = std::time::Instant::now();
        loop {
            if handle.is_finished() {
                break;
            }
            if started.elapsed() > timeout {
                return Err(format!(
                    "nft helper timed out after {timeout:?} (fail-closed)"
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = handle
            .join()
            .map_err(|_| "nft helper wait thread panicked (fail-closed)".to_string())?
            .map_err(|e| format!("wait nft helper: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        if let Ok(resp) = HelperResponse::from_json_line(&stdout) {
            return resp.into_result().map_err(|e| {
                format!(
                    "nft helper refused (fail-closed; exit {}): {e}",
                    output.status
                )
            });
        }

        if !output.status.success() {
            return Err(format!(
                "nft helper exited with {} (fail-closed); stderr={}",
                output.status,
                stderr.trim()
            ));
        }

        Err(format!(
            "nft helper returned unparseable stdout (fail-closed): {}",
            stdout.trim()
        ))
    }
}

/// Apply path used by [`crate::ban::NftBanBackend`] (direct nft args or helper).
pub trait BanNftApply: Send + Sync {
    fn add_ban_element(&self, ip: IpAddr) -> Result<(), String>;
    /// Optional lab/cleanup path. Default: fail with "unsupported" so callers
    /// that only ban (Enforce path) need not implement unban. Helper + NftExec
    /// apply override this.
    fn remove_ban_element(&self, ip: IpAddr) -> Result<(), String> {
        let _ = ip;
        Err("remove_ban_element not supported on this apply path".into())
    }
}

/// Adapts low-level [`NftExec`] (RecordingNftExec / ProcessNftExec) to apply.
pub struct NftExecApply<E: NftExec> {
    pub exec: E,
}

impl<E: NftExec> NftExecApply<E> {
    pub fn new(exec: E) -> Self {
        Self { exec }
    }

    #[cfg(test)]
    pub fn exec(&self) -> &E {
        &self.exec
    }
}

impl<E: NftExec> BanNftApply for NftExecApply<E> {
    fn add_ban_element(&self, ip: IpAddr) -> Result<(), String> {
        match self.exec.run_nft(&nft_add_ban_args(ip)) {
            Ok(()) => Ok(()),
            // Idempotent host drop: already-present is apply success (crash-window).
            Err(e) if crate::ban::nft_element_already_present(&e) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn remove_ban_element(&self, ip: IpAddr) -> Result<(), String> {
        match self.exec.run_nft(&nft_remove_ban_args(ip)) {
            Ok(()) => Ok(()),
            // Idempotent lab unban: already-absent is success.
            Err(e) if crate::ban::nft_element_already_absent(&e) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// No-op apply (nft sync disabled).
#[derive(Debug, Default)]
pub struct NoopNftApply;

impl BanNftApply for NoopNftApply {
    fn add_ban_element(&self, _ip: IpAddr) -> Result<(), String> {
        Ok(())
    }

    fn remove_ban_element(&self, _ip: IpAddr) -> Result<(), String> {
        Ok(())
    }
}

impl BanNftApply for HelperNftClient {
    fn add_ban_element(&self, ip: IpAddr) -> Result<(), String> {
        self.add_ban(ip)
    }

    fn remove_ban_element(&self, ip: IpAddr) -> Result<(), String> {
        self.remove_ban(ip)
    }
}

/// Resolve nft binary path for the helper process (absolute required when set).
pub fn helper_nft_bin_from_env(
    get: impl Fn(&str) -> Option<String>,
) -> Result<Option<PathBuf>, String> {
    match get("SURMOUNT_BAN_NFT_BIN") {
        None => Ok(None),
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                Ok(None)
            } else {
                let p = PathBuf::from(t);
                if !p.is_absolute() {
                    return Err(format!(
                        "SURMOUNT_BAN_NFT_BIN must be absolute when set (fail-closed); got {t:?}"
                    ));
                }
                Ok(Some(p))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ban::{RecordingNftExec, NFT_SET_BAN4, NFT_SET_BAN6, NFT_TABLE};
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::sync::Arc;

    fn ip4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn protocol_roundtrip_add_ban_json() {
        let req = HelperRequest::add_ban(ip4(203, 0, 113, 9));
        let line = req.to_json_line().unwrap();
        let back = HelperRequest::from_json_line(&line).unwrap();
        assert_eq!(back, req);
        let resp = HelperResponse::success();
        let rline = resp.to_json_line().unwrap();
        assert!(HelperResponse::from_json_line(&rline).unwrap().ok);
    }

    #[test]
    fn apply_add_ban_records_canonical_nft_args() {
        let exec = RecordingNftExec::default();
        let req = HelperRequest::add_ban(ip4(198, 51, 100, 7));
        let resp = apply_helper_request(&req, &exec);
        assert!(resp.ok, "{resp:?}");
        let calls = exec.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0][0], "add");
        assert_eq!(calls[0][3], NFT_TABLE);
        assert_eq!(calls[0][4], NFT_SET_BAN4);
        assert!(calls[0].iter().any(|a| a == "198.51.100.7"));
    }

    #[test]
    fn apply_add_ban_v6_uses_ban6_set() {
        let exec = RecordingNftExec::default();
        let ip6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2));
        let resp = apply_helper_request(&HelperRequest::add_ban(ip6), &exec);
        assert!(resp.ok);
        assert_eq!(exec.calls.lock().unwrap()[0][4], NFT_SET_BAN6);
    }

    #[test]
    fn apply_unknown_op_fail_closed_no_nft() {
        let exec = RecordingNftExec::default();
        let req = HelperRequest {
            op: "flush_table".into(),
            ip: None,
        };
        let resp = apply_helper_request(&req, &exec);
        assert!(!resp.ok);
        assert!(exec.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_add_ban_missing_ip_fail_closed() {
        let exec = RecordingNftExec::default();
        let req = HelperRequest {
            op: OP_ADD_BAN.into(),
            ip: None,
        };
        let resp = apply_helper_request(&req, &exec);
        assert!(!resp.ok);
        assert!(exec.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_nft_exec_failure_surfaces_ok_false() {
        struct Boom;
        impl NftExec for Boom {
            fn run_nft(&self, _: &[String]) -> Result<(), String> {
                Err("permission denied".into())
            }
        }
        let resp = apply_helper_request(&HelperRequest::add_ban(ip4(1, 2, 3, 4)), &Boom);
        assert!(!resp.ok);
        let err = resp.error.unwrap_or_default();
        assert!(
            err.contains("permission denied") || err.contains("nft"),
            "{err}"
        );
    }

    #[test]
    fn apply_add_ban_file_exists_is_success() {
        struct Exists;
        impl NftExec for Exists {
            fn run_nft(&self, _: &[String]) -> Result<(), String> {
                Err("Error: Could not process rule: File exists".into())
            }
        }
        let resp = apply_helper_request(&HelperRequest::add_ban(ip4(1, 2, 3, 4)), &Exists);
        assert!(resp.ok, "already-present must be apply success: {resp:?}");
    }

    #[test]
    fn apply_remove_ban_records_delete_element_args() {
        let exec = RecordingNftExec::default();
        let req = HelperRequest::remove_ban(ip4(198, 51, 100, 44));
        let resp = apply_helper_request(&req, &exec);
        assert!(resp.ok, "{resp:?}");
        let calls = exec.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0][0], "delete");
        assert_eq!(calls[0][1], "element");
        assert_eq!(calls[0][3], NFT_TABLE);
        assert_eq!(calls[0][4], NFT_SET_BAN4);
        assert!(calls[0].iter().any(|a| a == "198.51.100.44"));
    }

    #[test]
    fn apply_remove_ban_v6_uses_ban6_set() {
        let exec = RecordingNftExec::default();
        let ip6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 3));
        let resp = apply_helper_request(&HelperRequest::remove_ban(ip6), &exec);
        assert!(resp.ok);
        assert_eq!(exec.calls.lock().unwrap()[0][4], NFT_SET_BAN6);
    }

    #[test]
    fn apply_remove_ban_absent_element_is_success() {
        struct Absent;
        impl NftExec for Absent {
            fn run_nft(&self, _: &[String]) -> Result<(), String> {
                Err("Error: Could not process rule: No such file or directory".into())
            }
        }
        let resp = apply_helper_request(&HelperRequest::remove_ban(ip4(1, 2, 3, 4)), &Absent);
        assert!(resp.ok, "already-absent must be apply success: {resp:?}");
    }

    #[test]
    fn apply_remove_ban_missing_ip_fail_closed() {
        let exec = RecordingNftExec::default();
        let req = HelperRequest {
            op: OP_REMOVE_BAN.into(),
            ip: None,
        };
        let resp = apply_helper_request(&req, &exec);
        assert!(!resp.ok);
        assert!(exec.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn apply_remove_ban_real_error_fail_closed() {
        struct Boom;
        impl NftExec for Boom {
            fn run_nft(&self, _: &[String]) -> Result<(), String> {
                Err("permission denied".into())
            }
        }
        let resp = apply_helper_request(&HelperRequest::remove_ban(ip4(1, 2, 3, 4)), &Boom);
        assert!(!resp.ok);
        let err = resp.error.unwrap_or_default();
        assert!(
            err.contains("permission denied") || err.contains("nft"),
            "{err}"
        );
    }

    #[test]
    fn argv_add_ban_and_ping() {
        let r = request_from_argv(&["add-ban".into(), "203.0.113.1".into()]).unwrap();
        assert_eq!(r.op, OP_ADD_BAN);
        assert_eq!(r.ip.as_deref(), Some("203.0.113.1"));
        let p = request_from_argv(&["ping".into()]).unwrap();
        assert_eq!(p.op, OP_PING);
        let err = request_from_argv(&["add-ban".into()]).unwrap_err();
        assert!(err.contains("fail-closed"), "{err}");
    }

    #[test]
    fn argv_remove_ban_and_unban_alias() {
        let r = request_from_argv(&["remove-ban".into(), "203.0.113.9".into()]).unwrap();
        assert_eq!(r.op, OP_REMOVE_BAN);
        assert_eq!(r.ip.as_deref(), Some("203.0.113.9"));
        let u = request_from_argv(&["unban".into(), "198.51.100.1".into()]).unwrap();
        assert_eq!(u.op, OP_REMOVE_BAN);
        let err = request_from_argv(&["remove-ban".into()]).unwrap_err();
        assert!(err.contains("fail-closed"), "{err}");
    }

    #[test]
    fn argv_unknown_command_help_lists_remove_ban() {
        let err = request_from_argv(&["flush-all".into()]).unwrap_err();
        assert!(err.contains("fail-closed"), "{err}");
        assert!(
            err.contains("remove-ban") && err.contains("add-ban") && err.contains("ping"),
            "unknown-command hint must list remove-ban: {err}"
        );
    }

    #[test]
    fn protocol_roundtrip_remove_ban_json() {
        let req = HelperRequest::remove_ban(ip4(203, 0, 113, 11));
        let line = req.to_json_line().unwrap();
        let back = HelperRequest::from_json_line(&line).unwrap();
        assert_eq!(back, req);
        assert_eq!(back.op, OP_REMOVE_BAN);
    }

    #[test]
    fn nft_exec_apply_remove_ban_records_delete() {
        let apply = NftExecApply::new(RecordingNftExec::default());
        apply
            .remove_ban_element(ip4(203, 0, 113, 55))
            .expect("remove ok");
        let calls = apply.exec().calls.lock().unwrap();
        assert_eq!(calls[0][0], "delete");
        assert_eq!(calls[0][4], NFT_SET_BAN4);
    }

    #[test]
    fn helper_client_missing_bin_fail_closed() {
        let client = HelperNftClient::new("/nonexistent/surmount-nft-ban-helper-missing");
        let err = client.add_ban(ip4(203, 0, 113, 1)).unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("missing"),
            "{err}"
        );
    }

    #[test]
    fn helper_client_relative_bin_fail_closed() {
        let client = HelperNftClient::new("relative/helper");
        let err = client.add_ban(ip4(203, 0, 113, 1)).unwrap_err();
        assert!(
            err.contains("absolute") && err.contains("fail-closed"),
            "{err}"
        );
    }

    #[test]
    fn helper_client_fake_script_ok_and_refuse() {
        let dir = std::env::temp_dir().join(format!(
            "surmount-nft-helper-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ok_bin = dir.join("helper-ok");
        std::fs::write(&ok_bin, "#!/bin/sh\necho '{\"ok\":true}'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&ok_bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&ok_bin, perms).unwrap();
        }

        let client = HelperNftClient::new(&ok_bin);
        client.add_ban(ip4(203, 0, 113, 44)).unwrap();

        let bad_bin = dir.join("helper-bad");
        std::fs::write(
            &bad_bin,
            "#!/bin/sh\necho '{\"ok\":false,\"error\":\"simulated refuse\"}'\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bad_bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bad_bin, perms).unwrap();
        }
        let client_bad = HelperNftClient::new(&bad_bin);
        let err = client_bad.add_ban(ip4(203, 0, 113, 45)).unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("refuse") || err.contains("helper"),
            "{err}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Contract: helper refuse => memory must not stay banned (apply-first).
    #[test]
    fn enforce_helper_refuse_leaves_memory_unbanned() {
        use crate::ban::{BanBackend, BanReason, MemoryBanBackend, NftBanBackend};

        let dir = std::env::temp_dir().join(format!(
            "surmount-nft-helper-rb-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let bad_bin = dir.join("helper-refuse");
        std::fs::write(
            &bad_bin,
            "#!/bin/sh\necho '{\"ok\":false,\"error\":\"nft deny\"}'\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&bad_bin).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&bad_bin, perms).unwrap();
        }

        let memory = MemoryBanBackend::new(vec![]);
        let client = HelperNftClient::new(&bad_bin);
        let backend = NftBanBackend::new(memory, client, true);
        let ip = ip4(198, 51, 100, 99);
        let err = backend.ban(ip, BanReason::Unauthorized).unwrap_err();
        assert!(
            err.contains("helper") || err.contains("nft") || err.contains("refuse"),
            "{err}"
        );
        assert!(
            !backend.is_banned(ip),
            "memory must not stay banned after helper refuse"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn helper_client_unix_socket_roundtrip() {
        use std::os::unix::net::UnixListener;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = std::env::temp_dir().join(format!(
            "surmount-nft-uds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sock_path = dir.join("helper.sock");
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();
        let ready = Arc::new(AtomicBool::new(false));
        let ready2 = ready.clone();
        let join = std::thread::spawn(move || {
            ready2.store(true, Ordering::SeqCst);
            let (mut stream, _) = listener.accept().unwrap();
            let exec = RecordingNftExec::default();
            serve_helper_once(&mut stream, &exec).unwrap();
        });
        while !ready.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(1));
        }
        // Absolute path required.
        let abs = sock_path.canonicalize().unwrap_or(sock_path.clone());
        let client = HelperNftClient::unix_socket(&abs).with_timeout(Duration::from_secs(2));
        client.add_ban(ip4(203, 0, 113, 50)).unwrap();
        join.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn helper_client_timeout_fail_closed() {
        use std::os::unix::net::UnixListener;

        let dir = std::env::temp_dir().join(format!(
            "surmount-nft-timeout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let sock_path = dir.join("slow.sock");
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path).unwrap();
        let join = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            // Hold connection open without responding.
            std::thread::sleep(Duration::from_secs(3));
        });
        let abs = std::fs::canonicalize(&sock_path).unwrap_or(sock_path.clone());
        // Brief settle for listener.
        std::thread::sleep(Duration::from_millis(20));
        let client = HelperNftClient::unix_socket(&abs).with_timeout(Duration::from_millis(150));
        let err = client.add_ban(ip4(203, 0, 113, 1)).unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("timed") || err.contains("read"),
            "{err}"
        );
        let _ = join.join();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
