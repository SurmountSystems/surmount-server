//! Hermetic CLI tests ported from script/test-dns-zone-namecheap.sh.
//! Mock zone via SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR. Live path uses a
//! local HTTP listener (API base override). Never live Namecheap.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-dns-zone")
}

fn scratch(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-dns-zone-{}-{}-{}",
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
    mock: PathBuf,
    cred: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let root = scratch("h");
        let mock = root.join("mock");
        fs::create_dir_all(&mock).unwrap();
        let cred = root.join("namecheap.env");
        fs::write(
            &cred,
            "# synthetic hermetic fixture; not a real Namecheap account\n\
             ApiUser=surmount-test-apiuser\n\
             ApiKey=surmount-test-apikey-not-real-zone-001\n\
             UserName=surmount-test-apiuser\n\
             ClientIp=203.0.113.50\n\
             SLD=example\n\
             TLD=test\n",
        )
        .unwrap();
        fs::set_permissions(&cred, fs::Permissions::from_mode(0o600)).unwrap();
        let hosts = mock.join("hosts.txt");
        fs::write(
            &hosts,
            "@|A|203.0.113.10|10|1800\n\
             www|CNAME|example.test.|10|1800\n\
             mail|A|203.0.113.11|10|1800\n\
             _acme-challenge.services|TXT|synthetic-challenge-txt|10|60\n",
        )
        .unwrap();
        fs::set_permissions(&hosts, fs::Permissions::from_mode(0o600)).unwrap();
        Self { mock, cred }
    }

    fn seed(&self) {
        fs::write(
            self.mock.join("hosts.txt"),
            "@|A|203.0.113.10|10|1800\n\
             www|CNAME|example.test.|10|1800\n\
             mail|A|203.0.113.11|10|1800\n\
             _acme-challenge.services|TXT|synthetic-challenge-txt|10|60\n",
        )
        .unwrap();
    }

    fn hosts(&self) -> String {
        fs::read_to_string(self.mock.join("hosts.txt")).unwrap_or_default()
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(bin());
        c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV", &self.cred);
        c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR", &self.mock);
        c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_VERBOSE", "0");
        c.env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_ENV");
        c
    }

    fn run(&self, args: &[&str]) -> (i32, String) {
        let out = self.cmd().args(args).output().expect("run");
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.code().unwrap_or(1), combined)
    }
}

#[test]
fn no_args_exits_nonzero() {
    let h = Harness::new();
    let (rc, _) = h.run(&[]);
    assert_ne!(rc, 0);
}

#[test]
fn list_shows_seed_and_email_type() {
    let h = Harness::new();
    fs::write(h.mock.join("email_type.txt"), "FWD\n").unwrap();
    let (rc, out) = h.run(&["list"]);
    assert_eq!(rc, 0);
    assert!(out.contains("mail"));
    assert!(out.contains("203.0.113.11"));
    assert!(out.contains("_acme-challenge.services"));
    assert!(out.to_ascii_lowercase().contains("emailtype") && out.contains("FWD"));
    fs::write(h.mock.join("email_type.txt"), "MX\n").unwrap();
    let (rc, out) = h.run(&["list"]);
    assert_eq!(rc, 0);
    assert!(out.contains("MX"));
    assert!(!out.contains("EmailType: FWD"));
}

#[test]
fn dry_run_set_a_does_not_write() {
    let h = Harness::new();
    let (rc, out) = h.run(&["set-a", "services", "203.0.113.20"]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    assert!(!h.hosts().contains("services|A|203.0.113.20|"));
    assert!(h.hosts().contains("_acme-challenge.services|TXT|"));
}

#[test]
fn live_set_a_merges_and_preserves() {
    let h = Harness::new();
    let (rc, _) = h.run(&[
        "--live",
        "set-a",
        "services",
        "203.0.113.20",
        "--ttl",
        "600",
    ]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert!(hosts.contains("services|A|203.0.113.20|10|600"));
    assert!(hosts.contains("@|A|203.0.113.10|"));
    assert!(hosts.contains("www|CNAME|example.test.|"));
    assert!(hosts.contains("mail|A|203.0.113.11|"));
    assert!(hosts.contains("_acme-challenge.services|TXT|synthetic-challenge-txt|"));
    let (rc, _) = h.run(&["--live", "set-a", "services", "203.0.113.21"]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert_eq!(hosts.matches("services|A|").count(), 1);
    assert!(hosts.contains("services|A|203.0.113.21|"));
}

#[test]
fn live_set_aaaa_keeps_a() {
    let h = Harness::new();
    h.run(&["--live", "set-a", "services", "203.0.113.21"]);
    let (rc, _) = h.run(&["--live", "set-aaaa", "services", "2001:db8::20"]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert!(hosts.contains("services|AAAA|2001:db8::20|"));
    assert!(hosts.contains("services|A|203.0.113.21|"));
}

#[test]
fn set_host_a_and_aaaa() {
    let h = Harness::new();
    let (rc, out) = h.run(&[
        "set-host",
        "mail",
        "--a",
        "203.0.113.30",
        "--aaaa",
        "2001:db8::30",
    ]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    assert!(h.hosts().contains("mail|A|203.0.113.11|"));
    let (rc, _) = h.run(&[
        "--live",
        "set-host",
        "mail",
        "--a",
        "203.0.113.30",
        "--aaaa",
        "2001:db8::30",
        "--ttl",
        "900",
    ]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert!(hosts.contains("mail|A|203.0.113.30|10|900"));
    assert!(hosts.contains("mail|AAAA|2001:db8::30|10|900"));
    assert!(hosts.contains("_acme-challenge.services|TXT|"));
}

#[test]
fn fqdn_and_foreign() {
    let h = Harness::new();
    let (rc, _) = h.run(&["--live", "set-a", "services.example.test", "203.0.113.40"]);
    assert_eq!(rc, 0);
    assert!(h.hosts().contains("services|A|203.0.113.40|"));
    let (rc, out) = h.run(&["--live", "set-a", "other.invalid", "203.0.113.1"]);
    assert_ne!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("not under"));
}

#[test]
fn set_txt_spf_dmarc_dkim() {
    let h = Harness::new();
    let mut hosts = h.hosts();
    hosts.push_str(
        "@|TXT|unrelated-txt-keep-me|10|1800\n@|TXT|v=spf1 include:old.example ~all|10|1800\n",
    );
    fs::write(h.mock.join("hosts.txt"), hosts).unwrap();
    let (rc, out) = h.run(&["set-txt", "@", "v=spf1 a:mail.example.test -all"]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    assert!(out.to_ascii_lowercase().contains("merge=spf"));
    assert!(h.hosts().contains("v=spf1 include:old.example"));
    let (rc, _) = h.run(&[
        "--live",
        "set-txt",
        "@",
        "v=spf1 a:mail.example.test -all",
        "--ttl",
        "600",
    ]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert!(hosts.contains("@|TXT|v=spf1 a:mail.example.test -all|10|600"));
    assert!(hosts.contains("unrelated-txt-keep-me"));
    assert!(!hosts.contains("include:old.example"));
    h.seed();
    let (rc, out) = h.run(&[
        "--live",
        "set-txt",
        "_dmarc",
        "v=DMARC1; p=none; rua=mailto:dmarc@example.test",
    ]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("merge=dmarc"));
    let (rc, _) = h.run(&[
        "--live",
        "set-txt",
        "stalwart._domainkey",
        "v=DKIM1; k=ed25519; p=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
    ]);
    assert_eq!(rc, 0);
    assert!(h.hosts().contains("stalwart._domainkey|TXT|"));
    let (rc, _) = h.run(&[
        "--live",
        "set-txt",
        "_dmarc.example.test",
        "v=DMARC1; p=quarantine",
    ]);
    assert_eq!(rc, 0);
    assert_eq!(h.hosts().matches("_dmarc|TXT|").count(), 1);
}

#[test]
fn empty_txt_and_bad_ipv4_and_creds() {
    let h = Harness::new();
    let (rc, _) = h.run(&["--live", "set-txt", "@", ""]);
    assert_ne!(rc, 0);
    let (rc, _) = h.run(&["--live", "set-a", "services", "999.0.0.1"]);
    assert_ne!(rc, 0);
    let missing = h.cred.parent().unwrap().join("missing.env");
    let mut c = h.cmd();
    c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV", &missing);
    let out = c.arg("list").output().unwrap();
    assert_ne!(out.status.code().unwrap_or(1), 0);
    let bad = h.cred.parent().unwrap().join("bad.env");
    fs::write(
        &bad,
        "ApiUser=surmount-test-apiuser\nSLD=example\nTLD=test\n",
    )
    .unwrap();
    fs::set_permissions(&bad, fs::Permissions::from_mode(0o600)).unwrap();
    let mut c = h.cmd();
    c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV", &bad);
    let out = c.arg("list").output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_ne!(out.status.code().unwrap_or(1), 0);
    assert!(text.contains("ApiKey"));
}

#[test]
fn verbose_does_not_print_apikey() {
    let h = Harness::new();
    let mut c = h.cmd();
    c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_VERBOSE", "1");
    let out = c
        .args(["--live", "set-a", "services", "203.0.113.50"])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code().unwrap_or(1), 0);
    assert!(!text.contains("surmount-test-apikey-not-real-zone-001"));
}

#[test]
fn set_caa_and_set_mx_and_fwd() {
    let h = Harness::new();
    let mut hosts = h.hosts();
    hosts.push_str("@|CAA|0 issue \"oldca.example\"|10|1800\n@|CAA|0 iodef \"mailto:caa@example.test\"|10|1800\n");
    fs::write(h.mock.join("hosts.txt"), hosts).unwrap();
    let (rc, out) = h.run(&["set-caa", "@", "0", "issue", "letsencrypt.org"]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    assert!(out.contains("0 issue \"letsencrypt.org\""));
    let (rc, _) = h.run(&[
        "--live",
        "set-caa",
        "@",
        "0",
        "issue",
        "letsencrypt.org",
        "--ttl",
        "600",
    ]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert!(hosts.contains("@|CAA|0 issue \"letsencrypt.org\"|10|600"));
    assert!(hosts.contains("0 iodef \"mailto:caa@example.test\""));
    assert!(!hosts.contains("oldca.example"));
    assert!(!hosts.contains("@|TXT|0 issue"));
    let (rc, _) = h.run(&[
        "--live",
        "set-caa",
        "@",
        "0",
        "issuewild",
        "letsencrypt.org",
    ]);
    assert_eq!(rc, 0);
    let (rc, _) = h.run(&["--live", "set-caa", "@", "999", "issue", "letsencrypt.org"]);
    assert_ne!(rc, 0);

    h.seed();
    fs::write(h.mock.join("email_type.txt"), "MX\n").unwrap();
    let mut hosts = h.hosts();
    hosts.push_str(
        "@|MX|eforward1.example.test.|10|1800\n@|MX|eforward2.example.test.|20|1800\nother|MX|other-mx.example.test.|10|1800\n",
    );
    fs::write(h.mock.join("hosts.txt"), hosts).unwrap();
    let (rc, out) = h.run(&["set-mx", "@", "mail.example.test.", "--pref", "10"]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    let (rc, _) = h.run(&[
        "--live",
        "set-mx",
        "@",
        "mail.example.test.",
        "--pref",
        "10",
        "--ttl",
        "900",
    ]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert_eq!(hosts.matches("@|MX|").count(), 1);
    assert!(hosts.contains("@|MX|mail.example.test.|10|900"));
    assert!(hosts.contains("other|MX|other-mx.example.test.|"));
    let (rc, _) = h.run(&["--live", "set-mx", "@", "bad exchange"]);
    assert_ne!(rc, 0);

    h.seed();
    let mut hosts = h.hosts();
    hosts.push_str("@|MX|eforward1.example.test.|10|1800\n");
    fs::write(h.mock.join("hosts.txt"), &hosts).unwrap();
    fs::write(h.mock.join("email_type.txt"), "FWD\n").unwrap();
    let before = h.hosts();
    let (rc, out) = h.run(&[
        "--live",
        "set-mx",
        "@",
        "mail.example.test.",
        "--pref",
        "10",
    ]);
    assert_ne!(rc, 0);
    assert!(out.contains("Email Forwarding is still on"));
    assert!(out.contains("change Mail Settings to Custom MX"));
    assert!(out.contains("Domain List"));
    assert!(out.contains("Advanced DNS"));
    assert!(out.contains("Mail Settings"));
    assert_eq!(h.hosts(), before);
}

#[test]
fn help_and_mode_and_credentials_flag_and_delete() {
    let h = Harness::new();
    let (rc, out) = h.run(&["--help"]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    assert!(out.contains("set-caa"));
    assert!(out.contains("set-mx"));
    assert!(out.contains("delete-host"));

    fs::set_permissions(&h.cred, fs::Permissions::from_mode(0o644)).unwrap();
    let (rc, out) = h.run(&["list"]);
    fs::set_permissions(&h.cred, fs::Permissions::from_mode(0o600)).unwrap();
    assert_ne!(rc, 0);
    assert!(
        out.to_ascii_lowercase().contains("owner-only")
            || out.contains("mode 0600")
            || out.contains("group/world")
    );
    assert!(!out.contains("surmount-test-apikey-not-real-zone-001"));
    let (rc, _) = h.run(&["list"]);
    assert_eq!(rc, 0);

    let root = h.cred.parent().unwrap();
    let other_cred = root.join("other.env");
    let other_mock = root.join("mock-other");
    fs::create_dir_all(&other_mock).unwrap();
    fs::write(
        &other_cred,
        "ApiUser=surmount-test-apiuser\n\
         ApiKey=surmount-test-apikey-not-real-zone-002\n\
         UserName=surmount-test-apiuser\n\
         ClientIp=203.0.113.50\n\
         SLD=extra\n\
         TLD=test\n",
    )
    .unwrap();
    fs::set_permissions(&other_cred, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(other_mock.join("hosts.txt"), "@|A|203.0.113.10|10|1800\n").unwrap();
    let mut c = Command::new(bin());
    c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR", &other_mock);
    c.env_remove("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV");
    let out = c
        .args([
            "--credentials",
            other_cred.to_str().unwrap(),
            "--live",
            "set-host",
            "@",
            "--a",
            "203.0.113.20",
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code().unwrap_or(1), 0);
    assert!(text.contains("extra.test"));

    fs::write(
        h.mock.join("hosts.txt"),
        "@|URL|http://parkingpage.namecheap.com|10|1800\n\
         @|A|203.0.113.10|10|1800\n\
         www|CNAME|parkingpage.namecheap.com|10|1800\n\
         www|A|203.0.113.10|10|1800\n\
         mail|A|203.0.113.11|10|1800\n",
    )
    .unwrap();
    let (rc, out) = h.run(&["delete-host", "@", "--type", "URL"]);
    assert_eq!(rc, 0);
    assert!(out.to_ascii_lowercase().contains("dry-run"));
    assert!(h.hosts().contains("@|URL|"));
    let (rc, _) = h.run(&["--live", "delete-host", "@", "--type", "URL"]);
    assert_eq!(rc, 0);
    assert!(!h.hosts().contains("@|URL|"));
    assert!(h.hosts().contains("@|A|203.0.113.10|"));
    let (rc, _) = h.run(&["--live", "delete-host", "www", "--type", "CNAME"]);
    assert_eq!(rc, 0);
    assert!(!h.hosts().contains("www|CNAME|"));
    assert!(h.hosts().contains("www|A|203.0.113.10|"));

    fs::write(h.mock.join("hosts.txt"), "").unwrap();
    let (rc, out) = h.run(&["--live", "delete-host", "@", "--type", "URL"]);
    assert_ne!(rc, 0);
    assert!(
        out.to_ascii_lowercase().contains("empty")
            || out.to_ascii_lowercase().contains("no records")
            || out.to_ascii_lowercase().contains("refuse")
    );

    fs::write(
        h.mock.join("hosts.txt"),
        "@|URL|http://parkingpage.namecheap.com|10|1800\n",
    )
    .unwrap();
    let (rc, out) = h.run(&["--live", "delete-host", "@", "--type", "URL"]);
    assert_ne!(rc, 0);
    assert!(
        out.to_ascii_lowercase().contains("empty zone")
            || out.to_ascii_lowercase().contains("refuse empty")
    );
    assert!(
        h.hosts()
            .contains("@|URL|http://parkingpage.namecheap.com|")
    );
}

#[test]
fn sha1_digest_type_1_live_fails_closed() {
    let h = Harness::new();
    fs::write(
        h.mock.join("hosts.txt"),
        "@|A|203.0.113.10|10|1800\n\
         @|DS|2368 13 1 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|10|1800\n",
    )
    .unwrap();
    let before = h.hosts();
    let (rc, out) = h.run(&["--live", "set-a", "www", "203.0.113.20"]);
    assert_ne!(rc, 0);
    assert!(out.contains("digest type 1") || out.contains("SHA-1") || out.contains("SHA-1"));
    assert_eq!(h.hosts(), before);
}

#[test]
fn digest_type_2_ds_may_merge() {
    let h = Harness::new();
    fs::write(
        h.mock.join("hosts.txt"),
        "@|A|203.0.113.10|10|1800\n\
         @|DS|370 13 2 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|10|1800\n",
    )
    .unwrap();
    let (rc, _) = h.run(&["--live", "set-a", "www", "203.0.113.20"]);
    assert_eq!(rc, 0);
    let hosts = h.hosts();
    assert!(hosts.contains("www|A|203.0.113.20|"));
    assert!(hosts.contains("@|DS|370 13 2 "));
}

struct FakeNc {
    records: Vec<(String, String, String, String, String)>,
    email_type: String,
    commands: Vec<String>,
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn url_decode(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_query(q: &str) -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    for part in q.split('&') {
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, '=');
        let k = url_decode(kv.next().unwrap_or(""));
        let v = url_decode(kv.next().unwrap_or(""));
        m.insert(k, v);
    }
    m
}

fn spawn_namecheap_http(state: std::sync::Arc<std::sync::Mutex<FakeNc>>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 8192];
            let n = match std::io::Read::read(&mut stream, &mut buf) {
                Ok(n) => n,
                Err(_) => continue,
            };
            let req = String::from_utf8_lossy(&buf[..n]);
            let first = req.lines().next().unwrap_or("");
            let path = first.split_whitespace().nth(1).unwrap_or("/");
            let q = path.split_once('?').map(|x| x.1).unwrap_or("");
            let params = parse_query(q);
            let body = {
                let mut g = state.lock().unwrap();
                let cmd = params.get("Command").cloned().unwrap_or_default();
                if cmd.contains("setHosts") {
                    g.commands.push("setHosts".into());
                    let mut recs = Vec::new();
                    for i in 1..64 {
                        let name = params.get(&format!("HostName{i}")).cloned();
                        let Some(name) = name else { break };
                        recs.push((
                            name,
                            params
                                .get(&format!("RecordType{i}"))
                                .cloned()
                                .unwrap_or_default(),
                            params
                                .get(&format!("Address{i}"))
                                .cloned()
                                .unwrap_or_default(),
                            params
                                .get(&format!("MXPref{i}"))
                                .cloned()
                                .unwrap_or_else(|| "10".into()),
                            params
                                .get(&format!("TTL{i}"))
                                .cloned()
                                .unwrap_or_else(|| "1800".into()),
                        ));
                    }
                    if let Some(et) = params.get("EmailType") {
                        g.email_type = et.clone();
                    }
                    g.records = recs;
                    r#"<?xml version="1.0"?><ApiResponse Status="OK"><DomainDNSSetHostsResult IsSuccess="true" /></ApiResponse>"#.to_string()
                } else {
                    g.commands.push("getHosts".into());
                    let mut xml = String::from(
                        r#"<?xml version="1.0"?><ApiResponse Status="OK"><CommandResponse><DomainDNSGetHostsResult EmailType=""#,
                    );
                    xml.push_str(&xml_escape(&g.email_type));
                    xml.push_str(r#"">"#);
                    for (n, t, a, mx, ttl) in &g.records {
                        xml.push_str(&format!(
                            r#"<host Name="{}" Type="{}" Address="{}" MXPref="{}" TTL="{}" />"#,
                            xml_escape(n),
                            xml_escape(t),
                            xml_escape(a),
                            xml_escape(mx),
                            xml_escape(ttl)
                        ));
                    }
                    xml.push_str("</DomainDNSGetHostsResult></CommandResponse></ApiResponse>");
                    xml
                }
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = std::io::Write::write_all(&mut stream, resp.as_bytes());
        }
    });
    format!("http://{addr}/xml.response")
}

fn live_http_cmd(cred: &std::path::Path, base: &str) -> Command {
    let mut c = Command::new(bin());
    c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_ENV", cred);
    c.env("SURMOUNT_DNS_ZONE_NAMECHEAP_API_BASE", base);
    c.env_remove("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR");
    c.env_remove("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR");
    c
}

/// `--live set-a` without MOCK_DIR must getHosts then setHosts on the
/// local listener, not return the mock-dir error.
#[test]
fn live_set_a_without_mock_dir_gethosts_then_sethosts() {
    let h = Harness::new();
    let state = std::sync::Arc::new(std::sync::Mutex::new(FakeNc {
        records: vec![
            (
                "@".into(),
                "A".into(),
                "203.0.113.10".into(),
                "10".into(),
                "1800".into(),
            ),
            (
                "mail".into(),
                "A".into(),
                "203.0.113.11".into(),
                "10".into(),
                "1800".into(),
            ),
        ],
        email_type: "MX".into(),
        commands: Vec::new(),
    }));
    let base = spawn_namecheap_http(state.clone());
    let out = live_http_cmd(&h.cred, &base)
        .args(["--live", "set-a", "splora", "192.0.2.10"])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!text.contains("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR"));
    assert!(!text.contains("live Namecheap API is not used in tests"));
    assert!(!text.contains("surmount-test-apikey-not-real-zone-001"));
    let g = state.lock().unwrap();
    assert_eq!(g.commands, ["getHosts", "setHosts"]);
    assert_eq!(g.email_type, "MX");
    assert!(
        g.records
            .iter()
            .any(|(n, t, a, _, _)| n == "splora" && t == "A" && a == "192.0.2.10"),
        "setHosts must publish merged A: {:?}",
        g.records
    );
    assert!(
        g.records.iter().any(|(n, t, _, _, _)| n == "@" && t == "A"),
        "setHosts must re-apply other getHosts records"
    );
    assert!(
        g.records
            .iter()
            .any(|(n, t, _, _, _)| n == "mail" && t == "A")
    );
}

/// Operator shape: `--live set-host HOST --a V4` without MOCK_DIR.
#[test]
fn live_set_host_without_mock_dir_merges_a() {
    let h = Harness::new();
    let state = std::sync::Arc::new(std::sync::Mutex::new(FakeNc {
        records: vec![(
            "@".into(),
            "A".into(),
            "203.0.113.10".into(),
            "10".into(),
            "1800".into(),
        )],
        email_type: "MX".into(),
        commands: Vec::new(),
    }));
    let base = spawn_namecheap_http(state.clone());
    let out = live_http_cmd(&h.cred, &base)
        .args(["--live", "set-host", "splora", "--a", "192.0.2.10"])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(!text.contains("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR"));
    let g = state.lock().unwrap();
    assert_eq!(g.commands, ["getHosts", "setHosts"]);
    assert!(
        g.records
            .iter()
            .any(|(n, t, a, _, _)| n == "splora" && t == "A" && a == "192.0.2.10")
    );
}

#[test]
fn dry_run_without_mock_dir_gethosts_not_sethosts() {
    let h = Harness::new();
    let state = std::sync::Arc::new(std::sync::Mutex::new(FakeNc {
        records: vec![(
            "@".into(),
            "A".into(),
            "203.0.113.10".into(),
            "10".into(),
            "1800".into(),
        )],
        email_type: "MX".into(),
        commands: Vec::new(),
    }));
    let base = spawn_namecheap_http(state.clone());
    let out = live_http_cmd(&h.cred, &base)
        .args(["set-a", "splora", "192.0.2.10"])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.to_ascii_lowercase().contains("dry-run"));
    let g = state.lock().unwrap();
    assert_eq!(g.commands, ["getHosts"]);
    assert!(
        !g.records
            .iter()
            .any(|(n, _, _, _, _)| n == "splora")
    );
}
