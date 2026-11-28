//! Resolve programs the recipes spawn.
//!
//! `nix run` replaces PATH before the recipe starts, so `Command::new("nix")`
//! returns "No such file or directory (os error 2)". The lookup uses the
//! parent PATH, not a PATH set on the child. The vendor Determinate Nix
//! binary is an absolute path. `/run/current-system/sw/bin` is where git,
//! nice, and ionice live. It is not the builder when Determinate Nix is
//! present.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::paths::{discover_repo_root, shell_quote};

pub const SYSTEM_BIN: &str = "/run/current-system/sw/bin";

const LD_LIBRARY_PATH_FILE: &str = "mail-vps-no-go-ld-library-path.txt";

pub(crate) fn resolve_tool(name: &str) -> PathBuf {
    if name.contains('/') {
        return PathBuf::from(name);
    }
    if name == "nix" {
        if let Some(nix) = determinate_nix_executable() {
            return nix;
        }
    }
    if let Some(found) = which_on_path(name) {
        return found;
    }
    let system = Path::new(SYSTEM_BIN).join(name);
    if is_exec_file(&system) {
        return system;
    }
    PathBuf::from(name)
}

pub(crate) fn child_path() -> std::ffi::OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(nix) = determinate_nix_executable() {
        if let Some(dir) = nix.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            if !dirs.iter().any(|have| have == &dir) {
                dirs.push(dir);
            }
        }
    }
    let system = PathBuf::from(SYSTEM_BIN);
    if !dirs.iter().any(|have| have == &system) {
        dirs.push(system);
    }
    env::join_paths(&dirs).unwrap_or_else(|_| std::ffi::OsString::from(SYSTEM_BIN))
}

/// PATH and the sentry library path for the remote build script.
/// Empty when this checkout has no Determinate Nix binary. The library
/// file is read on the target. This string does not contain its bytes.
pub(crate) fn remote_tool_path_prefix() -> String {
    let Some(nix) = determinate_nix_executable() else {
        return String::new();
    };
    let Some(bin_dir) = nix.parent() else {
        return String::new();
    };
    let ld = discover_repo_root().join(LD_LIBRARY_PATH_FILE);
    remote_tool_setup(bin_dir, &ld)
}

pub(crate) fn remote_tool_setup(nix_bin_dir: &Path, ld_file: &Path) -> String {
    let bin_q = shell_quote(&nix_bin_dir.to_string_lossy());
    let ld_q = shell_quote(&ld_file.to_string_lossy());
    format!(
        "export PATH={bin_q}:{SYSTEM_BIN}${{PATH:+:$PATH}}\nif [ -f {ld_q} ]; then LD_LIBRARY_PATH=$(tr -d '\\n' < {ld_q}); export LD_LIBRARY_PATH; fi\n"
    )
}

pub(crate) fn apply_nix_library_path(cmd: &mut Command, program: &Path) {
    if !program_needs_sentry(program) {
        return;
    }
    let Some(value) = ld_library_path_from_repo() else {
        return;
    };
    cmd.env("LD_LIBRARY_PATH", value);
}

fn program_needs_sentry(program: &Path) -> bool {
    if program.file_name().and_then(|name| name.to_str()) != Some("nix") {
        return false;
    }
    let Some(store_dir) = program.parent().and_then(|bin| bin.parent()) else {
        return false;
    };
    let Some(name) = store_dir.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    nix_cli_version(name).is_some()
}

fn ld_library_path_from_repo() -> Option<std::ffi::OsString> {
    let path = discover_repo_root().join(LD_LIBRARY_PATH_FILE);
    let text = fs::read_to_string(path).ok()?;
    let value: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
    if value.is_empty() {
        None
    } else {
        Some(std::ffi::OsString::from(value))
    }
}

pub(crate) fn determinate_nix_executable() -> Option<PathBuf> {
    let store = discover_repo_root()
        .join("vendor")
        .join("nix-store")
        .join("nix")
        .join("store");
    determinate_nix_in_store(&store)
}

pub(crate) fn determinate_nix_in_store(store: &Path) -> Option<PathBuf> {
    let rd = fs::read_dir(store).ok()?;
    let mut best: Option<(Vec<u64>, PathBuf)> = None;
    for ent in rd.flatten() {
        let name = ent.file_name();
        let name = name.to_string_lossy();
        let Some(version) = nix_cli_version(&name) else {
            continue;
        };
        let nix = ent.path().join("bin").join("nix");
        if !is_exec_file(&nix) {
            continue;
        }
        let key = version_key(version);
        let replace = match &best {
            None => true,
            Some((have, _)) => key > *have,
        };
        if replace {
            best = Some((key, nix));
        }
    }
    best.map(|(_, nix)| nix)
}

/// `hash-determinate-nix-3.24.2` only. Debug, dev, and split outputs
/// (`determinate-nix-flake-3.24.2`, `determinate-nix-3.24.2-debug`) are
/// not the client.
fn nix_cli_version(dir_name: &str) -> Option<&str> {
    let dash = dir_name.find('-')?;
    let rest = &dir_name[dash + 1..];
    let version = rest.strip_prefix("determinate-nix-")?;
    if version.is_empty() {
        return None;
    }
    if !version.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    if version.starts_with('.') || version.ends_with('.') || version.contains("..") {
        return None;
    }
    Some(version)
}

fn version_key(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn which_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        let cand = dir.join(name);
        if is_exec_file(&cand) {
            return Some(cand);
        }
    }
    None
}

fn is_exec_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_exec(dir: &Path) {
        let bin = dir.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let nix = bin.join("nix");
        fs::write(&nix, b"nix").unwrap();
        let mut perms = fs::metadata(&nix).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&nix, perms).unwrap();
    }

    #[test]
    fn cli_package_wins_over_debug_and_split_outputs() {
        let root = env::temp_dir().join(format!(
            "surmount-tool-path-{}-{}",
            std::process::id(),
            "cli"
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let older = root.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-determinate-nix-3.11.1");
        let debug = root.join("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-determinate-nix-3.24.2-debug");
        let flake = root.join("cccccccccccccccccccccccccccccccc-determinate-nix-flake-3.24.2");
        let cli = root.join("8aks5953ia2xq173x9mbplnahbzlmxdn-determinate-nix-3.24.2");
        for dir in [&older, &debug, &flake, &cli] {
            write_exec(dir);
        }
        let got = determinate_nix_in_store(&root).unwrap();
        assert_eq!(got, cli.join("bin").join("nix"), "{got:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_store_is_none() {
        let root =
            env::temp_dir().join(format!("surmount-tool-path-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        assert!(determinate_nix_in_store(&root).is_none());
    }

    #[test]
    fn slash_path_is_not_rewritten() {
        let path = "/tmp/does-not-need-to-exist/nix";
        assert_eq!(resolve_tool(path), PathBuf::from(path));
    }

    #[test]
    fn remote_setup_puts_determinate_bin_before_system_bin() {
        let text = remote_tool_setup(
            Path::new(
                "/mnt/hdd/projects/server/vendor/nix-store/nix/store/8aks5953ia2xq173x9mbplnahbzlmxdn-determinate-nix-3.24.2/bin",
            ),
            Path::new("/mnt/hdd/projects/server/mail-vps-no-go-ld-library-path.txt"),
        );
        let det = text
            .find("determinate-nix-3.24.2/bin")
            .expect("determinate bin");
        let system = text.find(SYSTEM_BIN).expect("system bin");
        assert!(det < system, "{text}");
        assert!(text.contains("tr -d '\\n'"), "{text}");
        assert!(!text.contains("nix build"), "{text}");
        assert!(!text.contains("echo "), "{text}");
        assert!(!text.contains("python"), "{text}");
    }

    #[test]
    fn system_nix_does_not_need_the_sentry_file() {
        assert!(!program_needs_sentry(Path::new(
            "/run/current-system/sw/bin/nix"
        )));
        assert!(program_needs_sentry(Path::new(
            "/mnt/hdd/projects/server/vendor/nix-store/nix/store/8aks5953ia2xq173x9mbplnahbzlmxdn-determinate-nix-3.24.2/bin/nix"
        )));
    }
}
