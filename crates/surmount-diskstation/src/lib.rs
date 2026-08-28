//! Office DiskStation AFP, mDNS discover, MailPlus uid copy.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug)]
pub struct DsError {
    pub message: String,
}

impl std::fmt::Display for DsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DsError {}

impl DsError {
    pub fn new(m: impl Into<String>) -> Self {
        Self { message: m.into() }
    }
}

pub fn validate_host_id(id: &str) -> Result<(), DsError> {
    match id {
        "DS1513" | "DS3018xs" => Ok(()),
        "diskstation" | "DiskStation" | "DISKSTATION" => Err(DsError::new(format!(
            "--host {id} is the old generic id. Re-store under DS1513 (5-bay) or DS3018xs (6-bay), then pass that --host"
        ))),
        _ => Err(DsError::new(format!(
            "--host must be DS1513 or DS3018xs (got {id})"
        ))),
    }
}

pub fn is_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    parts.iter().all(|p| {
        p.parse::<u32>()
            .ok()
            .map(|n| n <= 255 && !p.is_empty() && p.len() <= 3)
            .unwrap_or(false)
    })
}

pub fn strip_ipv4(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if let Some(len) = ipv4_span(&chars[i..]) {
            out.push_str("[redacted]");
            i += len;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn ipv4_span(chars: &[char]) -> Option<usize> {
    let mut i = 0;
    for oct in 0..4 {
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i == start || i - start > 3 {
            return None;
        }
        if oct == 3 {
            return Some(i);
        }
        if i < chars.len() && chars[i] == '.' {
            i += 1;
        } else {
            return None;
        }
    }
    None
}

pub fn contains_ipv4(s: &str) -> bool {
    strip_ipv4(s) != s && s.chars().any(|c| c.is_ascii_digit())
}

pub fn read_afp_host_hint(file: &Path, want: &str) -> Option<String> {
    let text = std::fs::read_to_string(file).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.split('#').next().unwrap_or("").trim();
            if (k == "DS1513" || k == "DS3018xs") && k == want && !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

pub fn gvfs_dir_host(base: &str) -> Option<String> {
    base.split("host=")
        .nth(1)
        .map(|r| r.split(',').next().unwrap_or(r).to_string())
}

pub fn reconstruct_maildir_name(name: &str) -> String {
    if let Some((left, right)) = name.split_once("/2,") {
        format!("{left}:2,{right}")
    } else {
        name.to_string()
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let cand = Path::new(dir).join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

pub fn run_capture(bin: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match out {
        Ok(o) => (
            o.status.code().unwrap_or(1),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
        ),
        Err(e) => (127, String::new(), e.to_string()),
    }
}

pub fn default_hint_file() -> PathBuf {
    let data = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.local/share")
    });
    PathBuf::from(data).join("surmount/diskstation-afp-hosts")
}

pub fn default_gvfs_root() -> PathBuf {
    if let Ok(p) = std::env::var("SURMOUNT_AFP_GVFS_ROOT") {
        return PathBuf::from(p);
    }
    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find_map(|l| {
                l.strip_prefix("Uid:")
                    .and_then(|r| r.split_whitespace().next().map(|x| x.to_string()))
            })
        })
        .unwrap_or_else(|| "1000".into());
    PathBuf::from(format!("/run/user/{uid}/gvfs"))
}

pub fn load_deploy_target_from_env_file(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let raw = line
            .strip_prefix("export SURMOUNT_DEPLOY_TARGET=")
            .or_else(|| line.strip_prefix("SURMOUNT_DEPLOY_TARGET="))?;
        let raw = raw.trim_matches(|c| c == '"' || c == '\'');
        if !raw.is_empty() {
            return Some(raw.to_string());
        }
    }
    None
}

pub fn default_agent_target_env() -> PathBuf {
    let data = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.local/share")
    });
    PathBuf::from(data).join("surmount/agent-target.env")
}

pub fn find_tool(env_name: &str, default: &str) -> Result<PathBuf, DsError> {
    if let Ok(p) = std::env::var(env_name) {
        let pb = PathBuf::from(&p);
        if pb.is_file() {
            return Ok(pb);
        }
        return Err(DsError::new(format!("{env_name} is not a file")));
    }
    which(default).ok_or_else(|| DsError::new(format!("{default} not on PATH")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconstruct_gio_flags() {
        assert_eq!(
            reconstruct_maildir_name("1710000000.M123P1.diskstation/2,S"),
            "1710000000.M123P1.diskstation:2,S"
        );
    }

    #[test]
    fn host_ids() {
        assert!(validate_host_id("DS1513").is_ok());
        assert!(validate_host_id("DiskStation").is_err());
    }

    #[test]
    fn strip_hides_testnet() {
        let s = strip_ipv4("host 192.0.2.10 x");
        assert!(!s.contains("192.0.2.10"));
        assert!(s.contains("[redacted]"));
    }
}
