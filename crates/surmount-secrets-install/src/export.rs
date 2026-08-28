use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Result, ToolError};
use crate::kinds::{is_known_export_kind, read_attr};
use crate::paths::{
    assert_safe_token, discover_repo_root, host_material_root_for, is_path_under_root, is_symlink,
    path_has_dot_segments, set_mode,
};

const USAGE: &str = "\
Usage:
  secrets-export-bw-to-staging --help
  secrets-export-bw-to-staging --map DIR --staging DIR --host-id ID \\
    --from-fixture FIXTURE_DIR [options]
  secrets-export-bw-to-staging --map DIR --staging DIR --host-id ID \\
    --from-bw [options]

Map Vaultwarden / Bitwarden CLI items into a private secrets-install staging
tree. Domain C human vault -> domain A staging only. Does NOT install to the
host and does NOT feed nixos-rebuild. Not Bitwarden Secrets Manager.

Required:
  --map DIR              Map tree: <id>/attributes with surmount.* keys
  --staging DIR          Output staging root (created if missing).
  --host-id ID           Logical host id; must match surmount.host

Source (exactly one):
  --from-fixture DIR     Read DIR/<item-id>/secret as the raw field value
  --from-bw              Use `bw get password|notes <bw_item>`

Options:
  --dry-run              Plan only (kinds, paths, bw labels); no write
  --allow-host-fill      If map omits surmount.host, write --host-id
  --bw-bin PATH          Override bw binary (default: bw on PATH)
  -h, --help             Show this help.
";

fn log_line(msg: &str) {
    eprintln!("secrets-export-bw-to-staging: {msg}");
}

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut map = String::new();
    let mut staging = env::var("SURMOUNT_SECRETS_STAGING").unwrap_or_default();
    let mut host_id = env::var("SURMOUNT_SECRETS_HOST_ID").unwrap_or_default();
    let mut from_fixture = String::new();
    let mut from_bw = false;
    let mut dry_run = false;
    let mut allow_host_fill = false;
    let mut bw_bin = env::var("SURMOUNT_BW_BIN").unwrap_or_else(|_| "bw".into());

    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(());
            }
            "--map" => {
                i += 1;
                map = need(&argv, i, "--map")?;
            }
            "--staging" => {
                i += 1;
                staging = need(&argv, i, "--staging")?;
            }
            "--host-id" => {
                i += 1;
                host_id = need(&argv, i, "--host-id")?;
            }
            "--from-fixture" => {
                i += 1;
                from_fixture = need(&argv, i, "--from-fixture")?;
            }
            "--from-bw" => from_bw = true,
            "--dry-run" => dry_run = true,
            "--allow-host-fill" => allow_host_fill = true,
            "--bw-bin" => {
                i += 1;
                bw_bin = need(&argv, i, "--bw-bin")?;
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
        i += 1;
    }

    if map.is_empty() {
        return Err(ToolError::fail("--map is required".to_string()));
    }
    if staging.is_empty() {
        return Err(ToolError::fail(
            "--staging is required (or SURMOUNT_SECRETS_STAGING)".to_string(),
        ));
    }
    if host_id.is_empty() {
        return Err(ToolError::fail(
            "--host-id is required (or SURMOUNT_SECRETS_HOST_ID)".to_string(),
        ));
    }
    assert_safe_token("host-id", &host_id)?;
    if !from_fixture.is_empty() && from_bw {
        return Err(ToolError::fail(
            "use exactly one of --from-fixture or --from-bw".to_string(),
        ));
    }
    if from_fixture.is_empty() && !from_bw {
        return Err(ToolError::fail(
            "exactly one source required: --from-fixture DIR or --from-bw".to_string(),
        ));
    }

    let map_path = PathBuf::from(&map);
    if !map_path.is_dir() {
        return Err(ToolError::fail(format!("map directory missing: {map}")));
    }
    if !from_fixture.is_empty() && !Path::new(&from_fixture).is_dir() {
        return Err(ToolError::fail(format!(
            "fixture directory missing: {from_fixture}"
        )));
    }
    if from_bw {
        let ok = Path::new(&bw_bin).is_file()
            || Command::new("command")
                .args(["-v", &bw_bin])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            || which_exists(&bw_bin);
        if !ok {
            return Err(ToolError::fail(format!(
                "bw binary not found: {bw_bin} (install official Bitwarden CLI; unlock session first)"
            )));
        }
    }

    let repo = discover_repo_root();
    if is_path_under_root(&repo, Path::new(&staging)) {
        return Err(ToolError::fail(format!(
            "staging must stay outside the public git work tree: {staging}"
        )));
    }

    if !dry_run {
        fs::create_dir_all(&staging)?;
        let _ = set_mode(Path::new(&staging), 0o700);
    }

    let mut planned = 0usize;
    let mut wrote = 0usize;
    let mut entries: Vec<PathBuf> = fs::read_dir(&map_path)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for item_dir in entries {
        let item_id = item_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert_safe_token("item-id", &item_id)?;
        let attr_file = item_dir.join("attributes");
        if !attr_file.is_file() {
            return Err(ToolError::fail(format!(
                "map item {item_id}: missing attributes"
            )));
        }
        let body = fs::read_to_string(&attr_file)?;
        let kind = read_attr(&body, "surmount.kind").unwrap_or_default();
        let mut host = read_attr(&body, "surmount.host").unwrap_or_default();
        let path = read_attr(&body, "surmount.path").unwrap_or_default();
        let bw_item = read_attr(&body, "surmount.bw_item").unwrap_or_default();
        let bw_field = read_attr(&body, "surmount.bw_field").unwrap_or_else(|| "password".into());
        let bw_wrap = read_attr(&body, "surmount.bw_wrap").unwrap_or_default();

        if kind.is_empty() {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.kind required"
            )));
        }
        if !is_known_export_kind(&kind) {
            return Err(ToolError::fail(format!(
                "map item {item_id}: unknown kind {kind}"
            )));
        }
        assert_safe_token("kind", &kind)?;
        if host.is_empty() {
            if allow_host_fill {
                host = host_id.clone();
            } else {
                return Err(ToolError::fail(format!(
                    "map item {item_id}: surmount.host required (or pass --allow-host-fill)"
                )));
            }
        }
        assert_safe_token("host", &host)?;
        if host != host_id {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.host={host} does not match --host-id {host_id}"
            )));
        }
        if path.is_empty() {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.path required"
            )));
        }
        assert_safe_token("path", &path)?;
        if !path.starts_with('/') {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.path must be absolute"
            )));
        }
        if path_has_dot_segments(&path) || path.contains("..") {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.path must not contain .. segments"
            )));
        }
        if host_material_root_for(&path).is_none() {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.path must be under /var/lib/surmount/secrets, /run/surmount-secrets, or /var/lib/sops-nix"
            )));
        }
        if bw_item.is_empty() {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.bw_item required"
            )));
        }
        assert_safe_token("bw_item", &bw_item)?;
        if bw_field != "password" && bw_field != "notes" {
            return Err(ToolError::fail(format!(
                "map item {item_id}: surmount.bw_field must be password or notes (got {bw_field})"
            )));
        }
        if !bw_wrap.is_empty() {
            assert_safe_token("bw_wrap", &bw_wrap)?;
            if !is_simple_env_key(&bw_wrap) {
                return Err(ToolError::fail(format!(
                    "map item {item_id}: surmount.bw_wrap must be a simple env key"
                )));
            }
        }

        planned += 1;
        log_line(&format!(
            "plan item={item_id} kind={kind} path={path} bw_field={bw_field} bw_item=<label>"
        ));
        if dry_run {
            continue;
        }

        let payload = fetch_field(
            &from_fixture,
            from_bw,
            &bw_bin,
            &item_id,
            &bw_item,
            &bw_field,
        )?;
        if payload.is_empty() {
            return Err(ToolError::fail(format!(
                "map item {item_id}: empty secret payload after fetch"
            )));
        }
        let mut payload = payload;
        if !bw_wrap.is_empty() {
            if payload.contains('\n') {
                return Err(ToolError::fail(format!(
                    "map item {item_id}: cannot bw_wrap multi-line value (use notes field without wrap)"
                )));
            }
            payload = format!("{bw_wrap}={payload}");
        }

        let out_dir = Path::new(&staging).join(&item_id);
        fs::create_dir_all(&out_dir)?;
        let _ = set_mode(&out_dir, 0o700);
        let attrs = format!("surmount.kind={kind}\nsurmount.host={host}\nsurmount.path={path}\n");
        write_0600(&out_dir.join("attributes"), attrs.as_bytes())?;
        write_0600(&out_dir.join("secret"), payload.as_bytes())?;
        if is_symlink(&out_dir.join("secret")) {
            let _ = fs::remove_file(out_dir.join("secret"));
            return Err(ToolError::fail(format!(
                "refused symlink secret output for {item_id}"
            )));
        }
        wrote += 1;
        log_line(&format!("wrote item={item_id} kind={kind} path={path}"));
        let _ = kind;
    }

    if planned == 0 {
        return Err(ToolError::fail(
            "map has no item subdirectories with attributes".to_string(),
        ));
    }
    if dry_run {
        log_line(&format!("dry-run ok: planned={planned} (no files written)"));
    } else {
        log_line(&format!("export ok: wrote={wrote} staging={staging}"));
    }
    Ok(())
}

fn need(argv: &[String], i: usize, flag: &str) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail(format!("{flag} requires a value")))
}

fn is_simple_env_key(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn which_exists(bin: &str) -> bool {
    if Path::new(bin).is_file() {
        return true;
    }
    if let Ok(path) = env::var("PATH") {
        for dir in path.split(':') {
            let p = Path::new(dir).join(bin);
            if p.is_file() {
                return true;
            }
        }
    }
    false
}

fn fetch_field(
    fixture: &str,
    from_bw: bool,
    bw_bin: &str,
    item_id: &str,
    bw_item: &str,
    field: &str,
) -> Result<String> {
    if !fixture.is_empty() {
        let fpath = Path::new(fixture).join(item_id).join("secret");
        if !fpath.is_file() {
            return Err(ToolError::fail(format!(
                "fixture secret missing for item {item_id}: {}",
                fpath.display()
            )));
        }
        if is_symlink(&fpath) {
            return Err(ToolError::fail(format!(
                "fixture secret must be a regular file (symlink refused): {item_id}"
            )));
        }
        let mut buf = Vec::new();
        File::open(&fpath)?.read_to_end(&mut buf)?;
        return Ok(String::from_utf8_lossy(&buf).into_owned());
    }
    if from_bw {
        let out = Command::new(bw_bin)
            .args(["get", field, bw_item])
            .output()
            .map_err(|_| {
                ToolError::fail(format!(
                    "bw get {field} failed for item label (id={item_id}); unlock session? item present?"
                ))
            })?;
        if !out.status.success() {
            return Err(ToolError::fail(format!(
                "bw get {field} failed for item label (id={item_id}); unlock session? item present?"
            )));
        }
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    Err(ToolError::fail("internal: no source".to_string()))
}

fn write_0600(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true).mode(0o600);
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    set_mode(path, 0o600)?;
    Ok(())
}
