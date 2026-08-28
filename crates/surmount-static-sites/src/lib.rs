//! Apex/www publish plus proven DS3018xs extra-vhost copy.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const PROVEN_SLUGS: &[&str] = &[
    "cryptoquick",
    "baxterartworks",
    "btcfur",
    "exophiles",
    "iantuckerstudios",
    "nostrfurs",
    "yiffa",
];

#[derive(Debug)]
pub struct SitesError {
    pub message: String,
}

impl std::fmt::Display for SitesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SitesError {}

impl SitesError {
    pub fn new(m: impl Into<String>) -> Self {
        Self { message: m.into() }
    }
}

pub fn redact_ipv4(s: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let chars: Vec<char> = s.chars().collect();
    while i < chars.len() {
        if let Some(len) = ipv4_len(&chars[i..]) {
            out.push_str("<redacted-ipv4>");
            i += len;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn ipv4_len(chars: &[char]) -> Option<usize> {
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

pub fn target_label(target: &str) -> String {
    if let Some((user, _)) = target.split_once('@') {
        format!("{user}@<mail-host>")
    } else {
        "<mail-host>".into()
    }
}

pub fn load_target_from_env_file(path: &Path) -> Option<String> {
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

pub fn resolve_deploy_target(cli: Option<&str>) -> Result<String, SitesError> {
    if let Some(t) = cli.filter(|s| !s.is_empty()) {
        return validate_target(t);
    }
    if let Ok(t) = std::env::var("SURMOUNT_DEPLOY_TARGET") {
        if !t.is_empty() {
            return validate_target(&t);
        }
    }
    let env_file = std::env::var("SURMOUNT_AGENT_TARGET_ENV")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_agent_target_env());
    if !env_file.is_file() {
        return Err(SitesError::new(
            "target required: pass --target USER@HOST, set SURMOUNT_DEPLOY_TARGET, or create agent-target.env (never a committed real box default)",
        ));
    }
    let t = load_target_from_env_file(&env_file).ok_or_else(|| {
        SitesError::new(format!(
            "agent-target.env has no SURMOUNT_DEPLOY_TARGET: {}",
            env_file.display()
        ))
    })?;
    validate_target(&t)
}

pub fn validate_target(t: &str) -> Result<String, SitesError> {
    if t.starts_with('-') {
        return Err(SitesError::new(
            "target must not start with '-': option-shaped values are rejected",
        ));
    }
    if t.contains(char::is_whitespace) || t.contains(';') || t.contains('|') {
        return Err(SitesError::new("--target has refused characters"));
    }
    Ok(t.to_string())
}

pub fn copy_tree(src: &Path, dest: &Path, rsync: &str) -> Result<(), SitesError> {
    std::fs::create_dir_all(dest).map_err(|e| SitesError::new(e.to_string()))?;
    if which(rsync) {
        let st = Command::new(rsync)
            .args(["-a", "--delete", "--"])
            .arg(format!("{}/", src.display()))
            .arg(format!("{}/", dest.display()))
            .status()
            .map_err(|e| SitesError::new(e.to_string()))?;
        if !st.success() {
            return Err(SitesError::new("rsync copy failed"));
        }
        return Ok(());
    }
    copy_recursive(src, dest)
}

fn copy_recursive(src: &Path, dest: &Path) -> Result<(), SitesError> {
    std::fs::create_dir_all(dest).map_err(|e| SitesError::new(e.to_string()))?;
    for ent in std::fs::read_dir(src).map_err(|e| SitesError::new(e.to_string()))? {
        let ent = ent.map_err(|e| SitesError::new(e.to_string()))?;
        let to = dest.join(ent.file_name());
        let ty = ent
            .file_type()
            .map_err(|e| SitesError::new(e.to_string()))?;
        if ty.is_dir() {
            copy_recursive(&ent.path(), &to)?;
        } else {
            std::fs::copy(ent.path(), to).map_err(|e| SitesError::new(e.to_string()))?;
        }
    }
    Ok(())
}

pub fn which(name: &str) -> bool {
    if name.contains('/') {
        return Path::new(name).is_file();
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            if Path::new(dir).join(name).is_file() {
                return true;
            }
        }
    }
    false
}

pub fn ssh_rsync_e(ssh: &str) -> String {
    format!(
        "{ssh} -o BatchMode=yes -o PreferredAuthentications=publickey -o PasswordAuthentication=no -o KbdInteractiveAuthentication=no"
    )
}

pub fn redact_gvfs_path(s: &str, share: &str) -> String {
    let red = redact_ipv4(s);
    if red.contains("gvfs") || red.contains("afp-volume") {
        format!("DS3018xs GVFS {share} share")
    } else {
        red
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proven_does_not_include_dropped() {
        assert!(!PROVEN_SLUGS.contains(&"denverspace"));
        assert!(PROVEN_SLUGS.contains(&"cryptoquick"));
        assert!(PROVEN_SLUGS.contains(&"yiffa"));
    }
}
