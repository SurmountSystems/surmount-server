use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use surmount_deploy_host::{assert_safe_token, is_path_under_root, realpath_m};

use crate::error::ToolError;

pub const INVENTORY_USAGE: &str = r#"Usage:
  surmount-host-material-inventory --help
  surmount-host-material-inventory --staging DIR [options]
  surmount-host-cutover --inventory-only --staging DIR [options]

Report present/missing deploy material kinds under a private staging tree.
Never prints secret values. Exit non-zero when required kinds are missing.

Required:
  --staging DIR          Private staging root (secrets-install layout:
                         <id>/attributes + <id>/secret). Must stay outside
                         the public git work tree for live cutover.

Options:
  --host-id ID           Logical host id (recommended). Items must have
                         non-empty surmount.host; when set, host must match
                         (same as secrets-install). Mismatch/empty host
                         does not count as present.
  --with-vaultwarden     Require kind vaultwarden-admin (S7b / O3 profile).
                         Path must be durable
                         /var/lib/surmount/secrets/vaultwarden/admin.env
                         (or legacy /run/... alias) and secret must contain
                         ADMIN_TOKEN=<non-empty> (value never logged).
  --acme-path            ACME path: tls-cert/tls-key not required; report
                         optional ACME account parent and DNS hook status.
  --acme-account-path P  Host path for ACME account credentials note
                         (default: /var/lib/surmount/secrets/acme/account.json).
  --dns-hook-path P      Optional absolute path of operator DNS-01 hook;
                         reported present if the path exists as a regular
                         executable file (lab or host). Not required for
                         exit 0 unless --require-dns-hook.
  --require-dns-hook     Fail if --dns-hook-path is missing or not a
                         regular executable file.
  --json                 Emit one JSON object on stdout (still no secrets).
  -h, --help             Show this help.
"#;

pub const PATH_TLS_CERT: &str = "/var/lib/surmount/secrets/tls/cert.pem";
pub const PATH_TLS_KEY: &str = "/var/lib/surmount/secrets/tls/key.pem";
pub const PATH_SESSION: &str = "/var/lib/surmount/secrets/ui/session-secret";
pub const PATH_STALWART: &str = "/var/lib/surmount/secrets/ui/stalwart-api-token";
pub const PATH_VW: &str = "/var/lib/surmount/secrets/vaultwarden/admin.env";
pub const PATH_ACME_ACCOUNT: &str = "/var/lib/surmount/secrets/acme/account.json";
pub const PATH_NAMECHEAP: &str = "/var/lib/surmount/secrets/acme/namecheap.env";
pub const PATH_SHC: &str = "/var/lib/surmount/secrets/rdns/shc.env";

#[derive(Debug, Clone, Default)]
pub struct InventoryOpts {
    pub staging: PathBuf,
    pub host_id: Option<String>,
    pub with_vaultwarden: bool,
    pub acme_path: bool,
    pub acme_account_path: String,
    pub dns_hook_path: Option<String>,
    pub require_dns_hook: bool,
    pub json: bool,
}

pub fn expected_path_for(kind: &str) -> Option<&'static str> {
    match kind {
        "tls-cert" => Some(PATH_TLS_CERT),
        "tls-key" => Some(PATH_TLS_KEY),
        "session-secret" => Some(PATH_SESSION),
        "stalwart-token" => Some(PATH_STALWART),
        "vaultwarden-admin" => Some(PATH_VW),
        "namecheap-api" => Some(PATH_NAMECHEAP),
        "shc-api" => Some(PATH_SHC),
        _ => None,
    }
}

pub fn path_matches_conventional(kind: &str, hpath: &str) -> bool {
    if let Some(exp) = expected_path_for(kind) {
        if hpath == exp {
            return true;
        }
    }
    match kind {
        "tls-cert" => hpath == "/run/surmount-secrets/tls/cert.pem",
        "tls-key" => hpath == "/run/surmount-secrets/tls/key.pem",
        "session-secret" => hpath == "/run/surmount-secrets/ui/session-secret",
        "stalwart-token" => hpath == "/run/surmount-secrets/ui/stalwart-api-token",
        "vaultwarden-admin" => hpath == "/run/surmount-secrets/vaultwarden/admin.env",
        "namecheap-api" => hpath == "/run/surmount-secrets/acme/namecheap.env",
        "shc-api" => hpath == "/run/surmount-secrets/rdns/shc.env",
        _ => true,
    }
}

pub fn read_attr(attr_file: &Path, key: &str) -> Option<String> {
    let fh = fs::File::open(attr_file).ok()?;
    let prefix = format!("{key}=");
    for line in BufReader::new(fh).lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix(&prefix) {
            return Some(rest.to_string());
        }
    }
    None
}

pub fn vw_admin_secret_ok(secret_file: &Path) -> bool {
    let Ok(fh) = fs::File::open(secret_file) else {
        return false;
    };
    for line in BufReader::new(fh).lines().map_while(Result::ok) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(mut val) = line.strip_prefix("ADMIN_TOKEN=") {
            if val.len() >= 2 && val.starts_with('"') && val.ends_with('"') {
                val = &val[1..val.len() - 1];
            }
            if !val.is_empty() {
                return true;
            }
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct PresentItem {
    pub kind: String,
    pub path: String,
    pub item: String,
}

pub fn scan_staging(
    staging: &Path,
    host_id: Option<&str>,
    notes: &mut dyn Write,
) -> BTreeMap<String, PresentItem> {
    let mut out = BTreeMap::new();
    let Ok(rd) = fs::read_dir(staging) else {
        return out;
    };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for item_dir in dirs {
        let attr = item_dir.join("attributes");
        let secret = item_dir.join("secret");
        let item_name = item_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !attr.exists() || !secret.exists() {
            continue;
        }
        if attr
            .symlink_metadata()
            .ok()
            .map(|m| m.file_type().is_symlink())
            == Some(true)
        {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name}: attributes is a symlink"
            );
            continue;
        }
        if secret
            .symlink_metadata()
            .ok()
            .map(|m| m.file_type().is_symlink())
            == Some(true)
        {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name}: secret is a symlink"
            );
            continue;
        }
        if !attr.is_file() || !secret.is_file() {
            continue;
        }
        if secret.metadata().map(|m| m.len() == 0).unwrap_or(true) {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name}: secret is empty (zero-byte not present)"
            );
            continue;
        }
        let Some(kind) = read_attr(&attr, "surmount.kind").filter(|s| !s.is_empty()) else {
            continue;
        };
        let host = read_attr(&attr, "surmount.host").unwrap_or_default();
        if host.is_empty() {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name} kind={kind}: missing surmount.host (not present)"
            );
            continue;
        }
        if let Some(want) = host_id {
            if host != want {
                let _ = writeln!(
                    notes,
                    "host-material-inventory: note: skip item {item_name} kind={kind}: surmount.host mismatch (not present)"
                );
                continue;
            }
        }
        let hpath = read_attr(&attr, "surmount.path").unwrap_or_default();
        if hpath.is_empty() {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name} kind={kind}: missing surmount.path (not present)"
            );
            continue;
        }
        if expected_path_for(&kind).is_some() && !path_matches_conventional(&kind, &hpath) {
            let exp = expected_path_for(&kind).unwrap_or("");
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name} kind={kind}: path mismatch expected={exp} (or /run ephemeral alias) got={hpath} (not present)"
            );
            continue;
        }
        if kind == "vaultwarden-admin" && !vw_admin_secret_ok(&secret) {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: skip item {item_name} kind=vaultwarden-admin: secret missing non-empty ADMIN_TOKEN= line (not present)"
            );
            continue;
        }
        if out.contains_key(&kind) {
            let _ = writeln!(
                notes,
                "host-material-inventory: note: duplicate kind={kind} item={item_name} (keeping first)"
            );
            continue;
        }
        out.insert(
            kind.clone(),
            PresentItem {
                kind,
                path: hpath,
                item: item_name,
            },
        );
    }
    out
}

pub fn staging_has_kind(staging: &Path, kind: &str) -> bool {
    let Ok(rd) = fs::read_dir(staging) else {
        return false;
    };
    for ent in rd.flatten() {
        let attr = ent.path().join("attributes");
        if !attr.is_file() {
            continue;
        }
        if read_attr(&attr, "surmount.kind").as_deref() == Some(kind) {
            return true;
        }
    }
    false
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub fn run_inventory(opts: &InventoryOpts, repo_root: &Path) -> Result<(), ToolError> {
    if !opts.staging.is_dir() {
        return Err(ToolError::fail(format!(
            "staging is not a directory: {}",
            opts.staging.display()
        )));
    }
    let canon = realpath_m(&opts.staging);
    if is_path_under_root(repo_root, &canon) {
        return Err(ToolError::fail(format!(
            "refuse: secrets staging must be outside the public git work tree ({}). Use a private path. See docs/SECRETS.md",
            opts.staging.display()
        )));
    }
    if let Some(id) = opts.host_id.as_deref() {
        assert_safe_token("host-id", id)?;
        if id.starts_with('-') {
            return Err(ToolError::fail("host-id must not start with '-'"));
        }
    }
    if let Some(p) = opts.dns_hook_path.as_deref() {
        assert_safe_token("dns-hook-path", p)?;
    }
    assert_safe_token("acme-account-path", &opts.acme_account_path)?;

    let mut notes = std::io::stderr();
    let found = scan_staging(&opts.staging, opts.host_id.as_deref(), &mut notes);

    let profile = if opts.with_vaultwarden {
        "https+vaultwarden"
    } else {
        "https-only"
    };
    let path_mode = if opts.acme_path { "acme" } else { "pem" };

    let mut required: Vec<&str> = Vec::new();
    if !opts.acme_path {
        required.extend(["tls-cert", "tls-key"]);
    }
    required.extend(["session-secret", "stalwart-token"]);
    if opts.with_vaultwarden {
        required.push("vaultwarden-admin");
    }

    let mut present: Vec<&PresentItem> = Vec::new();
    let mut missing: Vec<&str> = Vec::new();
    for kind in &required {
        if let Some(item) = found.get(*kind) {
            present.push(item);
            if !opts.json {
                let _ = writeln!(
                    notes,
                    "host-material-inventory: present kind={} path={} item={}",
                    item.kind, item.path, item.item
                );
            }
        } else {
            missing.push(*kind);
            if !opts.json {
                let exp = expected_path_for(kind).unwrap_or("");
                let _ = writeln!(
                    notes,
                    "host-material-inventory: MISSING kind={kind} expected-path={exp}"
                );
            }
        }
    }

    if opts.acme_path && !opts.json {
        for kind in ["tls-cert", "tls-key"] {
            if let Some(item) = found.get(kind) {
                let _ = writeln!(
                    notes,
                    "host-material-inventory: optional-present kind={kind} path={} (acme-path: PEMs optional)",
                    item.path
                );
            } else {
                let _ = writeln!(
                    notes,
                    "host-material-inventory: optional-absent kind={kind} (acme-path: PEMs not required)"
                );
            }
        }
        let _ = writeln!(
            notes,
            "host-material-inventory: advisory acme-account-path={} (host runtime; not staging secret bytes)",
            opts.acme_account_path
        );
    }

    let mut dns_status = "unset";
    if let Some(hook) = opts.dns_hook_path.as_ref() {
        let p = Path::new(hook);
        let is_exec_file = p.is_file()
            && p.symlink_metadata()
                .map(|m| !m.file_type().is_symlink())
                .unwrap_or(false)
            && p.metadata()
                .map(|m| m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false);
        if is_exec_file {
            dns_status = "present";
            if !opts.json {
                let _ = writeln!(
                    notes,
                    "host-material-inventory: present dns-hook path={hook}"
                );
            }
        } else {
            dns_status = "missing";
            if !opts.json {
                let _ = writeln!(
                    notes,
                    "host-material-inventory: MISSING dns-hook path={hook} (regular executable file required)"
                );
            }
        }
    } else if opts.require_dns_hook {
        dns_status = "missing";
        if !opts.json {
            let _ = writeln!(
                notes,
                "host-material-inventory: MISSING dns-hook (pass --dns-hook-path)"
            );
        }
    }

    if !opts.json {
        let _ = writeln!(
            notes,
            "host-material-inventory: profile={profile} path-mode={path_mode} staging={} required={} missing={}",
            opts.staging
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            required.len(),
            missing.len()
        );
    }

    let mut incomplete = !missing.is_empty();
    if opts.require_dns_hook && dns_status != "present" {
        incomplete = true;
    }

    if opts.json {
        let present_joined: Vec<String> = present
            .iter()
            .map(|i| {
                format!(
                    "{{\"kind\":\"{}\",\"path\":\"{}\",\"item\":\"{}\"}}",
                    json_escape(&i.kind),
                    json_escape(&i.path),
                    json_escape(&i.item)
                )
            })
            .collect();
        let missing_joined: Vec<String> = missing
            .iter()
            .map(|k| {
                format!(
                    "{{\"kind\":\"{}\",\"expected_path\":\"{}\"}}",
                    json_escape(k),
                    json_escape(expected_path_for(k).unwrap_or(""))
                )
            })
            .collect();
        println!(
            "{{\"profile\":\"{}\",\"path_mode\":\"{}\",\"complete\":{},\"present\":[{}],\"missing\":[{}],\"dns_hook\":\"{}\",\"acme_account_path\":\"{}\"}}",
            json_escape(profile),
            json_escape(path_mode),
            if incomplete { "false" } else { "true" },
            present_joined.join(","),
            missing_joined.join(","),
            json_escape(dns_status),
            json_escape(&opts.acme_account_path)
        );
    }

    if incomplete {
        if !opts.json {
            let _ = writeln!(
                notes,
                "host-material-inventory: incomplete: {} required kind(s) missing (and/or dns-hook). No secret values shown.",
                missing.len()
            );
        }
        return Err(ToolError::fail("inventory incomplete"));
    }
    if !opts.json {
        let _ = writeln!(
            notes,
            "host-material-inventory: complete: all required kinds present for profile={profile} path-mode={path_mode}"
        );
    }
    Ok(())
}

pub fn parse_inventory_args<I, S>(args: I) -> Result<InventoryParse, ToolError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let rest: Vec<String> = iter.map(|s| s.as_ref().to_string()).collect();
    let mut opts = InventoryOpts {
        staging: PathBuf::new(),
        host_id: std::env::var("SURMOUNT_SECRETS_HOST_ID")
            .ok()
            .filter(|s| !s.is_empty()),
        with_vaultwarden: false,
        acme_path: false,
        acme_account_path: PATH_ACME_ACCOUNT.to_string(),
        dns_hook_path: None,
        require_dns_hook: false,
        json: false,
    };
    if let Ok(s) = std::env::var("SURMOUNT_SECRETS_STAGING") {
        if !s.is_empty() {
            opts.staging = PathBuf::from(s);
        }
    }
    let mut i = 0usize;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => return Ok(InventoryParse::Help),
            "--staging" => {
                opts.staging = PathBuf::from(take(&rest, &mut i, "--staging")?);
            }
            "--host-id" => {
                opts.host_id = Some(take(&rest, &mut i, "--host-id")?);
            }
            "--with-vaultwarden" => {
                opts.with_vaultwarden = true;
                i += 1;
            }
            "--acme-path" => {
                opts.acme_path = true;
                i += 1;
            }
            "--acme-account-path" => {
                opts.acme_account_path = take(&rest, &mut i, "--acme-account-path")?;
            }
            "--dns-hook-path" => {
                opts.dns_hook_path = Some(take(&rest, &mut i, "--dns-hook-path")?);
            }
            "--require-dns-hook" => {
                opts.require_dns_hook = true;
                i += 1;
            }
            "--json" => {
                opts.json = true;
                i += 1;
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
    }
    if opts.staging.as_os_str().is_empty() {
        return Err(ToolError::fail(
            "staging required: --staging DIR or SURMOUNT_SECRETS_STAGING",
        ));
    }
    Ok(InventoryParse::Run(opts))
}

pub enum InventoryParse {
    Help,
    Run(InventoryOpts),
}

fn take(rest: &[String], i: &mut usize, flag: &str) -> Result<String, ToolError> {
    if *i + 1 >= rest.len() {
        return Err(ToolError::fail(format!("{flag} requires a value")));
    }
    let v = rest[*i + 1].clone();
    *i += 2;
    Ok(v)
}
