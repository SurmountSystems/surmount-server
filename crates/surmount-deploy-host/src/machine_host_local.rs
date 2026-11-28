//! Directory passed as `--host-local` when a recipe starts on this machine.
//!
//! The same binary runs on horizon and on surmount-1. Horizon keeps
//! `/home/hunter/.local/share/surmount/host-local`. A process whose
//! hostname is surmount-1 uses `/root/surmount-server/host-local`.
//! The flag name stays `--host-local`. That name is not a hostname.

use std::fs;
use std::path::Path;

pub(crate) const HORIZON_HOST_LOCAL: &str = "/home/hunter/.local/share/surmount/host-local";
pub(crate) const SURMOUNT_1_HOST_LOCAL: &str = "/root/surmount-server/host-local";

pub(crate) fn host_local_dir_for_this_machine() -> &'static str {
    let hostname = read_hostname().unwrap_or_default();
    select_host_local_dir(
        &hostname,
        Path::new(SURMOUNT_1_HOST_LOCAL).is_dir(),
        Path::new(HORIZON_HOST_LOCAL).is_dir(),
    )
}

pub(crate) fn select_host_local_dir(
    hostname: &str,
    surmount_1_dir_exists: bool,
    horizon_dir_exists: bool,
) -> &'static str {
    let name = hostname.trim();
    let on_surmount_1 = name == "surmount-1" || (surmount_1_dir_exists && !horizon_dir_exists);
    if on_surmount_1 {
        SURMOUNT_1_HOST_LOCAL
    } else {
        HORIZON_HOST_LOCAL
    }
}

fn read_hostname() -> Option<String> {
    let text = fs::read_to_string("/etc/hostname").ok()?;
    let name = text.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surmount_1_hostname_uses_the_checkout_overlay() {
        assert_eq!(
            select_host_local_dir("surmount-1", false, false),
            "/root/surmount-server/host-local"
        );
        assert_eq!(
            select_host_local_dir("surmount-1\n", true, true),
            "/root/surmount-server/host-local"
        );
    }

    #[test]
    fn horizon_keeps_its_directory() {
        assert_eq!(
            select_host_local_dir("horizon", false, true),
            "/home/hunter/.local/share/surmount/host-local"
        );
        assert_eq!(
            select_host_local_dir("", false, true),
            "/home/hunter/.local/share/surmount/host-local"
        );
    }

    #[test]
    fn missing_hostname_uses_the_mail_host_dir_when_only_that_dir_exists() {
        assert_eq!(
            select_host_local_dir("", true, false),
            "/root/surmount-server/host-local"
        );
    }
}
