//! Product DNS-01 set/clear/wait protocol.

use std::ffi::OsString;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::cred::{DEFAULT_CRED_PATH, die, fqdn_to_hostname, load_credentials};
use crate::zone::{self, Record};

pub fn run<I, S>(args: I) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match run_inner(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn run_inner<I, S>(args: I) -> anyhow::Result<u8>
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
    if args.is_empty() {
        return Err(die("acme-dns-hook-namecheap", "usage: set|clear|wait ..."));
    }
    let path = std::env::var("SURMOUNT_ACME_DNS_NAMECHEAP_ENV")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CRED_PATH));
    let cred = load_credentials(&path, "acme-dns-hook-namecheap")?;
    let mock = std::env::var_os("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR").map(PathBuf::from);
    let verbose = std::env::var("SURMOUNT_ACME_DNS_NAMECHEAP_VERBOSE")
        .ok()
        .as_deref()
        == Some("1");
    let cmd = args[0].as_str();
    match cmd {
        "set" => {
            if args.len() < 3 {
                return Err(die(
                    "acme-dns-hook-namecheap",
                    "set requires <fqdn> <txt_value>",
                ));
            }
            cmd_set(&cred, mock.as_deref(), &args[1], &args[2], verbose)?;
            Ok(0)
        }
        "clear" => {
            if args.len() < 2 {
                return Err(die("acme-dns-hook-namecheap", "clear requires <fqdn>"));
            }
            cmd_clear(&cred, mock.as_deref(), &args[1], verbose)?;
            Ok(0)
        }
        "wait" => {
            if args.len() < 3 {
                return Err(die(
                    "acme-dns-hook-namecheap",
                    "wait requires <fqdn> <txt_value>",
                ));
            }
            Ok(cmd_wait(
                &cred,
                mock.as_deref(),
                &args[1],
                &args[2],
                verbose,
            )?)
        }
        other => Err(die(
            "acme-dns-hook-namecheap",
            format!("unknown command: {other} (expected set|clear|wait)"),
        )),
    }
}

fn load_zone(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
) -> anyhow::Result<(Vec<Record>, String)> {
    if let Some(dir) = mock {
        return Ok((zone::load(dir)?, String::new()));
    }
    let hosts = live::get_hosts(cred)?;
    Ok((hosts.records, hosts.email_type))
}

fn save_zone(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    recs: Vec<Record>,
    email_type: String,
) -> anyhow::Result<()> {
    if recs.is_empty() {
        return Err(die(
            "acme-dns-hook-namecheap",
            "getHosts returned no records (refuse setHosts wipe)",
        ));
    }
    if let Some(dir) = mock {
        return zone::save(dir, &recs);
    }
    live::set_hosts(
        cred,
        &live::Hosts {
            records: recs,
            email_type,
        },
    )
}

fn cmd_set(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    fqdn: &str,
    value: &str,
    verbose: bool,
) -> anyhow::Result<()> {
    if fqdn.is_empty() {
        return Err(die("acme-dns-hook-namecheap", "set: missing fqdn"));
    }
    if value.is_empty() {
        return Err(die("acme-dns-hook-namecheap", "set: missing txt_value"));
    }
    if fqdn.chars().any(|c| c.is_control()) || value.chars().any(|c| c.is_control()) {
        return Err(die(
            "acme-dns-hook-namecheap",
            "set: control characters refused",
        ));
    }
    let host = fqdn_to_hostname(fqdn, &cred.sld, &cred.tld)?;
    let (mut recs, email_type) = load_zone(cred, mock)?;
    recs.retain(|r| !(r.r#type == "TXT" && r.name == host));
    recs.push(Record {
        name: host.clone(),
        r#type: "TXT".into(),
        address: value.into(),
        mx_pref: "10".into(),
        ttl: cred.txt_ttl.to_string(),
    });
    save_zone(cred, mock, recs, email_type)?;
    if verbose {
        eprintln!("acme-dns-hook-namecheap: set host={host}");
    }
    if cred.settle_secs > 0 {
        thread::sleep(Duration::from_secs(cred.settle_secs));
    }
    Ok(())
}

fn cmd_clear(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    fqdn: &str,
    verbose: bool,
) -> anyhow::Result<()> {
    if fqdn.chars().any(|c| c.is_control()) {
        return Err(die(
            "acme-dns-hook-namecheap",
            "clear: control characters refused",
        ));
    }
    let host = fqdn_to_hostname(fqdn, &cred.sld, &cred.tld)?;
    let (mut recs, email_type) = load_zone(cred, mock)?;
    recs.retain(|r| !(r.r#type == "TXT" && r.name == host));
    save_zone(cred, mock, recs, email_type)?;
    if verbose {
        eprintln!("acme-dns-hook-namecheap: clear host={host}");
    }
    Ok(())
}

fn cmd_wait(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    fqdn: &str,
    value: &str,
    verbose: bool,
) -> anyhow::Result<u8> {
    let host = fqdn_to_hostname(fqdn, &cred.sld, &cred.tld)?;
    let (recs, _) = load_zone(cred, mock)?;
    if recs
        .iter()
        .any(|r| r.r#type == "TXT" && r.name == host && r.address == value)
    {
        if verbose {
            eprintln!("acme-dns-hook-namecheap: wait ready host={host}");
        }
        Ok(0)
    } else {
        if verbose {
            eprintln!("acme-dns-hook-namecheap: wait not-ready host={host}");
        }
        Ok(1)
    }
}

/// Live Namecheap getHosts / setHosts. Never log ApiKey.
///
/// See [getHosts](https://www.namecheap.com/support/api/methods/domains-dns/get-hosts/)
/// and [setHosts](https://www.namecheap.com/support/api/methods/domains-dns/set-hosts/)
/// (accessed: 2026-09-07). Tests override the API base to a local HTTP
/// listener. Never live Namecheap as CI green. Lives in this tracked file
/// so crane git-filtered source includes it. `surmount-dns-zone --live`
/// reuses this module (same XML API, ClientIp required).
pub mod live {
    use anyhow::Result;

    use crate::cred::{Credentials, die};
    use crate::zone::Record;

    const DEFAULT_API: &str = "https://api.namecheap.com/xml.response";
    const PREFIX: &str = "acme-dns-hook-namecheap";

    #[derive(Debug)]
    pub struct Hosts {
        pub records: Vec<Record>,
        pub email_type: String,
    }

    pub fn get_hosts(cred: &Credentials) -> Result<Hosts> {
        get_hosts_with_prefix(cred, PREFIX)
    }

    /// Same getHosts path with a caller prefix (dns-zone errors).
    pub fn get_hosts_with_prefix(cred: &Credentials, prefix: &str) -> Result<Hosts> {
        require_client_ip(cred, prefix)?;
        let xml = api_get(
            cred,
            &[
                ("Command", "namecheap.domains.dns.getHosts"),
                ("SLD", cred.sld.as_str()),
                ("TLD", cred.tld.as_str()),
            ],
            prefix,
        )?;
        parse_get_hosts(&xml, prefix)
    }

    pub fn set_hosts(cred: &Credentials, hosts: &Hosts) -> Result<()> {
        set_hosts_with_prefix(cred, hosts, PREFIX)
    }

    /// Same setHosts path with a caller prefix (dns-zone errors).
    pub fn set_hosts_with_prefix(cred: &Credentials, hosts: &Hosts, prefix: &str) -> Result<()> {
        require_client_ip(cred, prefix)?;
        if hosts.records.is_empty() {
            return Err(die(
                prefix,
                "getHosts returned no records (refuse setHosts wipe)",
            ));
        }
        if hosts.email_type.is_empty() {
            return Err(die(
                prefix,
                "getHosts missing EmailType (refuse setHosts that would un-publish)",
            ));
        }
        let mut pairs: Vec<(String, String)> = vec![
            ("Command".into(), "namecheap.domains.dns.setHosts".into()),
            ("SLD".into(), cred.sld.clone()),
            ("TLD".into(), cred.tld.clone()),
            ("EmailType".into(), hosts.email_type.clone()),
        ];
        for (i, r) in hosts.records.iter().enumerate() {
            let n = i + 1;
            pairs.push((format!("HostName{n}"), r.name.clone()));
            pairs.push((format!("RecordType{n}"), r.r#type.clone()));
            pairs.push((format!("Address{n}"), r.address.clone()));
            pairs.push((format!("MXPref{n}"), r.mx_pref.clone()));
            pairs.push((format!("TTL{n}"), r.ttl.clone()));
        }
        let q: Vec<(&str, &str)> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let xml = api_get(cred, &q, prefix)?;
        parse_set_hosts_ok(&xml, prefix)
    }

    fn require_client_ip(cred: &Credentials, prefix: &str) -> Result<()> {
        if cred.client_ip.trim().is_empty() {
            return Err(die(
                prefix,
                "BLOCKED: live Namecheap API requires ClientIp (laptop egress whitelist)",
            ));
        }
        Ok(())
    }

    fn api_base() -> String {
        std::env::var("SURMOUNT_DNS_ZONE_NAMECHEAP_API_BASE")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("SURMOUNT_ACME_DNS_NAMECHEAP_API_BASE")
                    .ok()
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or_else(|| DEFAULT_API.to_string())
    }

    fn api_get(cred: &Credentials, extra: &[(&str, &str)], prefix: &str) -> Result<String> {
        let base = api_base();
        let https_only = base.starts_with("https://");
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .https_only(https_only)
            .build()
            .map_err(|_| die(prefix, "Namecheap API HTTP client failed (URL not logged)"))?;
        let mut q: Vec<(&str, &str)> = vec![
            ("ApiUser", cred.api_user.as_str()),
            ("ApiKey", cred.api_key.as_str()),
            ("UserName", cred.user_name.as_str()),
            ("ClientIp", cred.client_ip.as_str()),
        ];
        q.extend_from_slice(extra);
        let resp = client
            .get(&base)
            .query(&q)
            .send()
            .map_err(|_| die(prefix, "Namecheap API HTTP failed (URL not logged)"))?;
        let status = resp.status();
        let body = resp
            .text()
            .map_err(|_| die(prefix, "Namecheap API body unreadable (URL not logged)"))?;
        if !status.is_success() {
            return Err(api_xml_error(&body, prefix).unwrap_or_else(|| {
                die(
                    prefix,
                    "Namecheap API HTTP status failed (URL and body not logged)",
                )
            }));
        }
        Ok(body)
    }

    fn api_xml_error(xml: &str, prefix: &str) -> Option<anyhow::Error> {
        let low = xml.to_ascii_lowercase();
        let num = xml_attr(xml, "Number").unwrap_or_default();
        let num = if num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() {
            num
        } else {
            String::new()
        };
        let num_note = if num.is_empty() {
            String::new()
        } else {
            format!(" Number={num}")
        };
        if low.contains("invalid request ip") {
            return Some(die(
                prefix,
                format!(
                    "BLOCKED: Invalid request IP (Namecheap ClientIp whitelist of laptop egress){num_note}"
                ),
            ));
        }
        if xml_attr(xml, "Status").as_deref() == Some("ERROR")
            || low.contains("status=\"error\"")
            || low.contains("status='error'")
        {
            let msg = xml_tag_text(xml, "Error").unwrap_or_default();
            let msg_l = msg.to_ascii_lowercase();
            if msg_l.contains("invalid request ip") {
                return Some(die(
                    prefix,
                    format!(
                        "BLOCKED: Invalid request IP (Namecheap ClientIp whitelist of laptop egress){num_note}"
                    ),
                ));
            }
            if msg_l.contains("not using")
                && (msg_l.contains("name server") || msg_l.contains("dns"))
            {
                return Some(die(
                    prefix,
                    format!("Namecheap API: zone is not on Namecheap hosted DNS{num_note}"),
                ));
            }
            if msg_l.contains("domain not found") || msg_l.contains("no longer exists") {
                return Some(die(
                    prefix,
                    format!("Namecheap API: domain not found{num_note}"),
                ));
            }
            if msg_l.contains("api key") {
                return Some(die(
                    prefix,
                    format!("Namecheap API: credentials rejected{num_note}"),
                ));
            }
            return Some(die(
                prefix,
                format!("Namecheap API error{num_note} (body not logged; check ClientIp and zone)"),
            ));
        }
        None
    }

    fn parse_get_hosts(xml: &str, prefix: &str) -> Result<Hosts> {
        if let Some(e) = api_xml_error(xml, prefix) {
            return Err(e);
        }
        if xml_attr(xml, "Status").as_deref() != Some("OK")
            && !xml.to_ascii_lowercase().contains("status=\"ok\"")
            && !xml.to_ascii_lowercase().contains("status='ok'")
        {
            return Err(die(prefix, "Namecheap getHosts XML missing Status=OK"));
        }
        let email_type = xml_attr(xml, "EmailType").unwrap_or_default();
        let mut records = Vec::new();
        for tag in xml_tags(xml, "host") {
            let name = xml_attr(&tag, "Name").unwrap_or_default();
            let typ = xml_attr(&tag, "Type").unwrap_or_default();
            if name.is_empty() || typ.is_empty() {
                continue;
            }
            records.push(Record {
                name,
                r#type: typ,
                address: xml_attr(&tag, "Address").unwrap_or_default(),
                mx_pref: nonempty(xml_attr(&tag, "MXPref").unwrap_or_default(), "10"),
                ttl: nonempty(xml_attr(&tag, "TTL").unwrap_or_default(), "1800"),
            });
        }
        if records.is_empty() {
            return Err(die(
                prefix,
                "getHosts returned no records (refuse setHosts wipe)",
            ));
        }
        Ok(Hosts {
            records,
            email_type,
        })
    }

    fn parse_set_hosts_ok(xml: &str, prefix: &str) -> Result<()> {
        if let Some(e) = api_xml_error(xml, prefix) {
            return Err(e);
        }
        let low = xml.to_ascii_lowercase();
        if low.contains("issuccess=\"true\"")
            || low.contains("issuccess='true'")
            || xml_attr(xml, "Status").as_deref() == Some("OK")
            || low.contains("status=\"ok\"")
        {
            return Ok(());
        }
        Err(die(prefix, "Namecheap setHosts did not report success"))
    }

    fn nonempty(s: String, fallback: &str) -> String {
        if s.is_empty() { fallback.into() } else { s }
    }

    fn xml_tags(xml: &str, name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let want = name.to_ascii_lowercase();
        let bytes = xml.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'<' {
                i += 1;
                continue;
            }
            let rest = &xml[i + 1..];
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphabetic())
                .collect();
            if ident.to_ascii_lowercase() != want {
                i += 1;
                continue;
            }
            let after = &rest[ident.len()..];
            if let Some(end) = after.find('>') {
                out.push(format!("<{ident}{}", &after[..=end]));
                i += 1 + ident.len() + end + 1;
            } else {
                break;
            }
        }
        out
    }

    fn xml_tag_text(xml: &str, name: &str) -> Option<String> {
        let open = format!("<{name}");
        let low = xml.to_ascii_lowercase();
        let open_l = open.to_ascii_lowercase();
        let start = low.find(&open_l)?;
        let after = &xml[start..];
        let gt = after.find('>')?;
        if after.as_bytes().get(gt.saturating_sub(1)) == Some(&b'/') {
            return None;
        }
        let inner = &after[gt + 1..];
        let close_l = format!("</{}>", name.to_ascii_lowercase());
        let inner_l = inner.to_ascii_lowercase();
        let end = inner_l.find(&close_l)?;
        Some(xml_unescape(&inner[..end]))
    }

    fn xml_attr(blob: &str, name: &str) -> Option<String> {
        let patterns = [
            format!("{name}=\""),
            format!("{name}='"),
            format!(" {name}=\""),
            format!(" {name}='"),
        ];
        for pat in patterns {
            if let Some(idx) = find_ci(blob, &pat) {
                let rest = &blob[idx + pat.len()..];
                let quote = pat.chars().last()?;
                let end = rest.find(quote)?;
                return Some(xml_unescape(&rest[..end]));
            }
        }
        None
    }

    fn find_ci(hay: &str, needle: &str) -> Option<usize> {
        hay.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
    }

    fn xml_unescape(s: &str) -> String {
        s.replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parse_get_hosts_records_and_email_type() {
            let xml = r#"<?xml version="1.0"?><ApiResponse Status="OK"><CommandResponse><DomainDNSGetHostsResult EmailType="MX"><host Name="@" Type="A" Address="203.0.113.10" MXPref="10" TTL="1800" /><host Name="_acme-challenge" Type="TXT" Address="abc" MXPref="10" TTL="60" /></DomainDNSGetHostsResult></CommandResponse></ApiResponse>"#;
            let h = parse_get_hosts(xml, PREFIX).unwrap();
            assert_eq!(h.email_type, "MX");
            assert_eq!(h.records.len(), 2);
            assert_eq!(h.records[0].name, "@");
            assert_eq!(h.records[1].address, "abc");
        }

        #[test]
        fn parse_get_hosts_invalid_request_ip() {
            let xml = r#"<ApiResponse Status="ERROR"><Errors><Error Number="1011150">Invalid request IP</Error></Errors></ApiResponse>"#;
            let err = parse_get_hosts(xml, PREFIX).unwrap_err().to_string();
            assert!(err.to_ascii_lowercase().contains("invalid request ip"));
            assert!(err.contains("BLOCKED"));
        }

        #[test]
        fn parse_get_hosts_empty_refuses_wipe() {
            let xml = r#"<ApiResponse Status="OK"><DomainDNSGetHostsResult EmailType="MX"></DomainDNSGetHostsResult></ApiResponse>"#;
            let err = parse_get_hosts(xml, PREFIX).unwrap_err().to_string();
            assert!(err.to_ascii_lowercase().contains("no records"));
        }
    }
}
