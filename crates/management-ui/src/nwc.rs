//! Nostr Wallet Connect (NIP-47) URI store for contributors.
//!
//! Login stays NIP-07 / NIP-98. NWC is a wallet connection, not login and not
//! a mailbox password. URIs live off git (Domain B path). Never nsec. Store
//! responses and logs never echo the URI or the wallet secret.
//!
//! See [NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md)
//! (accessed: 2026-08-20).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Host default when `SURMOUNT_NWC_STORE` is unset.
pub const DEFAULT_NWC_STORE_PATH: &str = "/var/lib/surmount/secrets/ui/nwc.json";

/// Current on-disk schema version.
pub const NWC_STORE_VERSION: u32 = 1;

/// Parsed NWC connection (never logged).
#[derive(Clone, PartialEq, Eq)]
pub struct ParsedNwc {
    /// Wallet pubkey as lowercase hex (64 chars).
    pub wallet_hex: String,
    /// First relay URL (ws/wss).
    pub relay: String,
    /// Shared secret as lowercase hex. Never Debug.
    secret_hex: String,
}

impl std::fmt::Debug for ParsedNwc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedNwc")
            .field("wallet_hex", &self.wallet_hex)
            .field("relay", &self.relay)
            .field("secret_hex", &"[redacted]")
            .finish()
    }
}

impl ParsedNwc {
    pub fn secret_len(&self) -> usize {
        self.secret_hex.len()
    }
}

/// Refuse nsec and garbage. Accept `nostr+walletconnect://` with relay + secret.
pub fn parse_nwc_uri(raw: &str) -> Result<ParsedNwc, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(error_without_echo(
            raw,
            "NWC URI required (nostr+walletconnect://).",
        ));
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("nsec1") {
        return Err(error_without_echo(
            raw,
            "nsec is refused. Paste a nostr+walletconnect URI, never nsec.",
        ));
    }
    const SCHEME: &str = "nostr+walletconnect://";
    if !lower.starts_with(SCHEME) {
        return Err(error_without_echo(
            raw,
            "Invalid NWC URI (expected nostr+walletconnect://).",
        ));
    }
    let rest = &trimmed[SCHEME.len()..];
    let (host, query) = rest
        .split_once('?')
        .ok_or_else(|| error_without_echo(raw, "Invalid NWC URI (missing relay and secret)."))?;
    let wallet_hex = normalize_wallet_hex(host).map_err(|m| error_without_echo(raw, &m))?;
    let mut relay: Option<String> = None;
    let mut secret: Option<String> = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        let key = percent_decode(k).to_ascii_lowercase();
        let val = percent_decode(v);
        match key.as_str() {
            "relay" => relay = Some(val),
            "secret" => secret = Some(val),
            _ => {}
        }
    }
    let relay = relay.unwrap_or_default();
    let relay_l = relay.to_ascii_lowercase();
    if !(relay_l.starts_with("ws://") || relay_l.starts_with("wss://")) {
        return Err(error_without_echo(
            raw,
            "Invalid NWC URI (relay must be ws or wss).",
        ));
    }
    let secret_raw = secret.unwrap_or_default();
    if secret_raw.to_ascii_lowercase().contains("nsec1") {
        return Err(error_without_echo(
            raw,
            "nsec is refused. Paste a nostr+walletconnect URI, never nsec.",
        ));
    }
    let secret_hex = normalize_secret_hex(&secret_raw).map_err(|m| error_without_echo(raw, &m))?;
    Ok(ParsedNwc {
        wallet_hex,
        relay,
        secret_hex,
    })
}

fn error_without_echo(_raw: &str, msg: &str) -> String {
    msg.to_string()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn normalize_wallet_hex(host: &str) -> Result<String, String> {
    let h = host.trim().to_ascii_lowercase();
    if h.len() != 64 || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid NWC URI (wallet pubkey must be 64 hex characters).".into());
    }
    Ok(h)
}

fn normalize_secret_hex(secret: &str) -> Result<String, String> {
    let h = secret.trim().to_ascii_lowercase();
    if h.len() < 32 || !h.len().is_multiple_of(2) || !h.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid NWC URI (secret must be even-length hex).".into());
    }
    Ok(h)
}

/// One stored connection. `uri` is never serialized into HTML or JSON APIs.
/// Debug never includes the URI (logs must not echo it).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NwcEntry {
    mailbox: String,
    uri: String,
}

impl std::fmt::Debug for NwcEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NwcEntry")
            .field("mailbox", &self.mailbox)
            .field("uri", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct NwcFile {
    version: u32,
    #[serde(default)]
    connections: Vec<NwcEntry>,
}

impl Default for NwcFile {
    fn default() -> Self {
        Self {
            version: NWC_STORE_VERSION,
            connections: Vec::new(),
        }
    }
}

/// Whether this mailbox has a stored NWC URI (never the value).
pub fn nwc_connected(path: &Path, mailbox: &str) -> Result<bool, String> {
    let file = load_nwc_store(path)?;
    let want = mailbox.trim().to_ascii_lowercase();
    Ok(file
        .connections
        .iter()
        .any(|e| e.mailbox.eq_ignore_ascii_case(&want)))
}

/// Save a validated NWC URI for `mailbox`. Overwrites any previous URI.
pub fn save_nwc_uri(path: &Path, mailbox: &str, uri: &str) -> Result<(), String> {
    let parsed = parse_nwc_uri(uri)?;
    let _ = parsed;
    let mailbox = mailbox.trim().to_ascii_lowercase();
    if mailbox.is_empty() || !mailbox.contains('@') {
        return Err("mailbox address required (user@domain)".into());
    }
    update_nwc_store(path, |file| {
        file.connections.retain(|e| e.mailbox != mailbox);
        file.connections.push(NwcEntry {
            mailbox,
            uri: uri.trim().to_string(),
        });
        Ok(())
    })?;
    Ok(())
}

/// Remove the stored URI for `mailbox` (no-op if missing).
pub fn clear_nwc_uri(path: &Path, mailbox: &str) -> Result<(), String> {
    let mailbox = mailbox.trim().to_ascii_lowercase();
    update_nwc_store(path, |file| {
        file.connections.retain(|e| e.mailbox != mailbox);
        Ok(())
    })?;
    Ok(())
}

pub fn nwc_store_path_from_env() -> PathBuf {
    nwc_store_path_from_raw(std::env::var("SURMOUNT_NWC_STORE").ok().as_deref())
}

pub fn nwc_store_path_from_raw(raw: Option<&str>) -> PathBuf {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => PathBuf::from(s),
        None => PathBuf::from(DEFAULT_NWC_STORE_PATH),
    }
}

fn load_nwc_store(path: &Path) -> Result<NwcFile, String> {
    if !path.exists() {
        return Ok(NwcFile::default());
    }
    refuse_insecure_inode(path)?;
    let mut f = File::open(path).map_err(|e| format!("open nwc store: {e}"))?;
    let mut raw = String::new();
    f.read_to_string(&mut raw)
        .map_err(|e| format!("read nwc store: {e}"))?;
    if raw.trim().is_empty() {
        return Ok(NwcFile::default());
    }
    let file: NwcFile = serde_json::from_str(&raw).map_err(|e| format!("parse nwc store: {e}"))?;
    if file.version != NWC_STORE_VERSION {
        return Err(format!(
            "nwc store version {} unsupported (expected {NWC_STORE_VERSION})",
            file.version
        ));
    }
    Ok(file)
}

fn update_nwc_store<F>(path: &Path, edit: F) -> Result<NwcFile, String>
where
    F: FnOnce(&mut NwcFile) -> Result<(), String>,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create nwc store dir: {e}"))?;
    }
    let lock = lock_nwc_store(path)?;
    let mut data = load_nwc_store(path)?;
    edit(&mut data)?;
    save_nwc_store_locked(path, &data, &lock)?;
    Ok(data)
}

fn lock_nwc_store(path: &Path) -> Result<File, String> {
    let lock_path = lock_path_for(path);
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(&lock_path)
        .map_err(|e| format!("open nwc store lock: {e}"))?;
    let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return Err(format!(
            "flock nwc store failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(f)
}

fn save_nwc_store_locked(path: &Path, data: &NwcFile, _lock: &File) -> Result<(), String> {
    let encoded = serde_json::to_string_pretty(data).map_err(|e| format!("encode nwc: {e}"))?;
    let tmp = tmp_path_for(path);
    {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("create nwc store temp: {e}"))?;
        out.write_all(encoded.as_bytes())
            .map_err(|e| format!("write nwc store temp: {e}"))?;
        out.sync_all()
            .map_err(|e| format!("fsync nwc store temp: {e}"))?;
    }
    let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename nwc store: {e}")
    })?;
    Ok(())
}

fn lock_path_for(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_os_string();
    p.push(".lock");
    PathBuf::from(p)
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("nwc.json");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{name}.{}.{nanos}.tmp", std::process::id()))
}

fn refuse_insecure_inode(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("stat nwc store: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err("nwc store path is a symlink (refused)".into());
    }
    if !meta.file_type().is_file() {
        return Err("nwc store path is not a regular file (refused)".into());
    }
    let mode = meta.permissions().mode();
    if mode & 0o002 != 0 {
        return Err("nwc store path is world-writable (refused)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn valid_uri() -> String {
        format!(
            "nostr+walletconnect://{}?relay=wss%3A%2F%2Frelay.example.test&secret={}",
            "ab".repeat(32),
            "cd".repeat(32)
        )
    }

    fn scratch_path() -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("surmount-nwc-{}-{}", std::process::id(), n));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("nwc.json")
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    /// Named contract: valid nostr+walletconnect URI is accepted.
    #[test]
    fn nwc_uri_valid_accepted() {
        let parsed = parse_nwc_uri(&valid_uri()).expect("valid NWC URI");
        assert_eq!(parsed.wallet_hex, "ab".repeat(32));
        assert!(parsed.relay.contains("relay.example.test"));
        assert_eq!(parsed.secret_len(), 64);
        let dbg = format!("{parsed:?}");
        assert!(
            !dbg.contains("cd".repeat(32).as_str()),
            "Debug must not echo the wallet secret"
        );
    }

    /// Named contract: NwcEntry Debug never echoes the URI or wallet secret.
    #[test]
    fn nwc_entry_debug_does_not_contain_uri_or_secret() {
        let uri = valid_uri();
        let secret = "cd".repeat(32);
        let entry = NwcEntry {
            mailbox: "person@example.test".into(),
            uri: uri.clone(),
        };
        let dbg = format!("{entry:?}");
        assert!(
            !dbg.contains(&uri),
            "NwcEntry Debug must not include the URI: {dbg}"
        );
        assert!(
            !dbg.contains("nostr+walletconnect"),
            "NwcEntry Debug must not include the NWC scheme: {dbg}"
        );
        assert!(
            !dbg.contains(&secret),
            "NwcEntry Debug must not include the wallet secret: {dbg}"
        );
        assert!(
            dbg.contains("person@example.test"),
            "mailbox may stay in Debug: {dbg}"
        );
        let file = NwcFile {
            version: NWC_STORE_VERSION,
            connections: vec![entry],
        };
        let file_dbg = format!("{file:?}");
        assert!(
            !file_dbg.contains(&uri) && !file_dbg.contains(&secret),
            "NwcFile Debug must not leak URI or secret: {file_dbg}"
        );
    }

    /// Named contract: nsec and garbage are refused without echo.
    #[test]
    fn nwc_uri_nsec_and_garbage_refused_without_echo() {
        let nsec = concat!(
            "nsec",
            "1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq"
        )
        .to_string();
        let err = parse_nwc_uri(&nsec).unwrap_err();
        let err_l = err.to_ascii_lowercase();
        assert!(
            err_l.contains("nsec") || err_l.contains("refused") || err_l.contains("invalid"),
            "nsec must be refused: {err}"
        );
        assert!(!err.contains(&nsec), "error must not echo the nsec: {err}");

        let pasted_nsec_in_uri = format!(
            "nostr+walletconnect://{}?relay=wss://relay.example.test&secret={nsec}",
            "ab".repeat(32)
        );
        let err2 = parse_nwc_uri(&pasted_nsec_in_uri).unwrap_err();
        assert!(
            !err2.contains(&nsec),
            "error must not echo nsec from a fake URI: {err2}"
        );

        let garbage = "https://example.test/not-nwc";
        let err3 = parse_nwc_uri(garbage).unwrap_err();
        assert!(
            !err3.contains(garbage),
            "error must not echo garbage URI: {err3}"
        );
        assert!(
            err3.to_ascii_lowercase().contains("invalid")
                || err3.to_ascii_lowercase().contains("nwc")
                || err3.to_ascii_lowercase().contains("walletconnect"),
            "garbage must be refused: {err3}"
        );
    }

    #[test]
    fn nwc_store_round_trip_without_echoing_uri_in_connected_flag() {
        let path = scratch_path();
        let uri = valid_uri();
        save_nwc_uri(&path, "person@example.test", &uri).expect("save");
        assert!(nwc_connected(&path, "PERSON@example.test").unwrap());
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("person@example.test"));
        // Disk holds the URI (Domain B). Tests may see it on disk; APIs must not.
        clear_nwc_uri(&path, "person@example.test").unwrap();
        assert!(!nwc_connected(&path, "person@example.test").unwrap());
        cleanup(&path);
        let _ = uri;
    }
}
