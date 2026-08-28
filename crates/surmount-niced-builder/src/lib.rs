//! Niced + memory-capped helper for ssh-ng `nix-daemon --stdio`.
//!
//! Niceness is not a memory cap. This binary applies `nice -n 19`, idle
//! ionice, an about-95-percent disk guard, and `systemd-run --user`
//! MemoryMax/CPUQuota when a user manager is available. Mail and other
//! critical units must not use this wrapper. Host MemoryMax also lives on
//! the NixOS module (`nix-daemon.service` and builder slices).

use std::env;
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const USAGE: &str = "\
Usage: surmount-niced-builder [--] command [args...]

Run nix-daemon --stdio under nice -n 19, idle ionice, and a
hard memory cap. systemd-run --user applies MemoryMax and CPUQuota on
surmount-builder.slice when a user manager is present. Otherwise the
nixbuilder user slice MemoryMax (NixOS module) still applies.

This is not a mail unit wrapper. Do not point stalwart, management-ui,
sshd, or Arti at this binary.

Env:
  SURMOUNT_BUILDER_MEMORY_MAX   systemd MemoryMax (scaffold default 4G, not
                                a published guest RAM size)
  SURMOUNT_BUILDER_CPU_QUOTA    systemd CPUQuota (auto = 95 percent
                                times nproc; do not use 95% of one CPU)
  SURMOUNT_BUILDER_DISK_GUARD_PERCENT
                                refuse when df used-percent is >= this
                                (default 95)
  SURMOUNT_BUILDER_DISK_GUARD_PATH
                                filesystem path to measure (default /)
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Help,
    Run { cmd: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub memory_max: String,
    pub cpu_quota: String,
    pub disk_guard: u32,
    pub disk_path: String,
    pub slice: String,
    pub df_bin: String,
    pub use_systemd_run: bool,
    pub nproc: u32,
}

impl Settings {
    pub fn from_env() -> Result<Self, NicedError> {
        let disk_guard_raw =
            env::var("SURMOUNT_BUILDER_DISK_GUARD_PERCENT").unwrap_or_else(|_| "95".into());
        let disk_guard: u32 = disk_guard_raw.parse().map_err(|_| NicedError {
            message: "SURMOUNT_BUILDER_DISK_GUARD_PERCENT must be an integer".into(),
            exit_code: 1,
        })?;
        let cpu_quota_raw =
            env::var("SURMOUNT_BUILDER_CPU_QUOTA").unwrap_or_else(|_| "auto".into());
        let nproc = online_cpus();
        let cpu_quota = if cpu_quota_raw.is_empty() || cpu_quota_raw == "auto" {
            format!("{}%", 95u32.saturating_mul(nproc))
        } else {
            cpu_quota_raw
        };
        Ok(Self {
            memory_max: env::var("SURMOUNT_BUILDER_MEMORY_MAX").unwrap_or_else(|_| "4G".into()),
            cpu_quota,
            disk_guard,
            disk_path: env::var("SURMOUNT_BUILDER_DISK_GUARD_PATH").unwrap_or_else(|_| "/".into()),
            slice: env::var("SURMOUNT_BUILDER_SLICE")
                .unwrap_or_else(|_| "surmount-builder.slice".into()),
            df_bin: env::var("SURMOUNT_BUILDER_DF").unwrap_or_else(|_| "df".into()),
            use_systemd_run: env::var("SURMOUNT_BUILDER_USE_SYSTEMD_RUN")
                .map(|v| v != "0")
                .unwrap_or(true),
            nproc,
        })
    }
}

#[derive(Debug)]
pub struct NicedError {
    pub message: String,
    pub exit_code: i32,
}

impl std::fmt::Display for NicedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for NicedError {}

pub fn parse_args<I, S>(args: I) -> Result<Mode, NicedError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let rest: Vec<String> = iter.map(|s| s.as_ref().to_string()).collect();
    if rest.first().map(String::as_str) == Some("-h")
        || rest.first().map(String::as_str) == Some("--help")
    {
        return Ok(Mode::Help);
    }
    let mut i = 0;
    if rest.first().map(String::as_str) == Some("--") {
        i = 1;
    }
    if i >= rest.len() {
        return Err(NicedError {
            message: "command required".into(),
            exit_code: 1,
        });
    }
    Ok(Mode::Run {
        cmd: rest[i..].to_vec(),
    })
}

pub fn online_cpus() -> u32 {
    if let Ok(v) = env::var("SURMOUNT_BUILDER_NPROC") {
        if let Ok(n) = v.parse::<u32>() {
            if n > 0 {
                return n;
            }
        }
    }
    if let Ok(out) = Command::new("nproc").output() {
        if out.status.success() {
            if let Ok(n) = String::from_utf8_lossy(&out.stdout).trim().parse::<u32>() {
                if n > 0 {
                    return n;
                }
            }
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

/// Parse POSIX `df -P` used-percent from the second data row (`$5`).
pub fn parse_df_used_percent(df_out: &str) -> Option<u32> {
    let line = df_out.lines().nth(1)?;
    let col = line.split_whitespace().nth(4)?;
    col.trim_end_matches('%').parse().ok()
}

pub fn disk_used_percent(df_bin: &str, path: &str) -> Result<u32, NicedError> {
    let out = Command::new(df_bin)
        .args(["-P", path])
        .output()
        .map_err(|_| NicedError {
            message: format!("could not read disk use for {path}"),
            exit_code: 1,
        })?;
    let text = String::from_utf8_lossy(&out.stdout);
    parse_df_used_percent(&text).ok_or_else(|| NicedError {
        message: format!("could not read disk use for {path}"),
        exit_code: 1,
    })
}

pub fn user_bus_socket() -> PathBuf {
    let runtime = env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        let uid = rustix_uid();
        format!("/run/user/{uid}")
    });
    PathBuf::from(runtime).join("systemd/private")
}

fn rustix_uid() -> u32 {
    #[cfg(unix)]
    {
        libc_uid()
    }
    #[cfg(not(unix))]
    {
        0
    }
}

#[cfg(unix)]
fn libc_uid() -> u32 {
    // Avoid a libc crate dep: read /proc/self/status or use nix-less libc via std.
    // std has no getuid; parse /proc.
    if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                if let Some(uid) = rest.split_whitespace().next() {
                    if let Ok(n) = uid.parse() {
                        return n;
                    }
                }
            }
        }
    }
    0
}

pub fn systemd_run_available(use_flag: bool) -> bool {
    if !use_flag {
        return false;
    }
    if resolve_program("systemd-run").is_err() {
        return false;
    }
    let sock = user_bus_socket();
    sock.is_file() || UnixStream::connect(&sock).is_ok()
}

pub fn resolve_program(spec: &str) -> Result<PathBuf, NicedError> {
    let p = Path::new(spec);
    if spec.contains('/') {
        if p.is_file() {
            return Ok(p.to_path_buf());
        }
        return Err(NicedError {
            message: format!("{spec} not found"),
            exit_code: 1,
        });
    }
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            let cand = Path::new(dir).join(spec);
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }
    Err(NicedError {
        message: format!("{spec} not found"),
        exit_code: 1,
    })
}

pub fn systemd_run_args(settings: &Settings, cmd: &[String]) -> Vec<String> {
    let mut v = vec![
        "--user".into(),
        "--quiet".into(),
        "--collect".into(),
        "--pipe".into(),
        "--wait".into(),
        format!("--slice={}", settings.slice),
        format!("--property=MemoryMax={}", settings.memory_max),
        format!("--property=CPUQuota={}", settings.cpu_quota),
        "--property=Nice=19".into(),
        "--property=IOSchedulingClass=idle".into(),
        "--".into(),
    ];
    v.extend(cmd.iter().cloned());
    v
}

/// Guard disk, then exec systemd-run or nice+ionice. Does not return on success.
pub fn run(settings: &Settings, cmd: &[String]) -> Result<(), NicedError> {
    let used = disk_used_percent(&settings.df_bin, &settings.disk_path)?;
    if used >= settings.disk_guard {
        return Err(NicedError {
            message: format!(
                "refuse: {} is {used}% full (guard {}%)",
                settings.disk_path, settings.disk_guard
            ),
            exit_code: 1,
        });
    }
    if systemd_run_available(settings.use_systemd_run) {
        let bin = resolve_program("systemd-run")?;
        let args = systemd_run_args(settings, cmd);
        let err = Command::new(bin).args(&args).exec();
        return Err(NicedError {
            message: format!("systemd-run exec failed: {err}"),
            exit_code: 1,
        });
    }
    let nice = resolve_program("nice").unwrap_or_else(|_| PathBuf::from("nice"));
    let mut argv = vec!["-n".into(), "19".into(), "ionice".into(), "-c3".into()];
    argv.extend(cmd.iter().cloned());
    let err = Command::new(nice).args(&argv).exec();
    Err(NicedError {
        message: format!("nice/ionice exec failed: {err}"),
        exit_code: 1,
    })
}

pub fn print_help() -> io::Result<()> {
    let mut out = io::stdout();
    out.write_all(USAGE.as_bytes())?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_mode() {
        assert_eq!(
            parse_args(["surmount-niced-builder", "--help"]).unwrap(),
            Mode::Help
        );
    }

    #[test]
    fn missing_command() {
        let err = parse_args(["surmount-niced-builder"]).unwrap_err();
        assert!(err.message.contains("command"));
    }

    #[test]
    fn dash_dash_command() {
        match parse_args(["surmount-niced-builder", "--", "/bin/true"]).unwrap() {
            Mode::Run { cmd } => assert_eq!(cmd, ["/bin/true"]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn usage_names_memory_nice_disk_not_lake() {
        let u = USAGE.to_ascii_lowercase();
        assert!(USAGE.contains("MemoryMax"));
        assert!(u.contains("nice"));
        assert!(u.contains("disk") || u.contains("95"));
        assert!(!u.contains("lake"));
    }

    #[test]
    fn parse_df_second_row_percent() {
        let sample = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n\
/dev/dummy        1000000     000000     00000      94% /\n";
        assert_eq!(parse_df_used_percent(sample), Some(94));
        let full = "Filesystem     1024-blocks      Used Available Capacity Mounted on\n\
/dev/dummy        1000000     000000     00000      95% /\n";
        assert_eq!(parse_df_used_percent(full), Some(95));
    }

    #[test]
    fn auto_cpu_quota_scales_with_nproc() {
        unsafe { env::set_var("SURMOUNT_BUILDER_NPROC", "8") };
        unsafe { env::set_var("SURMOUNT_BUILDER_CPU_QUOTA", "auto") };
        unsafe { env::set_var("SURMOUNT_BUILDER_USE_SYSTEMD_RUN", "0") };
        let s = Settings::from_env().unwrap();
        assert_eq!(s.cpu_quota, "760%");
        assert_eq!(s.nproc, 8);
        unsafe { env::remove_var("SURMOUNT_BUILDER_NPROC") };
        unsafe { env::remove_var("SURMOUNT_BUILDER_CPU_QUOTA") };
    }

    #[test]
    fn systemd_run_args_include_caps() {
        let s = Settings {
            memory_max: "4G".into(),
            cpu_quota: "760%".into(),
            disk_guard: 95,
            disk_path: "/".into(),
            slice: "surmount-builder.slice".into(),
            df_bin: "df".into(),
            use_systemd_run: true,
            nproc: 8,
        };
        let args = systemd_run_args(&s, &["nix-daemon".into(), "--stdio".into()]);
        assert!(args.iter().any(|a| a.contains("MemoryMax=4G")));
        assert!(args.iter().any(|a| a.contains("CPUQuota=760%")));
        assert!(args.iter().any(|a| a.contains("Nice=19")));
        assert!(args.contains(&"--user".into()));
    }
}
