//! Map challenge FQDN to a per-zone env, then exec the inner hook.

use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cred::die;

pub fn run<I, S>(args: I) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match run_inner(args) {
        Ok(never) => never,
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
    let mut argv: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if argv.is_empty() {
        return Err(die("acme-dns-hook-namecheap-dispatch", "usage: set|clear|wait <fqdn> ..."));
    }
    let _exe = argv.remove(0);
    if argv.is_empty() {
        return Err(die("acme-dns-hook-namecheap-dispatch", "usage: set|clear|wait <fqdn> ..."));
    }
    let cmd = argv[0].to_string_lossy().into_owned();
    match cmd.as_str() {
        "set" | "clear" | "wait" => {}
        other => {
            return Err(die(
                "acme-dns-hook-namecheap-dispatch",
                format!("unknown command: {other} (expected set|clear|wait)"),
            ));
        }
    }
    if argv.len() < 2 {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            format!("{cmd} requires <fqdn>"),
        ));
    }
    let fqdn_raw = argv[1].to_string_lossy().into_owned();
    if fqdn_raw.is_empty() || fqdn_raw.chars().any(|c| c.is_control()) {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            "fqdn empty or contains control characters",
        ));
    }
    let fqdn = normalize_fqdn(&fqdn_raw);
    let xdg = std::env::var("XDG_DATA_HOME").ok().filter(|s| !s.is_empty());
    let home = std::env::var("HOME").unwrap_or_default();
    let xdg_home = xdg.unwrap_or_else(|| format!("{home}/.local/share"));
    let home_share = format!("{home}/.local/share");

    let zone_env = if fqdn == "surmount.systems" || fqdn.ends_with(".surmount.systems") {
        pick_first(&[
            format!("{xdg_home}/surmount/issue-le-prod/namecheap.env"),
            format!("{home_share}/surmount/issue-le-prod/namecheap.env"),
        ])?
        .ok_or_else(|| {
            die(
                "acme-dns-hook-namecheap-dispatch",
                "no primary Namecheap env for surmount.systems (expected issue-le-prod/namecheap.env)",
            )
        })?
    } else {
        find_extra(&fqdn, &xdg_home, &home_share)?.ok_or_else(|| {
            die(
                "acme-dns-hook-namecheap-dispatch",
                format!(
                    "no per-zone Namecheap env for {fqdn} (expected namecheap/<sld.tld>.env; refuse primary surmount.systems env)"
                ),
            )
        })?
    };

    let inner = std::env::var("SURMOUNT_ACME_DNS_DISPATCH_INNER_HOOK")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("acme-dns-hook-namecheap"));
    let meta = std::fs::symlink_metadata(&inner).map_err(|_| {
        die(
            "acme-dns-hook-namecheap-dispatch",
            format!("inner hook missing: {}", inner.display()),
        )
    })?;
    if meta.file_type().is_symlink() {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            format!("inner hook must be a regular file (symlink refused): {}", inner.display()),
        ));
    }
    if !meta.is_file() {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            format!("inner hook missing: {}", inner.display()),
        ));
    }

    let mut child = Command::new(&inner);
    child.args(&argv);
    child.env("SURMOUNT_ACME_DNS_NAMECHEAP_ENV", &zone_env);
    let err = child.exec();
    Err(die(
        "acme-dns-hook-namecheap-dispatch",
        format!("exec inner hook failed: {err}"),
    ))
}

fn normalize_fqdn(fqdn: &str) -> String {
    let mut f = fqdn.to_ascii_lowercase();
    while f.ends_with('.') {
        f.pop();
    }
    f
}

fn accept_zone_env(path: &Path) -> anyhow::Result<Option<PathBuf>> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    if meta.file_type().is_symlink() {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            format!("zone env must be a regular file (symlink refused): {}", path.display()),
        ));
    }
    if !meta.is_file() {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            format!("zone env must be a regular file: {}", path.display()),
        ));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(die(
            "acme-dns-hook-namecheap-dispatch",
            format!("zone env must be owner-only (mode 0600): {} (mode {:o})", path.display(), mode),
        ));
    }
    Ok(Some(path.to_path_buf()))
}

fn pick_first(cands: &[String]) -> anyhow::Result<Option<PathBuf>> {
    for c in cands {
        if c.is_empty() {
            continue;
        }
        if let Some(p) = accept_zone_env(Path::new(c))? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

fn find_extra(fqdn: &str, xdg_home: &str, home_share: &str) -> anyhow::Result<Option<PathBuf>> {
    let labels: Vec<&str> = fqdn.split('.').collect();
    if labels.len() < 2 {
        return Ok(None);
    }
    for i in 0..=labels.len() - 2 {
        let suffix = labels[i..].join(".");
        if let Some(p) = pick_first(&[
            format!("{xdg_home}/surmount/namecheap/{suffix}.env"),
            format!("{home_share}/surmount/namecheap/{suffix}.env"),
        ])? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}
