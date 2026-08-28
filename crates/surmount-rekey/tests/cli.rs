//! CLI contracts for just rekey. Fake gpg only. Never live keyring as CI green.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-rekey")
}

fn temp_dir(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "surmount-rekey-cli-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn fake_gpg(dir: &std::path::Path) -> PathBuf {
    let bin = dir.join("fake-gpg");
    fs::write(
        &bin,
        "#!/bin/sh\n\
         out=\"\"\n\
         while [ \"$#\" -gt 0 ]; do\n\
           if [ \"$1\" = \"-o\" ]; then out=\"$2\"; shift 2; continue; fi\n\
           shift\n\
         done\n\
         printf '%s\\n' '-----BEGIN PGP MESSAGE-----' >\"$out\"\n\
         cat >>\"$out\"\n\
         printf '%s\\n' '-----END PGP MESSAGE-----' >>\"$out\"\n\
         exit 0\n",
    )
    .unwrap();
    let mut p = fs::metadata(&bin).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&bin, p).unwrap();
    bin
}

#[test]
fn help_mentions_pgp() {
    let out = Command::new(bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("PGP"));
    assert!(t.contains("--gpg-recipient"));
}

#[test]
fn dry_run_prints_age1_not_secret() {
    let dir = temp_dir("dry");
    let out = Command::new(bin())
        .args(["--dry-run", "--dir"])
        .arg(&dir)
        .output()
        .unwrap();
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{t}");
    assert!(t.contains("Type: PGP"));
    assert!(t.contains("age1"));
    assert!(!t.contains("AGE-SECRET-KEY-1"));
}

#[test]
fn live_wrap_with_fake_gpg() {
    let dir = temp_dir("live");
    let gpg = fake_gpg(&dir);
    let out = Command::new(bin())
        .arg("--dir")
        .arg(&dir)
        .arg("--gpg-bin")
        .arg(&gpg)
        .arg("--gpg-recipient")
        .arg("me@example.test")
        .output()
        .unwrap();
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{t}");
    assert!(t.contains("age1"));
    assert!(!t.contains("AGE-SECRET-KEY-1"));
    let env = fs::read_to_string(dir.join("envelope.asc")).unwrap();
    assert!(env.contains("BEGIN PGP MESSAGE"));
}
