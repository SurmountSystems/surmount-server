//! Surmount-owned console account map: optional npub plus Administrator/User.
//!
//! Path default: `/var/lib/surmount/console/accounts.json` (owner surmount-ui,
//! mode 0600). Tests override with `SURMOUNT_CONSOLE_ACCOUNTS`. This file is
//! not the host Nostr allowlist. User npubs must not be copied there.
//! nsec is never stored.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::allowlist_contains;

/// Host default when `SURMOUNT_CONSOLE_ACCOUNTS` is unset.
pub const DEFAULT_CONSOLE_ACCOUNTS_PATH: &str = "/var/lib/surmount/console/accounts.json";

/// Current on-disk schema version.
pub const CONSOLE_ACCOUNTS_VERSION: u32 = 1;

/// Console permission: Administrator (host operators) or User (own mailbox).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleRole {
    Administrator,
    User,
}

impl ConsoleRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Administrator => "administrator",
            Self::User => "user",
        }
    }

    /// Parse `administrator` / `user` (case-insensitive). Empty defaults to User.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "user" => Ok(Self::User),
            "administrator" | "admin" => Ok(Self::Administrator),
            other => Err(format!(
                "console role {other:?} invalid (expected administrator or user)"
            )),
        }
    }
}

/// One map row. Mailbox and npub are independently optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleAccountEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<String>,
    /// Lowercase hex pubkey. Never nsec. None means IMAP/SMTP only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub npub_hex: Option<String>,
    pub role: ConsoleRole,
    /// Extra addresses on the same person. Not extra portal rows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// True after a portal password set. Never stores the secret. Yes/no only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub password_set: bool,
}

impl ConsoleAccountEntry {
    pub fn mailbox_normalized(&self) -> Option<String> {
        self.mailbox
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
    }

    pub fn npub_normalized(&self) -> Option<String> {
        self.npub_hex
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
    }
}

/// Versioned JSON list stored on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleAccountFile {
    pub version: u32,
    #[serde(default)]
    pub accounts: Vec<ConsoleAccountEntry>,
}

impl Default for ConsoleAccountFile {
    fn default() -> Self {
        Self {
            version: CONSOLE_ACCOUNTS_VERSION,
            accounts: Vec::new(),
        }
    }
}

impl ConsoleAccountFile {
    pub fn by_npub(&self, hex: &str) -> Option<&ConsoleAccountEntry> {
        let want = hex.trim().to_ascii_lowercase();
        if want.is_empty() {
            return None;
        }
        self.accounts
            .iter()
            .find(|e| e.npub_normalized().as_deref() == Some(want.as_str()))
    }

    pub fn by_mailbox(&self, address: &str) -> Option<&ConsoleAccountEntry> {
        let want = address.trim().to_ascii_lowercase();
        if want.is_empty() {
            return None;
        }
        self.accounts
            .iter()
            .find(|e| e.mailbox_normalized().as_deref() == Some(want.as_str()))
    }

    pub fn has_npub(&self, hex: &str) -> bool {
        self.by_npub(hex).is_some()
    }

    /// Insert or replace the row for `mailbox`. Rejects a npub already on another row.
    pub fn upsert_mailbox(
        &mut self,
        mailbox: &str,
        npub_hex: Option<String>,
        role: ConsoleRole,
    ) -> Result<(), String> {
        let mailbox = mailbox.trim().to_ascii_lowercase();
        if mailbox.is_empty() || !mailbox.contains('@') {
            return Err("mailbox address required (user@domain)".into());
        }
        let npub = match npub_hex {
            Some(s) if !s.trim().is_empty() => Some(s.trim().to_ascii_lowercase()),
            _ => None,
        };
        if let Some(hex) = npub.as_deref()
            && let Some(existing) = self.by_npub(hex)
            && existing.mailbox_normalized().as_deref() != Some(mailbox.as_str())
        {
            return Err("npub already bound to another mailbox".into());
        }
        if let Some(row) = self
            .accounts
            .iter_mut()
            .find(|e| e.mailbox_normalized().as_deref() == Some(mailbox.as_str()))
        {
            row.mailbox = Some(mailbox);
            row.npub_hex = npub;
            row.role = role;
            return Ok(());
        }
        self.accounts.push(ConsoleAccountEntry {
            mailbox: Some(mailbox),
            npub_hex: npub,
            role,
            aliases: Vec::new(),
            password_set: false,
        });
        Ok(())
    }

    /// Record that a mailbox password was set via the portal (never the secret).
    pub fn mark_password_set(&mut self, mailbox: &str) {
        let mailbox = mailbox.trim().to_ascii_lowercase();
        if mailbox.is_empty() || !mailbox.contains('@') {
            return;
        }
        if let Some(row) = self
            .accounts
            .iter_mut()
            .find(|e| e.mailbox_normalized().as_deref() == Some(mailbox.as_str()))
        {
            row.password_set = true;
            return;
        }
        self.accounts.push(ConsoleAccountEntry {
            mailbox: Some(mailbox),
            npub_hex: None,
            role: ConsoleRole::User,
            aliases: Vec::new(),
            password_set: true,
        });
    }
}

/// Resolved console identity for the current request (not stored in the cookie).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPrincipal {
    pub hex_pubkey: String,
    pub role: Option<ConsoleRole>,
    pub mailbox: Option<String>,
}

impl RequestPrincipal {
    pub fn is_administrator(&self) -> bool {
        matches!(self.role, Some(ConsoleRole::Administrator))
    }

    pub fn bound_mailbox(&self) -> Option<&str> {
        self.mailbox.as_deref()
    }
}

/// Accept login when the pubkey is on the host allowlist or on a map row.
pub fn console_login_accepted(
    hex: &str,
    allowlist: &HashSet<String>,
    file: &ConsoleAccountFile,
) -> bool {
    allowlist_contains(allowlist, hex) || file.has_npub(hex)
}

/// Role from the map when present; else Administrator for a live allowlist key.
///
/// Cookie identity is separate (Q-AUTH-1 does not re-check the allowlist for
/// the session). Callers that only have a leftover cookie and neither a map
/// row nor a live allowlist key get `None` so a revoked User cannot become
/// Administrator.
pub fn resolve_console_role(
    hex: &str,
    allowlist: &HashSet<String>,
    file: &ConsoleAccountFile,
) -> Option<ConsoleRole> {
    if let Some(entry) = file.by_npub(hex) {
        return Some(entry.role);
    }
    if allowlist_contains(allowlist, hex) {
        return Some(ConsoleRole::Administrator);
    }
    None
}

pub fn resolve_console_principal(
    hex: &str,
    allowlist: &HashSet<String>,
    file: &ConsoleAccountFile,
) -> RequestPrincipal {
    let hex = hex.trim().to_ascii_lowercase();
    let mailbox = file.by_npub(&hex).and_then(|e| e.mailbox_normalized());
    RequestPrincipal {
        role: resolve_console_role(&hex, allowlist, file),
        mailbox,
        hex_pubkey: hex,
    }
}

/// Load the map. Missing file is an empty map. Insecure inode is an error.
pub fn load_console_accounts(path: &Path) -> Result<ConsoleAccountFile, String> {
    if !path_exists_nofollow(path) {
        return Ok(ConsoleAccountFile::default());
    }
    refuse_insecure_inode(path)?;
    let mut file = File::open(path).map_err(|e| format!("open console accounts: {e}"))?;
    let mut raw = String::new();
    file.read_to_string(&mut raw)
        .map_err(|e| format!("read console accounts: {e}"))?;
    parse_console_accounts_json(&raw)
}

/// Parse JSON text (tests + load). Refuses nsec and unknown versions.
pub fn parse_console_accounts_json(raw: &str) -> Result<ConsoleAccountFile, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(ConsoleAccountFile::default());
    }
    if trimmed.to_ascii_lowercase().contains("nsec1") {
        return Err("console accounts file must not contain nsec".into());
    }
    let parsed: ConsoleAccountFile =
        serde_json::from_str(trimmed).map_err(|e| format!("console accounts JSON invalid: {e}"))?;
    if parsed.version != CONSOLE_ACCOUNTS_VERSION {
        return Err(format!(
            "console accounts version {} unsupported (expected {CONSOLE_ACCOUNTS_VERSION})",
            parsed.version
        ));
    }
    let mut seen = HashSet::new();
    for entry in &parsed.accounts {
        if let Some(hex) = entry.npub_normalized() {
            if !seen.insert(hex.clone()) {
                return Err("console accounts file has a duplicate npub".into());
            }
            if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err("console accounts npub must be 64 lowercase hex chars".into());
            }
        }
    }
    Ok(parsed)
}

/// Atomic save: temp file in the same directory, then rename. Mode 0600.
pub fn save_console_accounts(path: &Path, data: &ConsoleAccountFile) -> Result<(), String> {
    let lock = lock_console_accounts(path)?;
    save_console_accounts_locked(path, data, &lock)
}

/// Load, apply `edit`, save under one exclusive lock (re-check unique npub
/// inside `edit` / `upsert_mailbox` while the lock is held).
pub fn update_console_accounts<F>(path: &Path, edit: F) -> Result<ConsoleAccountFile, String>
where
    F: FnOnce(&mut ConsoleAccountFile) -> Result<(), String>,
{
    let lock = lock_console_accounts(path)?;
    let mut data = load_console_accounts(path)?;
    edit(&mut data)?;
    save_console_accounts_locked(path, &data, &lock)?;
    Ok(data)
}

fn lock_console_accounts(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|e| format!("create console accounts parent: {e}"))?;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    let lock_path = lock_path_for(path);
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(&lock_path)
        .map_err(|e| format!("open console accounts lock: {e}"))?;
    flock_exclusive(&lock)?;
    Ok(lock)
}

fn save_console_accounts_locked(
    path: &Path,
    data: &ConsoleAccountFile,
    _lock: &File,
) -> Result<(), String> {
    if data.version != CONSOLE_ACCOUNTS_VERSION {
        return Err("refusing to write unsupported console accounts version".into());
    }
    let encoded = serde_json::to_string_pretty(data)
        .map_err(|e| format!("serialize console accounts: {e}"))?;
    if encoded.to_ascii_lowercase().contains("nsec1") {
        return Err("refusing to write nsec into console accounts".into());
    }
    if path_exists_nofollow(path) {
        refuse_insecure_inode(path)?;
    }
    let tmp = tmp_path_for(path);
    if path_exists_nofollow(&tmp) {
        let meta =
            fs::symlink_metadata(&tmp).map_err(|e| format!("stat console accounts temp: {e}"))?;
        if meta.file_type().is_symlink() || meta.file_type().is_dir() {
            return Err("console accounts temp path is not a regular file".into());
        }
        let _ = fs::remove_file(&tmp);
    }
    {
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| format!("create console accounts temp: {e}"))?;
        out.write_all(encoded.as_bytes())
            .map_err(|e| format!("write console accounts temp: {e}"))?;
        out.sync_all()
            .map_err(|e| format!("fsync console accounts temp: {e}"))?;
    }
    let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("rename console accounts: {e}")
    })?;
    Ok(())
}

pub fn console_accounts_path_from_env() -> PathBuf {
    console_accounts_path_from_raw(std::env::var("SURMOUNT_CONSOLE_ACCOUNTS").ok().as_deref())
}

/// Resolve the map path: non-empty override wins; else the host default.
pub fn console_accounts_path_from_raw(raw: Option<&str>) -> PathBuf {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => PathBuf::from(s),
        None => PathBuf::from(DEFAULT_CONSOLE_ACCOUNTS_PATH),
    }
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
        .unwrap_or("accounts.json");
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!(".{name}.{}.{nanos}.tmp", std::process::id()))
}

fn path_exists_nofollow(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Refuse world-writable files, directories, and symlinks at `path` (lstat).
pub fn refuse_insecure_inode(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("stat console accounts: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err("console accounts path is a symlink (refused)".into());
    }
    if meta.file_type().is_dir() {
        return Err("console accounts path is a directory (refused)".into());
    }
    if !meta.file_type().is_file() {
        return Err("console accounts path is not a regular file (refused)".into());
    }
    let mode = meta.permissions().mode();
    if mode & 0o002 != 0 {
        return Err("console accounts path is world-writable (refused)".into());
    }
    Ok(())
}

fn flock_exclusive(file: &File) -> Result<(), String> {
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    if rc != 0 {
        return Err(format!(
            "flock exclusive failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn scratch_path() -> PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "surmount-console-accounts-{}-{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("accounts.json")
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn empty_and_missing_file_is_empty_map() {
        let path = scratch_path();
        let loaded = load_console_accounts(&path).unwrap();
        assert_eq!(loaded.version, 1);
        assert!(loaded.accounts.is_empty());
        cleanup(&path);
    }

    #[test]
    fn save_round_trip_unique_npub_and_roles() {
        let path = scratch_path();
        let mut data = ConsoleAccountFile::default();
        data.upsert_mailbox(
            "person@example.test",
            Some("aa".repeat(32)),
            ConsoleRole::User,
        )
        .unwrap();
        data.upsert_mailbox("ops@example.test", None, ConsoleRole::Administrator)
            .unwrap();
        save_console_accounts(&path, &data).unwrap();
        let meta = fs::symlink_metadata(&path).unwrap();
        assert!(meta.file_type().is_file());
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);

        let loaded = load_console_accounts(&path).unwrap();
        assert_eq!(loaded.accounts.len(), 2);
        assert_eq!(
            loaded.by_mailbox("PERSON@example.test").unwrap().role,
            ConsoleRole::User
        );
        assert_eq!(
            loaded.by_npub(&"AA".repeat(32)).unwrap().mailbox.as_deref(),
            Some("person@example.test")
        );
        assert!(
            loaded
                .by_mailbox("ops@example.test")
                .unwrap()
                .npub_hex
                .is_none()
        );
        cleanup(&path);
    }

    #[test]
    fn duplicate_npub_is_rejected() {
        let mut data = ConsoleAccountFile::default();
        data.upsert_mailbox("one@example.test", Some("bb".repeat(32)), ConsoleRole::User)
            .unwrap();
        let err = data
            .upsert_mailbox("two@example.test", Some("bb".repeat(32)), ConsoleRole::User)
            .unwrap_err();
        assert!(err.to_ascii_lowercase().contains("already bound"), "{err}");
    }

    #[test]
    fn two_imap_only_rows_share_empty_npub() {
        let mut data = ConsoleAccountFile::default();
        data.upsert_mailbox("a@example.test", None, ConsoleRole::User)
            .unwrap();
        data.upsert_mailbox("b@example.test", None, ConsoleRole::User)
            .unwrap();
        assert_eq!(data.accounts.len(), 2);
    }

    #[test]
    fn same_mailbox_upsert_replaces_npub_and_role() {
        let mut data = ConsoleAccountFile::default();
        data.upsert_mailbox("hunter@example.test", None, ConsoleRole::User)
            .unwrap();
        data.upsert_mailbox(
            "hunter@example.test",
            Some("cc".repeat(32)),
            ConsoleRole::Administrator,
        )
        .unwrap();
        assert_eq!(data.accounts.len(), 1);
        let row = data.by_mailbox("hunter@example.test").unwrap();
        assert_eq!(row.role, ConsoleRole::Administrator);
        assert_eq!(row.npub_normalized(), Some("cc".repeat(32)));
    }

    #[test]
    fn parse_refuses_nsec_and_bad_version() {
        let err = parse_console_accounts_json(
            r#"{"version":1,"accounts":[{"mailbox":"a@b.test","npub_hex":"nsec1abc","role":"user"}]}"#,
        )
        .unwrap_err();
        assert!(err.contains("nsec"), "{err}");
        let err = parse_console_accounts_json(r#"{"version":99,"accounts":[]}"#).unwrap_err();
        assert!(err.contains("unsupported"), "{err}");
    }

    #[test]
    fn role_parse_defaults_to_user() {
        assert_eq!(ConsoleRole::parse("").unwrap(), ConsoleRole::User);
        assert_eq!(ConsoleRole::parse("User").unwrap(), ConsoleRole::User);
        assert_eq!(
            ConsoleRole::parse("administrator").unwrap(),
            ConsoleRole::Administrator
        );
        assert!(ConsoleRole::parse("viewer").is_err());
    }

    #[test]
    fn allowlist_or_map_login_and_role() {
        let hex_admin = "dd".repeat(32);
        let hex_user = "ee".repeat(32);
        let hex_unknown = "ff".repeat(32);
        let mut allow = HashSet::new();
        allow.insert(hex_admin.clone());
        let mut file = ConsoleAccountFile::default();
        file.upsert_mailbox(
            "user@example.test",
            Some(hex_user.clone()),
            ConsoleRole::User,
        )
        .unwrap();

        assert!(console_login_accepted(&hex_admin, &allow, &file));
        assert!(console_login_accepted(&hex_user, &allow, &file));
        assert!(!console_login_accepted(&hex_unknown, &allow, &file));

        assert_eq!(
            resolve_console_role(&hex_admin, &allow, &file),
            Some(ConsoleRole::Administrator)
        );
        assert_eq!(
            resolve_console_role(&hex_user, &allow, &file),
            Some(ConsoleRole::User)
        );
        assert_eq!(resolve_console_role(&hex_unknown, &allow, &file), None);

        // Map wins when the same key is also allowlisted.
        file.upsert_mailbox(
            "demoted@example.test",
            Some(hex_admin.clone()),
            ConsoleRole::User,
        )
        .unwrap();
        assert_eq!(
            resolve_console_role(&hex_admin, &allow, &file),
            Some(ConsoleRole::User)
        );
    }

    #[test]
    fn refuse_world_writable_and_directory_and_symlink() {
        let path = scratch_path();
        let parent = path.parent().unwrap();

        fs::write(&path, "{\"version\":1,\"accounts\":[]}").unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o666);
        fs::set_permissions(&path, perms).unwrap();
        let err = load_console_accounts(&path).unwrap_err();
        assert!(err.contains("world-writable"), "{err}");
        let _ = fs::remove_file(&path);

        fs::create_dir(&path).unwrap();
        let err = load_console_accounts(&path).unwrap_err();
        assert!(err.contains("directory"), "{err}");
        fs::remove_dir(&path).unwrap();

        let target = parent.join("target.json");
        fs::write(&target, "{\"version\":1,\"accounts\":[]}").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let err = load_console_accounts(&path).unwrap_err();
        assert!(err.contains("symlink"), "{err}");
        cleanup(&path);
    }

    #[test]
    fn update_holds_lock_across_load_edit_save() {
        use std::thread;
        use std::time::Duration;

        let path = scratch_path();
        save_console_accounts(&path, &ConsoleAccountFile::default()).unwrap();
        let p1 = path.clone();
        let p2 = path.clone();
        let t1 = thread::spawn(move || {
            update_console_accounts(&p1, |file| {
                thread::sleep(Duration::from_millis(80));
                file.upsert_mailbox("one@example.test", Some("11".repeat(32)), ConsoleRole::User)
            })
        });
        thread::sleep(Duration::from_millis(15));
        let t2 = thread::spawn(move || {
            update_console_accounts(&p2, |file| {
                file.upsert_mailbox("two@example.test", Some("22".repeat(32)), ConsoleRole::User)
            })
        });
        t1.join().expect("thread one").expect("update one");
        t2.join().expect("thread two").expect("update two");
        let loaded = load_console_accounts(&path).unwrap();
        assert_eq!(
            loaded.accounts.len(),
            2,
            "exclusive lock across load+edit+save must keep both upserts"
        );
        cleanup(&path);
    }

    #[test]
    fn env_path_helper_defaults_when_unset() {
        // Named contract: missing override uses the host default, not a secret.
        assert_eq!(
            console_accounts_path_from_raw(None),
            PathBuf::from(DEFAULT_CONSOLE_ACCOUNTS_PATH)
        );
        assert_eq!(
            console_accounts_path_from_raw(Some("   ")),
            PathBuf::from(DEFAULT_CONSOLE_ACCOUNTS_PATH)
        );
        assert_eq!(
            console_accounts_path_from_raw(Some("/tmp/surmount-console-accounts-fixture.json")),
            PathBuf::from("/tmp/surmount-console-accounts-fixture.json")
        );
    }
}
