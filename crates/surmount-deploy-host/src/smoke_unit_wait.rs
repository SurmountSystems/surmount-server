//! Poll a unit that is still starting. An immediate `activating` is not a failure.
//!
//! The wait is bounded. `failed`, `inactive`, and any other settled state
//! return immediately. A unit that is still `activating` when the bound
//! ends is not reported as active.

use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_WAIT_MS: u64 = 30_000;
const DEFAULT_POLL_MS: u64 = 1_000;

pub fn is_active(systemctl: &str, unit: &str) -> String {
    let out = Command::new(systemctl).args(["is-active", unit]).output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
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

/// Last observed `is-active` value. `active` means the unit came up inside
/// the bound. Any other string is not active, including `activating` when
/// the wait ends and `failed` when the unit never becomes active.
pub fn wait_until_settled(systemctl: &str, unit: &str) -> String {
    let wait = Duration::from_millis(env_u64("SURMOUNT_SMOKE_UNIT_WAIT_MS", DEFAULT_WAIT_MS));
    let poll_ms = env_u64("SURMOUNT_SMOKE_UNIT_POLL_MS", DEFAULT_POLL_MS).max(1);
    let poll = Duration::from_millis(poll_ms);
    let start = Instant::now();
    loop {
        let state = is_active(systemctl, unit);
        if state == "active" || !transitional(&state) {
            return state;
        }
        if start.elapsed() >= wait {
            return state;
        }
        let rest = wait.saturating_sub(start.elapsed());
        thread::sleep(poll.min(rest));
    }
}
