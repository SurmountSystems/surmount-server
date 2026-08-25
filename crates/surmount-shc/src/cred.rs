use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::die;

pub const DEFAULT_CRED_PATH: &str = "/var/lib/surmount/secrets/rdns/shc.env";
pub const DEFAULT_API_BASE: &str = "https://blesta.sovereignhybridcompute.com/user-api/v2";
pub const DEFAULT_HOSTNAME: &str = "mail.surmount.systems";

#[derive(Clone)]
pub struct Credentials {
    pub api_key: String,
    pub api_base: String,
    pub service_id: Option<String>,
    pub path: PathBuf,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("api_key", &"<redacted>")
            .field("api_base", &self.api_base)
            .field("service_id", &self.service_id)
            .field("path", &self.path)
            .finish()
    }
}

pub fn resolve_cred_path(
    env_override: Option<&Path>,
    default_exists: bool,
    staging_secret: Option<&Path>,
) -> PathBuf {
    if let Some(p) = env_override {
        return p.to_path_buf();
    }
    if default_exists {
        return PathBuf::from(DEFAULT_CRED_PATH);
    }
    if let Some(p) = staging_secret {
        if p.is_file() {
            return p.to_path_buf();
        }
    }
    PathBuf::from(DEFAULT_CRED_PATH)
}

pub fn staging_secret_from_env(xdg_data_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let root = if let Some(xdg) = xdg_data_home.filter(|s| !s.is_empty()) {
        PathBuf::from(xdg).join("surmount/staging")
    } else {
        PathBuf::from(home.filter(|s| !s.is_empty())?).join(".local/share/surmount/staging")
    };
    Some(root.join("shc-api/secret"))
}

impl Credentials {
    pub fn load(
        path: &Path,
        env_api_base: Option<&str>,
        cli_service_id: Option<&str>,
    ) -> Result<Self> {
        if path.as_os_str().is_empty() {
            return Err(die("credentials path empty"));
        }
        let meta = fs::symlink_metadata(path).map_err(|_| {
            die(format!(
                "credentials file missing: {} (run: just secrets-prompt -- shc-api --host surmount-1)",
                path.display()
            ))
        })?;
        if meta.file_type().is_symlink() {
            return Err(die(format!(
                "credentials path must be a regular file (symlink refused): {}",
                path.display()
            )));
        }
        if !meta.is_file() {
            return Err(die(format!("credentials file missing: {}", path.display())));
        }
        let file = File::open(path)
            .map_err(|_| die(format!("credentials file unreadable: {}", path.display())))?;

        let mut api_key = String::new();
        let mut api_base = env_api_base.unwrap_or("").to_string();
        let mut service_id = String::new();

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
            if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
                val = val[1..val.len() - 1].to_string();
            }
            match key {
                "ApiKey" => api_key = val,
                "ApiBase" => api_base = val,
                "ServiceId" => service_id = val,
                _ => {}
            }
        }

        if api_key.is_empty() {
            return Err(die(format!(
                "credentials missing ApiKey= in {}",
                path.display()
            )));
        }
        if api_base.is_empty() {
            api_base = DEFAULT_API_BASE.to_string();
        }
        while api_base.ends_with('/') {
            api_base.pop();
        }

        let service_id = if let Some(cli) = cli_service_id.filter(|s| !s.is_empty()) {
            Some(cli.to_string())
        } else if service_id.is_empty() {
            None
        } else {
            Some(service_id)
        };

        Ok(Self {
            api_key,
            api_base,
            service_id,
            path: path.to_path_buf(),
        })
    }

    pub fn require_service_id(&self) -> Result<&str> {
        self.service_id.as_deref().ok_or_else(|| {
            die(
                "ServiceId required (set in credentials ServiceId=, or pass --service-id, or run --list-vms first)",
            )
        })
    }
}
