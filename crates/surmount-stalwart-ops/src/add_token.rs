use std::fs;
use std::io::{self, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use surmount_secrets_install::run_from_staging;
use surmount_secrets_prompt::{
    Kind, build_attributes, build_single_line_secret, default_staging_dir, write_staging_item,
};

use crate::error::{Result, ToolError};

const DEFAULT_PATH: &str = "/var/lib/surmount/secrets/ui/stalwart-api-token";

const USAGE: &str = "\
add-stalwart-token - Domain A Stalwart admin credential + optional Domain B install

Usage:
  add-stalwart-token [options]

Paste a credential Stalwart already accepts (first-boot admin or known API
token). Do NOT invent a random token. Random generate is not engine registration.
Live free-443 HTTP 401 means the value in Domain B is wrong or unknown.

Options:
  --host ID              Surmount inventory name (default: surmount-1)
  --staging DIR          Domain A staging root
  --target USER@HOST     SSH target for Domain B install
  --dest-root DIR        Hermetic local install root
  --token-file PATH      Read token from file (prefer mode 0600)
  --install              After Domain A is filled, install
  --no-install           Stage only
  --install-only         Skip intake; install existing staging item only
  --force-reprompt       Re-read credential even if Domain A staging exists
  --replace              Alias for --force-reprompt
  --dry-run-install      Pass --dry-run to secrets-install-host
  --path PATH            Override Domain B surmount.path
  -h, --help             Show this help.

  just free-stalwart-public-443 -- --dry-run
";

fn log_line(msg: &str) {
    eprintln!("add-stalwart-token: {msg}");
}

fn staging_has_token(staging: &Path) -> bool {
    let secret = staging.join("stalwart-token/secret");
    let attrs = staging.join("stalwart-token/attributes");
    if !secret.is_file() || !attrs.is_file() {
        return false;
    }
    fs::read_to_string(&secret)
        .ok()
        .is_some_and(|s| s.chars().any(|c| !c.is_whitespace()))
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
    let mut do_install = "auto";
    let mut install_only = false;
    let mut force = false;
    let mut dry_run_install = false;
    let mut path_override = String::new();
    let mut token_file = String::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                println!("default host surmount-1. token-file. Domain A. printf '%s'");
                return Ok(());
            }
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
            "--token-file" => {
                i += 1;
                token_file = need(&argv, i)?;
            }
            "--install" => do_install = "yes",
            "--no-install" => do_install = "no",
            "--install-only" => {
                install_only = true;
                do_install = "yes";
            }
            "--force-reprompt" | "--replace" => force = true,
            "--dry-run-install" => dry_run_install = true,
            "--secret-service" => {}
            "--path" => {
                i += 1;
                path_override = need(&argv, i)?;
            }
            other if other.starts_with('-') => {
                return Err(ToolError::fail(format!(
                    "unknown option: {other} (try --help)"
                )));
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unexpected argument: {other} (try --help)"
                )));
            }
        }
        i += 1;
    }
    if host.is_empty() || host.starts_with('-') {
        return Err(ToolError::fail(
            "host id is empty or option-shaped".to_string(),
        ));
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
    if do_install == "auto" {
        do_install = if !target.is_empty() || !dest_root.is_empty() {
            "yes"
        } else {
            "no"
        };
    }
    if do_install == "yes" && target.is_empty() && dest_root.is_empty() {
        return Err(ToolError::fail(
            "--install requires --target USER@HOST or --dest-root DIR (or set SURMOUNT_SECRETS_TARGET)".to_string(),
        ));
    }
    if install_only && force {
        return Err(ToolError::fail(
            "use either --install-only or --force-reprompt/--replace, not both".to_string(),
        ));
    }
    let has_dest = !target.is_empty() || !dest_root.is_empty();
    log_line(&format!("host={host} staging={}", staging_path.display()));
    log_line(&format!("Domain B path default={DEFAULT_PATH}"));

    let stdin_tty = atty0();
    if install_only {
        if !staging_has_token(&staging_path) {
            return Err(ToolError::fail(format!(
                "install-only: missing staging item under {}/stalwart-token (fill Domain A first, or pipe / --token-file)",
                staging_path.display()
            )));
        }
        log_line("install-only; using existing Domain A staging (values not logged)");
    } else if !force && staging_has_token(&staging_path) && has_dest {
        log_line("using existing Domain A staging (values not logged); install will follow");
    } else if !token_file.is_empty() {
        let tok = read_token_file(&token_file)?;
        write_token(&staging_path, &host, &path_override, &tok)?;
    } else if !stdin_tty {
        if force || !staging_has_token(&staging_path) {
            let tok = read_token_stdin()?;
            if tok.trim().is_empty() {
                return need_help(&host);
            }
            write_token(&staging_path, &host, &path_override, &tok)?;
        } else {
            log_line("using existing Domain A staging (values not logged)");
        }
    } else if staging_has_token(&staging_path) && !force && !has_dest {
        log_line("using existing Domain A staging (values not logged); pass --replace to re-paste");
    } else {
        return Err(ToolError::fail(
            "interactive Domain A paste requires surmount-secrets-prompt on a TTY; use --token-file or a pipe".to_string(),
        ));
    }

    if !staging_has_token(&staging_path) {
        if has_dest {
            return need_help(&host);
        }
        return Err(ToolError::fail(format!(
            "Domain A staging missing under {}/stalwart-token after intake",
            staging_path.display()
        )));
    }

    if do_install == "yes" {
        if dry_run_install {
            log_line("dry-run install (no host mutation)");
        } else {
            log_line("installing Domain B (values not logged)");
        }
        run_from_staging(
            &staging_path,
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
            Some("stalwart-token"),
            dry_run_install,
        )
        .map_err(|e| {
            ToolError::fail(format!(
                "secrets-install-host failed (Domain B not installed): {e}"
            ))
        })?;
        if !dry_run_install {
            log_line("Domain B install finished (stalwart-token).");
        }
    } else {
        log_line("skip install (no --target/--dest-root; or --no-install).");
    }
    eprintln!(
        "\nNext steps (secret values not shown):\n  Domain A staging: {}/stalwart-token\n  Durable Domain B: {DEFAULT_PATH}\n  just free-stalwart-public-443 -- --dry-run\n",
        staging_path.display()
    );
    Ok(())
}

fn need_help(host: &str) -> Result<()> {
    eprintln!(
        "add-stalwart-token: no Domain A staging for host={host} and no credential on stdin/--token-file.\n\
Fill Domain A then install Domain B (values never logged):\n\
  printf '%s\\n' 'REAL_TOKEN' | just add-stalwart-token -- --host {host} --target root@HOST\n"
    );
    Err(ToolError::fail("missing credential".to_string()))
}

fn write_token(staging: &Path, host: &str, path_override: &str, token: &str) -> Result<()> {
    let path = if path_override.is_empty() {
        DEFAULT_PATH
    } else {
        path_override
    };
    let secret = build_single_line_secret(token).map_err(|e| ToolError::fail(e.to_string()))?;
    let attrs = build_attributes(Kind::StalwartToken, host, path)
        .map_err(|e| ToolError::fail(e.to_string()))?;
    write_staging_item(staging, "stalwart-token", &attrs, &secret)
        .map_err(|e| ToolError::fail(e.to_string()))?;
    log_line(&format!(
        "wrote Domain A staging item=stalwart-token host={host} (value not logged)"
    ));
    Ok(())
}

fn read_token_file(path: &str) -> Result<String> {
    let p = Path::new(path);
    if !p.is_file() {
        return Err(ToolError::fail(format!("token-file not found: {path}")));
    }
    let meta = fs::metadata(p)?;
    let mode = meta.permissions().mode() & 0o007;
    if mode != 0 {
        return Err(ToolError::fail(format!(
            "token-file is world-accessible (mode {:o}); chmod 0600 and retry: {path}",
            meta.permissions().mode() & 0o777
        )));
    }
    let s = fs::read_to_string(p)?;
    Ok(s.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string())
}

fn read_token_stdin() -> Result<String> {
    let mut s = String::new();
    io::stdin().read_to_string(&mut s)?;
    Ok(s.lines()
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string())
}

fn atty0() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        unsafe extern "C" {
            fn isatty(fd: i32) -> i32;
        }
        let fd = io::stdin().as_raw_fd();
        // SAFETY: isatty on the live stdin fd.
        unsafe { isatty(fd) != 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn need(argv: &[String], i: usize) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail("flag needs a value".to_string()))
}
