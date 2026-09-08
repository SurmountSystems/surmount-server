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
  --secrets-install --timeout-secs --renew-days --dry-run --restart-ui
  --email --listen --ssh-cmd

--hook is the DNS-01 wrap inner (absolute regular file). Extra-zone TXT
needs acme-dns-hook-namecheap-dispatch. When --hook is omitted and
--domains includes a zone besides surmount.systems, wrap defaults to
that dispatcher (sibling of this binary when present).

Host-profile without --domains floors the intended 20-name leaf (includes
exophiles.org and www.exophiles.org) and strips Cloudflare extras
(btcdragonlord.com, btckitties.com). Production --issue that names any
intended extra zone must include the full 20-name list (no silent drop).
"#;

/// Intended production leaf (web + mail). One PEM, with_single_cert.
const INTENDED_LEAF_DOMAINS: &[&str] = &[
    "surmount.systems",
    "www.surmount.systems",
    "mail.surmount.systems",
    "services.surmount.systems",
    "mta-sts.surmount.systems",
    "baxterartworks.com",
    "www.baxterartworks.com",
    "btcfur.com",
    "www.btcfur.com",
    "exophiles.org",
    "www.exophiles.org",
    "iantuckerstudios.com",
    "www.iantuckerstudios.com",
    "nostrfurs.com",
    "www.nostrfurs.com",
    "yiffa.app",
    "www.yiffa.app",
    "cryptoquick.com",
    "www.cryptoquick.com",
    "mail.cryptoquick.com",
];

const CLOUDFLARE_EXTRA_ZONES: &[&str] = &[
    "btcdragonlord.com",
    "www.btcdragonlord.com",
    "btckitties.com",
    "www.btckitties.com",
];

struct Cfg {
    issue: bool,
    install: bool,
    check: bool,
    #[allow(dead_code)]
    live: bool,
    #[allow(dead_code)]
    install_timer: bool,
    restart_ui: bool,
    #[allow(dead_code)]
    directory_arg: String,
    namecheap_env: PathBuf,
    domains: String,
    email: String,
    work_dir: PathBuf,
    ui_bin: PathBuf,
    hook: Option<PathBuf>,
    timeout_secs: u64,
    renew_days: u64,
    secrets_install: PathBuf,
    host_id: String,
    target: String,
    listen: String,
    ssh_cmd: String,
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
            if s.contains("BLOCKED") { 2 } else { 1 }
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
    let mut domains_from_cli = false;
    let mut work_dir = None;
    let mut ui_bin = None;
    let mut timeout_secs = 300u64;
    let mut renew_days = 30u64;
    let mut secrets_install = PathBuf::from("secrets-install-host.sh");
    let mut host_id = "surmount-1".to_string();
    let mut host_id_set = false;
    let mut target = String::new();
    let mut hook = None;
    let mut host_profile = None;
    let mut email = String::new();
    let mut listen = String::new();
    let mut ssh_cmd = "ssh".to_string();
    let mut restart_ui = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--dry-run" | "--print-only" => {}
            "--issue" => issue = true,
            "--install" => install = true,
            "--check" => check = true,
            "--restart-ui" => restart_ui = true,
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
                domains_from_cli = !domains.trim().is_empty();
            }
            "--email" => {
                i += 1;
                email = args.get(i).cloned().unwrap_or_default();
            }
            "--work-dir" => {
                i += 1;
                work_dir = args.get(i).map(PathBuf::from);
            }
            "--ui-bin" => {
                i += 1;
                ui_bin = args.get(i).map(PathBuf::from);
            }
            "--hook" => {
                i += 1;
                hook = args
                    .get(i)
                    .map(PathBuf::from)
                    .filter(|p| !p.as_os_str().is_empty());
            }
            "--host-profile" => {
                i += 1;
                host_profile = args.get(i).map(PathBuf::from);
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
                host_id_set = true;
            }
            "--target" => {
                i += 1;
                target = args.get(i).cloned().unwrap_or_default();
            }
            "--listen" => {
                i += 1;
                listen = args.get(i).cloned().unwrap_or_default();
            }
            "--ssh-cmd" => {
                i += 1;
                ssh_cmd = args.get(i).cloned().unwrap_or_else(|| "ssh".into());
            }
            "--settle-seconds" | "--timer-dir" | "--health-url" | "--prove-host"
            | "--systemctl-cmd" | "--point-stalwart" | "--curl-cmd" | "--openssl-cmd" => {
                i += 1;
            }
            other => {
                return Err(die(
                    "laptop-renew-cert",
                    format!("unknown argument: {other}"),
                ));
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
            return Err(blocked(
                "missing Namecheap env (laptop custody; ClientIp is laptop egress)",
            ));
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
    let floor = if directory_label == "production" {
        120u64
    } else {
        30u64
    };
    if let Some(s) = settle {
        let n: u64 = s.parse().map_err(|_| {
            blocked("SettleSeconds in Namecheap env is not an integer (value not logged).")
        })?;
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
        return Err(blocked(
            "--install-timer requires --target so weekly --live can install PEMs",
        ));
    }
    if let Some(profile) = &host_profile {
        let p = load_host_profile_acme(profile)?;
        if !domains_from_cli {
            domains = floor_intended_leaf(&p.domains);
        }
        if email.is_empty() {
            email = p.email;
        }
        if !host_id_set && !p.host_id.is_empty() {
            host_id = p.host_id;
        }
    }
    if domains.is_empty() {
        domains = "services.surmount.systems,mail.surmount.systems,surmount.systems,www.surmount.systems,mta-sts.surmount.systems".into();
    }
    if issue || live {
        refuse_dropped_intended_leaf(&domains)?;
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
        restart_ui,
        directory_arg,
        namecheap_env: namecheap,
        domains,
        email,
        work_dir: absolutize(&work_dir),
        ui_bin: ui_bin.unwrap_or_else(|| PathBuf::from("surmount-management-ui")),
        hook,
        timeout_secs,
        renew_days,
        secrets_install,
        host_id,
        target,
        listen,
        ssh_cmd,
    };
    if cfg.check {
        return do_check(&cfg, directory_label, &directory_url);
    }
    if cfg.issue {
        do_issue(&cfg, directory_label, &directory_url)?;
        if !cfg.install && !cfg.restart_ui {
            return Ok(0);
        }
    }
    if cfg.install && (cfg.issue || !cfg.live) {
        do_install(&cfg)?;
    }
    if cfg.restart_ui {
        do_restart_ui(&cfg)?;
        return Ok(0);
    }
    if cfg.issue || cfg.install {
        return Ok(0);
    }
    eprintln!(
        "laptop-renew-cert: directory={directory_label} url={directory_url} (explicit; not silent)"
    );
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
        .args([
            "x509",
            "-in",
            &cert.to_string_lossy(),
            "-noout",
            "-checkend",
            &secs.to_string(),
        ])
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
    eprintln!("laptop-renew-cert: domains={}", cfg.domains);
    fs::create_dir_all(cfg.work_dir.join("tls"))
        .map_err(|e| die("laptop-renew-cert", format!("work-dir: {e}")))?;
    // --issue is force: leftover PEMs must not look like a new leaf. --live
    // is expiry-only and does not detect missing certificate hostnames.
    let cert = absolutize(&cfg.work_dir.join("tls/cert.pem"));
    let key = absolutize(&cfg.work_dir.join("tls/key.pem"));
    let _ = fs::remove_file(&cert);
    let _ = fs::remove_file(&key);
    let wrap = cfg.work_dir.join("acme-dns-hook-wrap.sh");
    let hook_log = cfg.work_dir.join("hook-stderr.log");
    let home = std::env::var("HOME").unwrap_or_default();
    let hook = resolve_hook(cfg)?;
    let inner = sibling_bin_of(&hook, "acme-dns-hook-namecheap");
    let wrap_body = wrap_script(
        &cfg.namecheap_env,
        &home,
        &hook,
        inner.as_deref(),
        &hook_log,
    );
    fs::write(&wrap, wrap_body).map_err(|e| die("laptop-renew-cert", format!("wrap: {e}")))?;
    let _ = fs::set_permissions(&wrap, fs::Permissions::from_mode(0o700));
    let wrap_abs = absolutize(&wrap);
    let account = absolutize(&cfg.work_dir.join("acme-account.json"));
    let listen = if cfg.listen.is_empty() {
        "127.0.0.1:18780"
    } else {
        cfg.listen.as_str()
    };
    let mut child = Command::new(&cfg.ui_bin);
    child.env("SURMOUNT_ACME_ENABLE", "1");
    child.env("SURMOUNT_ACME_DIRECTORY", url);
    child.env("SURMOUNT_ACME_DOMAINS", &cfg.domains);
    child.env("SURMOUNT_ACME_DNS_PROVIDER", "external-hook");
    child.env("SURMOUNT_ACME_DNS_HOOK", &wrap_abs);
    child.env("SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS", "600");
    child.env("SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH", &account);
    child.env("SURMOUNT_TLS_CERT", &cert);
    child.env("SURMOUNT_TLS_KEY", &key);
    child.env("SURMOUNT_LISTEN_MODE", "https");
    child.env("SURMOUNT_LISTEN", listen);
    child.env("SURMOUNT_AUTH_MODE", "off");
    if !cfg.email.is_empty() {
        child.env("SURMOUNT_ACME_EMAIL", &cfg.email);
    }
    child.env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV");
    let mut child = child
        .spawn()
        .map_err(|e| die("laptop-renew-cert", format!("ui-bin: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(cfg.timeout_secs);
    loop {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(issue_fail(cfg, "issue timed out waiting for PEMs"));
        }
        match child.try_wait() {
            Ok(Some(st)) => {
                if cert.is_file() && key.is_file() {
                    eprintln!("laptop-renew-cert: issue: directory={label} (host ACME stays off)");
                    assert_issued_sans_cover_requested(cfg)?;
                    return Ok(0);
                }
                if !st.success() {
                    return Err(issue_fail(
                        cfg,
                        format!(
                            "ui-bin/hook child exited non-zero ({})",
                            st.code().unwrap_or(-1)
                        ),
                    ));
                }
                return Err(issue_fail(cfg, "ui-bin exited before PEMs"));
            }
            Ok(None) => {
                if cert.is_file() && key.is_file() {
                    let _ = child.kill();
                    let _ = child.wait();
                    eprintln!("laptop-renew-cert: issue: directory={label} (host ACME stays off)");
                    assert_issued_sans_cover_requested(cfg)?;
                    return Ok(0);
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(die("laptop-renew-cert", format!("ui-bin wait: {e}")));
            }
        }
    }
}

fn issue_fail(cfg: &Cfg, msg: impl std::fmt::Display) -> anyhow::Error {
    let log = cfg.work_dir.join("hook-stderr.log");
    if let Ok(t) = fs::read_to_string(&log) {
        let low = t.to_ascii_lowercase();
        if low.contains("invalid request ip") {
            return blocked(
                "Invalid request IP (Namecheap API ClientIp whitelist of laptop egress)",
            );
        }
    }
    die("laptop-renew-cert", msg)
}

fn do_install(cfg: &Cfg) -> anyhow::Result<u8> {
    let cert = cfg.work_dir.join("tls/cert.pem");
    let key = cfg.work_dir.join("tls/key.pem");
    if cert.is_file() && key.is_file() {
        stage_tls_items(cfg, &cert, &key)?;
    }
    let mut args: Vec<String> = vec![
        "--require-kind".into(),
        "tls-cert".into(),
        "--require-kind".into(),
        "tls-key".into(),
        "--host-id".into(),
        cfg.host_id.clone(),
    ];
    let staging = cfg.work_dir.join("staging");
    if staging.is_dir() {
        args.push("--from-staging".into());
        args.push(staging.to_string_lossy().into_owned());
    }
    if !cfg.target.is_empty() {
        args.push("--target".into());
        args.push(cfg.target.clone());
    }
    if !cfg.ssh_cmd.is_empty() {
        args.push("--ssh-cmd".into());
        args.push(cfg.ssh_cmd.clone());
    }
    let status = Command::new(&cfg.secrets_install)
        .args(&args)
        .status()
        .map_err(|e| die("laptop-renew-cert", format!("secrets-install: {e}")))?;
    if !status.success() {
        return Err(die("laptop-renew-cert", "secrets-install failed"));
    }
    Ok(0)
}

fn do_restart_ui(cfg: &Cfg) -> anyhow::Result<u8> {
    if cfg.target.is_empty() {
        return Err(blocked(
            "--restart-ui requires --target USER@HOST (host ACME stays off)",
        ));
    }
    let status = Command::new(&cfg.ssh_cmd)
        .args([
            &cfg.target,
            "systemctl",
            "restart",
            "surmount-management-ui",
        ])
        .status()
        .map_err(|e| die("laptop-renew-cert", format!("restart-ui ssh: {e}")))?;
    if !status.success() {
        return Err(die("laptop-renew-cert", "restart-ui failed"));
    }
    eprintln!("laptop-renew-cert: restarted surmount-management-ui on target");
    Ok(0)
}

fn stage_tls_items(cfg: &Cfg, cert: &Path, key: &Path) -> anyhow::Result<()> {
    let staging = cfg.work_dir.join("staging");
    write_stage_item(
        &staging.join("tls-cert"),
        "tls-cert",
        &cfg.host_id,
        "/var/lib/surmount/secrets/tls/cert.pem",
        cert,
    )?;
    write_stage_item(
        &staging.join("tls-key"),
        "tls-key",
        &cfg.host_id,
        "/var/lib/surmount/secrets/tls/key.pem",
        key,
    )?;
    Ok(())
}

fn write_stage_item(
    dir: &Path,
    kind: &str,
    host: &str,
    hpath: &str,
    payload: &Path,
) -> anyhow::Result<()> {
    fs::create_dir_all(dir).map_err(|e| die("laptop-renew-cert", format!("staging: {e}")))?;
    fs::write(
        dir.join("attributes"),
        format!("surmount.kind={kind}\nsurmount.host={host}\nsurmount.path={hpath}\n"),
    )
    .map_err(|e| die("laptop-renew-cert", format!("staging: {e}")))?;
    let bytes = fs::read(payload).map_err(|e| die("laptop-renew-cert", format!("staging: {e}")))?;
    fs::write(dir.join("secret"), bytes)
        .map_err(|e| die("laptop-renew-cert", format!("staging: {e}")))?;
    let _ = fs::set_permissions(dir.join("secret"), fs::Permissions::from_mode(0o600));
    Ok(())
}

fn wrap_script(
    namecheap_env: &Path,
    home: &str,
    hook: &Path,
    inner: Option<&Path>,
    hook_log: &Path,
) -> String {
    let mut s = String::from("#!/bin/sh\nset -eu\n");
    s.push_str("export SURMOUNT_ACME_DNS_NAMECHEAP_ENV=");
    s.push_str(&sh_quote(&namecheap_env.display().to_string()));
    s.push('\n');
    s.push_str("export HOME=");
    s.push_str(&sh_quote(home));
    s.push('\n');
    if let Some(inner) = inner {
        s.push_str("export SURMOUNT_ACME_DNS_DISPATCH_INNER_HOOK=");
        s.push_str(&sh_quote(&inner.display().to_string()));
        s.push('\n');
    }
    s.push_str("exec ");
    s.push_str(&sh_quote(&hook.display().to_string()));
    s.push_str(" \"$@\" 2>>");
    s.push_str(&sh_quote(&hook_log.display().to_string()));
    s.push('\n');
    s
}

fn resolve_hook(cfg: &Cfg) -> anyhow::Result<PathBuf> {
    if let Some(h) = &cfg.hook {
        let h = absolutize(h);
        validate_hook_file(&h)?;
        return Ok(h);
    }
    let name = if extra_namecheap_zones(&cfg.domains) {
        "acme-dns-hook-namecheap-dispatch"
    } else {
        "acme-dns-hook-namecheap"
    };
    Ok(sibling_bin(name).unwrap_or_else(|| PathBuf::from(name)))
}

fn extra_namecheap_zones(domains: &str) -> bool {
    domains.split(',').any(|d| {
        let d = normalize_name(d);
        !d.is_empty() && d != "surmount.systems" && !d.ends_with(".surmount.systems")
    })
}

fn normalize_name(d: &str) -> String {
    d.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn parse_domain_list(domains: &str) -> Vec<String> {
    domains
        .split(',')
        .map(normalize_name)
        .filter(|d| !d.is_empty())
        .collect()
}

fn is_cloudflare_extra(name: &str) -> bool {
    CLOUDFLARE_EXTRA_ZONES.iter().any(|z| *z == name)
}

fn floor_intended_leaf(profile_domains: &[String]) -> String {
    let mut out: Vec<String> = INTENDED_LEAF_DOMAINS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for raw in profile_domains {
        let d = normalize_name(raw);
        if d.is_empty() || is_cloudflare_extra(&d) {
            continue;
        }
        if !out.iter().any(|x| x == &d) {
            out.push(d);
        }
    }
    out.join(",")
}

fn refuse_dropped_intended_leaf(domains: &str) -> anyhow::Result<()> {
    let have = parse_domain_list(domains);
    let extras_requested = have.iter().any(|d| {
        INTENDED_LEAF_DOMAINS.contains(&d.as_str())
            && *d != "surmount.systems"
            && !d.ends_with(".surmount.systems")
    });
    if !extras_requested {
        return Ok(());
    }
    let missing: Vec<&str> = INTENDED_LEAF_DOMAINS
        .iter()
        .copied()
        .filter(|want| !have.iter().any(|h| h == want))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(blocked(format!(
        "production extra-zone issue must keep the intended 20-name leaf (missing {}); do not drop Namecheap static zones such as exophiles.org",
        missing.join(",")
    )))
}

fn assert_issued_sans_cover_requested(cfg: &Cfg) -> anyhow::Result<()> {
    let cert = cfg.work_dir.join("tls/cert.pem");
    let output = Command::new("openssl")
        .args([
            "x509",
            "-in",
            &cert.to_string_lossy(),
            "-noout",
            "-ext",
            "subjectAltName",
        ])
        .output();
    let Ok(output) = output else {
        return Ok(());
    };
    if !output.status.success() {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
    if !text.contains("dns:") {
        return Ok(());
    }
    let sans: Vec<String> = text
        .split([',', '\n', ' '])
        .filter_map(|tok| {
            let t = tok.trim().trim_end_matches(',');
            t.strip_prefix("dns:")
                .map(|n| n.trim_end_matches('.').to_string())
        })
        .filter(|n| !n.is_empty())
        .collect();
    let missing: Vec<String> = parse_domain_list(&cfg.domains)
        .into_iter()
        .filter(|d| !sans.iter().any(|s| s == d))
        .collect();
    if missing.is_empty() {
        eprintln!(
            "laptop-renew-cert: issued certificate hostnames cover requested list (count={})",
            sans.len()
        );
        return Ok(());
    }
    Err(die(
        "laptop-renew-cert",
        format!(
            "issued certificate omitted requested hostnames: {}",
            missing.join(",")
        ),
    ))
}

fn sibling_bin(name: &str) -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| sibling_bin_of(&exe, name))
}

fn sibling_bin_of(path: &Path, name: &str) -> Option<PathBuf> {
    let p = path.parent()?.join(name);
    if p.is_file() { Some(p) } else { None }
}

fn validate_hook_file(path: &Path) -> anyhow::Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|_| {
        die(
            "laptop-renew-cert",
            format!("--hook missing: {}", path.display()),
        )
    })?;
    if meta.file_type().is_symlink() {
        return Err(die(
            "laptop-renew-cert",
            format!(
                "--hook must be a regular file (symlink refused): {}",
                path.display()
            ),
        ));
    }
    if !meta.is_file() {
        return Err(die(
            "laptop-renew-cert",
            format!("--hook must be a regular file: {}", path.display()),
        ));
    }
    Ok(())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn absolutize(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    }
}

struct HostProfileAcme {
    host_id: String,
    email: String,
    domains: Vec<String>,
}

fn load_host_profile_acme(path: &Path) -> anyhow::Result<HostProfileAcme> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| blocked(format!("host-profile missing: {}", path.display())))?;
    if meta.file_type().is_symlink() {
        return Err(die(
            "laptop-renew-cert",
            format!(
                "host-profile must be a regular file (symlink refused): {}",
                path.display()
            ),
        ));
    }
    let text = fs::read_to_string(path)
        .map_err(|_| blocked(format!("host-profile unreadable: {}", path.display())))?;
    let mut p = HostProfileAcme {
        host_id: String::new(),
        email: String::new(),
        domains: Vec::new(),
    };
    let mut in_domains = false;
    for raw in text.lines() {
        let line = raw.trim().trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if in_domains {
            if line == "]" {
                in_domains = false;
                continue;
            }
            let val = unquote(line.trim_end_matches(',').trim());
            if !val.is_empty() {
                p.domains.push(val);
            }
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        match key {
            "acme_domains" => {
                if val == "[" {
                    in_domains = true;
                } else if val.starts_with('[') && val.ends_with(']') {
                    for part in val[1..val.len() - 1].split(',') {
                        let part = unquote(part);
                        if !part.is_empty() {
                            p.domains.push(part);
                        }
                    }
                }
            }
            "acme_email" => p.email = unquote(val),
            "host_id" => p.host_id = unquote(val),
            _ => {}
        }
    }
    Ok(p)
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}
