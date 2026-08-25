//! One-shot `--status` snapshot: disk use, knobs, failed units, is-active, tails.

use std::fmt::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::args::{STATUS_UNITS, default_tail_args};
use crate::redact::redact_text;

/// CLI / probe failure. `exit_code` is the process status (1 unless journalctl).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLogsError {
    pub message: String,
    pub exit_code: i32,
}

impl HostLogsError {
    pub fn fail(message: String) -> Self {
        Self {
            message,
            exit_code: 1,
        }
    }
}

impl fmt::Display for HostLogsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HostLogsError {}

/// Command shapes for `--status` (no follow, no sudo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusPlan {
    pub disk_usage: &'static [&'static str],
    pub failed_units: &'static [&'static str],
    pub tail: &'static [&'static str],
}

/// Frozen plan so tests can assert snapshot argv without a live journal.
pub const STATUS_PLAN: StatusPlan = StatusPlan {
    disk_usage: &["--disk-usage"],
    failed_units: &["--failed", "--no-pager", "--plain", "--no-legend"],
    tail: &["--no-pager", "-n", "40"],
};

/// Read-only host facts for `--status`.
pub trait Probe {
    fn disk_usage(&self) -> Result<String, HostLogsError>;
    fn journald_conf(&self) -> Result<String, HostLogsError>;
    fn failed_units(&self) -> Result<String, HostLogsError>;
    fn is_active(&self, unit: &str) -> String;
    fn is_enabled(&self, unit: &str) -> String;
    fn recent_tails(&self) -> Result<String, HostLogsError>;
}

/// Live journalctl / systemctl / journald.conf (PATH or env overrides).
pub struct LiveProbe {
    pub journalctl: PathBuf,
    pub systemctl: PathBuf,
    pub journald_conf: PathBuf,
}

impl LiveProbe {
    pub fn from_env() -> Result<Self, HostLogsError> {
        let journalctl_spec = std::env::var("SURMOUNT_HOST_LOGS_JOURNALCTL")
            .unwrap_or_else(|_| "journalctl".to_string());
        let systemctl_spec = std::env::var("SURMOUNT_HOST_LOGS_SYSTEMCTL")
            .unwrap_or_else(|_| "systemctl".to_string());
        let conf = std::env::var("SURMOUNT_HOST_LOGS_JOURNALD_CONF")
            .unwrap_or_else(|_| "/etc/systemd/journald.conf".to_string());
        Ok(Self {
            journalctl: resolve_program(&journalctl_spec, "journalctl")?,
            systemctl: resolve_program(&systemctl_spec, "systemctl")?,
            journald_conf: PathBuf::from(conf),
        })
    }
}

impl Probe for LiveProbe {
    fn disk_usage(&self) -> Result<String, HostLogsError> {
        capture(&self.journalctl, STATUS_PLAN.disk_usage)
    }

    fn journald_conf(&self) -> Result<String, HostLogsError> {
        match std::fs::read_to_string(&self.journald_conf) {
            Ok(text) => Ok(text),
            Err(_) => Ok(String::new()),
        }
    }

    fn failed_units(&self) -> Result<String, HostLogsError> {
        capture(&self.systemctl, STATUS_PLAN.failed_units)
    }

    fn is_active(&self, unit: &str) -> String {
        first_line(&capture(&self.systemctl, &["is-active", unit]).unwrap_or_default())
    }

    fn is_enabled(&self, unit: &str) -> String {
        first_line(&capture(&self.systemctl, &["is-enabled", unit]).unwrap_or_default())
    }

    fn recent_tails(&self) -> Result<String, HostLogsError> {
        let args = default_tail_args();
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        capture(&self.journalctl, &refs)
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").trim().to_string()
}

fn capture(bin: &Path, args: &[&str]) -> Result<String, HostLogsError> {
    let out = Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| HostLogsError::fail(format!("{}: {e}", bin.display())))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Resolve `name` on PATH, or an absolute/relative override. No sudo.
pub fn resolve_program(spec: &str, label: &str) -> Result<PathBuf, HostLogsError> {
    let path = Path::new(spec);
    if spec.contains('/') {
        if is_executable(path) {
            return Ok(path.to_path_buf());
        }
        return Err(HostLogsError::fail(format!("{label} not found: {spec}")));
    }
    if let Some(found) = search_path(spec) {
        return Ok(found);
    }
    let env_name = match label {
        "journalctl" => "SURMOUNT_HOST_LOGS_JOURNALCTL",
        "systemctl" => "SURMOUNT_HOST_LOGS_SYSTEMCTL",
        _ => "PATH",
    };
    Err(HostLogsError::fail(format!(
        "{label} not found (install systemd or set {env_name})"
    )))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    meta.permissions().mode() & 0o111 != 0
}

fn search_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Lines from journald.conf that are Storage / MaxUse / RateLimit knobs.
pub fn journald_knob_lines(conf: &str) -> Vec<String> {
    conf.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| {
            l.starts_with("Storage=")
                || l.starts_with("SystemMaxUse=")
                || l.starts_with("RuntimeMaxUse=")
                || l.starts_with("MaxRetentionSec=")
                || l.starts_with("RateLimit")
                || l.starts_with("ForwardToSyslog=")
        })
        .map(str::to_string)
        .collect()
}

fn unknown_if_empty(s: &str) -> &str {
    if s.is_empty() { "unknown" } else { s }
}

/// Format one `active=` / `enabled=` row.
pub fn format_unit_state(unit: &str, active: &str, enabled: &str) -> String {
    format!(
        "  {unit} active={} enabled={}",
        unknown_if_empty(active),
        unknown_if_empty(enabled)
    )
}

/// Build the operator snapshot. Every section is secret-redacted.
pub fn collect_status(probe: &impl Probe) -> Result<String, HostLogsError> {
    let mut unit_lines = Vec::new();
    for unit in STATUS_UNITS {
        unit_lines.push(format_unit_state(
            unit,
            &probe.is_active(unit),
            &probe.is_enabled(unit),
        ));
    }
    let knobs = {
        let conf = probe.journald_conf()?;
        if conf.is_empty() {
            "journald.conf unreadable".to_string()
        } else {
            let lines = journald_knob_lines(&conf);
            if lines.is_empty() {
                String::new()
            } else {
                lines.join("\n")
            }
        }
    };
    Ok(render_status(
        &probe.disk_usage()?,
        &knobs,
        &probe.failed_units()?,
        &unit_lines,
        &probe.recent_tails()?,
    ))
}

/// Assemble `--status` sections (disk, knobs, failed, unit state, tails).
pub fn render_status(
    disk: &str,
    knobs: &str,
    failed: &str,
    unit_lines: &[String],
    tails: &str,
) -> String {
    let mut out = String::new();
    push_section(&mut out, "== journal disk ==", disk);
    push_section(
        &mut out,
        "== journald knobs (Storage / MaxUse / RateLimit) ==",
        knobs,
    );
    push_section(&mut out, "== failed units ==", failed);
    out.push_str("== unit state ==\n");
    if unit_lines.is_empty() {
        out.push('\n');
    } else {
        for line in unit_lines {
            out.push_str(line);
            out.push('\n');
        }
    }
    push_section(&mut out, "== recent tails (no follow) ==", tails);
    redact_text(&out)
}

fn push_section(out: &mut String, header: &str, body: &str) {
    out.push_str(header);
    out.push('\n');
    let trimmed = body.trim_end_matches('\n');
    if !trimmed.is_empty() {
        out.push_str(trimmed);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeProbe {
        disk: String,
        conf: String,
        failed: String,
        tails: String,
    }

    impl Probe for FakeProbe {
        fn disk_usage(&self) -> Result<String, HostLogsError> {
            Ok(self.disk.clone())
        }
        fn journald_conf(&self) -> Result<String, HostLogsError> {
            Ok(self.conf.clone())
        }
        fn failed_units(&self) -> Result<String, HostLogsError> {
            Ok(self.failed.clone())
        }
        fn is_active(&self, _unit: &str) -> String {
            "active".to_string()
        }
        fn is_enabled(&self, _unit: &str) -> String {
            "enabled".to_string()
        }
        fn recent_tails(&self) -> Result<String, HostLogsError> {
            Ok(self.tails.clone())
        }
    }

    #[test]
    fn knob_filter_keeps_storage_maxuse_ratelimit() {
        let conf = "\n# comment\nStorage=persistent\nSystemMaxUse=1G\nRuntimeMaxUse=256M\nMaxRetentionSec=30day\nRateLimitIntervalSec=30s\nRateLimitBurst=20000\nForwardToSyslog=no\nOther=nope\n";
        let lines = journald_knob_lines(conf);
        assert!(lines.iter().any(|l| l == "Storage=persistent"));
        assert!(lines.iter().any(|l| l.starts_with("SystemMaxUse=")));
        assert!(lines.iter().any(|l| l.starts_with("RateLimitBurst=")));
        assert!(!lines.iter().any(|l| l.starts_with("Other=")));
    }

    #[test]
    fn collect_status_redacts_tails_and_lists_qemu() {
        let probe = FakeProbe {
            disk: "Archived and active journals take up 12.0M on disk.\n".into(),
            conf: "Storage=persistent\nRateLimitBurst=20000\n".into(),
            failed: "0 loaded units listed.\n".into(),
            tails: "GET / Authorization: Bearer SYNTHETIC-NOT-A-SECRET\n".into(),
        };
        let report = collect_status(&probe).unwrap();
        assert!(report.contains("== journal disk =="), "{report}");
        assert!(report.contains("== journald knobs"), "{report}");
        assert!(report.contains("RateLimitBurst=20000"), "{report}");
        assert!(report.contains("== failed units =="), "{report}");
        assert!(report.contains("qemu-guest-agent"), "{report}");
        assert!(report.contains("stalwart-mail"), "{report}");
        assert!(report.contains("surmount-management-ui"), "{report}");
        assert!(report.contains("sshd"), "{report}");
        assert!(report.contains("fail2ban"), "{report}");
        assert!(report.contains("nix-daemon"), "{report}");
        assert!(
            report.contains("== recent tails (no follow) =="),
            "{report}"
        );
        assert!(!report.contains("SYNTHETIC-NOT-A-SECRET"), "{report}");
        assert!(report.contains("<redacted>"), "{report}");
        assert!(!report.contains(" sudo"), "{report}");
    }

    #[test]
    fn missing_conf_is_unreadable_not_a_panic() {
        let probe = FakeProbe {
            disk: "ok\n".into(),
            conf: String::new(),
            failed: String::new(),
            tails: String::new(),
        };
        let report = collect_status(&probe).unwrap();
        assert!(report.contains("journald.conf unreadable"), "{report}");
    }

    #[test]
    fn resolve_program_option_shaped_relative_missing_fails() {
        let err = resolve_program("/no/such/journalctl-bin", "journalctl").unwrap_err();
        assert!(
            err.message.contains("journalctl not found"),
            "{}",
            err.message
        );
    }
}
