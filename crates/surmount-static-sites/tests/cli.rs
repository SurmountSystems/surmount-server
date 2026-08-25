//! Hermetic static-site deploy/sync. Fake trees. No live SSH. No IPv4.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn deploy() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-deploy-static-sites")
}
fn sync() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-sync-static-sites")
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "surmount-static-sites-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn deploy_help_splits_from_deploy_host() {
    let out = Command::new(deploy()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("static sites"));
    assert!(t.contains("deploy-host"));
    assert!(t.contains("NixOS"));
}

#[test]
fn deploy_missing_target() {
    let dir = temp_dir("site");
    let site = dir.join("site");
    fs::create_dir_all(&site).unwrap();
    fs::write(site.join("index.html"), "<html>x</html>\n").unwrap();
    let out = Command::new(deploy())
        .args(["--dry-run", "--site-root"])
        .arg(&site)
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .env("SURMOUNT_AGENT_TARGET_ENV", dir.join("no-agent-target.env"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.to_ascii_lowercase().contains("target"));
}

#[test]
fn deploy_dry_run_plans_and_redacts() {
    let dir = temp_dir("plan");
    let site = dir.join("site");
    let extra = dir.join("extra");
    fs::create_dir_all(site.join("")).unwrap();
    fs::create_dir_all(extra.join("cryptoquick")).unwrap();
    fs::create_dir_all(extra.join("yiffa")).unwrap();
    fs::write(
        site.join("index.html"),
        "<html><title>Surmount Systems</title>Bitcoin-focused Deep Tech</html>\n",
    )
    .unwrap();
    fs::write(extra.join("cryptoquick/index.html"), "<html>CRYPTOQUICK-FIXTURE</html>\n").unwrap();
    fs::write(extra.join("yiffa/index.html"), "<html>YIFFA-FIXTURE</html>\n").unwrap();
    let out = Command::new(deploy())
        .args([
            "--dry-run",
            "--target",
            "root@example.test",
            "--site-root",
        ])
        .arg(&site)
        .arg("--extra-from")
        .arg(&extra)
        .output()
        .unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("dry-run") && t.contains("apex") && t.contains("cryptoquick") && t.contains("yiffa"));
    assert!(!t.contains("root@example.test"));
    assert!(!regex_ipv4(&t));
}

#[test]
fn deploy_local_dest_copies() {
    let dir = temp_dir("copy");
    let site = dir.join("site");
    let extra = dir.join("extra");
    let dest = dir.join("dest");
    fs::create_dir_all(&site).unwrap();
    fs::create_dir_all(extra.join("cryptoquick")).unwrap();
    fs::create_dir_all(extra.join("yiffa")).unwrap();
    fs::write(
        site.join("index.html"),
        "<html>Bitcoin-focused Deep Tech</html>\n",
    )
    .unwrap();
    fs::write(extra.join("cryptoquick/index.html"), "<html>CRYPTOQUICK-FIXTURE</html>\n").unwrap();
    fs::write(extra.join("yiffa/index.html"), "<html>YIFFA-FIXTURE</html>\n").unwrap();
    let out = Command::new(deploy())
        .args(["--live", "--local-dest"])
        .arg(&dest)
        .arg("--site-root")
        .arg(&site)
        .arg("--extra-from")
        .arg(&extra)
        .arg("--no-restart")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let apex = fs::read_to_string(dest.join("public-site/index.html")).unwrap();
    assert!(apex.contains("Bitcoin-focused Deep Tech"));
    assert!(fs::read_to_string(dest.join("static-sites/cryptoquick/index.html"))
        .unwrap()
        .contains("CRYPTOQUICK-FIXTURE"));
}

#[test]
fn deploy_missing_extra_skips() {
    let dir = temp_dir("skip");
    let site = dir.join("site");
    let dest = dir.join("dest2");
    fs::create_dir_all(&site).unwrap();
    fs::write(site.join("index.html"), "<html>x</html>\n").unwrap();
    let out = Command::new(deploy())
        .args(["--live", "--local-dest"])
        .arg(&dest)
        .arg("--site-root")
        .arg(&site)
        .arg("--extra-from")
        .arg(dir.join("no-such-extra"))
        .arg("--no-restart")
        .output()
        .unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout).to_ascii_lowercase();
    assert!(t.contains("skip"));
    assert!(dest.join("public-site/index.html").is_file());
    assert!(!dest.join("static-sites/cryptoquick").is_dir());
}

#[test]
fn sync_dry_run_and_live() {
    let dir = temp_dir("sync");
    let gvfs = dir.join("gvfs");
    let sites = gvfs.join("afp-volume:host=DS3018xs.local,user=hunter,volume=sites");
    let dest = dir.join("static-sites");
    for slug in [
        "cryptoquick",
        "yiffa",
        "baxterartworks",
        "btcfur",
        "exophiles",
        "iantuckerstudios",
        "nostrfurs",
        "denverspace",
        "btcdragonlord",
        "btckitties",
        "justsaybits",
    ] {
        fs::create_dir_all(sites.join(slug)).unwrap();
        fs::write(
            sites.join(slug).join("index.html"),
            format!("<html>{slug}-FIXTURE</html>\n"),
        )
        .unwrap();
    }
    let out = Command::new(sync())
        .arg("--gvfs-root")
        .arg(&gvfs)
        .arg("--dest")
        .arg(&dest)
        .output()
        .unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("dry-run") && t.contains("cryptoquick"));
    assert!(!t.contains("PLAN     denverspace"));
    assert!(!regex_ipv4(&t));
    assert!(!dest.join("cryptoquick/index.html").exists());

    let out = Command::new(sync())
        .args(["--live", "--gvfs-root"])
        .arg(&gvfs)
        .arg("--dest")
        .arg(&dest)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(fs::read_to_string(dest.join("cryptoquick/index.html"))
        .unwrap()
        .contains("cryptoquick-FIXTURE"));
    assert!(fs::read_to_string(dest.join("yiffa/index.html"))
        .unwrap()
        .contains("yiffa-FIXTURE"));
    assert!(!dest.join("denverspace/index.html").exists());
    assert!(!regex_ipv4(&String::from_utf8_lossy(&out.stdout)));
}

fn regex_ipv4(s: &str) -> bool {
    let re = regex_lite(s);
    re
}

fn regex_lite(s: &str) -> bool {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let mut dots = 0;
            let mut j = i;
            let mut oct = 0;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                if bytes[j] == b'.' {
                    dots += 1;
                    oct = 0;
                } else {
                    oct += 1;
                    if oct > 3 {
                        break;
                    }
                }
                j += 1;
            }
            if dots == 3 && oct > 0 {
                return true;
            }
        }
        i += 1;
    }
    false
}
