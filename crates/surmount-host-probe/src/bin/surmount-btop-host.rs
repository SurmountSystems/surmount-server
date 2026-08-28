//! Interactive btop on the deploy-host via Eternal Terminal.

use std::process::{Command, ExitCode};

use surmount_host_probe::et::{
    already_on_mail_guest, btop_et_extra, build_et_argv, operator_ssh_identity,
    refuse_without_live_tty,
};
use surmount_host_probe::ssh_target::{resolve_target, target_label};
use surmount_host_probe::{format_argv, resolve_program};

const USAGE: &str = "\
Usage:
  surmount-btop-host [--help]
  surmount-btop-host [--dry-run] [--target HOST]

Interactive btop on the mail host via Eternal Terminal (same reconnect
as just et). Needs etserver on the guest (after just deploy-host).
Mullvad stays operator-owned.

Does not use a raw SSH that can freeze-paint a last frame. After btop
exits, the Eternal Terminal session exits (not et --noexit / nested
guest shell). stdin and stdout must be a live tty.

If already on the mail guest (Eternal Terminal, guest hostname, or
/etc/surmount/root-justfile), runs local btop and refuses nested et.

Target resolution (first wins):
  1. --target HOST
  2. SURMOUNT_DEPLOY_TARGET
  3. agent-target.env

For root@ targets, adds --ssh-option IdentityFile=$HOME/.ssh/id_ed25519
when that file exists.

Options:
  --dry-run   Print planned argv; do not connect.
  --target    SSH/et target (user@host or host). Must not start with '-'.
  -h, --help  Show this help.

Environment:
  SURMOUNT_DEPLOY_TARGET
  SURMOUNT_DEPLOY_SSH_IDENTITY
  SURMOUNT_ET_BIN             et binary override (tests)
  SURMOUNT_BTOP_BIN           local btop override (tests / nested guest)
  SURMOUNT_BTOP_SESSION       local|remote force (tests)
  SURMOUNT_BTOP_REQUIRE_TTY   set 0 to skip the live tty check (tests)
  SURMOUNT_GUEST_MARKER       guest marker path, or 0/1 (tests)
";

fn local_btop_spec() -> String {
    std::env::var("SURMOUNT_BTOP_BIN").unwrap_or_else(|_| "btop".into())
}

fn run_local_btop(dry_run: bool) -> ExitCode {
    eprintln!(
        "btop-host: already on the mail guest; running local btop (refusing nested Eternal Terminal)."
    );
    eprintln!(
        "btop-host: this box. If the clock in this pane stops, the pane is dead: press q, exit the remote shell, then run just btop from the laptop."
    );
    let spec = local_btop_spec();
    if dry_run {
        println!("btop-host: dry-run: {}", format_argv(&spec, &[]));
        return ExitCode::SUCCESS;
    }
    if let Err(e) = refuse_without_live_tty() {
        eprintln!("btop-host: {e}");
        return ExitCode::from(1);
    }
    let btop = match resolve_program(&spec, "btop") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("btop-host: {e}");
            return ExitCode::from(1);
        }
    };
    match Command::new(&btop).status() {
        Ok(st) if st.success() => ExitCode::SUCCESS,
        Ok(st) => {
            eprintln!("btop-host: btop exited {}", st.code().unwrap_or(1));
            ExitCode::from(st.code().unwrap_or(1).min(255) as u8)
        }
        Err(e) => {
            eprintln!("btop-host: {e}");
            ExitCode::from(1)
        }
    }
}

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
    if already_on_mail_guest() {
        return run_local_btop(dry_run);
    }
    let target = match resolve_target(cli_target.as_deref(), None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("btop-host: {e}");
            return ExitCode::from(1);
        }
    };
    let extra = btop_et_extra();
    let id = operator_ssh_identity(&target);
    let et_spec = std::env::var("SURMOUNT_ET_BIN").unwrap_or_else(|_| "et".into());
    if dry_run {
        let argv = build_et_argv(&et_spec, &target, &extra, id.as_deref());
        println!("btop-host: dry-run: {}", format_argv(&argv[0], &argv[1..]));
        return ExitCode::SUCCESS;
    }
    if let Err(e) = refuse_without_live_tty() {
        eprintln!("btop-host: {e}");
        return ExitCode::from(1);
    }
    let et = match resolve_program(&et_spec, "et") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("btop-host: {e}");
            return ExitCode::from(1);
        }
    };
    let et_s = et.to_string_lossy();
    let argv = build_et_argv(&et_s, &target, &extra, id.as_deref());
    eprintln!(
        "btop-host: Eternal Terminal to {} (mail guest btop).",
        target_label(&target)
    );
    eprintln!(
        "btop-host: If the clock in this pane stops, the pane is dead. Press q to end btop (this session exits). Then run just btop from the laptop if you need a new live tty."
    );
    let mut cmd = Command::new(&argv[0]);
    if argv.len() > 1 {
        cmd.args(&argv[1..]);
    }
    cmd.env("ET_NO_TELEMETRY", "1");
    match cmd.status() {
        Ok(st) if st.success() => ExitCode::SUCCESS,
        Ok(st) => {
            eprintln!("btop-host: et exited {}", st.code().unwrap_or(1));
            ExitCode::from(st.code().unwrap_or(1).min(255) as u8)
        }
        Err(e) => {
            eprintln!("btop-host: {e}");
            ExitCode::from(1)
        }
    }
}
