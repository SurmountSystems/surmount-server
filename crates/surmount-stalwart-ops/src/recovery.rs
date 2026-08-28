use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use surmount_secrets_install::run_from_staging;
use surmount_secrets_prompt::{default_staging_dir, write_staging_item};

use crate::error::{Result, ToolError};
use crate::token::which_or;
use paths_local::{path_has_dot_segments, validate_recovery_path};

mod paths_local {
    use crate::error::{Result, ToolError};

    pub fn path_has_dot_segments(p: &str) -> bool {
        p.split('/').any(|part| part == "." || part == "..")
    }

    pub fn validate_recovery_path(p: &str) -> Result<()> {
        if p.is_empty() {
            return Err(ToolError::fail("Domain B path is empty".to_string()));
        }
        if p.chars().any(|c| c.is_control()) {
            return Err(ToolError::fail(
                "Domain B path must not contain control characters or newlines".to_string(),
            ));
        }
        if !p.starts_with('/') {
            return Err(ToolError::fail(
                "Domain B path must be absolute".to_string(),
            ));
        }
        if p.starts_with('-') {
            return Err(ToolError::fail(
                "Domain B path must not start with '-'".to_string(),
            ));
        }
        if !p
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
        {
            return Err(ToolError::fail(
                "Domain B path fails strict charset (only /[A-Za-z0-9._/-]+; no spaces or metacharacters)"
                    .to_string(),
            ));
        }
        if path_has_dot_segments(p) {
            return Err(ToolError::fail(
                "Domain B path must not contain '.' or '..' path segments".to_string(),
            ));
        }
        if p.contains("//") {
            return Err(ToolError::fail(
                "Domain B path must not contain '//'".to_string(),
            ));
        }
        let ok = p == "/var/lib/surmount/secrets"
            || p.starts_with("/var/lib/surmount/secrets/")
            || p == "/run/surmount-secrets"
            || p.starts_with("/run/surmount-secrets/");
        if !ok {
            return Err(ToolError::fail(
                "Domain B path must be under material roots /var/lib/surmount/secrets or /run/surmount-secrets"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

const KIND: &str = "stalwart-recovery-admin";
const ITEM: &str = "stalwart-recovery-admin";
const DEFAULT_PATH: &str = "/var/lib/surmount/secrets/stalwart/recovery.env";
const DROPIN: &str = "/etc/systemd/system/stalwart-mail.service.d/recovery-admin.conf";
const DROPIN_RUN: &str = "/run/systemd/system/stalwart-mail.service.d/recovery-admin.conf";
const UNIT: &str = "stalwart-mail";

const USAGE: &str = "\
stalwart-recovery-unlock - generate / install Stalwart recovery admin pin

Usage:
  stalwart-recovery-unlock [options]

Password values are never printed. Fail closed on empty material.

Options:
  --generate --reuse --install --no-install --install-only --strip
  --target --dest-root --host --staging --path --user --url --cli
  --ssh-cmd --systemctl --no-restart --no-probe --probe-only --dry-run
  -h, --help
";

fn log_line(msg: &str) {
    eprintln!("stalwart-recovery-unlock: {msg}");
}

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut host =
        std::env::var("SURMOUNT_SECRETS_HOST_ID").unwrap_or_else(|_| "surmount-1".into());
    let mut staging = String::new();
    let mut target = std::env::var("SURMOUNT_SECRETS_TARGET")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("SURMOUNT_DEPLOY_TARGET").ok())
        .unwrap_or_default();
    let mut dest_root = String::new();
    let mut do_generate = false;
    let mut do_reuse = false;
    let mut do_install = "auto";
    let mut do_strip = false;
    let mut install_only = false;
    let mut probe_only = false;
    let mut no_restart = false;
    let mut no_probe = false;
    let mut dry_run = false;
    let mut path_override = String::new();
    let mut user = std::env::var("STALWART_USER").unwrap_or_else(|_| "admin".into());
    let mut url = std::env::var("STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut cli = std::env::var("STALWART_CLI").unwrap_or_else(|_| "stalwart-cli".into());
    let mut ssh = std::env::var("SURMOUNT_RECOVERY_SSH").unwrap_or_else(|_| "ssh".into());
    let mut systemctl =
        std::env::var("SURMOUNT_RECOVERY_SYSTEMCTL").unwrap_or_else(|_| "systemctl".into());
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                println!("EnvironmentFile path only; never password in the drop-in.");
                println!("https://stalw.art/docs/configuration/recovery-mode/");
                return Ok(());
            }
            "--generate" => do_generate = true,
            "--reuse" => do_reuse = true,
            "--install" => do_install = "yes",
            "--no-install" => do_install = "no",
            "--install-only" => {
                install_only = true;
                do_install = "yes";
                do_reuse = true;
            }
            "--strip" => do_strip = true,
            "--probe-only" => probe_only = true,
            "--no-restart" => no_restart = true,
            "--no-probe" => no_probe = true,
            "--dry-run" => dry_run = true,
            "--host" => {
                i += 1;
                host = need(&argv, i)?;
            }
            "--staging" => {
                i += 1;
                staging = need(&argv, i)?;
            }
            "--target" => {
                i += 1;
                target = need(&argv, i)?;
            }
            "--dest-root" => {
                i += 1;
                dest_root = need(&argv, i)?;
            }
            "--path" => {
                i += 1;
                path_override = need(&argv, i)?;
            }
            "--user" => {
                i += 1;
                user = need(&argv, i)?;
            }
            "--url" => {
                i += 1;
                url = need(&argv, i)?;
            }
            "--cli" => {
                i += 1;
                cli = need(&argv, i)?;
            }
            "--ssh-cmd" => {
                i += 1;
                ssh = need(&argv, i)?;
            }
            "--systemctl" => {
                i += 1;
                systemctl = need(&argv, i)?;
            }
            other if other.starts_with('-') => {
                return Err(ToolError::fail(format!(
                    "unknown option: {other} (try --help)"
                )));
            }
            other => return Err(ToolError::fail(format!("unexpected argument: {other}"))),
        }
        i += 1;
    }
    let staging_path = if staging.is_empty() {
        if let Ok(s) = std::env::var("SURMOUNT_SECRETS_STAGING")
            && !s.is_empty()
        {
            PathBuf::from(s)
        } else {
            default_staging_dir()
        }
    } else {
        PathBuf::from(staging)
    };
    let domain_b = if path_override.is_empty() {
        DEFAULT_PATH.to_string()
    } else {
        path_override.clone()
    };
    validate_recovery_path(&domain_b)?;
    log_line(&format!("host={host} staging={}", staging_path.display()));
    log_line(&format!("Domain B path={domain_b} (values not logged)"));

    if do_strip {
        if do_install == "yes" || install_only {
            return Err(ToolError::fail(
                "use either --strip or --install, not both".to_string(),
            ));
        }
        if probe_only {
            return Err(ToolError::fail(
                "use either --strip or --probe-only, not both".to_string(),
            ));
        }
        if do_generate {
            return Err(ToolError::fail(
                "use either --strip or --generate, not both".to_string(),
            ));
        }
        if target.is_empty() && dest_root.is_empty() {
            return Err(ToolError::fail(
                "--strip requires --target USER@HOST or --dest-root DIR".to_string(),
            ));
        }
        return run_strip(
            &dest_root, &target, &domain_b, &ssh, &systemctl, no_restart, dry_run,
        );
    }

    if do_install == "auto" {
        do_install = if !target.is_empty() || !dest_root.is_empty() {
            "yes"
        } else {
            "no"
        };
    }
    if do_install == "yes" && target.is_empty() && dest_root.is_empty() {
        return Err(ToolError::fail(
            "--install requires --target USER@HOST or --dest-root DIR".to_string(),
        ));
    }

    if probe_only {
        return run_probe_only(
            &staging_path,
            &dest_root,
            &target,
            &domain_b,
            &user,
            &url,
            &cli,
            &ssh,
            dry_run,
        );
    }

    if install_only {
        if !staging_has(&staging_path) {
            return Err(ToolError::fail(format!(
                "install-only: missing Domain A staging under {}/{ITEM}",
                staging_path.display()
            )));
        }
        log_line("install-only: using existing Domain A staging (values not logged)");
    } else if do_reuse {
        if staging_has(&staging_path) {
            log_line(
                "reuse: Domain A staging already non-empty (values not logged); skip generate",
            );
        } else {
            return Err(ToolError::fail(format!(
                "reuse requested but Domain A staging empty under {}/{ITEM} (pass --generate)",
                staging_path.display()
            )));
        }
    } else if do_generate
        || (do_install == "yes" && !staging_has(&staging_path))
        || !staging_has(&staging_path)
    {
        if dry_run {
            log_line(&format!(
                "dry-run: would generate Domain A kind={KIND} host={host} path={domain_b}"
            ));
        } else {
            log_line("generating high-entropy recovery password (value not logged)");
            let pw = generate_password()?;
            write_staging(&staging_path, &host, &domain_b, &user, &pw)?;
        }
    } else {
        log_line(
            "Domain A staging already present (values not logged); pass --generate to replace",
        );
    }

    if do_install != "yes" {
        eprintln!(
            "\nNext steps (secret values not shown):\n  Domain A staging: {}/{ITEM}\n  Durable Domain B: {domain_b}\n",
            staging_path.display()
        );
        return Ok(());
    }

    if dry_run {
        log_line(&format!(
            "dry-run: would install kind={KIND} path={domain_b}"
        ));
        if !dest_root.is_empty() {
            log_line(&format!(
                "dry-run: would materialize under dest-root={dest_root} (durable drop-in {DROPIN})"
            ));
        }
        if !no_restart {
            log_line(&format!(
                "dry-run: would systemctl daemon-reload + restart {UNIT}"
            ));
        }
        if !no_probe {
            log_line(&format!(
                "dry-run: would probe Basic auth url={url} (password not logged)"
            ));
        }
        return Ok(());
    }

    if !staging_has(&staging_path) {
        return Err(ToolError::fail(format!(
            "Domain A staging missing under {}/{ITEM} after generate/reuse",
            staging_path.display()
        )));
    }

    // Isolated staging: only this kind.
    let iso =
        std::env::temp_dir().join(format!("surmount-recovery-install.{}", std::process::id()));
    fs::create_dir_all(iso.join(ITEM))?;
    let _ = set_mode(&iso, 0o700);
    fs::copy(
        staging_path.join(ITEM).join("attributes"),
        iso.join(ITEM).join("attributes"),
    )?;
    fs::copy(
        staging_path.join(ITEM).join("secret"),
        iso.join(ITEM).join("secret"),
    )?;
    let _ = set_mode(&iso.join(ITEM).join("secret"), 0o600);
    log_line(
        "installing Domain B kind=stalwart-recovery-admin only (isolated staging; values not logged)",
    );
    let inst = run_from_staging(
        &iso,
        &host,
        if dest_root.is_empty() {
            None
        } else {
            Some(Path::new(&dest_root))
        },
        if dest_root.is_empty() && !target.is_empty() {
            Some(target.as_str())
        } else {
            None
        },
        Some(KIND),
        false,
    );
    let _ = fs::remove_dir_all(&iso);
    inst.map_err(|e| {
        ToolError::fail(format!(
            "secrets-install-host failed (Domain B not installed). Secret values not logged. {e}"
        ))
    })?;
    log_line("Domain B recovery.env install finished (value not logged)");

    let body = format!("[Service]\nEnvironmentFile=-{domain_b}\n");
    if body.contains("STALWART_RECOVERY_ADMIN=") {
        return Err(ToolError::fail(
            "internal: drop-in must not contain password material".to_string(),
        ));
    }
    if !dest_root.is_empty() {
        if unit_has_envfile(&systemctl, &domain_b) {
            log_line(&format!(
                "unit already declares EnvironmentFile for {domain_b}; skip drop-in (prefer durable unit)"
            ));
        } else {
            let abs = format!("{dest_root}{DROPIN}");
            if let Some(p) = Path::new(&abs).parent() {
                fs::create_dir_all(p)?;
            }
            fs::write(&abs, &body)?;
            let _ = set_mode(Path::new(&abs), 0o644);
            if fs::read_to_string(&abs)
                .unwrap_or_default()
                .contains("STALWART_RECOVERY_ADMIN=")
            {
                return Err(ToolError::fail(
                    "internal: drop-in must not contain password material".to_string(),
                ));
            }
            log_line(&format!(
                "wrote durable systemd drop-in at {abs} (EnvironmentFile path only; no password)"
            ));
        }
        if !no_restart {
            if Path::new(&systemctl).is_file() || which_or(&systemctl).is_ok() {
                log_line(&format!("daemon-reload + restart {UNIT} via {systemctl}"));
                let st = Command::new(&systemctl).arg("daemon-reload").status()?;
                if !st.success() {
                    return Err(ToolError::fail(
                        "systemctl daemon-reload failed".to_string(),
                    ));
                }
                let st = Command::new(&systemctl).args(["restart", UNIT]).status()?;
                if !st.success() {
                    return Err(ToolError::fail(format!("systemctl restart {UNIT} failed")));
                }
            } else {
                log_line(&format!(
                    "note: systemctl not found ({systemctl}); skip restart (pass --systemctl for tests)"
                ));
            }
        } else {
            log_line("skip restart (--no-restart)");
        }
    }

    if !no_probe {
        let body = read_staging_body(&staging_path)?;
        let pw = extract_password(&body)?;
        let u = extract_user(&body).unwrap_or_else(|| user.clone());
        probe_auth(&pw, &u, &url, &cli)?;
    } else {
        log_line("skip probe (--no-probe)");
    }
    eprintln!(
        "\nNext steps (secret values not shown):\n  Domain A staging: {}/{ITEM}\n  Durable Domain B: {domain_b}\n",
        staging_path.display()
    );
    let _ = path_has_dot_segments;
    Ok(())
}

fn run_strip(
    dest_root: &str,
    target: &str,
    domain_b: &str,
    ssh: &str,
    systemctl: &str,
    no_restart: bool,
    dry_run: bool,
) -> Result<()> {
    log_line("strip: remove recovery drop-in(s) + Domain B env (password not logged)");
    let paths = [DROPIN, DROPIN_RUN, domain_b];
    if dry_run {
        log_line("dry-run: would remove:");
        for p in paths {
            log_line(&format!("dry-run:   {p}"));
        }
        if !dest_root.is_empty() {
            log_line(&format!("dry-run: under dest-root={dest_root}"));
        }
        if !target.is_empty() && dest_root.is_empty() {
            log_line(&format!("dry-run: over SSH target={target}"));
        }
        if no_restart {
            log_line("dry-run: would skip restart (--no-restart)");
        } else {
            log_line(&format!(
                "dry-run: would systemctl daemon-reload + restart {UNIT}"
            ));
        }
        log_line("dry-run: note: Domain A staging is not removed (host strip only;");
        return Ok(());
    }
    if !dest_root.is_empty() {
        for p in paths {
            let abs = format!("{dest_root}{p}");
            if Path::new(&abs).exists() || Path::new(&abs).symlink_metadata().is_ok() {
                let _ = fs::remove_file(&abs);
                log_line(&format!("strip: removed {abs}"));
            } else {
                log_line(&format!("strip: absent (ok) {abs}"));
            }
        }
        let _ = fs::remove_dir(format!("{dest_root}/etc/systemd/system/{UNIT}.service.d"));
        let _ = fs::remove_dir(format!("{dest_root}/run/systemd/system/{UNIT}.service.d"));
        if !no_restart {
            if Path::new(systemctl).is_file() || which_or(systemctl).is_ok() {
                log_line(&format!(
                    "strip: daemon-reload + restart {UNIT} via {systemctl}"
                ));
                Command::new(systemctl).arg("daemon-reload").status()?;
                Command::new(systemctl).args(["restart", UNIT]).status()?;
            }
        } else {
            log_line(
                "strip: skip restart (--no-restart); process may keep STALWART_RECOVERY_ADMIN until restart",
            );
        }
        log_line("strip finished under dest-root (values not logged)");
        log_line("note: Domain A staging was not removed (host strip only)");
        return Ok(());
    }
    if !target.is_empty() {
        let script = format!(
            "set -euo pipefail\nfor p in {} {} {}; do\n  if [ -e \"$p\" ] || [ -L \"$p\" ]; then rm -f \"$p\"; echo removed \"$p\" >&2; else echo absent \"$p\" >&2; fi\ndone\n",
            sh_quote(DROPIN),
            sh_quote(DROPIN_RUN),
            sh_quote(domain_b)
        );
        let mut c = Command::new(ssh);
        c.args(["--", target, "bash", "-s"]);
        c.stdin(std::process::Stdio::piped());
        let mut child = c.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(script.as_bytes())?;
        }
        let st = child.wait()?;
        if !st.success() {
            return Err(ToolError::fail(format!(
                "remote strip failed (rc={}). Secret values not logged.",
                st.code().unwrap_or(1)
            )));
        }
        log_line("remote strip finished");
        return Ok(());
    }
    Err(ToolError::fail(
        "internal: strip without target or dest-root".to_string(),
    ))
}

fn run_probe_only(
    staging: &Path,
    dest_root: &str,
    target: &str,
    domain_b: &str,
    user: &str,
    url: &str,
    cli: &str,
    _ssh: &str,
    dry_run: bool,
) -> Result<()> {
    let body = if !dest_root.is_empty() {
        let env_file = format!("{dest_root}{domain_b}");
        if !Path::new(&env_file).is_file() {
            log_line(&format!(
                "probe BLOCKED: recovery env missing at {env_file}"
            ));
            return Err(ToolError::blocked("no recovery env material".to_string()));
        }
        fs::read_to_string(&env_file)?
            .lines()
            .next()
            .unwrap_or("")
            .trim_end_matches('\r')
            .to_string()
    } else if staging_has(staging) {
        read_staging_body(staging)?
    } else if !target.is_empty() {
        return Err(ToolError::blocked(format!(
            "could not read Domain B recovery env on target (path={domain_b})"
        )));
    } else {
        return Err(ToolError::fail(
            "probe-only needs --dest-root, Domain A staging, or --target".to_string(),
        ));
    };
    if body.is_empty() {
        log_line("probe BLOCKED: no recovery env material");
        return Err(ToolError::blocked("no recovery env material".to_string()));
    }
    let pw = extract_password(&body)?;
    let u = extract_user(&body).unwrap_or_else(|| user.to_string());
    if dry_run {
        log_line(&format!(
            "dry-run: would probe Basic auth url={url} user={u} (password not logged)"
        ));
        return Ok(());
    }
    probe_auth(&pw, &u, url, cli)
}

fn probe_auth(password: &str, user: &str, url: &str, cli: &str) -> Result<()> {
    log_line(&format!(
        "probe: local Basic auth (url={url} user={user}; password not logged)"
    ));
    let cli = which_or(cli)?;
    let try_cmd = |args: &[&str]| {
        Command::new(&cli)
            .args(["--url", url, "--user", user])
            .args(args)
            .env("STALWART_PASSWORD", password)
            .env("STALWART_USER", user)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    if try_cmd(&["query", "domain"]) || try_cmd(&["query", "account"]) || try_cmd(&["describe"]) {
        log_line("probe ok: Basic auth accepted (password not logged)");
        Ok(())
    } else {
        Err(ToolError::fail(
            "Basic auth probe failed. Recovery pin may not be loaded; check unit EnvironmentFile and restart. Secret values not logged.".to_string(),
        ))
    }
}

fn staging_has(staging: &Path) -> bool {
    let secret = staging.join(ITEM).join("secret");
    let attrs = staging.join(ITEM).join("attributes");
    secret.is_file()
        && attrs.is_file()
        && fs::read_to_string(&secret)
            .ok()
            .is_some_and(|s| s.chars().any(|c| !c.is_whitespace()))
}

fn write_staging(staging: &Path, host: &str, path: &str, user: &str, password: &str) -> Result<()> {
    if user.contains(':') || user.contains('\n') {
        return Err(ToolError::fail(
            "recovery user must not contain colon or newline".to_string(),
        ));
    }
    let body = format!("STALWART_RECOVERY_ADMIN={user}:{password}\n");
    let attrs = format!("surmount.kind={KIND}\nsurmount.host={host}\nsurmount.path={path}\n");
    write_staging_item(staging, ITEM, &attrs, &body).map_err(|e| ToolError::fail(e.to_string()))?;
    log_line(&format!(
        "wrote Domain A staging item={ITEM} host={host} path={path} (value not logged)"
    ));
    Ok(())
}

fn generate_password() -> Result<String> {
    use std::io::Read;
    let mut buf = [0u8; 32];
    // Bounded read. fs::read("/dev/urandom") never EOFs and hangs the Nix check.
    let mut f = fs::File::open("/dev/urandom").map_err(|_| {
        ToolError::fail("failed to generate high-entropy password (need /dev/urandom)".to_string())
    })?;
    f.read_exact(&mut buf).map_err(|_| {
        ToolError::fail("failed to generate high-entropy password (need /dev/urandom)".to_string())
    })?;
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let pw: String = buf
        .iter()
        .map(|b| alphabet[(*b as usize) % alphabet.len()] as char)
        .collect();
    if pw.len() < 24 || pw.contains(':') {
        return Err(ToolError::fail(
            "failed to generate high-entropy password (need /dev/urandom)".to_string(),
        ));
    }
    Ok(pw)
}

fn read_staging_body(staging: &Path) -> Result<String> {
    let p = staging.join(ITEM).join("secret");
    Ok(fs::read_to_string(p)?
        .lines()
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string())
}

fn extract_password(body: &str) -> Result<String> {
    let line = body
        .lines()
        .find(|l| l.trim_start().starts_with("STALWART_RECOVERY_ADMIN="))
        .ok_or_else(|| {
            ToolError::fail(
                "probe: could not parse STALWART_RECOVERY_ADMIN from material".to_string(),
            )
        })?;
    let rest = line.split_once('=').map(|x| x.1).unwrap_or("").trim();
    let pw = rest.split_once(':').map(|x| x.1).unwrap_or("");
    if pw.is_empty() {
        return Err(ToolError::fail(
            "probe: could not parse password".to_string(),
        ));
    }
    Ok(pw.to_string())
}

fn extract_user(body: &str) -> Option<String> {
    let line = body
        .lines()
        .find(|l| l.trim_start().starts_with("STALWART_RECOVERY_ADMIN="))?;
    let rest = line.split_once('=')?.1.trim();
    Some(rest.split_once(':')?.0.to_string())
}

fn unit_has_envfile(systemctl: &str, env_path: &str) -> bool {
    let out = Command::new(systemctl)
        .args(["show", UNIT, "-p", "EnvironmentFiles", "--value"])
        .output()
        .ok();
    out.map(|o| String::from_utf8_lossy(&o.stdout).contains(env_path))
        .unwrap_or(false)
}

fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn need(argv: &[String], i: usize) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail("flag needs a value".to_string()))
}
