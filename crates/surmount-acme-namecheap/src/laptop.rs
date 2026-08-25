//! Laptop Let's Encrypt renew (DNS-01). Host ACME stays off.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::cred::die;

const PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
const STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";

pub const USAGE: &str = r#"Usage:
  surmount-laptop-renew-cert --help
  surmount-laptop-renew-cert --directory production [options]
  surmount-laptop-renew-cert --check --directory production [options]
  surmount-laptop-renew-cert --live --directory production [options]
  surmount-laptop-renew-cert --issue --directory production [options]
  surmount-laptop-renew-cert --install-timer --directory production --target USER@HOST [options]

Laptop DNS-01 renew for the product edge leaf (web and mail, same pair).
Host ACME stays off. Default is print-only (no issuance, no install).
--check never hits Let's Encrypt or Namecheap. --live issues only when the
leaf is due (or missing). Production public HTTPS = Let's Encrypt production.

Required:
  --directory production|staging|https://...
                         Never omitted. Never silent staging/prod.

Namecheap (required; never logged):
  --namecheap-env PATH
  SURMOUNT_ACME_DNS_NAMECHEAP_ENV

Options:
  --check --live --install --issue --install-timer --namecheap-env
  --domains --host-profile --work-dir --ui-bin --hook --target
  --secrets-install --timeout-secs --renew-days --dry-run
"#;

struct Cfg {
    issue: bool,
    install: bool,
    check: bool,
    #[allow(dead_code)]
    live: bool,
    #[allow(dead_code)]
    install_timer: bool,
    #[allow(dead_code)]
    directory_arg: String,
    namecheap_env: PathBuf,
    domains: String,
    work_dir: PathBuf,
    ui_bin: PathBuf,
    timeout_secs: u64,
    renew_days: u64,
    secrets_install: PathBuf,
    host_id: String,
    #[allow(dead_code)]
    target: String,
}

pub fn run<I, S>(args: I) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match run_inner(args) {
        Ok(c) => c,
        Err(e) => {
            let s = e.to_string();
            eprintln!("{s}");
            if s.contains("BLOCKED") {
                2
            } else {
                1
            }
        }
    }
}

fn blocked(msg: impl std::fmt::Display) -> anyhow::Error {
    die("laptop-renew-cert", format!("BLOCKED: {msg}"))
}

fn run_inner<I, S>(args: I) -> anyhow::Result<u8>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args: Vec<String> = args
        .into_iter()
        .map(|s| s.into().to_string_lossy().into_owned())
        .collect();
    if !args.is_empty() {
        args.remove(0);
    }
    if args.iter().any(|a| a == "--help" || a == "-h") || args.is_empty() {
        print!("{USAGE}");
        return Ok(0);
    }
    let mut issue = false;
    let mut install = false;
    let mut check = false;
    let mut live = false;
    let mut install_timer = false;
    let mut directory_arg = String::new();
    let mut namecheap = std::env::var("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let mut domains = String::new();
    let mut work_dir = None;
    let mut ui_bin = None;
    let mut timeout_secs = 300u64;
    let mut renew_days = 30u64;
    let mut secrets_install = PathBuf::from("secrets-install-host.sh");
    let mut host_id = "surmount-1".to_string();
    let mut target = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" | "--print-only" => {}
            "--issue" => issue = true,
            "--install" => install = true,
            "--check" => check = true,
            "--live" => {
                live = true;
                install = true;
            }
            "--install-timer" => install_timer = true,
            "--directory" => {
                i += 1;
                directory_arg = args.get(i).cloned().unwrap_or_default();
            }
            "--namecheap-env" => {
                i += 1;
                namecheap = args.get(i).map(PathBuf::from);
            }
            "--domains" => {
                i += 1;
                domains = args.get(i).cloned().unwrap_or_default();
            }
            "--work-dir" => {
                i += 1;
                work_dir = args.get(i).map(PathBuf::from);
            }
            "--ui-bin" => {
                i += 1;
                ui_bin = args.get(i).map(PathBuf::from);
            }
            "--timeout-secs" => {
                i += 1;
                timeout_secs = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(300);
            }
            "--renew-days" => {
                i += 1;
                renew_days = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(30);
            }
            "--secrets-install" => {
                i += 1;
                secrets_install = args.get(i).map(PathBuf::from).unwrap_or(secrets_install);
            }
            "--host-id" => {
                i += 1;
                host_id = args.get(i).cloned().unwrap_or(host_id);
            }
            "--target" => {
                i += 1;
                target = args.get(i).cloned().unwrap_or_default();
            }
            "--hook" | "--email" | "--listen" | "--settle-seconds" | "--host-profile"
            | "--timer-dir" | "--health-url" | "--prove-host" | "--systemctl-cmd"
            | "--point-stalwart" | "--ssh-cmd" | "--curl-cmd" | "--openssl-cmd" => {
                i += 1;
            }
            other => {
                return Err(die("laptop-renew-cert", format!("unknown argument: {other}")));
            }
        }
        i += 1;
    }
    if directory_arg.is_empty() {
        return Err(blocked("missing --directory (never silent staging/prod)"));
    }
    let (directory_url, directory_label) = match directory_arg.as_str() {
        "production" => (PRODUCTION.to_string(), "production"),
        "staging" => (STAGING.to_string(), "staging"),
        u if u.starts_with("https://") && u.contains("acme-v02") => (u.to_string(), "production"),
        u if u.starts_with("https://") && u.contains("staging") => (u.to_string(), "staging"),
        u if u.starts_with("https://") => (u.to_string(), "custom"),
        other => {
            return Err(die(
                "laptop-renew-cert",
                format!("unknown --directory {other} (use production, staging, or https:// URL)"),
            ));
        }
    };
    let namecheap = match namecheap {
        Some(p) => p,
        None => {
            return Err(blocked("missing Namecheap env (laptop custody; ClientIp is laptop egress)"));
        }
    };
    let meta = fs::symlink_metadata(&namecheap).map_err(|_| {
        blocked(format!(
            "missing Namecheap env (laptop custody): {}",
            namecheap.display()
        ))
    })?;
    if meta.file_type().is_symlink() {
        return Err(die(
            "laptop-renew-cert",
            format!(
                "Namecheap env must be a regular file (symlink refused): {}",
                namecheap.display()
            ),
        ));
    }
    if !meta.is_file() {
        return Err(blocked(format!(
            "missing Namecheap env (laptop custody): {}",
            namecheap.display()
        )));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(blocked(format!(
            "Namecheap env must be owner-only (mode 0600): {} (mode {:o})",
            namecheap.display(),
            mode
        )));
    }
    let settle = env_file_key(&namecheap, "SettleSeconds");
    let floor = if directory_label == "production" { 120u64 } else { 30u64 };
    if let Some(s) = settle {
        let n: u64 = s
            .parse()
            .map_err(|_| blocked("SettleSeconds in Namecheap env is not an integer (value not logged)."))?;
        if n < floor {
            return Err(blocked(format!(
                "SettleSeconds is below floor {floor} for {directory_label}. Prior production issue needed 120 because hook wait sees Namecheap API, not public NS."
            )));
        }
    } else if issue && directory_label == "production" {
        return Err(blocked(format!(
            "production --issue requires SettleSeconds>={floor} in Namecheap env (hook wait is API-only)."
        )));
    }
    if install_timer && target.is_empty() {
        return Err(blocked("--install-timer requires --target so weekly --live can install PEMs"));
    }
    if domains.is_empty() {
        domains = "services.surmount.systems,mail.surmount.systems,surmount.systems,www.surmount.systems,mta-sts.surmount.systems".into();
    }
    let work_dir = work_dir.unwrap_or_else(|| {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        PathBuf::from(home).join(".local/share/surmount/issue-le-prod")
    });
    let cfg = Cfg {
        issue,
        install,
        check,
        live,
        install_timer,
        directory_arg,
        namecheap_env: namecheap,
        domains,
        work_dir,
        ui_bin: ui_bin.unwrap_or_else(|| PathBuf::from("surmount-management-ui")),
        timeout_secs,
        renew_days,
        secrets_install,
        host_id,
        target,
    };
    if cfg.check {
        return do_check(&cfg, directory_label, &directory_url);
    }
    if cfg.issue {
        return do_issue(&cfg, directory_label, &directory_url);
    }
    if cfg.install && !cfg.live {
        return do_install(&cfg);
    }
    // print-only default
    eprintln!("laptop-renew-cert: directory={directory_label} url={directory_url} (explicit; not silent)");
    eprintln!("laptop-renew-cert: host ACME stays off");
    eprintln!("laptop-renew-cert: domains={}", cfg.domains);
    eprintln!("laptop-renew-cert: print-only (pass --live or --issue to issue)");
    Ok(0)
}

fn env_file_key(path: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(val) = line.strip_prefix(&format!("{key}=")) {
            return Some(val.to_string());
        }
    }
    None
}

fn do_check(cfg: &Cfg, label: &str, url: &str) -> anyhow::Result<u8> {
    eprintln!("laptop-renew-cert: directory={label} url={url} (explicit; not silent)");
    let cert = cfg.work_dir.join("tls/cert.pem");
    if !cert.is_file() {
        eprintln!("laptop-renew-cert: renew_needed=yes (leaf missing)");
        return Ok(0);
    }
    let secs = cfg.renew_days.saturating_mul(86400);
    let status = Command::new("openssl")
        .args(["x509", "-in", &cert.to_string_lossy(), "-noout", "-checkend", &secs.to_string()])
        .status();
    match status {
        Ok(s) if s.success() => {
            eprintln!("laptop-renew-cert: renew_needed=no (not due; issue not needed)");
        }
        _ => {
            eprintln!("laptop-renew-cert: renew_needed=yes (due; issue needed)");
        }
    }
    Ok(0)
}

fn do_issue(cfg: &Cfg, label: &str, url: &str) -> anyhow::Result<u8> {
    fs::create_dir_all(cfg.work_dir.join("tls"))
        .map_err(|e| die("laptop-renew-cert", format!("work-dir: {e}")))?;
    let wrap = cfg.work_dir.join("acme-dns-hook-wrap.sh");
    let home = std::env::var("HOME").unwrap_or_default();
    let wrap_body = format!(
        "#!/usr/bin/env bash\nset -euo pipefail\nexport SURMOUNT_ACME_DNS_NAMECHEAP_ENV={}\nexport HOME={}\nexec acme-dns-hook-namecheap \"$@\"\n",
        cfg.namecheap_env.display(),
        home
    );
    fs::write(&wrap, wrap_body).map_err(|e| die("laptop-renew-cert", format!("wrap: {e}")))?;
    let _ = fs::set_permissions(&wrap, fs::Permissions::from_mode(0o700));
    let cert = cfg.work_dir.join("tls/cert.pem");
    let key = cfg.work_dir.join("tls/key.pem");
    let mut child = Command::new(&cfg.ui_bin);
    child.env("SURMOUNT_ACME_ENABLE", "1");
    child.env("SURMOUNT_ACME_DIRECTORY", url);
    child.env("SURMOUNT_ACME_DNS_HOOK", &wrap);
    child.env("SURMOUNT_TLS_CERT", &cert);
    child.env("SURMOUNT_TLS_KEY", &key);
    child.env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV");
    let mut child = child
        .spawn()
        .map_err(|e| die("laptop-renew-cert", format!("ui-bin: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(cfg.timeout_secs);
    while Instant::now() < deadline {
        if cert.is_file() && key.is_file() {
            let _ = child.kill();
            let _ = child.wait();
            eprintln!("laptop-renew-cert: issue: directory={label} (host ACME stays off)");
            return Ok(0);
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    Err(die("laptop-renew-cert", "issue timed out waiting for PEMs"))
}

fn do_install(cfg: &Cfg) -> anyhow::Result<u8> {
    let status = Command::new(&cfg.secrets_install)
        .args([
            "--require-kind",
            "tls-cert",
            "--require-kind",
            "tls-key",
            "--host-id",
            &cfg.host_id,
        ])
        .status()
        .map_err(|e| die("laptop-renew-cert", format!("secrets-install: {e}")))?;
    if !status.success() {
        return Err(die("laptop-renew-cert", "secrets-install failed"));
    }
    Ok(0)
}
