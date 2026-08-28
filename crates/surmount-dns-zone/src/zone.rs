//! Hermetic mock zone (pipe format) plus in-memory merge.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::cred::die;
use crate::dnssec::refuse_sha1_ds;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub name: String,
    pub r#type: String,
    pub address: String,
    pub mx_pref: String,
    pub ttl: String,
}

impl Record {
    pub fn pipe_line(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.name, self.r#type, self.address, self.mx_pref, self.ttl
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Zone {
    pub records: Vec<Record>,
    pub email_type: String,
}

pub fn mock_dir_from_env() -> Option<PathBuf> {
    std::env::var_os("SURMOUNT_DNS_ZONE_NAMECHEAP_MOCK_DIR")
        .or_else(|| std::env::var_os("SURMOUNT_ACME_DNS_NAMECHEAP_MOCK_DIR"))
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

pub fn load_mock(dir: &Path) -> Result<Zone> {
    fs::create_dir_all(dir).map_err(|e| die(format!("mock dir: {e}")))?;
    let hosts = dir.join("hosts.txt");
    if !hosts.exists() {
        fs::write(&hosts, "").map_err(|e| die(format!("mock hosts: {e}")))?;
        let _ = fs::set_permissions(&hosts, fs::Permissions::from_mode(0o600));
    }
    let file = fs::File::open(&hosts).map_err(|e| die(format!("mock hosts: {e}")))?;
    let mut zone = Zone::default();
    for line in BufReader::new(file).lines() {
        let mut line = line.map_err(|e| die(format!("mock hosts: {e}")))?;
        if let Some(s) = line.strip_suffix('\r') {
            line = s.to_string();
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(5, '|');
        let name = parts.next().unwrap_or("").to_string();
        let typ = parts.next().unwrap_or("").to_string();
        let addr = parts.next().unwrap_or("").to_string();
        let mx = parts.next().unwrap_or("10");
        let ttl = parts.next().unwrap_or("1800");
        if name.is_empty() || typ.is_empty() {
            continue;
        }
        zone.records.push(Record {
            name,
            r#type: typ,
            address: addr,
            mx_pref: if mx.is_empty() {
                "10".into()
            } else {
                mx.into()
            },
            ttl: if ttl.is_empty() {
                "1800".into()
            } else {
                ttl.into()
            },
        });
    }
    let et = dir.join("email_type.txt");
    if et.is_file() {
        let mut s = fs::read_to_string(&et).unwrap_or_default();
        if let Some(stripped) = s.strip_suffix('\r') {
            s = stripped.to_string();
        }
        zone.email_type = s.trim().to_string();
    }
    Ok(zone)
}

pub fn save_mock(dir: &Path, zone: &Zone) -> Result<()> {
    refuse_sha1_ds(&zone.records)?;
    fs::create_dir_all(dir).map_err(|e| die(format!("mock dir: {e}")))?;
    let hosts = dir.join("hosts.txt");
    let tmp = dir.join(".hosts.tmp");
    if tmp
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&tmp);
        return Err(die("refuse: mktemp produced a symlink"));
    }
    {
        let mut f = fs::File::create(&tmp).map_err(|e| die(format!("mock tmp: {e}")))?;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        for rec in &zone.records {
            writeln!(f, "{}", rec.pipe_line()).map_err(|e| die(format!("mock write: {e}")))?;
        }
    }
    fs::rename(&tmp, &hosts).map_err(|e| die(format!("mock rename: {e}")))?;
    let _ = fs::set_permissions(&hosts, fs::Permissions::from_mode(0o600));
    if !zone.email_type.is_empty() {
        let et = dir.join("email_type.txt");
        fs::write(&et, format!("{}\n", zone.email_type))
            .map_err(|e| die(format!("email_type: {e}")))?;
        let _ = fs::set_permissions(&et, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn drop_type_at_host(zone: &mut Zone, host: &str, typ: &str) {
    zone.records
        .retain(|r| !(r.name == host && r.r#type == typ));
}

pub fn drop_type_at_host_ci(zone: &mut Zone, host: &str, typ: &str) {
    let want = typ.to_ascii_lowercase();
    zone.records
        .retain(|r| !(r.name == host && r.r#type.eq_ignore_ascii_case(&want)));
}

pub fn drop_txt_prefix_at_host(zone: &mut Zone, host: &str, prefix: &str) {
    let prefix = prefix.to_ascii_lowercase();
    zone.records.retain(|r| {
        if r.name == host && r.r#type == "TXT" {
            !r.address.to_ascii_lowercase().starts_with(&prefix)
        } else {
            true
        }
    });
}

pub fn drop_caa_tag_at_host(zone: &mut Zone, host: &str, tag: &str) {
    let want = tag.to_ascii_lowercase();
    zone.records.retain(|r| {
        if r.name == host && r.r#type == "CAA" {
            let mut toks = r.address.split_whitespace();
            let _flags = toks.next();
            let t = toks
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .to_ascii_lowercase();
            t != want
        } else {
            true
        }
    });
}

pub fn append(zone: &mut Zone, rec: Record) {
    zone.records.push(rec);
}

pub fn print_hosts(zone: &Zone) -> String {
    if zone.records.is_empty() {
        return "(empty zone)\n".to_string();
    }
    let mut out = format!(
        "{:<20} {:<6} {:<44} {:>6} {}\n",
        "NAME", "TYPE", "ADDRESS", "MXPREF", "TTL"
    );
    for rec in &zone.records {
        let mut addr = rec.address.clone();
        if addr.len() > 44 {
            addr = format!("{}...", &addr[..41]);
        }
        out.push_str(&format!(
            "{:<20} {:<6} {:<44} {:>6} {}\n",
            rec.name, rec.r#type, addr, rec.mx_pref, rec.ttl
        ));
    }
    out
}

pub fn email_type_is_fwd(et: &str) -> bool {
    et.eq_ignore_ascii_case("FWD")
}
