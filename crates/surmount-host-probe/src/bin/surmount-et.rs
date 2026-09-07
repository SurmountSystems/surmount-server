//! Eternal Terminal client. Reconnects after sleep and network change.
//! Mullvad stays operator-owned. For root@ targets, pass IdentityFile.

use std::process::{Command, ExitCode};

use surmount_host_probe::et::{
    build_et_argv, collect_et_status, format_et_status, operator_ssh_identity,
};
use surmount_host_probe::ssh_target::resolve_target;
use surmount_host_probe::{ProbeError, format_argv, resolve_program};

const USAGE: &str = "\
Usage:
  surmount-et [--help]
  surmount-et [--status] [--target HOST]
  surmount-et [--dry-run] [--target HOST] [-- ET_ARGS...]

Eternal Terminal client (et). Survives laptop sleep and network change.
Does not replace Mullvad. Long commands on the guest still belong in tmux.

--status prints whether TCP 22 and TCP 2022 answer. It does not print
addresses. If 2022 does not answer, it tells you to use SSH/tmux
(just grok-oss).

Target resolution (first wins):
  1. --target HOST
  2. First argument that looks like user@host or host (not starting with -)
  3. SURMOUNT_DEPLOY_TARGET
  4. agent-target.env

For root@ targets, adds --ssh-option IdentityFile=$HOME/.ssh/id_ed25519
when that file exists (Host surmount-1 is the nixbuilder key). Override:
SURMOUNT_DEPLOY_SSH_IDENTITY.

Options:
  --status    Probe TCP 22 and TCP 2022; print answers without addresses.
  --dry-run   Print planned et argv; do not connect.
  --target    SSH/et target (user@host or host). Must not start with '-'.
  -h, --help  Show this help.

Environment:
  SURMOUNT_DEPLOY_TARGET
  SURMOUNT_DEPLOY_SSH_IDENTITY
  SURMOUNT_ET_BIN             et binary override (tests)
  SURMOUNT_ET_STATUS_TCP22    1/0 override for --status (tests)
  SURMOUNT_ET_STATUS_TCP2022  1/0 override for --status (tests)
";

struct Opts {
    dry_run: bool,
    status: bool,
    target: Option<String>,
    extra: Vec<String>,
}

fn parse_args(args: &[String]) -> Result<Opts, ProbeError> {
    let mut dry_run = false;
    let mut status = false;
    let mut target = None;
    let mut extra = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--dry-run" => dry_run = true,
            "--status" => status = true,
            "--target" => {
                i += 1;
                if i >= args.len() {
                    return Err(ProbeError::fail("--target requires a value"));
                }
                if args[i].starts_with('-') {
                    return Err(ProbeError::fail("target must not start with '-'"));
                }
                target = Some(args[i].clone());
            }
            "--" => {
                extra.extend(args[i + 1..].iter().cloned());
                break;
            }
            other if other.starts_with('-') => extra.push(other.to_string()),
            other => {
                if target.is_none() && !other.starts_with('-') {
                    target = Some(other.to_string());
                } else {
                    extra.push(other.to_string());
                }
            }
        }
        i += 1;
    }
    if extra.iter().any(|a| a == "--status") {
        status = true;
        extra.retain(|a| a != "--status");
    }
    Ok(Opts {
        dry_run,
        status,
        target,
        extra,
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("surmount-et: {}", e.message);
            ExitCode::from(e.exit_code as u8)
        }
    }
}

fn run(args: &[String]) -> Result<(), ProbeError> {
    let opts = parse_args(args)?;
    let target = resolve_target(opts.target.as_deref(), None)?;
    if opts.status {
        let text = format_et_status(collect_et_status(&target));
        println!("{text}");
        return Ok(());
    }
    let et_spec = std::env::var("SURMOUNT_ET_BIN").unwrap_or_else(|_| "et".to_string());
    let et = resolve_program(&et_spec, "et")?;
    let id = operator_ssh_identity(&target);
    let et_s = et.to_string_lossy();
    let argv = build_et_argv(&et_s, &target, &opts.extra, id.as_deref());
    if opts.dry_run {
        println!(
            "surmount-et: dry-run: {}",
            format_argv(&argv[0], &argv[1..])
        );
        return Ok(());
    }
    let mut cmd = Command::new(&argv[0]);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.env("ET_NO_TELEMETRY", "1");
    let st = cmd
        .status()
        .map_err(|e| ProbeError::fail(format!("et failed: {e}")))?;
    if st.success() {
        Ok(())
    } else {
        Err(ProbeError::fail(format!(
            "et exited {}",
            st.code().unwrap_or(1)
        )))
    }
}
