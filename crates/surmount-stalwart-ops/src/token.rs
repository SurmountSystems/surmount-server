use std::fs;
use std::path::Path;

use crate::error::{Result, ToolError};

pub const DEFAULT_TOKEN_FILE: &str = "/var/lib/surmount/secrets/ui/stalwart-api-token";

#[derive(Debug, Clone)]
pub struct Token {
    pub value: String,
    pub source: &'static str,
    pub file: String,
}

pub fn assert_safe(label: &str, val: &str) -> Result<()> {
    if val.is_empty() {
        return Err(ToolError::fail(format!("empty {label}")));
    }
    if val.chars().any(|c| c.is_control()) {
        return Err(ToolError::fail(format!(
            "{label} must not contain control characters"
        )));
    }
    Ok(())
}

/// Load token from file (first non-empty non-# line) else STALWART_TOKEN.
/// Never returns the value in error messages.
pub fn load_token(token_file: &str) -> Result<Token> {
    assert_safe("token-file path", token_file)?;
    if token_file.starts_with('-') {
        return Err(ToolError::fail(format!(
            "token-file must not start with '-' (got {token_file})"
        )));
    }
    let path = Path::new(token_file);
    let mut value = String::new();
    let mut source = "";
    if path.exists() || path.symlink_metadata().is_ok() {
        if path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ToolError::fail(format!(
                "token file must be a regular file (symlink refused): {token_file}"
            )));
        }
        if path.is_file() {
            let body = fs::read_to_string(path).unwrap_or_default();
            for line in body.lines() {
                let line = line.trim_end_matches('\r');
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                value = line.to_string();
                source = "file";
                break;
            }
            if value.is_empty()
                && std::env::var("STALWART_TOKEN")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .is_none()
            {
                return Err(ToolError::blocked(format!(
                    "token file present but empty/comments-only ({token_file}). Populate kind stalwart-token Domain B material, or set STALWART_TOKEN. Secret values not logged."
                )));
            }
        }
    }
    if value.is_empty() {
        if let Ok(envv) = std::env::var("STALWART_TOKEN")
            && !envv.is_empty()
        {
            value = envv;
            source = "env";
        } else {
            return Err(ToolError::blocked(format!(
                "no token material (missing {token_file} and STALWART_TOKEN unset). Install kind stalwart-token via secrets-install-host, or set STALWART_TOKEN_FILE / STALWART_TOKEN. Secret values not logged."
            )));
        }
    }
    if value.is_empty() {
        return Err(ToolError::blocked(
            "token resolved empty (refuse). Secret values not logged.".to_string(),
        ));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(ToolError::fail(format!(
            "token must not contain control characters (source={source})"
        )));
    }
    Ok(Token {
        value,
        source,
        file: token_file.to_string(),
    })
}

pub fn assert_loopback_8080(url: &str) -> Result<()> {
    match url {
        "http://127.0.0.1:8080"
        | "http://127.0.0.1:8080/"
        | "http://[::1]:8080"
        | "http://[::1]:8080/" => Ok(()),
        _ => Err(ToolError::fail(format!(
            "url must be loopback :8080 only (got {url}). Refusing non-loopback or other ports."
        ))),
    }
}

pub fn which_or(spec: &str) -> Result<String> {
    let p = Path::new(spec);
    if p.is_file() {
        return Ok(spec.to_string());
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let c = Path::new(dir).join(spec);
            if c.is_file() {
                return Ok(c.to_string_lossy().into_owned());
            }
        }
    }
    if spec.contains('/') {
        return Err(ToolError::fail(format!(
            "stalwart-cli not found ({spec}). Set STALWART_CLI or install stalwart-cli on PATH."
        )));
    }
    Ok(spec.to_string())
}
