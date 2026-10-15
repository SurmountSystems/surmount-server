//! Lab VM guest: wait until Stalwart management HTTP is on 127.0.0.1:8080,
//! ensure the domain and permanent admin, mint an API key, and write that
//! token mode 0600. Does not create the mailbox and does not print the token.
//!
//! Domain, permanent admin, and API key minting go through the existing
//! bootstrap helpers. This module adds the port wait and the token file.

use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::bootstrap::{ensure_local, local_create, scrub};
use crate::error::{Result, ToolError};
use crate::json::extract_secret_line;
use crate::token::{assert_loopback_8080, which_or, write_api_token_file};

const FORWARD_UNIT: &str = "surmount-vm-lab-http-forward.service";
const MANAGEMENT_PORT: u16 = 8080;

pub struct VmLabDirectory {
    recovery_path: PathBuf,
    token_path: PathBuf,
    domain: String,
    url: String,
    cli: String,
    owner: Option<String>,
    forward_unit: String,
    http_attempts: u32,
    cli_attempts: u32,
    http_pause: Duration,
    forward_pause: Duration,
    cli_pause: Duration,
}

impl VmLabDirectory {
    pub fn guest_defaults() -> Self {
        Self {
            recovery_path: PathBuf::from("/var/lib/surmount/secrets/stalwart/recovery.env"),
            token_path: PathBuf::from("/var/lib/surmount/secrets/ui/stalwart-api-token"),
            domain: "example.test".to_string(),
            url: "http://127.0.0.1:8080".to_string(),
            cli: "stalwart-cli".to_string(),
            owner: Some("surmount-ui".to_string()),
            forward_unit: FORWARD_UNIT.to_string(),
            http_attempts: 90,
            cli_attempts: 15,
            http_pause: Duration::from_secs(1),
            forward_pause: Duration::from_millis(500),
            cli_pause: Duration::from_secs(2),
        }
    }

    /// Hermetic stand-in: no sleeps, no chown, fixed example.test paths the caller supplies.
    pub fn hermetic(
        recovery_path: impl Into<PathBuf>,
        token_path: impl Into<PathBuf>,
        cli: impl Into<PathBuf>,
    ) -> Self {
        let cli = cli.into();
        Self {
            recovery_path: recovery_path.into(),
            token_path: token_path.into(),
            domain: "example.test".to_string(),
            url: "http://127.0.0.1:8080".to_string(),
            cli: cli.to_string_lossy().into_owned(),
            owner: None,
            forward_unit: FORWARD_UNIT.to_string(),
            http_attempts: 3,
            cli_attempts: 3,
            http_pause: Duration::ZERO,
            forward_pause: Duration::ZERO,
            cli_pause: Duration::ZERO,
        }
    }
}

pub fn run_vm_lab_directory_guest<I, S>(args: I) -> Result<Vec<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let extra: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    if !extra.is_empty() {
        return Err(ToolError::fail(
            "vm-lab-directory-setup takes no arguments".to_string(),
        ));
    }
    let opts = VmLabDirectory::guest_defaults();
    run_vm_lab_directory(&opts, vm_lab_tcp_open, vm_lab_systemctl_start)
}

pub fn run_vm_lab_directory(
    opts: &VmLabDirectory,
    http_open: impl Fn(&str, u16) -> bool,
    start_forward: impl Fn(&str) -> Result<()>,
) -> Result<Vec<String>> {
    if opts.token_path == opts.recovery_path {
        return Err(ToolError::fail(
            "token path and recovery path must differ".to_string(),
        ));
    }
    absolute_no_dotdot(&opts.recovery_path)?;
    absolute_no_dotdot(&opts.token_path)?;
    let (user, password) = read_recovery_pin(&opts.recovery_path)?;
    bare_domain(&opts.domain)?;
    assert_loopback_8080(&opts.url)?;
    let cli_bin = which_or(&opts.cli)?;
    let mut lines = Vec::new();
    lines.push(wait_management(opts, &http_open, &start_forward)?);
    let primary = ensure_with_retry(opts, &cli_bin, &user, &password)?;
    lines.push("domain-ready".to_string());
    let token = mint_with_fallback(opts, &cli_bin, &primary, &user, &password)?;
    write_api_token_file(&opts.token_path, &token, opts.owner.as_deref())?;
    lines.push("token-ready".to_string());
    Ok(lines)
}

fn wait_management(
    opts: &VmLabDirectory,
    http_open: &impl Fn(&str, u16) -> bool,
    start_forward: &impl Fn(&str) -> Result<()>,
) -> Result<String> {
    let attempts = opts.http_attempts.max(1);
    for attempt in 0..attempts {
        if http_open("127.0.0.1", MANAGEMENT_PORT) {
            return Ok("management-http 127.0.0.1:8080".to_string());
        }
        if http_open("::1", MANAGEMENT_PORT) {
            start_forward(&opts.forward_unit)?;
            thread::sleep(opts.forward_pause);
            if http_open("127.0.0.1", MANAGEMENT_PORT) {
                return Ok("management-http forwarded from ::1".to_string());
            }
        }
        if attempt + 1 < attempts {
            thread::sleep(opts.http_pause);
        }
    }
    Err(ToolError::fail(
        "stalwart management HTTP did not accept 127.0.0.1:8080".to_string(),
    ))
}

fn ensure_with_retry(
    opts: &VmLabDirectory,
    cli: &str,
    user: &str,
    password: &str,
) -> Result<String> {
    let attempts = opts.cli_attempts.max(1);
    let mut last = None;
    for attempt in 0..attempts {
        match ensure_local(cli, &opts.url, user, &opts.domain, password) {
            Ok(login) => return Ok(login),
            Err(err) => {
                last = Some(err);
                if attempt + 1 < attempts {
                    thread::sleep(opts.cli_pause);
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| ToolError::fail("ensure directory failed".to_string())))
}

fn mint_with_fallback(
    opts: &VmLabDirectory,
    cli: &str,
    primary: &str,
    user: &str,
    password: &str,
) -> Result<String> {
    let mut logins = vec![primary.to_string()];
    if primary != user {
        logins.push(user.to_string());
    }
    let mut last_text = String::new();
    for login in logins {
        if let Some(token) = mint_login(opts, cli, &login, password, &mut last_text)? {
            return Ok(token);
        }
    }
    if !last_text.is_empty() {
        let safe = scrub(&last_text, password);
        if !safe.is_empty() {
            eprintln!("{safe}");
        }
    }
    Err(ToolError::fail(
        "create apikey did not return a Secret line".to_string(),
    ))
}

fn mint_login(
    opts: &VmLabDirectory,
    cli: &str,
    login: &str,
    password: &str,
    last_text: &mut String,
) -> Result<Option<String>> {
    let attempts = opts.cli_attempts.max(1);
    for attempt in 0..attempts {
        let (text, code) = local_create(cli, &opts.url, login, password, "vm-receive-proof")?;
        *last_text = text;
        if code == 0 {
            if let Some(token) = extract_secret_line(last_text) {
                return Ok(Some(token));
            }
        }
        if attempt + 1 < attempts {
            thread::sleep(opts.cli_pause);
        }
    }
    Ok(None)
}

fn read_recovery_pin(path: &Path) -> Result<(String, String)> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() {
        return Err(ToolError::fail(
            "recovery pin path must not be a symlink".to_string(),
        ));
    }
    if !meta.is_file() {
        return Err(ToolError::fail(
            "recovery pin path is not a regular file".to_string(),
        ));
    }
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    };
    if mode & 0o077 != 0 {
        return Err(ToolError::fail(format!(
            "recovery pin is group or world accessible (mode {:03o})",
            mode & 0o777
        )));
    }
    let text = fs::read_to_string(path)?;
    parse_recovery_pin(&text)
}

fn parse_recovery_pin(text: &str) -> Result<(String, String)> {
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let Some(rest) = line.strip_prefix("STALWART_RECOVERY_ADMIN=") else {
            continue;
        };
        let Some((user, password)) = rest.split_once(':') else {
            return Err(ToolError::fail("recovery pin shape refused".to_string()));
        };
        if user.is_empty() || password.is_empty() || password.chars().any(|c| c.is_control()) {
            return Err(ToolError::fail("recovery pin shape refused".to_string()));
        }
        return Ok((user.to_string(), password.to_string()));
    }
    Err(ToolError::fail("recovery pin missing".to_string()))
}

fn bare_domain(domain: &str) -> Result<()> {
    if domain.is_empty()
        || domain.contains('/')
        || domain.contains(' ')
        || domain.starts_with('-')
        || domain.contains('@')
        || domain.contains('\\')
    {
        return Err(ToolError::fail(
            "domain must be a bare hostname (got invalid shape; values not logged)".to_string(),
        ));
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

fn vm_lab_tcp_open(host: &str, port: u16) -> bool {
    if port != MANAGEMENT_PORT {
        return false;
    }
    let addr = match host {
        "127.0.0.1" => SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
        "::1" => SocketAddr::from((Ipv6Addr::LOCALHOST, port)),
        _ => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok()
}

fn vm_lab_systemctl_start(unit: &str) -> Result<()> {
    if unit != FORWARD_UNIT {
        return Err(ToolError::fail("forward unit name refused".to_string()));
    }
    let _ = std::process::Command::new("systemctl")
        .args(["start", unit])
        .output()?;
    Ok(())
}
