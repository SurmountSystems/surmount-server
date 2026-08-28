use std::path::Path;
use std::process::Command;

use crate::cli::run_cli_ok;
use crate::error::{Result, ToolError};
use crate::json::{file_cert_id_in_query, query_has_file_path};
use crate::token::{DEFAULT_TOKEN_FILE, assert_loopback_8080, assert_safe, load_token, which_or};

const DEFAULT_CERT: &str = "/var/lib/surmount/secrets/mail/tls/cert.pem";
const DEFAULT_KEY: &str = "/var/lib/surmount/secrets/mail/tls/key.pem";
const DEFAULT_TLS_DIR: &str = "/var/lib/surmount/secrets/mail/tls";
const AXUM_TLS_DIR: &str = "/var/lib/surmount/secrets/tls";
const DEFAULT_EXPECT: &str = "services.surmount.systems,mail.surmount.systems,surmount.systems,www.surmount.systems,mta-sts.surmount.systems";

const USAGE: &str = "\
Usage:
  point-stalwart-mail-tls --help
  point-stalwart-mail-tls [--dry-run] [options]
  point-stalwart-mail-tls --live [options]
  point-stalwart-mail-tls --query-only [options]

Register a Stalwart Certificate object whose File paths are the mail-plane
copies of the durable Let's Encrypt PEMs (secrets/mail/tls). Axum keeps
secrets/tls with key mode 0600. Default is dry-run. Never logs secret values.
URL is loopback :8080 only.
";

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut dry_run = true;
    let mut live = false;
    let mut query_only = false;
    let mut do_restart = false;
    let mut skip_sandbox = false;
    let mut skip_pem = false;
    let mut expect = std::env::var("SURMOUNT_POINT_MAIL_TLS_EXPECT_HOSTNAMES")
        .unwrap_or_else(|_| DEFAULT_EXPECT.into());
    let mut token_file = std::env::var("STALWART_TOKEN_FILE").unwrap_or_default();
    let mut url = std::env::var("STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut cli = std::env::var("STALWART_CLI").unwrap_or_default();
    let mut systemctl =
        std::env::var("SURMOUNT_POINT_MAIL_TLS_SYSTEMCTL").unwrap_or_else(|_| "systemctl".into());
    let mut cert =
        std::env::var("SURMOUNT_POINT_MAIL_TLS_CERT").unwrap_or_else(|_| DEFAULT_CERT.into());
    let mut key =
        std::env::var("SURMOUNT_POINT_MAIL_TLS_KEY").unwrap_or_else(|_| DEFAULT_KEY.into());
    let mut plan = String::new();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                println!(
                    "  --dry-run  --live  --query-only  --restart  --token-file  --url  --cli"
                );
                println!(
                    "  --cert  --key  --plan  --systemctl-cmd  --skip-sandbox-check  --skip-pem-check"
                );
                println!("  --expect-hostnames  Certificate File paths. loopback :8080.");
                return Ok(());
            }
            "--dry-run" => {
                dry_run = true;
                live = false;
                query_only = false;
            }
            "--live" => {
                live = true;
                dry_run = false;
                query_only = false;
            }
            "--query-only" => {
                query_only = true;
                dry_run = false;
                live = false;
            }
            "--restart" => do_restart = true,
            "--token-file" => {
                i += 1;
                token_file = need(&argv, i)?;
            }
            "--url" => {
                i += 1;
                url = need(&argv, i)?;
            }
            "--cli" => {
                i += 1;
                cli = need(&argv, i)?;
            }
            "--cert" => {
                i += 1;
                cert = need(&argv, i)?;
            }
            "--key" => {
                i += 1;
                key = need(&argv, i)?;
            }
            "--plan" => {
                i += 1;
                plan = need(&argv, i)?;
            }
            "--systemctl-cmd" => {
                i += 1;
                systemctl = need(&argv, i)?;
            }
            "--skip-sandbox-check" => skip_sandbox = true,
            "--skip-pem-check" => skip_pem = true,
            "--expect-hostnames" => {
                i += 1;
                expect = need(&argv, i)?;
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
        i += 1;
    }
    if token_file.is_empty() {
        token_file = DEFAULT_TOKEN_FILE.into();
    }
    assert_safe("cert path", &cert)?;
    assert_safe("key path", &key)?;
    assert_safe("url", &url)?;
    if url.starts_with('-') {
        return Err(ToolError::fail("url must not start with '-'".to_string()));
    }
    assert_loopback_8080(&url)?;
    if cli.is_empty() {
        cli = "stalwart-cli".into();
    }
    let cli = which_or(&cli)?;
    let token = load_token(&token_file)?;
    if !skip_pem {
        assert_regular("TLS cert PEM", &cert)?;
        assert_regular("TLS key PEM", &key)?;
        eprintln!("point-stalwart-mail-tls: pem preflight ok cert={cert} key={key}");
    }
    if skip_sandbox {
        eprintln!("point-stalwart-mail-tls: skip sandbox check (--skip-sandbox-check)");
    } else {
        sandbox(&systemctl, &cert)?;
    }
    let mode = if live {
        "live"
    } else if query_only {
        "query-only"
    } else {
        "dry-run"
    };
    eprintln!(
        "point-stalwart-mail-tls: mode={mode} url={url} cert={cert} token_source={} token_file={} plan={plan}",
        token.source, token.file
    );
    eprintln!("point-stalwart-mail-tls: query Certificate");
    let cert_out = run_cli_ok(&cli, &url, &token, &["query", "Certificate", "--json"], "point-stalwart-mail-tls")
        .map_err(|_| {
            ToolError::fail(
                "stalwart-cli query Certificate failed (auth, URL, or engine). Installing Domain B stalwart-token is not the same as engine registration. Secret values not logged.".to_string(),
            )
        })?;
    let mut cert_id = file_cert_id_in_query(&cert_out, DEFAULT_CERT, &expect)
        .or_else(|| file_cert_id_in_query(&cert_out, &cert, &expect));
    let mut have_file = cert_id.is_some()
        || query_has_file_path(&cert_out, DEFAULT_CERT)
        || query_has_file_path(&cert_out, &cert);
    eprintln!(
        "point-stalwart-mail-tls: existing File Certificate at durable PEMs: {} id={}",
        if have_file { "present" } else { "absent" },
        cert_id.as_deref().unwrap_or("unknown")
    );
    if query_only {
        eprintln!(
            "point-stalwart-mail-tls: query-only complete (not claiming IMAP presents Let's Encrypt)"
        );
        return Ok(());
    }
    if dry_run {
        if have_file {
            eprintln!(
                "point-stalwart-mail-tls: dry-run: File Certificate already present (id={}); --live would set defaultCertificateId and skip create",
                cert_id.as_deref().unwrap_or("unknown")
            );
        } else {
            eprintln!(
                "point-stalwart-mail-tls: dry-run: --live would create Certificate File paths cert={cert} key={key} and set SystemSettings.defaultCertificateId"
            );
        }
        eprintln!(
            "point-stalwart-mail-tls: dry-run complete (not claiming IMAP presents Let's Encrypt)"
        );
        return Ok(());
    }
    if !have_file {
        let field_c = format!("certificate={{\"@type\":\"File\",\"filePath\":\"{cert}\"}}");
        let field_k = format!("privateKey={{\"@type\":\"File\",\"filePath\":\"{key}\"}}");
        eprintln!("point-stalwart-mail-tls: create Certificate File cert={cert} key={key}");
        run_cli_ok(
            &cli,
            &url,
            &token,
            &[
                "create",
                "Certificate",
                "--field",
                &field_c,
                "--field",
                &field_k,
            ],
            "point-stalwart-mail-tls",
        )?;
        eprintln!("point-stalwart-mail-tls: create Certificate ok");
        let cert_out = run_cli_ok(
            &cli,
            &url,
            &token,
            &["query", "Certificate", "--json"],
            "point-stalwart-mail-tls",
        )?;
        cert_id = file_cert_id_in_query(&cert_out, DEFAULT_CERT, &expect)
            .or_else(|| file_cert_id_in_query(&cert_out, &cert, &expect));
        have_file = cert_id.is_some();
        let _ = have_file;
    }
    let cert_id = cert_id.ok_or_else(|| {
        ToolError::fail(
            "could not resolve File Certificate id after create/query (refusing first-boot rcgen). Secret values not logged.".to_string(),
        )
    })?;
    let field_c = format!("certificate={{\"@type\":\"File\",\"filePath\":\"{cert}\"}}");
    let field_k = format!("privateKey={{\"@type\":\"File\",\"filePath\":\"{key}\"}}");
    eprintln!(
        "point-stalwart-mail-tls: update Certificate {cert_id} File paths cert={cert} key={key}"
    );
    run_cli_ok(
        &cli,
        &url,
        &token,
        &[
            "update",
            "Certificate",
            &cert_id,
            "--field",
            &field_c,
            "--field",
            &field_k,
        ],
        "point-stalwart-mail-tls",
    )?;
    let field = format!("defaultCertificateId={cert_id}");
    eprintln!("point-stalwart-mail-tls: update SystemSettings defaultCertificateId={cert_id}");
    run_cli_ok(
        &cli,
        &url,
        &token,
        &["update", "SystemSettings", "--field", &field],
        "point-stalwart-mail-tls",
    )?;
    eprintln!("point-stalwart-mail-tls: SystemSettings.defaultCertificateId set");
    if do_restart {
        eprintln!("point-stalwart-mail-tls: restart stalwart-mail");
        let st = Command::new(&systemctl)
            .args(["restart", "stalwart-mail"])
            .status()?;
        if !st.success() {
            return Err(ToolError::fail(
                "systemctl restart stalwart-mail failed".to_string(),
            ));
        }
    }
    eprintln!(
        "point-stalwart-mail-tls: live complete: File Certificate id={cert_id} (not a live :993 handshake proof)"
    );
    Ok(())
}

fn assert_regular(label: &str, path: &str) -> Result<()> {
    let p = Path::new(path);
    if p.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(ToolError::fail(format!(
            "{label} must be a regular file (symlink refused): {path}"
        )));
    }
    if !p.is_file() {
        return Err(ToolError::blocked(format!("{label} missing: {path}")));
    }
    Ok(())
}

fn sandbox(systemctl: &str, cert: &str) -> Result<()> {
    let out = Command::new(systemctl)
        .args([
            "show",
            "stalwart-mail",
            "-p",
            "ProtectSystem",
            "-p",
            "ReadWritePaths",
            "-p",
            "ReadOnlyPaths",
        ])
        .output()
        .map_err(|_| {
            ToolError::blocked(format!(
                "systemctl not found ({systemctl}); cannot prove mail unit can read File PEM paths. Pass --skip-sandbox-check only for hermetic tests."
            ))
        })?;
    if !out.status.success() {
        return Err(ToolError::blocked(format!(
            "systemctl show stalwart-mail failed (rc={}). Mail unit may be missing. Secret values not logged.",
            out.status.code().unwrap_or(1)
        )));
    }
    let show = String::from_utf8_lossy(&out.stdout);
    let tls_dir = Path::new(cert)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if show.contains("ProtectSystem=strict") {
        if show.contains(&tls_dir)
            || show.contains(DEFAULT_TLS_DIR)
            || show.contains(AXUM_TLS_DIR)
            || show.contains(DEFAULT_CERT)
        {
            eprintln!("point-stalwart-mail-tls: sandbox grants TLS PEM dir ({tls_dir})");
        } else {
            return Err(ToolError::blocked(
                "ProtectSystem=strict and mail/tls (or secrets/tls) is not on ReadOnlyPaths/ReadWritePaths. Add Nix ReadOnlyPaths grant (modules/mail.nix) before creating File Certificate objects.".to_string(),
            ));
        }
    }
    Ok(())
}

fn need(argv: &[String], i: usize) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail("flag requires a value".to_string()))
}
