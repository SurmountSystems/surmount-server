//! CLI: --now (one scram) or --watch (1s loop). Highest-priority on the unit.

use std::io::{self, Write};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::Duration;

use surmount_scram::{
    AVAIL_FLOOR_KIB, Cfg, kill_pids, log_plan, lookup_nixbuilder_uid, passwd_from_env, plan_scram,
    proc_root_from_env, read_snapshot,
};

fn usage() -> &'static str {
    "\
Usage:
  surmount-scram --now
  surmount-scram --watch
  surmount-scram --help

Last line of defense: if MemAvailable is below 16 GiB, SIGKILL builder
hogs (lean/lake/rustc, nixbuilder uid, nixbld). Never mail, sshd, UI, et.
--now is one pass. --watch loops every 1s. SURMOUNT_SCRAM_PROC overrides /proc.
"
}

fn cfg() -> Cfg {
    let uid = fs_read_uid();
    Cfg {
        avail_floor_kib: AVAIL_FLOOR_KIB,
        nixbuilder_uid: uid,
    }
}

fn fs_read_uid() -> Option<u32> {
    let p = passwd_from_env();
    let text = std::fs::read_to_string(p).ok()?;
    lookup_nixbuilder_uid(&text)
}

fn run_once(watch: bool) -> io::Result<i32> {
    let cfg = cfg();
    let snap = read_snapshot(&proc_root_from_env(), &cfg)?;
    let plan = plan_scram(&snap, &cfg);
    let mut err = io::stderr();
    log_plan(&mut err, &plan, snap.mem_available_kib)?;
    let _ = err.flush();
    if plan.is_idle() {
        if !watch {
            let _ = writeln!(err, "surmount-scram: idle (enough RAM)");
        }
        return Ok(0);
    }
    let errs = kill_pids(&plan.pids);
    for (pid, m) in &errs {
        let _ = writeln!(err, "surmount-scram: {m} pid={pid}");
    }
    if let Some(slice) = &plan.slice {
        let st = Command::new("systemctl")
            .args(["kill", "-s", "KILL", slice])
            .status();
        match st {
            Ok(s) if s.success() => {
                let _ = writeln!(err, "surmount-scram: killed slice {slice}");
            }
            Ok(s) => {
                let _ = writeln!(
                    err,
                    "surmount-scram: systemctl kill {slice} exit={}",
                    s.code().unwrap_or(-1)
                );
            }
            Err(e) => {
                let _ = writeln!(err, "surmount-scram: systemctl: {e}");
            }
        }
    }
    Ok(0)
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprint!("{}", usage());
        return ExitCode::from(2);
    }
    match args.remove(0).as_str() {
        "-h" | "--help" => {
            print!("{}", usage());
            ExitCode::SUCCESS
        }
        "--now" => match run_once(false) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("surmount-scram: {e}");
                ExitCode::from(1)
            }
        },
        "--watch" => loop {
            if let Err(e) = run_once(true) {
                eprintln!("surmount-scram: {e}");
            }
            thread::sleep(Duration::from_secs(1));
        },
        other => {
            eprintln!("surmount-scram: unknown argument: {other}");
            eprint!("{}", usage());
            ExitCode::from(2)
        }
    }
}
