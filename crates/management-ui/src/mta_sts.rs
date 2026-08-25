//! MTA-STS policy body helpers (RFC 8461).
//!
//! Offline skeleton: product can serve `/.well-known/mta-sts.txt` when mode is
//! `testing` or `enforce`. Default is **off** (no policy body) until the
//! operator enables it and public HTTPS for the policy host is live.
//!
//! Policy host is `mta-sts.<primary_domain>` (see docs/DNS.md). DNS TXT
//! `_mta-sts` and A/AAAA for that host remain operator/DNS residual.

use crate::redirect::{host_for_url_authority, host_is_allowlisted};

/// STS policy mode. Default off for offline / pre-public-HTTPS safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MtaStsMode {
    /// Do not serve a policy body (404).
    #[default]
    Off,
    /// RFC `mode: testing` (report only; do not fail closed on mismatch).
    Testing,
    /// RFC `mode: enforce` (receivers should refuse non-TLS / wrong MX).
    Enforce,
}

impl MtaStsMode {
    /// Parse env/Nix value: `off` | `testing` | `enforce` (case-insensitive).
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "testing" | "test" => Ok(Self::Testing),
            "enforce" | "enforcing" => Ok(Self::Enforce),
            other => Err(format!(
                "invalid MTA-STS mode {other:?}; expected off, testing, or enforce"
            )),
        }
    }

    /// RFC mode token when a policy body should be served.
    pub fn as_policy_mode(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Testing => Some("testing"),
            Self::Enforce => Some("enforce"),
        }
    }

    pub fn is_enabled(self) -> bool {
        self.as_policy_mode().is_some()
    }
}

/// Policy hostname for a primary domain (`mta-sts.example.test`).
pub fn mta_sts_policy_hostname(primary_domain: &str) -> String {
    let primary = host_for_url_authority(primary_domain);
    if primary.is_empty() {
        return String::new();
    }
    format!("mta-sts.{primary}")
}

/// True when Host is the MTA-STS policy host for `primary_domain`.
pub fn host_is_mta_sts_policy_host(host_header: &str, primary_domain: &str) -> bool {
    let expect = mta_sts_policy_hostname(primary_domain);
    if expect.is_empty() {
        return false;
    }
    host_for_url_authority(host_header).eq_ignore_ascii_case(&expect)
}

/// Build RFC 8461 policy body, or None when mode is off / mx empty.
///
/// Shape matches docs/DNS.md sample:
/// ```text
/// version: STSv1
/// mode: testing
/// mx: mail.example.test
/// max_age: 86400
/// ```
pub fn mta_sts_policy_body(mode: MtaStsMode, mx: &str, max_age: u64) -> Option<String> {
    let mode_s = mode.as_policy_mode()?;
    let mx = host_for_url_authority(mx);
    if mx.is_empty() {
        return None;
    }
    // max_age 0 is allowed by RFC (policy expires immediately); still emit.
    Some(format!(
        "version: STSv1\nmode: {mode_s}\nmx: {mx}\nmax_age: {max_age}\n"
    ))
}

/// Whether the edge should serve the policy for this Host.
///
/// Requires: mode enabled, Host is `mta-sts.<primary>`, and Host is on the
/// redirect/edge allowlist when that list is non-empty (open-host safe).
/// Empty allowlist with mode on still requires correct policy Host (no
/// serve-on-any-Host shortcut).
pub fn should_serve_mta_sts_policy(
    mode: MtaStsMode,
    host_header: &str,
    primary_domain: &str,
    allowed_hosts: &[String],
) -> bool {
    if !mode.is_enabled() {
        return false;
    }
    if !host_is_mta_sts_policy_host(host_header, primary_domain) {
        return false;
    }
    // When allowlist is configured, policy host must be on it (same edge
    // Host discipline as redirects). Empty allowlist: Host match alone.
    if !allowed_hosts.is_empty() && !host_is_allowlisted(host_header, allowed_hosts) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allow(hosts: &[&str]) -> Vec<String> {
        hosts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn mode_parse_defaults_and_aliases() {
        assert_eq!(MtaStsMode::parse("").unwrap(), MtaStsMode::Off);
        assert_eq!(MtaStsMode::parse("off").unwrap(), MtaStsMode::Off);
        assert_eq!(MtaStsMode::parse("TESTING").unwrap(), MtaStsMode::Testing);
        assert_eq!(MtaStsMode::parse("enforce").unwrap(), MtaStsMode::Enforce);
        assert!(MtaStsMode::parse("bogus").is_err());
    }

    #[test]
    fn policy_body_off_is_none() {
        assert!(mta_sts_policy_body(MtaStsMode::Off, "mail.example.test", 86400).is_none());
    }

    #[test]
    fn policy_body_testing_matches_dns_docs_shape() {
        let body = mta_sts_policy_body(MtaStsMode::Testing, "mail.example.test", 86400).unwrap();
        assert_eq!(
            body,
            "version: STSv1\nmode: testing\nmx: mail.example.test\nmax_age: 86400\n"
        );
    }

    #[test]
    fn policy_body_enforce() {
        let body =
            mta_sts_policy_body(MtaStsMode::Enforce, "mail.surmount.systems", 604800).unwrap();
        assert!(body.contains("mode: enforce"));
        assert!(body.contains("mx: mail.surmount.systems"));
        assert!(body.contains("max_age: 604800"));
    }

    #[test]
    fn empty_mx_yields_no_body() {
        assert!(mta_sts_policy_body(MtaStsMode::Testing, "", 86400).is_none());
        assert!(mta_sts_policy_body(MtaStsMode::Testing, "   ", 86400).is_none());
    }

    #[test]
    fn policy_hostname_and_host_match() {
        assert_eq!(
            mta_sts_policy_hostname("example.test"),
            "mta-sts.example.test"
        );
        assert!(host_is_mta_sts_policy_host(
            "mta-sts.example.test",
            "example.test"
        ));
        assert!(host_is_mta_sts_policy_host(
            "MTA-STS.Example.TEST:443",
            "example.test"
        ));
        assert!(!host_is_mta_sts_policy_host(
            "services.example.test",
            "example.test"
        ));
    }

    #[test]
    fn should_serve_requires_mode_host_and_allowlist() {
        let hosts = allow(&["mta-sts.example.test", "services.example.test"]);
        assert!(!should_serve_mta_sts_policy(
            MtaStsMode::Off,
            "mta-sts.example.test",
            "example.test",
            &hosts
        ));
        assert!(should_serve_mta_sts_policy(
            MtaStsMode::Testing,
            "mta-sts.example.test",
            "example.test",
            &hosts
        ));
        // Wrong Host: never serve (even with mode on).
        assert!(!should_serve_mta_sts_policy(
            MtaStsMode::Testing,
            "services.example.test",
            "example.test",
            &hosts
        ));
        // Policy host not allowlisted: refuse.
        assert!(!should_serve_mta_sts_policy(
            MtaStsMode::Testing,
            "mta-sts.example.test",
            "example.test",
            &allow(&["services.example.test"])
        ));
        // Empty allowlist: Host match alone is enough (lab).
        assert!(should_serve_mta_sts_policy(
            MtaStsMode::Testing,
            "mta-sts.example.test",
            "example.test",
            &[]
        ));
    }
}
