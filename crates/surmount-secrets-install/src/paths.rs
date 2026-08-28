use std::env;
use std::path::{Component, Path, PathBuf};

use crate::error::{Result, ToolError};

pub const DURABLE_MATERIAL: &str = "/var/lib/surmount/secrets";
pub const EPHEMERAL_MATERIAL: &str = "/run/surmount-secrets";
pub const SOPS_MATERIAL: &str = "/var/lib/sops-nix";
pub const PRODUCT_STATE: &str = "/var/lib/surmount";

/// Discover the public git work tree. `SURMOUNT_REPO_ROOT` wins (tests).
pub fn discover_repo_root() -> PathBuf {
    if let Ok(p) = env::var("SURMOUNT_REPO_ROOT")
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    if let Ok(cwd) = env::current_dir()
        && let Some(p) = walk_for_flake(&cwd)
    {
        return p;
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(p) = walk_for_flake(&manifest) {
        return p;
    }
    manifest
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or(manifest)
}

fn walk_for_flake(start: &Path) -> Option<PathBuf> {
    for anc in start.ancestors() {
        if anc.join("flake.nix").is_file() {
            return Some(anc.to_path_buf());
        }
    }
    None
}

/// `realpath -m` shape: absolute, `.`/`..` collapsed, missing parents OK.
pub fn realpath_m(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map(|c| c.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    normalize_components(&abs)
}

fn normalize_components(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::RootDir => out.push("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s),
            Component::Prefix(p) => out.push(p.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from("/")
    } else {
        out
    }
}

pub fn strip_trailing_slashes(p: &str) -> String {
    if p == "/" {
        return p.to_string();
    }
    p.trim_end_matches('/').to_string()
}

pub fn is_path_under_root(root: &Path, path: &Path) -> bool {
    let r = strip_trailing_slashes(&realpath_m(root).to_string_lossy());
    let p = strip_trailing_slashes(&realpath_m(path).to_string_lossy());
    p == r || p.starts_with(&format!("{r}/"))
}

pub fn path_has_dot_segments(path: &str) -> bool {
    for part in path.split('/') {
        if part == "." || part == ".." {
            return true;
        }
    }
    false
}

pub fn assert_safe_token(label: &str, val: &str) -> Result<()> {
    if val.is_empty() {
        return Err(ToolError::fail(format!("empty {label}")));
    }
    if val.chars().any(|c| c.is_control()) {
        return Err(ToolError::fail(format!(
            "{label} must not contain control characters"
        )));
    }
    Ok(())
}

pub fn is_durable_material_path_shape(p: &str) -> bool {
    let p = strip_trailing_slashes(p);
    p == DURABLE_MATERIAL
        || p.starts_with(&format!("{DURABLE_MATERIAL}/"))
        || p.ends_with(DURABLE_MATERIAL)
        || p.contains(&format!("{DURABLE_MATERIAL}/"))
}

fn path_has_product_segment(p: &str) -> bool {
    if is_durable_material_path_shape(p) {
        return false;
    }
    let p = strip_trailing_slashes(p);
    p == "hosts"
        || p == "secrets"
        || p.starts_with("hosts/")
        || p.starts_with("secrets/")
        || p.ends_with("/hosts")
        || p.ends_with("/secrets")
        || p.contains("/hosts/")
        || p.contains("/secrets/")
}

/// True when dest is (or resolves into) public `hosts/` or `secrets/`.
pub fn is_public_product_path(dest: &Path, repo_root: &Path) -> bool {
    let mut candidates: Vec<PathBuf> = vec![dest.to_path_buf()];
    candidates.push(realpath_m(dest));
    if dest.exists() || dest.symlink_metadata().is_ok() {
        if let Ok(c) = dest.canonicalize() {
            candidates.push(c);
        }
    }
    for c in &candidates {
        if path_has_product_segment(&c.to_string_lossy()) {
            return true;
        }
    }
    let repo = realpath_m(repo_root);
    let hosts = repo.join("hosts");
    let secrets = repo.join("secrets");
    for c in &candidates {
        let p = realpath_m(c);
        if is_path_under_root(&hosts, &p) || is_path_under_root(&secrets, &p) {
            return true;
        }
    }
    false
}

pub fn host_material_root_for(p: &str) -> Option<&'static str> {
    let p = strip_trailing_slashes(p);
    if p == DURABLE_MATERIAL || p.starts_with(&format!("{DURABLE_MATERIAL}/")) {
        Some(DURABLE_MATERIAL)
    } else if p == EPHEMERAL_MATERIAL || p.starts_with(&format!("{EPHEMERAL_MATERIAL}/")) {
        Some(EPHEMERAL_MATERIAL)
    } else if p == SOPS_MATERIAL || p.starts_with(&format!("{SOPS_MATERIAL}/")) {
        Some(SOPS_MATERIAL)
    } else {
        None
    }
}

pub fn material_root_create_mode(root: &str) -> u32 {
    match root {
        DURABLE_MATERIAL | EPHEMERAL_MATERIAL => 0o755,
        _ => 0o700,
    }
}

/// Absolute host path: no option shape, no `..`, under material roots.
pub fn normalize_host_path(p: &str, repo_root: &Path) -> Result<String> {
    assert_safe_token("surmount.path", p)?;
    if !p.starts_with('/') {
        return Err(ToolError::fail(format!(
            "surmount.path must be absolute (got {p})"
        )));
    }
    if p.starts_with('-') {
        return Err(ToolError::fail(format!(
            "surmount.path must not start with '-' (got {p})"
        )));
    }
    if path_has_dot_segments(p) {
        return Err(ToolError::fail(format!(
            "refuse: surmount.path must not contain '.' or '..' segments (got {p})"
        )));
    }
    let canon = realpath_m(Path::new(p));
    let canon_s = canon.to_string_lossy().into_owned();
    if path_has_dot_segments(&canon_s) {
        return Err(ToolError::fail(format!(
            "refuse: surmount.path normalizes with '.' or '..' (got {p} -> {canon_s})"
        )));
    }
    if is_public_product_path(Path::new(p), repo_root) || is_public_product_path(&canon, repo_root)
    {
        return Err(ToolError::fail(format!(
            "refuse: will not install into public product path ({p}). Host runtime paths only. Never hosts/ or secrets/ in the git tree."
        )));
    }
    if host_material_root_for(&canon_s).is_none() {
        return Err(ToolError::fail(format!(
            "refuse: surmount.path must be under deploy material roots {DURABLE_MATERIAL} (durable), {EPHEMERAL_MATERIAL} (ephemeral), or {SOPS_MATERIAL} (got {canon_s})"
        )));
    }
    Ok(canon_s)
}

pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(unix)]
pub fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
pub fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
pub fn mkdir_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().mode(mode).create(path)
}

#[cfg(not(unix))]
pub fn mkdir_mode(path: &Path, _mode: u32) -> std::io::Result<()> {
    std::fs::create_dir(path)
}

pub fn is_symlink(path: &Path) -> bool {
    path.symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}
