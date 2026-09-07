//! Remote grok-oss attach over SSH + tmux as user grok. Not Eternal Terminal.

use std::process::{Command, ExitCode};

use surmount_host_probe::et::{already_on_mail_guest, operator_ssh_identity};
use surmount_host_probe::grok_oss::{
    build_remote_argv, build_running_json_argv, local_running_json_cmd, local_tmux_cmd,
    refuse_without_live_tty,
};
use surmount_host_probe::ssh_target::resolve_target;
use surmount_host_probe::{format_argv, resolve_program};

const USAGE: &str = "\
Usage:
  surmount-grok-oss [--help]
  surmount-grok-oss [--dry-run] [--target HOST]
  surmount-grok-oss --running [--dry-run] [--target HOST]

Attach grok-oss on the mail host via SSH + tmux (ssh -t, then
runuser as user grok so user-<uid>.slice MemoryMax applies:
tmux attach -t grok-oss or tmux new -s grok-oss grok-oss).
Root tmux is a miss. Does not use Eternal Terminal (not et -p 2022).

--running prints grok-oss running --json as user grok (no TTY, no
HTTP bind, no secrets). Nested guest prints local JSON.

If already on the mail guest (Eternal Terminal ancestor, guest hostname,
or /etc/surmount/root-justfile), uses local tmux (or local --running)
and refuses nested SSH.

Target resolution (first wins):
  1. --target HOST
  2. SURMOUNT_DEPLOY_TARGET
  3. agent-target.env

For root@ targets, adds -i $HOME/.ssh/id_ed25519 when that file exists
(IdentitiesOnly=yes). Override: SURMOUNT_DEPLOY_SSH_IDENTITY.

Options:
  --running   Print grok-oss running --json as user grok. No attach.
  --dry-run   Print planned argv; do not connect.
  --target    SSH target (user@host or host). Must not start with '-'.
  -h, --help  Show this help.

Environment:
  SURMOUNT_DEPLOY_TARGET
  SURMOUNT_DEPLOY_SSH_IDENTITY
  SURMOUNT_GROK_OSS_SSH           ssh binary override (tests)
  SURMOUNT_GROK_OSS_TMUX          tmux binary override (tests / nested guest)
  SURMOUNT_GROK_OSS_USER          session user (default grok; not root)
  SURMOUNT_GROK_OSS_HOME          session home (default /home/grok)
  SURMOUNT_GROK_OSS_REQUIRE_TTY   set 0 to skip the live tty check (tests)
  SURMOUNT_BTOP_SESSION           local|remote force (same nested-guest hints)
  SURMOUNT_GUEST_MARKER           guest marker path, or 0/1 (tests)
";

fn local_tmux_spec() -> String {
    std::env::var("SURMOUNT_GROK_OSS_TMUX").unwrap_or_else(|_| "tmux".into())
}

fn ssh_spec() -> String {
    std::env::var("SURMOUNT_GROK_OSS_SSH").unwrap_or_else(|_| "ssh".into())
}

fn run_shell(shell: &str) -> ExitCode {
    match Command::new("sh").arg("-c").arg(shell).status() {
        Ok(st) if st.success() => ExitCode::SUCCESS,
        Ok(st) => {
            eprintln!("grok-oss: command exited {}", st.code().unwrap_or(1));
            ExitCode::from(st.code().unwrap_or(1).min(255) as u8)
        }
        Err(e) => {
            eprintln!("grok-oss: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_local(dry_run: bool) -> ExitCode {
    eprintln!(
        "grok-oss: already on the mail guest; attaching local tmux as user grok (refusing nested SSH). This box."
    );
    let spec = local_tmux_spec();
    let planned = match local_tmux_cmd(&spec) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    if dry_run {
        println!("grok-oss: dry-run: {planned}");
        return ExitCode::SUCCESS;
    }
    if let Err(e) = refuse_without_live_tty() {
        eprintln!("grok-oss: {e}");
        return ExitCode::from(1);
    }
    let tmux = match resolve_program(&spec, "tmux") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    let tmux_s = tmux.to_string_lossy();
    let shell = match local_tmux_cmd(&tmux_s) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    run_shell(&shell)
}

fn run_local_running(dry_run: bool) -> ExitCode {
    eprintln!(
        "grok-oss: already on the mail guest; printing grok-oss running --json as user grok (refusing nested SSH). This box."
    );
    let planned = match local_running_json_cmd() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    if dry_run {
        println!("grok-oss: dry-run: {planned}");
        return ExitCode::SUCCESS;
    }
    run_shell(&planned)
}

fn run_remote_argv(argv: &[String], dry_run: bool, need_tty: bool) -> ExitCode {
    if dry_run {
        println!("grok-oss: dry-run: {}", format_argv(&argv[0], &argv[1..]));
        return ExitCode::SUCCESS;
    }
    if need_tty {
        if let Err(e) = refuse_without_live_tty() {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    }
    let ssh = match resolve_program(&argv[0], "ssh") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    let mut cmd = Command::new(&ssh);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    match cmd.status() {
        Ok(st) if st.success() => ExitCode::SUCCESS,
        Ok(st) => {
            eprintln!("grok-oss: ssh exited {}", st.code().unwrap_or(1));
            ExitCode::from(st.code().unwrap_or(1).min(255) as u8)
        }
        Err(e) => {
            eprintln!("grok-oss: {e}");
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let mut dry_run = false;
    let mut running = false;
    let mut cli_target = None;
    let rest: Vec<String> = std::env::args().skip(1).collect();
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
            "--running" => {
                running = true;
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
                        eprintln!("grok-oss: --target requires a value");
                        return ExitCode::from(1);
                    }
                }
            }
            other => {
                eprintln!("grok-oss: unknown argument: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
    }
    if already_on_mail_guest() {
        if running {
            return run_local_running(dry_run);
        }
        return run_local(dry_run);
    }
    let target = match resolve_target(cli_target.as_deref(), None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    let id = operator_ssh_identity(&target);
    let ssh = ssh_spec();
    let argv = if running {
        build_running_json_argv(&ssh, &target, id.as_deref())
    } else {
        build_remote_argv(&ssh, &target, id.as_deref())
    };
    let argv = match argv {
        Ok(a) => a,
        Err(e) => {
            eprintln!("grok-oss: {e}");
            return ExitCode::from(1);
        }
    };
    run_remote_argv(&argv, dry_run, !running)
}
