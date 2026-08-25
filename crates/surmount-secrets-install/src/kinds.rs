use crate::error::{Result, ToolError};
use crate::paths::assert_safe_token;

pub fn validate_kind(kind: &str) -> Result<()> {
    assert_safe_token("surmount.kind", kind)?;
    match kind {
        "tls-cert"
        | "tls-key"
        | "session-secret"
        | "stalwart-token"
        | "restic-password"
        | "age-admin"
        | "age-host"
        | "arti-hs-bundle"
        | "nostr-allowlist"
        | "vaultwarden-admin"
        | "namecheap-api"
        | "shc-api"
        | "stalwart-recovery-admin"
        | "other" => Ok(()),
        "synology-afp" => Err(ToolError::fail(
            "refuse: kind=synology-afp is laptop-only (never install DiskStation LAN password to the mail host). See docs/SECRETS.md",
        )),
        _ => Err(ToolError::fail(format!(
            "unknown or unsupported surmount.kind={kind}"
        ))),
    }
}

pub fn kind_needs_ui_owner(kind: &str) -> bool {
    matches!(
        kind,
        "session-secret"
            | "stalwart-token"
            | "namecheap-api"
            | "vaultwarden-admin"
            | "tls-cert"
            | "tls-key"
            | "nostr-allowlist"
    )
}

pub fn kind_tls_shared_leaf(kind: &str) -> bool {
    matches!(kind, "tls-cert" | "tls-key")
}

pub fn leaf_install_mode(kind: &str) -> u32 {
    if kind_tls_shared_leaf(kind) {
        0o640
    } else {
        0o600
    }
}

pub fn is_known_export_kind(kind: &str) -> bool {
    matches!(
        kind,
        "tls-cert"
            | "tls-key"
            | "session-secret"
            | "stalwart-token"
            | "restic-password"
            | "age-admin"
            | "age-host"
            | "arti-hs-bundle"
            | "nostr-allowlist"
            | "vaultwarden-admin"
            | "namecheap-api"
            | "shc-api"
            | "other"
    )
}

pub fn safe_stage_id(raw: &str) -> String {
    if !raw.is_empty()
        && raw
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && raw
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
    {
        return raw.to_string();
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in raw.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("k-{h:016x}")
}

pub fn read_attr(body: &str, key: &str) -> Option<String> {
    for line in body.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && k == key
        {
            return Some(v.to_string());
        }
    }
    None
}

/// Rewrite only ClientIp= (and aliases) in a KEY=value body. Never invent IP.
pub fn rewrite_namecheap_client_ip(src: &str, client_ip: &str) -> String {
    let mut found = false;
    let mut out = String::new();
    let ends_nl = src.ends_with('\n');
    for line in src.split_inclusive('\n') {
        let raw = line.trim_end_matches(['\n', '\r']);
        if raw.is_empty() || raw.starts_with('#') {
            out.push_str(raw);
            if line.ends_with('\n') {
                out.push('\n');
            }
            continue;
        }
        let key = raw.split('=').next().unwrap_or("");
        match key {
            "ClientIp" | "CLIENT_IP" | "SourceIp" | "SOURCE_IP" => {
                out.push_str("ClientIp=");
                out.push_str(client_ip);
                out.push('\n');
                found = true;
            }
            _ => {
                out.push_str(raw);
                if line.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    if !found {
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
        out.push_str("ClientIp=");
        out.push_str(client_ip);
        out.push('\n');
    } else if ends_nl && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
