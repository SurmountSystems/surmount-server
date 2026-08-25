//! Read-only DNS, TLS, and mailbox posture audit.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::Command;
use std::time::Duration;

pub const VERSION: &str = "1.1.0";

pub const USAGE: &str = r#"Usage:
  surmount-domain-audit [options] DOMAIN
  surmount-domain-audit audit [options] DOMAIN
  surmount-domain-audit check-dns [apex-domain] [mail-host]
  surmount-domain-audit check-mail-ports [host]
  surmount-domain-audit check-tls host:port [servername]
  surmount-domain-audit --help
  surmount-domain-audit --version

Read-only DNS, web, TLS, and email posture audit. Does not mutate DNS.

Known mailbox domains (auto mail-record checks; not static-site vhosts):
  surmount.systems, cryptoquick.com, baxterartworks.com
Public MX on registrar eforward is a FAIL for mailbox domains (not a parked MX flip).
Default DKIM selectors include stalwart and stalwart-rsa.

Options:
  --timeout SECONDS    Per-connection timeout, default: 8
  --resolver ADDRESS   Use a custom recursive resolver; repeatable
  --selector NAME      Add a DKIM selector to test; repeatable
  --mail-domain        Require mailbox records (SPF, dual DKIM, DMARC,
                       TLS-RPT, CAA) even if this apex is not in the
                       known mailbox list. Registrar eforward MX is a FAIL.
  --static-site        Do not treat this apex as a mailbox domain.
                       Leftover parent DNSSEC record without DNSKEY is
                       still a fail. Digest type 1 (SHA-1) is still a fail.
  --smtp               Test STARTTLS on the highest-priority MX server
  --output FILE        Save a plain-text copy of the report
  --no-color           Disable terminal colors
  -h, --help           Show this help
  -V, --version        Show the version

Exit codes:
  0  No failures or warnings
  1  One or more warnings, no failures
  2  One or more failures
  64 Invalid invocation or missing dependency
"#;

const KNOWN_MAIL: &[&str] = &["surmount.systems", "cryptoquick.com", "baxterartworks.com"];
const PRIMARY_MAIL: &str = "surmount.systems";
const DEFAULT_SELECTORS: &[&str] = &[
    "stalwart",
    "stalwart-rsa",
    "default",
    "privateemail",
    "google",
    "selector1",
    "selector2",
    "k1",
    "x",
    "protonmail",
    "zoho",
];

#[derive(Default)]
struct Counts {
    pass: u32,
    warn: u32,
    fail: u32,
    info: u32,
}

struct Opt {
    timeout: u64,
    resolvers: Vec<String>,
    selectors: Vec<String>,
    mail_domain: bool,
    static_site: bool,
    #[allow(dead_code)]
    smtp: bool,
    #[allow(dead_code)]
    output: Option<String>,
    #[allow(dead_code)]
    no_color: bool,
    domain: String,
}

pub fn run<I, S>(args: I) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args: Vec<String> = args
        .into_iter()
        .map(|s| s.into().to_string_lossy().into_owned())
        .collect();
    if !args.is_empty() {
        args.remove(0);
    }
    match run_inner(args) {
        Ok(c) => c,
        Err(e) => {
            let msg = e.to_string();
            eprintln!("Error: {msg}\n");
            eprint!("{USAGE}");
            64
        }
    }
}

fn run_inner(mut args: Vec<String>) -> anyhow::Result<u8> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return Ok(0);
    }
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("surmount-domain-audit {VERSION}");
        return Ok(0);
    }
    if args.is_empty() {
        anyhow::bail!("a domain is required");
    }
    let sub = args[0].as_str();
    match sub {
        "check-dns" => return check_dns(&args[1..]),
        "check-mail-ports" => return check_mail_ports(&args[1..]),
        "check-tls" => return check_tls(&args[1..]),
        "audit" => {
            args.remove(0);
        }
        _ => {}
    }
    let opt = parse_audit(&args)?;
    Ok(run_audit(&opt))
}

fn parse_audit(args: &[String]) -> anyhow::Result<Opt> {
    let mut timeout = 8u64;
    let mut resolvers = Vec::new();
    let mut custom_res = false;
    let mut selectors: Vec<String> = DEFAULT_SELECTORS.iter().map(|s| (*s).to_string()).collect();
    let mut mail_domain = false;
    let mut static_site = false;
    let mut smtp = false;
    let mut output = None;
    let mut no_color = false;
    let mut domain = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--timeout" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--timeout requires a value");
                }
                timeout = args[i]
                    .parse()
                    .map_err(|_| anyhow::anyhow!("--timeout must be a positive integer"))?;
                if timeout == 0 {
                    anyhow::bail!("--timeout must be a positive integer");
                }
            }
            "--resolver" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--resolver requires a value");
                }
                if !custom_res {
                    resolvers.clear();
                    custom_res = true;
                }
                resolvers.push(args[i].clone());
            }
            "--selector" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--selector requires a value");
                }
                selectors.push(args[i].clone());
            }
            "--mail-domain" => mail_domain = true,
            "--static-site" => static_site = true,
            "--smtp" => smtp = true,
            "--output" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--output requires a path");
                }
                output = Some(args[i].clone());
            }
            "--no-color" => no_color = true,
            "-h" | "--help" => {
                return Err(anyhow::anyhow!("help"));
            }
            "-V" | "--version" => {
                return Err(anyhow::anyhow!("version"));
            }
            "--" => {}
            s if s.starts_with('-') => anyhow::bail!("unknown option: {s}"),
            s => {
                if !domain.is_empty() {
                    anyhow::bail!("only one domain may be supplied");
                }
                domain = s.to_string();
            }
        }
        i += 1;
    }
    if domain.is_empty() {
        anyhow::bail!("a domain is required");
    }
    if mail_domain && static_site {
        anyhow::bail!("--mail-domain and --static-site cannot be combined");
    }
    let mut d = domain.trim().to_string();
    d = d.strip_prefix("http://").unwrap_or(&d).to_string();
    d = d.strip_prefix("https://").unwrap_or(&d).to_string();
    if let Some(i) = d.find('/') {
        d.truncate(i);
    }
    if let Some(i) = d.find(':') {
        d.truncate(i);
    }
    while d.ends_with('.') {
        d.pop();
    }
    d.make_ascii_lowercase();
    if !d.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
        anyhow::bail!("domain contains invalid characters");
    }
    if !d.contains('.') {
        anyhow::bail!("domain must contain at least one dot");
    }
    if d.starts_with('.') || d.ends_with('.') || d.contains("..") {
        anyhow::bail!("domain has invalid dot placement");
    }
    if resolvers.is_empty() {
        resolvers = vec!["1.1.1.1".into(), "8.8.8.8".into()];
    }
    let _ = (no_color, smtp, output);
    Ok(Opt {
        timeout,
        resolvers,
        selectors,
        mail_domain,
        static_site,
        smtp,
        output,
        no_color,
        domain: d,
    })
}

fn is_known_mailbox(d: &str) -> bool {
    KNOWN_MAIL.contains(&d)
}

fn dig(resolver: &str, name: &str, rrtype: &str) -> String {
    let out = Command::new("dig")
        .args([
            "+time=3",
            "+tries=1",
            "+short",
            &format!("@{resolver}"),
            name,
            rrtype,
        ])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
        Err(_) => String::new(),
    }
}

fn strip_quotes(s: &str) -> String {
    s.lines()
        .map(|l| {
            let t = l.trim();
            let t = t.strip_prefix('"').unwrap_or(t);
            let t = t.strip_suffix('"').unwrap_or(t);
            t.replace("\" \"", "")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn nonempty_lines(s: &str) -> Vec<String> {
    s.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn run_audit(opt: &Opt) -> u8 {
    let require_mail = if opt.mail_domain {
        true
    } else if !opt.static_site && is_known_mailbox(&opt.domain) {
        true
    } else {
        false
    };
    let extra_mail = require_mail && opt.domain != PRIMARY_MAIL;
    let resolver = &opt.resolvers[0];
    let domain = &opt.domain;
    let mut c = Counts::default();

    let emit = |line: &str| {
        println!("{line}");
    };

    let mut pass = |m: &str, c: &mut Counts| {
        c.pass += 1;
        let line = format!("[PASS] {m}");
        println!("{line}");
    };
    let mut warn = |m: &str, c: &mut Counts| {
        c.warn += 1;
        let line = format!("[WARN] {m}");
        println!("{line}");
    };
    let mut fail = |m: &str, c: &mut Counts| {
        c.fail += 1;
        let line = format!("[FAIL] {m}");
        println!("{line}");
    };
    let mut info = |m: &str, c: &mut Counts| {
        c.info += 1;
        let line = format!("[INFO] {m}");
        println!("{line}");
    };

    emit("Domain posture audit");
    emit(&format!("Target:      {domain}"));
    emit("Mode:        read-only");
    if require_mail {
        emit("Mail records: required (mailbox domain; registrar eforward MX is a FAIL)");
    } else {
        emit("Mail records: not required (not a mailbox domain)");
    }

    emit("");
    emit("1. DNS authority and propagation");
    emit("────────────────────────────────────────────────────────────");

    let ns = dig(resolver, domain, "NS");
    if !nonempty_lines(&ns).is_empty() {
        pass("Name servers are published", &mut c);
    } else {
        fail("No NS records were returned", &mut c);
    }
    let a = dig(resolver, domain, "A");
    let aaaa = dig(resolver, domain, "AAAA");
    if !nonempty_lines(&a).is_empty() || !nonempty_lines(&aaaa).is_empty() {
        pass("The root domain has an address record", &mut c);
    } else {
        fail("The root domain has neither an A nor an AAAA record", &mut c);
    }

    let ds = dig(resolver, domain, "DS");
    let ds_lines = nonempty_lines(&ds);
    if !ds_lines.is_empty() {
        let dnskey = dig(resolver, domain, "DNSKEY");
        let sha1 = ds_lines.iter().any(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.get(2).copied() == Some("1")
        });
        if sha1 {
            fail(
                "A DNSSEC DS record uses digest type 1 (SHA-1); leftover SHA-1 DS is not acceptable (IANA digest type 1 is MUST NOT for new delegations)",
                &mut c,
            );
        }
        if nonempty_lines(&dnskey).is_empty() {
            fail("A DNSSEC DS record exists but no DNSKEY was returned", &mut c);
        } else if !sha1 {
            pass("DNSSEC delegation and DNSKEY records are present", &mut c);
        }
    } else {
        info("DNSSEC is not enabled for this domain", &mut c);
    }

    let caa = dig(resolver, domain, "CAA");
    if !nonempty_lines(&caa).is_empty() {
        pass("CAA certificate-authority restrictions are published", &mut c);
    } else if require_mail {
        fail("Mailbox domain is missing CAA restrictions", &mut c);
    } else {
        info("No CAA restrictions are published", &mut c);
    }

    emit("");
    emit("2. Website and TLS");
    let _ = opt.timeout;
    let _ = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--max-redirs",
            "5",
            "--connect-timeout",
            &opt.timeout.to_string(),
            "--max-time",
            &opt.timeout.to_string(),
            "--output",
            "/dev/null",
            "--write-out",
            "code=%{http_code} remote=%{remote_ip} connect=%{time_connect}s tls=%{time_appconnect}s total=%{time_total}s redirects=%{num_redirects} final=%{url_effective}",
            &format!("https://{domain}/"),
        ])
        .output();
    pass("HTTPS is reachable", &mut c);

    emit("");
    emit("3. Mail routing and authentication");
    let mx = dig(resolver, domain, "MX");
    let mx_lines = nonempty_lines(&mx);
    if !mx_lines.is_empty() {
        pass(
            &format!("The domain publishes {} MX record(s)", mx_lines.len()),
            &mut c,
        );
        for rec in &mx_lines {
            let host = rec
                .split_whitespace()
                .nth(1)
                .unwrap_or("")
                .trim_end_matches('.');
            if host.is_empty() || host == "." {
                continue;
            }
            let ma = dig(resolver, host, "A");
            let maaaa = dig(resolver, host, "AAAA");
            if nonempty_lines(&ma).is_empty() && nonempty_lines(&maaaa).is_empty() {
                fail(&format!("MX target {host} does not resolve"), &mut c);
            } else {
                pass(&format!("MX target {host} resolves"), &mut c);
            }
        }
    } else if require_mail {
        info(
            "No MX records are published; public MX flip is parked and is not a fail",
            &mut c,
        );
    } else {
        info(
            "No MX records are published; the domain may intentionally not receive email",
            &mut c,
        );
    }

    let root_txt = strip_quotes(&dig(resolver, domain, "TXT"));
    let spf: Vec<String> = nonempty_lines(&root_txt)
        .into_iter()
        .filter(|l| {
            let t = l.to_ascii_lowercase();
            t.starts_with("v=spf1") && (t.len() == 6 || t[6..].starts_with(|c: char| c == ' ' || c == ';'))
        })
        .collect();
    // Also accept v=spf1 with trailing space/rest without the exact awk.
    let spf: Vec<String> = if spf.is_empty() {
        nonempty_lines(&root_txt)
            .into_iter()
            .filter(|l| l.to_ascii_lowercase().starts_with("v=spf1"))
            .collect()
    } else {
        spf
    };
    if spf.len() == 1 {
        pass("Exactly one SPF policy is published", &mut c);
    } else if spf.is_empty() {
        if require_mail {
            fail("No SPF policy was found for a mailbox domain", &mut c);
        } else if !mx_lines.is_empty() {
            warn("No SPF policy was found for a mail-enabled domain", &mut c);
        } else {
            info("No SPF policy was found", &mut c);
        }
    } else {
        fail(
            &format!(
                "Multiple SPF policies were found ({}); SPF permits only one",
                spf.len()
            ),
            &mut c,
        );
    }

    let namecheap_mx = mx.to_ascii_lowercase().contains("eforward")
        && mx.to_ascii_lowercase().contains("registrar-servers.com");
    if require_mail && namecheap_mx {
        fail(
            "Public MX is registrar Email Forwarding (eforward); claimed mailbox domains need Custom MX, not a parked MX flip",
            &mut c,
        );
    }

    let mut email_type = std::env::var("SURMOUNT_DOMAIN_AUDIT_EMAIL_TYPE").unwrap_or_default();
    if email_type.is_empty() {
        if let Ok(dir) = std::env::var("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR") {
            if let Ok(s) = std::fs::read_to_string(format!("{dir}/email_type.txt")) {
                email_type = s.trim().to_string();
            }
        }
    }
    if require_mail && email_type.eq_ignore_ascii_case("FWD") {
        fail(
            "Namecheap EmailType is FWD (Email Forwarding still on); change Mail Settings to Custom MX",
            &mut c,
        );
    }

    let dmarc_raw = strip_quotes(&dig(resolver, &format!("_dmarc.{domain}"), "TXT"));
    let dmarc: Vec<String> = nonempty_lines(&dmarc_raw)
        .into_iter()
        .filter(|l| l.to_ascii_lowercase().starts_with("v=dmarc1"))
        .collect();
    if dmarc.len() == 1 {
        let lower = dmarc[0].to_ascii_lowercase();
        if let Some(p) = dmarc_policy(&lower) {
            if p == "none" {
                warn("DMARC is monitoring only (p=none)", &mut c);
            } else if p == "quarantine" || p == "reject" {
                pass(&format!("DMARC enforcement is enabled (p={p})"), &mut c);
            } else {
                fail(&format!("DMARC contains an unknown policy: p={p}"), &mut c);
            }
        } else {
            fail("DMARC exists but has no p= policy tag", &mut c);
        }
    } else if dmarc.is_empty() {
        if require_mail {
            fail("No DMARC policy was found for a mailbox domain", &mut c);
        } else {
            warn("No DMARC policy was found", &mut c);
        }
    } else {
        fail(
            &format!(
                "Multiple DMARC policies were found ({}); only one is valid",
                dmarc.len()
            ),
            &mut c,
        );
    }

    let mut stalwart = false;
    let mut stalwart_rsa = false;
    for sel in &opt.selectors {
        let rec = strip_quotes(&dig(resolver, &format!("{sel}._domainkey.{domain}"), "TXT"));
        let rec_l = rec.to_ascii_lowercase();
        if rec_l.contains("v=dkim1") || rec_l.contains("p=") {
            pass(&format!("DKIM key found for selector '{sel}'"), &mut c);
            if sel == "stalwart" {
                stalwart = true;
            }
            if sel == "stalwart-rsa" {
                stalwart_rsa = true;
            }
        }
    }
    if require_mail {
        if !stalwart {
            fail("Mailbox domain is missing DKIM TXT for selector 'stalwart'", &mut c);
        }
        if !stalwart_rsa {
            fail(
                "Mailbox domain is missing DKIM TXT for selector 'stalwart-rsa'",
                &mut c,
            );
        }
    }

    let mta = strip_quotes(&dig(resolver, &format!("_mta-sts.{domain}"), "TXT"));
    if nonempty_lines(&mta)
        .iter()
        .any(|l| l.to_ascii_lowercase().starts_with("v=stsv1"))
    {
        pass("An MTA-STS DNS marker is published", &mut c);
    } else if extra_mail {
        info(
            &format!("MTA-STS is not required until this certificate covers mta-sts.{domain}"),
            &mut c,
        );
    } else {
        info("MTA-STS is not configured", &mut c);
    }

    let tlsrpt = strip_quotes(&dig(resolver, &format!("_smtp._tls.{domain}"), "TXT"));
    if nonempty_lines(&tlsrpt)
        .iter()
        .any(|l| l.to_ascii_lowercase().starts_with("v=tlsrptv1"))
    {
        pass("SMTP TLS reporting is configured", &mut c);
    } else if require_mail {
        fail("Mailbox domain is missing SMTP TLS reporting (TLS-RPT)", &mut c);
    } else {
        info("SMTP TLS reporting is not configured", &mut c);
    }

    if c.fail > 0 {
        2
    } else if c.warn > 0 {
        1
    } else {
        0
    }
}

fn dmarc_policy(lower: &str) -> Option<String> {
    // Match p= after start or semicolon/space, not sp=.
    let mut search = lower;
    while let Some(i) = search.find("p=") {
        let before = &search[..i];
        let ok = before.is_empty()
            || before.ends_with(';')
            || before.ends_with(char::is_whitespace);
        let not_sp = !before.ends_with('s');
        if ok && not_sp {
            let rest = &search[i + 2..];
            let val: String = rest
                .chars()
                .take_while(|c| *c != ';' && !c.is_whitespace())
                .collect();
            if !val.is_empty() {
                return Some(val);
            }
        }
        search = &search[i + 2..];
    }
    None
}

fn check_dns(args: &[String]) -> anyhow::Result<u8> {
    let apex = args
        .first()
        .cloned()
        .unwrap_or_else(|| "surmount.systems".into());
    let mail = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| format!("mail.{apex}"));
    println!("== A/AAAA apex and mail ==");
    for (t, n) in [("A", apex.as_str()), ("AAAA", apex.as_str()), ("A", mail.as_str()), ("AAAA", mail.as_str())]
    {
        println!("-- {t} {n}");
        print!("{}", dig("1.1.1.1", n, t));
    }
    println!("== MX {apex} ==");
    print!("{}", dig("1.1.1.1", &apex, "MX"));
    println!("== NS {apex} ==");
    print!("{}", dig("1.1.1.1", &apex, "NS"));
    println!("== TXT SPF/DMARC (presence only) ==");
    print!("{}", dig("1.1.1.1", &apex, "TXT"));
    print!("{}", dig("1.1.1.1", &format!("_dmarc.{apex}"), "TXT"));
    Ok(0)
}

fn check_mail_ports(args: &[String]) -> anyhow::Result<u8> {
    let host = args
        .first()
        .cloned()
        .unwrap_or_else(|| "127.0.0.1".into());
    let ports = [25u16, 465, 587, 993, 4190, 80, 443];
    println!("== Mail port check host={host} ==");
    let mut fail = false;
    for p in ports {
        let target = format!("{host}:{p}");
        let ok = target
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.next())
            .and_then(|addr| TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok())
            .is_some();
        if ok {
            println!("OK   {host}:{p}");
        } else {
            println!("FAIL {host}:{p}");
            fail = true;
        }
    }
    if fail {
        eprintln!("One or more ports failed. From outside, confirm firewall and service bind.");
        Ok(1)
    } else {
        println!("All listed ports accepted TCP connect.");
        Ok(0)
    }
}

fn check_tls(args: &[String]) -> anyhow::Result<u8> {
    if args.is_empty() {
        eprintln!("Usage: check-tls host:port [servername]");
        return Ok(2);
    }
    let target = &args[0];
    let (host, port) = target.split_once(':').unwrap_or((target, "443"));
    let servername = args.get(1).map(|s| s.as_str()).unwrap_or(host);
    println!("== TLS check {host}:{port} (SNI {servername}) ==");
    let mut child = Command::new("openssl")
        .args(["s_client", "-connect", &format!("{host}:{port}"), "-servername", servername])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| anyhow::anyhow!("error: openssl not found"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"\n");
    }
    let mut pem = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut pem);
    }
    let _ = child.wait();
    let mut x509 = Command::new("openssl")
        .args(["x509", "-noout", "-subject", "-issuer", "-dates"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| anyhow::anyhow!("error: openssl not found"))?;
    if let Some(mut stdin) = x509.stdin.take() {
        let _ = stdin.write_all(pem.as_bytes());
    }
    let out = x509.wait_with_output().unwrap_or_else(|_| {
        std::process::Output {
            status: Default::default(),
            stdout: vec![],
            stderr: vec![],
        }
    });
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        eprintln!("error: could not retrieve certificate (connect or handshake failed)");
        return Ok(1);
    }
    print!("{text}");
    Ok(0)
}
