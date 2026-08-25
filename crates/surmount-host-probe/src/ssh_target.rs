//! Deploy-host SSH target resolution (same as deploy-host / btop / inxi).

use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use crate::ProbeError;

pub fn default_agent_target_env() -> PathBuf {
    let data = env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/.local/share")
    });
    PathBuf::from(data).join("surmount/agent-target.env")
}

/// Parse SURMOUNT_DEPLOY_TARGET from an env file. Does not source the file.
pub fn load_target_from_env_file(path: &std::path::Path) -> Option<String> {
    let file = File::open(path).ok()?;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let raw = if let Some(r) = line.strip_prefix("export SURMOUNT_DEPLOY_TARGET=") {
            r
        } else if let Some(r) = line.strip_prefix("SURMOUNT_DEPLOY_TARGET=") {
            r
        } else {
            continue;
        };
        let raw = unquote(raw);
        if !raw.is_empty() {
            return Some(raw);
        }
    }
    None
}

fn unquote(raw: &str) -> String {
    let b = raw.as_bytes();
    if b.len() >= 2
        && ((b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\''))
    {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

pub fn resolve_target(
    cli_target: Option<&str>,
    env_file_override: Option<&str>,
) -> Result<String, ProbeError> {
    if let Some(t) = cli_target.filter(|s| !s.is_empty()) {
        return reject_option_shaped(t);
    }
    if let Ok(t) = env::var("SURMOUNT_DEPLOY_TARGET") {
        if !t.is_empty() {
            return reject_option_shaped(&t);
        }
    }
    let env_file = env_file_override
        .map(PathBuf::from)
        .or_else(|| env::var("SURMOUNT_AGENT_TARGET_ENV").ok().map(PathBuf::from))
        .unwrap_or_else(default_agent_target_env);
    if !env_file.is_file() {
        return Err(ProbeError::fail(format!(
            "target required: pass --target HOST, set SURMOUNT_DEPLOY_TARGET, or create agent-target.env at {} (never a committed real box default)",
            env_file.display()
        )));
    }
    load_target_from_env_file(&env_file)
        .ok_or_else(|| {
            ProbeError::fail(format!(
                "agent-target.env has no SURMOUNT_DEPLOY_TARGET: {}",
                env_file.display()
            ))
        })
        .and_then(|t| reject_option_shaped(&t))
}

pub fn reject_option_shaped(target: &str) -> Result<String, ProbeError> {
    if target.starts_with('-') {
        return Err(ProbeError::fail(format!(
            "target must not start with '-': option-shaped values are rejected (got {target})"
        )));
    }
    Ok(target.to_string())
}

pub fn target_label(target: &str) -> String {
    if let Some((user, _)) = target.split_once('@') {
        format!("{user}@<mail-host>")
    } else {
        "<mail-host>".into()
    }
}

pub fn redact_ipv4(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if let Some((end, _)) = match_ipv4(&s[i..]) {
            out.push_str("<redacted-ipv4>");
            i += end;
        } else {
            out.push(s[i..].chars().next().unwrap());
            i += s[i..].chars().next().unwrap().len_utf8();
        }
    }
    out
}

fn match_ipv4(s: &str) -> Option<(usize, ())> {
    let mut octets = 0;
    let mut i = 0;
    let bytes = s.as_bytes();
    while octets < 4 {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start || i - start > 3 {
            return None;
        }
        octets += 1;
        if octets == 4 {
            return Some((i, ()));
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
        } else {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn env_file_export_line() {
        let dir = std::env::temp_dir().join(format!("ssh-target-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("agent-target.env");
        let mut f = std::fs::File::create(&p).unwrap();
        writeln!(f, "export SURMOUNT_DEPLOY_TARGET=root@example.test").unwrap();
        assert_eq!(
            load_target_from_env_file(&p).as_deref(),
            Some("root@example.test")
        );
    }

    #[test]
    fn option_shaped_rejected() {
        assert!(reject_option_shaped("-oProxyCommand=true").is_err());
    }

    #[test]
    fn redact_ipv4_hides_dotted_quad() {
        assert_eq!(redact_ipv4("see 192.0.2.10 now"), "see <redacted-ipv4> now");
    }
}
