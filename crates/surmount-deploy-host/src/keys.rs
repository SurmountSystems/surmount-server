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
    // Known-name layout (no default.nix) auto-wires authorized_keys.
    // A custom default.nix that already sets authorizedKeys is enough.
    // Warn only when that entry module exists and does not mention keys.
    if entry_module_omits_keys(dir) {
        let _ = writeln!(
            notes,
            "deploy-host: note: host-local default.nix does not mention authorizedKeys. File presence alone is not enough. Wire users.users.root.openssh.authorizedKeys in that file or drop default.nix so the flake auto-wires host-local/authorized_keys. See docs/deploy-host-local.md"
        );
    }
    Ok(())
}

/// True when default.nix or host-local.nix exists and never mentions SSH keys.
fn entry_module_omits_keys(dir: &Path) -> bool {
    for name in ["default.nix", "host-local.nix"] {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            return true;
        };
        let mentions = text.contains("authorizedKeys") || text.contains("authorized_keys");
        return !mentions;
    }
    false
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
    fn wired_default_nix_does_not_warn() {
        let dir = std::env::temp_dir().join(format!("surmount-keys-wired-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("authorized_keys"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILEpaPf6ltFz4UT0qajfxQojuKuVF8FVE4Kj5U54k+gM test\n",
        )
        .unwrap();
        fs::write(
            dir.join("default.nix"),
            "{ users.users.root.openssh.authorizedKeys.keys = [ \"ssh-ed25519 AAAA\" ]; }\n",
        )
        .unwrap();
        let mut notes = Vec::new();
        check_host_local(&dir, &mut notes).unwrap();
        let text = String::from_utf8(notes).unwrap();
        assert!(!text.contains("does not mention authorizedKeys"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_nix_without_keys_warns() {
        let dir = std::env::temp_dir().join(format!("surmount-keys-omit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("authorized_keys"),
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILEpaPf6ltFz4UT0qajfxQojuKuVF8FVE4Kj5U54k+gM test\n",
        )
        .unwrap();
        fs::write(
            dir.join("default.nix"),
            "{ networking.hostName = \"x\"; }\n",
        )
        .unwrap();
        let mut notes = Vec::new();
        check_host_local(&dir, &mut notes).unwrap();
        let text = String::from_utf8(notes).unwrap();
        assert!(text.contains("does not mention authorizedKeys"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn short_placeholder_counts_zero() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/deploy-host/short-placeholder-keys-host-local");
        assert_eq!(count_authorized_key_lines(&dir).unwrap(), 0);
    }
}
