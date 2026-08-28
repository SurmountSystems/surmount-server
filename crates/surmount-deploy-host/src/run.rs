use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::args::{DeployOpts, Mode, USAGE, validate_secrets_kind};
use crate::error::ToolError;
use crate::keys::check_host_local;
use crate::paths::{
    assert_safe_token, discover_repo_root, is_path_under_root, is_public_product_path, realpath_m,
    shell_quote,
};

const RSYNC_EXCLUDES: &[&str] = &[
    "--exclude",
    ".git/",
    "--exclude",
    "result",
    "--exclude",
    "result-*",
    "--exclude",
    "crates/target/",
    "--exclude",
    "host-local/",
    "--exclude",
    "**/host-local/",
    "--exclude",
    ".agents/",
    "--exclude",
    ".env",
    "--exclude",
    ".env.*",
    "--exclude",
    "*.pem",
    "--exclude",
    "*.agekey",
    "--exclude",
    "*.p12",
    "--exclude",
    "*.pfx",
    "--exclude",
    "id_ed25519",
    "--exclude",
    "id_rsa",
    "--exclude",
    "id_ecdsa",
    "--exclude",
    "id_dsa",
    "--exclude",
    "keys.txt",
    "--exclude",
    "secrets.yaml",
    "--exclude",
    "secrets.yml",
    "--exclude",
    "secrets/",
    "--exclude",
    "**/secrets.yaml",
    "--exclude",
    "**/secrets.yml",
];

const POST_SMOKE_TAIL: &str = r#"
deploy-host: post-switch smoke is automatic (generation + units + loopback /health).
Optional deeper checks (operator):
  # When public HTTPS is live:
  #   SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://... just e2e-host
  # DNS / TLS / mail ports:
  #   nix run .#surmount-domain-audit -- --help
  # Re-run smoke only:
  #   ssh -- <target> surmount-deploy-host-post-switch-smoke

If switch hangs on "reloading user units for root":
  Prefer a second SSH session before risky switches. See
  docs/deploy-host-local.md (activation reliability) and docs/OPS.md.
  Verify generation + units with post-switch smoke after recovery.
"#;

pub fn run<I, S>(args: I) -> Result<(), ToolError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    match crate::args::parse_args(args)? {
        Mode::Help => {
            print!("{USAGE}");
            Ok(())
        }
        Mode::Run(opts) => run_deploy(opts),
    }
}

fn run_deploy(opts: DeployOpts) -> Result<(), ToolError> {
    let repo_root = discover_repo_root();
    let mut notes = io::stderr();

    if !opts.skip_host_local_check {
        let Some(hl) = opts.host_local.as_ref() else {
            return Err(ToolError::fail(
                "host-local required for safety checks: pass --host-local DIR or SURMOUNT_HOST_LOCAL_DIR (or --skip-host-local-check)",
            ));
        };
        check_host_local(hl, &mut notes)?;
    }

    let mut secrets_install_sh: Option<PathBuf> = None;
    let mut secrets_staging_canon: Option<PathBuf> = None;
    if opts.install_secrets {
        let Some(host_id) = opts.secrets_host_id.as_ref() else {
            return Err(ToolError::fail(
                "install-secrets requires --secrets-host-id ID or SURMOUNT_SECRETS_HOST_ID (logical host id for the install bridge; never a committed public IP)",
            ));
        };
        if host_id.starts_with('-') {
            return Err(ToolError::fail(format!(
                "secrets-host-id must not start with '-': option-shaped values are rejected (got {host_id})"
            )));
        }
        assert_safe_token("secrets-host-id", host_id)?;

        let src_count = usize::from(opts.secrets_staging.is_some())
            + usize::from(opts.secrets_from_secret_service);
        if src_count == 0 {
            return Err(ToolError::fail(
                "install-secrets requires a source: --secrets-staging DIR or --secrets-from-secret-service (or matching env). Default deploy does not auto-install secrets. See docs/SECRETS.md",
            ));
        }
        if src_count > 1 {
            return Err(ToolError::fail(
                "install-secrets: choose exactly one source (staging or secret-service), not both",
            ));
        }
        if let Some(stg) = opts.secrets_staging.as_ref() {
            if !stg.is_dir() {
                return Err(ToolError::fail(format!(
                    "secrets staging dir missing or not a directory: {}",
                    stg.display()
                )));
            }
            let canon = realpath_m(stg);
            if is_path_under_root(&repo_root, &canon) {
                return Err(ToolError::fail(format!(
                    "refuse: secrets staging must be outside the public git work tree ({}). Public rsync of the flake would copy staging payloads into the remote checkout. Use a private path outside the repo. See docs/SECRETS.md and docs/hygiene.md",
                    stg.display()
                )));
            }
            secrets_staging_canon = Some(canon);
        }
        for kind in &opts.secrets_require_kinds {
            validate_secrets_kind(kind)?;
        }
        let sh = resolve_secrets_install()?;
        secrets_install_sh = Some(sh);
    }

    let hl_rsync_flags: &[&str] = if opts.host_local_delete {
        &["-az", "--delete"]
    } else {
        &["-az"]
    };

    if let Some(dest) = opts.sync_host_local_into.as_ref() {
        if is_public_product_path(dest, &repo_root) {
            return Err(ToolError::fail(format!(
                "refuse: will not copy host-local into public product path ({}). Host-local stays off git (hosts/, secrets/). See docs/deploy-host-local.md and docs/hygiene.md",
                dest.display()
            )));
        }
        let Some(hl) = opts.host_local.as_ref() else {
            return Err(ToolError::fail(
                "--sync-host-local-into requires --host-local",
            ));
        };
        if opts.dry_run {
            println!(
                "deploy-host: dry-run: would rsync host-local -> {} (private destination; flags: {})",
                dest.display(),
                hl_rsync_flags.join(" ")
            );
        } else {
            fs::create_dir_all(dest)?;
            rsync_local(hl_rsync_flags, hl, dest)?;
            println!(
                "deploy-host: synced host-local to private destination {}",
                dest.display()
            );
        }
    }

    if !opts.skip_sync {
        if opts.dry_run {
            if let Some(id) = operator_ssh_identity(&opts.target) {
                println!(
                    "deploy-host: dry-run: ssh as {} uses {} (plus keepalives). Override: SURMOUNT_DEPLOY_SSH_IDENTITY",
                    opts.target,
                    id.display()
                );
            }
            println!(
                "deploy-host: dry-run: rsync public tree -> {}:{} / (excludes host-local, secrets patterns; not private-data admission control)",
                opts.target,
                shell_quote(&opts.remote_dir)
            );
            let excl = RSYNC_EXCLUDES.join(" ");
            println!(
                "deploy-host: dry-run: rsync -az --delete {excl} {}/ {}:{} /",
                shell_quote(&repo_root.to_string_lossy()),
                opts.target,
                shell_quote(&opts.remote_dir)
            );
        } else {
            ssh_run(
                &opts.target,
                &format!("mkdir -p {}", shell_quote(&opts.remote_dir)),
            )?;
            rsync_remote(
                &["-az", "--delete"],
                RSYNC_EXCLUDES,
                &repo_root,
                &opts.target,
                &opts.remote_dir,
            )?;
        }
    }

    if opts.host_local.is_some() && !opts.skip_host_local_check {
        let remote_hl = format!("{}/host-local", opts.remote_dir.trim_end_matches('/'));
        if opts.dry_run {
            println!(
                "deploy-host: dry-run: ensure remote host-local at {}:{} (operator-placed; not from public git)",
                opts.target,
                shell_quote(&remote_hl)
            );
            println!(
                "deploy-host: dry-run: rsync {} private host-local -> {}:{}/ (never into tracked hosts/; default without --delete unless --host-local-delete)",
                hl_rsync_flags.join(" "),
                opts.target,
                shell_quote(&remote_hl)
            );
        } else if let Some(hl) = opts.host_local.as_ref() {
            ssh_run(
                &opts.target,
                &format!("mkdir -p {}", shell_quote(&remote_hl)),
            )?;
            rsync_remote(hl_rsync_flags, &[], hl, &opts.target, &remote_hl)?;
        }
    } else if opts.skip_host_local_check {
        eprintln!(
            "deploy-host: note: --skip-host-local-check set: skipping host-local key checks and remote host-local rsync"
        );
    }

    if opts.install_secrets {
        let sh = secrets_install_sh.expect("checked above");
        let mut secrets_cmd: Vec<String> = vec![sh.to_string_lossy().into_owned()];
        if opts.dry_run {
            secrets_cmd.push("--dry-run".into());
        }
        if let Some(canon) = secrets_staging_canon.as_ref() {
            secrets_cmd.push("--from-staging".into());
            secrets_cmd.push(canon.to_string_lossy().into_owned());
        } else if let Some(stg) = opts.secrets_staging.as_ref() {
            secrets_cmd.push("--from-staging".into());
            secrets_cmd.push(stg.to_string_lossy().into_owned());
        }
        if opts.secrets_from_secret_service {
            secrets_cmd.push("--from-secret-service".into());
        }
        secrets_cmd.push("--host-id".into());
        secrets_cmd.push(opts.secrets_host_id.clone().unwrap());
        secrets_cmd.push("--target".into());
        secrets_cmd.push(opts.target.clone());
        for kind in &opts.secrets_require_kinds {
            secrets_cmd.push("--require-kind".into());
            secrets_cmd.push(kind.clone());
        }
        if opts.dry_run {
            print!("deploy-host: dry-run: would run secrets-install-host:");
            for a in &secrets_cmd {
                print!(" {}", shell_quote(a));
            }
            println!();
        } else {
            eprintln!(
                "deploy-host: running secrets-install-host (host-id={}; values not logged)",
                opts.secrets_host_id.as_deref().unwrap_or("")
            );
            run_cmd(&secrets_cmd)?;
        }
    }

    let rebuild_flake_ref = format!("path:{}#{}", opts.remote_dir, opts.flake_attr);
    let rebuild_remote_cmd = format!(
        "nixos-rebuild switch --flake {}",
        shell_quote(&rebuild_flake_ref)
    );
    let smoke_name = smoke_remote_name();
    let smoke_remote_cmd = format!(
        "if command -v {smoke} >/dev/null 2>&1; then {smoke}; else echo 'deploy-host: remote smoke missing (surmount-deploy-host-post-switch-smoke not on PATH after switch)' >&2; exit 1; fi",
        smoke = smoke_name
    );

    let mut rebuild_rc = 0i32;
    let mut smoke_rc = 0i32;
    if opts.dry_run {
        println!(
            "deploy-host: dry-run: rebuild uses path: flake so host-local (rsync'd, not in public git) is visible to Nix (git flake would omit untracked/gitignored paths)"
        );
        println!(
            "deploy-host: dry-run: ssh -- {} {}",
            shell_quote(&opts.target),
            shell_quote(&rebuild_remote_cmd)
        );
        println!(
            "deploy-host: dry-run: post-switch smoke always runs after rebuild (even if rebuild exit non-zero): ssh -- {} {}",
            shell_quote(&opts.target),
            shell_quote(&smoke_remote_cmd)
        );
        println!(
            "deploy-host: dry-run: smoke checks generation + systemctl is-active sshd stalwart-mail surmount-management-ui (start UI if inactive) + loopback /health"
        );
    } else {
        rebuild_rc = ssh_run_rc(&opts.target, &rebuild_remote_cmd);
        eprintln!("deploy-host: nixos-rebuild exit={rebuild_rc}");
        eprintln!(
            "deploy-host: running post-switch smoke on {} (always after rebuild)",
            opts.target
        );
        smoke_rc = ssh_run_rc(&opts.target, &smoke_remote_cmd);
        eprintln!("deploy-host: post-switch smoke exit={smoke_rc}");
    }

    print!("{POST_SMOKE_TAIL}");
    let _ = io::stdout().flush();

    if opts.dry_run {
        return Ok(());
    }
    if rebuild_rc != 0 && smoke_rc != 0 {
        return Err(ToolError::fail(format!(
            "nixos-rebuild failed (exit {rebuild_rc}) and post-switch smoke failed (exit {smoke_rc})"
        )));
    }
    if rebuild_rc != 0 {
        return Err(ToolError::fail(format!(
            "nixos-rebuild failed (exit {rebuild_rc}); post-switch smoke exit={smoke_rc} (activation may still have applied; inspect host)"
        )));
    }
    if smoke_rc != 0 {
        return Err(ToolError::fail(format!(
            "post-switch smoke failed (exit {smoke_rc}); units or /health not green"
        )));
    }
    Ok(())
}

fn smoke_remote_name() -> String {
    env::var("SURMOUNT_DEPLOY_SMOKE_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "surmount-deploy-host-post-switch-smoke".to_string())
}

fn resolve_secrets_install() -> Result<PathBuf, ToolError> {
    if let Ok(p) = env::var("SURMOUNT_DEPLOY_SECRETS_INSTALL") {
        if !p.is_empty() {
            let pb = PathBuf::from(&p);
            if pb.is_file() {
                return Ok(pb);
            }
            return Err(ToolError::fail(format!(
                "secrets-install bridge missing: {p}"
            )));
        }
    }
    if let Ok(p) = which("secrets-install-host") {
        return Ok(p);
    }
    if let Ok(p) = which("surmount-secrets-install-host") {
        return Ok(p);
    }
    Err(ToolError::fail(
        "secrets-install-host not on PATH. Install the crate (nix run .#secrets-install-host) or set SURMOUNT_DEPLOY_SECRETS_INSTALL. Default deploy does not auto-install secrets.",
    ))
}

fn which(name: &str) -> Result<PathBuf, ()> {
    if let Ok(paths) = env::var("PATH") {
        for dir in paths.split(':') {
            let p = Path::new(dir).join(name);
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    Err(())
}

fn ssh_bin() -> String {
    env::var("SURMOUNT_DEPLOY_SSH")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "ssh".to_string())
}

/// True when the operator asked for a root login (rebuild needs root, not nixbuilder).
fn target_user_is_root(target: &str) -> bool {
    let t = target.trim();
    t == "root" || t.starts_with("root@")
}

fn operator_ssh_identity(target: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var("SURMOUNT_DEPLOY_SSH_IDENTITY") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if !target_user_is_root(target) {
        return None;
    }
    let home = env::var("HOME").ok()?;
    let id = PathBuf::from(home).join(".ssh/id_ed25519");
    if id.is_file() { Some(id) } else { None }
}

/// Keepalives for flaky laptop NAT, plus laptop key when target is root@...
/// Host `surmount-1` in ssh_config is IdentitiesOnly + nixbuilder; that key is
/// not root. Passing -i id_ed25519 lets ssh try the operator key without a
/// second Host alias.
fn ssh_extra_args(target: &str) -> Vec<String> {
    let mut v = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=10".to_string(),
        "-o".to_string(),
        "TCPKeepAlive=yes".to_string(),
    ];
    if let Some(id) = operator_ssh_identity(target) {
        v.push("-i".to_string());
        v.push(id.to_string_lossy().into_owned());
    }
    v
}

fn ssh_rsh(target: &str) -> String {
    let mut parts = vec![ssh_bin()];
    parts.extend(ssh_extra_args(target));
    parts
        .into_iter()
        .map(|p| shell_quote(&p))
        .collect::<Vec<_>>()
        .join(" ")
}

fn ssh_run(host: &str, remote_cmd: &str) -> Result<(), ToolError> {
    let extra = ssh_extra_args(host);
    let st = Command::new(ssh_bin())
        .args(&extra)
        .arg("--")
        .arg(host)
        .arg(remote_cmd)
        .status()
        .map_err(|e| ToolError::fail(format!("ssh failed: {e}")))?;
    if st.success() {
        Ok(())
    } else {
        Err(ToolError::fail(format!(
            "ssh -- {host} exited {}",
            st.code().unwrap_or(1)
        )))
    }
}

fn ssh_run_rc(host: &str, remote_cmd: &str) -> i32 {
    let extra = ssh_extra_args(host);
    match Command::new(ssh_bin())
        .args(&extra)
        .arg("--")
        .arg(host)
        .arg(remote_cmd)
        .status()
    {
        Ok(s) => s.code().unwrap_or(1),
        Err(_) => 1,
    }
}

fn rsync_local(flags: &[&str], src: &Path, dest: &Path) -> Result<(), ToolError> {
    let src_s = format!("{}/", src.display());
    let dest_s = format!("{}/", dest.display());
    let mut cmd = Command::new("rsync");
    cmd.args(flags).arg(&src_s).arg(&dest_s);
    let st = cmd
        .status()
        .map_err(|e| ToolError::fail(format!("rsync failed: {e}")))?;
    if st.success() {
        Ok(())
    } else {
        Err(ToolError::fail("rsync exited non-zero"))
    }
}

fn rsync_remote(
    flags: &[&str],
    extra: &[&str],
    src: &Path,
    target: &str,
    remote_dir: &str,
) -> Result<(), ToolError> {
    let src_s = format!("{}/", src.display());
    let dest_s = format!("{target}:{}/", remote_dir.trim_end_matches('/'));
    let mut cmd = Command::new("rsync");
    cmd.arg("-e").arg(ssh_rsh(target));
    cmd.args(flags).args(extra).arg(&src_s).arg(&dest_s);
    let st = cmd
        .status()
        .map_err(|e| ToolError::fail(format!("rsync failed: {e}")))?;
    if st.success() {
        Ok(())
    } else {
        Err(ToolError::fail("rsync exited non-zero"))
    }
}

fn run_cmd(argv: &[String]) -> Result<(), ToolError> {
    if argv.is_empty() {
        return Err(ToolError::fail("empty command"));
    }
    let mut c = Command::new(&argv[0]);
    if argv.len() > 1 {
        c.args(&argv[1..]);
    }
    c.stdin(Stdio::inherit());
    let st = c
        .status()
        .map_err(|e| ToolError::fail(format!("failed to start {}: {e}", argv[0])))?;
    if st.success() {
        Ok(())
    } else {
        Err(ToolError::fail(format!(
            "{} exited {}",
            argv[0],
            st.code().unwrap_or(1)
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_at_host_is_root_user() {
        assert!(target_user_is_root("root@surmount-1"));
        assert!(target_user_is_root("root"));
        assert!(!target_user_is_root("surmount-1"));
        assert!(!target_user_is_root("nixbuilder@surmount-1"));
    }

    #[test]
    fn extra_args_include_keepalives() {
        let a = ssh_extra_args("example.test");
        let joined = a.join(" ");
        assert!(joined.contains("ServerAliveInterval=30"), "{joined}");
        assert!(joined.contains("BatchMode=yes"), "{joined}");
    }
}
