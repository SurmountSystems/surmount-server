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

/// Remote `just host-status` body. sshd, stalwart-mail, and
/// surmount-management-ui must be active or this script exits non-zero.
/// qemu-guest-agent and eternal-terminal stay n/a when the unit is absent.
/// A failed required unit is printed as its real state, not n/a.
pub(crate) const HOST_STATUS_REMOTE: &str = r#"u=$(id -u nixbuilder 2>/dev/null || echo "")
echo gen=$(readlink /run/current-system | sed "s|.*/||")
echo qemu=$(systemctl is-active qemu-guest-agent 2>/dev/null || echo n/a)
echo et=$(systemctl is-active eternal-terminal 2>/dev/null || echo n/a)
rc=0
for s in sshd stalwart-mail surmount-management-ui; do
  st=$(systemctl is-active "$s" 2>/dev/null || true)
  printf '%s=%s ' "$s" "$st"
  if [ "$st" != active ]; then
    rc=1
  fi
done
echo
if [ -n "$u" ]; then
  echo "nixbuilder_uid=$u"
  systemctl show "user-${u}.slice" -p MemoryMax,MemoryCurrent,Description --no-pager || true
fi
echo lake=$(pgrep -c -x lake 2>/dev/null || echo 0)
expected=1a50824e06dec0680ea7e414255970e8618ddf13
splora_rev=$(systemctl show surmount-management-ui -p Environment --value --no-pager 2>/dev/null | tr ' ' '\n' | sed -n 's/^SURMOUNT_SPLORA_REV=//p' | head -n 1)
if [ -z "$splora_rev" ]; then
  splora_rev=absent
fi
echo "splora_rev=$splora_rev"
if [ "$splora_rev" = "$expected" ]; then
  echo splora_pin=pinned
else
  echo splora_pin=mismatch
fi
exit "$rc"
"#;

pub(crate) fn host_status_remote_script() -> &'static str {
    HOST_STATUS_REMOTE
}

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
    ssh(&target, &[host_status_remote_script().to_string()])
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::host_status_remote_script;

    const SYSTEMCTL: &str = r#"#!/bin/sh
set -eu
cmd=${1:-}
unit=${2:-}
case "$cmd" in
  is-active)
    case "$unit" in
      qemu-guest-agent|eternal-terminal)
        exit 4
        ;;
    esac
    fail=${HOST_STATUS_FAIL:-}
    case " $fail " in
      *" $unit "*)
        printf '%s\n' failed
        exit 3
        ;;
    esac
    printf '%s\n' active
    exit 0
    ;;
  show)
    printf '%s\n' "SURMOUNT_SPLORA_REV=${HOST_STATUS_SPLORA_REV:-1a50824e06dec0680ea7e414255970e8618ddf13}"
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
"#;

    fn temp_dir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "surmount-host-status-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn write_systemctl(dir: &Path) {
        let path = dir.join("systemctl");
        fs::write(&path, SYSTEMCTL).unwrap();
        let mut perm = fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(&path, perm).unwrap();
    }

    fn run(fail: &str) -> std::process::Output {
        run_rev(fail, None)
    }

    fn run_rev(fail: &str, rev: Option<&str>) -> std::process::Output {
        let dir = temp_dir();
        write_systemctl(&dir);
        let path = format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(host_status_remote_script())
            .env("PATH", path)
            .env("HOST_STATUS_FAIL", fail);
        if let Some(rev) = rev {
            cmd.env("HOST_STATUS_SPLORA_REV", rev);
        }
        cmd.output().expect("run host-status script")
    }

    fn text(out: &std::process::Output) -> String {
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    #[test]
    fn host_status_exits_zero_when_required_units_are_active_and_optional_units_are_absent() {
        let out = run("");
        let body = text(&out);
        assert!(
            out.status.success(),
            "active required units must exit 0: {body}"
        );
        assert!(body.contains("sshd=active"), "{body}");
        assert!(body.contains("stalwart-mail=active"), "{body}");
        assert!(body.contains("surmount-management-ui=active"), "{body}");
        assert!(body.contains("qemu=n/a"), "{body}");
        assert!(body.contains("et=n/a"), "{body}");
    }

    #[test]
    fn host_status_exits_nonzero_when_sshd_is_failed() {
        let out = run("sshd");
        let body = text(&out);
        assert_eq!(
            out.status.code(),
            Some(1),
            "failed sshd must exit 1, not 0: {body}"
        );
        assert!(body.contains("sshd=failed"), "{body}");
        assert!(!body.contains("sshd=n/a"), "{body}");
        assert!(body.contains("stalwart-mail=active"), "{body}");
    }

    #[test]
    fn host_status_exits_nonzero_when_stalwart_mail_is_failed() {
        let out = run("stalwart-mail");
        let body = text(&out);
        assert_eq!(out.status.code(), Some(1), "{body}");
        assert!(body.contains("stalwart-mail=failed"), "{body}");
        assert!(!body.contains("stalwart-mail=n/a"), "{body}");
    }

    #[test]
    fn host_status_observes_the_splora_pin() {
        let script = host_status_remote_script();
        assert!(
            script.contains("1a50824e06dec0680ea7e414255970e8618ddf13"),
            "{script}"
        );
        let out = run("");
        let body = text(&out);
        assert!(out.status.success(), "{body}");
        assert!(
            body.contains("splora_rev=1a50824e06dec0680ea7e414255970e8618ddf13"),
            "{body}"
        );
        assert!(body.contains("splora_pin=pinned"), "{body}");
        let bad = run_rev("", Some("0000000000000000000000000000000000000000"));
        let bad_body = text(&bad);
        assert!(bad.status.success(), "{bad_body}");
        assert!(bad_body.contains("splora_pin=mismatch"), "{bad_body}");
        assert!(
            bad_body.contains("splora_rev=0000000000000000000000000000000000000000"),
            "{bad_body}"
        );
    }

    #[test]
    fn host_status_exits_nonzero_when_management_ui_is_failed() {
        let out = run("surmount-management-ui");
        let body = text(&out);
        assert_eq!(out.status.code(), Some(1), "{body}");
        assert!(body.contains("surmount-management-ui=failed"), "{body}");
        assert!(!body.contains("surmount-management-ui=n/a"), "{body}");
    }
}
