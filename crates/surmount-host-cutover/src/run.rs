use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use surmount_deploy_host::{
    assert_safe_token, discover_repo_root, is_path_under_root, is_public_product_path, realpath_m,
};

use crate::args::{CutoverOpts, Parse, USAGE};
use crate::error::ToolError;
use crate::fragments::emit_fragments;
use crate::generate::generate_missing_material;
use crate::inventory::{
    InventoryOpts, PATH_ACME_ACCOUNT, PATH_STALWART, PATH_TLS_CERT, run_inventory, staging_has_kind,
};

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if s.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '/' | '.' | '_' | '-' | '=' | ':' | '@' | '+' | ',' | '#')
    }) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn repo_root() -> PathBuf {
    if let Ok(p) = env::var("SURMOUNT_CUTOVER_REPO_ROOT") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    discover_repo_root()
}

pub fn run<I, S>(args: I) -> Result<(), ToolError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    match crate::args::parse_args(args)? {
        Parse::Help => {
            print!("{USAGE}");
            Ok(())
        }
        Parse::Run(opts) => run_cutover(opts),
    }
}

fn inv_from(opts: &CutoverOpts) -> Result<InventoryOpts, ToolError> {
    let staging = opts.staging.clone().ok_or_else(|| {
        ToolError::fail("staging required: --staging DIR or SURMOUNT_SECRETS_STAGING".to_string())
    })?;
    Ok(InventoryOpts {
        staging,
        host_id: opts.host_id.clone(),
        with_vaultwarden: opts.with_vw,
        acme_path: opts.acme_path,
        acme_account_path: opts.acme_account_path.clone(),
        dns_hook_path: opts.dns_hook_path.clone(),
        require_dns_hook: false,
        json: false,
    })
}

fn log_line(msg: &str) {
    eprintln!("host-cutover: {msg}");
}

fn print_honesty(step: &str) {
    log_line(&format!(
        "step={step}: will NOT claim public HTTPS proven, Let's Encrypt issued, or mail engine token registered"
    ));
}

fn print_next(opts: &CutoverOpts, next: &str, extra: &str) {
    let mut base = format!("surmount-host-cutover --step {next}");
    if let Some(s) = opts.staging.as_ref() {
        base.push_str(&format!(" --staging {}", shell_quote(&s.to_string_lossy())));
    }
    if let Some(id) = opts.host_id.as_ref() {
        base.push_str(&format!(" --host-id {}", shell_quote(id)));
    }
    if let Some(t) = opts.target.as_ref() {
        base.push_str(&format!(" --target {}", shell_quote(t)));
    }
    if let Some(hl) = opts.host_local.as_ref() {
        base.push_str(&format!(
            " --host-local {}",
            shell_quote(&hl.to_string_lossy())
        ));
    }
    if let Some(p) = opts.host_profile.as_ref() {
        base.push_str(&format!(
            " --host-profile {}",
            shell_quote(&p.to_string_lossy())
        ));
    }
    if opts.acme_path {
        base.push_str(" --acme-path");
    }
    if opts.with_vw {
        base.push_str(" --with-vaultwarden");
    }
    if !extra.is_empty() {
        base.push(' ');
        base.push_str(extra);
    }
    log_line(&format!("step next command suggestion: {base}"));
}

fn resolve_tool(env_key: &str, names: &[&str]) -> Result<PathBuf, ToolError> {
    if let Ok(p) = env::var(env_key) {
        if !p.is_empty() {
            let pb = PathBuf::from(&p);
            if pb.is_file() {
                return Ok(pb);
            }
            return Err(ToolError::fail(format!("{env_key} missing: {p}")));
        }
    }
    if let Ok(path) = env::var("PATH") {
        for n in names {
            for dir in path.split(':') {
                let p = Path::new(dir).join(n);
                if p.is_file() {
                    return Ok(p);
                }
            }
        }
    }
    Err(ToolError::fail(format!(
        "helper missing on PATH (set {env_key} or nix run the matching crate): {}",
        names.join(", ")
    )))
}

fn assert_staging_outside(staging: &Path, repo: &Path) -> Result<PathBuf, ToolError> {
    if !staging.is_dir() {
        return Err(ToolError::fail(format!(
            "staging is not a directory: {}",
            staging.display()
        )));
    }
    let canon = realpath_m(staging);
    if is_path_under_root(repo, &canon) {
        return Err(ToolError::fail(format!(
            "refuse: secrets staging must be outside the public git work tree ({}). Public rsync would copy staging into the remote checkout. See docs/SECRETS.md",
            staging.display()
        )));
    }
    Ok(canon)
}

fn print_argv(prefix: &str, argv: &[String]) {
    print!("{prefix}");
    for a in argv {
        print!(" {}", shell_quote(a));
    }
    println!();
}

fn run_argv(argv: &[String]) -> Result<(), ToolError> {
    if argv.is_empty() {
        return Ok(());
    }
    let st = Command::new(&argv[0])
        .args(&argv[1..])
        .status()
        .map_err(|e| ToolError::fail(format!("failed to start {}: {e}", argv[0])))?;
    if st.success() {
        Ok(())
    } else {
        Err(ToolError::fail(format!(
            "{} exited {}",
            argv[0],
            st.code().unwrap_or(1)
        )))
    }
}

fn require_kinds(opts: &CutoverOpts) -> Vec<String> {
    let mut k = Vec::new();
    if !opts.acme_path {
        k.extend(["tls-cert".into(), "tls-key".into()]);
    }
    k.extend(["session-secret".into(), "stalwart-token".into()]);
    if opts.with_vw {
        k.push("vaultwarden-admin".into());
    }
    if opts.require_namecheap {
        k.push("namecheap-api".into());
    } else if let Some(st) = opts.staging.as_ref() {
        if staging_has_kind(st, "namecheap-api") {
            k.push("namecheap-api".into());
        }
    }
    k
}

fn extract_acme_domains(profile: &Path) -> Result<Vec<String>, ToolError> {
    let text = fs::read_to_string(profile)?;
    let mut domains = Vec::new();
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
            let val = line
                .trim_end_matches(',')
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            if !val.is_empty() {
                domains.push(val.to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("acme_domains") {
            let rest = rest.trim().trim_start_matches('=').trim();
            if rest == "[" {
                in_domains = true;
            } else if rest.starts_with('[') && rest.ends_with(']') {
                let inner = &rest[1..rest.len() - 1];
                for part in inner.split(',') {
                    let p = part.trim().trim_matches('"').trim_matches('\'');
                    if !p.is_empty() {
                        domains.push(p.to_string());
                    }
                }
            }
        }
    }
    if domains.is_empty() {
        return Err(ToolError::fail(
            "step dns: no acme_domains in host profile".into(),
        ));
    }
    Ok(domains)
}

fn run_cutover(mut opts: CutoverOpts) -> Result<(), ToolError> {
    let repo = repo_root();
    if let Some(p) = opts.dns_hook_path.as_deref() {
        assert_safe_token("dns-hook-path", p)?;
    }
    assert_safe_token("acme-account-path", &opts.acme_account_path)?;

    if opts.inventory_only {
        let inv = inv_from(&opts)?;
        assert_staging_outside(&inv.staging, &repo)?;
        return run_inventory(&inv, &repo).map_err(|e| {
            ToolError::fail(if e.message.contains("refuse") {
                e.message
            } else {
                e.message
            })
        });
    }

    if opts.emit_only {
        let dest = opts.emit_fragments.clone().ok_or_else(|| {
            ToolError::fail("--emit-fragments-only requires --emit-fragments DIR")
        })?;
        let inv = if opts.with_vw {
            let i = inv_from(&opts)?;
            if opts.host_id.is_none() {
                return Err(ToolError::fail(
                    "--emit-fragments-only --with-vaultwarden requires --host-id".into(),
                ));
            }
            assert_safe_token("host-id", opts.host_id.as_deref().unwrap())?;
            assert_staging_outside(&i.staging, &repo)?;
            Some(i)
        } else {
            None
        };
        let dns = opts
            .dns_hook_path
            .clone()
            .unwrap_or_else(|| "/run/surmount/acme-dns-hook".into());
        return emit_fragments(
            &dest,
            &repo,
            opts.with_vw,
            opts.acme_path,
            &dns,
            &opts.acme_account_path,
            inv.as_ref(),
        );
    }

    if let Some(step) = opts.step.clone() {
        match step.as_str() {
            "material" | "prep" | "install" | "free-443" | "dns" | "deploy" | "prove"
            | "le-prod" | "all" => {}
            other => {
                return Err(ToolError::fail(format!(
                    "unknown --step {other} (try material|prep|install|free-443|dns|deploy|prove|le-prod|all)"
                )));
            }
        }
        if let Some(t) = opts.target.as_deref() {
            assert_safe_token("target", t)?;
            if t.starts_with('-') {
                return Err(ToolError::fail(format!(
                    "target must not start with '-': option-shaped SSH targets are rejected (got {t})"
                )));
            }
        }
        return match step.as_str() {
            "material" => step_material(&opts, &repo),
            "prep" => step_prep(&opts, &repo),
            "install" => step_install(&opts, &repo),
            "free-443" => {
                opts.free_443 = true;
                step_free_443(&opts, &repo)
            }
            "dns" => step_dns(&opts, &repo),
            "deploy" => step_deploy(&opts, &repo),
            "prove" => step_prove(&opts),
            "le-prod" => step_le_prod(&opts, &repo),
            "all" => step_all(&opts, &repo),
            _ => unreachable!(),
        };
    }

    full_cutover(&opts, &repo)
}

fn step_material(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    log_line(
        "step=material: will inventory private staging and optionally generate non-CA material",
    );
    print_honesty("material");
    let stg = opts
        .staging
        .as_ref()
        .ok_or_else(|| ToolError::fail("--step material requires --staging"))?;
    let hid = opts
        .host_id
        .as_deref()
        .ok_or_else(|| ToolError::fail("--step material requires --host-id"))?;
    assert_safe_token("host-id", hid)?;
    assert_staging_outside(stg, repo)?;
    if opts.generate_material {
        generate_missing_material(stg, hid, opts.with_vw, opts.acme_path, repo)?;
    }
    log_line("gate G1: material inventory (step=material)");
    let inv = inv_from(opts)?;
    if run_inventory(&inv, repo).is_err() {
        return Err(ToolError::fail(
            "step material: inventory incomplete. Fill private staging or pass --generate-material for non-CA kinds."
                .into(),
        ));
    }
    log_line("step=material: inventory complete (values not logged)");
    print_next(
        opts,
        "prep",
        "--acme-path --host-profile PATH --host-local DIR",
    );
    Ok(())
}

fn step_prep(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    log_line("step=prep: will render private host profile (ACME + public listen) into host-local");
    print_honesty("prep");
    let profile = opts
        .host_profile
        .as_ref()
        .ok_or_else(|| ToolError::fail("--step prep requires --host-profile PATH"))?;
    if opts.host_local.is_none() {
        if opts.dry_run {
            log_line(
                "step=prep: dry-run without --host-local: would render when host-local is set",
            );
            println!(
                "host-cutover: dry-run: would run {} --profile {} --out HOST_LOCAL --force",
                shell_quote("render-host-profile-acme"),
                shell_quote(&profile.to_string_lossy())
            );
            print_next(opts, "install", "--target HOST --acme-path");
            return Ok(());
        }
        return Err(ToolError::fail(
            "--step prep requires --host-local DIR".into(),
        ));
    }
    render_profile(opts, repo)?;
    log_line("step=prep: host-local-acme.nix rendered (non-secret)");
    print_next(opts, "install", "--target HOST --acme-path");
    Ok(())
}

fn step_install(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    log_line(
        "step=install: will push Domain B secrets to VPS (default durable /var/lib/surmount/secrets; laptop custody remains)",
    );
    print_honesty("install");
    let stg = opts
        .staging
        .as_ref()
        .ok_or_else(|| ToolError::fail("--step install requires --staging"))?;
    let hid = opts
        .host_id
        .as_deref()
        .ok_or_else(|| ToolError::fail("--step install requires --host-id"))?;
    assert_safe_token("host-id", hid)?;
    let canon = assert_staging_outside(stg, repo)?;
    if let Some(t) = opts.target.as_deref() {
        assert_safe_token("target", t)?;
        if t.starts_with('-') {
            return Err(ToolError::fail("target must not start with '-'"));
        }
    }
    if opts.live {
        if opts.target.is_none() {
            return Err(ToolError::fail("--live install requires --target"));
        }
        if opts.dest_root.is_some() {
            return Err(ToolError::fail(
                "--live cannot combine with --dest-root".into(),
            ));
        }
    }
    let kinds = require_kinds(opts);
    log_line(&format!("step=install: require kinds: {}", kinds.join(" ")));
    secrets_plan(opts, repo, &canon, &kinds)?;
    print_next(opts, "free-443", "--target HOST --free-443");
    Ok(())
}

fn secrets_plan(
    opts: &CutoverOpts,
    repo: &Path,
    staging_canon: &Path,
    kinds: &[String],
) -> Result<(), ToolError> {
    let sh = resolve_tool(
        "SURMOUNT_CUTOVER_SECRETS_INSTALL",
        &["secrets-install-host", "surmount-secrets-install-host"],
    )?;
    if opts.target.is_none() && opts.dest_root.is_none() {
        if opts.dry_run {
            log_line("step=install: dry-run without dest: printing require-kinds only");
            print!("host-cutover: dry-run: would require secrets kinds:");
            for k in kinds {
                print!(" {k}");
            }
            println!();
            return Ok(());
        }
        return Err(ToolError::fail(
            "step install requires --target or --dest-root".into(),
        ));
    }
    let mut cmd = vec![sh.to_string_lossy().into_owned()];
    if opts.dry_run {
        cmd.push("--dry-run".into());
    }
    cmd.push("--from-staging".into());
    cmd.push(staging_canon.to_string_lossy().into_owned());
    cmd.push("--host-id".into());
    cmd.push(opts.host_id.clone().unwrap_or_default());
    if let Some(d) = opts.dest_root.as_ref() {
        cmd.push("--dest-root".into());
        cmd.push(d.to_string_lossy().into_owned());
    } else if let Some(t) = opts.target.as_ref() {
        cmd.push("--target".into());
        cmd.push(t.clone());
    }
    for k in kinds {
        cmd.push("--require-kind".into());
        cmd.push(k.clone());
    }
    if let Some(ip) = opts.client_ip.as_ref() {
        cmd.push("--client-ip".into());
        cmd.push(ip.clone());
    }
    print_argv("host-cutover: would run", &cmd);
    run_argv(&cmd)?;
    if opts.acme_path && (opts.dest_root.is_some() || opts.target.is_some()) {
        let mut p = vec![sh.to_string_lossy().into_owned()];
        if opts.dry_run {
            p.push("--dry-run".into());
        }
        p.push("--ensure-acme-parents".into());
        if let Some(d) = opts.dest_root.as_ref() {
            p.push("--dest-root".into());
            p.push(d.to_string_lossy().into_owned());
        } else if let Some(t) = opts.target.as_ref() {
            p.push("--target".into());
            p.push(t.clone());
        }
        p.push("--tls-dir".into());
        p.push(
            Path::new(PATH_TLS_CERT)
                .parent()
                .unwrap()
                .display()
                .to_string(),
        );
        p.push("--acme-dir".into());
        p.push(
            Path::new(&opts.acme_account_path)
                .parent()
                .unwrap_or(Path::new(PATH_ACME_ACCOUNT).parent().unwrap())
                .display()
                .to_string(),
        );
        print_argv("host-cutover: would run", &p);
        run_argv(&p)?;
    }
    log_line("step=install: secrets plan/run finished (values not logged)");
    Ok(())
}

fn step_free_443(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    log_line("step=free-443: will free Stalwart public :443 so Axum can bind product HTTPS");
    print_honesty("free-443");
    run_free_443(opts, repo)?;
    print_next(opts, "dns", "--host-profile PATH --dns-a IPV4");
    Ok(())
}

fn run_free_443(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    let sh = resolve_tool(
        "SURMOUNT_CUTOVER_FREE_443",
        &["free-stalwart-public-443"],
    )?;
    let ssh = env::var("SURMOUNT_CUTOVER_SSH").unwrap_or_else(|_| "ssh".into());
    let token = if let Some(d) = opts.dest_root.as_ref() {
        format!("{}{PATH_STALWART}", d.display())
    } else {
        PATH_STALWART.to_string()
    };
    if opts.dest_root.is_none() && opts.target.is_some() {
        let target = opts.target.as_deref().unwrap();
        let mut inner = vec![
            "--token-file".to_string(),
            token.clone(),
            "--plan".into(),
            "/etc/surmount/stalwart/free-public-443-for-axum-edge.ndjson".into(),
        ];
        if opts.live && !opts.dry_run {
            inner.extend(["--live".into(), "--restart".into()]);
        } else {
            inner.extend(["--dry-run".into(), "--skip-ss-check".into()]);
        }
        log_line(&format!(
            "optional free-443: remote plan over SSH (target set; token path on VPS={token}; value not logged)"
        ));
        print!(
            "host-cutover: remote free-443: would run {} -- {} bash -s --",
            shell_quote(&ssh),
            shell_quote(target)
        );
        for a in &inner {
            print!(" {}", shell_quote(a));
        }
        println!(" < {}", shell_quote(&sh.to_string_lossy()));
        print_honesty("free-443");
        if opts.dry_run {
            log_line(
                "free-443 dry-run: remote plan printed only (no SSH mutation). BLOCKED if token missing on live.",
            );
            return Ok(());
        }
        return Ok(());
    }
    let mut cmd = vec![sh.to_string_lossy().into_owned()];
    cmd.push("--token-file".into());
    cmd.push(token.clone());
    cmd.push("--plan".into());
    cmd.push(
        repo.join("nix/stalwart/free-public-443-for-axum-edge.ndjson")
            .display()
            .to_string(),
    );
    if opts.live && !opts.dry_run {
        cmd.extend(["--live".into(), "--restart".into()]);
    } else {
        cmd.extend(["--dry-run".into(), "--skip-ss-check".into()]);
    }
    log_line(&format!(
        "optional free-443: plan/run free-stalwart-public-443 (token path={token}; value not logged)"
    ));
    print_argv("host-cutover: would run", &cmd);
    print_honesty("free-443");
    match run_argv(&cmd) {
        Ok(()) => Ok(()),
        Err(e) if e.message.contains("exited 2") => {
            log_line(
                "free-443 BLOCKED (no token material). Install kind stalwart-token first. Not a hard cutover failure when optional.",
            );
            if opts.live && !opts.dry_run {
                Err(ToolError::fail(
                    "free-443 blocked on live cutover: missing Domain B stalwart-token (or engine auth). Secret values not logged.".into(),
                ))
            } else {
                Ok(())
            }
        }
        Err(e) => Err(e),
    }
}

fn step_dns(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    let zone = resolve_tool(
        "SURMOUNT_CUTOVER_DNS_ZONE",
        &["surmount-dns-zone", "dns-zone-namecheap"],
    )?;
    let profile = opts
        .host_profile
        .as_ref()
        .ok_or_else(|| ToolError::fail("--step dns requires --host-profile PATH"))?;
    if !profile.is_file() {
        return Err(ToolError::fail(format!(
            "--host-profile is not a readable file: {}",
            profile.display()
        )));
    }
    log_line(
        "step=dns: will plan A/AAAA via dns-zone-namecheap from laptop (profile domains; default dry-run)",
    );
    log_line("step=dns: will NOT run concurrent live zone rewrite during ACME TXT challenge");
    print_honesty("dns");
    let domains = extract_acme_domains(profile)?;
    log_line(&format!(
        "step=dns: profile domains (count={}; values are non-secret FQDNs)",
        domains.len()
    ));
    for d in &domains {
        assert_safe_token("dns-domain", d)?;
        let mut cmd = vec![zone.to_string_lossy().into_owned()];
        if opts.live && !opts.dry_run {
            cmd.push("--live".into());
        } else {
            cmd.push("--dry-run".into());
        }
        cmd.push("set-host".into());
        cmd.push(d.clone());
        if let Some(a) = opts.dns_a.as_ref() {
            cmd.push("--a".into());
            cmd.push(a.clone());
        }
        if let Some(a) = opts.dns_aaaa.as_ref() {
            cmd.push("--aaaa".into());
            cmd.push(a.clone());
        }
        print!(
            "host-cutover: dns plan domain={} host={} would run",
            shell_quote(d),
            shell_quote(d)
        );
        for a in &cmd {
            print!(" {}", shell_quote(a));
        }
        println!();
        if opts.dns_a.is_none() && opts.dns_aaaa.is_none() {
            log_line(
                "step=dns: note: no --dns-a/--dns-aaaa yet; printed set-host skeleton only (pass addresses to plan apply)",
            );
        } else if opts.live && !opts.dry_run {
            run_argv(&cmd)?;
        } else if env::var("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR").is_ok()
            || env::var("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV").is_ok()
        {
            run_argv(&cmd)?;
        } else {
            log_line(
                "step=dns: dry-run plan only (no Namecheap call; set SURMOUNT_DNS_ZONE_NAMECHEAP_ENV or pass --live for apply)",
            );
        }
    }
    log_line("step=dns: ban concurrent live zone rewrite while ACME TXT challenges are in flight");
    print_next(opts, "deploy", "");
    Ok(())
}

fn step_deploy(opts: &CutoverOpts, _repo: &Path) -> Result<(), ToolError> {
    log_line("step=deploy: will run deploy-host switch + loopback smoke");
    print_honesty("deploy");
    let target = opts
        .target
        .as_deref()
        .ok_or_else(|| ToolError::fail("--step deploy requires --target"))?;
    assert_safe_token("target", target)?;
    if target.starts_with('-') {
        return Err(ToolError::fail("target must not start with '-'"));
    }
    if opts.live && opts.host_local.is_none() {
        return Err(ToolError::fail(
            "--live deploy requires --host-local".into(),
        ));
    }
    invoke_deploy(opts)?;
    print_next(opts, "prove", "");
    Ok(())
}

fn invoke_deploy(opts: &CutoverOpts) -> Result<(), ToolError> {
    let mut argv = vec!["surmount-deploy-host".to_string()];
    if opts.dry_run {
        argv.push("--dry-run".into());
    }
    if let Some(t) = opts.target.as_ref() {
        argv.push("--target".into());
        argv.push(t.clone());
    }
    if let Some(hl) = opts.host_local.as_ref() {
        argv.push("--host-local".into());
        argv.push(hl.to_string_lossy().into_owned());
    } else if opts.dry_run {
        log_line("step=deploy: note: no --host-local; printing plan only");
        print_argv("host-cutover: would run", &argv);
        log_line("step=deploy: skipping deploy-host invoke (no host-local)");
        return Ok(());
    }
    print_argv("host-cutover: would run", &argv);
    if let Ok(p) = env::var("SURMOUNT_CUTOVER_DEPLOY_HOST") {
        if !p.is_empty() {
            argv[0] = p;
            return run_argv(&argv);
        }
    }
    surmount_deploy_host::run(argv).map_err(Into::into)
}

fn step_prove(opts: &CutoverOpts) -> Result<(), ToolError> {
    log_line("step=prove: will print prove checklist only (no soft-elevate)");
    print_honesty("prove");
    eprintln!(
        "host-cutover: prove checklist (operator; never CI green):\n  \
         1. systemctl is-active sshd stalwart-mail surmount-management-ui\n  \
         2. free-443 already applied (or step free-443) so Axum owns :443\n  \
         3. First ACME issue may take time (staging directory first)\n  \
         4. Public HTTPS only when BASE_URL set:\n       SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://YOUR_DOMAIN just e2e-host\n  \
         5. This step does NOT claim LE issued or HTTPS proven without that proof"
    );
    print_next(
        opts,
        "le-prod",
        "--host-profile PATH --host-local DIR --le-directory production",
    );
    Ok(())
}

fn step_le_prod(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    log_line(
        "step=le-prod: will re-render host-local with production Let's Encrypt directory (explicit only)",
    );
    print_honesty("le-prod");
    if opts.host_profile.is_none() {
        return Err(ToolError::fail(
            "--step le-prod requires --host-profile".into(),
        ));
    }
    if opts.host_local.is_none() {
        return Err(ToolError::fail(
            "--step le-prod requires --host-local".into(),
        ));
    }
    render_profile(opts, repo)?;
    log_line("step=le-prod: re-render done; next is deploy then prove (no auto HTTPS claim)");
    print_next(opts, "deploy", "--target HOST --host-local DIR");
    Ok(())
}

fn step_all(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    log_line(
        "step=all: full dry compose of safe planning steps (no live LE / live Namecheap claim)",
    );
    print_honesty("all");
    step_material(opts, repo)?;
    if opts.host_profile.is_some() {
        step_prep(opts, repo)?;
    } else {
        log_line("step=all: skip prep (no --host-profile)");
    }
    step_install(opts, repo)?;
    if opts.target.is_some() || opts.dest_root.is_some() {
        let mut o = opts.clone();
        o.free_443 = true;
        step_free_443(&o, repo)?;
    } else {
        log_line("step=all: skip free-443 plan (no --target/--dest-root)");
    }
    if opts.host_profile.is_some() {
        step_dns(opts, repo)?;
    } else {
        log_line("step=all: skip dns (no --host-profile)");
    }
    if opts.target.is_some() {
        step_deploy(opts, repo)?;
    } else {
        log_line("step=all: skip deploy plan (no --target)");
    }
    step_prove(opts)?;
    log_line("step=all: dry compose complete. Run le-prod only after staging LE looks good.");
    print_next(opts, "le-prod", "--host-profile PATH --host-local DIR");
    Ok(())
}

fn render_profile(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    let profile = opts
        .host_profile
        .as_ref()
        .ok_or_else(|| ToolError::fail("render_host_profile: empty profile path"))?;
    if !profile.is_file() {
        return Err(ToolError::fail(format!(
            "--host-profile is not a readable file: {}",
            profile.display()
        )));
    }
    let Some(out_dir) = opts.host_local.as_ref() else {
        return Err(ToolError::fail(
            "--host-profile requires --host-local DIR (or SURMOUNT_HOST_LOCAL_DIR)".into(),
        ));
    };
    if !out_dir.is_dir() {
        return Err(ToolError::fail(format!(
            "host-local is not a directory: {}",
            out_dir.display()
        )));
    }
    if is_path_under_root(repo, &realpath_m(out_dir)) {
        return Err(ToolError::fail(format!(
            "refuse: --host-profile out (--host-local) under public git work tree ({})",
            out_dir.display()
        )));
    }
    if is_public_product_path(out_dir, repo) {
        return Err(ToolError::fail(format!(
            "refuse: will not write host-local-acme.nix into public product path ({})",
            out_dir.display()
        )));
    }
    let sh = resolve_tool(
        "SURMOUNT_CUTOVER_RENDER_PROFILE",
        &[
            "surmount-render-host-profile-acme",
            "render-host-profile-acme",
        ],
    )?;
    let mut cmd = vec![
        sh.to_string_lossy().into_owned(),
        "--profile".into(),
        profile.to_string_lossy().into_owned(),
        "--out".into(),
        out_dir.to_string_lossy().into_owned(),
        "--force".into(),
    ];
    if let Some(d) = opts.le_directory.as_ref() {
        cmd.push("--directory".into());
        cmd.push(d.clone());
    }
    log_line("rendering non-secret ACME host-local from profile (profile set; values not secrets)");
    print_argv("host-cutover: would run", &cmd);
    run_argv(&cmd)
}

fn full_cutover(opts: &CutoverOpts, repo: &Path) -> Result<(), ToolError> {
    let stg = opts.staging.as_ref().ok_or_else(|| {
        ToolError::fail("staging required: --staging DIR or SURMOUNT_SECRETS_STAGING")
    })?;
    let hid = opts.host_id.as_deref().ok_or_else(|| {
        ToolError::fail("host-id required: --host-id ID or SURMOUNT_SECRETS_HOST_ID")
    })?;
    assert_safe_token("host-id", hid)?;
    if hid.starts_with('-') {
        return Err(ToolError::fail("host-id must not start with '-'"));
    }
    let canon = assert_staging_outside(stg, repo)?;
    if let Some(t) = opts.target.as_deref() {
        assert_safe_token("target", t)?;
        if t.starts_with('-') {
            return Err(ToolError::fail(format!(
                "target must not start with '-': option-shaped SSH targets are rejected (got {t})"
            )));
        }
    }
    if opts.live {
        if opts.target.is_none() {
            return Err(ToolError::fail(
                "--live requires --target HOST (or SURMOUNT_DEPLOY_TARGET)".into(),
            ));
        }
        let hl = opts.host_local.as_ref().ok_or_else(|| {
            ToolError::fail("--live requires --host-local DIR (or SURMOUNT_HOST_LOCAL_DIR)")
        })?;
        if !hl.is_dir() {
            return Err(ToolError::fail(format!(
                "host-local is not a directory: {}",
                hl.display()
            )));
        }
        if opts.dest_root.is_some() {
            return Err(ToolError::fail(
                "--live cannot combine with --dest-root (dest-root is hermetic local only)".into(),
            ));
        }
    }
    if opts.generate_material {
        generate_missing_material(stg, hid, opts.with_vw, opts.acme_path, repo)?;
    }
    let profile = if opts.with_vw {
        "https+vaultwarden"
    } else {
        "https-only"
    };
    let path = if opts.acme_path { "acme" } else { "pem" };
    log_line(&format!(
        "gate G1: material inventory (profile={profile}; path={path})"
    ));
    let inv = inv_from(opts)?;
    if run_inventory(&inv, repo).is_err() {
        return Err(ToolError::fail(
            "gate G1 failed: material inventory incomplete. Fill private staging (or pass --generate-material for non-CA kinds) before install/deploy. No remote mutation performed.".into(),
        ));
    }
    log_line("gate G1 complete: required material present (values not logged)");

    if let Some(dest) = opts.emit_fragments.as_ref() {
        log_line(&format!(
            "emitting private host-local fragments under {}",
            dest.display()
        ));
        let dns = opts
            .dns_hook_path
            .clone()
            .unwrap_or_else(|| "/run/surmount/acme-dns-hook".into());
        emit_fragments(
            dest,
            repo,
            opts.with_vw,
            opts.acme_path,
            &dns,
            &opts.acme_account_path,
            Some(&inv),
        )?;
    }

    if opts.host_profile.is_some() {
        if opts.host_local.is_none() {
            if opts.dry_run {
                log_line(
                    "note: --host-profile without --host-local: would render host-local-acme.nix when host-local is set",
                );
                println!(
                    "host-cutover: dry-run: would run {} --profile {} --out HOST_LOCAL --force",
                    shell_quote("render-host-profile-acme"),
                    shell_quote(&opts.host_profile.as_ref().unwrap().to_string_lossy())
                );
            } else {
                return Err(ToolError::fail(
                    "--host-profile requires --host-local DIR (or SURMOUNT_HOST_LOCAL_DIR)".into(),
                ));
            }
        } else {
            render_profile(opts, repo)?;
        }
    }

    let kinds = require_kinds(opts);
    if !opts.skip_secrets {
        log_line(&format!(
            "gate G2: secrets-install plan/run (kinds: {})",
            kinds.join(" ")
        ));
        if opts.target.is_none() && opts.dest_root.is_none() {
            if opts.dry_run {
                log_line(
                    "note: no --target/--dest-root; secrets-install dry-run still needs a destination for the bridge",
                );
                log_line(
                    "note: pass --target or --dest-root to plan installs; printing require-kinds only",
                );
                print!("host-cutover: dry-run: would require secrets kinds:");
                for k in &kinds {
                    print!(" {k}");
                }
                println!();
            } else {
                return Err(ToolError::fail(
                    "secrets-install requires --target or --dest-root".into(),
                ));
            }
        } else {
            let prefix = if opts.dry_run {
                "host-cutover: dry-run: would run"
            } else {
                "host-cutover: would run"
            };
            let sh = resolve_tool(
                "SURMOUNT_CUTOVER_SECRETS_INSTALL",
                &["surmount-secrets-install-host", "secrets-install-host"],
            )?;
            let mut cmd = vec![sh.to_string_lossy().into_owned()];
            if opts.dry_run {
                cmd.push("--dry-run".into());
            }
            cmd.push("--from-staging".into());
            cmd.push(canon.to_string_lossy().into_owned());
            cmd.push("--host-id".into());
            cmd.push(hid.to_string());
            if let Some(d) = opts.dest_root.as_ref() {
                cmd.push("--dest-root".into());
                cmd.push(d.to_string_lossy().into_owned());
            } else if let Some(t) = opts.target.as_ref() {
                cmd.push("--target".into());
                cmd.push(t.clone());
            }
            for k in &kinds {
                cmd.push("--require-kind".into());
                cmd.push(k.clone());
            }
            print_argv(prefix, &cmd);
            if !opts.dry_run {
                log_line(&format!(
                    "running secrets-install-host (host-id={hid}; values not logged)"
                ));
            }
            run_argv(&cmd)?;
        }
    } else {
        log_line("gate G2 skipped (--skip-secrets-install)");
    }

    if opts.acme_path {
        if opts.dest_root.is_none() && opts.target.is_none() {
            if opts.dry_run {
                log_line(
                    "note: --acme-path without --target/--dest-root: would ensure ACME parents only when destination is set",
                );
            } else {
                return Err(ToolError::fail(
                    "--acme-path ensure-acme-parents requires --target or --dest-root".into(),
                ));
            }
        } else {
            let sh = resolve_tool(
                "SURMOUNT_CUTOVER_SECRETS_INSTALL",
                &["surmount-secrets-install-host", "secrets-install-host"],
            )?;
            let mut cmd = vec![sh.to_string_lossy().into_owned()];
            if opts.dry_run {
                cmd.push("--dry-run".into());
            }
            cmd.push("--ensure-acme-parents".into());
            if let Some(d) = opts.dest_root.as_ref() {
                cmd.push("--dest-root".into());
                cmd.push(d.to_string_lossy().into_owned());
            } else if let Some(t) = opts.target.as_ref() {
                cmd.push("--target".into());
                cmd.push(t.clone());
            }
            cmd.push("--tls-dir".into());
            cmd.push("/var/lib/surmount/secrets/tls".into());
            cmd.push("--acme-dir".into());
            cmd.push(
                Path::new(&opts.acme_account_path)
                    .parent()
                    .unwrap()
                    .display()
                    .to_string(),
            );
            let prefix = if opts.dry_run {
                "host-cutover: dry-run: would run"
            } else {
                "host-cutover: would run"
            };
            print_argv(prefix, &cmd);
            run_argv(&cmd)?;
        }
    }

    if !opts.skip_deploy {
        if opts.target.is_none() && opts.dry_run {
            log_line("gate deploy: no --target; printing deploy-host reminder only");
            log_line(
                "dry-run: would run deploy-host --dry-run --target HOST --host-local DIR --install-secrets ...",
            );
        } else if opts.target.is_none() {
            return Err(ToolError::fail(
                "deploy requires --target (or --skip-deploy)".into(),
            ));
        } else {
            let prefix = if opts.dry_run {
                "host-cutover: dry-run: would run"
            } else {
                "host-cutover: would run"
            };
            let mut argv = vec!["surmount-deploy-host".to_string()];
            if opts.dry_run {
                argv.push("--dry-run".into());
            }
            argv.push("--target".into());
            argv.push(opts.target.clone().unwrap());
            if let Some(hl) = opts.host_local.as_ref() {
                argv.push("--host-local".into());
                argv.push(hl.to_string_lossy().into_owned());
            } else if opts.dry_run {
                log_line(
                    "note: no --host-local; deploy-host dry-run may fail lockout checks (pass --host-local for full plan)",
                );
            } else {
                return Err(ToolError::fail(
                    "--live deploy requires --host-local".into(),
                ));
            }
            if opts.skip_secrets {
                argv.push("--install-secrets".into());
                argv.push("--secrets-staging".into());
                argv.push(canon.to_string_lossy().into_owned());
                argv.push("--secrets-host-id".into());
                argv.push(hid.to_string());
                for k in &kinds {
                    argv.push("--secrets-require-kind".into());
                    argv.push(k.clone());
                }
            }
            print_argv(prefix, &argv);
            if opts.host_local.is_some() {
                if let Ok(p) = env::var("SURMOUNT_CUTOVER_DEPLOY_HOST") {
                    if !p.is_empty() {
                        argv[0] = p;
                        run_argv(&argv)?;
                    } else {
                        surmount_deploy_host::run(argv.clone())?;
                    }
                } else {
                    surmount_deploy_host::run(argv.clone())?;
                }
            } else {
                log_line("skipping deploy-host invoke (no host-local); command printed above");
            }
        }
    } else {
        log_line("deploy skipped (--skip-deploy)");
    }

    if opts.free_443 {
        run_free_443(opts, repo)?;
    }

    if opts.dry_run {
        log_line(
            "dry-run complete: no remote mutation claimed. Re-run with --live for install+switch on operator machine.",
        );
    } else {
        log_line("live cutover commands finished (verify smoke on host; not CI green).");
    }
    print_smoke_recipes(opts.with_vw);
    Ok(())
}

fn print_smoke_recipes(with_vw: bool) {
    println!(
        "\nhost-cutover: next smoke (operator; not CI green):\n  \
         # After switch / units up:\n  \
         systemctl is-active sshd stalwart-mail surmount-management-ui\n  \
         # Free Stalwart product :443 before public Axum https (token on Domain B):\n  \
         #   just free-stalwart-public-443 -- --dry-run\n  \
         # Public HTTPS + MDWE (when BASE_URL live):\n  \
         #   SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://YOUR_DOMAIN just e2e-host"
    );
    if with_vw {
        println!(
            "  # S7b Vaultwarden (after token install + enable fragment + switch):\n  \
             #   systemctl is-active vaultwarden"
        );
    }
    println!(
        "\nGates checklist (docs/OPS.md): material -> install -> free :443 (optional) ->\n\
         units -> TLS -> :80 redirect -> MDWE -> optional hybrid -> VW token (S7b) ->\n\
         VW enable -> VW unit."
    );
}
