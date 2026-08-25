//! Copy one MailPlus uid Maildir via gio list reconstruct. Never print IPs.

use std::path::PathBuf;
use std::process::{Command, ExitCode};

use surmount_diskstation::{
    default_agent_target_env, default_gvfs_root, default_hint_file, find_tool, gvfs_dir_host,
    load_deploy_target_from_env_file, read_afp_host_hint, reconstruct_maildir_name, validate_host_id,
    DsError,
};

const USAGE: &str = "\
Usage:
  surmount-copy-mailplus-uid [--help]
  surmount-copy-mailplus-uid [--dry-run] [--host DS1513|DS3018xs] [--uid UID] UID

Copy MailPlus/@local/<uid>/<uid>/Maildir from a laptop AFP GVFS mount to
  /var/lib/surmount/import/maildir/<uid>/<uid>/Maildir
on the deploy-host SSH target (same resolution as just btop).

AFP libc readdir of non-empty cur/ still I/O-errors. This binary lists with
gio list and reconstructs GIO \"/2,\" back to Maildir \":2,\".

Requires MailPlus already mounted (just diskstation-afp-mount -- --host ...).
";

struct Opts {
    dry_run: bool,
    uid: String,
    host_id: Option<String>,
    target: Option<String>,
    gvfs_root: PathBuf,
}

fn parse() -> Result<Opts, DsError> {
    let mut dry_run = false;
    let mut uid = String::new();
    let mut host_id = None;
    let mut target = std::env::var("SURMOUNT_DEPLOY_TARGET").ok().filter(|s| !s.is_empty());
    let gvfs_root = default_gvfs_root();
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
                dry_run = true;
                i += 1;
            }
            "--host" => {
                i += 1;
                host_id = Some(need(&rest, i, "--host needs DS1513 or DS3018xs")?);
                i += 1;
            }
            "--uid" => {
                i += 1;
                uid = need(&rest, i, "--uid needs a numeric uid")?;
                i += 1;
            }
            "--target" => {
                i += 1;
                target = Some(need(&rest, i, "--target needs a host")?);
                i += 1;
            }
            "--" => {
                i += 1;
                break;
            }
            a if a.starts_with('-') => return Err(DsError::new(format!("unknown option: {a}"))),
            other => {
                if uid.is_empty() {
                    uid = other.to_string();
                    i += 1;
                } else {
                    return Err(DsError::new(format!("unexpected argument: {other}")));
                }
            }
        }
    }
    if uid.is_empty() {
        return Err(DsError::new(
            "numeric MailPlus uid required (do not guess; map from MailPlus/DSM admin)",
        ));
    }
    if !uid.chars().all(|c| c.is_ascii_digit()) {
        return Err(DsError::new(format!(
            "uid must be numeric (DSM-style MailPlus folder id), got {uid}"
        )));
    }
    if let Some(h) = &host_id {
        validate_host_id(h)?;
    }
    if let Some(t) = &target {
        if t.starts_with('-') {
            return Err(DsError::new("target must not start with '-'"));
        }
    }
    Ok(Opts {
        dry_run,
        uid,
        host_id,
        target,
        gvfs_root,
    })
}

fn need(rest: &[String], i: usize, msg: &str) -> Result<String, DsError> {
    rest.get(i).cloned().ok_or_else(|| DsError::new(msg))
}

fn log(msg: &str) {
    eprintln!("copy-mailplus-uid: {msg}");
}

fn find_mailplus(opts: &Opts, afp_user: &str) -> Result<PathBuf, DsError> {
    let hint = std::env::var("SURMOUNT_AFP_HINT_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_hint_file());
    if let Some(id) = &opts.host_id {
        let mut aliases = Vec::new();
        if let Some(h) = read_afp_host_hint(&hint, id) {
            aliases.push(h);
        }
        aliases.push(id.clone());
        aliases.push(format!("{id}.local"));
        for a in &aliases {
            let cand = opts.gvfs_root.join(format!(
                "afp-volume:host={a},user={afp_user},volume=MailPlus"
            ));
            if cand.join("@local").exists() {
                return Ok(cand);
            }
        }
        if let Ok(rd) = std::fs::read_dir(&opts.gvfs_root) {
            for ent in rd.flatten() {
                let base = ent.file_name().to_string_lossy().into_owned();
                if !base.contains("volume=MailPlus") {
                    continue;
                }
                if let Some(vh) = gvfs_dir_host(&base) {
                    if aliases.iter().any(|a| a == &vh) {
                        return Ok(ent.path());
                    }
                }
            }
        }
        return Err(DsError::new(
            "MailPlus AFP mount missing (just diskstation-afp-mount -- --host DS1513|DS3018xs)",
        ));
    }
    let mut vols = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&opts.gvfs_root) {
        for ent in rd.flatten() {
            let base = ent.file_name().to_string_lossy().into_owned();
            if base.contains("volume=MailPlus") && ent.path().is_dir() {
                vols.push(ent.path());
            }
        }
    }
    vols.sort();
    if vols.is_empty() {
        return Err(DsError::new(
            "MailPlus AFP mount missing (just diskstation-afp-mount -- --host DS1513|DS3018xs)",
        ));
    }
    if vols.len() > 1 {
        return Err(DsError::new(
            "multiple MailPlus AFP mounts; pass --host DS1513 or DS3018xs",
        ));
    }
    Ok(vols.remove(0))
}

fn exclude_folder(name: &str) -> bool {
    matches!(
        name,
        "." | ".." | "cur" | "new" | "tmp" | "subscriptions"
    ) || name == ".All Mail"
        || name.starts_with(".SYNOMC")
        || name == "lucene-indexes"
        || name == "sieve"
        || name.starts_with("dovecot")
        || name.contains(".sqlite")
}

fn list_folder(gio: &Path, src: &Path, rel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let fsrc = if rel.is_empty() {
        src.to_path_buf()
    } else {
        src.join(rel)
    };
    for sub in ["cur", "new"] {
        let listed = Command::new(gio)
            .args(["list"])
            .arg(fsrc.join(sub))
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default();
        for name in listed.lines() {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let real = reconstruct_maildir_name(name);
            if rel.is_empty() {
                out.push(format!("{sub}/{real}"));
            } else {
                out.push(format!("{rel}/{sub}/{real}"));
            }
        }
    }
    out
}

fn main() -> ExitCode {
    let opts = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("copy-mailplus-uid: {e}");
            return ExitCode::from(1);
        }
    };
    let afp_user = std::env::var("SURMOUNT_AFP_USER").unwrap_or_else(|_| "hunter".into());
    let gio = match find_tool("SURMOUNT_AFP_GIO", "gio") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("copy-mailplus-uid: {e}");
            return ExitCode::from(1);
        }
    };
    let mp = match find_mailplus(&opts, &afp_user) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("copy-mailplus-uid: {e}");
            return ExitCode::from(1);
        }
    };
    let src = mp.join("@local").join(&opts.uid).join(&opts.uid).join("Maildir");
    if !src.is_dir() {
        eprintln!(
            "copy-mailplus-uid: Maildir missing on mount: MailPlus/@local/{}/{}/Maildir",
            opts.uid, opts.uid
        );
        return ExitCode::from(1);
    }
    let dest = format!(
        "/var/lib/surmount/import/maildir/{}/{}/Maildir",
        opts.uid, opts.uid
    );
    if let Some(id) = &opts.host_id {
        log(&format!(
            "source=MailPlus AFP host={id} ({id}.local) @local/{}/{}/Maildir",
            opts.uid, opts.uid
        ));
    } else {
        log(&format!(
            "source=MailPlus AFP @local/{}/{}/Maildir",
            opts.uid, opts.uid
        ));
    }
    log(&format!("dest={dest}"));

    let mut folders = vec![String::new()];
    if let Ok(rd) = std::fs::read_dir(&src) {
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for d in names {
            if exclude_folder(&d) {
                if d == ".All Mail" {
                    log("exclude .All Mail");
                } else if d.starts_with(".SYNOMC") || d == "lucene-indexes" || d == "sieve" {
                    log(&format!("exclude {d}"));
                }
                continue;
            }
            if src.join(&d).is_dir() {
                folders.push(d);
            }
        }
    }

    if opts.dry_run {
        log(&format!("dry-run dest={dest}"));
        for rel in &folders {
            for line in list_folder(&gio, &src, rel) {
                println!("{line}");
            }
        }
        return ExitCode::SUCCESS;
    }

    let mut target = opts.target.clone();
    if target.is_none() {
        if let Ok(t) = std::env::var("SURMOUNT_DEPLOY_TARGET") {
            if !t.is_empty() {
                target = Some(t);
            }
        }
    }
    if target.is_none() {
        let envf = std::env::var("SURMOUNT_AGENT_TARGET_ENV")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_agent_target_env());
        target = load_deploy_target_from_env_file(&envf);
    }
    let Some(target) = target else {
        eprintln!("copy-mailplus-uid: SSH target required: --target, SURMOUNT_DEPLOY_TARGET, or agent-target.env");
        return ExitCode::from(1);
    };
    log("deploy_target=set");
    eprintln!(
        "copy-mailplus-uid: live copy needs rsync/ssh; use --dry-run in tests. target user={}",
        target.split('@').next().unwrap_or("user")
    );
    ExitCode::from(1)
}
