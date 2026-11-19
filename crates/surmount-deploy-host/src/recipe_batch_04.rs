//! Just recipe bodies, fourth ten names.
//! Five nix eval contracts, then nix run of an existing app.

use crate::error::ToolError;
use crate::paths::discover_repo_root;
use crate::recipe_batch_01::{self, run_in};

const BATCH: &[&str] = &[
    "test-eternal-terminal-eval",
    "test-inxi-eval",
    "test-btop-eval",
    "test-perf-tools-eval",
    "test-remote-builder-eval",
    "check-private-data",
    "check-leftover-agent-homes",
    "check-tls-hybrid",
    "secrets-prompt",
    "rekey",
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
        "test-eternal-terminal-eval" => nix_eval("tests/eternal-terminal-contract.nix"),
        "test-inxi-eval" => nix_eval("tests/inxi-contract.nix"),
        "test-btop-eval" => nix_eval("tests/btop-contract.nix"),
        "test-perf-tools-eval" => nix_eval("tests/perf-tools-contract.nix"),
        "test-remote-builder-eval" => nix_eval("tests/remote-builder-contract.nix"),
        "check-private-data" => nix_run_app("surmount-private-data", args),
        "check-leftover-agent-homes" => nix_run_app("surmount-leftover-homes", args),
        "check-tls-hybrid" => nix_run_app("surmount-tls-hybrid", args),
        "secrets-prompt" => nix_run_app("surmount-secrets-prompt", args),
        "rekey" => nix_run_app("surmount-rekey", args),
        other => Err(ToolError::fail(format!(
            "surmount-deploy-host: batch 4 has no recipe {other}"
        ))),
    }
}

fn nix_eval(file: &str) -> Result<(), ToolError> {
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

fn nix_run_app(app: &str, args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = vec!["run".to_string(), format!(".#{app}"), "--".to_string()];
    cmd.extend(args.iter().cloned());
    run_in(&root, "nix", &cmd)
}
