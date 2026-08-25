use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use surmount_deploy_host::{assert_safe_token, is_path_under_root, realpath_m};

use crate::error::ToolError;
use crate::inventory::{
    PATH_SESSION, PATH_STALWART, PATH_VW, expected_path_for, path_matches_conventional, read_attr,
    vw_admin_secret_ok,
};

pub fn rand_hex(nbytes: usize) -> Result<String, ToolError> {
    let mut buf = vec![0u8; nbytes];
    let mut f = fs::File::open("/dev/urandom")
        .map_err(|_| ToolError::fail("cannot generate random material (need /dev/urandom)"))?;
    use std::io::Read;
    f.read_exact(&mut buf)
        .map_err(|_| ToolError::fail("cannot read /dev/urandom"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

pub fn find_staging_kind_id(staging: &Path, want: &str) -> Option<String> {
    let rd = fs::read_dir(staging).ok()?;
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for item_dir in dirs {
        let attr = item_dir.join("attributes");
        if !attr.is_file() {
            continue;
        }
        if attr.symlink_metadata().ok()?.file_type().is_symlink() {
            continue;
        }
        if read_attr(&attr, "surmount.kind").as_deref() == Some(want) {
            return item_dir
                .file_name()
                .map(|s| s.to_string_lossy().into_owned());
        }
    }
    None
}

pub fn staging_item_inventory_ok(
    staging: &Path,
    item_id: &str,
    want_kind: &str,
    host_id: Option<&str>,
) -> Result<(), String> {
    let item_dir = staging.join(item_id);
    let attr = item_dir.join("attributes");
    let secret = item_dir.join("secret");
    if attr
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("symlink-attributes".into());
    }
    if secret
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("symlink-secret".into());
    }
    if !attr.is_file() || !secret.is_file() {
        return Err("missing-files".into());
    }
    if secret.metadata().map(|m| m.len() == 0).unwrap_or(true) {
        return Err("empty-secret".into());
    }
    let kind = read_attr(&attr, "surmount.kind").unwrap_or_default();
    if kind != want_kind {
        return Err("kind-mismatch".into());
    }
    let host = read_attr(&attr, "surmount.host").unwrap_or_default();
    if host.is_empty() {
        return Err("missing-host".into());
    }
    if let Some(want) = host_id {
        if host != want {
            return Err("host-mismatch".into());
        }
    }
    let hpath = read_attr(&attr, "surmount.path").unwrap_or_default();
    if hpath.is_empty() {
        return Err("missing-path".into());
    }
    if expected_path_for(want_kind).is_some() && !path_matches_conventional(want_kind, &hpath) {
        return Err("path-mismatch".into());
    }
    if want_kind == "vaultwarden-admin" && !vw_admin_secret_ok(&secret) {
        return Err("missing-admin-token-line".into());
    }
    Ok(())
}

pub fn make_staging_item(
    staging: &Path,
    id: &str,
    kind: &str,
    host_id: &str,
    hpath: &str,
    payload: &str,
) -> Result<(), ToolError> {
    let dir = staging.join(id);
    fs::create_dir_all(&dir)?;
    let mut perm = fs::metadata(&dir)?.permissions();
    perm.set_mode(0o700);
    fs::set_permissions(&dir, perm)?;
    if dir.symlink_metadata()?.file_type().is_symlink() {
        return Err(ToolError::fail(format!(
            "refuse: staging item directory is a symlink: {}",
            dir.display()
        )));
    }
    for name in ["attributes", "secret"] {
        let p = dir.join(name);
        if p.symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err(ToolError::fail(format!(
                "refuse: staging item attributes/secret is a symlink under {}",
                dir.display()
            )));
        }
    }
    let attr_body =
        format!("surmount.kind={kind}\nsurmount.host={host_id}\nsurmount.path={hpath}\n");
    atomic_write(&dir.join("attributes"), attr_body.as_bytes())?;
    atomic_write(&dir.join("secret"), payload.as_bytes())?;
    eprintln!(
        "host-cutover: generated staging item id={id} kind={kind} path={hpath} (value not logged)"
    );
    Ok(())
}

fn atomic_write(dest: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    let parent = dest.parent().unwrap_or(Path::new("."));
    let tmp = parent.join(format!(
        ".tmp-{}",
        dest.file_name().unwrap().to_string_lossy()
    ));
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    let mut perm = fs::metadata(&tmp)?.permissions();
    perm.set_mode(0o600);
    fs::set_permissions(&tmp, perm)?;
    if tmp.symlink_metadata()?.file_type().is_symlink() {
        let _ = fs::remove_file(&tmp);
        return Err(ToolError::fail("refuse: mktemp produced a symlink"));
    }
    fs::rename(&tmp, dest)?;
    if dest.symlink_metadata()?.file_type().is_symlink() {
        return Err(ToolError::fail(format!(
            "refuse: postcondition became a symlink: {}",
            dest.display()
        )));
    }
    Ok(())
}

pub fn generate_missing_material(
    staging: &Path,
    host_id: &str,
    with_vw: bool,
    acme_path: bool,
    repo_root: &Path,
) -> Result<(), ToolError> {
    assert_safe_token("host-id", host_id)?;
    if is_path_under_root(repo_root, &realpath_m(staging)) {
        return Err(ToolError::fail(format!(
            "refuse: staging must be outside the public git work tree for generate-material ({})",
            staging.display()
        )));
    }
    fs::create_dir_all(staging)?;
    let mut perm = fs::metadata(staging)?.permissions();
    perm.set_mode(0o700);
    fs::set_permissions(staging, perm)?;
    if staging.symlink_metadata()?.file_type().is_symlink() {
        return Err(ToolError::fail(format!(
            "refuse: staging path is a symlink: {}",
            staging.display()
        )));
    }
    generate_or_check_kind(
        staging,
        "session-secret",
        "session-secret",
        PATH_SESSION,
        &rand_hex(32)?,
        host_id,
    )?;
    generate_or_check_kind(
        staging,
        "stalwart-token",
        "stalwart-token",
        PATH_STALWART,
        &rand_hex(24)?,
        host_id,
    )?;
    if with_vw {
        let payload = format!("ADMIN_TOKEN={}", rand_hex(32)?);
        generate_or_check_kind(
            staging,
            "vaultwarden-admin",
            "vaultwarden-admin",
            PATH_VW,
            &payload,
            host_id,
        )?;
    }
    if !acme_path {
        eprintln!(
            "host-cutover: generate-material: PEMs not generated (operator-supplied or ACME path). Use --acme-path or place tls-cert/tls-key in staging."
        );
    }
    Ok(())
}

fn generate_or_check_kind(
    staging: &Path,
    kind: &str,
    default_id: &str,
    host_path: &str,
    payload: &str,
    host_id: &str,
) -> Result<(), ToolError> {
    match find_staging_kind_id(staging, kind) {
        None => make_staging_item(staging, default_id, kind, host_id, host_path, payload),
        Some(id) => match staging_item_inventory_ok(staging, &id, kind, Some(host_id)) {
            Ok(()) => {
                eprintln!(
                    "host-cutover: generate-material: {kind} already present and inventory-accept (skip id={id})"
                );
                Ok(())
            }
            Err(reason) => Err(ToolError::fail(format!(
                "generate-material: kind={kind} item id={id} exists but fails inventory (reason={reason}). path/host/secret must match secrets-install accept rules (conventional path, non-empty host matching --host-id, non-empty regular secret; vaultwarden-admin needs ADMIN_TOKEN=<non-empty>). Remove or fix staging/{id}/ then re-run. Secret values not logged."
            ))),
        },
    }
}
