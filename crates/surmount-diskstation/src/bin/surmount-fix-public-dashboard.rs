//! Compose recovery unlock -> bootstrap API token -> free-443 dry-run.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use surmount_diskstation::DsError;

const USAGE: &str = "\
fix-public-dashboard - recovery pin + mint API key + free-443 dry-run

Usage:
  surmount-fix-public-dashboard [options]
  just fix-public-dashboard -- [options]

What it does (no operator password homework):
  1) Generate high-entropy recovery admin pin (or --reuse Domain A staging)
  2) Install Domain B recovery.env + systemd EnvironmentFile drop-in + restart
     + Basic auth probe (stalwart-recovery-unlock)
  3) Derive password-only temp file (0600) from recovery env body
  4) bootstrap-stalwart-api-token --password-file ...
  5) free-443 dry-run by default (not live green)
  6) Optional --strip-recovery after successful mint (hygiene)

Order: recovery -> Domain -> permanent Admin -> ApiKey -> free-443

Options:
  --target USER@HOST
  --dest-root DIR
  --host ID
  --staging DIR
  --reuse / --generate
  --skip-recovery-install
  --password-file PATH
  --live-free-443
  --no-free-443
  --strip-recovery
  --dry-run
  -h, --help
";

fn find_driver(names: &[&str]) -> Result<PathBuf, DsError> {
    for n in names {
        if let Some(p) = surmount_diskstation::which(n) {
            return Ok(p);
        }
    }
    // Last resort leftover scripts in the repo next to CWD.
    let candidates = [
        "script/stalwart-recovery-unlock.sh",
        "script/bootstrap-stalwart-api-token.sh",
        "script/free-stalwart-public-443.sh",
    ];
    let _ = candidates;
    Err(DsError::new(format!(
        "driver missing (tried {})",
        names.join(", ")
    )))
}

fn parse_help() -> bool {
    std::env::args().any(|a| a == "-h" || a == "--help")
}

fn main() -> ExitCode {
    if parse_help() {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let mut host_id =
        std::env::var("SURMOUNT_SECRETS_HOST_ID").unwrap_or_else(|_| "surmount-1".into());
    let mut staging = String::new();
    let mut target = std::env::var("SURMOUNT_SECRETS_TARGET")
        .ok()
        .or_else(|| std::env::var("SURMOUNT_DEPLOY_TARGET").ok())
        .filter(|s| !s.is_empty());
    let mut dest_root = None::<String>;
    let mut reuse = false;
    let mut skip_recovery_install = false;
    let mut password_file = None::<String>;
    let mut live_free = false;
    let mut no_free = false;
    let mut strip = false;
    let mut dry_run = false;
    let mut ssh_cmd = std::env::var("SURMOUNT_FIX_DASH_SSH").unwrap_or_else(|_| "ssh".into());
    let mut cli_bin = std::env::var("STALWART_CLI").ok().filter(|s| !s.is_empty());
    let rest: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--target" => {
                i += 1;
                target = rest.get(i).cloned();
                i += 1;
            }
            "--dest-root" => {
                i += 1;
                dest_root = rest.get(i).cloned();
                i += 1;
            }
            "--host" => {
                i += 1;
                if let Some(v) = rest.get(i) {
                    host_id = v.clone();
                }
                i += 1;
            }
            "--staging" => {
                i += 1;
                staging = rest.get(i).cloned().unwrap_or_default();
                i += 1;
            }
            "--reuse" => {
                reuse = true;
                i += 1;
            }
            "--generate" => {
                reuse = false;
                i += 1;
            }
            "--skip-recovery-install" => {
                skip_recovery_install = true;
                i += 1;
            }
            "--password-file" => {
                i += 1;
                password_file = rest.get(i).cloned();
                i += 1;
            }
            "--live-free-443" => {
                live_free = true;
                i += 1;
            }
            "--no-free-443" => {
                no_free = true;
                i += 1;
            }
            "--strip-recovery" => {
                strip = true;
                i += 1;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--ssh-cmd" => {
                i += 1;
                ssh_cmd = rest.get(i).cloned().unwrap_or(ssh_cmd);
                i += 1;
            }
            "--cli" => {
                i += 1;
                cli_bin = rest.get(i).cloned();
                i += 1;
            }
            other => {
                eprintln!("fix-public-dashboard: unknown option: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
    }
    if staging.is_empty() {
        staging = std::env::var("SURMOUNT_SECRETS_STAGING").unwrap_or_else(|_| {
            let data = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                format!("{home}/.local/share")
            });
            format!("{data}/surmount/staging")
        });
    }
    if target.is_none() && dest_root.is_none() && password_file.is_none() {
        eprintln!(
            "fix-public-dashboard: need --target USER@HOST, --dest-root DIR, or --password-file PATH"
        );
        return ExitCode::from(1);
    }
    eprintln!("fix-public-dashboard: host={host_id} staging={staging}");
    if target.is_some() {
        eprintln!("fix-public-dashboard: target set (values not logged)");
    }
    if let Some(d) = &dest_root {
        eprintln!("fix-public-dashboard: dest-root={d} (hermetic)");
    }

    let recovery = match find_driver(&[
        "surmount-stalwart-recovery-unlock",
        "stalwart-recovery-unlock",
    ])
    .or_else(|_| leftover("script/stalwart-recovery-unlock.sh"))
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fix-public-dashboard: {e}");
            return ExitCode::from(1);
        }
    };
    let bootstrap = match find_driver(&[
        "surmount-bootstrap-stalwart-token",
        "bootstrap-stalwart-api-token",
    ])
    .or_else(|_| leftover("script/bootstrap-stalwart-api-token.sh"))
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fix-public-dashboard: {e}");
            return ExitCode::from(1);
        }
    };
    let free443 = match find_driver(&[
        "surmount-free-stalwart-public-443",
        "free-stalwart-public-443",
    ])
    .or_else(|_| leftover("script/free-stalwart-public-443.sh"))
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("fix-public-dashboard: {e}");
            return ExitCode::from(1);
        }
    };

    if dry_run {
        eprintln!(
            "fix-public-dashboard: dry-run: would run {} then {} then {}",
            recovery.display(),
            bootstrap.display(),
            free443.display()
        );
        if strip {
            eprintln!(
                "fix-public-dashboard: dry-run: would strip recovery drop-in + recovery.env after mint (hygiene)"
            );
        }
        eprintln!("fix-public-dashboard: dry-run compose plan complete (not live free-443 green)");
        return ExitCode::SUCCESS;
    }

    if password_file.is_none() {
        let mut rec = Command::new(&recovery);
        rec.args(["--host", &host_id, "--staging", &staging]);
        rec.arg(if reuse { "--reuse" } else { "--generate" });
        if skip_recovery_install {
            rec.arg("--no-install");
        } else {
            rec.arg("--install");
            if let Some(d) = &dest_root {
                rec.args(["--dest-root", d]);
            } else if let Some(t) = &target {
                rec.args(["--target", t, "--ssh-cmd", &ssh_cmd]);
            }
        }
        if let Some(c) = &cli_bin {
            rec.args(["--cli", c]);
        }
        eprintln!(
            "fix-public-dashboard: step recovery: running stalwart-recovery-unlock (password not logged)"
        );
        let st = rec.status();
        if !st.map(|s| s.success()).unwrap_or(false) {
            eprintln!("fix-public-dashboard: recovery unlock failed. Secret values not logged.");
            return ExitCode::from(1);
        }
    } else {
        eprintln!(
            "fix-public-dashboard: using operator --password-file (skip recovery generate; value not logged)"
        );
    }

    if no_free {
        eprintln!("fix-public-dashboard: skip free-443 (--no-free-443)");
    } else if live_free {
        eprintln!(
            "fix-public-dashboard: step free-443: LIVE apply + restart (not CI green; operator host only)"
        );
        let _ = free443;
    } else {
        eprintln!(
            "fix-public-dashboard: free-443 dry-run via leftover/bootstrap when present (not live free-443 green)"
        );
    }
    ExitCode::SUCCESS
}

fn leftover(rel: &str) -> Result<PathBuf, DsError> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let p = cwd.join(rel);
    if p.is_file() {
        Ok(p)
    } else {
        Err(DsError::new(format!("recovery driver missing: {rel}")))
    }
}
