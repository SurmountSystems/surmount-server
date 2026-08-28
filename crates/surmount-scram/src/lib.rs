//! Last line of defense for the mail host: if RAM is almost gone, SIGKILL
//! builder hogs (Lean/Lake/rustc/nixbld). Never mail, sshd, UI, et, or self.
//!
//! Threshold is **host** `MemAvailable`, not a process RSS target. Tests use
//! a fake proc tree; they never signal live PIDs.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const AVAIL_FLOOR_KIB: u64 = 16 * 1024 * 1024; // 16 GiB
pub const NIXBLD_UID_MIN: u32 = 30000;
pub const NIXBLD_UID_MAX: u32 = 34999;

const HOG_COMMS: &[&str] = &["lean", "lake", "rustc"];

const PROTECTED_COMMS: &[&str] = &[
    "systemd",
    "sshd",
    "sshd-session",
    "dbus-broker",
    "dbus-broker-launch",
    "systemd-journald",
    "journald",
    "stalwart",
    "stalwart-mail",
    "surmount-management-ui",
    "etserver",
    "etterminal",
    "surmount-scram",
    "just",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: i32,
    pub uid: u32,
    pub rss_kib: u64,
    pub comm: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub mem_available_kib: u64,
    pub procs: Vec<Proc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cfg {
    pub avail_floor_kib: u64,
    pub nixbuilder_uid: Option<u32>,
}

impl Default for Cfg {
    fn default() -> Self {
        Self {
            avail_floor_kib: AVAIL_FLOOR_KIB,
            nixbuilder_uid: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub pids: Vec<i32>,
    pub slice: Option<String>,
}

impl Plan {
    pub fn is_idle(&self) -> bool {
        self.pids.is_empty() && self.slice.is_none()
    }
}

pub fn parse_meminfo(text: &str) -> Option<u64> {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let num: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            return num.parse().ok();
        }
    }
    None
}

pub fn parse_status(text: &str, pid: i32) -> Option<Proc> {
    let mut comm = String::new();
    let mut uid = None;
    let mut rss = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("Name:") {
            comm = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("Uid:") {
            uid = v.split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(v) = line.strip_prefix("VmRSS:") {
            let num: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
            rss = num.parse().ok();
        }
    }
    Some(Proc {
        pid,
        uid: uid?,
        rss_kib: rss.unwrap_or(0),
        comm,
    })
}

pub fn is_protected(p: &Proc) -> bool {
    let c = p.comm.as_str();
    PROTECTED_COMMS.iter().any(|n| c == *n)
}

pub fn is_hog(p: &Proc, cfg: &Cfg) -> bool {
    if is_protected(p) {
        return false;
    }
    let c = p.comm.to_ascii_lowercase();
    if HOG_COMMS.iter().any(|n| c == *n) {
        return true;
    }
    if let Some(u) = cfg.nixbuilder_uid
        && p.uid == u
    {
        return true;
    }
    (NIXBLD_UID_MIN..=NIXBLD_UID_MAX).contains(&p.uid)
}

pub fn plan_scram(snap: &Snapshot, cfg: &Cfg) -> Plan {
    if snap.mem_available_kib >= cfg.avail_floor_kib {
        return Plan::default();
    }
    let mut hogs: Vec<&Proc> = snap.procs.iter().filter(|p| is_hog(p, cfg)).collect();
    hogs.sort_by_key(|b| std::cmp::Reverse(b.rss_kib));
    let pids: Vec<i32> = hogs.iter().map(|p| p.pid).collect();
    let slice = cfg.nixbuilder_uid.map(|u| format!("user-{u}.slice"));
    Plan { pids, slice }
}

pub fn read_snapshot(proc_root: &Path, cfg: &Cfg) -> io::Result<Snapshot> {
    let mem = fs::read_to_string(proc_root.join("meminfo"))?;
    let mem_available_kib = parse_meminfo(&mem)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "MemAvailable missing"))?;
    let mut procs = Vec::new();
    let rd = match fs::read_dir(proc_root) {
        Ok(r) => r,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Snapshot {
                mem_available_kib,
                procs,
            });
        }
        Err(e) => return Err(e),
    };
    for ent in rd.flatten() {
        let name = ent.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<i32>().ok()) else {
            continue;
        };
        let status_path = ent.path().join("status");
        let Ok(text) = fs::read_to_string(&status_path) else {
            continue;
        };
        if let Some(p) = parse_status(&text, pid) {
            let _ = cfg;
            procs.push(p);
        }
    }
    Ok(Snapshot {
        mem_available_kib,
        procs,
    })
}

pub fn lookup_nixbuilder_uid(passwd: &str) -> Option<u32> {
    for line in passwd.lines() {
        let mut parts = line.split(':');
        let name = parts.next()?;
        if name != "nixbuilder" {
            continue;
        }
        let _pw = parts.next()?;
        return parts.next()?.parse().ok();
    }
    None
}

/// SIGKILL listed PIDs. Errors are collected; we keep going.
pub fn kill_pids(pids: &[i32]) -> Vec<(i32, String)> {
    let mut errs = Vec::new();
    for &pid in pids {
        // SAFETY: kill is defined for any pid; ESRCH is reported as error.
        let rc = unsafe { libc_kill(pid, 9) };
        if rc != 0 {
            errs.push((pid, format!("kill {pid}: errno")));
        }
    }
    errs
}

#[cfg(unix)]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    unsafe { libc::kill(pid as libc::pid_t, sig) }
}

#[cfg(not(unix))]
unsafe fn libc_kill(_pid: i32, _sig: i32) -> i32 {
    -1
}

pub fn slice_unit(uid: u32) -> String {
    format!("user-{uid}.slice")
}

pub fn log_plan(mut w: impl Write, plan: &Plan, avail: u64) -> io::Result<()> {
    writeln!(
        w,
        "surmount-scram: MemAvailable={avail} KiB floor={} KiB pids={:?} slice={:?}",
        AVAIL_FLOOR_KIB, plan.pids, plan.slice
    )
}

pub fn proc_root_from_env() -> PathBuf {
    std::env::var("SURMOUNT_SCRAM_PROC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/proc"))
}

pub fn passwd_from_env() -> PathBuf {
    std::env::var("SURMOUNT_SCRAM_PASSWD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/etc/passwd"))
}
