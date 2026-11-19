//! Just recipe bodies, fifth ten names.
//! Apps whose flake name is not the recipe name are `nix run`.
//! Apps that share the recipe name run the package binary, so the
//! recipe app does not call itself.

use std::process::{Command, Stdio};

use crate::error::ToolError;
use crate::paths::discover_repo_root;
use crate::recipe_batch_01::{self, flake_system, run_in};

const BATCH: &[&str] = &[
    "diskstation-afp-mount",
    "diskstation-discover",
    "sync-static-sites-from-ds3018xs",
    "mailplus-copy-uid",
    "add-stalwart-token",
    "bootstrap-stalwart-token",
    "stalwart-recovery-unlock",
    "fix-public-dashboard",
    "secrets-install-host",
    "secrets-export-bw-to-staging",
];

pub fn dispatch(args: &[String]) -> Option<Result<(), ToolError>> {
    let (name, rest) = recipe_batch_01::split_invocation(args)?;
    if !BATCH.contains(&name.as_str()) {
        return None;
    }
    Some(run(&name, &rest))
}

fn run(name: &str, args: &[String]) -> Result<(), ToolError> {
    match name {
        "diskstation-afp-mount" => nix_run_app("surmount-diskstation-afp-mount", args),
        "diskstation-discover" => nix_run_app("surmount-diskstation-discover", args),
        "sync-static-sites-from-ds3018xs" => nix_run_app("surmount-sync-static-sites", args),
        "mailplus-copy-uid" => nix_run_app("surmount-copy-mailplus-uid", args),
        "add-stalwart-token" => package_bin("surmount-stalwart-ops", "add-stalwart-token", args),
        "bootstrap-stalwart-token" => nix_run_app("bootstrap-stalwart-api-token", args),
        "stalwart-recovery-unlock" => {
            package_bin("surmount-stalwart-ops", "stalwart-recovery-unlock", args)
        }
        "fix-public-dashboard" => nix_run_app("surmount-fix-public-dashboard", args),
        "secrets-install-host" => {
            package_bin("surmount-secrets-install", "secrets-install-host", args)
        }
        "secrets-export-bw-to-staging" => package_bin(
            "surmount-secrets-install",
            "secrets-export-bw-to-staging",
            args,
        ),
        other => Err(ToolError::fail(format!(
            "surmount-deploy-host: batch 5 has no recipe {other}"
        ))),
    }
}

fn nix_run_app(app: &str, args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = vec!["run".to_string(), format!(".#{app}"), "--".to_string()];
    cmd.extend(args.iter().cloned());
    run_in(&root, "nix", &cmd)
}

fn package_bin(pkg: &str, bin: &str, args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let system = flake_system()?;
    let installable = format!(".#packages.{system}.{pkg}");
    let out = Command::new("nix")
        .args(["build", "--no-link", "--print-out-paths", &installable])
        .current_dir(&root)
        .stderr(Stdio::inherit())
        .output()?;
    if !out.status.success() {
        let code = out.status.code().unwrap_or(1);
        return Err(ToolError {
            message: format!("command exited {code}"),
            exit_code: code,
        });
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let store = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .ok_or_else(|| ToolError::fail(format!("nix build {installable} printed no store path")))?;
    let exe = format!("{store}/bin/{bin}");
    let status = Command::new(exe).args(args).current_dir(&root).status()?;
    if status.success() {
        return Ok(());
    }
    let code = status.code().unwrap_or(1);
    Err(ToolError {
        message: format!("command exited {code}"),
        exit_code: code,
    })
}
