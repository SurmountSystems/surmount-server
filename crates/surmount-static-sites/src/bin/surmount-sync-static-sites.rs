//! Copy proven DS3018xs static HTML trees from AFP GVFS to staging.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use surmount_static_sites::{
    copy_tree, redact_gvfs_path, validate_target, which, PROVEN_SLUGS, SitesError,
};

const USAGE: &str = "\
Usage:
  surmount-sync-static-sites [--dry-run|--live] [options]

Copy proven DS3018xs static HTML trees (sites/<slug>/) to
/var/lib/surmount/static-sites/<slug> on this machine or via rsync to
the mail host. Default is dry-run.

Options:
  --dest DIR       Local destination parent (default /var/lib/surmount/static-sites)
  --target USER@H  rsync each slug to USER@H:DEST/<slug>/ (no IP printed)
  --gvfs-root DIR  GVFS root (default /run/user/$UID/gvfs)
  --sites-dir DIR  Explicit sites share directory (tests; skips GVFS walk)
  --host ID        NAS label (default DS3018xs; must stay DS3018xs)
  --share NAME     AFP share name to match (default sites)
  --live           Copy files
  --dry-run        Default; print plan only
  -h, --help       Show this help

Proven slugs only (skip Ghost/Grav/uncertain). Mount first:
  just diskstation-afp-mount -- --host DS3018xs --share sites
";

struct Opts {
    live: bool,
    dest: PathBuf,
    target: Option<String>,
    gvfs_root: PathBuf,
    sites_dir: Option<PathBuf>,
    host_id: String,
    share: String,
    rsync: String,
}

fn parse() -> Result<Opts, SitesError> {
    let uid = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("Uid:").and_then(|r| r.split_whitespace().next().map(|x| x.to_string())))
        })
        .unwrap_or_else(|| "1000".into());
    let mut live = false;
    let mut dest = PathBuf::from(
        std::env::var("SURMOUNT_STATIC_SITES_DEST")
            .unwrap_or_else(|_| "/var/lib/surmount/static-sites".into()),
    );
    let mut target = None;
    let mut gvfs_root = PathBuf::from(
        std::env::var("SURMOUNT_AFP_GVFS_ROOT").unwrap_or_else(|_| format!("/run/user/{uid}/gvfs")),
    );
    let mut sites_dir = None;
    let mut host_id = "DS3018xs".to_string();
    let mut share = "sites".to_string();
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
            "--live" => {
                live = true;
                i += 1;
            }
            "--dry-run" => {
                live = false;
                i += 1;
            }
            "--dest" => {
                i += 1;
                dest = PathBuf::from(need(&rest, i, "--dest requires a directory")?);
                i += 1;
            }
            "--target" => {
                i += 1;
                target = Some(need(&rest, i, "--target requires USER@HOST")?);
                i += 1;
            }
            "--gvfs-root" => {
                i += 1;
                gvfs_root = PathBuf::from(need(&rest, i, "--gvfs-root requires a directory")?);
                i += 1;
            }
            "--sites-dir" => {
                i += 1;
                sites_dir = Some(PathBuf::from(need(
                    &rest,
                    i,
                    "--sites-dir requires a directory",
                )?));
                i += 1;
            }
            "--host" => {
                i += 1;
                host_id = need(&rest, i, "--host requires an id")?;
                i += 1;
            }
            "--share" => {
                i += 1;
                share = need(&rest, i, "--share requires a name")?;
                i += 1;
            }
            other => return Err(SitesError::new(format!("unknown argument: {other} (try --help)"))),
        }
    }
    if host_id != "DS3018xs" {
        return Err(SitesError::new(format!(
            "this sync is DS3018xs only (got {host_id})"
        )));
    }
    Ok(Opts {
        live,
        dest,
        target,
        gvfs_root,
        sites_dir,
        host_id,
        share,
        rsync: std::env::var("SURMOUNT_RSYNC").unwrap_or_else(|_| "rsync".into()),
    })
}

fn need(rest: &[String], i: usize, msg: &str) -> Result<String, SitesError> {
    rest.get(i).cloned().ok_or_else(|| SitesError::new(msg))
}

fn find_sites_dir(opts: &Opts) -> Result<PathBuf, SitesError> {
    if let Some(d) = &opts.sites_dir {
        if !d.is_dir() {
            return Err(SitesError::new(format!(
                "sites dir missing: {}",
                redact_gvfs_path(&d.display().to_string(), &opts.share)
            )));
        }
        return Ok(d.clone());
    }
    if !opts.gvfs_root.is_dir() {
        return Err(SitesError::new(format!(
            "GVFS root missing. Mount first: just diskstation-afp-mount -- --host {} --share {}",
            opts.host_id, opts.share
        )));
    }
    let share_lc = opts.share.to_ascii_lowercase();
    if let Ok(rd) = std::fs::read_dir(&opts.gvfs_root) {
        for ent in rd.flatten() {
            if !ent.path().is_dir() {
                continue;
            }
            let base = ent.file_name().to_string_lossy().into_owned();
            if let Some(vol) = base.split("volume=").nth(1) {
                if vol.to_ascii_lowercase() == share_lc {
                    return Ok(ent.path());
                }
            }
        }
    }
    Err(SitesError::new(format!(
        "no GVFS volume={} under GVFS root. Mount: just diskstation-afp-mount -- --host {} --share {}",
        opts.share, opts.host_id, opts.share
    )))
}

fn main() -> ExitCode {
    let opts = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("sync-static-sites-from-ds3018xs: {e}");
            return ExitCode::from(1);
        }
    };
    if !opts.dest.is_absolute() {
        eprintln!("sync-static-sites-from-ds3018xs: --dest must be an absolute path");
        return ExitCode::from(1);
    }
    if let Some(t) = &opts.target {
        if let Err(e) = validate_target(t) {
            eprintln!("sync-static-sites-from-ds3018xs: {e}");
            return ExitCode::from(1);
        }
        if !t.contains('@') {
            eprintln!(
                "sync-static-sites-from-ds3018xs: --target must be USER@HOST (hostname, never paste a provisioned IP into docs)"
            );
            return ExitCode::from(1);
        }
    }
    let src_root = match find_sites_dir(&opts) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("sync-static-sites-from-ds3018xs: {e}");
            return ExitCode::from(1);
        }
    };
    println!(
        "sync-static-sites-from-ds3018xs: source={} dest={}",
        redact_gvfs_path(&src_root.display().to_string(), &opts.share),
        opts.dest.display()
    );
    if let Some(t) = &opts.target {
        let user = t.split('@').next().unwrap_or("user");
        println!("  target={user}@<mail-host>");
    }
    println!(
        "  mode: {}",
        if opts.live { "LIVE" } else { "dry-run" }
    );

    let mut missing = false;
    for slug in PROVEN_SLUGS {
        let src = src_root.join(slug);
        if !src.is_dir() {
            println!(
                "  MISSING  {slug}  (folder {slug} not on DS3018xs GVFS {})",
                opts.share
            );
            missing = true;
            continue;
        }
        if !src.join("index.html").is_file() {
            println!("  SKIP     {slug}  (no index.html)");
            continue;
        }
        println!("  PLAN     {slug}  -> {}/{slug}/", opts.dest.display());
        if !opts.live {
            continue;
        }
        if let Some(t) = &opts.target {
            if !which(&opts.rsync) {
                eprintln!("sync-static-sites-from-ds3018xs: rsync is required for --target");
                return ExitCode::from(1);
            }
            let dest = format!("{}:{}/{slug}/", t, opts.dest.display());
            let st = Command::new(&opts.rsync)
                .args(["-a", "--no-owner", "--no-group", "--"])
                .arg(format!("{}/", src.display()))
                .arg(&dest)
                .status();
            if !st.map(|s| s.success()).unwrap_or(false) {
                eprintln!("sync-static-sites-from-ds3018xs: rsync {slug} failed");
                return ExitCode::from(1);
            }
        } else if let Err(e) = copy_tree(&src, &opts.dest.join(slug), &opts.rsync) {
            eprintln!("sync-static-sites-from-ds3018xs: {e}");
            return ExitCode::from(1);
        }
        println!("  COPIED   {slug}");
    }
    if missing {
        eprintln!(
            "sync-static-sites-from-ds3018xs: some proven folders were missing (see MISSING lines)"
        );
    }
    if !opts.live {
        println!("sync-static-sites-from-ds3018xs: dry-run only. Pass --live to copy.");
    }
    ExitCode::SUCCESS
}
