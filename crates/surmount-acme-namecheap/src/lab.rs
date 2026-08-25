//! Lab DNS-01 hook against a temp zone file (not production).

use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::cred::die;

pub fn run<I, S>(args: I) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match run_inner(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_inner<I, S>(args: I) -> anyhow::Result<u8>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args: Vec<String> = args
        .into_iter()
        .map(|s| s.into().to_string_lossy().into_owned())
        .collect();
    if !args.is_empty() {
        args.remove(0);
    }
    let zone = std::env::var("SURMOUNT_ACME_DNS_LAB_ZONE")
        .map_err(|_| die("acme-dns-hook-lab", "SURMOUNT_ACME_DNS_LAB_ZONE is required (path to lab zone file)"))?;
    let zone = PathBuf::from(zone);
    let parent = zone.parent().ok_or_else(|| {
        die("acme-dns-hook-lab", format!("zone parent directory missing: {}", zone.display()))
    })?;
    if !parent.is_dir() {
        return Err(die(
            "acme-dns-hook-lab",
            format!("zone parent directory missing: {}", parent.display()),
        ));
    }
    if !zone.exists() {
        fs::write(&zone, "").map_err(|e| die("acme-dns-hook-lab", format!("zone create: {e}")))?;
        let _ = fs::set_permissions(&zone, fs::Permissions::from_mode(0o600));
    }
    let meta = fs::symlink_metadata(&zone)
        .map_err(|_| die("acme-dns-hook-lab", format!("zone path is not a regular file: {}", zone.display())))?;
    if meta.file_type().is_symlink() {
        return Err(die(
            "acme-dns-hook-lab",
            format!("zone path must be a regular file (symlink refused): {}", zone.display()),
        ));
    }
    if args.is_empty() {
        return Err(die("acme-dns-hook-lab", "usage: set|clear|wait"));
    }
    match args[0].as_str() {
        "set" => {
            if args.len() < 3 {
                return Err(die("acme-dns-hook-lab", "set: missing fqdn"));
            }
            if args[1].chars().any(|c| c.is_control()) || args[2].chars().any(|c| c.is_control()) {
                return Err(die("acme-dns-hook-lab", "set: control characters refused"));
            }
            let mut recs = load(&zone)?;
            if let Some(r) = recs.iter_mut().find(|(f, _)| f == &args[1]) {
                r.1 = args[2].clone();
            } else {
                recs.push((args[1].clone(), args[2].clone()));
            }
            save(&zone, &recs)?;
            Ok(0)
        }
        "clear" => {
            if args.len() < 2 {
                return Err(die("acme-dns-hook-lab", "clear: missing fqdn"));
            }
            let mut recs = load(&zone)?;
            recs.retain(|(f, _)| f != &args[1]);
            save(&zone, &recs)?;
            Ok(0)
        }
        "wait" => {
            if args.len() < 3 {
                return Err(die("acme-dns-hook-lab", "wait: missing fqdn"));
            }
            let recs = load(&zone)?;
            if recs.iter().any(|(f, v)| f == &args[1] && v == &args[2]) {
                Ok(0)
            } else {
                Ok(1)
            }
        }
        other => Err(die("acme-dns-hook-lab", format!("unknown command: {other}"))),
    }
}

fn load(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let file = fs::File::open(path).map_err(|e| die("acme-dns-hook-lab", format!("zone: {e}")))?;
    let mut recs = Vec::new();
    for line in BufReader::new(file).lines() {
        let mut line = line.map_err(|e| die("acme-dns-hook-lab", format!("zone: {e}")))?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (fqdn, val) = line.split_once(' ').unwrap_or((line.as_str(), ""));
        recs.push((fqdn.to_string(), val.to_string()));
    }
    Ok(recs)
}

fn save(path: &Path, recs: &[(String, String)]) -> anyhow::Result<()> {
    let parent = path.parent().unwrap();
    let tmp = parent.join(".acme-dns-lab-zone.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| die("acme-dns-hook-lab", format!("zone tmp: {e}")))?;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        for (fqdn, val) in recs {
            writeln!(f, "{fqdn} {val}").map_err(|e| die("acme-dns-hook-lab", format!("zone: {e}")))?;
        }
    }
    fs::rename(&tmp, path).map_err(|e| die("acme-dns-hook-lab", format!("zone: {e}")))?;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(())
}
