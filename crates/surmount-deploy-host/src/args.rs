use std::env;
use std::path::PathBuf;

use crate::error::ToolError;
use crate::paths::env_flag_true;

pub const USAGE: &str = r#"Usage:
  surmount-deploy-host --help
  surmount-deploy-host --target HOST [options]
  surmount-deploy-host --dry-run --target HOST [options]

Operator-driven deploy for flake attr #mail-vps (alias #surmount-mail).
Never commits host-local material. Never embeds real host IPs or keys.

Required:
  --target HOST          SSH target (user@host or host). Or env SURMOUNT_DEPLOY_TARGET.
                         Never a committed default of a real box.
                         Must not start with '-' (option-shaped values rejected).

Options:
  --dry-run              Print planned commands; do not rsync or rebuild.
  --host-local DIR       Path to host-local overlay (local machine or mount).
                         Or env SURMOUNT_HOST_LOCAL_DIR.
                         Required for lockout / layout checks unless --skip-host-local-check.
  --flake-attr ATTR      Flake host attr (default: mail-vps). Or SURMOUNT_DEPLOY_FLAKE_ATTR.
  --remote-dir PATH      Remote checkout path (default: /root/surmount-server).
  --skip-sync            Do not rsync the public tree (rebuild only).
  --skip-host-local-check
                         Skip host-local presence / authorized_keys checks
                         AND skip remote host-local rsync (not recommended).
  --host-local-delete    When syncing host-local to remote (or --sync-host-local-into),
                         pass rsync --delete. Default is without --delete so incomplete
                         local material does not wipe remote-only files.
  --sync-host-local-into DIR
                         Attempt to copy host-local into DIR. Refused when DIR
                         is under public tracked layout (hosts/, secrets/, or
                         repo root product paths). Use only for private out-of-
                         tree destinations if you really need a local inject.
  --build-host MODE      On-host rebuild (default). Reserved for future modes.
  --install-secrets      Opt-in: run secrets-install-host before rebuild
                         (domain A -> B). Default OFF: deploy never auto-installs
                         secrets. Requires a secrets source + --secrets-host-id.
  --secrets-staging DIR  With --install-secrets: pass --from-staging DIR to the
                         install bridge. Or env SURMOUNT_SECRETS_STAGING.
                         Must be a directory outside the public git work tree
                         (refuse in-tree staging so public rsync cannot copy
                         payload into the remote checkout).
  --secrets-from-secret-service
                         With --install-secrets: pass --from-secret-service.
                         Or env SURMOUNT_SECRETS_FROM_SECRET_SERVICE=1.
                         Exactly one of staging or secret-service when opt-in.
  --secrets-host-id ID   With --install-secrets: logical host id for the bridge
                         (--host-id). Or env SURMOUNT_SECRETS_HOST_ID. Required
                         when --install-secrets is set.
  --secrets-require-kind KIND
                         With --install-secrets: forward --require-kind KIND
                         (repeatable; allowlisted kinds). The install bridge
                         fails if a required kind is missing when install
                         actually runs (not re-checked by deploy-host dry-run).
  -h, --help             Show this help.

Environment:
  SURMOUNT_DEPLOY_SSH          ssh binary (default ssh)
  SURMOUNT_DEPLOY_SSH_IDENTITY extra IdentityFile. For root@ targets, default
                               is $HOME/.ssh/id_ed25519 when that file exists
                               so Host surmount-1 nixbuilder IdentitiesOnly
                               still reaches root.
  SURMOUNT_DEPLOY_TARGET       Same as --target
  SURMOUNT_HOST_LOCAL_DIR      Same as --host-local
  SURMOUNT_DEPLOY_FLAKE_ATTR   Same as --flake-attr (default mail-vps)
  SURMOUNT_DEPLOY_REMOTE_DIR   Same as --remote-dir
  SURMOUNT_DEPLOY_INSTALL_SECRETS=1
                               Same as --install-secrets (opt-in only)
  SURMOUNT_SECRETS_STAGING     Same as --secrets-staging
  SURMOUNT_SECRETS_FROM_SECRET_SERVICE=1
                               Same as --secrets-from-secret-service
  SURMOUNT_SECRETS_HOST_ID     Same as --secrets-host-id

After rebuild (success or non-zero), deploy-host always runs post-switch
smoke on the remote via SSH (surmount-deploy-host-post-switch-smoke):
  generation pointer, systemctl is-active sshd/stalwart-mail/
  surmount-management-ui (starts UI if inactive), loopback /health.
"#;

pub const ALLOWED_SECRETS_KINDS: &[&str] = &[
    "tls-cert",
    "tls-key",
    "session-secret",
    "stalwart-token",
    "restic-password",
    "age-admin",
    "age-host",
    "arti-hs-bundle",
    "nostr-allowlist",
    "vaultwarden-admin",
    "namecheap-api",
    "shc-api",
    "stalwart-recovery-admin",
    "other",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Help,
    Run(DeployOpts),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployOpts {
    pub dry_run: bool,
    pub target: String,
    pub host_local: Option<PathBuf>,
    pub flake_attr: String,
    pub remote_dir: String,
    pub skip_sync: bool,
    pub skip_host_local_check: bool,
    pub host_local_delete: bool,
    pub sync_host_local_into: Option<PathBuf>,
    pub install_secrets: bool,
    pub secrets_staging: Option<PathBuf>,
    pub secrets_from_secret_service: bool,
    pub secrets_host_id: Option<String>,
    pub secrets_require_kinds: Vec<String>,
}

pub fn parse_args<I, S>(args: I) -> Result<Mode, ToolError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let rest: Vec<String> = iter.map(|s| s.as_ref().to_string()).collect();

    let mut dry_run = false;
    let mut target = env::var("SURMOUNT_DEPLOY_TARGET")
        .ok()
        .filter(|s| !s.is_empty());
    let mut host_local = env::var("SURMOUNT_HOST_LOCAL_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let mut flake_attr = env::var("SURMOUNT_DEPLOY_FLAKE_ATTR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "mail-vps".to_string());
    let mut remote_dir = env::var("SURMOUNT_DEPLOY_REMOTE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/root/surmount-server".to_string());
    let mut skip_sync = false;
    let mut skip_hl = false;
    let mut hl_delete = false;
    let mut sync_into: Option<PathBuf> = None;
    let mut install_secrets = env_flag_true("SURMOUNT_DEPLOY_INSTALL_SECRETS");
    let mut secrets_staging = env::var("SURMOUNT_SECRETS_STAGING")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let mut secrets_from_ss = env_flag_true("SURMOUNT_SECRETS_FROM_SECRET_SERVICE");
    let mut secrets_host_id = env::var("SURMOUNT_SECRETS_HOST_ID")
        .ok()
        .filter(|s| !s.is_empty());
    let mut secrets_require_kinds: Vec<String> = Vec::new();

    let mut i = 0usize;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => return Ok(Mode::Help),
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--target" => {
                let v = take_value(&rest, &mut i, "--target")?;
                target = Some(v);
            }
            "--host-local" => {
                let v = take_value(&rest, &mut i, "--host-local")?;
                host_local = Some(PathBuf::from(v));
            }
            "--flake-attr" => {
                flake_attr = take_value(&rest, &mut i, "--flake-attr")?;
            }
            "--remote-dir" => {
                remote_dir = take_value(&rest, &mut i, "--remote-dir")?;
            }
            "--skip-sync" => {
                skip_sync = true;
                i += 1;
            }
            "--skip-host-local-check" => {
                skip_hl = true;
                i += 1;
            }
            "--host-local-delete" => {
                hl_delete = true;
                i += 1;
            }
            "--sync-host-local-into" => {
                let v = take_value(&rest, &mut i, "--sync-host-local-into")?;
                sync_into = Some(PathBuf::from(v));
            }
            "--build-host" => {
                i += 1;
                if i < rest.len() && !rest[i].starts_with("--") {
                    i += 1;
                }
            }
            "--install-secrets" => {
                install_secrets = true;
                i += 1;
            }
            "--secrets-staging" => {
                let v = take_value(&rest, &mut i, "--secrets-staging")?;
                secrets_staging = Some(PathBuf::from(v));
            }
            "--secrets-from-secret-service" => {
                secrets_from_ss = true;
                i += 1;
            }
            "--secrets-host-id" => {
                secrets_host_id = Some(take_value(&rest, &mut i, "--secrets-host-id")?);
            }
            "--secrets-require-kind" => {
                secrets_require_kinds.push(take_value(&rest, &mut i, "--secrets-require-kind")?);
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
    }

    let Some(target) = target else {
        return Err(ToolError::fail(
            "target required: pass --target HOST or set SURMOUNT_DEPLOY_TARGET (never a committed real box default)",
        ));
    };
    if target.starts_with('-') {
        return Err(ToolError::fail(format!(
            "target must not start with '-': option-shaped values are rejected (got {target})"
        )));
    }

    Ok(Mode::Run(DeployOpts {
        dry_run,
        target,
        host_local,
        flake_attr,
        remote_dir,
        skip_sync,
        skip_host_local_check: skip_hl,
        host_local_delete: hl_delete,
        sync_host_local_into: sync_into,
        install_secrets,
        secrets_staging,
        secrets_from_secret_service: secrets_from_ss,
        secrets_host_id,
        secrets_require_kinds,
    }))
}

fn take_value(rest: &[String], i: &mut usize, flag: &str) -> Result<String, ToolError> {
    if *i + 1 >= rest.len() {
        return Err(ToolError::fail(format!("{flag} requires a value")));
    }
    let v = rest[*i + 1].clone();
    *i += 2;
    Ok(v)
}

pub fn validate_secrets_kind(kind: &str) -> Result<(), ToolError> {
    crate::paths::assert_safe_token("secrets-require-kind", kind)?;
    if ALLOWED_SECRETS_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(ToolError::fail(format!(
            "unknown or unsupported secrets-require-kind={kind}"
        )))
    }
}
