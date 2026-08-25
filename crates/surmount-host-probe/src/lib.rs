//! Host probes: SSH inxi/btop plus env-gated hybrid TLS.

pub mod et;
pub mod ssh_target;
pub mod tls_hybrid;

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub struct ProbeError {
    pub message: String,
    pub exit_code: i32,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProbeError {}

impl ProbeError {
    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }
}

pub fn shell_quote(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:@=,+%".contains(c))
        && !s.is_empty()
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn format_argv(bin: &str, args: &[String]) -> String {
    let mut out = shell_quote(bin);
    for a in args {
        out.push(' ');
        out.push_str(&shell_quote(a));
    }
    out
}

pub fn resolve_program(spec: &str, label: &str) -> Result<PathBuf, ProbeError> {
    let p = Path::new(spec);
    if spec.contains('/') {
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
        return Err(ProbeError::fail(format!("{label} not found: {spec}")));
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let cand = Path::new(dir).join(spec);
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }
    Err(ProbeError::fail(format!(
        "{label} not found (install openssh or set the override env)"
    )))
}

pub fn key_only_ssh_opts() -> Vec<String> {
    vec![
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "PreferredAuthentications=publickey".into(),
        "-o".into(),
        "PasswordAuthentication=no".into(),
        "-o".into(),
        "KbdInteractiveAuthentication=no".into(),
    ]
}

pub fn exec_ssh(ssh_bin: &Path, args: &[String]) -> Result<(), ProbeError> {
    let status = Command::new(ssh_bin)
        .args(args)
        .status()
        .map_err(|e| ProbeError::fail(format!("ssh failed to start: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(ProbeError {
            message: format!("ssh exited {}", status.code().unwrap_or(1)),
            exit_code: status.code().unwrap_or(1),
        })
    }
}
