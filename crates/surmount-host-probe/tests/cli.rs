//! Hermetic CLI contracts for inxi/btop/tls-hybrid. Synthetic example.test only.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn inxi() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-inxi-host")
}
fn btop() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-btop-host")
}
fn tls() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-tls-hybrid")
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "surmount-host-probe-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn chmod_x(p: &std::path::Path) {
    let mut perms = fs::metadata(p).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(p, perms).unwrap();
}

fn fake_ssh(dir: &std::path::Path) -> (PathBuf, PathBuf) {
    let log = dir.join("ssh.log");
    let bin = dir.join("fake-ssh");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\nprintf 'ssh-args: %s\\n' \"$*\" >>{}\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    chmod_x(&bin);
    (bin, log)
}

#[test]
fn inxi_help() {
    let out = Command::new(inxi()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("inxi"));
    assert!(t.to_ascii_lowercase().contains("sudo"));
    assert!(t.contains("SURMOUNT_DEPLOY_TARGET"));
}

#[test]
fn inxi_missing_target() {
    let dir = temp_dir("empty-xdg");
    let out = Command::new(inxi())
        .arg("--dry-run")
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .env_remove("SURMOUNT_AGENT_TARGET_ENV")
        .env("XDG_DATA_HOME", &dir)
        .env("HOME", dir.join("empty-home"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("agent-target.env")
            || err.contains("SURMOUNT_DEPLOY_TARGET")
            || err.contains("target required")
    );
}

#[test]
fn inxi_option_shaped_target() {
    let out = Command::new(inxi())
        .args(["--dry-run", "--target", "-oProxyCommand=true"])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn inxi_dry_run_defaults() {
    let out = Command::new(inxi())
        .args(["--dry-run", "--target", "root@example.test"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("inxi"));
    assert!(t.contains("example.test"));
    assert!(t.contains("BatchMode") || t.contains("publickey"));
    assert!(!t.split_whitespace().any(|w| w == "sudo"));
    assert!(!t.split_whitespace().any(|w| w == "-t"));
    assert!(t.contains("-Fxxxz"));
    assert!(t.contains("-c0"));
}

#[test]
fn inxi_extra_args_replace_default() {
    let out = Command::new(inxi())
        .args([
            "--dry-run",
            "--target",
            "root@example.test",
            "--",
            "-C",
            "-c0",
        ])
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("-C"));
    assert!(!t.contains("-Fxxxz"));
}

#[test]
fn inxi_bare_flags() {
    let out = Command::new(inxi())
        .args(["--dry-run", "--target", "root@example.test", "-C", "-c0"])
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("-C"));
    assert!(!t.contains("-Fxxxz"));
}

#[test]
fn inxi_userland() {
    let out = Command::new(inxi())
        .args(["--dry-run", "--userland", "--target", "root@example.test"])
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("nixpkgs#inxi"));
    assert!(!t.split_whitespace().any(|w| w == "sudo"));
}

#[test]
fn inxi_env_file() {
    let dir = temp_dir("env");
    let envf = dir.join("agent-target.env");
    fs::write(&envf, "export SURMOUNT_DEPLOY_TARGET=root@example.test\n").unwrap();
    let out = Command::new(inxi())
        .arg("--dry-run")
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .env("SURMOUNT_AGENT_TARGET_ENV", &envf)
        .output()
        .unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("example.test") && t.contains("inxi"));
}

#[test]
fn inxi_fake_ssh() {
    let dir = temp_dir("ssh");
    let (ssh, log) = fake_ssh(&dir);
    let out = Command::new(inxi())
        .env("SURMOUNT_DEPLOY_TARGET", "root@example.test")
        .env("SURMOUNT_INXI_SSH", &ssh)
        .output()
        .unwrap();
    assert!(out.status.success());
    let args = fs::read_to_string(log).unwrap_or_default();
    assert!(args.contains("inxi") && args.contains("example.test"));
    assert!(!args.split_whitespace().any(|w| w == "sudo"));
    assert!(!args.split_whitespace().any(|w| w == "-t"));
}

#[test]
fn inxi_missing_ssh() {
    let dir = temp_dir("nosh");
    let empty = dir.join("empty-bin");
    fs::create_dir_all(&empty).unwrap();
    let out = Command::new(inxi())
        .env_remove("SURMOUNT_INXI_SSH")
        .env("PATH", &empty)
        .env("SURMOUNT_DEPLOY_TARGET", "root@example.test")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(err.contains("ssh") && err.contains("not found"));
}

fn btop_remote_env(cmd: &mut Command) -> &mut Command {
    cmd.env("SURMOUNT_BTOP_SESSION", "remote")
        .env_remove("SSH_CONNECTION")
        .env_remove("SSH_CLIENT")
        .env_remove("SSH_TTY")
        .env("SURMOUNT_GUEST_MARKER", "0")
}

#[test]
fn btop_help() {
    let out = Command::new(btop()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("btop"));
    assert!(t.contains("Eternal Terminal") || t.contains("et"));
    assert!(t.contains("SURMOUNT_DEPLOY_TARGET"));
    assert!(t.contains("live tty") || t.contains("live terminal"));
    assert!(t.to_ascii_lowercase().contains("nested"));
}

#[test]
fn btop_dry_run_tty() {
    let out = btop_remote_env(&mut Command::new(btop()))
        .args(["--dry-run", "--target", "root@example.test"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("-c") && t.contains("btop"), "{t}");
    assert!(t.contains("example.test"), "{t}");
    assert!(!t.contains(" ssh "), "{t}");
    // -e is et --noexit: after btop the session would drop to a guest shell.
    assert!(
        !t.split_whitespace().any(|w| w == "-e"),
        "just btop must not pass et --noexit (-e): {t}"
    );
}

#[test]
fn btop_fake_et() {
    let dir = temp_dir("btop-et");
    let (et, log) = fake_ssh(&dir);
    let out = btop_remote_env(&mut Command::new(btop()))
        .env("SURMOUNT_DEPLOY_TARGET", "root@example.test")
        .env("SURMOUNT_ET_BIN", &et)
        .env("SURMOUNT_BTOP_REQUIRE_TTY", "0")
        .output()
        .unwrap();
    assert!(out.status.success());
    let args = fs::read_to_string(log).unwrap_or_default();
    assert!(args.contains("-c") && args.contains("btop"), "{args}");
    assert!(args.contains("example.test"), "{args}");
    assert!(
        !args.split_whitespace().any(|w| w == "-e"),
        "et --noexit (-e) leaves a guest shell after btop: {args}"
    );
}

#[test]
fn btop_refuses_non_tty() {
    let dir = temp_dir("btop-notty");
    let (et, log) = fake_ssh(&dir);
    let out = btop_remote_env(&mut Command::new(btop()))
        .env("SURMOUNT_DEPLOY_TARGET", "root@example.test")
        .env("SURMOUNT_ET_BIN", &et)
        .env_remove("SURMOUNT_BTOP_REQUIRE_TTY")
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "piped stdin/stdout is not a live tty; must fail loud, not attach"
    );
    let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(
        err.contains("live tty") || err.contains("not a tty") || err.contains("terminal"),
        "{err}"
    );
    assert!(
        !log.is_file() || fs::read_to_string(log).unwrap_or_default().is_empty(),
        "must not spawn et without a live tty"
    );
}

#[test]
fn btop_nested_guest_dry_run_runs_local() {
    let out = Command::new(btop())
        .arg("--dry-run")
        .env("SURMOUNT_BTOP_SESSION", "local")
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .env_remove("SURMOUNT_AGENT_TARGET_ENV")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "nested guest must not require a deploy target: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let t = String::from_utf8_lossy(&out.stdout);
    let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(t.contains("btop"), "{t}");
    assert!(
        !t.split_whitespace().any(|w| w == "-c"),
        "local guest btop must not invoke et -c: {t}"
    );
    assert!(!t.contains("example.test"), "{t}");
    assert!(
        err.contains("already") || err.contains("nested") || err.contains("this box"),
        "{err}"
    );
}

#[test]
fn btop_nested_ssh_marker_runs_local() {
    let dir = temp_dir("btop-marker");
    let marker = dir.join("root-justfile");
    fs::write(&marker, "# guest marker\n").unwrap();
    let out = Command::new(btop())
        .arg("--dry-run")
        .env_remove("SURMOUNT_BTOP_SESSION")
        .env("SSH_CONNECTION", "present")
        .env("SURMOUNT_GUEST_MARKER", &marker)
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("btop"), "{t}");
    assert!(!t.split_whitespace().any(|w| w == "-c"), "{t}");
}

#[test]
fn btop_nested_execs_local_btop() {
    let dir = temp_dir("btop-local-bin");
    let log = dir.join("btop.log");
    let bin = dir.join("fake-btop");
    fs::write(
        &bin,
        format!(
            "#!/bin/sh\nprintf 'btop-args: %s\\n' \"$*\" >>{}\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    chmod_x(&bin);
    let out = Command::new(btop())
        .env("SURMOUNT_BTOP_SESSION", "local")
        .env("SURMOUNT_BTOP_BIN", &bin)
        .env("SURMOUNT_BTOP_REQUIRE_TTY", "0")
        .env_remove("SURMOUNT_DEPLOY_TARGET")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let args = fs::read_to_string(&log).unwrap_or_default();
    assert!(args.contains("btop-args:"), "{args}");
}

#[test]
fn tls_help() {
    let out = Command::new(tls()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.to_ascii_lowercase().contains("hybrid") || t.contains("BASE_URL"));
}

#[test]
fn tls_blocked_unset() {
    let out = Command::new(tls())
        .env_remove("SURMOUNT_E2E_BASE_URL")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("BLOCKED"));
    assert!(!err.contains("result=HYBRID"));
}

#[test]
fn tls_blocked_empty() {
    let out = Command::new(tls())
        .env("SURMOUNT_E2E_BASE_URL", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn tls_option_shaped_url() {
    let out = Command::new(tls())
        .env("SURMOUNT_E2E_BASE_URL", "-oProxyCommand=true")
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(0);
    assert!(code != 0 && code != 2);
}

#[test]
fn tls_http_refused() {
    let out = Command::new(tls())
        .env("SURMOUNT_E2E_BASE_URL", "http://example.test")
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(0);
    assert!(code != 0 && code != 2);
    let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(err.contains("https"));
}

fn install_mock_openssl(dir: &std::path::Path) {
    let p = dir.join("openssl");
    fs::write(
        &p,
        r#"#!/bin/sh
set -e
mode="${SURMOUNT_TEST_OPENSSL_MODE:-classical}"
if [ "${1:-}" = version ]; then
  echo "OpenSSL mock 3.0.0 for hermetic hybrid probe tests"
  exit 0
fi
if [ "${1:-}" = s_client ]; then
  for a in "$@"; do
    if [ "$a" = "-help" ]; then
      echo " -groups val                Groups to advertise (mock)"
      exit 0
    fi
  done
  if [ "$mode" = mlkem ]; then
    printf '%s\n' "Protocol: TLSv1.3" "New, TLSv1.3, Cipher is TLS_AES_256_GCM_SHA384" "Negotiated TLS1.3 group: X25519MLKEM768"
    exit 0
  fi
  printf '%s\n' "Protocol: TLSv1.3" "New, TLSv1.3, Cipher is TLS_AES_256_GCM_SHA384" "Negotiated TLS1.3 group: X25519"
  exit 0
fi
echo "mock openssl unexpected: $*" >&2
exit 99
"#,
    )
    .unwrap();
    chmod_x(&p);
}

#[test]
fn tls_classical_mock() {
    let dir = temp_dir("openssl-c");
    install_mock_openssl(&dir);
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(tls())
        .env("PATH", path)
        .env("SURMOUNT_TEST_OPENSSL_MODE", "classical")
        .env("SURMOUNT_E2E_BASE_URL", "https://example.test")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(t.contains("result=CLASSICAL_ONLY"));
    assert!(!t.contains("result=HYBRID"));
}

#[test]
fn tls_mlkem_mock() {
    let dir = temp_dir("openssl-m");
    install_mock_openssl(&dir);
    let path = format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(tls())
        .env("PATH", path)
        .env("SURMOUNT_TEST_OPENSSL_MODE", "mlkem")
        .env("SURMOUNT_E2E_BASE_URL", "https://example.test")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let t = String::from_utf8_lossy(&out.stdout);
    assert!(t.contains("result=HYBRID"));
    assert!(t.contains("X25519MLKEM768"));
}
