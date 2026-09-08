//! Pipe-separated mock zone: Name|Type|Address|MXPref|TTL

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::Result;

use crate::cred::die;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub name: String,
    pub r#type: String,
    pub address: String,
    pub mx_pref: String,
    pub ttl: String,
}

pub fn load(dir: &Path) -> Result<Vec<Record>> {
    fs::create_dir_all(dir)
        .map_err(|e| die("acme-dns-hook-namecheap", format!("mock dir: {e}")))?;
    let hosts = dir.join("hosts.txt");
    if !hosts.exists() {
        fs::write(&hosts, "").map_err(|e| die("acme-dns-hook-namecheap", format!("mock: {e}")))?;
        let _ = fs::set_permissions(&hosts, fs::Permissions::from_mode(0o600));
    }
    let file =
        fs::File::open(&hosts).map_err(|e| die("acme-dns-hook-namecheap", format!("mock: {e}")))?;
    let mut recs = Vec::new();
    for line in BufReader::new(file).lines() {
        let mut line = line.map_err(|e| die("acme-dns-hook-namecheap", format!("mock: {e}")))?;
        if let Some(s) = line.strip_suffix('\r') {
            line = s.to_string();
        }
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let mut p = line.splitn(5, '|');
        let name = p.next().unwrap_or("").to_string();
        let typ = p.next().unwrap_or("").to_string();
        let addr = p.next().unwrap_or("").to_string();
        let mx = p.next().unwrap_or("10");
        let ttl = p.next().unwrap_or("1800");
        if name.is_empty() || typ.is_empty() {
            continue;
        }
        recs.push(Record {
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
    Ok(recs)
}

pub fn save(dir: &Path, recs: &[Record]) -> Result<()> {
    fs::create_dir_all(dir)
        .map_err(|e| die("acme-dns-hook-namecheap", format!("mock dir: {e}")))?;
    let hosts = dir.join("hosts.txt");
    let tmp = dir.join(".hosts.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| die("acme-dns-hook-namecheap", format!("mock: {e}")))?;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
        for r in recs {
            writeln!(
                f,
                "{}|{}|{}|{}|{}",
                r.name, r.r#type, r.address, r.mx_pref, r.ttl
            )
            .map_err(|e| die("acme-dns-hook-namecheap", format!("mock: {e}")))?;
        }
    }
    fs::rename(&tmp, &hosts).map_err(|e| die("acme-dns-hook-namecheap", format!("mock: {e}")))?;
    let _ = fs::set_permissions(&hosts, fs::Permissions::from_mode(0o600));
    Ok(())
}
