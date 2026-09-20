//! CLI: list / set-* / delete-host. Default dry-run.
//! `--live` without MOCK_DIR: Namecheap getHosts, merge, setHosts.

use std::ffi::OsString;
use std::net::Ipv4Addr;
use std::path::PathBuf;

use anyhow::Result;

use crate::cred::{DEFAULT_TTL, die, load_credentials, resolve_cred_path};
use crate::dnssec::refuse_sha1_ds;
use crate::hostname::to_hostname;
use crate::zone::{
    Record, Zone, append, drop_caa_tag_at_host, drop_txt_prefix_at_host, drop_type_at_host,
    drop_type_at_host_ci, email_type_is_fwd, load_mock, mock_dir_from_env, print_hosts, save_mock,
};

pub const USAGE: &str = r#"Usage:
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] set-a HOST A_IPV4 [--ttl N]
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] set-aaaa HOST AAAA_IPV6 [--ttl N]
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] set-host HOST [--a V4] [--aaaa V6] [--ttl N]
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] set-txt HOST TXT_VALUE [--ttl N]
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] set-caa HOST FLAGS TAG VALUE [--ttl N]
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] set-mx HOST EXCHANGE [--pref N] [--ttl N]
  surmount-dns-zone [--dry-run|--live] [--credentials FILE] delete-host HOST --type TYPE
  surmount-dns-zone [--credentials FILE] list
  surmount-dns-zone --help

list prints EmailType from getHosts (FWD vs MX). --live set-mx fails if
EmailType is FWD; that is not a public MX flip while Email Forwarding is on.

Merge A/AAAA/TXT/CAA/MX, or drop one host+type, via Namecheap getHosts + setHosts.
Default is dry-run (print plan; no API write). Pass --live to apply.
"#;

struct Flags {
    live: bool,
    ttl_override: Option<String>,
    credentials: Option<PathBuf>,
}

pub fn run<I, S>(args: I) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    match run_inner(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{err}");
            1
        }
    }
}

fn run_inner<I, S>(args: I) -> Result<u8>
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
        eprint!("{USAGE}");
        return Ok(1);
    }

    let mut flags = Flags {
        live: false,
        ttl_override: None,
        credentials: None,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print!("{USAGE}");
                return Ok(0);
            }
            "--live" => {
                flags.live = true;
                i += 1;
            }
            "--dry-run" => {
                flags.live = false;
                i += 1;
            }
            "--ttl" => {
                if i + 1 >= args.len() {
                    return Err(die("--ttl requires a value"));
                }
                flags.ttl_override = Some(args[i + 1].clone());
                i += 2;
            }
            "--credentials" | "--env" => {
                if i + 1 >= args.len() {
                    return Err(die("--credentials requires a file path"));
                }
                flags.credentials = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "set-a" | "set-aaaa" | "set-host" | "set-txt" | "set-caa" | "set-mx"
            | "delete-host" | "list" => break,
            other => return Err(die(format!("unknown argument: {other} (try --help)"))),
        }
    }
    if i >= args.len() {
        return Err(die(
            "missing command (set-a|set-aaaa|set-host|set-txt|set-caa|set-mx|delete-host|list)",
        ));
    }
    let cmd = args[i].clone();
    let rest = &args[i + 1..];

    let cred_path = resolve_cred_path(flags.credentials.as_deref());
    let cred = load_credentials(&cred_path)?;
    let mock = mock_dir_from_env();

    match cmd.as_str() {
        "list" => cmd_list(&cred, mock.as_deref()),
        "set-a" => {
            if rest.len() < 2 {
                return Err(die("set-a requires HOST A_IPV4"));
            }
            let (host, a, ttl) =
                parse_ttl_tail(&rest[0], Some(&rest[1]), None, &rest[2..], &flags)?;
            cmd_set_records(
                &cred,
                mock.as_deref(),
                flags.live,
                &host,
                Some(&a),
                None,
                &ttl,
            )
        }
        "set-aaaa" => {
            if rest.len() < 2 {
                return Err(die("set-aaaa requires HOST AAAA_IPV6"));
            }
            let (host, aaaa, ttl) =
                parse_ttl_tail(&rest[0], Some(&rest[1]), None, &rest[2..], &flags)?;
            cmd_set_records(
                &cred,
                mock.as_deref(),
                flags.live,
                &host,
                None,
                Some(&aaaa),
                &ttl,
            )
        }
        "set-host" => {
            if rest.is_empty() {
                return Err(die("set-host requires HOST"));
            }
            let host_raw = &rest[0];
            let mut a = None;
            let mut aaaa = None;
            let mut ttl = flags
                .ttl_override
                .clone()
                .unwrap_or_else(|| DEFAULT_TTL.to_string());
            let mut j = 1;
            while j < rest.len() {
                match rest[j].as_str() {
                    "--a" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--a requires an IPv4"));
                        }
                        a = Some(rest[j + 1].as_str());
                        j += 2;
                    }
                    "--aaaa" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--aaaa requires an IPv6"));
                        }
                        aaaa = Some(rest[j + 1].as_str());
                        j += 2;
                    }
                    "--ttl" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--ttl requires a value"));
                        }
                        ttl = rest[j + 1].clone();
                        j += 2;
                    }
                    other => return Err(die(format!("unknown set-host argument: {other}"))),
                }
            }
            cmd_set_records(&cred, mock.as_deref(), flags.live, host_raw, a, aaaa, &ttl)
        }
        "set-txt" => {
            if rest.len() < 2 {
                return Err(die("set-txt requires HOST TXT_VALUE"));
            }
            let (host, txt, ttl) =
                parse_ttl_tail(&rest[0], Some(&rest[1]), None, &rest[2..], &flags)?;
            cmd_set_txt(&cred, mock.as_deref(), flags.live, &host, &txt, &ttl)
        }
        "set-caa" => {
            if rest.len() < 4 {
                return Err(die("set-caa requires HOST FLAGS TAG VALUE"));
            }
            let host = &rest[0];
            let flags_v = &rest[1];
            let tag = &rest[2];
            let value = &rest[3];
            let mut ttl = flags
                .ttl_override
                .clone()
                .unwrap_or_else(|| DEFAULT_TTL.to_string());
            let mut j = 4;
            while j < rest.len() {
                match rest[j].as_str() {
                    "--ttl" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--ttl requires a value"));
                        }
                        ttl = rest[j + 1].clone();
                        j += 2;
                    }
                    other => return Err(die(format!("unknown set-caa argument: {other}"))),
                }
            }
            cmd_set_caa(
                &cred,
                mock.as_deref(),
                flags.live,
                host,
                flags_v,
                tag,
                value,
                &ttl,
            )
        }
        "set-mx" => {
            if rest.len() < 2 {
                return Err(die("set-mx requires HOST EXCHANGE"));
            }
            let host = &rest[0];
            let exchange = &rest[1];
            let mut pref = "10".to_string();
            let mut ttl = flags
                .ttl_override
                .clone()
                .unwrap_or_else(|| DEFAULT_TTL.to_string());
            let mut j = 2;
            while j < rest.len() {
                match rest[j].as_str() {
                    "--pref" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--pref requires a value"));
                        }
                        pref = rest[j + 1].clone();
                        j += 2;
                    }
                    "--ttl" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--ttl requires a value"));
                        }
                        ttl = rest[j + 1].clone();
                        j += 2;
                    }
                    other => return Err(die(format!("unknown set-mx argument: {other}"))),
                }
            }
            cmd_set_mx(
                &cred,
                mock.as_deref(),
                flags.live,
                host,
                exchange,
                &pref,
                &ttl,
            )
        }
        "delete-host" => {
            if rest.is_empty() {
                return Err(die("delete-host requires HOST --type TYPE"));
            }
            let host = &rest[0];
            let mut typ = None;
            let mut j = 1;
            while j < rest.len() {
                match rest[j].as_str() {
                    "--type" => {
                        if j + 1 >= rest.len() {
                            return Err(die("--type requires a RecordType"));
                        }
                        typ = Some(rest[j + 1].as_str());
                        j += 2;
                    }
                    other => return Err(die(format!("unknown delete-host argument: {other}"))),
                }
            }
            let typ = typ.ok_or_else(|| die("delete-host requires --type TYPE"))?;
            cmd_delete_host(&cred, mock.as_deref(), flags.live, host, typ)
        }
        other => Err(die(format!("unknown command: {other}"))),
    }
}

fn parse_ttl_tail(
    host: &str,
    val: Option<&str>,
    _unused: Option<&str>,
    tail: &[String],
    flags: &Flags,
) -> Result<(String, String, String)> {
    let mut ttl = flags
        .ttl_override
        .clone()
        .unwrap_or_else(|| DEFAULT_TTL.to_string());
    let mut j = 0;
    while j < tail.len() {
        match tail[j].as_str() {
            "--ttl" => {
                if j + 1 >= tail.len() {
                    return Err(die("--ttl requires a value"));
                }
                ttl = tail[j + 1].clone();
                j += 2;
            }
            other => return Err(die(format!("unknown argument: {other}"))),
        }
    }
    Ok((host.to_string(), val.unwrap_or("").to_string(), ttl))
}

const LIVE_PREFIX: &str = "dns-zone-namecheap";

fn acme_cred(cred: &crate::cred::Credentials) -> surmount_acme_namecheap::cred::Credentials {
    surmount_acme_namecheap::cred::Credentials {
        api_user: cred.api_user.clone(),
        api_key: cred.api_key.clone(),
        user_name: cred.user_name.clone(),
        client_ip: cred.client_ip.clone(),
        sld: cred.sld.clone(),
        tld: cred.tld.clone(),
        settle_secs: 0,
        txt_ttl: DEFAULT_TTL,
        path: cred.path.clone(),
    }
}

fn record_from_acme(r: surmount_acme_namecheap::zone::Record) -> Record {
    Record {
        name: r.name,
        r#type: r.r#type,
        address: r.address,
        mx_pref: r.mx_pref,
        ttl: r.ttl,
    }
}

fn record_to_acme(r: &Record) -> surmount_acme_namecheap::zone::Record {
    surmount_acme_namecheap::zone::Record {
        name: r.name.clone(),
        r#type: r.r#type.clone(),
        address: r.address.clone(),
        mx_pref: r.mx_pref.clone(),
        ttl: r.ttl.clone(),
    }
}

fn zone_from_hosts(h: surmount_acme_namecheap::hook::live::Hosts) -> Zone {
    Zone {
        records: h.records.into_iter().map(record_from_acme).collect(),
        email_type: h.email_type,
    }
}

fn hosts_from_zone(zone: &Zone) -> surmount_acme_namecheap::hook::live::Hosts {
    surmount_acme_namecheap::hook::live::Hosts {
        records: zone.records.iter().map(record_to_acme).collect(),
        email_type: zone.email_type.clone(),
    }
}

fn load_zone(cred: &crate::cred::Credentials, mock: Option<&std::path::Path>) -> Result<Zone> {
    if let Some(dir) = mock {
        return load_mock(dir);
    }
    let hosts = surmount_acme_namecheap::hook::live::get_hosts_with_prefix(
        &acme_cred(cred),
        LIVE_PREFIX,
    )?;
    Ok(zone_from_hosts(hosts))
}

fn save_zone(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    zone: &Zone,
) -> Result<()> {
    refuse_sha1_ds(&zone.records)?;
    if let Some(dir) = mock {
        return save_mock(dir, zone);
    }
    surmount_acme_namecheap::hook::live::set_hosts_with_prefix(
        &acme_cred(cred),
        &hosts_from_zone(zone),
        LIVE_PREFIX,
    )
}

fn assert_ttl(t: &str) -> Result<String> {
    let n: u32 = t.parse().map_err(|_| die("TTL must be an integer >= 60"))?;
    if n < 60 {
        return Err(die("TTL must be an integer >= 60"));
    }
    Ok(t.to_string())
}

fn assert_ipv4(ip: &str) -> Result<()> {
    ip.parse::<Ipv4Addr>()
        .map(|_| ())
        .map_err(|_| die(format!("not an IPv4 address: {ip}")))
}

fn assert_ipv6(ip: &str) -> Result<()> {
    if ip.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(die("IPv6: control/space refused"));
    }
    if !ip.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
        return Err(die(format!("not an IPv6 address shape: {ip}")));
    }
    if !ip.contains(':') {
        return Err(die(format!("not an IPv6 address: {ip}")));
    }
    Ok(())
}

fn cmd_list(cred: &crate::cred::Credentials, mock: Option<&std::path::Path>) -> Result<u8> {
    let zone = load_zone(cred, mock)?;
    let mode = if mock.is_some() { "mock" } else { "live-api" };
    let et = if zone.email_type.is_empty() {
        "unset"
    } else {
        zone.email_type.as_str()
    };
    println!(
        "dns-zone-namecheap: zone {} (records={}; EmailType={et}; mode={mode})",
        cred.zone(),
        zone.records.len()
    );
    println!("EmailType: {et}");
    print!("{}", print_hosts(&zone));
    Ok(0)
}

fn apply_or_dry(
    cred: &crate::cred::Credentials,
    live: bool,
    mock: Option<&std::path::Path>,
    zone: &Zone,
    host: &str,
    extra_applied: &str,
) -> Result<u8> {
    refuse_sha1_ds(&zone.records)?;
    if !live {
        println!("dns-zone-namecheap: dry-run only (no setHosts). Pass --live to apply.");
        return Ok(0);
    }
    save_zone(cred, mock, zone)?;
    println!("dns-zone-namecheap: applied setHosts for host={host}{extra_applied}");
    Ok(0)
}

fn plan_header(
    cred: &crate::cred::Credentials,
    host: &str,
    live: bool,
    before: usize,
    after: usize,
) {
    println!(
        "dns-zone-namecheap: plan for host={host} zone={}",
        cred.zone()
    );
    println!("  mode: {}", if live { "LIVE" } else { "dry-run" });
    println!("  records before merge: {before}; after: {after}");
    println!("  note: setHosts replaces entire zone; other records are re-applied from getHosts");
    println!("  note: do not --live concurrent with ACME DNS-01 challenge set/clear");
}

fn cmd_set_records(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    live: bool,
    host_raw: &str,
    a: Option<&str>,
    aaaa: Option<&str>,
    ttl: &str,
) -> Result<u8> {
    if a.is_none() && aaaa.is_none() {
        return Err(die("set: need --a and/or --aaaa (or set-a / set-aaaa)"));
    }
    let ttl = assert_ttl(ttl)?;
    let host = to_hostname(host_raw, &cred.sld, &cred.tld)?;
    if let Some(v) = a {
        assert_ipv4(v)?;
    }
    if let Some(v) = aaaa {
        assert_ipv6(v)?;
    }
    let mut zone = load_zone(cred, mock)?;
    let before = zone.records.len();
    if let Some(v) = a {
        drop_type_at_host(&mut zone, &host, "A");
        append(
            &mut zone,
            Record {
                name: host.clone(),
                r#type: "A".into(),
                address: v.into(),
                mx_pref: "10".into(),
                ttl: ttl.clone(),
            },
        );
    }
    if let Some(v) = aaaa {
        drop_type_at_host(&mut zone, &host, "AAAA");
        append(
            &mut zone,
            Record {
                name: host.clone(),
                r#type: "AAAA".into(),
                address: v.into(),
                mx_pref: "10".into(),
                ttl: ttl.clone(),
            },
        );
    }
    plan_header(cred, &host, live, before, zone.records.len());
    if let Some(v) = a {
        println!("  set A    {host} -> {v} (ttl={ttl})");
    }
    if let Some(v) = aaaa {
        println!("  set AAAA {host} -> {v} (ttl={ttl})");
    }
    println!("  planned zone:");
    print!("{}", print_hosts(&zone));
    apply_or_dry(cred, live, mock, &zone, &host, "")
}

fn cmd_set_txt(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    live: bool,
    host_raw: &str,
    txt: &str,
    ttl: &str,
) -> Result<u8> {
    if txt.is_empty() {
        return Err(die("TXT value empty"));
    }
    if txt.chars().any(|c| c.is_control()) {
        return Err(die("TXT value: control characters refused"));
    }
    if txt.len() > 2048 {
        return Err(die(format!("TXT value too long ({} > 2048)", txt.len())));
    }
    let ttl = assert_ttl(ttl)?;
    let host = to_hostname(host_raw, &cred.sld, &cred.tld)?;
    let lower = txt.to_ascii_lowercase();
    let merge = if lower.starts_with("v=spf1") {
        "spf"
    } else if lower.starts_with("v=dmarc1") {
        "dmarc"
    } else {
        "all-txt"
    };
    let mut zone = load_zone(cred, mock)?;
    let before = zone.records.len();
    match merge {
        "spf" => drop_txt_prefix_at_host(&mut zone, &host, "v=spf1"),
        "dmarc" => drop_txt_prefix_at_host(&mut zone, &host, "v=dmarc1"),
        _ => drop_type_at_host(&mut zone, &host, "TXT"),
    }
    append(
        &mut zone,
        Record {
            name: host.clone(),
            r#type: "TXT".into(),
            address: txt.into(),
            mx_pref: "10".into(),
            ttl: ttl.clone(),
        },
    );
    plan_header(cred, &host, live, before, zone.records.len());
    let preview = if txt.len() <= 96 {
        txt.to_string()
    } else {
        format!("{}...({} chars)", &txt[..96], txt.len())
    };
    println!("  set TXT  {host} -> {preview} (ttl={ttl}; merge={merge})");
    println!("  planned zone:");
    print!("{}", print_hosts(&zone));
    apply_or_dry(cred, live, mock, &zone, &host, " type=TXT")
}

#[allow(clippy::too_many_arguments)]
fn cmd_set_caa(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    live: bool,
    host_raw: &str,
    flags_v: &str,
    tag: &str,
    value: &str,
    ttl: &str,
) -> Result<u8> {
    let flags_n: u32 = flags_v
        .parse()
        .map_err(|_| die(format!("CAA FLAGS must be integer 0-255: {flags_v}")))?;
    if flags_n > 255 {
        return Err(die(format!("CAA FLAGS must be integer 0-255: {flags_v}")));
    }
    if tag.is_empty() {
        return Err(die("CAA TAG empty"));
    }
    if tag.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(die("CAA TAG: control/space refused"));
    }
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err(die(format!("CAA TAG has invalid characters: {tag}")));
    }
    if value.is_empty() {
        return Err(die("CAA VALUE empty"));
    }
    if value.contains('"') {
        return Err(die(
            "CAA VALUE must not contain double quotes (tool adds them)",
        ));
    }
    let ttl = assert_ttl(ttl)?;
    let host = to_hostname(host_raw, &cred.sld, &cred.tld)?;
    let caa_addr = format!("{flags_v} {tag} \"{value}\"");
    let mut zone = load_zone(cred, mock)?;
    let before = zone.records.len();
    drop_caa_tag_at_host(&mut zone, &host, tag);
    append(
        &mut zone,
        Record {
            name: host.clone(),
            r#type: "CAA".into(),
            address: caa_addr.clone(),
            mx_pref: "10".into(),
            ttl: ttl.clone(),
        },
    );
    plan_header(cred, &host, live, before, zone.records.len());
    println!("  set CAA  {host} -> {caa_addr} (ttl={ttl}; merge=tag)");
    println!("  note: CAA is RecordType=CAA (not TXT)");
    println!("  planned zone:");
    print!("{}", print_hosts(&zone));
    apply_or_dry(
        cred,
        live,
        mock,
        &zone,
        &host,
        &format!(" type=CAA tag={tag}"),
    )
}

fn cmd_set_mx(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    live: bool,
    host_raw: &str,
    exchange: &str,
    pref: &str,
    ttl: &str,
) -> Result<u8> {
    if exchange.is_empty() {
        return Err(die("MX exchange empty"));
    }
    if exchange
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(die("MX exchange: control/space refused"));
    }
    if !exchange
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(die(format!(
            "MX exchange has invalid characters: {exchange}"
        )));
    }
    let pref_n: u32 = pref
        .parse()
        .map_err(|_| die(format!("MX preference must be integer 0-65535: {pref}")))?;
    if pref_n > 65535 {
        return Err(die(format!(
            "MX preference must be integer 0-65535: {pref}"
        )));
    }
    let ttl = assert_ttl(ttl)?;
    let host = to_hostname(host_raw, &cred.sld, &cred.tld)?;
    let mut zone = load_zone(cred, mock)?;
    let before = zone.records.len();
    let et = if zone.email_type.is_empty() {
        "unset"
    } else {
        zone.email_type.as_str()
    };
    println!("dns-zone-namecheap: EmailType={et}");
    if email_type_is_fwd(&zone.email_type) && live {
        return Err(die(format!(
            "Email Forwarding is still on; change Mail Settings to Custom MX, then retry. UI: Domain List, Manage {}, Advanced DNS, Mail Settings, Email Forwarding -> Custom MX.",
            cred.zone()
        )));
    }
    drop_type_at_host(&mut zone, &host, "MX");
    append(
        &mut zone,
        Record {
            name: host.clone(),
            r#type: "MX".into(),
            address: exchange.into(),
            mx_pref: pref.into(),
            ttl: ttl.clone(),
        },
    );
    plan_header(cred, &host, live, before, zone.records.len());
    println!("  set MX   {host} -> {exchange} (pref={pref}; ttl={ttl}; merge=all-mx)");
    if email_type_is_fwd(&zone.email_type) {
        println!(
            "  note: EmailType is FWD; --live set-mx is not a public MX flip while Email Forwarding is on"
        );
        println!("  note: change Mail Settings to Custom MX, then retry --live");
    }
    println!("  planned zone:");
    print!("{}", print_hosts(&zone));
    apply_or_dry(cred, live, mock, &zone, &host, " type=MX")
}

fn cmd_delete_host(
    cred: &crate::cred::Credentials,
    mock: Option<&std::path::Path>,
    live: bool,
    host_raw: &str,
    rec_type: &str,
) -> Result<u8> {
    if rec_type.is_empty() {
        return Err(die("record type empty"));
    }
    if rec_type
        .chars()
        .any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(die("record type: control/space refused"));
    }
    if rec_type.len() > 16
        || !rec_type
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic())
            .unwrap_or(false)
        || !rec_type.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(die(format!(
            "record type has invalid characters: {rec_type}"
        )));
    }
    let host = to_hostname(host_raw, &cred.sld, &cred.tld)?;
    let mut zone = load_zone(cred, mock)?;
    let before = zone.records.len();
    if before == 0 {
        return Err(die("getHosts returned no records (refuse setHosts wipe)"));
    }
    drop_type_at_host_ci(&mut zone, &host, rec_type);
    let after = zone.records.len();
    let dropped = before.saturating_sub(after);
    if dropped == 0 {
        return Err(die(format!(
            "no {rec_type} record at host={host} to delete"
        )));
    }
    if after == 0 {
        return Err(die(
            "delete-host would leave an empty zone (refuse empty setHosts)",
        ));
    }
    plan_header(cred, &host, live, before, after);
    println!("  delete {rec_type} at {host} (dropped={dropped})");
    println!("  planned zone:");
    print!("{}", print_hosts(&zone));
    apply_or_dry(
        cred,
        live,
        mock,
        &zone,
        &host,
        &format!(" delete type={rec_type}"),
    )
}
