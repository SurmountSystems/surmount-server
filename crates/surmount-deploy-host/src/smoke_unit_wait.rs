//! Poll a unit that is still starting. An immediate `activating` is not a failure.
//!
//! `failed` and `inactive` return immediately. A unit that stays `activating`
//! is polled until the wait bound. `active` passes only when a later sample
//! in that same bound is still `active`, with the same main PID, the same
//! NRestarts, and a main-process exit status other than 1.

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
    main_pid: String,
    n_restarts: String,
    exec_status: String,
}

fn show_props(systemctl: &str, unit: &str) -> UnitProps {
    let out = Command::new(systemctl)
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
            unit,
        ])
        .output();
    let mut props = UnitProps {
        main_pid: String::new(),
        n_restarts: String::new(),
        exec_status: String::new(),
    };
    let Ok(out) = out else {
        return props;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("MainPID=") {
            props.main_pid = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("NRestarts=") {
            props.n_restarts = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("ExecMainStatus=") {
            props.exec_status = v.trim().to_string();
        }
    }
    props
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

fn restarts_increased(before: &str, after: &str) -> bool {
    match (before.parse::<u64>(), after.parse::<u64>()) {
        (Ok(a), Ok(b)) => b > a,
        _ => false,
    }
}

fn pid_changed(before: &str, after: &str) -> bool {
    match (before.parse::<u64>(), after.parse::<u64>()) {
        (Ok(a), Ok(0)) if a > 0 => true,
        (Ok(a), Ok(b)) if a > 0 && b > 0 => a != b,
        _ => false,
    }
}

fn sample_problem(baseline: &UnitProps, sample: &UnitProps) -> Option<String> {
    if sample.exec_status == "1" {
        return Some("exit 1".to_string());
    }
    if restarts_increased(&baseline.n_restarts, &sample.n_restarts) {
        return Some("NRestarts increased".to_string());
    }
    if pid_changed(&baseline.main_pid, &sample.main_pid) {
        return Some("main PID changed".to_string());
    }
    None
}

/// `Active` means the unit stayed active for the sample window.
/// `NotActive` is `failed`, `inactive`, or a wait that ended still starting.
/// `Unstable` is an active unit that exited 1, gained NRestarts, or changed PID.
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

    let baseline = show_props(systemctl, unit);
    if baseline.exec_status == "1" {
        return UnitVerdict::Unstable {
            reason: "exit 1".to_string(),
        };
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
        let sample = show_props(systemctl, unit);
        if let Some(reason) = sample_problem(&baseline, &sample) {
            return UnitVerdict::Unstable { reason };
        }
        if stable_start.elapsed() >= wait {
            return UnitVerdict::Active;
        }
    }
}
