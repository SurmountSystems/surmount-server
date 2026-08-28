//! CLI contracts from the former script/test-deploy-host.sh harness.
//! Synthetic fixtures only. Never assert secret payload bytes in output.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const PLANTED: &str = "synthetic-not-a-real-cert";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-deploy-host")
}

fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/deploy-host")
}

fn good_hl() -> PathBuf {
    testdata().join("good-host-local")
}

fn cmd() -> Command {
    let mut c = Command::new(bin());
    for k in [
        "SURMOUNT_DEPLOY_TARGET",
        "SURMOUNT_HOST_LOCAL_DIR",
        "SURMOUNT_DEPLOY_FLAKE_ATTR",
        "SURMOUNT_DEPLOY_REMOTE_DIR",
        "SURMOUNT_DEPLOY_INSTALL_SECRETS",
        "SURMOUNT_SECRETS_STAGING",
        "SURMOUNT_SECRETS_FROM_SECRET_SERVICE",
        "SURMOUNT_SECRETS_HOST_ID",
        "SURMOUNT_SECRETS_TARGET",
        "SURMOUNT_DEPLOY_SECRETS_INSTALL",
        "SURMOUNT_DEPLOY_REPO_ROOT",
        "SURMOUNT_DEPLOY_SSH_IDENTITY",
        "SURMOUNT_DEPLOY_SSH",
    ] {
        c.env_remove(k);
    }
    c
}

fn combined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

fn temp_dir(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "surmount-deploy-host-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_exec(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
    path
}

fn secrets_stub() -> PathBuf {
    let dir = temp_dir("secrets-stub");
    write_exec(
        &dir,
        "secrets-install-host",
        "#!/bin/sh\necho secrets-install-host \"$@\"\nexit 0\n",
    )
}

#[test]
fn help_exits_0_and_mentions_dry_run() {
    let out = cmd().arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = combined(&out);
    assert!(s.contains("--dry-run"), "{s}");
    assert!(s.contains("--install-secrets"), "{s}");
}

#[test]
fn missing_target_fails_loud() {
    let out = cmd().arg("--dry-run").output().unwrap();
    assert!(!out.status.success());
}

#[test]
fn option_shaped_target_rejected() {
    let out = cmd()
        .args(["--dry-run", "--target", "-oProxyCommand=true"])
        .arg("--host-local")
        .arg(good_hl())
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn dry_run_good_host_local_prints_rebuild_and_smoke() {
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("nixos-rebuild") && s.contains("mail-vps"), "{s}");
    assert!(s.contains("path:") && s.contains("#mail-vps"), "{s}");
    assert!(s.contains("host-local"), "{s}");
    assert!(
        s.contains("secrets.yaml") || s.contains(".agekey") || s.contains("secrets/"),
        "{s}"
    );
    let low = s.to_ascii_lowercase();
    assert!(
        low.contains("evaluated")
            || low.contains("authorizedkeys")
            || low.contains("not enough")
            || low.contains("not sufficient"),
        "{s}"
    );
    assert!(
        low.contains("post-switch smoke") || s.contains("deploy-host-post-switch-smoke"),
        "{s}"
    );
    assert!(
        low.contains("systemctl is-active") || low.contains("surmount-management-ui"),
        "{s}"
    );
    assert!(low.contains("health") || low.contains("loopback"), "{s}");
}

#[test]
fn root_target_dry_run_mentions_laptop_identity() {
    let home = temp_dir("ssh-home");
    fs::create_dir_all(home.join(".ssh")).unwrap();
    fs::write(home.join(".ssh/id_ed25519"), "synthetic-not-a-real-key\n").unwrap();
    let out = cmd()
        .env("HOME", &home)
        .args(["--dry-run", "--target", "root@example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("id_ed25519"), "{s}");
    assert!(
        s.contains("keepalives") || s.contains("SURMOUNT_DEPLOY_SSH_IDENTITY"),
        "{s}"
    );
}

#[test]
fn empty_authorized_keys_fails_lockout() {
    let hl = testdata().join("empty-keys-host-local");
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(&hl)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn short_placeholder_key_fails_lockout() {
    let hl = testdata().join("short-placeholder-keys-host-local");
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(&hl)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

fn refuse_into(dest: &Path) {
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .arg("--sync-host-local-into")
        .arg(dest)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "expected refuse for {}: {}",
        dest.display(),
        combined(&out)
    );
}

#[test]
fn refuse_host_local_into_hosts_and_secrets_paths() {
    let tmp = temp_dir("tracked");
    fs::create_dir_all(tmp.join("hosts/mail-vps")).unwrap();
    refuse_into(&tmp.join("hosts/mail-vps"));
    refuse_into(&tmp.join("no-such-parent/hosts/mail-vps"));
    refuse_into(&tmp.join("no-such-parent/secrets/foo"));
    fs::create_dir_all(tmp.join("secrets/private")).unwrap();
    refuse_into(&tmp.join("secrets/private"));
    fs::create_dir_all(tmp.join("hosts/via-link")).unwrap();
    let link = tmp.join("innocent-link");
    std::os::unix::fs::symlink(tmp.join("hosts/via-link"), &link).unwrap();
    refuse_into(&link);

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tracked = repo.join("hosts/mail-vps");
    if tracked.is_dir() {
        refuse_into(&tracked);
        let out = cmd()
            .current_dir(&repo)
            .args(["--dry-run", "--target", "example.test"])
            .arg("--host-local")
            .arg(good_hl())
            .args(["--sync-host-local-into", "hosts/mail-vps"])
            .output()
            .unwrap();
        assert!(!out.status.success(), "{}", combined(&out));
    }
}

#[test]
fn default_dry_run_does_not_plan_secrets_install() {
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .output()
        .unwrap();
    assert!(out.status.success());
    let s = combined(&out).to_ascii_lowercase();
    assert!(
        !s.contains("secrets-install-host") && !s.contains("would run secrets-install"),
        "{s}"
    );
}

#[test]
fn install_secrets_without_source_fails() {
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args(["--install-secrets", "--secrets-host-id", "mail-lab"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let s = combined(&out).to_ascii_lowercase();
    assert!(
        s.contains("source") || s.contains("staging") || s.contains("secret-service"),
        "{s}"
    );
}

#[test]
fn install_secrets_without_host_id_fails() {
    let stg = temp_dir("empty-stage");
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .arg("--install-secrets")
        .arg("--secrets-staging")
        .arg(&stg)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn missing_secrets_staging_path_fails_without_payload_leak() {
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args([
            "--install-secrets",
            "--secrets-host-id",
            "mail-lab",
            "--secrets-staging",
            "/no/such/surmount-s4-staging-dir",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let s = combined(&out);
    let low = s.to_ascii_lowercase();
    assert!(
        low.contains("staging dir missing") || low.contains("not a directory"),
        "{s}"
    );
    assert!(!s.contains(PLANTED));
}

#[test]
fn refuse_in_tree_secrets_staging_no_payload_leak() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let stg = repo.join(format!(".surmount-s4-in-tree-stage-{}", std::process::id()));
    let item = stg.join("item1");
    fs::create_dir_all(&item).unwrap();
    fs::write(
        item.join("attributes"),
        "surmount.kind=tls-cert\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/tls/cert.pem\n",
    )
    .unwrap();
    fs::write(item.join("secret"), format!("{PLANTED}\n")).unwrap();
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args(["--install-secrets", "--secrets-host-id", "mail-lab"])
        .arg("--secrets-staging")
        .arg(&stg)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&stg);
    assert!(!out.status.success());
    let s = combined(&out);
    let low = s.to_ascii_lowercase();
    assert!(
        low.contains("outside the public git")
            || low.contains("refuse") && low.contains("staging")
            || low.contains("work tree"),
        "{s}"
    );
    assert!(!s.contains(PLANTED), "{s}");
}

fn plant_stage() -> PathBuf {
    let stg = temp_dir("s4-stage");
    let item = stg.join("item1");
    fs::create_dir_all(&item).unwrap();
    fs::write(
        item.join("attributes"),
        "surmount.kind=tls-cert\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/tls/cert.pem\n",
    )
    .unwrap();
    fs::write(item.join("secret"), format!("{PLANTED}\n")).unwrap();
    stg
}

#[test]
fn opt_in_dry_run_plans_bridge_without_payload() {
    let stub = secrets_stub();
    let stg = plant_stage();
    let out = cmd()
        .env("SURMOUNT_DEPLOY_SECRETS_INSTALL", &stub)
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args([
            "--install-secrets",
            "--secrets-host-id",
            "mail-lab",
            "--secrets-require-kind",
            "tls-cert",
        ])
        .arg("--secrets-staging")
        .arg(&stg)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("secrets-install-host"), "{s}");
    assert!(s.contains("--from-staging"), "{s}");
    assert!(s.contains("mail-lab"), "{s}");
    assert!(s.contains("example.test"), "{s}");
    assert!(
        s.contains("--require-kind") && s.contains("tls-cert"),
        "{s}"
    );
    assert!(s.contains("nixos-rebuild"), "{s}");
    assert!(!s.contains(PLANTED), "{s}");
}

#[test]
fn env_install_secrets_plans_bridge() {
    let stub = secrets_stub();
    let stg = plant_stage();
    let out = cmd()
        .env("SURMOUNT_DEPLOY_SECRETS_INSTALL", &stub)
        .env("SURMOUNT_DEPLOY_INSTALL_SECRETS", "1")
        .env("SURMOUNT_SECRETS_STAGING", &stg)
        .env("SURMOUNT_SECRETS_HOST_ID", "mail-lab")
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("secrets-install-host"), "{s}");
    assert!(!s.contains(PLANTED));
}

#[test]
fn secret_service_only_plans_without_staging() {
    let stub = secrets_stub();
    let out = cmd()
        .env("SURMOUNT_DEPLOY_SECRETS_INSTALL", &stub)
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args([
            "--install-secrets",
            "--secrets-from-secret-service",
            "--secrets-host-id",
            "mail-lab",
        ])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("secrets-install-host"), "{s}");
    assert!(s.contains("--from-secret-service"), "{s}");
    assert!(!s.contains("--from-staging"), "{s}");
    assert!(s.contains("nixos-rebuild"), "{s}");
}

#[test]
fn both_sources_refused() {
    let stg = plant_stage();
    let out = cmd()
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args([
            "--install-secrets",
            "--secrets-from-secret-service",
            "--secrets-host-id",
            "mail-lab",
        ])
        .arg("--secrets-staging")
        .arg(&stg)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn unknown_require_kind_fails() {
    let stub = secrets_stub();
    let stg = plant_stage();
    let out = cmd()
        .env("SURMOUNT_DEPLOY_SECRETS_INSTALL", &stub)
        .args(["--dry-run", "--target", "example.test"])
        .arg("--host-local")
        .arg(good_hl())
        .args([
            "--install-secrets",
            "--secrets-host-id",
            "mail-lab",
            "--secrets-require-kind",
            "not-a-real-kind",
        ])
        .arg("--secrets-staging")
        .arg(&stg)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn vaultwarden_admin_and_namecheap_allowlisted() {
    let stub = secrets_stub();
    let stg = plant_stage();
    for kind in ["vaultwarden-admin", "namecheap-api"] {
        let out = cmd()
            .env("SURMOUNT_DEPLOY_SECRETS_INSTALL", &stub)
            .args(["--dry-run", "--target", "example.test"])
            .arg("--host-local")
            .arg(good_hl())
            .args([
                "--install-secrets",
                "--secrets-host-id",
                "mail-lab",
                "--secrets-require-kind",
                kind,
            ])
            .arg("--secrets-staging")
            .arg(&stg)
            .output()
            .unwrap();
        let s = combined(&out);
        assert!(out.status.success(), "{kind}: {s}");
        assert!(s.contains("secrets-install-host"), "{s}");
        assert!(s.contains("--require-kind") && s.contains(kind), "{s}");
        assert!(!s.contains(PLANTED), "{s}");
    }
}
