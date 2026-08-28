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

fn load_zone(mock: Option<&std::path::Path>) -> anyhow::Result<Vec<Record>> {
    let Some(dir) = mock else {
        return Err(die(
            "acme-dns-hook-namecheap",
            "live Namecheap API requires ClientIp; tests must set SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR",
        ));
    };
    zone::load(dir)
}

fn save_zone(mock: Option<&std::path::Path>, recs: &[Record]) -> anyhow::Result<()> {
    let Some(dir) = mock else {
        return Err(die(
            "acme-dns-hook-namecheap",
            "live setHosts refused without MOCK_DIR",
        ));
    };
    zone::save(dir, recs)
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
    let mut recs = load_zone(mock)?;
    recs.retain(|r| !(r.r#type == "TXT" && r.name == host));
    recs.push(Record {
        name: host.clone(),
        r#type: "TXT".into(),
        address: value.into(),
        mx_pref: "10".into(),
        ttl: cred.txt_ttl.to_string(),
    });
    save_zone(mock, &recs)?;
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
    let mut recs = load_zone(mock)?;
    recs.retain(|r| !(r.r#type == "TXT" && r.name == host));
    save_zone(mock, &recs)?;
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
    let recs = load_zone(mock)?;
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
