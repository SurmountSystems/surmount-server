//! Namecheap KEY=value credentials. Never log ApiKey.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

pub const DEFAULT_CRED_PATH: &str = "/var/lib/surmount/secrets/acme/namecheap.env";

#[derive(Clone)]
pub struct Credentials {
    pub api_user: String,
    pub api_key: String,
    pub user_name: String,
    pub client_ip: String,
    pub sld: String,
    pub tld: String,
    pub settle_secs: u64,
    pub txt_ttl: u32,
    pub path: PathBuf,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("api_user", &self.api_user)
            .field("api_key", &"<redacted>")
            .field("sld", &self.sld)
            .field("tld", &self.tld)
            .finish_non_exhaustive()
    }
}

pub fn die(prefix: &str, msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("{prefix}: {msg}")
}

pub fn require_owner_only(path: &Path, prefix: &str) -> Result<fs::Metadata> {
    if path.as_os_str().is_empty() {
        return Err(die(prefix, "credentials path empty"));
    }
    let meta = fs::symlink_metadata(path).map_err(|_| {
        die(
            prefix,
            format!("credentials file missing: {}", path.display()),
        )
    })?;
    if meta.file_type().is_symlink() {
        return Err(die(
            prefix,
            format!(
                "credentials path must be a regular file (symlink refused): {}",
                path.display()
            ),
        ));
    }
    if !meta.is_file() {
        return Err(die(
            prefix,
            format!("credentials file missing: {}", path.display()),
        ));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(die(
            prefix,
            format!(
                "credentials file must be owner-only (mode 0600; refuse group/world bits): {} (mode {:o})",
                path.display(),
                mode
            ),
        ));
    }
    Ok(meta)
}

pub fn load_credentials(path: &Path, prefix: &str) -> Result<Credentials> {
    require_owner_only(path, prefix)?;
    let file = File::open(path).map_err(|_| {
        die(
            prefix,
            format!("credentials file unreadable: {}", path.display()),
        )
    })?;
    let mut api_user = String::new();
    let mut api_key = String::new();
    let mut user_name = String::new();
    let mut client_ip = String::new();
    let mut sld = String::new();
    let mut tld = String::new();
    let mut settle_secs = 0u64;
    let mut txt_ttl = 60u32;
    for line in BufReader::new(file).lines() {
        let mut line = line.map_err(|_| {
            die(
                prefix,
                format!("credentials unreadable: {}", path.display()),
            )
        })?;
        if let Some(s) = line.strip_suffix('\r') {
            line = s.to_string();
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            return Err(die(
                prefix,
                format!(
                    "credentials line must be KEY=value (file={})",
                    path.display()
                ),
            ));
        };
        let key = key.trim();
        let mut val = val.to_string();
        if val.len() >= 2
            && ((val.starts_with('"') && val.ends_with('"'))
                || (val.starts_with('\'') && val.ends_with('\'')))
        {
            val = val[1..val.len() - 1].to_string();
        }
        match key {
            "ApiUser" | "API_USER" => api_user = val,
            "ApiKey" | "API_KEY" => api_key = val,
            "UserName" | "Username" | "USER_NAME" => user_name = val,
            "ClientIp" | "CLIENT_IP" | "SourceIp" | "SOURCE_IP" => client_ip = val,
            "SLD" | "sld" => sld = val,
            "TLD" | "tld" => tld = val,
            "SettleSeconds" | "SETTLE_SECONDS" => {
                settle_secs = val
                    .parse()
                    .map_err(|_| die(prefix, "SettleSeconds must be a non-negative integer"))?;
            }
            "TxtTtl" | "TXT_TTL" | "TTL" => {
                txt_ttl = val
                    .parse()
                    .map_err(|_| die(prefix, "TxtTtl must be an integer >= 60"))?;
            }
            _ => {}
        }
    }
    if api_user.is_empty() {
        return Err(die(
            prefix,
            format!("credentials missing ApiUser (file={})", path.display()),
        ));
    }
    if api_key.is_empty() {
        return Err(die(
            prefix,
            format!("credentials missing ApiKey (file={})", path.display()),
        ));
    }
    if sld.is_empty() {
        return Err(die(
            prefix,
            format!("credentials missing SLD (file={})", path.display()),
        ));
    }
    if tld.is_empty() {
        return Err(die(
            prefix,
            format!("credentials missing TLD (file={})", path.display()),
        ));
    }
    if txt_ttl < 60 {
        return Err(die(prefix, "TxtTtl must be an integer >= 60"));
    }
    if user_name.is_empty() {
        user_name = api_user.clone();
    }
    Ok(Credentials {
        api_user,
        api_key,
        user_name,
        client_ip,
        sld,
        tld,
        settle_secs,
        txt_ttl,
        path: path.to_path_buf(),
    })
}

pub fn fqdn_to_hostname(fqdn: &str, sld: &str, tld: &str) -> Result<String> {
    let mut fqdn = fqdn.to_string();
    while fqdn.ends_with('.') {
        fqdn.pop();
    }
    if fqdn.is_empty() {
        return Err(die("acme-dns-hook-namecheap", "empty fqdn"));
    }
    let suffix = format!("{sld}.{tld}");
    if fqdn == suffix {
        return Ok("@".into());
    }
    if let Some(host) = fqdn.strip_suffix(&format!(".{suffix}")) {
        if host.is_empty() {
            return Ok("@".into());
        }
        return Ok(host.to_string());
    }
    Err(die(
        "acme-dns-hook-namecheap",
        format!("fqdn {fqdn} is not under {suffix} (check SLD/TLD in credentials)"),
    ))
}
