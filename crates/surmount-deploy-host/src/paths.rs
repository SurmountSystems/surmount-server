use std::env;
use std::path::{Component, Path, PathBuf};

/// Discover the public git work tree. Env wins (tests / operator).
pub fn discover_repo_root() -> PathBuf {
    if let Ok(p) = env::var("SURMOUNT_DEPLOY_REPO_ROOT") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(cwd) = env::current_dir() {
        if let Some(p) = walk_for_flake(&cwd) {
            return p;
        }
    }
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(p) = walk_for_flake(&manifest) {
        return p;
    }
    // Workspace crates dir when flake.nix is outside the crane src filter.
    manifest.parent().map(Path::to_path_buf).unwrap_or(manifest)
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

fn strip_trailing_slashes(p: &str) -> String {
    if p == "/" {
        return p.to_string();
    }
    p.trim_end_matches('/').to_string()
}

/// Return true if `path` is `root` or a descendant (boundary-aware).
pub fn is_path_under_root(root: &Path, path: &Path) -> bool {
    let r = strip_trailing_slashes(&realpath_m(root).to_string_lossy());
    let p = strip_trailing_slashes(&realpath_m(path).to_string_lossy());
    p == r || p.starts_with(&format!("{r}/"))
}

fn path_has_product_segment(p: &str) -> bool {
    let p = strip_trailing_slashes(p);
    matches!(p.as_str(), "hosts" | "secrets" | "hosts/" | "secrets/")
        || p.starts_with("hosts/")
        || p.starts_with("secrets/")
        || p == "hosts"
        || p == "secrets"
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

/// Control characters / empty tokens rejected (host-id, kinds).
pub fn assert_safe_token(label: &str, val: &str) -> Result<(), super::error::ToolError> {
    use super::error::ToolError;
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

pub fn env_flag_true(key: &str) -> bool {
    match env::var(key) {
        Ok(v) => matches!(
            v.as_str(),
            "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
        ),
        Err(_) => false,
    }
}

/// POSIX-ish quoting for dry-run argv (no secret bytes).
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if s.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '/' | '.' | '_' | '-' | '=' | ':' | '@' | '+' | ',' | '#')
    }) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_segment_hosts_and_secrets() {
        assert!(path_has_product_segment("/tmp/hosts/mail-vps"));
        assert!(path_has_product_segment(
            "/tmp/no-such-parent/hosts/mail-vps"
        ));
        assert!(path_has_product_segment("/tmp/no-such-parent/secrets/foo"));
        assert!(!path_has_product_segment("/tmp/private-host-local"));
    }

    #[test]
    fn under_root_boundary() {
        let root = Path::new("/tmp/repo");
        assert!(is_path_under_root(root, Path::new("/tmp/repo")));
        assert!(is_path_under_root(root, Path::new("/tmp/repo/secret")));
        assert!(!is_path_under_root(root, Path::new("/tmp/repo-other")));
    }
}
