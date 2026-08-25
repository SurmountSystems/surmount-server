//! inxi on the deploy-host SSH target. No TTY. No sudo.

use std::process::ExitCode;

use surmount_host_probe::ssh_target::resolve_target;
use surmount_host_probe::{exec_ssh, format_argv, key_only_ssh_opts, resolve_program, ProbeError};

const USAGE: &str = "\
Usage:
  surmount-inxi-host [--help]
  surmount-inxi-host [--dry-run] [--userland] [--target HOST] [-- INXI_ARGS...]

Run inxi on the mail/management host via SSH. Same target as deploy-host
and just btop. Key-only (publickey; BatchMode; no password prompt).
Does not request a TTY. Does not use sudo. Measure the mail host on the
mail host; laptop inxi is a different machine.

Default inxi flags: -Fxxxz -c0 (full report, no color). Extra args after
-- replace that default (example: -- -C -c0).

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
  --userland  Remote: nix shell nixpkgs#inxi -c inxi ... (no sudo; works
              before pkgs.inxi is on the host PATH).
  --target    SSH target (user@host or host). Must not start with '-'.
  -h, --help  Show this help.

Environment:
  SURMOUNT_DEPLOY_TARGET     Same as --target
  SURMOUNT_AGENT_TARGET_ENV  Path to env file (tests / override)
  SURMOUNT_INXI_SSH          ssh binary override (tests)
";

struct Opts {
    dry_run: bool,
    userland: bool,
    target: Option<String>,
    inxi_args: Vec<String>,
}

fn parse(args: impl IntoIterator<Item = String>) -> Result<Opts, ProbeError> {
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let rest: Vec<String> = iter.collect();
    let mut dry_run = false;
    let mut userland = false;
    let mut target = None;
    let mut inxi_args = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--userland" => {
                userland = true;
                i += 1;
            }
            "--target" => {
                i += 1;
                let v = rest.get(i).ok_or_else(|| ProbeError::fail("--target requires a value"))?;
                target = Some(v.clone());
                i += 1;
            }
            "--" => {
                inxi_args.extend(rest[i + 1..].iter().cloned());
                break;
            }
            a if a.starts_with('-') => {
                inxi_args.extend(rest[i..].iter().cloned());
                break;
            }
            other => {
                return Err(ProbeError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
    }
    Ok(Opts {
        dry_run,
        userland,
        target,
        inxi_args,
    })
}

fn main() -> ExitCode {
    let opts = match parse(std::env::args()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("inxi-host: {e}");
            return ExitCode::from(1);
        }
    };
    let target = match resolve_target(opts.target.as_deref(), None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("inxi-host: {e}");
            return ExitCode::from(1);
        }
    };
    let inxi_args = if opts.inxi_args.is_empty() {
        vec!["-Fxxxz".into(), "-c0".into()]
    } else {
        opts.inxi_args
    };
    let mut ssh_args = key_only_ssh_opts();
    ssh_args.push("--".into());
    ssh_args.push(target);
    if opts.userland {
        ssh_args.extend([
            "nix".into(),
            "shell".into(),
            "nixpkgs#inxi".into(),
            "-c".into(),
            "inxi".into(),
        ]);
        ssh_args.extend(inxi_args);
    } else {
        ssh_args.push("inxi".into());
        ssh_args.extend(inxi_args);
    }
    let ssh_spec = std::env::var("SURMOUNT_INXI_SSH").unwrap_or_else(|_| "ssh".into());
    if opts.dry_run {
        println!(
            "inxi-host: dry-run: {}",
            format_argv(&ssh_spec, &ssh_args)
        );
        return ExitCode::SUCCESS;
    }
    let ssh = match resolve_program(&ssh_spec, "ssh") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("inxi-host: {e}");
            return ExitCode::from(1);
        }
    };
    match exec_ssh(&ssh, &ssh_args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("inxi-host: {e}");
            ExitCode::from(e.exit_code.min(255) as u8)
        }
    }
}
