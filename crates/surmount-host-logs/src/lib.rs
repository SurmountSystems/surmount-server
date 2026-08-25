//! Host journal reader for Surmount operators.
//!
//! Journald is the source of truth. This crate only reads: follow units or a
//! one-shot `--status` snapshot. It does not sudo, SSH, or write logs.

mod args;
mod redact;
mod status;

pub use args::{Mode, USAGE, default_follow_args, default_tail_args, parse_args, status_units};
pub use redact::{RedactingStream, redact_secret_line, redact_text};
pub use status::{
    HostLogsError, LiveProbe, Probe, STATUS_PLAN, StatusPlan, collect_status, render_status,
    resolve_program,
};

use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Run the CLI against process stdio. Extra journalctl args replace defaults.
pub fn run<I, S>(args: I) -> Result<(), HostLogsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    match parse_args(args)? {
        Mode::Help => {
            print!("{USAGE}");
            Ok(())
        }
        Mode::Status => {
            let probe = LiveProbe::from_env()?;
            let report = collect_status(&probe)?;
            print!("{report}");
            let _ = io::stdout().flush();
            Ok(())
        }
        Mode::Follow { journal_args } => follow_journal(&journal_args),
    }
}

fn follow_journal(journal_args: &[String]) -> Result<(), HostLogsError> {
    let spec =
        std::env::var("SURMOUNT_HOST_LOGS_JOURNALCTL").unwrap_or_else(|_| "journalctl".to_string());
    let journalctl = resolve_program(&spec, "journalctl")?;
    stream_journalctl(&journalctl, journal_args)
}

fn stream_journalctl(journalctl: &Path, args: &[String]) -> Result<(), HostLogsError> {
    let mut child = Command::new(journalctl)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            HostLogsError::fail(format!(
                "failed to start journalctl ({}): {e}",
                journalctl.display()
            ))
        })?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| HostLogsError::fail("journalctl stdout was not piped".to_string()))?;
    let mut stream = RedactingStream::new();
    let mut out = io::stdout();
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| HostLogsError::fail(format!("journalctl read: {e}")))?;
        if let Some(safe) = stream.push_line(&line) {
            writeln!(out, "{safe}").map_err(|e| HostLogsError::fail(e.to_string()))?;
            let _ = out.flush();
        }
    }
    let status = child
        .wait()
        .map_err(|e| HostLogsError::fail(format!("journalctl wait: {e}")))?;
    if let Some(code) = status.code() {
        if code == 0 {
            Ok(())
        } else {
            Err(HostLogsError {
                message: format!("journalctl exited {code}"),
                exit_code: code,
            })
        }
    } else {
        Err(HostLogsError::fail(
            "journalctl terminated by signal".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_mode_from_flags() {
        assert_eq!(
            parse_args(["surmount-host-logs", "--help"]).unwrap(),
            Mode::Help
        );
        assert_eq!(
            parse_args(["surmount-host-logs", "-h"]).unwrap(),
            Mode::Help
        );
    }

    #[test]
    fn status_mode_without_extra_args() {
        assert_eq!(
            parse_args(["surmount-host-logs", "--status"]).unwrap(),
            Mode::Status
        );
    }

    #[test]
    fn usage_names_journalctl_no_sudo_status_and_disk() {
        let u = USAGE.to_ascii_lowercase();
        assert!(USAGE.contains("journalctl"), "help must name journalctl");
        assert!(
            u.contains("sudo")
                && (u.contains("no sudo")
                    || u.contains("without sudo")
                    || u.contains("does not use sudo")),
            "help must say this bin does not use sudo"
        );
        assert!(USAGE.contains("--status"), "help must name --status");
        assert!(u.contains("disk"), "help must mention disk use");
        assert!(
            u.contains("systemd-journal") || u.contains("nixbuilder"),
            "help should say nixbuilder/systemd-journal can read without sudo"
        );
    }

    #[test]
    fn default_follow_is_journalctl_dash_f_named_units() {
        let args = default_follow_args();
        assert!(args.contains(&"--no-pager".into()));
        assert!(args.contains(&"-f".into()));
        assert!(args.contains(&"stalwart-mail".into()));
        assert!(args.contains(&"surmount-management-ui".into()));
        assert!(args.contains(&"sshd".into()));
        assert!(args.contains(&"fail2ban".into()));
        assert!(args.contains(&"nix-daemon".into()));
        assert!(!args.iter().any(|a| a == "sudo"));
    }

    #[test]
    fn extra_args_replace_default_follow_list() {
        let mode = parse_args(["surmount-host-logs", "--", "-u", "sshd", "-n", "5"]).unwrap();
        match mode {
            Mode::Follow { journal_args } => {
                assert_eq!(journal_args, ["-u", "sshd", "-n", "5"].map(str::to_string));
                assert!(!journal_args.iter().any(|a| a == "-f"));
            }
            other => panic!("expected follow, got {other:?}"),
        }
    }

    #[test]
    fn dash_args_without_end_of_options_are_journalctl() {
        let mode = parse_args(["surmount-host-logs", "-u", "sshd", "-n", "5"]).unwrap();
        match mode {
            Mode::Follow { journal_args } => {
                assert_eq!(journal_args, ["-u", "sshd", "-n", "5"].map(str::to_string));
            }
            other => panic!("expected follow, got {other:?}"),
        }
    }

    #[test]
    fn status_rejects_extra_journalctl_args() {
        let err = parse_args(["surmount-host-logs", "--status", "--", "-u", "sshd"]).unwrap_err();
        assert!(
            err.message
                .to_ascii_lowercase()
                .contains("do not combine --status"),
            "got {}",
            err.message
        );
        assert_ne!(err.exit_code, 0);
    }

    #[test]
    fn unknown_positional_fails_loud() {
        let err = parse_args(["surmount-host-logs", "mystery"]).unwrap_err();
        assert!(
            err.message.contains("unknown argument"),
            "got {}",
            err.message
        );
    }

    #[test]
    fn status_units_include_mail_ui_sshd_fail2ban_nix_qemu() {
        let units = status_units();
        for need in [
            "stalwart-mail",
            "surmount-management-ui",
            "sshd",
            "fail2ban",
            "nix-daemon",
            "qemu-guest-agent",
        ] {
            assert!(units.contains(&need), "missing {need} in {units:?}");
        }
    }

    #[test]
    fn status_plan_is_snapshot_not_follow() {
        assert_eq!(STATUS_PLAN.disk_usage, &["--disk-usage"][..]);
        assert!(STATUS_PLAN.failed_units.iter().any(|a| *a == "--failed"));
        assert!(!STATUS_PLAN.tail.iter().any(|a| *a == "-f"));
        assert!(!STATUS_PLAN.failed_units.iter().any(|a| *a == "sudo"));
    }

    #[test]
    fn default_tail_has_no_follow_flag() {
        let tail = default_tail_args();
        assert!(tail.contains(&"--no-pager".into()));
        assert!(!tail.iter().any(|a| a == "-f"));
        assert!(tail.contains(&"stalwart-mail".into()));
        assert!(tail.contains(&"surmount-management-ui".into()));
        assert!(tail.contains(&"sshd".into()));
        assert!(tail.contains(&"fail2ban".into()));
        assert!(tail.contains(&"nix-daemon".into()));
    }
}
