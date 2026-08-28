//! Hermetic CLI contracts. Fake stalwart-cli / systemctl / ss only. No live Stalwart.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-stalwart-ops-{}-{}-{}",
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

fn write_exec(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
    path
}

fn chmod_600(path: &Path) {
    let mut p = fs::metadata(path).unwrap().permissions();
    p.set_mode(0o600);
    fs::set_permissions(path, p).unwrap();
}

const PLANT: &str = "SURMOUNT-TEST-STALWART-TOKEN-dkim-9f3c2a1b-do-not-log";

fn dkim_bin() -> &'static str {
    env!("CARGO_BIN_EXE_register-dkim")
}
fn free_bin() -> &'static str {
    env!("CARGO_BIN_EXE_free-stalwart-public-443")
}
fn tls_bin() -> &'static str {
    env!("CARGO_BIN_EXE_point-stalwart-mail-tls")
}
fn add_bin() -> &'static str {
    env!("CARGO_BIN_EXE_add-stalwart-token")
}
fn boot_bin() -> &'static str {
    env!("CARGO_BIN_EXE_bootstrap-stalwart-api-token")
}
fn rec_bin() -> &'static str {
    env!("CARGO_BIN_EXE_stalwart-recovery-unlock")
}

/// Drop host-leaking env so Nix sandbox / operator laptop cannot SSH or pick
/// a live SURMOUNT_DEPLOY_TARGET. Tests pass --host / --dest-root / --cli.
fn hermetic_cmd(bin: &str) -> Command {
    let mut c = Command::new(bin);
    c.env_remove("SURMOUNT_DEPLOY_TARGET")
        .env_remove("SURMOUNT_SECRETS_TARGET")
        .env_remove("SURMOUNT_SECRETS_HOST_ID")
        .env_remove("SURMOUNT_SECRETS_STAGING")
        .env_remove("SURMOUNT_FREE_443_PLAN")
        .env_remove("SURMOUNT_REGISTER_DKIM_DOMAIN")
        .env_remove("SURMOUNT_REGISTER_DKIM_ED25519_PEM")
        .env_remove("SURMOUNT_REGISTER_DKIM_RSA_PEM")
        .env_remove("SURMOUNT_REGISTER_DKIM_SYSTEMCTL")
        .env_remove("SURMOUNT_FREE_443_SS")
        .env_remove("SURMOUNT_FREE_443_SYSTEMCTL")
        .env_remove("SURMOUNT_RECOVERY_SSH")
        .env_remove("SURMOUNT_RECOVERY_SYSTEMCTL")
        .env_remove("STALWART_URL")
        .env_remove("STALWART_CLI")
        .env_remove("STALWART_PASSWORD")
        .env_remove("STALWART_USER")
        .env_remove("STALWART_TOKEN")
        .env_remove("STALWART_TOKEN_FILE")
        .env_remove("HOSTNAME");
    c
}

#[test]
fn dkim_help_and_blocked_token() {
    let out = Command::new(dkim_bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("--dry-run") && s.contains("--token-file") && s.contains("loopback"));

    let work = temp_dir("dkim-miss");
    let cli = write_exec(&work, "stalwart-cli", "#!/bin/sh\nexit 0\n");
    let out = Command::new(dkim_bin())
        .env_remove("STALWART_TOKEN")
        .env_remove("STALWART_TOKEN_FILE")
        .args(["--dry-run", "--token-file"])
        .arg(work.join("missing-token"))
        .args(["--cli"])
        .arg(&cli)
        .args([
            "--url",
            "http://127.0.0.1:8080",
            "--skip-sandbox-check",
            "--skip-pem-check",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("BLOCKED"));
    assert!(!err.contains(PLANT));
}

#[test]
fn dkim_refuse_non_loopback() {
    let work = temp_dir("dkim-url");
    let tok = work.join("t");
    fs::write(&tok, format!("{PLANT}\n")).unwrap();
    chmod_600(&tok);
    let cli = write_exec(&work, "stalwart-cli", "#!/bin/sh\nexit 0\n");
    let out = Command::new(dkim_bin())
        .env_remove("STALWART_TOKEN")
        .args(["--dry-run", "--token-file"])
        .arg(&tok)
        .args(["--cli"])
        .arg(&cli)
        .args([
            "--url",
            "http://10.0.0.1:8080",
            "--skip-sandbox-check",
            "--skip-pem-check",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.to_lowercase().contains("loopback"));
}

#[test]
fn dkim_dry_run_and_live() {
    let work = temp_dir("dkim-ok");
    let tok = work.join("t");
    fs::write(&tok, format!("{PLANT}\n")).unwrap();
    chmod_600(&tok);
    let dkim = work.join("dkim");
    fs::create_dir_all(&dkim).unwrap();
    fs::write(dkim.join("ed.pem"), "HERMETIC-NOT-A-KEY\n").unwrap();
    fs::write(dkim.join("rsa.pem"), "HERMETIC-NOT-A-KEY\n").unwrap();
    let log = work.join("cli.log");
    let state = work.join("sig-state");
    fs::write(&state, "empty\n").unwrap();
    let cli = write_exec(
        &work,
        "stalwart-cli",
        &format!(
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >>'{log}'
args="$*"
case "$args" in
  *'query Domain'*) echo '{{"items":[{{"id":"dom-surmount-test","name":"surmount.systems"}}]}}'; exit 0 ;;
  *'query DkimSignature'*)
    state="$(cat '{state}' 2>/dev/null || echo empty)"
    if [ "$state" = both ]; then
      echo '{{"items":[{{"id":"sig-ed","selector":"stalwart","stage":"active","domainId":"dom-surmount-test"}},{{"id":"sig-rsa","selector":"stalwart-rsa","stage":"active","domainId":"dom-surmount-test"}}]}}'
    else
      echo '{{"items":[]}}'
    fi
    exit 0 ;;
  *'create DkimSignature/Dkim1Ed25519Sha256'*) echo ed-only >'{state}'; echo 'Created DkimSignature sig-ed'; exit 0 ;;
  *'create DkimSignature/Dkim1RsaSha256'*) echo both >'{state}'; echo 'Created DkimSignature sig-rsa'; exit 0 ;;
  *) echo unexpected >&2; exit 1 ;;
esac
"#,
            log = log.display(),
            state = state.display()
        ),
    );
    let sys = write_exec(
        &work,
        "systemctl",
        "#!/bin/sh\necho ProtectSystem=strict\necho ReadOnlyPaths=/var/lib/surmount/secrets/mail/dkim\nexit 0\n",
    );
    let common = |c: &mut Command| {
        c.args(["--token-file"])
            .arg(&tok)
            .args(["--cli"])
            .arg(&cli)
            .args([
                "--url",
                "http://127.0.0.1:8080",
                "--domain",
                "surmount.systems",
            ])
            .args(["--ed25519-pem"])
            .arg(dkim.join("ed.pem"))
            .args(["--rsa-pem"])
            .arg(dkim.join("rsa.pem"))
            .args(["--systemctl-cmd"])
            .arg(&sys);
    };
    let mut c = hermetic_cmd(dkim_bin());
    common(&mut c);
    c.arg("--dry-run");
    let out = c.output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains("would create"));
    assert!(!err.contains(PLANT));
    let clog = fs::read_to_string(&log).unwrap();
    assert!(clog.contains("query Domain"));
    assert!(!clog.contains("create DkimSignature"));

    fs::write(&log, "").unwrap();
    fs::write(&state, "empty\n").unwrap();
    let mut c = hermetic_cmd(dkim_bin());
    common(&mut c);
    c.arg("--live");
    let out = c.output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    let err_l = err.to_lowercase();
    assert!(err_l.contains("sign-ready"), "{err}");
    assert!(err_l.contains("not claiming outbound"), "{err}");
    let clog = fs::read_to_string(&log).unwrap();
    assert!(clog.contains("Dkim1Ed25519Sha256"));
    assert!(clog.contains("Dkim1RsaSha256"));
}

#[test]
fn free443_blocked_and_dry_run() {
    let work = temp_dir("443");
    // Crane filterCargoSources drops repo nix/*.ndjson. Write a fixture plan.
    let plan = work.join("plan.ndjson");
    fs::write(
        &plan,
        "{\"@type\":\"destroy\",\"object\":\"NetworkListener\",\"value\":{\"name\":\"https\"}}\n",
    )
    .unwrap();
    let cli = write_exec(
        &work,
        "stalwart-cli",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>'{}'\n\
case \"$*\" in *query*) echo '{{}}'; exit 0;; *apply*) echo dry-run ok; exit 0;; *) exit 1;; esac\n",
            work.join("cli.log").display()
        ),
    );
    let out = hermetic_cmd(free_bin())
        .args(["--dry-run", "--token-file"])
        .arg(work.join("missing"))
        .args(["--plan"])
        .arg(&plan)
        .args(["--cli"])
        .arg(&cli)
        .arg("--skip-ss-check")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));

    let tok = work.join("t");
    fs::write(&tok, format!("{PLANT}\n")).unwrap();
    chmod_600(&tok);
    let out = hermetic_cmd(free_bin())
        .args(["--dry-run", "--token-file"])
        .arg(&tok)
        .args(["--plan"])
        .arg(&plan)
        .args(["--cli"])
        .arg(&cli)
        .arg("--skip-ss-check")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains("dry-run"));
    assert!(!err.contains(PLANT));
    let clog = fs::read_to_string(work.join("cli.log")).unwrap();
    assert!(clog.contains("apply") && clog.contains("--dry-run"));
}

#[test]
fn add_token_install_dest_root() {
    let work = temp_dir("add");
    let stage = work.join("stage");
    let dest = work.join("dest");
    fs::create_dir_all(stage.join("stalwart-token")).unwrap();
    fs::write(
        stage.join("stalwart-token/secret"),
        "SYNTHETIC-STALWART-TOKEN-NOT-REAL-9f3c2a1b",
    )
    .unwrap();
    fs::write(
        stage.join("stalwart-token/attributes"),
        "surmount.kind=stalwart-token\nsurmount.host=surmount-1\nsurmount.path=/var/lib/surmount/secrets/ui/stalwart-api-token\n",
    )
    .unwrap();
    chmod_600(&stage.join("stalwart-token/secret"));
    let out = Command::new(add_bin())
        .args(["--host", "surmount-1", "--staging"])
        .arg(&stage)
        .args(["--dest-root"])
        .arg(&dest)
        .arg("--install-only")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("SYNTHETIC-STALWART-TOKEN-NOT-REAL-9f3c2a1b"));
    let want = dest.join("var/lib/surmount/secrets/ui/stalwart-api-token");
    assert!(want.is_file());
}

#[test]
fn add_token_world_readable_refused() {
    let work = temp_dir("wr");
    let tf = work.join("tok");
    fs::write(&tf, "secret\n").unwrap();
    let mut p = fs::metadata(&tf).unwrap().permissions();
    p.set_mode(0o644);
    fs::set_permissions(&tf, p).unwrap();
    let out = Command::new(add_bin())
        .args(["--host", "surmount-1", "--staging"])
        .arg(work.join("stage"))
        .args(["--token-file"])
        .arg(&tf)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("world-accessible") || err.contains("0600"));
}

#[test]
fn bootstrap_empty_password_fails() {
    let work = temp_dir("boot");
    let pf = work.join("p");
    fs::write(&pf, "\n").unwrap();
    chmod_600(&pf);
    let out = Command::new(boot_bin())
        .env_remove("STALWART_PASSWORD")
        .args(["--password-file"])
        .arg(&pf)
        .args(["--staging"])
        .arg(work.join("stage"))
        .arg("--no-install")
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn bootstrap_mint_stage_only() {
    let work = temp_dir("mint");
    let pf = work.join("p");
    fs::write(&pf, "SYNTHETIC-ADMIN-PASSWORD-NOT-REAL-7e4b\n").unwrap();
    chmod_600(&pf);
    let cli = write_exec(
        &work,
        "stalwart-cli",
        "#!/bin/sh\nset -eu\n\
if printf '%s' \"$*\" | grep -q SYNTHETIC-ADMIN-PASSWORD; then echo leaked >&2; exit 99; fi\n\
case \"$*\" in\n\
  *query*domain*) echo '{\"items\":[{\"id\":\"d1\",\"name\":\"surmount.systems\"}]}'; exit 0 ;;\n\
  *query*account*) echo '{\"items\":[{\"id\":\"a\",\"name\":\"admin\",\"emailAddress\":\"admin@surmount.systems\",\"roles\":{\"@type\":\"Admin\"}}]}'; exit 0 ;;\n\
  *create*apikey*) printf '%s\\n' 'Created ApiKey synth111' '' '  Secret: API_SYNTHETIC_TEST_ONLY_NOT_A_REAL_KEY_9f3c2a1b'; exit 0 ;;\n\
  *) echo unexpected \"$*\" >&2; exit 1 ;;\n\
esac\n",
    );
    let out = Command::new(boot_bin())
        .env_remove("STALWART_PASSWORD")
        .args(["--password-file"])
        .arg(&pf)
        .args(["--cli"])
        .arg(&cli)
        .args(["--staging"])
        .arg(work.join("stage"))
        .arg("--no-install")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("API_SYNTHETIC_TEST_ONLY_NOT_A_REAL_KEY_9f3c2a1b"));
    assert!(!err.contains("SYNTHETIC-ADMIN-PASSWORD-NOT-REAL-7e4b"));
    let secret = fs::read_to_string(work.join("stage/stalwart-token/secret")).unwrap();
    assert!(secret.contains("API_SYNTHETIC"));
}

#[test]
fn recovery_generate_and_strip() {
    let work = temp_dir("rec");
    let stage = work.join("stage");
    let dest = work.join("dest");
    let host_id = "hermetic-test-host";
    let out = hermetic_cmd(rec_bin())
        .env("TMPDIR", &work)
        .args(["--generate", "--staging"])
        .arg(&stage)
        .args(["--host", host_id, "--no-install"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains(host_id), "{err}");
    assert!(!err.contains("host=surmount-1"), "{err}");
    let body = fs::read_to_string(stage.join("stalwart-recovery-admin/secret")).unwrap();
    assert!(body.starts_with("STALWART_RECOVERY_ADMIN=admin:"));
    assert!(!err.contains(&body[body.find(':').unwrap() + 1..]));

    let out = hermetic_cmd(rec_bin())
        .env("TMPDIR", &work)
        .args(["--generate", "--install", "--dest-root"])
        .arg(&dest)
        .args(["--staging"])
        .arg(&stage)
        .args(["--host", host_id, "--no-restart", "--no-probe"])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("host=surmount-1"), "{err}");
    let envp = dest.join("var/lib/surmount/secrets/stalwart/recovery.env");
    assert!(envp.is_file());
    let dropin = dest.join("etc/systemd/system/stalwart-mail.service.d/recovery-admin.conf");
    let d = fs::read_to_string(&dropin).unwrap();
    assert!(d.contains("EnvironmentFile=-/var/lib/surmount/secrets/stalwart/recovery.env"));
    assert!(!d.contains("STALWART_RECOVERY_ADMIN="));

    let out = hermetic_cmd(rec_bin())
        .env("TMPDIR", &work)
        .args(["--strip", "--dest-root"])
        .arg(&dest)
        .args(["--no-restart", "--dry-run"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(envp.is_file());
    let out = hermetic_cmd(rec_bin())
        .env("TMPDIR", &work)
        .args(["--strip", "--dest-root"])
        .arg(&dest)
        .arg("--no-restart")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!envp.exists());
}

#[test]
fn recovery_path_dotdot_refused() {
    let out = Command::new(rec_bin())
        .args([
            "--strip",
            "--dest-root",
            "/tmp",
            "--path",
            "/var/lib/surmount/secrets/../escape",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn json_domain_id_matches_name() {
    let blob = r#"{"items":[{"id":"other","name":"example.test"},{"id":"dom-surmount-test","name":"surmount.systems"}]}"#;
    assert_eq!(
        surmount_stalwart_ops::find_domain_id(blob, "surmount.systems").as_deref(),
        Some("dom-surmount-test")
    );
}

#[test]
fn account_admin_not_false_positive() {
    let incidental = r#"{"items":[{"id":"u","name":"hunter","emailAddress":"hunter@surmount.systems","description":"not an admin user"}]}"#;
    assert!(!surmount_stalwart_ops::account_query_has_admin(
        incidental,
        "admin",
        "surmount.systems"
    ));
}

#[test]
fn mail_tls_help() {
    let out = Command::new(tls_bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("--dry-run") && s.contains("Certificate"));
}
