//! Interactive btop on the deploy-host SSH target (TTY).

use std::process::ExitCode;

use surmount_host_probe::ssh_target::resolve_target;
use surmount_host_probe::{exec_ssh, format_argv, key_only_ssh_opts, resolve_program};

const USAGE: &str = "\
Usage:
  surmount-btop-host [--help]
  surmount-btop-host [--dry-run] [--target HOST]

Interactive btop on the mail/management host via SSH. Same target as
deploy-host. Requests a TTY (ssh -t). Key-only (publickey; BatchMode;
no password prompt).

Target resolution (first wins):
  1. --target HOST
  2. SURMOUNT_DEPLOY_TARGET
  3. agent-target.env (SURMOUNT_AGENT_TARGET_ENV, else
     $XDG_DATA_HOME/surmount/agent-target.env, else
     ~/.local/share/surmount/agent-target.env)
     The file must set SURMOUNT_DEPLOY_TARGET (export optional).

Fails loud if the env file and SURMOUNT_DEPLOY_TARGET are both missing,
if the env file has no target, or if ssh is missing.

Options:
  --dry-run   Print planned ssh argv; do not connect.
  --target    SSH target (user@host or host). Must not start with '-'.
  -h, --help  Show this help.

Environment:
  SURMOUNT_DEPLOY_TARGET     Same as --target
  SURMOUNT_AGENT_TARGET_ENV  Path to env file (tests / override)
  SURMOUNT_BTOP_SSH          ssh binary override (tests)
";

fn main() -> ExitCode {
    let mut dry_run = false;
    let mut cli_target = None;
    let mut args = std::env::args();
    let _argv0 = args.next();
    let rest: Vec<String> = args.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--target" => {
                i += 1;
                match rest.get(i) {
                    Some(v) => {
                        cli_target = Some(v.clone());
                        i += 1;
                    }
                    None => {
                        eprintln!("btop-host: --target requires a value");
                        return ExitCode::from(1);
                    }
                }
            }
            other => {
                eprintln!("btop-host: unknown argument: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
    }
    let target = match resolve_target(cli_target.as_deref(), None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("btop-host: {e}");
            return ExitCode::from(1);
        }
    };
    let mut ssh_args = vec!["-t".into()];
    ssh_args.extend(key_only_ssh_opts());
    ssh_args.push("--".into());
    ssh_args.push(target);
    ssh_args.push("btop".into());
    let ssh_spec = std::env::var("SURMOUNT_BTOP_SSH").unwrap_or_else(|_| "ssh".into());
    if dry_run {
        println!("btop-host: dry-run: {}", format_argv(&ssh_spec, &ssh_args));
        return ExitCode::SUCCESS;
    }
    let ssh = match resolve_program(&ssh_spec, "ssh") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("btop-host: {e}");
            return ExitCode::from(1);
        }
    };
    match exec_ssh(&ssh, &ssh_args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("btop-host: {e}");
            ExitCode::from(e.exit_code.min(255) as u8)
        }
    }
}
