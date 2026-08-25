use std::process::{Command, Output, Stdio};

use crate::error::{Result, ToolError};
use crate::token::Token;

pub fn run_cli(cli: &str, url: &str, token: &Token, args: &[&str], prefix: &str) -> Result<Output> {
    eprintln!(
        "{prefix}: running: {cli} --url {url} --api-key <redacted> {}",
        args.join(" ")
    );
    let out = Command::new(cli)
        .arg("--url")
        .arg(url)
        .arg("--api-key")
        .arg(&token.value)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            ToolError::fail(format!(
                "stalwart-cli not found ({cli}): {e}. Set STALWART_CLI or install stalwart-cli on PATH."
            ))
        })?;
    if !out.stderr.is_empty() {
        let _ = std::io::Write::write_all(&mut std::io::stderr(), &out.stderr);
    }
    Ok(out)
}

pub fn run_cli_ok(cli: &str, url: &str, token: &Token, args: &[&str], prefix: &str) -> Result<String> {
    let out = run_cli(cli, url, token, args, prefix)?;
    if !out.status.success() {
        return Err(ToolError::fail(format!(
            "stalwart-cli {} failed (auth, URL, or engine). Secret values not logged.",
            args.first().unwrap_or(&"")
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
