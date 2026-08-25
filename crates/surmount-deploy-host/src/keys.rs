use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use crate::error::ToolError;

const KEY_TYPES: &[&str] = &[
    "ssh-ed25519",
    "ssh-rsa",
    "ecdsa-sha2-nistp256",
    "sk-ssh-ed25519@openssh.com",
    "sk-ecdsa-sha2-nistp256@openssh.com",
];

/// Count non-empty, non-comment lines that look like real SSH public keys.
pub fn count_authorized_key_lines(dir: &Path) -> Result<usize, ToolError> {
    let mut files: Vec<_> = Vec::new();
    let ak = dir.join("authorized_keys");
    if ak.is_file() {
        files.push(ak);
    }
    if let Ok(rd) = fs::read_dir(dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if !p.is_file() {
                continue;
            }
            let name = ent.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".pub") || name.starts_with("authorized_keys.") {
                files.push(p);
            }
        }
    }
    files.sort();
    files.dedup();
    let mut n = 0usize;
    for f in files {
        let fh = fs::File::open(&f)?;
        for line in BufReader::new(fh).lines() {
            let line = line?;
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if looks_like_real_key(line) {
                n += 1;
            }
        }
    }
    Ok(n)
}

fn looks_like_real_key(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let Some(ty) = parts.next() else {
        return false;
    };
    if !KEY_TYPES.contains(&ty) {
        return false;
    }
    let Some(blob) = parts.next() else {
        return false;
    };
    blob.len() >= 40
        && blob
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
}

/// Lockout check: at least one usable SSH public key. Soft notes on stderr.
pub fn check_host_local(dir: &Path, notes: &mut dyn std::io::Write) -> Result<(), ToolError> {
    if !dir.is_dir() {
        return Err(ToolError::fail(format!(
            "host-local dir missing or not a directory: {}",
            dir.display()
        )));
    }
    let keys = count_authorized_key_lines(dir)?;
    if keys < 1 {
        return Err(ToolError::fail(format!(
            "lockout risk: no usable SSH authorized key in host-local ({}). Add authorized_keys (or *.pub) with at least one real public key before switch with password auth off. See docs/deploy-host-local.md",
            dir.display()
        )));
    }
    let hw = dir.join("hardware-configuration.nix");
    let hw_ex = dir.join("hardware-configuration.nix.example");
    if !hw.is_file() && !hw_ex.is_file() {
        let _ = writeln!(
            notes,
            "deploy-host: note: no hardware-configuration.nix under host-local (ok if already on host)"
        );
    }
    let _ = writeln!(
        notes,
        "deploy-host: note: host-local key files present; flake path: rebuild auto-wires authorized_keys into evaluated config when using known-name layout (or host-local/default.nix that imports them). File presence alone is not enough / not sufficient if default.nix omits keys. See docs/deploy-host-local.md"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_fixture_counts_one_key() {
        let dir =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/deploy-host/good-host-local");
        assert_eq!(count_authorized_key_lines(&dir).unwrap(), 1);
    }

    #[test]
    fn empty_fixture_counts_zero() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/deploy-host/empty-keys-host-local");
        assert_eq!(count_authorized_key_lines(&dir).unwrap(), 0);
    }

    #[test]
    fn short_placeholder_counts_zero() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/deploy-host/short-placeholder-keys-host-local");
        assert_eq!(count_authorized_key_lines(&dir).unwrap(), 0);
    }
}
