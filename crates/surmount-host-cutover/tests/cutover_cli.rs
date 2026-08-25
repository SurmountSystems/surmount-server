//! Hermetic CLI contracts from script/test-host-cutover.sh (inventory + cutover).
//! Lab DNS hook and secrets-install dest-root install are other drivers.
//! Never log planted secret payloads.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const PLANT: &str = "SURMOUNT-TEST-CUTOVER-PAYLOAD-a0-do-not-log";

fn cutover() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-host-cutover")
}

fn inventory() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-host-material-inventory")
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn good_hl() -> PathBuf {
    manifest().join("testdata/deploy-host/good-host-local")
}

fn sample_profile() -> PathBuf {
    manifest().join("testdata/host-profile/sample-host-profile.toml")
}

fn combined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

fn temp_dir(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "surmount-cutover-{label}-{}-{}",
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

struct Harness {
    work: PathBuf,
    /// Fake public git work tree (in-tree refuse uses this root).
    repo: PathBuf,
    staging: PathBuf,
    dest: PathBuf,
    hl: PathBuf,
    secrets: PathBuf,
    render: PathBuf,
    free443: PathBuf,
    dns: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let work = temp_dir("work");
        let repo = work.join("public-tree");
        let staging = work.join("staging");
        let dest = work.join("dest-root");
        let hl = work.join("host-local");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("flake.nix"), "{ }\n").unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(&hl).unwrap();
        copy_dir(&good_hl(), &hl);
        let tools = work.join("tools");
        fs::create_dir_all(&tools).unwrap();
        let secrets = write_exec(
            &tools,
            "secrets-install-host",
            "#!/bin/sh\necho secrets-install-host \"$@\"\nexit 0\n",
        );
        let render = write_exec(
            &tools,
            "render-host-profile-acme",
            r#"#!/bin/sh
out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out=$2; shift 2 ;;
    *) shift ;;
  esac
done
mkdir -p "$out"
printf '%s\n' '# rendered acme fragment' 'example.invalid' >"$out/host-local-acme.nix"
echo render-host-profile-acme
exit 0
"#,
        );
        let free443 = write_exec(
            &tools,
            "free-stalwart-public-443",
            "#!/bin/sh\necho free-stalwart-public-443 \"$@\"\nexit 0\n",
        );
        let dns = write_exec(
            &tools,
            "dns-zone-namecheap",
            "#!/bin/sh\necho dns-zone-namecheap \"$@\"\nexit 0\n",
        );
        Self {
            work,
            repo,
            staging,
            dest,
            hl,
            secrets,
            render,
            free443,
            dns,
        }
    }

    fn cmd_cutover(&self) -> Command {
        let mut c = Command::new(cutover());
        clear_env(&mut c);
        c.env("SURMOUNT_CUTOVER_REPO_ROOT", &self.repo);
        c.env("SURMOUNT_DEPLOY_REPO_ROOT", &self.repo);
        c.env("SURMOUNT_CUTOVER_SECRETS_INSTALL", &self.secrets);
        c.env("SURMOUNT_CUTOVER_RENDER_PROFILE", &self.render);
        c.env("SURMOUNT_CUTOVER_FREE_443", &self.free443);
        c.env("SURMOUNT_CUTOVER_DNS_ZONE", &self.dns);
        c.env("SURMOUNT_DEPLOY_SECRETS_INSTALL", &self.secrets);
        c
    }

    fn cmd_inv(&self) -> Command {
        let mut c = Command::new(inventory());
        clear_env(&mut c);
        c.env("SURMOUNT_CUTOVER_REPO_ROOT", &self.repo);
        c.env("SURMOUNT_DEPLOY_REPO_ROOT", &self.repo);
        c
    }

    fn reset_staging(&self) {
        let _ = fs::remove_dir_all(&self.staging);
        fs::create_dir_all(&self.staging).unwrap();
    }

    fn make_item(&self, id: &str, kind: &str, hpath: &str, payload: &str) {
        self.make_item_host(id, kind, hpath, payload, "mail-lab");
    }

    fn make_item_host(&self, id: &str, kind: &str, hpath: &str, payload: &str, host: &str) {
        let dir = self.staging.join(id);
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

    fn complete_https(&self) {
        self.reset_staging();
        self.make_item(
            "tls-cert",
            "tls-cert",
            "/run/surmount-secrets/tls/cert.pem",
            &format!("{PLANT}-c"),
        );
        self.make_item(
            "tls-key",
            "tls-key",
            "/run/surmount-secrets/tls/key.pem",
            &format!("{PLANT}-k"),
        );
        self.make_item(
            "session-secret",
            "session-secret",
            "/run/surmount-secrets/ui/session-secret",
            &format!("{PLANT}-s"),
        );
        self.make_item(
            "stalwart-token",
            "stalwart-token",
            "/run/surmount-secrets/ui/stalwart-api-token",
            &format!("{PLANT}-t"),
        );
    }
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for ent in fs::read_dir(src).unwrap() {
        let ent = ent.unwrap();
        let to = dst.join(ent.file_name());
        if ent.file_type().unwrap().is_dir() {
            copy_dir(&ent.path(), &to);
        } else {
            fs::copy(ent.path(), to).unwrap();
        }
    }
}

fn clear_env(c: &mut Command) {
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
        "SURMOUNT_CUTOVER_WITH_VAULTWARDEN",
        "SURMOUNT_CUTOVER_ACME_PATH",
        "SURMOUNT_CUTOVER_HOST_PROFILE",
        "SURMOUNT_ACME_DNS_LAB_ZONE",
        "SURMOUNT_CUTOVER_STEP",
    ] {
        c.env_remove(k);
    }
}

#[test]
fn help_documents_flags() {
    let h = Harness::new();
    let out = h.cmd_cutover().arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = combined(&out);
    assert!(
        s.contains("--dry-run")
            && s.contains("--with-vaultwarden")
            && s.contains("--generate-material"),
        "{s}"
    );
    assert!(
        s.contains("--host-profile") && s.contains("--free-443") && s.contains("--acme-path"),
        "{s}"
    );
    assert!(s.contains("namecheap-api"), "{s}");
    assert!(s.contains("S7b"), "{s}");
    assert!(
        s.contains("--step")
            && s.contains("material")
            && s.contains("free-443")
            && s.contains("le-prod"),
        "{s}"
    );
    let low = s.to_ascii_lowercase();
    assert!(
        low.contains("will not claim") || low.contains("honesty") || low.contains("not claim"),
        "{s}"
    );
}

#[test]
fn inventory_help() {
    let h = Harness::new();
    let out = h.cmd_inv().arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = combined(&out);
    assert!(
        s.contains("--staging") && s.contains("--with-vaultwarden"),
        "{s}"
    );
}

#[test]
fn incomplete_inventory_lists_missing_no_leak() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "tls-cert",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        PLANT,
    );
    let out = h
        .cmd_inv()
        .args(["--staging"])
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("MISSING"), "{s}");
    assert!(!s.contains(PLANT), "{s}");
}

#[test]
fn complete_https_only_inventory() {
    let h = Harness::new();
    h.complete_https();
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("complete"), "{s}");
}

#[test]
fn vw_profile_requires_token_then_complete() {
    let h = Harness::new();
    h.complete_https();
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--with-vaultwarden"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("vaultwarden-admin"), "{s}");
    h.make_item(
        "vaultwarden-admin",
        "vaultwarden-admin",
        "/run/surmount-secrets/vaultwarden/admin.env",
        &format!("ADMIN_TOKEN={PLANT}-vw"),
    );
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--with-vaultwarden"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(!s.contains(PLANT), "{s}");
}

#[test]
fn acme_path_does_not_require_pems() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        &format!("{PLANT}-s"),
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        &format!("{PLANT}-t"),
    );
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
}

#[test]
fn incomplete_dry_run_fails_at_g1() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "tls-cert",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        PLANT,
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--target", "example.test"])
        .arg("--host-local")
        .arg(&h.hl)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.to_ascii_lowercase().contains("incomplete")
            || s.contains("G1 failed")
            || s.contains("MISSING"),
        "{s}"
    );
    assert!(
        !s.contains("nixos-rebuild switch") || s.contains("G1 failed"),
        "{s}"
    );
    assert!(!s.contains(PLANT));
}

#[test]
fn complete_dry_run_skip_deploy_plans_secrets() {
    let h = Harness::new();
    h.complete_https();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--target", "example.test"])
        .arg("--host-local")
        .arg(&h.hl)
        .arg("--dest-root")
        .arg(&h.dest)
        .arg("--skip-deploy")
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains("secrets-install") || s.contains("require-kind"),
        "{s}"
    );
    assert!(!s.contains(PLANT), "{s}");
}

#[test]
fn full_dry_run_reaches_deploy_host() {
    let h = Harness::new();
    h.complete_https();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--target", "example.test"])
        .arg("--host-local")
        .arg(&h.hl)
        .arg("--dest-root")
        .arg(&h.dest)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains("nixos-rebuild") && (s.contains("deploy-host") || s.contains("rsync")),
        "{s}"
    );
    assert!(!s.contains(PLANT), "{s}");
}

#[test]
fn option_shaped_target_rejected() {
    let h = Harness::new();
    h.complete_https();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--target", "-oProxyCommand=true"])
        .arg("--host-local")
        .arg(&h.hl)
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn in_tree_staging_refused() {
    let h = Harness::new();
    let stg = h.repo.join("in-tree-stage");
    fs::create_dir_all(stg.join("tls-cert")).unwrap();
    fs::write(
        stg.join("tls-cert/attributes"),
        "surmount.kind=tls-cert\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/tls/cert.pem\n",
    )
    .unwrap();
    fs::write(stg.join("tls-cert/secret"), "x").unwrap();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&stg)
        .args(["--host-id", "mail-lab", "--target", "example.test"])
        .arg("--host-local")
        .arg(&h.hl)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.to_ascii_lowercase().contains("outside the public git"),
        "{s}"
    );
}

#[test]
fn fragments_vw_only_with_profile() {
    let h = Harness::new();
    h.complete_https();
    let frag = h.work.join("fragments-no-vw");
    fs::create_dir_all(&frag).unwrap();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .arg("--emit-fragments")
        .arg(&frag)
        .args(["--skip-deploy", "--skip-secrets-install"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(frag.join("surmount-cutover-fragments.nix").is_file());
    assert!(!frag.join("surmount-vaultwarden-enable.nix").exists());

    h.make_item(
        "vaultwarden-admin",
        "vaultwarden-admin",
        "/run/surmount-secrets/vaultwarden/admin.env",
        &format!("ADMIN_TOKEN={PLANT}-vw"),
    );
    let frag_vw = h.work.join("fragments-vw");
    fs::create_dir_all(&frag_vw).unwrap();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--with-vaultwarden"])
        .arg("--emit-fragments")
        .arg(&frag_vw)
        .args(["--skip-deploy", "--skip-secrets-install"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    let vw = fs::read_to_string(frag_vw.join("surmount-vaultwarden-enable.nix")).unwrap();
    assert!(vw.contains("enable = true") && vw.contains("vaultwarden"));
    assert!(s.contains("S7b") || s.to_ascii_lowercase().contains("vaultwarden"));
}

#[test]
fn refuse_fragments_into_public_paths() {
    let h = Harness::new();
    let hosts = h.work.join("hosts/mail-vps");
    fs::create_dir_all(&hosts).unwrap();
    let out = h
        .cmd_cutover()
        .args(["--emit-fragments-only", "--emit-fragments"])
        .arg(&hosts)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.to_ascii_lowercase().contains("refuse"), "{s}");

    let docs = h.repo.join("docs");
    fs::create_dir_all(&docs).unwrap();
    let out = h
        .cmd_cutover()
        .args(["--emit-fragments-only", "--emit-fragments"])
        .arg(&docs)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.to_ascii_lowercase().contains("refuse")
            || s.to_ascii_lowercase().contains("public git work tree"),
        "{s}"
    );
}

#[test]
fn emit_vw_without_token_fails() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        &format!("{PLANT}-s"),
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        &format!("{PLANT}-t"),
    );
    let frag = h.work.join("fragments-no-token");
    fs::create_dir_all(&frag).unwrap();
    fs::write(frag.join("surmount-vaultwarden-enable.nix"), "# stale\n").unwrap();
    let out = h
        .cmd_cutover()
        .args(["--emit-fragments-only", "--with-vaultwarden", "--acme-path"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .arg("--emit-fragments")
        .arg(&frag)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let p = frag.join("surmount-vaultwarden-enable.nix");
    if p.is_file() {
        let body = fs::read_to_string(&p).unwrap();
        assert!(!body.contains("adminTokenEnvFile") || !body.contains("enable = true"));
    }
}

#[test]
fn inventory_empty_host_path_zero_wrong_vw() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "tls-cert",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        &format!("{PLANT}-c"),
    );
    h.make_item(
        "tls-key",
        "tls-key",
        "/run/surmount-secrets/tls/key.pem",
        &format!("{PLANT}-k"),
    );
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        &format!("{PLANT}-s"),
    );
    let dir = h.staging.join("stalwart-token");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("attributes"),
        "surmount.kind=stalwart-token\nsurmount.host=\nsurmount.path=/run/surmount-secrets/ui/stalwart-api-token\n",
    )
    .unwrap();
    fs::write(dir.join("secret"), format!("{PLANT}-t")).unwrap();
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("stalwart-token"), "{s}");

    h.reset_staging();
    h.make_item(
        "tls-cert",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        "c",
    );
    h.make_item(
        "tls-key",
        "tls-key",
        "/run/surmount-secrets/tls/key.pem",
        "k",
    );
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    let dir = h.staging.join("stalwart-token");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("attributes"),
        "surmount.kind=stalwart-token\nsurmount.host=mail-lab\nsurmount.path=\n",
    )
    .unwrap();
    fs::write(dir.join("secret"), "t").unwrap();
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(combined(&out).contains("stalwart-token"));

    h.reset_staging();
    h.make_item(
        "tls-cert",
        "tls-cert",
        "/run/surmount-secrets/tls/cert.pem",
        "c",
    );
    h.make_item(
        "tls-key",
        "tls-key",
        "/run/surmount-secrets/tls/key.pem",
        "k",
    );
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    let dir = h.staging.join("stalwart-token");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("attributes"),
        "surmount.kind=stalwart-token\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/ui/stalwart-api-token\n",
    )
    .unwrap();
    fs::write(dir.join("secret"), "").unwrap();
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.contains("stalwart-token")
            || s.to_ascii_lowercase().contains("empty")
            || s.contains("zero"),
        "{s}"
    );

    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    h.make_item(
        "vaultwarden-admin",
        "vaultwarden-admin",
        "/run/surmount-secrets/other/wrong.env",
        &format!("ADMIN_TOKEN={PLANT}-vw"),
    );
    let out = h
        .cmd_inv()
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--with-vaultwarden", "--acme-path"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("vaultwarden-admin"), "{s}");
}

#[test]
fn inventory_only_refuses_in_tree() {
    let h = Harness::new();
    let stg = h.repo.join("inv-in-tree");
    fs::create_dir_all(stg.join("session-secret")).unwrap();
    fs::write(
        stg.join("session-secret/attributes"),
        "surmount.kind=session-secret\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/ui/session-secret\n",
    )
    .unwrap();
    fs::write(stg.join("session-secret/secret"), "x").unwrap();
    let out = h
        .cmd_cutover()
        .args(["--inventory-only", "--acme-path"])
        .arg("--staging")
        .arg(&stg)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.to_ascii_lowercase().contains("outside the public git"),
        "{s}"
    );
}

#[test]
fn profile_off_removes_stale_vw_fragment() {
    let h = Harness::new();
    h.complete_https();
    let frag = h.work.join("fragments-stale-vw");
    fs::create_dir_all(&frag).unwrap();
    fs::write(
        frag.join("surmount-vaultwarden-enable.nix"),
        "surmount.vaultwarden.enable = true;\n",
    )
    .unwrap();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .arg("--emit-fragments")
        .arg(&frag)
        .args(["--skip-deploy", "--skip-secrets-install"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", combined(&out));
    assert!(!frag.join("surmount-vaultwarden-enable.nix").exists());
}

#[test]
fn acme_path_dry_run_plans_ensure_parents() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path"])
        .arg("--dest-root")
        .arg(&h.dest)
        .arg("--skip-deploy")
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("--ensure-acme-parents"), "{s}");
    assert!(!h.dest.join("run/surmount-secrets/tls").exists());
}

#[test]
fn generate_material_fills_non_ca() {
    let h = Harness::new();
    let staging_gen = h.work.join("gen-staging");
    fs::create_dir_all(&staging_gen).unwrap();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&staging_gen)
        .args([
            "--host-id",
            "mail-lab",
            "--generate-material",
            "--acme-path",
            "--with-vaultwarden",
            "--skip-deploy",
            "--skip-secrets-install",
        ])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(staging_gen.join("session-secret/secret").is_file());
    assert!(staging_gen.join("stalwart-token/secret").is_file());
    assert!(staging_gen.join("vaultwarden-admin/secret").is_file());
    assert!(!s.contains("ADMIN_TOKEN="), "{s}");
    let mode = fs::metadata(staging_gen.join("session-secret/secret"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let dmode = fs::metadata(staging_gen.join("session-secret"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dmode, 0o700);
    assert!(
        !staging_gen
            .join("session-secret/secret")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn generate_material_dies_on_broken_vw() {
    let h = Harness::new();
    let staging_broken = h.work.join("gen-broken-vw");
    fs::create_dir_all(staging_broken.join("vaultwarden-admin")).unwrap();
    fs::write(
        staging_broken.join("vaultwarden-admin/attributes"),
        "surmount.kind=vaultwarden-admin\nsurmount.host=mail-lab\nsurmount.path=/run/surmount-secrets/other/wrong.env\n",
    )
    .unwrap();
    fs::write(
        staging_broken.join("vaultwarden-admin/secret"),
        format!("ADMIN_TOKEN={PLANT}-broken-vw\n"),
    )
    .unwrap();
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&staging_broken)
        .args([
            "--host-id",
            "mail-lab",
            "--generate-material",
            "--acme-path",
            "--with-vaultwarden",
            "--skip-deploy",
            "--skip-secrets-install",
        ])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("vaultwarden-admin"), "{s}");
    assert!(
        s.contains("fails inventory") || s.contains("path-mismatch"),
        "{s}"
    );
    assert!(!s.contains(PLANT), "{s}");
    let attr = fs::read_to_string(staging_broken.join("vaultwarden-admin/attributes")).unwrap();
    assert!(attr.contains("wrong.env"));
}

#[test]
fn live_without_target_fails() {
    let h = Harness::new();
    h.complete_https();
    let out = h
        .cmd_cutover()
        .arg("--live")
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .arg("--host-local")
        .arg(&h.hl)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.to_ascii_lowercase().contains("target"), "{s}");
}

#[test]
fn namecheap_require_kind_when_present() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    h.make_item(
        "namecheap-api",
        "namecheap-api",
        "/run/surmount-secrets/acme/namecheap.env",
        "ApiUser=lab\nApiKey=SURMOUNT-TEST-NAMECHEAP-PLACEHOLDER\nClientIp=127.0.0.1\nSLD=example\nTLD=invalid\n",
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path"])
        .arg("--dest-root")
        .arg(&h.dest)
        .arg("--skip-deploy")
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("--require-kind namecheap-api"), "{s}");
    assert!(s.contains("--require-kind session-secret"), "{s}");
    assert!(s.contains("--require-kind stalwart-token"), "{s}");
    assert!(!s.contains("--require-kind tls-cert"), "{s}");
    assert!(!s.contains("SURMOUNT-TEST-NAMECHEAP-PLACEHOLDER"), "{s}");
}

#[test]
fn free_443_dry_run_plans_driver() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path"])
        .arg("--dest-root")
        .arg(&h.dest)
        .args(["--skip-deploy", "--free-443"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("free-stalwart-public-443"), "{s}");
    assert!(
        s.contains("--dry-run") || s.contains("--skip-ss-check"),
        "{s}"
    );
}

#[test]
fn host_profile_writes_fragment() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path"])
        .arg("--host-local")
        .arg(&h.hl)
        .arg("--host-profile")
        .arg(sample_profile())
        .args(["--skip-deploy", "--skip-secrets-install"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    let frag = fs::read_to_string(h.hl.join("host-local-acme.nix")).unwrap();
    assert!(s.contains("render-host-profile-acme"), "{s}");
    assert!(frag.contains("acme") && frag.contains("example.invalid"));
    assert!(!frag.to_ascii_lowercase().contains("apikey"));
}

#[test]
fn host_profile_without_host_local_notes() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path"])
        .arg("--host-profile")
        .arg(sample_profile())
        .args(["--skip-deploy", "--skip-secrets-install"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.to_ascii_lowercase().contains("host-local"), "{s}");
    assert!(s.contains("would run"), "{s}");
}

#[test]
fn live_host_profile_without_host_local_fails() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--live"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--target", "root@example.test"])
        .arg("--host-profile")
        .arg(sample_profile())
        .args(["--skip-deploy", "--skip-secrets-install"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.to_ascii_lowercase().contains("host-local")
            || s.to_ascii_lowercase().contains("host-profile"),
        "{s}"
    );
}

#[test]
fn with_vaultwarden_adds_require_kind() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    h.make_item(
        "vaultwarden-admin",
        "vaultwarden-admin",
        "/run/surmount-secrets/vaultwarden/admin.env",
        &format!("ADMIN_TOKEN={PLANT}-vw"),
    );
    let out = h
        .cmd_cutover()
        .args(["--dry-run"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--acme-path", "--with-vaultwarden"])
        .arg("--dest-root")
        .arg(&h.dest)
        .arg("--skip-deploy")
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("--require-kind vaultwarden-admin"), "{s}");
    assert!(!s.contains(PLANT), "{s}");
}

#[test]
fn step_material_honesty() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--step", "material", "--dry-run", "--acme-path"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("step=material"), "{s}");
    assert!(s.to_ascii_lowercase().contains("will not claim"), "{s}");
    assert!(
        s.contains("next command") && s.contains("--step prep"),
        "{s}"
    );
}

#[test]
fn step_prove_honesty() {
    let h = Harness::new();
    let out = h
        .cmd_cutover()
        .args(["--step", "prove", "--dry-run"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.to_ascii_lowercase().contains("will not claim"), "{s}");
    assert!(s.to_ascii_lowercase().contains("prove checklist"), "{s}");
    assert!(
        s.to_ascii_lowercase().contains("does not claim le issued"),
        "{s}"
    );
}

#[test]
fn unknown_step_fails() {
    let h = Harness::new();
    let out = h
        .cmd_cutover()
        .args(["--step", "not-a-step"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.to_ascii_lowercase().contains("unknown") || s.contains("step"),
        "{s}"
    );
}

#[test]
fn remote_free_443_plan() {
    let h = Harness::new();
    let out = h
        .cmd_cutover()
        .args([
            "--step",
            "free-443",
            "--dry-run",
            "--target",
            "root@example.test",
        ])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("remote free-443"), "{s}");
    assert!(s.contains("root@example.test"), "{s}");
    assert!(
        s.contains("free-stalwart-public-443") || s.contains("bash -s"),
        "{s}"
    );
    assert!(
        s.contains("/var/lib/surmount/secrets/ui/stalwart-api-token"),
        "{s}"
    );
}

#[test]
fn step_dns_plans_domains() {
    let h = Harness::new();
    let out = h
        .cmd_cutover()
        .args(["--step", "dns", "--dry-run"])
        .arg("--host-profile")
        .arg(sample_profile())
        .args(["--dns-a", "203.0.113.10"])
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(s.contains("dns plan domain"), "{s}");
    assert!(s.contains("services.example.invalid"), "{s}");
    assert!(s.contains("dns-zone-namecheap"), "{s}");
    assert!(
        s.to_ascii_lowercase().contains("will not claim")
            || s.to_ascii_lowercase().contains("ban concurrent"),
        "{s}"
    );
    assert!(!s.contains("ApiKey="), "{s}");
}

#[test]
fn step_all_dry_compose() {
    let h = Harness::new();
    h.reset_staging();
    h.make_item(
        "session-secret",
        "session-secret",
        "/run/surmount-secrets/ui/session-secret",
        "s",
    );
    h.make_item(
        "stalwart-token",
        "stalwart-token",
        "/run/surmount-secrets/ui/stalwart-api-token",
        "t",
    );
    let out = h
        .cmd_cutover()
        .args(["--step", "all", "--dry-run", "--acme-path"])
        .arg("--staging")
        .arg(&h.staging)
        .args(["--host-id", "mail-lab", "--target", "root@example.test"])
        .arg("--host-local")
        .arg(&h.hl)
        .arg("--host-profile")
        .arg(sample_profile())
        .arg("--dest-root")
        .arg(&h.dest)
        .output()
        .unwrap();
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("step=material"), "{s}");
    assert!(
        s.contains("step=prep") || s.contains("render-host-profile-acme"),
        "{s}"
    );
    assert!(
        s.contains("step=install") || s.contains("secrets-install"),
        "{s}"
    );
    assert!(
        s.contains("remote free-443") || s.contains("free-443"),
        "{s}"
    );
    assert!(
        s.contains("prove checklist") || s.contains("step=prove"),
        "{s}"
    );
    assert!(s.to_ascii_lowercase().contains("will not claim"), "{s}");
}
