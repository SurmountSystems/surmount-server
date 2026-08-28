//! Hermetic PATH-fixture tests ported from script/test-domain-audit.sh.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-domain-audit")
}

fn scratch(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-domain-audit-{}-{}-{}",
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

struct Harness {
    bin_dir: PathBuf,
    fixture: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let root = scratch("h");
        let bin_dir = root.join("bin");
        let fixture = root.join("rr");
        fs::create_dir_all(&bin_dir).unwrap();
        fs::create_dir_all(&fixture).unwrap();
        let fixture_q = fixture.display().to_string();
        let dig = bin_dir.join("dig");
        fs::write(
            &dig,
            format!(
                r#"#!/bin/sh
set -eu
FIXTURE={fixture_q:?}
name=""
rrtype="A"
ptr=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    +*) shift ;;
    @*) shift ;;
    -x) ptr="$2"; shift 2 ;;
    *)
      if [ -z "$name" ]; then name="$1"; else rrtype="$1"; fi
      shift
      ;;
  esac
done
if [ -n "$ptr" ]; then
  f="$FIXTURE/$ptr/PTR"
  [ -f "$f" ] && cat "$f"
  exit 0
fi
[ -n "$rrtype" ] || rrtype="A"
f="$FIXTURE/$name/$rrtype"
[ -f "$f" ] && cat "$f"
exit 0
"#
            ),
        )
        .unwrap();
        fs::set_permissions(&dig, fs::Permissions::from_mode(0o755)).unwrap();
        let curl = bin_dir.join("curl");
        fs::write(
            &curl,
            r#"#!/bin/sh
set -eu
url=""
have_write_out=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --write-out) have_write_out=1; shift 2 ;;
    --output|--max-redirs|--connect-timeout|--max-time|--user-agent) shift 2 ;;
    --silent|--show-error|--location|--fail) shift ;;
    http://*|https://*) url="$1"; shift ;;
    *) shift ;;
  esac
done
case "$url" in
  *mta-sts.*) exit 22 ;;
esac
if [ "$have_write_out" = 1 ]; then
  printf '%s' "code=200 remote=203.0.113.10 connect=0.001s tls=0.002s total=0.003s redirects=0 final=${url}"
fi
exit 0
"#,
        )
        .unwrap();
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
        let openssl = bin_dir.join("openssl");
        fs::write(
            &openssl,
            r#"#!/bin/sh
set -eu
if [ "${1:-}" = "s_client" ]; then
  printf '%s\n' "CONNECTED(00000003)"
  exit 0
fi
if [ "${1:-}" = "x509" ]; then
  case " $* " in
    *" -subject "*) printf '%s\n' "subject=CN=hermetic.test" ;;
  esac
  case " $* " in
    *" -issuer "*) printf '%s\n' "issuer=CN=hermetic-issuer" ;;
  esac
  case " $* " in
    *" -dates "*) printf '%s\n' "notBefore=Jan 1 00:00:00 2026 GMT" "notAfter=Jan 1 00:00:00 2027 GMT" ;;
  esac
  exit 0
fi
exit 0
"#,
        )
        .unwrap();
        fs::set_permissions(&openssl, fs::Permissions::from_mode(0o755)).unwrap();
        Self { bin_dir, fixture }
    }

    fn write_rr(&self, qname: &str, rrtype: &str, lines: &[&str]) {
        let dir = self.fixture.join(qname);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(rrtype), format!("{}\n", lines.join("\n"))).unwrap();
    }

    fn seed_apex_web(&self, apex: &str) {
        self.write_rr(
            apex,
            "NS",
            &["dns1.registrar-servers.com.", "dns2.registrar-servers.com."],
        );
        self.write_rr(
            apex,
            "SOA",
            &["dns1.registrar-servers.com. hostmaster.example.test. 1 3600 600 1209600 3600"],
        );
        self.write_rr(apex, "A", &["203.0.113.10"]);
        self.write_rr(apex, "AAAA", &["2001:db8::10"]);
        self.write_rr(&format!("www.{apex}"), "A", &["203.0.113.10"]);
        self.write_rr(&format!("www.{apex}"), "AAAA", &["2001:db8::10"]);
    }

    fn reset(&self) {
        let _ = fs::remove_dir_all(&self.fixture);
        fs::create_dir_all(&self.fixture).unwrap();
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        let path = format!(
            "{}:{}",
            self.bin_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = Command::new(bin())
            .args(args)
            .env("PATH", path)
            .output()
            .expect("run audit");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.code().unwrap_or(1), combined)
    }
}

#[test]
fn help_version_and_usage() {
    let h = Harness::new();
    let (rc, out) = h.run(&["--help"]);
    assert_eq!(rc, 0);
    assert!(out.contains("Usage:"));
    assert!(out.contains("stalwart"));
    assert!(out.to_ascii_lowercase().contains("read-only"));
    assert!(out.contains("--mail-domain"));
    assert!(out.contains("--static-site"));
    assert!(out.contains("cryptoquick.com"));
    assert!(out.contains("MX flip"));
    let (rc, out) = h.run(&["--version"]);
    assert_eq!(rc, 0);
    assert!(out.contains("surmount-domain-audit"));
    let (rc, out) = h.run(&[]);
    assert_eq!(rc, 64);
    assert!(out.to_ascii_lowercase().contains("domain is required") || out.contains("Usage:"));
    let (rc, _) = h.run(&["--not-a-real-flag", "example.com"]);
    assert_eq!(rc, 64);
}

#[test]
fn leftover_parent_dnssec_without_dnskey_is_fail() {
    let h = Harness::new();
    h.seed_apex_web("cryptoquick.com");
    h.write_rr(
        "cryptoquick.com",
        "DS",
        &["370 13 2 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    );
    h.write_rr(
        "cryptoquick.com",
        "MX",
        &["10 eforward1.registrar-servers.com."],
    );
    h.write_rr("eforward1.registrar-servers.com", "A", &["203.0.113.53"]);
    let (rc, out) = h.run(&["--no-color", "--timeout", "2", "cryptoquick.com"]);
    assert!(out.contains("[FAIL]") && out.contains("DS record exists but no DNSKEY"));
    assert_eq!(rc, 2);
}

#[test]
fn sha1_digest_type_1_fails_with_and_without_dnskey() {
    let h = Harness::new();
    h.seed_apex_web("sha1-noden.test");
    h.write_rr(
        "sha1-noden.test",
        "DS",
        &["2368 13 1 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    );
    let (rc, out) = h.run(&[
        "--no-color",
        "--timeout",
        "2",
        "--static-site",
        "sha1-noden.test",
    ]);
    assert!(out.contains("[FAIL]") && (out.contains("digest type 1") || out.contains("SHA-1")));
    assert_eq!(rc, 2);

    h.reset();
    h.seed_apex_web("sha1-withkey.test");
    h.write_rr(
        "sha1-withkey.test",
        "DS",
        &["370 13 1 bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"],
    );
    h.write_rr(
        "sha1-withkey.test",
        "DNSKEY",
        &["257 3 13 cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"],
    );
    let (rc, out) = h.run(&[
        "--no-color",
        "--timeout",
        "2",
        "--static-site",
        "sha1-withkey.test",
    ]);
    assert!(out.contains("[FAIL]") && (out.contains("digest type 1") || out.contains("SHA-1")));
    assert!(!out.contains("[PASS] DNSSEC delegation and DNSKEY"));
    assert_eq!(rc, 2);
}

#[test]
fn type2_ds_plus_dnskey_pass_unsigned_info() {
    let h = Harness::new();
    h.seed_apex_web("dnssec-ok.test");
    h.write_rr(
        "dnssec-ok.test",
        "DS",
        &["370 13 2 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    );
    h.write_rr(
        "dnssec-ok.test",
        "DNSKEY",
        &["257 3 13 cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"],
    );
    let (rc, out) = h.run(&[
        "--no-color",
        "--timeout",
        "2",
        "--static-site",
        "dnssec-ok.test",
    ]);
    assert!(out.contains("[PASS] DNSSEC delegation and DNSKEY"));
    assert!(!out.contains("[FAIL]") || !out.contains("digest type 1"));
    assert!(!out.contains("[INFO] DNSSEC is not enabled"));
    let _ = rc;

    h.reset();
    h.seed_apex_web("unsigned.test");
    let (_rc, out) = h.run(&[
        "--no-color",
        "--timeout",
        "2",
        "--static-site",
        "unsigned.test",
    ]);
    assert!(out.contains("[INFO] DNSSEC is not enabled"));
    assert!(!out.contains("[FAIL]") || !out.contains("digest type"));
}

fn mailbox_complete(h: &Harness, apex: &str, mx: &str) {
    h.seed_apex_web(apex);
    h.write_rr(apex, "MX", &[mx]);
    if mx.contains("eforward") {
        h.write_rr("eforward1.registrar-servers.com", "A", &["203.0.113.53"]);
    } else {
        let host = mx
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .trim_end_matches('.');
        h.write_rr(host, "A", &["203.0.113.11"]);
    }
    h.write_rr(apex, "TXT", &[r#""v=spf1 a:mail.example.test -all""#]);
    h.write_rr(
        apex,
        "CAA",
        &[
            r#"0 issue "letsencrypt.org""#,
            r#"0 issuewild "letsencrypt.org""#,
        ],
    );
    h.write_rr(
        &format!("_dmarc.{apex}"),
        "TXT",
        &[r#""v=DMARC1; p=none; rua=mailto:admin@example.test""#],
    );
    h.write_rr(
        &format!("stalwart._domainkey.{apex}"),
        "TXT",
        &[r#""v=DKIM1; k=ed25519; p=hermeticed25519key""#],
    );
    h.write_rr(
        &format!("stalwart-rsa._domainkey.{apex}"),
        "TXT",
        &[r#""v=DKIM1; k=rsa; p=hermeticrsakey""#],
    );
    h.write_rr(
        &format!("_smtp._tls.{apex}"),
        "TXT",
        &[r#""v=TLSRPTv1; rua=mailto:admin@example.test""#],
    );
}

#[test]
fn extra_mailbox_missing_and_eforward_and_custom_mx() {
    let h = Harness::new();
    h.seed_apex_web("cryptoquick.com");
    h.write_rr(
        "cryptoquick.com",
        "MX",
        &["10 eforward1.registrar-servers.com."],
    );
    h.write_rr("eforward1.registrar-servers.com", "A", &["203.0.113.53"]);
    let (rc, out) = h.run(&["--no-color", "--timeout", "2", "cryptoquick.com"]);
    assert!(out.contains("[FAIL]") && out.contains("SPF"));
    assert!(out.contains("selector 'stalwart'"));
    assert!(out.contains("selector 'stalwart-rsa'"));
    assert!(out.contains("DMARC"));
    assert!(out.contains("TLS-RPT") || out.contains("TLS reporting"));
    assert!(out.contains("CAA"));
    assert!(
        out.contains("eforward") || out.contains("Email Forwarding") || out.contains("public MX")
    );
    assert!(!out.contains("[FAIL]") || !out.contains("MTA-STS") || out.contains("not required"));
    assert_eq!(rc, 2);

    h.reset();
    mailbox_complete(
        &h,
        "baxterartworks.com",
        "10 eforward1.registrar-servers.com.",
    );
    let (rc, out) = h.run(&["--no-color", "--timeout", "2", "baxterartworks.com"]);
    assert!(
        out.contains("eforward") || out.contains("Email Forwarding") || out.contains("public MX")
    );
    assert!(out.contains("[WARN]") && out.contains("p=none"));
    assert!(
        out.to_ascii_lowercase().contains("mta-sts is not required")
            || out
                .to_ascii_lowercase()
                .contains("mta-sts is not configured")
    );
    assert_eq!(rc, 2);

    h.reset();
    mailbox_complete(&h, "baxterartworks.com", "10 mail.baxterartworks.com.");
    let (rc, out) = h.run(&["--no-color", "--timeout", "2", "baxterartworks.com"]);
    assert!(
        !out.contains("[FAIL]"),
        "complete extra mailbox + custom MX must not FAIL: {out}"
    );
    assert!(out.contains("[WARN]") && out.contains("p=none"));
    assert_eq!(rc, 1);
}

#[test]
fn emailtype_fwd_static_site_and_flags() {
    let h = Harness::new();
    mailbox_complete(&h, "baxterartworks.com", "10 mail.baxterartworks.com.");
    let path = format!(
        "{}:{}",
        h.bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(bin())
        .args(["--no-color", "--timeout", "2", "baxterartworks.com"])
        .env("PATH", &path)
        .env("SURMOUNT_DOMAIN_AUDIT_EMAIL_TYPE", "FWD")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("EmailType") || text.contains("FWD") || text.contains("Email Forwarding")
    );
    assert_eq!(out.status.code().unwrap_or(1), 2);

    h.reset();
    h.seed_apex_web("btcfur.com");
    h.write_rr("btcfur.com", "MX", &["10 eforward1.registrar-servers.com."]);
    h.write_rr("eforward1.registrar-servers.com", "A", &["203.0.113.53"]);
    let (_rc, out) = h.run(&["--no-color", "--timeout", "2", "btcfur.com"]);
    assert!(!out.contains("[FAIL]") || !(out.contains("eforward") && out.contains("[FAIL]")));
    let fail_mail = out.contains("[FAIL]")
        && (out.contains("SPF")
            || out.contains("DKIM")
            || out.contains("DMARC")
            || out.contains("TLS-RPT")
            || out.contains("CAA"));
    assert!(!fail_mail);
    assert!(
        !(out.to_ascii_lowercase().contains("mailbox domain")
            && out.contains("Mail records: required"))
    );

    h.reset();
    h.seed_apex_web("mail.example.test");
    h.write_rr(
        "mail.example.test",
        "MX",
        &["10 eforward1.registrar-servers.com."],
    );
    h.write_rr("eforward1.registrar-servers.com", "A", &["203.0.113.53"]);
    let (rc, out) = h.run(&[
        "--no-color",
        "--timeout",
        "2",
        "--mail-domain",
        "mail.example.test",
    ]);
    assert!(out.contains("selector 'stalwart'"));
    assert!(out.contains("selector 'stalwart-rsa'"));
    assert_eq!(rc, 2);

    h.reset();
    h.seed_apex_web("cryptoquick.com");
    h.write_rr(
        "cryptoquick.com",
        "DS",
        &["370 13 2 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    );
    let (_rc, out) = h.run(&[
        "--no-color",
        "--timeout",
        "2",
        "--static-site",
        "cryptoquick.com",
    ]);
    assert!(!out.contains("selector 'stalwart'") || !out.contains("[FAIL]"));
    assert!(out.contains("DS record exists but no DNSKEY"));

    let (rc, _) = h.run(&[
        "--no-color",
        "--mail-domain",
        "--static-site",
        "example.test",
    ]);
    assert_eq!(rc, 64);
}

#[test]
fn check_tls_usage() {
    let h = Harness::new();
    let (rc, _) = h.run(&["check-tls"]);
    assert_eq!(rc, 2);
}
