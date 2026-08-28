use std::path::Path;
use std::process::Command;

use crate::cli::{run_cli, run_cli_ok};
use crate::error::{Result, ToolError};
use crate::token::{DEFAULT_TOKEN_FILE, assert_safe, load_token, which_or};

const USAGE: &str = "\
Usage:
  free-stalwart-public-443 --help
  free-stalwart-public-443 [--dry-run] [options]
  free-stalwart-public-443 --live [options]
  free-stalwart-public-443 --query-only [options]

Free Stalwart public HTTPS on :443 for Axum product edge. Default is dry-run
(no datastore mutation, no restart). Never logs secret values.

Modes:
  (default) / --dry-run  Query listeners; apply plan with --dry-run only
  --live                 Apply plan for real; optional restart; prove with ss
  --query-only           Query NetworkListener only (still needs token)

Token:
  --token-file PATH
";

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut dry_run = true;
    let mut live = false;
    let mut query_only = false;
    let mut restart = false;
    let mut skip_ss = false;
    let mut token_file = std::env::var("STALWART_TOKEN_FILE").unwrap_or_default();
    let mut plan = std::env::var("SURMOUNT_FREE_443_PLAN").unwrap_or_default();
    let mut url = std::env::var("STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut cli = std::env::var("STALWART_CLI").unwrap_or_default();
    let mut ss_cmd = std::env::var("SURMOUNT_FREE_443_SS").unwrap_or_else(|_| "ss".into());
    let mut systemctl =
        std::env::var("SURMOUNT_FREE_443_SYSTEMCTL").unwrap_or_else(|_| "systemctl".into());
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                println!("  --dry-run  --live  --query-only  --token-file  --plan  --url  --cli");
                println!("  --restart  --skip-ss-check  --ss-cmd  --systemctl-cmd");
                return Ok(());
            }
            "--dry-run" => {
                dry_run = true;
                live = false;
                query_only = false;
            }
            "--live" => {
                live = true;
                dry_run = false;
                query_only = false;
            }
            "--query-only" => {
                query_only = true;
                dry_run = false;
                live = false;
            }
            "--token-file" => {
                i += 1;
                token_file = need(&argv, i)?;
            }
            "--plan" => {
                i += 1;
                plan = need(&argv, i)?;
            }
            "--url" => {
                i += 1;
                url = need(&argv, i)?;
            }
            "--cli" => {
                i += 1;
                cli = need(&argv, i)?;
            }
            "--restart" => restart = true,
            "--skip-ss-check" => skip_ss = true,
            "--ss-cmd" => {
                i += 1;
                ss_cmd = need(&argv, i)?;
            }
            "--systemctl-cmd" => {
                i += 1;
                systemctl = need(&argv, i)?;
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
        i += 1;
    }
    if token_file.is_empty() {
        token_file = DEFAULT_TOKEN_FILE.into();
    }
    if plan.is_empty() {
        let host = "/etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson";
        let tree = repo_plan();
        if Path::new(host).is_file() {
            plan = host.into();
        } else if tree.is_file() {
            plan = tree.to_string_lossy().into_owned();
        } else {
            plan = host.into();
        }
    }
    assert_safe("plan path", &plan)?;
    assert_safe("url", &url)?;
    if url.starts_with('-') {
        return Err(ToolError::fail("url must not start with '-'".to_string()));
    }
    if cli.is_empty() {
        cli = "stalwart-cli".into();
    }
    let cli = which_or(&cli)?;
    let token = load_token(&token_file)?;

    if !query_only {
        let p = Path::new(&plan);
        if p.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
            && !p.exists()
        {
            return Err(ToolError::fail(format!(
                "plan path is a dangling symlink: {plan}"
            )));
        }
        if !p.is_file() && !p.symlink_metadata().is_ok() {
            return Err(ToolError::fail(format!("plan file missing: {plan}")));
        }
        if !p.exists() {
            return Err(ToolError::fail(format!("plan file missing: {plan}")));
        }
    }

    let mode = if live {
        "live"
    } else if query_only {
        "query-only"
    } else {
        "dry-run"
    };
    eprintln!(
        "free-stalwart-public-443: mode={mode} url={url} plan={plan} token_source={} token_file={}",
        token.source, token.file
    );
    eprintln!("free-stalwart-public-443: query NetworkListener (fields id,name,protocol,bind)");
    run_cli_ok(
        &cli,
        &url,
        &token,
        &[
            "query",
            "NetworkListener",
            "--fields",
            "id,name,protocol,bind",
            "--json",
        ],
        "free-stalwart-public-443",
    )
    .map_err(|_| {
        ToolError::fail(
            "stalwart-cli query failed (auth, URL, or engine). Installing Domain B stalwart-token is not the same as engine registration; first-boot admin may still be required. Secret values not logged.".to_string(),
        )
    })?;
    if query_only {
        eprintln!("free-stalwart-public-443: query-only complete");
        return Ok(());
    }
    if dry_run {
        eprintln!(
            "free-stalwart-public-443: dry-run: apply plan with --dry-run (no datastore mutation claimed)"
        );
        let out = run_cli(
            &cli,
            &url,
            &token,
            &["apply", "--file", &plan, "--dry-run"],
            "free-stalwart-public-443",
        )?;
        if !out.status.success() {
            return Err(ToolError::fail(
                "stalwart-cli apply --dry-run failed (plan or auth). Secret values not logged."
                    .to_string(),
            ));
        }
        if !out.stdout.is_empty() {
            let _ = std::io::Write::write_all(&mut std::io::stdout(), &out.stdout);
        }
        eprintln!(
            "free-stalwart-public-443: dry-run complete: no live free of :443 claimed. Re-run with --live on the operator host when ready."
        );
        return Ok(());
    }

    eprintln!("free-stalwart-public-443: live: apply plan (datastore mutation)");
    let out = run_cli(
        &cli,
        &url,
        &token,
        &["apply", "--file", &plan],
        "free-stalwart-public-443",
    )?;
    if !out.status.success() {
        return Err(ToolError::fail(
            "stalwart-cli apply failed. Mail left as-is. Secret values not logged.".to_string(),
        ));
    }
    eprintln!("free-stalwart-public-443: live apply finished");
    if restart {
        eprintln!("free-stalwart-public-443: restarting stalwart-mail ({systemctl})");
        let st = Command::new(&systemctl)
            .args(["restart", "stalwart-mail"])
            .status()?;
        if !st.success() {
            return Err(ToolError::fail(
                "systemctl restart stalwart-mail failed after apply".to_string(),
            ));
        }
        eprintln!("free-stalwart-public-443: stalwart-mail restart requested");
    } else {
        eprintln!(
            "free-stalwart-public-443: note: not restarting stalwart-mail (pass --restart if binds persist until process restart)"
        );
    }
    if skip_ss {
        eprintln!("free-stalwart-public-443: skip ss check (--skip-ss-check)");
        eprintln!("free-stalwart-public-443: live complete: apply ok; ss not proven this run");
        return Ok(());
    }
    let ss_out = Command::new(&ss_cmd).arg("-lntp").output().map_err(|_| {
        ToolError::fail(format!(
            "ss not found ({ss_cmd}); pass --skip-ss-check or --ss-cmd for hermetic"
        ))
    })?;
    if !ss_out.status.success() {
        return Err(ToolError::fail(format!(
            "ss failed (rc={}); cannot prove :443 free",
            ss_out.status.code().unwrap_or(1)
        )));
    }
    let text = String::from_utf8_lossy(&ss_out.stdout);
    let has_443_stalwart = text
        .lines()
        .any(|l| l.contains(":443") && l.to_ascii_lowercase().contains("stalwart"));
    if has_443_stalwart {
        return Err(ToolError::fail(
            "prove failed: stalwart still appears on :443 (ss). Restart may be required, or plan name filters may not match first-boot listeners. Query listeners and adjust host-local plan copy.".to_string(),
        ));
    }
    if text.contains(":443") {
        eprintln!(
            "free-stalwart-public-443: note: something still listens on :443 (not matched as stalwart); confirm it is product edge when enabled"
        );
    } else {
        eprintln!(
            "free-stalwart-public-443: no :443 listeners visible in ss (free for Axum product edge)"
        );
    }
    eprintln!("free-stalwart-public-443: live complete: apply ok; stalwart not observed on :443");
    Ok(())
}

fn repo_plan() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(&manifest)
        .join("nix/stalwart/free-public-443-for-axum-edge.ndjson")
}

fn need(argv: &[String], i: usize) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail("flag requires a value".to_string()))
}
