//! Just recipe bodies, third ten names.
//! btop execs the Eternal Terminal client or local btop. It does not nix-run
//! that client. The other names nix-run an existing app, ssh, or nix eval.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::ToolError;
use crate::paths::discover_repo_root;
use crate::recipe_batch_01::{self, run_in};

const BATCH: &[&str] = &[
    "btop",
    "host-inxi",
    "et",
    "grok-oss",
    "grok-oss-running",
    "host-logs",
    "host-status",
    "scram",
    "test-logging-eval",
    "test-sshd-hostkeys-eval",
];

const HOST_STATUS_REMOTE: &str = "u=$(id -u nixbuilder 2>/dev/null || echo \"\"); echo gen=$(readlink /run/current-system | sed \"s|.*/||\"); echo qemu=$(systemctl is-active qemu-guest-agent 2>/dev/null || echo n/a); echo et=$(systemctl is-active eternal-terminal 2>/dev/null || echo n/a); for s in sshd stalwart-mail surmount-management-ui; do printf \"%s=%s \" \"$s\" \"$(systemctl is-active $s)\"; done; echo; if [[ -n \"$u\" ]]; then echo \"nixbuilder_uid=$u\"; systemctl show \"user-${u}.slice\" -p MemoryMax,MemoryCurrent,Description --no-pager; fi; echo lake=$(pgrep -c -x lake 2>/dev/null || echo 0)";

pub fn dispatch(args: &[String]) -> Option<Result<(), ToolError>> {
    let (name, rest) = recipe_batch_01::split_invocation(args)?;
    if !BATCH.contains(&name.as_str()) {
        return None;
    }
    Some(run(&name, &rest))
}

fn run(name: &str, args: &[String]) -> Result<(), ToolError> {
    match name {
        "btop" => run_btop(args),
        "host-inxi" => nix_run_app("surmount-inxi-host", args),
        "et" => nix_run_app("surmount-et", args),
        "grok-oss" => nix_run_app("surmount-grok-oss", args),
        "grok-oss-running" => {
            let mut prefixed = vec!["--running".to_string()];
            prefixed.extend(args.iter().cloned());
            nix_run_app("surmount-grok-oss", &prefixed)
        }
        "host-logs" => nix_run_app("surmount-host-logs", args),
        "host-status" => run_host_status(args),
        "scram" => run_scram(args),
        "test-logging-eval" => nix_eval_file("tests/logging-contract.nix"),
        "test-sshd-hostkeys-eval" => nix_eval_file("tests/sshd-hostkeys-contract.nix"),
        other => Err(ToolError::fail(format!(
            "surmount-deploy-host: batch 3 has no recipe {other}"
        ))),
    }
}

fn nix_run_app(app: &str, args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = vec!["run".to_string(), format!(".#{app}"), "--".to_string()];
    cmd.extend(args.iter().cloned());
    run_in(&root, "nix", &cmd)
}

fn nix_eval_file(file: &str) -> Result<(), ToolError> {
    let root = discover_repo_root();
    run_in(
        &root,
        "nix",
        &[
            "eval".to_string(),
            "--impure".to_string(),
            "--json".to_string(),
            "--file".to_string(),
            file.to_string(),
        ],
    )
}

fn run_btop(args: &[String]) -> Result<(), ToolError> {
    if Path::new("/etc/surmount/root-justfile").is_file() {
        return run_status("btop", args, &[]);
    }
    let home = env::var("HOME").unwrap_or_default();
    let identity = env::var("SURMOUNT_DEPLOY_SSH_IDENTITY")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("{home}/.ssh/id_ed25519"));
    let et = format!("{home}/.nix-profile/bin/et");
    let mut cmd = vec![
        "-k".to_string(),
        "5".to_string(),
        "-p".to_string(),
        "2022".to_string(),
        "--ssh-option".to_string(),
        format!("IdentityFile={identity}"),
        "-e".to_string(),
        "-c".to_string(),
        "btop".to_string(),
        "root@surmount-1".to_string(),
    ];
    cmd.extend(args.iter().cloned());
    run_status(&et, &cmd, &[])
}

fn run_host_status(args: &[String]) -> Result<(), ToolError> {
    let target = target_from_args(args, false)?;
    if target.is_empty() {
        return Err(ToolError {
            message: "host-status: set SURMOUNT_DEPLOY_TARGET or --target USER@HOST".into(),
            exit_code: 2,
        });
    }
    ssh(&target, &[HOST_STATUS_REMOTE.to_string()])
}

fn run_scram(args: &[String]) -> Result<(), ToolError> {
    let mut target = target_from_args(args, true)?;
    if target.is_empty() {
        target = "root@surmount-1".to_string();
    }
    eprintln!("scram: {target} surmount-scram --now");
    ssh(
        &target,
        &[
            "--".to_string(),
            "surmount-scram".to_string(),
            "--now".to_string(),
        ],
    )
}

fn target_from_args(args: &[String], _shift_target: bool) -> Result<String, ToolError> {
    let mut target = env::var("SURMOUNT_DEPLOY_TARGET").unwrap_or_default();
    if target.is_empty() {
        target = target_from_env_file();
    }
    if args.first().map(String::as_str) == Some("--target") {
        target = args.get(1).cloned().unwrap_or_default();
    }
    Ok(target)
}

fn target_from_env_file() -> String {
    let home = env::var("HOME").unwrap_or_default();
    let path = Path::new(&home).join(".local/share/surmount/agent-target.env");
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "SURMOUNT_DEPLOY_TARGET" {
            continue;
        }
        return unquote(value.trim());
    }
    String::new()
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn ssh(target: &str, remote: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "--".to_string(),
        target.to_string(),
    ];
    cmd.extend(remote.iter().cloned());
    run_in(&root, "ssh", &cmd)
}

fn run_status(bin: &str, args: &[String], _env: &[(&str, &str)]) -> Result<(), ToolError> {
    let status = Command::new(bin).args(args).status()?;
    if status.success() {
        return Ok(());
    }
    let code = status.code().unwrap_or(1);
    Err(ToolError {
        message: format!("command exited {code}"),
        exit_code: code,
    })
}
