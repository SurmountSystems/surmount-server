//! Eternal Terminal client argv. Reconnects after sleep/network change.
//! Mullvad stays operator-owned. For root@ targets, pass IdentityFile so
//! Host surmount-1 nixbuilder IdentitiesOnly still reaches root.

use std::env;
use std::path::{Path, PathBuf};

pub fn target_user_is_root(target: &str) -> bool {
    let t = target.trim();
    t == "root" || t.starts_with("root@")
}

pub fn operator_ssh_identity(target: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var("SURMOUNT_DEPLOY_SSH_IDENTITY") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if !target_user_is_root(target) {
        return None;
    }
    let home = env::var("HOME").ok()?;
    let id = PathBuf::from(home).join(".ssh/id_ed25519");
    if id.is_file() {
        Some(id)
    } else {
        None
    }
}

/// Build `et` argv. Does not spawn. Keepalives via et -k 30.
pub fn build_et_argv(
    et_bin: &str,
    target: &str,
    extra: &[String],
    identity: Option<&Path>,
) -> Vec<String> {
    let mut v = vec![
        et_bin.to_string(),
        "-k".to_string(),
        "30".to_string(),
        "-p".to_string(),
        "2022".to_string(),
    ];
    if let Some(id) = identity {
        v.push("--ssh-option".to_string());
        v.push(format!("IdentityFile={}", id.display()));
    }
    v.push(target.to_string());
    v.extend(extra.iter().cloned());
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn root_target_gets_identity_file() {
        let id = Path::new("/tmp/id_ed25519");
        let argv = build_et_argv("et", "root@example.test", &[], Some(id));
        let joined = argv.join(" ");
        assert!(joined.contains("--ssh-option"), "{joined}");
        assert!(joined.contains("IdentityFile=/tmp/id_ed25519"), "{joined}");
        assert!(joined.contains("-k 30"), "{joined}");
        assert!(joined.contains("root@example.test"), "{joined}");
    }

    #[test]
    fn nixbuilder_target_skips_forced_identity() {
        let argv = build_et_argv("et", "nixbuilder@example.test", &[], None);
        let joined = argv.join(" ");
        assert!(!joined.contains("IdentityFile="), "{joined}");
        assert!(joined.contains("nixbuilder@example.test"), "{joined}");
    }
}
