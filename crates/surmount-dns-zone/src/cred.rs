//! Namecheap KEY=value credentials. Never log ApiKey.

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

pub const DEFAULT_CRED_PATH: &str = "/var/lib/surmount/secrets/acme/namecheap.env";
pub const DEFAULT_TTL: u32 = 1800;

#[derive(Clone)]
pub struct Credentials {
    pub api_user: String,
    pub api_key: String,
    pub user_name: String,
    pub client_ip: String,
    pub sld: String,
    pub tld: String,
    pub path: PathBuf,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("api_user", &self.api_user)
            .field("api_key", &"<redacted>")
            .field("user_name", &self.user_name)
            .field("client_ip", &self.client_ip)
            .field("sld", &self.sld)
            .field("tld", &self.tld)
            .field("path", &self.path)
            .finish()
    }
}

impl Credentials {
    pub fn zone(&self) -> String {
        format!("{}.{}", self.sld, self.tld)
    }
}

pub fn die(msg: impl std::fmt::Display) -> anyhow::Error {
    anyhow!("dns-zone-namecheap: {msg}")
}

pub fn resolve_cred_path(cli: Option<&Path>) -> PathBuf {
    if let Some(p) = cli {
        return p.to_path_buf();
    }
    if let Ok(p) = std::env::var("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    if let Ok(p) = std::env::var("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    PathBuf::from(DEFAULT_CRED_PATH)
}

pub fn load_credentials(path: &Path) -> Result<Credentials> {
    if path.as_os_str().is_empty() {
        return Err(die("credentials path empty"));
    }
    let meta = fs::symlink_metadata(path)
        .map_err(|_| die(format!("credentials file missing: {}", path.display())))?;
    if meta.file_type().is_symlink() {
        return Err(die(format!(
            "credentials path must be a regular file (symlink refused): {}",
            path.display()
        )));
    }
    if !meta.is_file() {
        return Err(die(format!("credentials file missing: {}", path.display())));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(die(format!(
            "credentials file must be owner-only (mode 0600; refuse group/world bits): {} (mode {:o})",
            path.display(),
            mode
        )));
    }

    let file = File::open(path)
        .map_err(|_| die(format!("credentials file unreadable: {}", path.display())))?;

    let mut api_user = String::new();
    let mut api_key = String::new();
    let mut user_name = String::new();
    let mut client_ip = String::new();
    let mut sld = String::new();
    let mut tld = String::new();

    for line in BufReader::new(file).lines() {
        let mut line =
            line.map_err(|_| die(format!("credentials unreadable: {}", path.display())))?;
        if let Some(stripped) = line.strip_suffix('\r') {
            line = stripped.to_string();
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            return Err(die(format!(
                "credentials line must be KEY=value (file={})",
                path.display()
            )));
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
            _ => {}
        }
    }

    if api_user.is_empty() {
        return Err(die(format!(
            "credentials missing ApiUser (file={})",
            path.display()
        )));
    }
    if api_key.is_empty() {
        return Err(die(format!(
            "credentials missing ApiKey (file={})",
            path.display()
        )));
    }
    if sld.is_empty() {
        return Err(die(format!(
            "credentials missing SLD (file={})",
            path.display()
        )));
    }
    if tld.is_empty() {
        return Err(die(format!(
            "credentials missing TLD (file={})",
            path.display()
        )));
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
        path: path.to_path_buf(),
    })
}

/// Attribute ` Name="..."` only (not FriendlyName). Live getHosts includes
/// FriendlyName="" after Name=; a greedy Name= match dropped every record.
pub fn xml_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!(" {name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_tag_name_ignores_friendly_name() {
        let tag = r#"<host HostId="1" Name="mail" Type="A" Address="203.0.113.11" MXPref="10" TTL="1800" AssociatedAppTitle="" FriendlyName="" IsActive="true" IsDDNSEnabled="false" />"#;
        assert_eq!(xml_attr(tag, "Name"), Some("mail"));
        assert_eq!(xml_attr(tag, "Type"), Some("A"));
    }

    #[test]
    fn host_tag_apex_at() {
        let tag = r#"<host HostId="2" Name="@" Type="A" Address="203.0.113.10" MXPref="10" TTL="1800" FriendlyName="" />"#;
        assert_eq!(xml_attr(tag, "Name"), Some("@"));
    }
}
