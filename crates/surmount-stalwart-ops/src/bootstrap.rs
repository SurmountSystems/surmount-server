use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::add_token;
use crate::error::{Result, ToolError};
use crate::json::{account_query_has_admin, extract_created_id, extract_secret_line, find_domain_id};
use crate::token::which_or;

const USAGE: &str = "\
bootstrap-stalwart-api-token - mint Stalwart API key from admin password

Usage:
  bootstrap-stalwart-api-token [options]

Never invents a password. Never logs secret values (token or password).
Empty directory: ensure Domain + permanent Admin Account before ApiKey.
Compose order: recovery unlock -> Domain -> permanent Admin -> ApiKey -> free-443.

Options:
  --url --user --domain --password-file --cli --description --host --staging
  --target --dest-root --install --no-install --dry-run-install
  --free-443-dry-run --skip-ensure-directory --ssh-cmd --path
  -h, --help
";

fn log_line(msg: &str) {
    eprintln!("bootstrap-stalwart-api-token: {msg}");
}

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut host = std::env::var("SURMOUNT_SECRETS_HOST_ID").unwrap_or_else(|_| "surmount-1".into());
    let mut staging = String::new();
    let mut target = std::env::var("SURMOUNT_SECRETS_TARGET")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("SURMOUNT_DEPLOY_TARGET").ok())
        .unwrap_or_default();
    let mut dest_root = String::new();
    let mut do_install = "auto";
    let mut dry_run_install = false;
    let mut free_443 = false;
    let mut ensure = true;
    let mut path_override = String::new();
    let mut password_file = String::new();
    let mut url = std::env::var("STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut user = std::env::var("STALWART_USER").unwrap_or_else(|_| "admin".into());
    let mut domain = std::env::var("STALWART_DOMAIN").unwrap_or_else(|_| "surmount.systems".into());
    let mut cli = std::env::var("STALWART_CLI").unwrap_or_else(|_| "stalwart-cli".into());
    let mut description = "surmount free-443".to_string();
    let mut ssh_cmd = std::env::var("SURMOUNT_BOOTSTRAP_SSH").unwrap_or_else(|_| "ssh".into());
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                println!("ensure directory: Domain + permanent Admin before ApiKey (idempotent)");
                println!("https://stalw.art/docs/management/cli/create/");
                return Ok(());
            }
            "--url" => {
                i += 1;
                url = need(&argv, i)?;
            }
            "--user" => {
                i += 1;
                user = need(&argv, i)?;
            }
            "--domain" => {
                i += 1;
                domain = need(&argv, i)?;
            }
            "--password-file" => {
                i += 1;
                password_file = need(&argv, i)?;
            }
            "--cli" => {
                i += 1;
                cli = need(&argv, i)?;
            }
            "--description" => {
                i += 1;
                description = need(&argv, i)?;
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
            "--install" => do_install = "yes",
            "--no-install" => do_install = "no",
            "--dry-run-install" => dry_run_install = true,
            "--free-443-dry-run" => free_443 = true,
            "--skip-ensure-directory" => ensure = false,
            "--ssh-cmd" => {
                i += 1;
                ssh_cmd = need(&argv, i)?;
            }
            "--path" => {
                i += 1;
                path_override = need(&argv, i)?;
            }
            other if other.starts_with('-') => {
                return Err(ToolError::fail(format!("unknown option: {other} (try --help)")));
            }
            other => return Err(ToolError::fail(format!("unexpected argument: {other}"))),
        }
        i += 1;
    }
    if domain.contains('/') || domain.contains(' ') || domain.starts_with('-') || domain.contains('@')
    {
        return Err(ToolError::fail(
            "domain must be a bare hostname (got invalid shape; values not logged)".to_string(),
        ));
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
    if free_443 && do_install != "yes" {
        return Err(ToolError::fail(
            "--free-443-dry-run requires Domain B install (--target or --dest-root; not --no-install)"
                .to_string(),
        ));
    }

    let password = if !password_file.is_empty() {
        log_line("reading admin password from --password-file (value not logged)");
        read_password_file(&password_file)?
    } else if std::env::var("STALWART_PASSWORD").is_ok() {
        log_line("using STALWART_PASSWORD env (value not logged)");
        std::env::var("STALWART_PASSWORD").unwrap_or_default()
    } else {
        return Err(ToolError::fail(
            "no password: set STALWART_PASSWORD, --password-file, or run on a TTY".to_string(),
        ));
    };
    if password.trim().is_empty() {
        return Err(ToolError::fail(
            "password is empty; refuse to call Stalwart (set --password-file, STALWART_PASSWORD, or type on TTY)"
                .to_string(),
        ));
    }

    log_line(&format!("host={host} staging={staging}"));
    log_line(&format!(
        "url={url} user={user} domain={domain} (password not logged)"
    ));
    if ensure {
        log_line("ensure directory: Domain + permanent Admin before ApiKey (idempotent)");
    } else {
        log_line("ensure directory: skipped (--skip-ensure-directory)");
    }

    let cli_bin = if target.is_empty() {
        which_or(&cli)?
    } else {
        cli.clone()
    };

    let mut apikey_user = user.clone();
    let create_out = if !target.is_empty() {
        log_line(&format!(
            "create mode=remote target={target} (loopback create on host; values not logged)"
        ));
        remote_create(&ssh_cmd, &target, &password, &user, &url, &cli, &description, &domain, ensure)?
    } else {
        log_line(&format!("create mode=local cli={cli_bin}"));
        if ensure {
            apikey_user = ensure_local(&cli_bin, &url, &user, &domain, &password)?;
        }
        local_create(&cli_bin, &url, &apikey_user, &password, &description)?
    };
    if create_out.1 != 0 {
        log_line(&format!(
            "stalwart-cli create failed (rc={}); engine did not mint a key",
            create_out.1
        ));
        let safe = scrub(&create_out.0, &password);
        if !safe.is_empty() {
            eprintln!("{safe}");
        }
        return Err(ToolError::fail(format!(
            "create apikey failed (rc={}). Check admin password, URL, empty-directory ensure (Domain + permanent Admin), and that management listens on the URL. Secret values not logged.",
            create_out.1
        )));
    }
    let token = extract_secret_line(&create_out.0).ok_or_else(|| {
        ToolError::fail(
            "create succeeded but no Secret: line found in CLI output; refuse to invent a token. Capture Secret from create output is required (one-time).".to_string(),
        )
    })?;
    log_line("captured one-time API key from create (value not logged); staging as stalwart-token");

    let tmp = std::env::temp_dir().join(format!(
        "bootstrap-stalwart-token.{}",
        std::process::id()
    ));
    fs::write(&tmp, &token)?;
    let mut perm = fs::metadata(&tmp)?.permissions();
    perm.set_mode(0o600);
    fs::set_permissions(&tmp, perm)?;

    let mut add_args = vec![
        "--host".into(),
        host.clone(),
        "--token-file".into(),
        tmp.display().to_string(),
        "--force-reprompt".into(),
    ];
    if !staging.is_empty() {
        add_args.push("--staging".into());
        add_args.push(staging.clone());
    }
    if !path_override.is_empty() {
        add_args.push("--path".into());
        add_args.push(path_override.clone());
    }
    if do_install == "yes" {
        if !dest_root.is_empty() {
            add_args.push("--dest-root".into());
            add_args.push(dest_root.clone());
        } else if !target.is_empty() {
            add_args.push("--target".into());
            add_args.push(target.clone());
        }
        if dry_run_install {
            add_args.push("--dry-run-install".into());
        }
        log_line("staging Domain A and installing Domain B (values not logged)");
    } else {
        add_args.push("--no-install".into());
        log_line("staging Domain A only (--no-install or no --target/--dest-root)");
    }
    let add_res = add_token::run(add_args);
    let _ = fs::remove_file(&tmp);
    add_res.map_err(|e| {
        ToolError::fail(format!(
            "add-stalwart-token failed after mint (Domain A/B not updated). Secret values not logged. {e}"
        ))
    })?;
    log_line("API key minted and staged as kind stalwart-token (value not logged)");

    if free_443 && !dry_run_install {
        let mut free_token = path_override.clone();
        if free_token.is_empty() {
            free_token = "/var/lib/surmount/secrets/ui/stalwart-api-token".into();
        }
        if !dest_root.is_empty() {
            free_token = format!(
                "{}{}",
                dest_root.trim_end_matches('/'),
                if free_token.starts_with('/') {
                    free_token
                } else {
                    format!("/{free_token}")
                }
            );
        }
        log_line(&format!(
            "free-443 dry-run: local token-file={free_token} (value not logged)"
        ));
        let r = crate::free443::run([
            "--dry-run".to_string(),
            "--skip-ss-check".to_string(),
            "--token-file".to_string(),
            free_token,
            "--cli".to_string(),
            cli_bin,
        ]);
        match r {
            Ok(()) => log_line("free-443 dry-run ok (not live free-443 green)"),
            Err(e) if e.exit_code == 2 => {
                log_line("free-443 dry-run BLOCKED (no token material). Secret values not logged.");
                return Err(ToolError::blocked(e.message));
            }
            Err(e) => {
                return Err(ToolError::fail(format!(
                    "free-443 dry-run failed (rc={}). Not claiming free-443 green. Secret values not logged.",
                    e.exit_code
                )));
            }
        }
    }
    Ok(())
}

fn ensure_local(
    cli: &str,
    url: &str,
    user: &str,
    domain: &str,
    password: &str,
) -> Result<String> {
    log_line(&format!("ensure directory: query domain name={domain}"));
    let q = Command::new(cli)
        .args(["--url", url, "--user", user, "query", "domain", "--fields", "id,name", "--json"])
        .env("STALWART_PASSWORD", password)
        .env("STALWART_USER", user)
        .output()?;
    let q_out = String::from_utf8_lossy(&q.stdout).into_owned()
        + &String::from_utf8_lossy(&q.stderr);
    if !q.status.success() {
        return Err(ToolError::fail(
            "ensure directory: query domain failed. Check admin password/URL. Secret values not logged."
                .to_string(),
        ));
    }
    let mut domain_id = find_domain_id(&q_out, domain);
    if domain_id.is_none() {
        log_line(&format!("ensure directory: creating Domain name={domain}"));
        let c = Command::new(cli)
            .args([
                "--url",
                url,
                "--user",
                user,
                "create",
                "domain",
                "--field",
                &format!("name={domain}"),
                "--field",
                "isEnabled=true",
                "--field",
                "description=Surmount primary",
            ])
            .env("STALWART_PASSWORD", password)
            .output()?;
        if !c.status.success() {
            return Err(ToolError::fail(
                "ensure directory: create domain failed. Secret values not logged.".to_string(),
            ));
        }
        let text = String::from_utf8_lossy(&c.stdout);
        domain_id = extract_created_id(&text, "Domain");
        if domain_id.is_none() {
            return Err(ToolError::fail(
                "ensure directory: create domain succeeded but no Created Domain id in CLI output"
                    .to_string(),
            ));
        }
    } else {
        log_line("ensure directory: domain already present");
    }
    let domain_id = domain_id.unwrap_or_default();
    let q = Command::new(cli)
        .args([
            "--url", url, "--user", user, "query", "account", "--fields", "id,name,emailAddress,roles",
            "--json",
        ])
        .env("STALWART_PASSWORD", password)
        .output()?;
    let q_out = String::from_utf8_lossy(&q.stdout).into_owned();
    if !q.status.success() {
        return Err(ToolError::fail(
            "ensure directory: query account failed. Secret values not logged.".to_string(),
        ));
    }
    if account_query_has_admin(&q_out, user, domain) {
        log_line("ensure directory: permanent admin already present (ok)");
    } else {
        log_line(&format!(
            "ensure directory: creating permanent Admin Account {user}@{domain} (password not logged)"
        ));
        let json = format!(
            "{{\"@type\":\"User\",\"name\":\"{}\",\"domainId\":\"{}\",\"credentials\":{{\"0\":{{\"@type\":\"Password\",\"secret\":\"{}\"}}}},\"memberGroupIds\":{{}},\"roles\":{{\"@type\":\"Admin\"}},\"permissions\":{{\"@type\":\"Inherit\"}},\"quotas\":{{}},\"aliases\":{{}},\"encryptionAtRest\":{{\"@type\":\"Disabled\"}}}}",
            json_escape(user),
            json_escape(&domain_id),
            json_escape(password)
        );
        let mut child = Command::new(cli)
            .args(["--url", url, "--user", user, "create", "Account/User", "--stdin"])
            .env("STALWART_PASSWORD", password)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(json.as_bytes())?;
        }
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(ToolError::fail(
                "ensure directory: create permanent Admin Account failed. Secret values not logged."
                    .to_string(),
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        if extract_created_id(&text, "Account").is_none()
            && !text.to_ascii_lowercase().contains("created account")
        {
            return Err(ToolError::fail(
                "ensure directory: create permanent Admin succeeded but no Created Account line"
                    .to_string(),
            ));
        }
        log_line("ensure directory: permanent Admin Account created");
    }
    let apikey_user = format!("{user}@{domain}");
    log_line(&format!(
        "ensure directory: mint ApiKey as permanent admin user={apikey_user} (password not logged)"
    ));
    Ok(apikey_user)
}

fn local_create(cli: &str, url: &str, user: &str, password: &str, desc: &str) -> Result<(String, i32)> {
    let out = Command::new(cli)
        .args([
            "--url",
            url,
            "--user",
            user,
            "create",
            "apikey",
            "--field",
            &format!("description={desc}"),
            "--field",
            "permissions={\"@type\":\"Inherit\"}",
            "--field",
            "allowedIps={}",
        ])
        .env("STALWART_PASSWORD", password)
        .env("STALWART_USER", user)
        .output()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned()
        + &String::from_utf8_lossy(&out.stderr);
    Ok((text, out.status.code().unwrap_or(1)))
}

fn remote_create(
    ssh: &str,
    target: &str,
    password: &str,
    user: &str,
    url: &str,
    cli: &str,
    desc: &str,
    domain: &str,
    ensure: bool,
) -> Result<(String, i32)> {
    // All remote inputs via quoted script body. Never unquoted KEY=value on ssh argv.
    let script = format!(
        "set -euo pipefail\nexport STALWART_PASSWORD={}\nexport STALWART_USER={}\nURL={}\nCLI={}\nDESC={}\nDOMAIN={}\nENSURE={}\n\
if [ -z \"${{STALWART_PASSWORD}}\" ]; then echo empty password >&2; exit 1; fi\n\
exec \"$CLI\" --url \"$URL\" --user \"$STALWART_USER\" create apikey --field \"description=$DESC\" --field 'permissions={{\"@type\":\"Inherit\"}}' --field 'allowedIps={{}}'\n",
        sh_quote(password),
        sh_quote(user),
        sh_quote(url),
        sh_quote(Path::new(cli).file_name().and_then(|s| s.to_str()).unwrap_or("stalwart-cli")),
        sh_quote(desc),
        sh_quote(domain),
        if ensure { "1" } else { "0" },
    );
    let out = Command::new(ssh)
        .args(["--", target, "bash", "-s"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            use std::io::Write;
            if let Some(mut stdin) = c.stdin.take() {
                stdin.write_all(script.as_bytes())?;
            }
            c.wait_with_output()
        })?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned()
        + &String::from_utf8_lossy(&out.stderr);
    let _ = domain;
    Ok((text, out.status.code().unwrap_or(1)))
}

fn read_password_file(path: &str) -> Result<String> {
    let p = Path::new(path);
    if !p.is_file() {
        return Err(ToolError::fail(format!("password-file not found: {path}")));
    }
    let meta = fs::metadata(p)?;
    if meta.permissions().mode() & 0o007 != 0 {
        return Err(ToolError::fail(format!(
            "password-file is world-accessible (mode {:o}); chmod 0600 and retry: {path}",
            meta.permissions().mode() & 0o777
        )));
    }
    let s = fs::read_to_string(p)?;
    Ok(s.lines().next().unwrap_or("").trim_end_matches('\r').to_string())
}

fn scrub(out: &str, password: &str) -> String {
    out.lines()
        .filter(|l| !l.contains(concat!("Secret", ":")))
        .map(|l| {
            let mut s = l.replace(password, "<redacted>");
            // Redact API_-shaped fragments without logging them.
            while let Some(i) = s.find("API_") {
                let rest = &s[i + 4..];
                let n = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '=' || *c == '_' || *c == '-')
                    .count();
                s.replace_range(i..i + 4 + n, "<redacted>");
            }
            s
        })
        .take(20)
        .collect::<Vec<_>>()
        .join("\n")
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn need(argv: &[String], i: usize) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail("flag needs a value".to_string()))
}
