//! CLI parse. Follow is the default. `--status` is a one-shot snapshot.

use crate::status::HostLogsError;

/// Operator-facing help. Names journalctl, no sudo, --status, and disk use.
pub const USAGE: &str = "\
Usage:
  surmount-host-logs [--help]
  surmount-host-logs [--status]
  surmount-host-logs [-- JOURNALCTL_ARGS...]

Read the systemd journal on this host. Journald is the source of truth.
This binary only reads (follow or a --status snapshot). It does not use sudo.
nixbuilder can read when it is in group systemd-journal.

Default follow: journalctl --no-pager -f -n 80 for stalwart-mail,
surmount-management-ui, sshd, ssh, surmount-arti-hidden-service, fail2ban,
nix-daemon, and vaultwarden. Extra args after -- (or a leading - flag)
replace that unit list and are passed to journalctl.

--status is a one-shot diagnose snapshot: journalctl --disk-usage,
journald knobs (Storage / MaxUse / RateLimit), systemctl --failed,
is-active / is-enabled for mail, management-ui, sshd, fail2ban,
nix-daemon, qemu-guest-agent and related units, then a short tail
(no follow). Do not combine --status with extra journalctl args.

Output never prints Authorization, cookies, PEM, or tokens (those
substrings are redacted).

Options:
  --status    One-shot diagnose snapshot (disk use, failed units, tails).
  -h, --help  Show this help.

Environment:
  SURMOUNT_HOST_LOGS_JOURNALCTL     journalctl binary override (tests)
  SURMOUNT_HOST_LOGS_SYSTEMCTL      systemctl binary override (tests)
  SURMOUNT_HOST_LOGS_JOURNALD_CONF  journald.conf path override (tests)
";

/// Units shown in `--status` is-active / is-enabled.
pub const STATUS_UNITS: &[&str] = &[
    "stalwart-mail",
    "surmount-management-ui",
    "sshd",
    "fail2ban",
    "nix-daemon",
    "qemu-guest-agent",
    "surmount-arti-hidden-service",
    "vaultwarden",
    "systemd-journald",
];

/// Default follow unit list (journalctl `-u` each).
pub const FOLLOW_UNITS: &[&str] = &[
    "stalwart-mail",
    "surmount-management-ui",
    "sshd",
    "ssh",
    "surmount-arti-hidden-service",
    "fail2ban",
    "nix-daemon",
    "vaultwarden",
];

/// Short tail units for `--status` (no qemu-guest-agent; matches prior snapshot).
pub const TAIL_UNITS: &[&str] = &[
    "stalwart-mail",
    "surmount-management-ui",
    "sshd",
    "fail2ban",
    "nix-daemon",
    "surmount-arti-hidden-service",
    "vaultwarden",
];

/// Parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Help,
    Follow { journal_args: Vec<String> },
    Status,
}

/// Units listed by `--status`.
pub fn status_units() -> &'static [&'static str] {
    STATUS_UNITS
}

/// Default `journalctl` argv for follow (includes `-f`).
pub fn default_follow_args() -> Vec<String> {
    let mut args = vec![
        "--no-pager".to_string(),
        "-f".to_string(),
        "-n".to_string(),
        "80".to_string(),
    ];
    push_units(&mut args, FOLLOW_UNITS);
    args
}

/// Default `journalctl` argv for the status tail (no `-f`).
pub fn default_tail_args() -> Vec<String> {
    let mut args = vec!["--no-pager".to_string(), "-n".to_string(), "40".to_string()];
    push_units(&mut args, TAIL_UNITS);
    args
}

fn push_units(args: &mut Vec<String>, units: &[&str]) {
    for unit in units {
        args.push("-u".to_string());
        args.push((*unit).to_string());
    }
}

/// Parse argv including argv0. Unknown positionals fail loud.
pub fn parse_args<I, S>(args: I) -> Result<Mode, HostLogsError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let mut status = false;
    let mut journal_args: Vec<String> = Vec::new();
    let mut rest = false;

    for raw in iter {
        let arg = raw.as_ref();
        if rest {
            journal_args.push(arg.to_string());
            continue;
        }
        match arg {
            "-h" | "--help" => return Ok(Mode::Help),
            "--status" => status = true,
            "--" => {
                rest = true;
            }
            s if s.starts_with('-') => {
                journal_args.push(s.to_string());
                rest = true;
            }
            other => {
                return Err(HostLogsError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
    }

    if status && !journal_args.is_empty() {
        return Err(HostLogsError::fail(
            "do not combine --status with extra journalctl args (omit --status to pass custom journalctl)"
                .to_string(),
        ));
    }

    if status {
        Ok(Mode::Status)
    } else if journal_args.is_empty() {
        Ok(Mode::Follow {
            journal_args: default_follow_args(),
        })
    } else {
        Ok(Mode::Follow { journal_args })
    }
}
