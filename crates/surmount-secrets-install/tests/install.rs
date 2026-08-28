//! Hermetic CLI contracts for secrets-install-host. Staging + dest-root only.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_secrets-install-host")
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-secrets-install-{}-{}-{}",
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

fn mode(path: &Path) -> String {
    let m = fs::metadata(path).unwrap().permissions().mode() & 0o777;
    format!("{m:o}")
}

fn make_item(staging: &Path, id: &str, kind: &str, hpath: &str, payload: &str, host: &str) {
    let dir = staging.join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("attributes"),
        format!("surmount.kind={kind}\nsurmount.host={host}\nsurmount.path={hpath}\n"),
    )
    .unwrap();
    fs::write(dir.join("secret"), payload).unwrap();
    let mut p = fs::metadata(dir.join("secret")).unwrap().permissions();
    p.set_mode(0o600);
    fs::set_permissions(dir.join("secret"), p).unwrap();
}

fn cmd() -> Command {
    let mut c = Command::new(bin());
    c.env_remove("SURMOUNT_SECRETS_TARGET")
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .env_remove("SURMOUNT_SECRETS_HOST_ID")
        .env(
            "SURMOUNT_REPO_ROOT",
            "/nonexistent-surmount-repo-root-hermetic",
        );
    c
}

const PLANT: &str = "SURMOUNT-TEST-SECRET-PAYLOAD-9f3c2a1b-do-not-log";

#[test]
fn help_documents_staging_and_dest_root() {
    let out = cmd().arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("--from-staging"));
    assert!(s.contains("--dest-root"));
    assert!(s.contains("--ensure-acme-parents"));
    assert!(s.contains("/var/lib/surmount/secrets"));
}

#[test]
fn missing_source_fails() {
    let work = temp_dir("no-src");
    let dest = work.join("dest");
    fs::create_dir_all(&dest).unwrap();
    let out = cmd()
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn option_shaped_target_rejected() {
    let work = temp_dir("opt-tgt");
    let staging = work.join("staging");
    fs::create_dir_all(&staging).unwrap();
    make_item(
        &staging,
        "item-tls",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--target", "-oProxyCommand=true"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn staging_install_tls_cert_0640_key_0600() {
    let work = temp_dir("tls");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "item-tls",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        PLANT,
        "mail-lab",
    );
    make_item(
        &staging,
        "item-key",
        "tls-key",
        "/run/surmount-secrets/tls/key.pem",
        &format!("{PLANT}-key"),
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let cert = dest.join("run/surmount-secrets/tls/cert.pem");
    let key = dest.join("run/surmount-secrets/tls/key.pem");
    assert!(cert.is_file());
    assert!(key.is_file());
    assert!(!cert.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(mode(&cert), "640");
    assert_eq!(mode(&key), "600");
    assert_eq!(fs::read_to_string(&cert).unwrap(), PLANT);
    let parent = dest.join("run/surmount-secrets/tls");
    assert_eq!(mode(&parent), "750");
    assert!(!err.contains(PLANT));
}

#[test]
fn refuse_hosts_and_secrets_paths() {
    let work = temp_dir("pub");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "bad-hosts",
        "other",
        "/hosts/mail-vps/evil.secret",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.to_lowercase().contains("refuse"));
    assert!(!err.contains(PLANT));
}

#[test]
fn refuse_path_dotdot() {
    let work = temp_dir("dotdot");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "escape-item",
        "other",
        "/run/../../../escape-outside",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!work.join("escape-outside").exists());
}

#[test]
fn refuse_symlink_secret() {
    let work = temp_dir("sym");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(work.join("real-payload-bytes"), PLANT).unwrap();
    let dir = staging.join("sym-item");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("attributes"),
        "surmount.kind=session-secret\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/ui/session-secret\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(work.join("real-payload-bytes"), dir.join("secret")).unwrap();
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn dry_run_does_not_write() {
    let work = temp_dir("dry");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "item-tls",
        "session-secret",
        "/var/lib/surmount/secrets/ui/session-secret",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--dry-run", "--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains("dry-run"));
    assert!(!err.contains(PLANT));
    assert!(
        !dest
            .join("var/lib/surmount/secrets/ui/session-secret")
            .exists()
    );
}

#[test]
fn missing_required_kind_fails() {
    let work = temp_dir("req");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "item",
        "session-secret",
        "/var/lib/surmount/secrets/ui/session-secret",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args([
            "--host-id",
            "mail-lab",
            "--require-kind",
            "stalwart-token",
            "--dest-root",
        ])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn unknown_kind_at_parse_fails() {
    let work = temp_dir("unk");
    let dest = work.join("dest");
    fs::create_dir_all(&dest).unwrap();
    let out = cmd()
        .args([
            "--from-staging",
            "/tmp",
            "--host-id",
            "mail-lab",
            "--require-kind",
            "not-a-kind",
            "--dest-root",
        ])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn refuse_age_admin_and_synology() {
    let work = temp_dir("age");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "age",
        "age-admin",
        "/var/lib/sops-nix/key.txt",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("age-admin"));

    let staging2 = work.join("staging2");
    fs::create_dir_all(&staging2).unwrap();
    make_item(
        &staging2,
        "afp",
        "synology-afp",
        "/var/lib/surmount/secrets/afp",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging2)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn namecheap_client_ip_rewrite() {
    let work = temp_dir("nc");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "nc",
        "namecheap-api",
        "/var/lib/surmount/secrets/acme/namecheap.env",
        "ApiUser=lab\nApiKey=SYNTHETIC-KEY\nClientIp=203.0.113.1\nSLD=example\nTLD=com\n",
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args([
            "--host-id",
            "mail-lab",
            "--client-ip",
            "198.51.100.9",
            "--dest-root",
        ])
        .arg(&dest)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("SYNTHETIC-KEY"));
    let body =
        fs::read_to_string(dest.join("var/lib/surmount/secrets/acme/namecheap.env")).unwrap();
    assert!(body.contains("ClientIp=198.51.100.9"));
    assert!(!body.contains("203.0.113.1"));
    assert!(body.contains("ApiKey=SYNTHETIC-KEY"));
}

#[test]
fn ensure_acme_parents_durable() {
    let work = temp_dir("acme");
    let dest = work.join("dest");
    fs::create_dir_all(&dest).unwrap();
    let out = cmd()
        .args(["--ensure-acme-parents", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let tls = dest.join("var/lib/surmount/secrets/tls");
    let acme = dest.join("var/lib/surmount/secrets/acme");
    assert!(tls.is_dir());
    assert!(acme.is_dir());
    assert_eq!(mode(&tls), "750");
    assert_eq!(mode(&acme), "750");
    assert!(!tls.join("cert.pem").exists());
}

#[test]
fn ensure_acme_parents_refuses_staging() {
    let work = temp_dir("acme-bad");
    let dest = work.join("dest");
    let staging = work.join("staging");
    fs::create_dir_all(&dest).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let out = cmd()
        .args(["--ensure-acme-parents", "--from-staging"])
        .arg(&staging)
        .args(["--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn host_mismatch_fails() {
    let work = temp_dir("host");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "item",
        "session-secret",
        "/var/lib/surmount/secrets/ui/session-secret",
        PLANT,
        "other-host",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn durable_session_and_token() {
    let work = temp_dir("dur");
    let staging = work.join("staging");
    let dest = work.join("dest");
    fs::create_dir_all(&staging).unwrap();
    fs::create_dir_all(&dest).unwrap();
    make_item(
        &staging,
        "sess",
        "session-secret",
        "/var/lib/surmount/secrets/ui/session-secret",
        PLANT,
        "mail-lab",
    );
    make_item(
        &staging,
        "tok",
        "stalwart-token",
        "/var/lib/surmount/secrets/ui/stalwart-api-token",
        PLANT,
        "mail-lab",
    );
    let out = cmd()
        .args(["--from-staging"])
        .arg(&staging)
        .args(["--host-id", "mail-lab", "--dest-root"])
        .arg(&dest)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let tok = dest.join("var/lib/surmount/secrets/ui/stalwart-api-token");
    assert!(tok.is_file());
    assert_eq!(mode(&tok), "600");
    assert_eq!(mode(&dest.join("var/lib/surmount/secrets/ui")), "750");
}

#[test]
fn remote_mock_ssh_refuses_directory_final() {
    let work = temp_dir("ssh");
    let staging = work.join("staging");
    fs::create_dir_all(&staging).unwrap();
    make_item(
        &staging,
        "item-tls",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        PLANT,
        "mail-lab",
    );
    let fake_remote = work.join("fake-remote");
    fs::create_dir_all(fake_remote.join("run/surmount-secrets/tls/cert.pem")).unwrap();
    let mock_bin = work.join("mock-bin");
    fs::create_dir_all(&mock_bin).unwrap();
    let ssh = mock_bin.join("ssh");
    fs::write(
        &ssh,
        format!(
            "#!/bin/sh\nset -eu\n\
while [ $# -gt 0 ]; do\n\
  case \"$1\" in -*) shift ;; *) break ;; esac\n\
done\n\
host=\"${{1:-}}\"; shift || true\n\
in=\"$*\"\n\
old='/run/surmount-secrets'\n\
new='{fake}/run/surmount-secrets'\n\
out=\n\
while :; do\n\
  case \"$in\" in\n\
    *\"$old\"*)\n\
      out=\"$out${{in%%\"$old\"*}}$new\"\n\
      in=\"${{in#*\"$old\"}}\"\n\
      ;;\n\
    *)\n\
      out=\"$out$in\"\n\
      break\n\
      ;;\n\
  esac\n\
done\n\
sh -c \"$out\"\n",
            fake = fake_remote.display()
        ),
    )
    .unwrap();
    let mut p = fs::metadata(&ssh).unwrap().permissions();
    p.set_mode(0o755);
    fs::set_permissions(&ssh, p).unwrap();
    let out = cmd()
        .env("SURMOUNT_TEST_FAKE_REMOTE", &fake_remote)
        .env("PATH", format!("{}:/usr/bin:/bin", mock_bin.display()))
        .args(["--from-staging"])
        .arg(&staging)
        .args([
            "--host-id",
            "mail-lab",
            "--target",
            "mock-host",
            "--ssh-cmd",
        ])
        .arg(&ssh)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    assert!(
        err.to_lowercase().contains("directory") || err.to_lowercase().contains("refuse"),
        "{err}"
    );
    assert!(!err.contains(PLANT));
}
