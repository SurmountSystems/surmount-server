//! Hermetic CLI contracts for secrets-export-bw-to-staging. Fixture + mock bw.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_secrets-export-bw-to-staging")
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-secrets-export-{}-{}-{}",
        label,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

const PLANT: &str = "SURMOUNT-TEST-EXPORT-PAYLOAD-7e2a-do-not-log";

fn make_map(map: &PathBuf, id: &str, kind: &str, path: &str, bw_item: &str, wrap: Option<&str>) {
    let dir = map.join(id);
    fs::create_dir_all(&dir).unwrap();
    let mut body = format!(
        "surmount.kind={kind}\nsurmount.host=mail-lab\nsurmount.path={path}\nsurmount.bw_item={bw_item}\nsurmount.bw_field=password\n"
    );
    if let Some(w) = wrap {
        body.push_str(&format!("surmount.bw_wrap={w}\n"));
    }
    fs::write(dir.join("attributes"), body).unwrap();
}

#[test]
fn help_documents_map() {
    let out = Command::new(bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("--map"));
    assert!(s.contains("--from-fixture"));
    assert!(s.contains("--from-bw"));
}

#[test]
fn refuse_missing_source() {
    let work = temp_dir("nosrc");
    let map = work.join("map");
    fs::create_dir_all(&map).unwrap();
    let out = Command::new(bin())
        .args(["--map"])
        .arg(&map)
        .args(["--staging"])
        .arg(work.join("staging"))
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn fixture_export_wraps_admin_token() {
    let work = temp_dir("fix");
    let map = work.join("map");
    let staging = work.join("staging");
    let fixture = work.join("fixture");
    fs::create_dir_all(&map).unwrap();
    fs::create_dir_all(fixture.join("vw")).unwrap();
    make_map(
        &map,
        "vw",
        "vaultwarden-admin",
        "/var/lib/surmount/secrets/vaultwarden/admin.env",
        "vw-item",
        Some("ADMIN_TOKEN"),
    );
    fs::write(fixture.join("vw/secret"), PLANT).unwrap();
    let out = Command::new(bin())
        .args(["--map"])
        .arg(&map)
        .args(["--staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--from-fixture"])
        .arg(&fixture)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains(PLANT));
    let secret = fs::read_to_string(staging.join("vw/secret")).unwrap();
    assert_eq!(secret, format!("ADMIN_TOKEN={PLANT}"));
    let attrs = fs::read_to_string(staging.join("vw/attributes")).unwrap();
    assert!(attrs.contains("surmount.kind=vaultwarden-admin"));
    assert!(!attrs.contains("bw_item"));
}

#[test]
fn dry_run_fixture_no_write() {
    let work = temp_dir("dry");
    let map = work.join("map");
    let staging = work.join("staging");
    let fixture = work.join("fixture");
    fs::create_dir_all(&map).unwrap();
    fs::create_dir_all(fixture.join("a")).unwrap();
    make_map(
        &map,
        "a",
        "session-secret",
        "/var/lib/surmount/secrets/ui/session-secret",
        "item",
        None,
    );
    fs::write(fixture.join("a/secret"), PLANT).unwrap();
    let out = Command::new(bin())
        .args(["--dry-run", "--map"])
        .arg(&map)
        .args(["--staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--from-fixture"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!staging.join("a/secret").exists());
}

#[test]
fn host_mismatch_fails() {
    let work = temp_dir("hm");
    let map = work.join("map");
    let staging = work.join("staging");
    let fixture = work.join("fixture");
    fs::create_dir_all(map.join("a")).unwrap();
    fs::create_dir_all(fixture.join("a")).unwrap();
    fs::write(
        map.join("a/attributes"),
        "surmount.kind=session-secret\nsurmount.host=other\nsurmount.path=/var/lib/surmount/secrets/ui/session-secret\nsurmount.bw_item=x\n",
    )
    .unwrap();
    fs::write(fixture.join("a/secret"), PLANT).unwrap();
    let out = Command::new(bin())
        .args(["--map"])
        .arg(&map)
        .args(["--staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--from-fixture"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn mock_bw_path() {
    let work = temp_dir("bw");
    let map = work.join("map");
    let staging = work.join("staging");
    fs::create_dir_all(&map).unwrap();
    make_map(
        &map,
        "tok",
        "stalwart-token",
        "/var/lib/surmount/secrets/ui/stalwart-api-token",
        "tok-item",
        None,
    );
    let bw = work.join("bw");
    fs::write(
        &bw,
        format!("#!/usr/bin/env bash\nset -euo pipefail\necho -n '{PLANT}'\n"),
    )
    .unwrap();
    let mut p = fs::metadata(&bw).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&bw, p).unwrap();
    let out = Command::new(bin())
        .args(["--map"])
        .arg(&map)
        .args(["--staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--from-bw", "--bw-bin"])
        .arg(&bw)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains(PLANT));
    assert_eq!(fs::read_to_string(staging.join("tok/secret")).unwrap(), PLANT);
}
