use std::fs;
use std::path::Path;
use std::process::Command;

use crate::cli::run_cli_ok;
use crate::error::{Result, ToolError};
use crate::json::{find_domain_id, query_has_selector, selector_stage_active};
use crate::token::{
    DEFAULT_TOKEN_FILE, Token, assert_loopback_8080, assert_safe, load_token, which_or,
};

const USAGE: &str = "\
Usage:
  register-dkim --help
  register-dkim [--dry-run] [options]
  register-dkim --live [options]
  register-dkim --query-only [options]

Register File-type DkimSignature objects for dual-sign (selectors stalwart +
stalwart-rsa). Default is dry-run (no datastore create). Never logs secret
values. URL is loopback :8080 only.
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
    let mut skip_sandbox = false;
    let mut skip_pem = false;
    let mut token_file = std::env::var("STALWART_TOKEN_FILE").unwrap_or_default();
    let mut url = std::env::var("STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut cli = std::env::var("STALWART_CLI").unwrap_or_default();
    let mut systemctl = std::env::var("SURMOUNT_REGISTER_DKIM_SYSTEMCTL")
        .unwrap_or_else(|_| "systemctl".into());
    let mut domain = std::env::var("SURMOUNT_REGISTER_DKIM_DOMAIN")
        .unwrap_or_else(|_| "surmount.systems".into());
    let mut ed_pem = std::env::var("SURMOUNT_REGISTER_DKIM_ED25519_PEM").unwrap_or_else(|_| {
        "/var/lib/surmount/secrets/mail/dkim/stalwart-ed25519.pem".into()
    });
    let mut rsa_pem = std::env::var("SURMOUNT_REGISTER_DKIM_RSA_PEM").unwrap_or_else(|_| {
        "/var/lib/surmount/secrets/mail/dkim/stalwart-rsa4096.pem".into()
    });
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                println!("  --dry-run  --live  --query-only  --token-file  --url  --cli  --domain");
                println!("  --ed25519-pem  --rsa-pem  --systemctl-cmd  --skip-sandbox-check  --skip-pem-check");
                println!("Management HTTP is loopback :8080 only.");
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
            "--token-file" => {
                i += 1;
                token_file = need(&argv, i, "--token-file")?;
            }
            "--url" => {
                i += 1;
                url = need(&argv, i, "--url")?;
            }
            "--cli" => {
                i += 1;
                cli = need(&argv, i, "--cli")?;
            }
            "--domain" => {
                i += 1;
                domain = need(&argv, i, "--domain")?;
            }
            "--ed25519-pem" => {
                i += 1;
                ed_pem = need(&argv, i, "--ed25519-pem")?;
            }
            "--rsa-pem" => {
                i += 1;
                rsa_pem = need(&argv, i, "--rsa-pem")?;
            }
            "--systemctl-cmd" => {
                i += 1;
                systemctl = need(&argv, i, "--systemctl-cmd")?;
            }
            "--skip-sandbox-check" => skip_sandbox = true,
            "--skip-pem-check" => skip_pem = true,
            other => return Err(ToolError::fail(format!("unknown argument: {other} (try --help)"))),
        }
        i += 1;
    }
    if token_file.is_empty() {
        token_file = DEFAULT_TOKEN_FILE.into();
    }
    assert_safe("domain", &domain)?;
    assert_safe("ed25519-pem path", &ed_pem)?;
    assert_safe("rsa-pem path", &rsa_pem)?;
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
        assert_regular_secret("Ed25519 PEM", &ed_pem)?;
        assert_regular_secret("RSA-4096 PEM", &rsa_pem)?;
        eprintln!("register-dkim: pem preflight ok ed25519={ed_pem} rsa={rsa_pem}");
    } else {
        eprintln!("register-dkim: skip pem check (--skip-pem-check)");
    }

    if skip_sandbox {
        eprintln!("register-dkim: skip sandbox check (--skip-sandbox-check)");
    } else {
        sandbox_preflight(&systemctl, &ed_pem)?;
    }

    let mode = if live {
        "live"
    } else if query_only {
        "query-only"
    } else {
        "dry-run"
    };
    eprintln!(
        "register-dkim: mode={mode} url={url} domain={domain} token_source={} token_file={}",
        token.source, token.file
    );

    eprintln!("register-dkim: query Domain (fields id,name)");
    let domain_out = run_cli_ok(
        &cli,
        &url,
        &token,
        &["query", "Domain", "--fields", "id,name", "--json"],
        "register-dkim",
    )?;
    let domain_id = find_domain_id(&domain_out, &domain).ok_or_else(|| {
        ToolError::blocked(format!(
            "query Domain did not find name={domain}. Create the Domain first (live dashboard / bootstrap). Secret values not logged."
        ))
    })?;
    eprintln!("register-dkim: domain found name={domain} id={domain_id}");

    eprintln!("register-dkim: query DkimSignature (fields id,selector,stage,domainId)");
    let sig_out = run_cli_ok(
        &cli,
        &url,
        &token,
        &[
            "query",
            "DkimSignature",
            "--fields",
            "id,selector,stage,domainId",
            "--json",
        ],
        "register-dkim",
    )?;
    let have_ed = query_has_selector(&sig_out, "stalwart", &domain_id);
    let have_rsa = query_has_selector(&sig_out, "stalwart-rsa", &domain_id);
    eprintln!(
        "register-dkim: existing selectors: stalwart={} stalwart-rsa={}",
        if have_ed { "present" } else { "absent" },
        if have_rsa { "present" } else { "absent" }
    );

    if query_only {
        eprintln!("register-dkim: query-only complete (not claiming sign-ready)");
        return Ok(());
    }

    let need_ed = !have_ed;
    let need_rsa = !have_rsa;
    if dry_run {
        if !need_ed && !need_rsa {
            eprintln!("register-dkim: dry-run: both selectors already present; --live would succeed honestly (no create)");
        } else {
            if need_ed {
                eprintln!(
                    "register-dkim: dry-run: would create DkimSignature/Dkim1Ed25519Sha256 selector=stalwart File {ed_pem}"
                );
            }
            if need_rsa {
                eprintln!(
                    "register-dkim: dry-run: would create DkimSignature/Dkim1RsaSha256 selector=stalwart-rsa File {rsa_pem}"
                );
            }
        }
        eprintln!("register-dkim: dry-run complete: no DkimSignature create claimed. Re-run with --live on the operator host when ready. Not claiming outbound signed mail.");
        return Ok(());
    }

    if !need_ed && !need_rsa {
        eprintln!("register-dkim: idempotent: both selectors already present; no create");
    } else {
        if need_ed {
            create_one(&cli, &url, &token, "DkimSignature/Dkim1Ed25519Sha256", "stalwart", &ed_pem, &domain_id)?;
        } else {
            eprintln!("register-dkim: skip create selector=stalwart (already present)");
        }
        if need_rsa {
            create_one(&cli, &url, &token, "DkimSignature/Dkim1RsaSha256", "stalwart-rsa", &rsa_pem, &domain_id)?;
        } else {
            eprintln!("register-dkim: skip create selector=stalwart-rsa (already present)");
        }
    }

    eprintln!("register-dkim: prove query DkimSignature after live");
    let sig_out = run_cli_ok(
        &cli,
        &url,
        &token,
        &[
            "query",
            "DkimSignature",
            "--fields",
            "id,selector,stage,domainId",
            "--json",
        ],
        "register-dkim",
    )?;
    if !query_has_selector(&sig_out, "stalwart", &domain_id) {
        return Err(ToolError::fail(
            "prove failed: selector=stalwart not present after live".to_string(),
        ));
    }
    if !query_has_selector(&sig_out, "stalwart-rsa", &domain_id) {
        return Err(ToolError::fail(
            "prove failed: selector=stalwart-rsa not present after live".to_string(),
        ));
    }
    if !selector_stage_active(&sig_out, "stalwart", &domain_id) {
        return Err(ToolError::fail(
            "prove failed: selector=stalwart not stage=active".to_string(),
        ));
    }
    if !selector_stage_active(&sig_out, "stalwart-rsa", &domain_id) {
        return Err(ToolError::fail(
            "prove failed: selector=stalwart-rsa not stage=active".to_string(),
        ));
    }
    eprintln!("register-dkim: live complete: both DkimSignature objects present stage=active (selectors stalwart + stalwart-rsa). Sign-ready. Not claiming outbound signed mail (MX / mail-tester residual).");
    Ok(())
}

fn create_one(
    cli: &str,
    url: &str,
    token: &Token,
    variant: &str,
    selector: &str,
    pem: &str,
    domain_id: &str,
) -> Result<()> {
    let pk = format!("{{\"@type\":\"File\",\"filePath\":\"{pem}\"}}");
    eprintln!("register-dkim: create {variant} selector={selector} privateKey=File path (value not logged)");
    let field_domain = format!("domainId={domain_id}");
    let field_sel = format!("selector={selector}");
    let field_pk = format!("privateKey={pk}");
    run_cli_ok(
        cli,
        url,
        token,
        &[
            "create",
            variant,
            "--field",
            &field_domain,
            "--field",
            &field_sel,
            "--field",
            &field_pk,
            "--field",
            "stage=active",
        ],
        "register-dkim",
    )?;
    Ok(())
}

fn assert_regular_secret(label: &str, path: &str) -> Result<()> {
    let p = Path::new(path);
    if p.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
        return Err(ToolError::fail(format!(
            "{label} must be a regular file (symlink refused): {path}"
        )));
    }
    if !p.is_file() {
        return Err(ToolError::blocked(format!("{label} missing: {path}")));
    }
    Ok(())
}

fn sandbox_preflight(systemctl: &str, ed_pem: &str) -> Result<()> {
    if !Path::new(systemctl).is_file() && which_cmd(systemctl).is_none() {
        return Err(ToolError::blocked(format!(
            "systemctl not found ({systemctl}); cannot prove mail unit can read File PEM paths. Pass --skip-sandbox-check only for hermetic tests."
        )));
    }
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
        .map_err(|e| {
            ToolError::blocked(format!(
                "systemctl show stalwart-mail failed ({e}). Mail unit may be missing. Secret values not logged."
            ))
        })?;
    if !out.status.success() {
        return Err(ToolError::blocked(format!(
            "systemctl show stalwart-mail failed (rc={}). Mail unit may be missing. Secret values not logged.",
            out.status.code().unwrap_or(1)
        )));
    }
    let show = String::from_utf8_lossy(&out.stdout);
    let dkim_dir = Path::new(ed_pem)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    if show.contains("ProtectSystem=strict") {
        if show.contains(&dkim_dir) || show.contains("/var/lib/surmount/secrets/mail/dkim") {
            eprintln!("register-dkim: sandbox grants DKIM PEM dir ({dkim_dir})");
        } else {
            return Err(ToolError::blocked(format!(
                "ProtectSystem=strict and {dkim_dir} is not on ReadOnlyPaths/ReadWritePaths. Add Nix ReadOnlyPaths grant (modules/mail.nix) before creating File DkimSignature objects."
            )));
        }
    } else {
        eprintln!("register-dkim: sandbox: ProtectSystem is not strict (or unset); File path grant not required this run");
    }
    let _ = fs::metadata;
    Ok(())
}

fn which_cmd(spec: &str) -> Option<String> {
    crate::token::which_or(spec).ok()
}

fn need(argv: &[String], i: usize, flag: &str) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail(format!("{flag} requires a path")))
}
