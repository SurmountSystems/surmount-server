//! Publish apex/www plus extra vhosts. Not a NixOS generation.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use surmount_static_sites::{
    copy_tree, redact_ipv4, resolve_deploy_target, ssh_rsync_e, target_label, which, PROVEN_SLUGS,
    SitesError,
};

const USAGE: &str = "\
Usage:
  surmount-deploy-static-sites [--dry-run|--live] [options]
  just deploy
  nix run .#surmount-deploy-static-sites

Publish static sites this mail host serves. Apex/www is the packaged
github:SurmountSystems/site tree. Extra vhosts are the proven DS3018xs
slugs under /var/lib/surmount/static-sites. This is not a NixOS
generation; that is just deploy-host.

Target resolution (first wins):
  1. --target USER@HOST
  2. SURMOUNT_DEPLOY_TARGET
  3. agent-target.env

Options:
  --dry-run          Plan only; do not copy or restart
  --live             Copy files (default)
  --target USER@H    SSH target (must not start with '-')
  --site-root DIR    Packaged site tree (else SURMOUNT_PUBLIC_SITE_ROOT)
  --extra-from DIR   Local extra-vhost parent (skip if missing)
  --apex-dest DIR    Remote/local apex dest (default /var/lib/surmount/public-site)
  --extra-dest DIR   Remote/local extra parent (default /var/lib/surmount/static-sites)
  --local-dest DIR   Hermetic: copy under DIR/public-site and DIR/static-sites (no SSH)
  --no-restart       Do not write the apex-root systemd drop-in or restart the edge
  --restart-unit N   systemd unit (default surmount-management-ui)
  -h, --help         Show this help
";

struct Opts {
    live: bool,
    target: Option<String>,
    site_root: PathBuf,
    extra_from: PathBuf,
    apex_dest: PathBuf,
    extra_dest: PathBuf,
    local_dest: Option<PathBuf>,
    restart: bool,
    unit: String,
    rsync: String,
    ssh: String,
}

fn parse() -> Result<Opts, SitesError> {
    let mut live = true;
    let mut target = None;
    let mut site_root = std::env::var("SURMOUNT_PUBLIC_SITE_ROOT")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let mut extra_from = PathBuf::from(
        std::env::var("SURMOUNT_STATIC_SITES_EXTRA_FROM")
            .unwrap_or_else(|_| "/var/lib/surmount/static-sites".into()),
    );
    let mut apex_dest = PathBuf::from("/var/lib/surmount/public-site");
    let mut extra_dest = PathBuf::from("/var/lib/surmount/static-sites");
    let mut local_dest = None;
    let mut restart = true;
    let mut unit = "surmount-management-ui".to_string();
    let mut args = std::env::args();
    let _ = args.next();
    let rest: Vec<String> = args.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--dry-run" => {
                live = false;
                i += 1;
            }
            "--live" => {
                live = true;
                i += 1;
            }
            "--target" => {
                i += 1;
                target = Some(need(&rest, i, "--target requires USER@HOST")?);
                i += 1;
            }
            "--site-root" => {
                i += 1;
                site_root = Some(PathBuf::from(need(&rest, i, "--site-root requires a directory")?));
                i += 1;
            }
            "--extra-from" => {
                i += 1;
                extra_from = PathBuf::from(need(&rest, i, "--extra-from requires a directory")?);
                i += 1;
            }
            "--apex-dest" => {
                i += 1;
                apex_dest = PathBuf::from(need(&rest, i, "--apex-dest requires a directory")?);
                i += 1;
            }
            "--extra-dest" => {
                i += 1;
                extra_dest = PathBuf::from(need(&rest, i, "--extra-dest requires a directory")?);
                i += 1;
            }
            "--local-dest" => {
                i += 1;
                local_dest = Some(PathBuf::from(need(
                    &rest,
                    i,
                    "--local-dest requires a directory",
                )?));
                i += 1;
            }
            "--no-restart" => {
                restart = false;
                i += 1;
            }
            "--restart-unit" => {
                i += 1;
                unit = need(&rest, i, "--restart-unit requires a unit name")?;
                i += 1;
            }
            other => return Err(SitesError::new(format!("unknown argument: {other} (try --help)"))),
        }
    }
    let site_root = site_root.ok_or_else(|| {
        SitesError::new(
            "site root required: --site-root or SURMOUNT_PUBLIC_SITE_ROOT (nix run .#surmount-deploy-static-sites sets this)",
        )
    })?;
    Ok(Opts {
        live,
        target,
        site_root,
        extra_from,
        apex_dest,
        extra_dest,
        local_dest,
        restart,
        unit,
        rsync: std::env::var("SURMOUNT_RSYNC").unwrap_or_else(|_| "rsync".into()),
        ssh: std::env::var("SURMOUNT_DEPLOY_SSH").unwrap_or_else(|_| "ssh".into()),
    })
}

fn need(rest: &[String], i: usize, msg: &str) -> Result<String, SitesError> {
    rest.get(i).cloned().ok_or_else(|| SitesError::new(msg))
}

fn main() -> ExitCode {
    let mut opts = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("deploy-static-sites: {e}");
            return ExitCode::from(1);
        }
    };
    if let Some(local) = &opts.local_dest {
        if !local.is_absolute() {
            eprintln!("deploy-static-sites: --local-dest must be an absolute path");
            return ExitCode::from(1);
        }
        opts.apex_dest = local.join("public-site");
        opts.extra_dest = local.join("static-sites");
    }
    if !opts.site_root.is_dir() {
        eprintln!(
            "deploy-static-sites: site root missing: {}",
            redact_ipv4(&opts.site_root.display().to_string())
        );
        return ExitCode::from(1);
    }
    if !opts.site_root.join("index.html").is_file() {
        eprintln!(
            "deploy-static-sites: site root has no index.html: {}",
            redact_ipv4(&opts.site_root.display().to_string())
        );
        return ExitCode::from(1);
    }
    let target = if opts.local_dest.is_none() {
        match resolve_deploy_target(opts.target.as_deref()) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("deploy-static-sites: {e}");
                return ExitCode::from(1);
            }
        }
    } else {
        None
    };

    println!(
        "deploy-static-sites: mode={}",
        if opts.live { "LIVE" } else { "dry-run" }
    );
    println!(
        "  apex-src={}",
        redact_ipv4(&opts.site_root.display().to_string())
    );
    println!("  apex-dest={}", opts.apex_dest.display());
    if let Some(t) = &target {
        println!("  target={}", target_label(t));
    }
    println!("  PLAN     apex/www  -> {}/", opts.apex_dest.display());

    let have_extra = opts.extra_from.is_dir();
    if have_extra {
        for slug in PROVEN_SLUGS {
            let src = opts.extra_from.join(slug);
            if !src.is_dir() {
                println!("  SKIP     {slug}  (not under extra-from)");
                continue;
            }
            if !src.join("index.html").is_file() {
                println!("  SKIP     {slug}  (no index.html)");
                continue;
            }
            println!("  PLAN     {slug}  -> {}/{slug}/", opts.extra_dest.display());
        }
    } else {
        println!(
            "  SKIP     extra vhosts (extra-from missing: {})",
            redact_ipv4(&opts.extra_from.display().to_string())
        );
    }
    if opts.restart && opts.local_dest.is_none() {
        println!(
            "  PLAN     systemd drop-in + restart {} (apex root {})",
            opts.unit,
            opts.apex_dest.display()
        );
    }
    if !opts.live {
        println!("deploy-static-sites: dry-run only. Pass --live (or just deploy) to copy.");
        return ExitCode::SUCCESS;
    }

    if opts.local_dest.is_some() {
        if let Err(e) = copy_tree(&opts.site_root, &opts.apex_dest, &opts.rsync) {
            eprintln!("deploy-static-sites: {e}");
            return ExitCode::from(1);
        }
        println!("  COPIED   apex/www");
        if have_extra {
            for slug in PROVEN_SLUGS {
                let src = opts.extra_from.join(slug);
                if src.is_dir() && src.join("index.html").is_file() {
                    if let Err(e) = copy_tree(&src, &opts.extra_dest.join(slug), &opts.rsync) {
                        eprintln!("deploy-static-sites: {e}");
                        return ExitCode::from(1);
                    }
                    println!("  COPIED   {slug}");
                }
            }
        }
        println!("deploy-static-sites: local copy done");
        return ExitCode::SUCCESS;
    }

    let target = target.unwrap();
    if !which(&opts.rsync) {
        eprintln!("deploy-static-sites: rsync is required");
        return ExitCode::from(1);
    }
    if !which(&opts.ssh) {
        eprintln!("deploy-static-sites: ssh is required");
        return ExitCode::from(1);
    }
    let e = ssh_rsync_e(&opts.ssh);
    let dest = format!("{}:{}/", target, opts.apex_dest.display());
    let st = Command::new(&opts.rsync)
        .args(["-a", "--delete", "-e", &e, "--"])
        .arg(format!("{}/", opts.site_root.display()))
        .arg(&dest)
        .status();
    if !st.map(|s| s.success()).unwrap_or(false) {
        eprintln!("deploy-static-sites: rsync apex failed");
        return ExitCode::from(1);
    }
    println!("  COPIED   apex/www");
    if have_extra {
        for slug in PROVEN_SLUGS {
            let src = opts.extra_from.join(slug);
            if !(src.is_dir() && src.join("index.html").is_file()) {
                continue;
            }
            let dest = format!("{}:{}/{slug}/", target, opts.extra_dest.display());
            let st = Command::new(&opts.rsync)
                .args(["-a", "--delete", "-e", &e, "--"])
                .arg(format!("{}/", src.display()))
                .arg(&dest)
                .status();
            if !st.map(|s| s.success()).unwrap_or(false) {
                eprintln!("deploy-static-sites: rsync {slug} failed");
                return ExitCode::from(1);
            }
            println!("  COPIED   {slug}");
        }
    }
    if opts.restart {
        let apex = opts.apex_dest.display().to_string();
        let unit = opts.unit.clone();
        let remote = format!(
            "set -euo pipefail; apex={apex:?}; unit={unit:?}; drop_dir=\"/run/systemd/system/${{unit}}.service.d\"; mkdir -p -- \"$apex\" \"$drop_dir\"; printf '%s\\n' '[Service]' \"Environment=SURMOUNT_APEX_PUBLIC_ROOT=$apex\" \"ReadOnlyPaths=$apex\" >\"$drop_dir/apex-public-root.conf\"; systemctl daemon-reload; systemctl restart \"$unit\""
        );
        let st = Command::new(&opts.ssh)
            .args([
                "-o",
                "BatchMode=yes",
                "-o",
                "PreferredAuthentications=publickey",
                "-o",
                "PasswordAuthentication=no",
                "-o",
                "KbdInteractiveAuthentication=no",
                "--",
                &target,
                "bash",
                "-c",
                &remote,
            ])
            .status();
        if !st.map(|s| s.success()).unwrap_or(false) {
            eprintln!("deploy-static-sites: restart failed");
            return ExitCode::from(1);
        }
        println!("  RESTART  {}", opts.unit);
    }
    println!("deploy-static-sites: done");
    ExitCode::SUCCESS
}
