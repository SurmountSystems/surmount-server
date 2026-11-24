//! Poll a unit that is still starting. An immediate `activating` is not a failure.
//!
//! `failed` and `inactive` return immediately. A unit that stays `activating`
//! is polled until the wait bound. `active` passes only when a later sample
//! in that same bound is still `active`, with the same main PID, the same
//! NRestarts, a main-process exit status of 0 or empty, and Result=success.
//! A non-zero ExecMainStatus, a Result= other than success, a missing Result=,
//! a failed or unparsed systemctl show, or an NRestarts value that does not
//! parse is not stable.

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_WAIT_MS: u64 = 30_000;
const DEFAULT_POLL_MS: u64 = 1_000;

#[derive(Debug, PartialEq, Eq)]
pub enum UnitVerdict {
    Active,
    NotActive { state: String },
    Unstable { reason: String },
}

pub fn is_active(systemctl: &str, unit: &str) -> String {
    let out = Command::new(systemctl).args(["is-active", unit]).output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

struct UnitProps {
    main_pid: u64,
    n_restarts: u64,
    /// `None` is an empty ExecMainStatus. A missing field stays an error.
    exec_status: Option<i64>,
    result: String,
}

fn show_props(systemctl: &str, unit: &str) -> Result<UnitProps, String> {
    let out = match Command::new(systemctl)
        .args([
            "show",
            "-p",
            "ActiveState",
            "-p",
            "MainPID",
            "-p",
            "NRestarts",
            "-p",
            "ExecMainStatus",
            "-p",
            "Result",
            unit,
        ])
        .output()
    {
        Ok(out) => out,
        Err(_) => return Err("systemctl show failed".to_string()),
    };
    if !out.status.success() {
        return Err("systemctl show failed".to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut main_pid: Option<u64> = None;
    let mut n_restarts: Option<u64> = None;
    let mut exec_status: Option<Option<i64>> = None;
    let mut result: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("MainPID=") {
            match v.trim().parse::<u64>() {
                Ok(n) => main_pid = Some(n),
                Err(_) => return Err("systemctl show unparsed".to_string()),
            }
        } else if let Some(v) = line.strip_prefix("NRestarts=") {
            match v.trim().parse::<u64>() {
                Ok(n) => n_restarts = Some(n),
                Err(_) => return Err("NRestarts did not parse".to_string()),
            }
        } else if let Some(v) = line.strip_prefix("ExecMainStatus=") {
            let v = v.trim();
            if v.is_empty() {
                exec_status = Some(None);
            } else {
                match v.parse::<i64>() {
                    Ok(n) => exec_status = Some(Some(n)),
                    Err(_) => return Err("systemctl show unparsed".to_string()),
                }
            }
        } else if let Some(v) = line.strip_prefix("Result=") {
            result = Some(v.trim().to_string());
        }
    }
    let Some(main_pid) = main_pid else {
        return Err("systemctl show unparsed".to_string());
    };
    let Some(n_restarts) = n_restarts else {
        return Err("NRestarts did not parse".to_string());
    };
    let Some(exec_status) = exec_status else {
        return Err("systemctl show unparsed".to_string());
    };
    let Some(result) = result else {
        return Err("Result missing".to_string());
    };
    Ok(UnitProps {
        main_pid,
        n_restarts,
        exec_status,
        result,
    })
}

fn env_u64(key: &str, default: u64) -> u64 {
    match std::env::var(key) {
        Ok(raw) => raw.trim().parse::<u64>().unwrap_or(default),
        Err(_) => default,
    }
}

fn transitional(state: &str) -> bool {
    matches!(state, "activating" | "deactivating" | "reloading")
}

fn pid_changed(before: u64, after: u64) -> bool {
    if before > 0 && after == 0 {
        return true;
    }
    before > 0 && after > 0 && before != after
}

fn exec_failure(status: Option<i64>) -> Option<String> {
    match status {
        Some(code) if code != 0 => Some(format!("exit {code}")),
        _ => None,
    }
}

fn result_failure(result: &str) -> Option<String> {
    if result == "success" {
        None
    } else {
        Some(format!("Result {result}"))
    }
}

fn sample_problem(baseline: &UnitProps, sample: &UnitProps) -> Option<String> {
    if let Some(reason) = exec_failure(sample.exec_status) {
        return Some(reason);
    }
    if let Some(reason) = result_failure(&sample.result) {
        return Some(reason);
    }
    if sample.n_restarts > baseline.n_restarts {
        return Some("NRestarts increased".to_string());
    }
    if pid_changed(baseline.main_pid, sample.main_pid) {
        return Some("main PID changed".to_string());
    }
    None
}

/// `Active` means the unit stayed active for the sample window.
/// `NotActive` is `failed`, `inactive`, or a wait that ended still starting.
/// `Unstable` is an active unit that did not stay stable: non-zero exit,
/// Result= other than success, a missing Result=, unparsed show, NRestarts
/// that do not parse or increased, or a new main PID.
pub fn wait_until_settled(systemctl: &str, unit: &str) -> UnitVerdict {
    let wait = Duration::from_millis(env_u64("SURMOUNT_SMOKE_UNIT_WAIT_MS", DEFAULT_WAIT_MS));
    let poll_ms = env_u64("SURMOUNT_SMOKE_UNIT_POLL_MS", DEFAULT_POLL_MS).max(1);
    let poll = Duration::from_millis(poll_ms);
    let start = Instant::now();

    loop {
        let state = is_active(systemctl, unit);
        if state == "failed" || state == "inactive" || (state != "active" && !transitional(&state))
        {
            return UnitVerdict::NotActive { state };
        }
        if state == "active" {
            break;
        }
        if start.elapsed() >= wait {
            return UnitVerdict::NotActive { state };
        }
        let rest = wait.saturating_sub(start.elapsed());
        thread::sleep(poll.min(rest));
    }

    let baseline = match show_props(systemctl, unit) {
        Ok(props) => props,
        Err(reason) => return UnitVerdict::Unstable { reason },
    };
    if let Some(reason) = exec_failure(baseline.exec_status) {
        return UnitVerdict::Unstable { reason };
    }
    if let Some(reason) = result_failure(&baseline.result) {
        return UnitVerdict::Unstable { reason };
    }

    let stable_start = Instant::now();
    loop {
        let rest = wait.saturating_sub(stable_start.elapsed());
        let pause = if rest.is_zero() { poll } else { poll.min(rest) };
        thread::sleep(pause);
        let state = is_active(systemctl, unit);
        if state == "failed" || state == "inactive" {
            return UnitVerdict::NotActive { state };
        }
        if state != "active" {
            let shown = if state.is_empty() {
                "unknown".to_string()
            } else {
                state
            };
            return UnitVerdict::Unstable {
                reason: format!("left active (is-active={shown})"),
            };
        }
        let sample = match show_props(systemctl, unit) {
            Ok(props) => props,
            Err(reason) => return UnitVerdict::Unstable { reason },
        };
        if let Some(reason) = sample_problem(&baseline, &sample) {
            return UnitVerdict::Unstable { reason };
        }
        if stable_start.elapsed() >= wait {
            return UnitVerdict::Active;
        }
    }
}
