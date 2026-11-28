//! Just recipe bodies, first ten names.
//! `default`, `ci`, `check-remote`, `audit-remote`, `remote-nix-build`,
//! `audit`, `deny`, `fmt`, plus app entry points for dependency recipes
//! `check` and `check-ci`. Each one is `std::process::Command`.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;
use std::time::Duration;

use crate::error::ToolError;
use crate::paths::discover_repo_root;

const BATCH: &[&str] = &[
    "default",
    "check",
    "ci",
    "check-remote",
    "audit-remote",
    "remote-nix-build",
    "audit",
    "deny",
    "check-ci",
    "fmt",
];

pub fn dispatch(args: &[String]) -> Option<Result<(), ToolError>> {
    let (name, rest) = split_invocation(args)?;
    if !BATCH.contains(&name.as_str()) {
        return None;
    }
    Some(run(&name, &rest))
}

pub fn split_invocation(args: &[String]) -> Option<(String, Vec<String>)> {
    if args.is_empty() {
        return None;
    }
    let argv0 = file_name(&args[0]);
    let mut rest = args[1..].to_vec();
    strip_leading_ddash(&mut rest);
    if argv0 != "surmount-deploy-host" && argv0 != "surmount-deploy-host-post-switch-smoke" {
        return Some((argv0, rest));
    }
    let first = rest.first()?;
    if first.starts_with('-') {
        return None;
    }
    let name = rest.remove(0);
    strip_leading_ddash(&mut rest);
    Some((name, rest))
}

fn file_name(arg: &str) -> String {
    Path::new(arg)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(arg)
        .to_string()
}

fn strip_leading_ddash(args: &mut Vec<String>) {
    if args.first().map(String::as_str) == Some("--") {
        args.remove(0);
    }
}

fn run(name: &str, args: &[String]) -> Result<(), ToolError> {
    match name {
        "default" => run_default(),
        "check" => run_check(),
        "ci" => run_ci(),
        "check-remote" => run_check_remote(),
        "audit-remote" => run_audit_remote(),
        "remote-nix-build" => run_remote_nix_build(args),
        "audit" => run_audit(),
        "deny" => run_deny(),
        "check-ci" => run_ci(),
        "fmt" => run_fmt(false),
        other => Err(ToolError::fail(format!(
            "surmount-deploy-host: batch 1 has no recipe {other}"
        ))),
    }
}

fn run_default() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let justfile = root.join("justfile");
    run_in(
        &root,
        "just",
        &[
            "--list".to_string(),
            "--justfile".to_string(),
            justfile.display().to_string(),
            "--working-directory".to_string(),
            root.display().to_string(),
        ],
    )
}

fn run_check() -> Result<(), ToolError> {
    run_fmt(false)?;
    run_clippy()?;
    run_cargo(&["test".to_string()])
}

pub(crate) fn run_clippy() -> Result<(), ToolError> {
    run_cargo(&[
        "clippy".to_string(),
        "--all-targets".to_string(),
        "--".to_string(),
        "-D".to_string(),
        "warnings".to_string(),
    ])
}

pub(crate) fn run_fmt(write: bool) -> Result<(), ToolError> {
    let mut fmt_args = vec!["fmt".to_string()];
    if !write {
        fmt_args.push("--check".to_string());
    }
    run_cargo(&fmt_args)?;
    let root = discover_repo_root();
    let system = flake_system()?;
    let mut check_flag: Vec<String> = Vec::new();
    if !write {
        check_flag.push("--check".to_string());
    }
    let dirs = ["modules", "tests", "hosts", "nix"];
    let mut files = Vec::new();
    for dir in dirs {
        collect_nix(&root.join(dir), &mut files);
    }
    files.sort();
    if !files.is_empty() {
        nixfmt(&root, &system, &files, &check_flag)?;
    }
    let flake = root.join("flake.nix");
    nixfmt(&root, &system, &[flake], &check_flag)
}

fn nixfmt(
    root: &Path,
    system: &str,
    files: &[PathBuf],
    check_flag: &[String],
) -> Result<(), ToolError> {
    const CHUNK: usize = 80;
    for chunk in files.chunks(CHUNK) {
        let mut args = vec![
            "run".to_string(),
            format!(".#formatter.{system}"),
            "--".to_string(),
        ];
        args.extend(check_flag.iter().cloned());
        for file in chunk {
            args.push(file.display().to_string());
        }
        run_in(root, "nix", &args)?;
    }
    Ok(())
}

fn collect_nix(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut ents: Vec<_> = rd.flatten().collect();
    ents.sort_by_key(|ent| ent.file_name());
    for ent in ents {
        let path = ent.path();
        if path.is_dir() {
            collect_nix(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("nix") {
            out.push(path);
        }
    }
}

fn run_ci() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let system = flake_system()?;
    local_nix_build(&root, &format!(".#checks.{system}.ci"), false)
}

fn run_audit() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let system = flake_system()?;
    local_nix_build(&root, &format!(".#checks.{system}.cargo-audit"), true)
}

fn run_deny() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let system = flake_system()?;
    local_nix_build(&root, &format!(".#checks.{system}.cargo-deny-bans"), false)
}

fn run_check_remote() -> Result<(), ToolError> {
    let system = flake_system()?;
    run_remote_nix_build(&[format!(".#checks.{system}.ci"), "check-remote".to_string()])
}

fn run_audit_remote() -> Result<(), ToolError> {
    if env_nonempty("BUILD_LOCAL") {
        return Err(ToolError {
            message: "just audit-remote: refuse BUILD_LOCAL".into(),
            exit_code: 2,
        });
    }
    let system = flake_system()?;
    run_remote_nix_build(&[
        format!(".#checks.{system}.cargo-audit"),
        "audit-remote".to_string(),
    ])
}

fn run_remote_nix_build(args: &[String]) -> Result<(), ToolError> {
    if args.len() < 2 {
        return Err(ToolError {
            message: "remote-nix-build: need installable and label".into(),
            exit_code: 2,
        });
    }
    let installable = &args[0];
    let label = &args[1];
    if env_nonempty("BUILD_LOCAL") {
        return Err(ToolError {
            message: format!("just {label}: refuse BUILD_LOCAL (that is laptop rustc)"),
            exit_code: 2,
        });
    }
    let root = discover_repo_root();
    eprintln!("==> just {label}: nix build {installable} (max-jobs 0)");
    eprintln!("==> just {label}: eval can take a few minutes; then remote logs or a store path");
    eprintln!(
        "==> just {label}: ssh-ng keepalives via NIX_SSHOPTS; connection drops retry; derivation failures do not"
    );
    let max = 3u32;
    for attempt in 1..=max {
        let (code, log, stdout) = nix_build_captured(&root, installable)?;
        if code == 0 {
            eprintln!("==> just {label}: ok");
            let _ = std::io::stdout().write_all(&stdout);
            return Ok(());
        }
        if is_connection_drop(&log) {
            if attempt < max {
                eprintln!(
                    "==> just {label}: connection failed (attempt {attempt} of {max}); retrying the nix build"
                );
                thread::sleep(Duration::from_secs(2));
                continue;
            }
            eprintln!("==> just {label}: connection failed after {max} attempts");
            return Err(ToolError {
                message: format!("just {label}: connection failed after {max} attempts"),
                exit_code: 1,
            });
        }
        eprintln!("==> just {label}: build failed (not a connection drop); not retrying");
        return Err(ToolError {
            message: format!("just {label}: build failed"),
            exit_code: code,
        });
    }
    Err(ToolError {
        message: format!("just {label}: connection failed after {max} attempts"),
        exit_code: 1,
    })
}

fn nix_build_captured(root: &Path, installable: &str) -> Result<(i32, String, Vec<u8>), ToolError> {
    let mut child = Command::new("nix")
        .args([
            "build",
            "--option",
            "max-jobs",
            "0",
            "--option",
            "warn-dirty",
            "false",
            "--print-build-logs",
            "--print-out-paths",
            "--no-link",
            installable,
        ])
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::fail("remote-nix-build: nix stderr missing"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::fail("remote-nix-build: nix stdout missing"))?;
    let err_thread = thread::spawn(move || -> Vec<u8> {
        let mut acc = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match stderr.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = std::io::stderr().write_all(&buf[..n]);
                    let _ = std::io::stderr().flush();
                    acc.extend_from_slice(&buf[..n]);
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        acc
    });
    let mut out_acc = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stdout.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => out_acc.extend_from_slice(&buf[..n]),
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => {
                let _ = child.wait();
                let _ = err_thread.join();
                return Err(err.into());
            }
        }
    }
    let status = child.wait()?;
    let err_bytes = err_thread.join().unwrap_or_default();
    let log = String::from_utf8_lossy(&err_bytes).into_owned();
    Ok((exit_code(status), log, out_acc))
}

fn is_connection_drop(log: &str) -> bool {
    if log.contains("builder failed with exit code") || log.contains("Reason: builder failed") {
        return false;
    }
    const NEEDLES: &[&str] = &[
        "failed to start SSH connection",
        "failed to start SSH master connection",
        "unable to open connection to remote store",
        "unexpected end-of-file",
        "unexpected EOF",
        "kex_exchange_identification",
        "Connection reset by peer",
        "Connection timed out",
        "Connection refused",
        "Broken pipe",
        "closed by remote host",
    ];
    if NEEDLES.iter().any(|needle| log.contains(needle)) {
        return true;
    }
    log.contains("Timeout, server ") && log.contains(" not responding")
}

fn local_nix_build(root: &Path, installable: &str, warn_dirty_off: bool) -> Result<(), ToolError> {
    let mut args = vec!["build".to_string(), "--print-build-logs".to_string()];
    if warn_dirty_off {
        args.push("--option".to_string());
        args.push("warn-dirty".to_string());
        args.push("false".to_string());
    }
    args.push(installable.to_string());
    run_in(root, "nix", &args)
}

pub(crate) fn flake_system() -> Result<String, ToolError> {
    if let Ok(sys) = env::var("CI_SYSTEM") {
        if !sys.is_empty() {
            return Ok(sys);
        }
    }
    let out = Command::new("nix")
        .args([
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ])
        .output()?;
    if !out.status.success() {
        return Err(ToolError::fail("nix eval builtins.currentSystem failed"));
    }
    let sys = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sys.is_empty() {
        return Err(ToolError::fail("nix eval builtins.currentSystem was empty"));
    }
    Ok(sys)
}

fn env_nonempty(key: &str) -> bool {
    env::var(key).ok().is_some_and(|value| !value.is_empty())
}

fn run_cargo(args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    run_in(&root, "cargo", args)
}

pub(crate) fn run_in(root: &Path, bin: &str, args: &[String]) -> Result<(), ToolError> {
    let program = crate::tool_path::resolve_tool(bin);
    let mut cmd = Command::new(&program);
    cmd.args(args)
        .current_dir(root)
        .env("PATH", crate::tool_path::child_path());
    crate::tool_path::apply_nix_library_path(&mut cmd, &program);
    let status = cmd.status()?;
    status_result(status)
}

fn status_result(status: ExitStatus) -> Result<(), ToolError> {
    if status.success() {
        return Ok(());
    }
    let code = exit_code(status);
    Err(ToolError {
        message: format!("command exited {code}"),
        exit_code: code,
    })
}

fn exit_code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}
