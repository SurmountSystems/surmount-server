//! Splora git pin the systems status surface observes.
//!
//! The NixOS unit sets `SURMOUNT_SPLORA_REV` from the locked flake input
//! (`nix/splora-pin-status.nix`). This module does not open a socket and
//! does not start a second status page.

/// Locked Splora revision the status row must report as pinned.
pub const EXPECTED_SPLORA_REV: &str = "1a50824e06dec0680ea7e414255970e8618ddf13";

/// Rev from the process environment. Empty or unset is `absent`.
pub fn observed_rev() -> String {
    match std::env::var("SURMOUNT_SPLORA_REV") {
        Ok(raw) => {
            let rev = raw.trim();
            if rev.is_empty() {
                "absent".to_string()
            } else {
                rev.to_string()
            }
        }
        Err(_) => "absent".to_string(),
    }
}

/// `pinned` only when the observed rev is the locked pin.
pub fn classify(observed: &str) -> &'static str {
    if observed == EXPECTED_SPLORA_REV {
        "pinned"
    } else if observed.is_empty() || observed == "absent" {
        "absent"
    } else {
        "mismatch"
    }
}

/// One status sentence. Includes the locked pin and the observed rev.
pub fn status_line(observed: &str) -> String {
    format!(
        "Splora pin {EXPECTED_SPLORA_REV}. Observed {observed}. State {}.",
        classify(observed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_rev_is_the_locked_pin() {
        assert_eq!(
            EXPECTED_SPLORA_REV,
            "1a50824e06dec0680ea7e414255970e8618ddf13"
        );
        assert_eq!(EXPECTED_SPLORA_REV.len(), 40);
    }

    #[test]
    fn classify_pinned_only_for_the_locked_rev() {
        assert_eq!(classify(EXPECTED_SPLORA_REV), "pinned");
        assert_eq!(classify("absent"), "absent");
        assert_eq!(classify(""), "absent");
        assert_eq!(
            classify("28eb3ea077eee02bd8a9c0cba63ee9a611b10e54"),
            "mismatch"
        );
    }

    #[test]
    fn status_line_contains_the_locked_pin() {
        let line = status_line(EXPECTED_SPLORA_REV);
        assert!(line.contains(EXPECTED_SPLORA_REV), "{line}");
        assert!(line.contains("State pinned."), "{line}");
        let missing = status_line("absent");
        assert!(missing.contains(EXPECTED_SPLORA_REV), "{missing}");
        assert!(missing.contains("Observed absent."), "{missing}");
        assert!(missing.contains("State absent."), "{missing}");
    }
}
