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
# Fifth sample field is systemd Result=. Token "missing" omits the field.
# Exec token "-" is an empty ExecMainStatus=. Absent fifth field is success.
split_sample() {
  line=$1
  state=${line%% *}
  rest=${line#* }
  pid=${rest%% *}
  rest=${rest#* }
  restarts=${rest%% *}
  rest=${rest#* }
  status=${rest%% *}
  case "$rest" in
    *" "*) result=${rest#* } ;;
    *) result=success ;;
  esac
}
cmd=${1:-}
case "$cmd" in
  is-active)
    unit=${2:-}
    case "$unit" in
      sshd) cat "$DIR/state.sshd" 2>/dev/null || echo inactive ;;
      stalwart-mail)
        n=$(cat "$DIR/stalwart_polls" 2>/dev/null || echo 0)
        n=$((n + 1))
        printf '%s\n' "$n" >"$DIR/stalwart_polls"
        if [ -f "$DIR/stalwart_samples" ]; then
          line=$(sed -n "${n}p" "$DIR/stalwart_samples" || true)
          if [ -z "$line" ]; then
            line=$(tail -n 1 "$DIR/stalwart_samples")
          fi
          split_sample "$line"
          printf '%s %s %s %s %s\n' "$state" "$pid" "$restarts" "$status" "$result" >"$DIR/stalwart_current"
          printf '%s\n' "$state" >"$DIR/state.stalwart"
          printf '%s\n' "$state"
          exit 0
        fi
        state=$(cat "$DIR/state.stalwart" 2>/dev/null || echo inactive)
        if [ -f "$DIR/stalwart_flip_after" ]; then
          need=$(cat "$DIR/stalwart_flip_after")
          if [ "$n" -ge "$need" ]; then
            state=$(cat "$DIR/stalwart_flip_to")
            printf '%s\n' "$state" >"$DIR/state.stalwart"
          fi
        fi
        printf '%s\n' "$state"
        exit 0
        ;;
      surmount-management-ui)
        if [ -f "$DIR/ui_samples" ]; then
          n=$(cat "$DIR/ui_polls" 2>/dev/null || echo 0)
          n=$((n + 1))
          printf '%s\n' "$n" >"$DIR/ui_polls"
          line=$(sed -n "${n}p" "$DIR/ui_samples" || true)
          if [ -z "$line" ]; then
            line=$(tail -n 1 "$DIR/ui_samples")
          fi
          split_sample "$line"
          printf '%s %s %s %s %s\n' "$state" "$pid" "$restarts" "$status" "$result" >"$DIR/ui_current"
          printf '%s\n' "$state"
          exit 0
        fi
        cat "$DIR/state.ui" 2>/dev/null || echo inactive
        exit 0
        ;;
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
    unit=""
    for a in "$@"; do
      case "$a" in
        show|-p|--value|Environment|ActiveState|MainPID|NRestarts|ExecMainStatus|Result) ;;
        *) unit=$a ;;
      esac
    done
    case " $* " in
      *" NRestarts "*|*" MainPID "*)
        printf '%s\n' "$*" >>"$DIR/show_log"
        if [ "$unit" = "stalwart-mail" ] && [ -f "$DIR/stalwart_show_rc" ]; then
          rc=$(cat "$DIR/stalwart_show_rc")
          if [ "$rc" != 0 ]; then
            exit "$rc"
          fi
        fi
        result=success
        if [ "$unit" = "surmount-management-ui" ] && [ -f "$DIR/ui_current" ]; then
          read -r state pid restarts status result <"$DIR/ui_current"
        elif [ "$unit" = "stalwart-mail" ] && [ -f "$DIR/stalwart_current" ]; then
          read -r state pid restarts status result <"$DIR/stalwart_current"
        else
          if [ "$unit" = "stalwart-mail" ]; then
            state=$(cat "$DIR/state.stalwart" 2>/dev/null || echo inactive)
          elif [ "$unit" = "sshd" ]; then
            state=$(cat "$DIR/state.sshd" 2>/dev/null || echo inactive)
          else
            state=inactive
          fi
          pid=4242
          restarts=0
          status=0
          result=success
        fi
        if [ -z "$result" ]; then
          result=success
        fi
        printf 'ActiveState=%s\nMainPID=%s\nNRestarts=%s\n' "$state" "$pid" "$restarts"
        if [ "$status" = "-" ]; then
          printf 'ExecMainStatus=\n'
        else
          printf 'ExecMainStatus=%s\n' "$status"
        fi
        case " $* " in
          *" Result "*)
            if [ "$result" != "missing" ]; then
              printf 'Result=%s\n' "$result"
            fi
            ;;
        esac
        exit 0
        ;;
    esac
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
        // Short sample window for hermetic tests. A caller in `extra`
        // overrides these. Production stays on the 30000 ms default.
        c.env("SURMOUNT_SMOKE_UNIT_WAIT_MS", "400");
        c.env("SURMOUNT_SMOKE_UNIT_POLL_MS", "20");
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

/// The checker must not treat the first `activating` read as fail=1.
/// The fake stays `activating` on poll 1 and becomes `active` on poll 2.
#[test]
fn activating_is_not_an_immediate_failure_when_the_unit_becomes_active() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("state.stalwart"), "activating\n").unwrap();
    fs::write(f.bin.join("stalwart_flip_after"), "2\n").unwrap();
    fs::write(f.bin.join("stalwart_flip_to"), "active\n").unwrap();
    let out = f.run(&[
        ("SURMOUNT_SMOKE_UNIT_POLL_MS", "20"),
        ("SURMOUNT_SMOKE_UNIT_WAIT_MS", "5000"),
    ]);
    let s = combined(&out);
    assert!(
        out.status.success(),
        "activating must be polled until active, not fail=1 on the first read: {s}"
    );
    assert!(s.contains("PASS: unit stalwart-mail is active"), "{s}");
    assert!(!s.contains("FAIL: unit stalwart-mail is not active"), "{s}");
    let polls = fs::read_to_string(f.bin.join("stalwart_polls")).unwrap_or_default();
    assert!(
        polls.trim().parse::<u32>().unwrap_or(0) >= 2,
        "the checker must read is-active more than once while activating: {polls}"
    );
}

/// A unit that leaves `activating` for `failed` is still a failure.
#[test]
fn activating_that_becomes_failed_is_still_a_failure() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("state.stalwart"), "activating\n").unwrap();
    fs::write(f.bin.join("stalwart_flip_after"), "2\n").unwrap();
    fs::write(f.bin.join("stalwart_flip_to"), "failed\n").unwrap();
    let out = f.run(&[
        ("SURMOUNT_SMOKE_UNIT_POLL_MS", "20"),
        ("SURMOUNT_SMOKE_UNIT_WAIT_MS", "5000"),
    ]);
    let s = combined(&out);
    assert!(!out.status.success(), "{s}");
    assert!(
        s.contains("FAIL: unit stalwart-mail is not active (is-active=failed)"),
        "a unit that leaves activating for failed must stay a failure: {s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

fn crash_window() -> [(&'static str, &'static str); 2] {
    [
        ("SURMOUNT_SMOKE_UNIT_POLL_MS", "20"),
        ("SURMOUNT_SMOKE_UNIT_WAIT_MS", "400"),
    ]
}

fn write_stalwart_samples(f: &Fakes, body: &str) {
    fs::write(f.bin.join("stalwart_samples"), body).unwrap();
}

/// The first sample is active. The next sample still reports active, and
/// the main process exit status is 1. One active read must not pass.
#[test]
fn crash_loop_active_then_exit_1_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 100 0 1\n");
    let env = crash_window();
    let out = f.run(&env);
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "active then exit 1 must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (exit 1)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// The first sample is active. The next sample is failed.
#[test]
fn crash_loop_active_then_failed_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nfailed 0 0 1\n");
    let env = crash_window();
    let out = f.run(&env);
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "active then failed must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail is not active (is-active=failed)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// is-active stays active while NRestarts goes from 0 to 1.
#[test]
fn crash_loop_nrestarts_increase_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 100 1 0\n");
    let env = crash_window();
    let out = f.run(&env);
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "NRestarts increase during the sample window must fail smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (NRestarts increased)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// is-active stays active and NRestarts stays 0, but the main PID changes.
#[test]
fn crash_loop_main_pid_change_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 200 0 0\n");
    let env = crash_window();
    let out = f.run(&env);
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "a new main PID during the sample window must fail smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (main PID changed)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// Active, same main PID, same NRestarts, across more than one sample.
#[test]
fn crash_loop_stable_same_pid_and_nrestarts_passes() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 100 0 0\n");
    let env = crash_window();
    let out = f.run(&env);
    let s = combined(&out);
    assert!(
        out.status.success(),
        "active with the same main PID and the same NRestarts must pass: {s}"
    );
    assert!(s.contains("PASS: unit stalwart-mail is active"), "{s}");
    let polls = fs::read_to_string(f.bin.join("stalwart_polls")).unwrap_or_default();
    assert!(
        polls.trim().parse::<u32>().unwrap_or(0) >= 2,
        "a single active sample is not a pass: {polls}"
    );
    let show = fs::read_to_string(f.bin.join("show_log")).unwrap_or_default();
    let stalwart_shows = show
        .lines()
        .filter(|line| line.contains("stalwart-mail") && line.contains("NRestarts"))
        .count();
    assert!(
        stalwart_shows >= 2,
        "the checker must read NRestarts more than once: {show}"
    );
}

/// `failed` and `inactive` fail on the first read. They are not polled.
#[test]
fn crash_loop_failed_and_inactive_fail_immediately() {
    let env = crash_window();

    let failed = Fakes::new();
    failed.reset_day1();
    fs::write(failed.bin.join("state.stalwart"), "failed\n").unwrap();
    let out = failed.run(&env);
    let s = combined(&out);
    assert!(!out.status.success(), "{s}");
    assert!(
        s.contains("FAIL: unit stalwart-mail is not active (is-active=failed)"),
        "{s}"
    );
    let polls = fs::read_to_string(failed.bin.join("stalwart_polls")).unwrap_or_default();
    assert_eq!(
        polls.trim(),
        "1",
        "failed must not be polled through the sample window: {polls}"
    );

    let inactive = Fakes::new();
    inactive.reset_day1();
    fs::write(inactive.bin.join("state.stalwart"), "inactive\n").unwrap();
    let out = inactive.run(&env);
    let s = combined(&out);
    assert!(!out.status.success(), "{s}");
    assert!(
        s.contains("FAIL: unit stalwart-mail is not active (is-active=inactive)"),
        "{s}"
    );
    let polls = fs::read_to_string(inactive.bin.join("stalwart_polls")).unwrap_or_default();
    assert_eq!(
        polls.trim(),
        "1",
        "inactive must fail on the first read: {polls}"
    );
}

fn write_ui_samples(f: &Fakes, body: &str) {
    fs::write(f.bin.join("ui_samples"), body).unwrap();
}

/// ExecMainStatus 2 is a failure. Only status 1 used to fail.
#[test]
fn crash_loop_nonzero_exec_main_status_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 100 0 2\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "ExecMainStatus 2 must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (exit 2)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// NRestarts that is not a number must fail. It must not be ignored.
#[test]
fn crash_loop_unparsed_nrestarts_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 100 nope 0\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "unparsed NRestarts must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (NRestarts did not parse)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// ExecMainStatus that is not a number is an unparsed show, not a pass.
#[test]
fn crash_loop_unparsed_exec_main_status_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0\nactive 100 0 bogus\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "unparsed ExecMainStatus must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (systemctl show unparsed)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

fn stalwart_lines(text: &str) -> String {
    text.lines()
        .filter(|line| line.contains("stalwart-mail"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A later sample fails when systemd Result= is not success.
/// ExecMainStatus 0 does not excuse exit-code, timeout, signal,
/// core-dump, watchdog, or oom-kill. An empty ExecMainStatus does not
/// excuse timeout, signal, or oom-kill. A missing Result= field fails
/// the sample. Result=success with ExecMainStatus 0 does not fail by itself.
#[test]
fn systemd_result_other_than_success_fails_the_sample() {
    // result token, ExecMainStatus token, expected failure text.
    // Exec token "-" is an empty ExecMainStatus field.
    // Result token "missing" omits the Result= field.
    let bad = [
        ("exit-code", "0", "Result exit-code"),
        ("timeout", "0", "Result timeout"),
        ("timeout", "-", "Result timeout"),
        ("signal", "0", "Result signal"),
        ("signal", "-", "Result signal"),
        ("core-dump", "0", "Result core-dump"),
        ("watchdog", "0", "Result watchdog"),
        ("oom-kill", "0", "Result oom-kill"),
        ("oom-kill", "-", "Result oom-kill"),
        ("missing", "0", "Result missing"),
    ];
    let mut misses = Vec::new();
    for (result, exec_status, expect) in bad {
        let f = Fakes::new();
        f.reset_day1();
        let body = format!("active 100 0 0\nactive 100 0 {exec_status} {result}\n");
        write_stalwart_samples(&f, &body);
        let out = f.run(&crash_window());
        let s = combined(&out);
        let show = fs::read_to_string(f.bin.join("show_log")).unwrap_or_default();
        let mut why = Vec::new();
        if out.status.success() {
            why.push("smoke exited 0");
        }
        let fail_line = format!("FAIL: unit stalwart-mail did not stay active ({expect})");
        if !s.contains(&fail_line) {
            why.push("missing failure line");
        }
        if s.contains("PASS: unit stalwart-mail is active") {
            why.push("stalwart passed");
        }
        if !show.contains("Result") {
            why.push("show was not asked for Result");
        }
        if !why.is_empty() {
            misses.push(format!(
                "Result={result} ExecMainStatus token {exec_status} ({}):\n{}",
                why.join(", "),
                stalwart_lines(&s)
            ));
        }
    }

    let f = Fakes::new();
    f.reset_day1();
    write_stalwart_samples(&f, "active 100 0 0 success\nactive 100 0 0 success\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    let show = fs::read_to_string(f.bin.join("show_log")).unwrap_or_default();
    let mut why = Vec::new();
    if !out.status.success() {
        why.push("smoke exited non-zero");
    }
    if !s.contains("PASS: unit stalwart-mail is active") {
        why.push("missing pass line");
    }
    if s.contains("did not stay active (Result") {
        why.push("Result=success failed the sample");
    }
    if !show.contains("Result") {
        why.push("show was not asked for Result");
    }
    if !why.is_empty() {
        misses.push(format!(
            "Result=success ExecMainStatus 0 ({}):\n{}",
            why.join(", "),
            stalwart_lines(&s)
        ));
    }

    assert!(
        misses.is_empty(),
        "systemd Result= sample contract missed:\n{}",
        misses.join("\n---\n")
    );
}

/// A non-zero systemctl show must not count as a stable active unit.
#[test]
fn systemctl_show_failure_is_not_stable() {
    let f = Fakes::new();
    f.reset_day1();
    fs::write(f.bin.join("stalwart_show_rc"), "1\n").unwrap();
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "failed systemctl show must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit stalwart-mail did not stay active (systemctl show failed)"),
        "{s}"
    );
    assert!(!s.contains("PASS: unit stalwart-mail is active"), "{s}");
}

/// The UI unit uses the same window. One active sample, then exit 1, fails.
#[test]
fn ui_active_once_then_exit_1_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_ui_samples(&f, "active 100 0 0\nactive 100 0 1\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "UI active then exit 1 must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit surmount-management-ui did not stay active (exit 1)"),
        "{s}"
    );
    assert!(
        !s.contains("PASS: unit surmount-management-ui is active"),
        "{s}"
    );
}

/// The UI unit fails when NRestarts increases during the window.
#[test]
fn ui_nrestarts_increase_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_ui_samples(&f, "active 100 0 0\nactive 100 1 0\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "UI NRestarts increase must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit surmount-management-ui did not stay active (NRestarts increased)"),
        "{s}"
    );
    assert!(
        !s.contains("PASS: unit surmount-management-ui is active"),
        "{s}"
    );
}

/// The UI unit fails when the main PID changes during the window.
#[test]
fn ui_main_pid_change_fails_smoke() {
    let f = Fakes::new();
    f.reset_day1();
    write_ui_samples(&f, "active 100 0 0\nactive 200 0 0\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        !out.status.success(),
        "UI main PID change must fail post-switch smoke: {s}"
    );
    assert!(
        s.contains("FAIL: unit surmount-management-ui did not stay active (main PID changed)"),
        "{s}"
    );
    assert!(
        !s.contains("PASS: unit surmount-management-ui is active"),
        "{s}"
    );
}

/// Active UI with the same PID and NRestarts must be sampled more than once.
#[test]
fn ui_stable_window_reads_more_than_once() {
    let f = Fakes::new();
    f.reset_day1();
    write_ui_samples(&f, "active 100 0 0\nactive 100 0 0\n");
    let out = f.run(&crash_window());
    let s = combined(&out);
    assert!(
        out.status.success(),
        "stable UI must pass the sample window: {s}"
    );
    assert!(
        s.contains("PASS: unit surmount-management-ui is active"),
        "{s}"
    );
    let polls = fs::read_to_string(f.bin.join("ui_polls")).unwrap_or_default();
    assert!(
        polls.trim().parse::<u32>().unwrap_or(0) >= 2,
        "a single active UI sample is not a pass: {polls}"
    );
    let show = fs::read_to_string(f.bin.join("show_log")).unwrap_or_default();
    let ui_shows = show
        .lines()
        .filter(|line| line.contains("surmount-management-ui") && line.contains("NRestarts"))
        .count();
    assert!(
        ui_shows >= 2,
        "the UI checker must read NRestarts more than once: {show}"
    );
}
