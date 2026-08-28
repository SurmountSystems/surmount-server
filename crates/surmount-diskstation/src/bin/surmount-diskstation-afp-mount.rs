//! Mount DiskStation AFP via gio + Secret Service. Password never on argv.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use surmount_diskstation::{
    DsError, default_gvfs_root, default_hint_file, find_tool, gvfs_dir_host, read_afp_host_hint,
    validate_host_id,
};

const USAGE: &str = "\
Usage:
  surmount-diskstation-afp-mount [--help]
  surmount-diskstation-afp-mount --host DS1513 [mount|unmount|status] [options]
  surmount-diskstation-afp-mount --host DS3018xs [mount|unmount|status] [options]

Mount AFP share MailPlus on the office LAN using the laptop GNOME Secret
Service item (kind synology-afp). One item per NAS. --host is required and
must be DS1513 (5-bay) or DS3018xs (6-bay). Reuses an existing GVFS dir
matching that host and *volume=MailPlus* when already mounted.

Documented AFP URI examples (mDNS hostname default; IPv4 allowed at runtime):
  afp://hunter@DS1513.local/MailPlus
  afp://hunter@DS3018xs.local/MailPlus

Lookup (never put the password on argv):
  secret-tool lookup surmount.kind synology-afp surmount.host DS1513
  secret-tool lookup surmount.kind synology-afp surmount.host DS3018xs

Intake (operator pastes LAN password, no-echo; passwords may differ):
  just secrets-prompt -- synology-afp --host DS1513 --secret-service --no-staging
";

struct Opts {
    cmd: String,
    host_id: String,
    shares: Vec<String>,
    afp_host: String,
    afp_user: String,
    secret_host: String,
    uri_override: Option<String>,
    gvfs_root: PathBuf,
    dry_run: bool,
    hint_file: PathBuf,
}

fn parse() -> Result<Opts, DsError> {
    let mut cmd = "mount".to_string();
    let mut host_id = String::new();
    let mut shares = Vec::new();
    let mut afp_host = std::env::var("SURMOUNT_AFP_HOST").unwrap_or_default();
    let mut afp_user = std::env::var("SURMOUNT_AFP_USER").unwrap_or_else(|_| "hunter".into());
    let mut secret_host = std::env::var("SURMOUNT_AFP_SECRET_HOST").unwrap_or_default();
    let mut uri_override = None;
    let mut gvfs_root = default_gvfs_root();
    let mut dry_run = false;
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
            "mount" | "unmount" | "status" => {
                cmd = rest[i].clone();
                i += 1;
            }
            "--host" => {
                i += 1;
                host_id = need(&rest, i, "--host needs DS1513 or DS3018xs")?;
                i += 1;
            }
            "--share" => {
                i += 1;
                shares.push(need(&rest, i, "--share needs a name")?);
                i += 1;
            }
            "--afp-host" => {
                i += 1;
                afp_host = need(&rest, i, "--afp-host needs a hostname or IPv4")?;
                i += 1;
            }
            "--afp-user" => {
                i += 1;
                afp_user = need(&rest, i, "--afp-user needs a name")?;
                i += 1;
            }
            "--secret-host" => {
                i += 1;
                secret_host = need(&rest, i, "--secret-host needs an id")?;
                i += 1;
            }
            "--uri" => {
                i += 1;
                uri_override = Some(need(&rest, i, "--uri needs a URI")?);
                i += 1;
            }
            "--gvfs-root" => {
                i += 1;
                gvfs_root = PathBuf::from(need(&rest, i, "--gvfs-root needs a directory")?);
                i += 1;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            other => return Err(DsError::new(format!("unknown option: {other}"))),
        }
    }
    if host_id.is_empty() {
        return Err(DsError::new("--host is required (DS1513 or DS3018xs)"));
    }
    validate_host_id(&host_id)?;
    let hint_file = std::env::var("SURMOUNT_AFP_HINT_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_hint_file());
    if afp_host.is_empty() {
        afp_host =
            read_afp_host_hint(&hint_file, &host_id).unwrap_or_else(|| format!("{host_id}.local"));
    }
    if secret_host.is_empty() {
        secret_host = host_id.clone();
    } else {
        validate_host_id(&secret_host)?;
    }
    if shares.is_empty() {
        shares.push("MailPlus".into());
    }
    Ok(Opts {
        cmd,
        host_id,
        shares,
        afp_host,
        afp_user,
        secret_host,
        uri_override,
        gvfs_root,
        dry_run,
        hint_file,
    })
}

fn need(rest: &[String], i: usize, msg: &str) -> Result<String, DsError> {
    rest.get(i).cloned().ok_or_else(|| DsError::new(msg))
}

fn afp_uri(opts: &Opts, share: &str) -> String {
    if let Some(u) = &opts.uri_override {
        u.clone()
    } else {
        format!("afp://{}@{}/{}", opts.afp_user, opts.afp_host, share)
    }
}

fn host_aliases(opts: &Opts) -> Vec<String> {
    let mut a = vec![opts.host_id.clone(), format!("{}.local", opts.host_id)];
    if !opts.afp_host.is_empty() {
        a.push(opts.afp_host.clone());
    }
    if let Some(h) = read_afp_host_hint(&opts.hint_file, &opts.host_id) {
        a.push(h);
    }
    a
}

fn find_share_volume(opts: &Opts, share: &str) -> Option<PathBuf> {
    let aliases = host_aliases(opts);
    let rd = std::fs::read_dir(&opts.gvfs_root).ok()?;
    for ent in rd.flatten() {
        if !ent.path().is_dir() {
            continue;
        }
        let base = ent.file_name().to_string_lossy().into_owned();
        if !base.contains(&format!("volume={share}")) {
            continue;
        }
        if let Some(vol_host) = gvfs_dir_host(&base)
            && aliases.iter().any(|a| a == &vol_host)
        {
            return Some(ent.path());
        }
    }
    None
}

fn log(msg: &str) {
    eprintln!("diskstation-afp-mount: {msg}");
}

fn lookup_password(st: &Path, secret_host: &str) -> Result<Vec<u8>, DsError> {
    let out = Command::new(st)
        .args([
            "lookup",
            "surmount.kind",
            "synology-afp",
            "surmount.host",
            secret_host,
        ])
        .output()
        .map_err(|e| DsError::new(e.to_string()))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err(DsError::new(format!(
            "Secret Service lookup failed for synology-afp host={secret_host}. Next: just secrets-prompt -- synology-afp --host {secret_host} --secret-service --no-staging"
        )));
    }
    Ok(out.stdout)
}

fn ensure_network_password(st: &Path, server: &str, user: &str, pass: &[u8]) {
    let lookup = Command::new(st)
        .args([
            "lookup",
            "xdg:schema",
            "org.gnome.keyring.NetworkPassword",
            "protocol",
            "afp",
            "server",
            server,
            "user",
            user,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if lookup.map(|s| s.success()).unwrap_or(false) {
        return;
    }
    let mut child = match Command::new(st)
        .args([
            "store",
            &format!("--label=Password for {user} on {server}"),
            "xdg:schema",
            "org.gnome.keyring.NetworkPassword",
            "protocol",
            "afp",
            "server",
            server,
            "user",
            user,
        ])
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(pass);
    }
    let _ = child.wait();
    wipe_paste();
    log(&format!(
        "stored GNOME NetworkPassword protocol=afp user={user} server={server}"
    ));
}

fn wipe_paste() {
    for (bin, args) in [
        ("xclip", vec!["-selection", "clipboard"]),
        ("xclip", vec!["-selection", "primary"]),
        ("pbcopy", vec![]),
        ("wl-copy", vec!["--clear"]),
    ] {
        if let Some(p) = surmount_diskstation::which(bin) {
            let mut c = Command::new(p);
            c.args(&args);
            if bin != "wl-copy" {
                c.stdin(Stdio::piped());
            }
            if let Ok(mut child) = c.spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"");
                }
                let _ = child.wait();
            }
        }
    }
}

fn main() -> ExitCode {
    let opts = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("diskstation-afp-mount: {e}");
            return ExitCode::from(1);
        }
    };
    let gio = match find_tool("SURMOUNT_AFP_GIO", "gio") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("diskstation-afp-mount: {e}");
            return ExitCode::from(1);
        }
    };
    match opts.cmd.as_str() {
        "status" => {
            for share in &opts.shares {
                if find_share_volume(&opts, share).is_some() {
                    log(&format!(
                        "status host={} share={share} mounted=yes",
                        opts.host_id
                    ));
                } else {
                    log(&format!(
                        "status host={} share={share} mounted=no",
                        opts.host_id
                    ));
                }
            }
        }
        "unmount" => {
            for share in &opts.shares {
                let uri = afp_uri(&opts, share);
                if opts.dry_run {
                    log(&format!("dry-run unmount host={} uri={uri}", opts.host_id));
                    continue;
                }
                if let Some(vol) = find_share_volume(&opts, share) {
                    let _ = Command::new(&gio).args(["mount", "-u"]).arg(&vol).status();
                } else {
                    let _ = Command::new(&gio).args(["mount", "-u", &uri]).status();
                }
                log(&format!("unmounted host={} share={share}", opts.host_id));
            }
        }
        "mount" => {
            let st = match find_tool("SURMOUNT_SECRET_TOOL", "secret-tool") {
                Ok(p) => p,
                Err(e) if !opts.dry_run => {
                    eprintln!("diskstation-afp-mount: {e}");
                    return ExitCode::from(1);
                }
                Err(_) => PathBuf::from("secret-tool"),
            };
            for share in &opts.shares {
                let uri = afp_uri(&opts, share);
                if opts.dry_run {
                    log(&format!(
                        "dry-run mount uri={uri} lookup=surmount.kind synology-afp surmount.host {}",
                        opts.secret_host
                    ));
                    continue;
                }
                if find_share_volume(&opts, share).is_some() {
                    log(&format!(
                        "reuse already mounted host={} share={share}",
                        opts.host_id
                    ));
                    continue;
                }
                let pass = match lookup_password(&st, &opts.secret_host) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("diskstation-afp-mount: {e}");
                        return ExitCode::from(1);
                    }
                };
                let mut child = match Command::new(&gio)
                    .args(["mount", &uri])
                    .stdin(Stdio::piped())
                    .spawn()
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("diskstation-afp-mount: gio mount failed: {e}");
                        return ExitCode::from(1);
                    }
                };
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(&pass);
                }
                let ok = child.wait().map(|s| s.success()).unwrap_or(false);
                if !ok {
                    eprintln!(
                        "diskstation-afp-mount: gio mount failed for {uri} (password not logged)"
                    );
                    return ExitCode::from(1);
                }
                for server in [&opts.host_id, &format!("{}.local", opts.host_id)] {
                    ensure_network_password(&st, server, &opts.afp_user, &pass);
                }
                if find_share_volume(&opts, share).is_some() {
                    log(&format!("mounted host={} share={share}", opts.host_id));
                } else {
                    log(&format!(
                        "gio mount returned 0 for {uri}; GVFS dir not found yet under {}",
                        opts.gvfs_root.display()
                    ));
                }
            }
        }
        other => {
            eprintln!("diskstation-afp-mount: unknown command {other}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}
