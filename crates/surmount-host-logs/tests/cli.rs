//! Hermetic CLI contracts. Fake journalctl/systemctl only. No live SSH, no sudo.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-host-logs")
}

fn cmd() -> Command {
    let mut c = Command::new(bin());
    c.env_remove("SURMOUNT_HOST_LOGS_JOURNALCTL")
        .env_remove("SURMOUNT_HOST_LOGS_SYSTEMCTL")
        .env_remove("SURMOUNT_HOST_LOGS_JOURNALD_CONF");
    c
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-host-logs-{}-{}-{}",
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

fn pem_begin() -> String {
    ["-----", "BEGIN", " ", "PRIVATE", " ", "KEY", "-----"].concat()
}

fn pem_end() -> String {
    ["-----", "END", " ", "PRIVATE", " ", "KEY", "-----"].concat()
}

fn stub_journalctl(dir: &Path) -> PathBuf {
    let begin = pem_begin();
    let end = pem_end();
    let body = format!(
        "#!/bin/sh\n\
set -eu\n\
args=\"$*\"\n\
case \"$args\" in\n\
  *--disk-usage*)\n\
    echo 'Archived and active journals take up 12.0M on disk.'\n\
    exit 0\n\
    ;;\n\
esac\n\
echo 'sshd[1]: Accepted publickey for nixbuilder'\n\
echo 'GET / Authorization: Bearer SYNTHETIC-NOT-A-SECRET'\n\
echo 'Cookie: session=synthetic-session-value'\n\
echo '{begin}'\n\
echo 'SYNTHETIC-PEM-BODY-NOT-A-REAL-KEY'\n\
echo '{end}'\n\
echo 'token=SYNTHETICVALUE'\n\
exit 0\n"
    );
    write_exec(dir, "journalctl", &body)
}

fn stub_systemctl(dir: &Path) -> PathBuf {
    write_exec(
        dir,
        "systemctl",
        "#!/bin/sh\n\
set -eu\n\
args=\"$*\"\n\
case \"$args\" in\n\
  *--failed*)\n\
    echo '0 loaded units listed.'\n\
    exit 0\n\
    ;;\n\
  *is-active*)\n\
    echo active\n\
    exit 0\n\
    ;;\n\
  *is-enabled*)\n\
    echo enabled\n\
    exit 0\n\
    ;;\n\
esac\n\
exit 0\n",
    )
}

fn path_with(dir: &Path) -> String {
    format!(
        "{}:{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn help_exits_zero_and_names_journalctl_sudo_status_disk() {
    let out = cmd().arg("--help").output().expect("run help");
    assert!(out.status.success(), "stderr={}", combined(&out));
    let text = combined(&out);
    let lower = text.to_ascii_lowercase();
    assert!(text.contains("journalctl"), "{text}");
    assert!(
        lower.contains("sudo")
            && (lower.contains("no sudo")
                || lower.contains("without sudo")
                || lower.contains("does not use sudo")),
        "{text}"
    );
    assert!(text.contains("--status"), "{text}");
    assert!(lower.contains("disk"), "{text}");
}

#[test]
fn status_snapshot_from_stubs_no_sudo_no_follow() {
    let dir = temp_dir("status");
    stub_journalctl(&dir);
    stub_systemctl(&dir);
    let conf = dir.join("journald.conf");
    fs::write(
        &conf,
        "Storage=persistent\nSystemMaxUse=1G\nRateLimitBurst=20000\n",
    )
    .unwrap();

    let out = cmd()
        .arg("--status")
        .env("PATH", path_with(&dir))
        .env("SURMOUNT_HOST_LOGS_JOURNALD_CONF", &conf)
        .output()
        .expect("run --status");
    assert!(out.status.success(), "{}", combined(&out));
    let text = combined(&out);
    assert!(text.contains("== journal disk =="), "{text}");
    assert!(text.contains("12.0M"), "{text}");
    assert!(text.contains("RateLimitBurst=20000"), "{text}");
    assert!(text.contains("== failed units =="), "{text}");
    assert!(text.contains("qemu-guest-agent"), "{text}");
    assert!(text.contains("stalwart-mail"), "{text}");
    assert!(text.contains("surmount-management-ui"), "{text}");
    assert!(text.contains("sshd"), "{text}");
    assert!(text.contains("fail2ban"), "{text}");
    assert!(text.contains("nix-daemon"), "{text}");
    assert!(text.contains("== recent tails (no follow) =="), "{text}");
    assert!(!text.contains(" sudo"), "{text}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains(" -f ") && !stdout.contains("\n-f\n"),
        "status stdout must not be a follow stream marker: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn status_never_prints_authorization_cookies_pem_or_tokens() {
    let dir = temp_dir("redact");
    stub_journalctl(&dir);
    stub_systemctl(&dir);
    let conf = dir.join("journald.conf");
    fs::write(&conf, "Storage=persistent\n").unwrap();

    let out = cmd()
        .arg("--status")
        .env("PATH", path_with(&dir))
        .env("SURMOUNT_HOST_LOGS_JOURNALD_CONF", &conf)
        .output()
        .expect("run --status redact");
    assert!(out.status.success(), "{}", combined(&out));
    let text = combined(&out);
    assert!(!text.contains("SYNTHETIC-NOT-A-SECRET"), "{text}");
    assert!(!text.contains("synthetic-session-value"), "{text}");
    assert!(
        !text.contains("SYNTHETIC-PEM-BODY-NOT-A-REAL-KEY"),
        "{text}"
    );
    assert!(!text.contains("SYNTHETICVALUE"), "{text}");
    assert!(!text.contains(&pem_begin()), "{text}");
    assert!(
        text.contains("<redacted") || text.contains("<redacted-pem>"),
        "{text}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn follow_invokes_journalctl_dash_f_and_redacts() {
    let dir = temp_dir("follow");
    stub_journalctl(&dir);
    stub_systemctl(&dir);

    let out = cmd()
        .env("PATH", path_with(&dir))
        .output()
        .expect("run follow");
    assert!(out.status.success(), "{}", combined(&out));
    let text = combined(&out);
    assert!(text.contains("Accepted publickey"), "{text}");
    assert!(!text.contains("SYNTHETIC-NOT-A-SECRET"), "{text}");
    assert!(!text.contains("synthetic-session-value"), "{text}");
    assert!(!text.contains("SYNTHETICVALUE"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn extra_journalctl_args_are_passed() {
    let dir = temp_dir("extra");
    write_exec(
        &dir,
        "journalctl",
        "#!/bin/sh\nprintf 'argv:%s\\n' \"$*\"\nexit 0\n",
    );
    let out = cmd()
        .args(["--", "-u", "sshd", "-n", "5"])
        .env("PATH", path_with(&dir))
        .output()
        .expect("run extra args");
    assert!(out.status.success(), "{}", combined(&out));
    let text = combined(&out);
    assert!(text.contains("argv:"), "{text}");
    assert!(text.contains("sshd"), "{text}");
    assert!(text.contains("-n"), "{text}");
    assert!(text.contains('5'), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn status_plus_extra_args_fails_loud() {
    let out = cmd()
        .args(["--status", "--", "-u", "sshd"])
        .output()
        .expect("run combo");
    assert!(!out.status.success());
    let text = combined(&out);
    assert!(
        text.to_ascii_lowercase()
            .contains("do not combine --status"),
        "{text}"
    );
}

#[test]
fn missing_journalctl_fails_loud() {
    let dir = temp_dir("missing");
    let out = cmd()
        .env("PATH", dir.display().to_string())
        .output()
        .expect("run missing journalctl");
    assert!(!out.status.success());
    let text = combined(&out);
    assert!(
        text.to_ascii_lowercase().contains("journalctl")
            && text.to_ascii_lowercase().contains("not found"),
        "{text}"
    );
    let _ = fs::remove_dir_all(&dir);
}
