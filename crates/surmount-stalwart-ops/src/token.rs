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

/// Write `token` plus a newline at `path` with mode 0600.
///
/// When `owner` is set, the parent directory is mode 0750 and both the parent
/// and the file are given to that user (the lab VM uses `surmount-ui`).
/// Success does not print the token.
pub fn write_api_token_file(path: &Path, token: &str, owner: Option<&str>) -> Result<()> {
    assert_safe("api token", token)?;
    if token.len() > 4096 || token.contains('\n') || token.contains('\r') {
        return Err(ToolError::fail(
            "api token must be a single reasonable line".to_string(),
        ));
    }
    absolute_no_dotdot(path)?;
    refuse_symlink(path, "token file must be a regular file (symlink refused)")?;
    let Some(parent) = path.parent() else {
        return Err(ToolError::fail("token path has no parent".to_string()));
    };
    if parent.as_os_str().is_empty() {
        return Err(ToolError::fail("token path has no parent".to_string()));
    }
    fs::create_dir_all(parent)?;
    if let Some(user) = owner {
        chown_user(parent, user)?;
        set_mode(parent, 0o750)?;
    }
    let tmp = parent.join(format!(".stalwart-api-token.{}.tmp", std::process::id()));
    refuse_symlink(
        &tmp,
        "token temp path must be a regular file (symlink refused)",
    )?;
    if fs::symlink_metadata(&tmp).is_ok() {
        fs::remove_file(&tmp)?;
    }
    let write_result = (|| -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        writeln!(file, "{token}")?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    if let Err(err) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(err.into());
    }
    set_mode(path, 0o600)?;
    if let Some(user) = owner {
        chown_user(path, user)?;
        set_mode(path, 0o600)?;
    }
    Ok(())
}

fn absolute_no_dotdot(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(ToolError::fail(
            "path must be absolute and must not contain ..".to_string(),
        ));
    }
    Ok(())
}

fn refuse_symlink(path: &Path, message: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(ToolError::fail(message.to_string())),
        Ok(meta) if !meta.is_file() && meta.file_type().is_dir() => {
            Err(ToolError::fail(format!("{message} (path is a directory)")))
        }
        Ok(_) | Err(_) => Ok(()),
    }
}

fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)?.permissions();
    perm.set_mode(mode);
    fs::set_permissions(path, perm)?;
    Ok(())
}

fn chown_user(path: &Path, user: &str) -> Result<()> {
    if !owner_name_ok(user) {
        return Err(ToolError::fail("token owner name refused".to_string()));
    }
    let status = std::process::Command::new("chown")
        .arg("--")
        .arg(format!("{user}:{user}"))
        .arg(path)
        .status()?;
    if !status.success() {
        return Err(ToolError::fail(
            "chown of the token path failed (secret values not logged)".to_string(),
        ));
    }
    Ok(())
}

fn owner_name_ok(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 32
        && !user.starts_with('-')
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
