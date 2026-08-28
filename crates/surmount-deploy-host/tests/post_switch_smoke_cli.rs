//! Hermetic CLI contracts from script/test-deploy-host-post-switch-smoke.sh.
//! Fake systemctl/curl/readlink/ss. Never logs PEM bodies.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

fn smoke() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-deploy-host-post-switch-smoke")
}

fn temp_dir(label: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "surmount-smoke-{label}-{}-{}",
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

fn combined(out: &Output) -> String {
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

struct Fakes {
    bin: PathBuf,
    tls: PathBuf,
}

impl Fakes {
    fn new() -> Self {
        let bin = temp_dir("fake-bin");
        let tls = temp_dir("fake-tls");
        fs::write(
            tls.join("cert.pem"),
            "-----BEGIN CERTIFICATE-----\nLAB\n-----END CERTIFICATE-----\n",
        )
        .unwrap();
        fs::write(
            tls.join("key.pem"),
            format!(
                "{}BEGIN PRIVATE KEY{}\nLAB\n{}END PRIVATE KEY{}\n",
                "-".repeat(5),
                "-".repeat(5),
                "-".repeat(5),
                "-".repeat(5)
            ),
        )
        .unwrap();
        let mut perm = fs::metadata(tls.join("key.pem")).unwrap().permissions();
        perm.set_mode(0o600);
        fs::set_permissions(tls.join("key.pem"), perm).unwrap();

        write_exec(
            &bin,
            "systemctl",
            r#"#!/bin/sh
set -eu
DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
cmd=${1:-}
case "$cmd" in
  is-active)
    unit=${2:-}
    case "$unit" in
      sshd) cat "$DIR/state.sshd" 2>/dev/null || echo inactive ;;
      stalwart-mail) cat "$DIR/state.stalwart" 2>/dev/null || echo inactive ;;
      surmount-management-ui) cat "$DIR/state.ui" 2>/dev/null || echo inactive ;;
      *) echo inactive ;;
    esac
    exit 0
    ;;
  start)
    unit=${2:-}
    if [ "$unit" = "surmount-management-ui" ]; then
      rc=$(cat "$DIR/start_ui_rc" 2>/dev/null || echo 0)
      if [ "$rc" -eq 0 ]; then
        if [ "$(cat "$DIR/start_ui_then_active" 2>/dev/null || echo 1)" = "1" ]; then
          echo active >"$DIR/state.ui"
        fi
      fi
      exit "$rc"
    fi
    exit 1
    ;;
  show)
    if [ "${2:-}" = "-p" ] && [ "${3:-}" = "Environment" ]; then
      out=""
      if [ -f "$DIR/listen" ]; then listen=$(cat "$DIR/listen"); [ -n "$listen" ] && out="$out SURMOUNT_LISTEN=$listen"; fi
      if [ -f "$DIR/listen_mode" ]; then mode=$(cat "$DIR/listen_mode"); [ -n "$mode" ] && out="$out SURMOUNT_LISTEN_MODE=$mode"; fi
      if [ -f "$DIR/tls_cert" ]; then cert=$(cat "$DIR/tls_cert"); [ -n "$cert" ] && out="$out SURMOUNT_TLS_CERT=$cert"; fi
      if [ -f "$DIR/tls_key" ]; then key=$(cat "$DIR/tls_key"); [ -n "$key" ] && out="$out SURMOUNT_TLS_KEY=$key"; fi
      if [ -f "$DIR/acme_enable" ]; then acme=$(cat "$DIR/acme_enable"); [ -n "$acme" ] && out="$out SURMOUNT_ACME_ENABLE=$acme"; fi
      if [ -f "$DIR/redirect_http" ]; then redir=$(cat "$DIR/redirect_http"); [ -n "$redir" ] && out="$out SURMOUNT_REDIRECT_HTTP_TO_HTTPS=$redir"; fi
      echo "$out" | sed 's/^ *//'
      exit 0
    fi
    exit 0
    ;;
  *) exit 0 ;;
esac
"#,
        );
        write_exec(
            &bin,
            "curl",
            r#"#!/bin/sh
set -eu
DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
url=""
for a in "$@"; do
  case "$a" in
    -*) ;;
    *) url=$a ;;
  esac
done
printf '%s\n' "$url" >"$DIR/curl_url"
rc=$(cat "$DIR/curl_rc" 2>/dev/null || echo 0)
exit "$rc"
"#,
        );
        write_exec(
            &bin,
            "readlink",
            r#"#!/bin/sh
set -eu
DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
target=""
for a in "$@"; do
  case "$a" in
    -*) ;;
    *) target=$a ;;
  esac
done
if [ "$target" = "/run/current-system" ] || [ "$target" = "/nix/var/nix/profiles/system" ]; then
  if [ -f "$DIR/generation" ]; then
    cat "$DIR/generation"
    exit 0
  fi
  exit 1
fi
exit 1
"#,
        );
        write_exec(
            &bin,
            "ss",
            r#"#!/bin/sh
set -eu
DIR=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
rc=$(cat "$DIR/ss_rc" 2>/dev/null || echo 0)
if [ -f "$DIR/ss_out" ]; then
  cat "$DIR/ss_out"
fi
exit "$rc"
"#,
        );
        Self { bin, tls }
    }

    fn reset_day1(&self) {
        fs::write(self.bin.join("state.sshd"), "active\n").unwrap();
        fs::write(self.bin.join("state.stalwart"), "active\n").unwrap();
        fs::write(self.bin.join("state.ui"), "active\n").unwrap();
        fs::write(self.bin.join("curl_rc"), "0\n").unwrap();
        fs::write(
            self.bin.join("generation"),
            "/nix/store/fake-gen-aaaa-linux\n",
        )
        .unwrap();
        fs::write(self.bin.join("ss_rc"), "0\n").unwrap();
        for n in [
            "listen",
            "listen_mode",
            "tls_cert",
            "tls_key",
            "acme_enable",
            "redirect_http",
            "ss_out",
            "curl_url",
        ] {
            let _ = fs::remove_file(self.bin.join(n));
        }
    }

    fn run(&self, extra: &[(&str, &str)]) -> Output {
        let mut c = Command::new(smoke());
        c.env(
            "PATH",
            format!(
                "{}:{}",
                self.bin.display(),
                std::env::var("PATH").unwrap_or_default()
            ),
        );
        c.env("SURMOUNT_SMOKE_SYSTEMCTL", self.bin.join("systemctl"));
        c.env("SURMOUNT_SMOKE_CURL", self.bin.join("curl"));
        c.env("SURMOUNT_SMOKE_READLINK", self.bin.join("readlink"));
        c.env("SURMOUNT_SMOKE_SS", self.bin.join("ss"));
        for (k, v) in extra {
            c.env(*k, *v);
        }
        c.output().unwrap()
    }
}

#[test]
fn help_documents_units_health_pem_listen_redirect() {
    let out = Command::new(smoke()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let s = combined(&out);
    let low = s.to_ascii_lowercase();
    assert!(s.contains("surmount-management-ui"), "{s}");
    assert!(low.contains("health"), "{s}");
    assert!(low.contains("pem") || low.contains("tls"), "{s}");
    assert!(s.contains(":443"), "{s}");
    assert!(low.contains("redirect"), "{s}");
    assert!(
        low.contains("soft")
            || low.contains("clearly off")
            || low.contains("redirect") && low.contains("off"),
        "{s}"
    );
}

#[test]
fn all_green_day1() {
    let f = Fakes::new();
    f.reset_day1();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("PASS: unit sshd is active"), "{s}");
    assert!(s.contains("PASS: unit stalwart-mail is active"), "{s}");
    assert!(
        s.contains("PASS: unit surmount-management-ui is active"),
        "{s}"
    );
    assert!(s.contains("PASS: health"), "{s}");
    assert!(s.contains("PASS: generation current"), "{s}");
    assert!(s.contains("PASS: tls pem check not required"), "{s}");
    assert!(s.contains("PASS: listen check not required"), "{s}");
    assert!(s.contains("PASS: acme enable noted"), "{s}");
    let url = fs::read_to_string(f.bin.join("curl_url")).unwrap();
    assert!(url.trim() == "http://127.0.0.1:8090/health", "{url}");
}

#[test]
fn inactive_ui_started() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("state.ui"), "inactive\n").unwrap();
    fs::write(f.bin.join("start_ui_rc"), "0\n").unwrap();
    fs::write(f.bin.join("start_ui_then_active"), "1\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains("PASS: unit surmount-management-ui started and is active"),
        "{s}"
    );
}

#[test]
fn inactive_ui_start_failure() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("state.ui"), "inactive\n").unwrap();
    fs::write(f.bin.join("start_ui_rc"), "1\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(
        s.contains("FAIL: unit surmount-management-ui systemctl start failed"),
        "{s}"
    );
}

#[test]
fn sshd_inactive_fails() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("state.sshd"), "inactive\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("FAIL: unit sshd"), "{s}");
}

#[test]
fn health_curl_failure() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("curl_rc"), "22\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("FAIL: health"), "{s}");
}

#[test]
fn listen_override_loopback_port() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen"), "127.0.0.1:9090\n").unwrap();
    let out = f.run(&[]);
    assert!(out.status.success(), "{}", combined(&out));
    let url = fs::read_to_string(f.bin.join("curl_url")).unwrap();
    assert_eq!(url.trim(), "http://127.0.0.1:9090/health");
}

#[test]
fn public_listen_host_uses_loopback_port_only() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen"), "203.0.113.10:8443\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    let url = fs::read_to_string(f.bin.join("curl_url")).unwrap();
    assert_eq!(url.trim(), "http://127.0.0.1:8443/health");
    let low = s.to_ascii_lowercase();
    assert!(
        low.contains("not loopback")
            || low.contains("no public url")
            || low.contains("loopback port only"),
        "{s}"
    );
    assert!(!url.contains("203.0.113"));
}

#[test]
fn generation_unreadable_fails() {
    let f = Fakes::new();
    f.reset_day1();
    let _ = fs::remove_file(f.bin.join("generation"));
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("FAIL: generation"), "{s}");
}

fn https_edge_green_ss() -> &'static str {
    "LISTEN 0 128 0.0.0.0:80 0.0.0.0:* users:((\"surmount-manag\",pid=1,fd=3))\nLISTEN 0 128 0.0.0.0:443 0.0.0.0:* users:((\"surmount-manag\",pid=1,fd=4))\n"
}

#[test]
fn https_edge_pems_and_listen_green_no_pem_body() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(f.bin.join("listen"), "0.0.0.0:443\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("acme_enable"), "0\n").unwrap();
    fs::write(f.bin.join("ss_out"), https_edge_green_ss()).unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains(&format!(
            "PASS: tls cert path exists: {}",
            f.tls.join("cert.pem").display()
        )),
        "{s}"
    );
    assert!(
        s.contains(&format!(
            "PASS: tls key path exists: {}",
            f.tls.join("key.pem").display()
        )),
        "{s}"
    );
    assert!(s.contains("PASS: acme enable noted (disabled"), "{s}");
    assert!(s.contains("PASS: listen :443 present"), "{s}");
    assert!(s.contains("PASS: listen :80 present"), "{s}");
    assert!(s.contains("PASS: tls key mode 600 owner-only"), "{s}");
    assert!(
        !s.contains("BEGIN CERTIFICATE") && !s.contains("BEGIN PRIVATE KEY") && !s.contains("LAB"),
        "{s}"
    );
}

#[test]
fn https_edge_tls_key_0640_fails_not_note() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(f.bin.join("listen"), "0.0.0.0:443\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("ss_out"), https_edge_green_ss()).unwrap();
    let mut perm = fs::metadata(f.tls.join("key.pem")).unwrap().permissions();
    perm.set_mode(0o640);
    fs::set_permissions(f.tls.join("key.pem"), perm).unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success(), "{s}");
    assert!(
        s.contains("FAIL: tls key mode 640 is not owner-only"),
        "{s}"
    );
    assert!(
        !s.contains("NOTE: tls key mode 640 is not owner-only"),
        "{s}"
    );
}

#[test]
fn https_edge_health_uses_https_loopback() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(f.bin.join("listen"), "0.0.0.0:443\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("ss_out"),
        "LISTEN 0 128 0.0.0.0:80 0.0.0.0:*\nLISTEN 0 128 0.0.0.0:443 0.0.0.0:*\n",
    )
    .unwrap();
    let out = f.run(&[]);
    assert!(out.status.success(), "{}", combined(&out));
    let url = fs::read_to_string(f.bin.join("curl_url")).unwrap();
    assert_eq!(url.trim(), "https://127.0.0.1:443/health");
    assert!(!url.contains("http://127.0.0.1:443"));
}

#[test]
fn missing_pem_paths_fail() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("missing-cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("missing-key.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("ss_out"),
        "LISTEN 0 128 0.0.0.0:80 0.0.0.0:*\nLISTEN 0 128 0.0.0.0:443 0.0.0.0:*\n",
    )
    .unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("FAIL: tls cert path missing"), "{s}");
    assert!(s.contains("FAIL: tls key path missing"), "{s}");
}

#[test]
fn smoke_tls_env_overrides() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("ss_out"),
        "LISTEN 0 128 *:80 *:*\nLISTEN 0 128 *:443 *:*\n",
    )
    .unwrap();
    let cert = f.tls.join("cert.pem");
    let key = f.tls.join("key.pem");
    let out = f.run(&[
        ("SURMOUNT_SMOKE_TLS_CERT", cert.to_str().unwrap()),
        ("SURMOUNT_SMOKE_TLS_KEY", key.to_str().unwrap()),
    ]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains(&format!("PASS: tls cert path exists: {}", cert.display())),
        "{s}"
    );
}

#[test]
fn missing_443_fails() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("ss_out"), "LISTEN 0 128 0.0.0.0:80 0.0.0.0:*\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("FAIL: listen :443"), "{s}");
}

#[test]
fn missing_80_fails_when_redirect_default_on() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("ss_out"), "LISTEN 0 128 0.0.0.0:443 0.0.0.0:*\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(!out.status.success());
    assert!(s.contains("FAIL: listen :80"), "{s}");
}

#[test]
fn missing_80_soft_when_redirect_false() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("redirect_http"), "false\n").unwrap();
    fs::write(f.bin.join("ss_out"), "LISTEN 0 128 0.0.0.0:443 0.0.0.0:*\n").unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(!s.contains("FAIL: listen :80"), "{s}");
    assert!(
        s.contains("PASS: listen :80 not required (redirect off)"),
        "{s}"
    );
    assert!(s.contains("PASS: listen :443 present"), "{s}");
}

#[test]
fn smoke_override_softs_80() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("redirect_http"), "true\n").unwrap();
    fs::write(f.bin.join("ss_out"), "LISTEN 0 128 0.0.0.0:443 0.0.0.0:*\n").unwrap();
    let out = f.run(&[("SURMOUNT_SMOKE_REDIRECT_HTTP_TO_HTTPS", "false")]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(
        s.contains("PASS: listen :80 not required (redirect off)"),
        "{s}"
    );
    assert!(!s.contains("FAIL: listen :80"), "{s}");
}

#[test]
fn acme_enabled_noted_not_fail() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    fs::write(
        f.bin.join("tls_cert"),
        format!("{}\n", f.tls.join("cert.pem").display()),
    )
    .unwrap();
    fs::write(
        f.bin.join("tls_key"),
        format!("{}\n", f.tls.join("key.pem").display()),
    )
    .unwrap();
    fs::write(f.bin.join("acme_enable"), "1\n").unwrap();
    fs::write(f.bin.join("ss_out"), https_edge_green_ss()).unwrap();
    let out = f.run(&[]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("PASS: acme enable noted (enabled)"), "{s}");
    assert!(s.to_ascii_lowercase().contains("acme enabled"), "{s}");
}

#[test]
fn skip_pem_and_listen() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("listen_mode"), "https\n").unwrap();
    let out = f.run(&[
        ("SURMOUNT_SMOKE_SKIP_PEM", "1"),
        ("SURMOUNT_SMOKE_SKIP_LISTEN", "1"),
    ]);
    let s = combined(&out);
    assert!(out.status.success(), "{s}");
    assert!(s.contains("PASS: tls pem check skipped"), "{s}");
    assert!(s.contains("PASS: listen check skipped"), "{s}");
}
