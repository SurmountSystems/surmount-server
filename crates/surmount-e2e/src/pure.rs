//! Pure parse/detect helpers (no systemctl, no nft, no network).
//!
//! Ported from the former `scripts/lib/e2e-host-pure.sh` with the same
//! contracts so offline tests stay hermetic.

/// Derive `host:port` for TLS checks from a BASE_URL (`scheme://host[:port][/path]`).
/// Default port: 443 for https, 80 otherwise. Empty input => empty output.
///
/// Limitation: bracketed IPv6 authorities (`https://[2001:db8::1]:8443`) are not
/// parsed; current host recipes use hostnames or IPv4.
pub fn hostport_from_base_url(base_url: &str) -> String {
    if base_url.is_empty() {
        return String::new();
    }
    let without_scheme = base_url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(base_url);
    let hostport = without_scheme
        .split_once('/')
        .map(|(hp, _)| hp)
        .unwrap_or(without_scheme);
    if hostport.contains(':') {
        return hostport.to_string();
    }
    if base_url.starts_with("https://") {
        format!("{hostport}:443")
    } else {
        format!("{hostport}:80")
    }
}

/// True when a capability property string mentions `CAP_NET_ADMIN`
/// (case-insensitive ambient/bounding text from `systemctl show`).
/// Empty string => false.
pub fn cap_string_has_net_admin(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.to_ascii_lowercase().contains("cap_net_admin")
}

/// Exact-element caution: LAB_IP must not be matched via substring of nft list
/// output (e.g. `203.0.113.5` must not match `203.0.113.50`).
/// Returns true when candidate looks like a single IPv4/IPv6 token (no spaces).
pub fn lab_ip_looks_exact_token(ip: &str) -> bool {
    if ip.is_empty() {
        return false;
    }
    if ip.chars().any(|c| c.is_whitespace() || c == ',') {
        return false;
    }
    // Very light shape: digits/hex/dots/colons only.
    ip.chars()
        .all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':')
}

/// Host e2e mode is on only for explicit `1` or `true` (else exit 2).
pub fn host_mode_enabled(env_val: &str) -> bool {
    env_val == "1" || env_val == "true"
}

/// Process exit code for host runner after probes complete.
/// `host_on` false => 2 (gate). Else any fail row => 1, else 0.
pub fn host_runner_exit_code(host_on: bool, fail_rows: u32) -> i32 {
    if !host_on {
        return 2;
    }
    if fail_rows > 0 {
        1
    } else {
        0
    }
}

/// BASE_URL empty while host mode is on is a FAIL row (not silent SKIP).
pub fn base_url_missing_is_fail(host_on: bool, base_url: &str) -> bool {
    host_on && base_url.is_empty()
}

/// Ban track without SKIP_BAN requires LAB_IP (empty => FAIL, not false-green).
pub fn lab_ip_required_for_ban(skip_ban: bool, lab_ip: &str) -> bool {
    !skip_ban && lab_ip.is_empty()
}

/// Ban-track LAB_IP precheck before nft membership.
///
/// When ban track is on (`!skip_ban`): empty IP fails; non-token (spaces/list
/// bleed) fails so operators get a clear message instead of opaque nft errors.
/// Returns `None` when the IP is ready for exact-element membership check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabIpBanPrecheck {
    Ok,
    Missing,
    NotExactToken,
    Skipped,
}

pub fn lab_ip_ban_precheck(skip_ban: bool, lab_ip: &str) -> LabIpBanPrecheck {
    if skip_ban {
        return LabIpBanPrecheck::Skipped;
    }
    if lab_ip.is_empty() {
        return LabIpBanPrecheck::Missing;
    }
    if !lab_ip_looks_exact_token(lab_ip) {
        return LabIpBanPrecheck::NotExactToken;
    }
    LabIpBanPrecheck::Ok
}

/// MemoryDenyWriteExecute row policy for host e2e.
///
/// Must not PASS when the UI unit is inactive even if the unit *file* property
/// is `yes` (systemctl show can return misleading defaults). Mirrors CAP gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdweRow {
    Pass,
    FailInactive,
    FailValue,
}

pub fn mdwe_row(unit_active: bool, mdwe_value: &str) -> MdweRow {
    if !unit_active {
        return MdweRow::FailInactive;
    }
    if mdwe_value == "yes" {
        MdweRow::Pass
    } else {
        MdweRow::FailValue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- hostport_from_base_url (named contracts from former bash pure tests) ---

    #[test]
    fn hostport_https_default_443() {
        assert_eq!(hostport_from_base_url("https://127.0.0.1"), "127.0.0.1:443");
    }

    #[test]
    fn hostport_https_with_path() {
        assert_eq!(
            hostport_from_base_url("https://services.example/health"),
            "services.example:443"
        );
    }

    #[test]
    fn hostport_http_default_80() {
        assert_eq!(hostport_from_base_url("http://127.0.0.1"), "127.0.0.1:80");
    }

    #[test]
    fn hostport_explicit_port_kept() {
        assert_eq!(
            hostport_from_base_url("https://127.0.0.1:8443/x"),
            "127.0.0.1:8443"
        );
    }

    #[test]
    fn hostport_empty() {
        assert_eq!(hostport_from_base_url(""), "");
    }

    // --- cap_string_has_net_admin ---

    #[test]
    fn cap_empty_is_no_net_admin() {
        assert!(!cap_string_has_net_admin(""));
    }

    #[test]
    fn cap_bind_only_is_no_net_admin() {
        assert!(!cap_string_has_net_admin("CAP_NET_BIND_SERVICE"));
    }

    #[test]
    fn cap_ambient_has_net_admin() {
        assert!(cap_string_has_net_admin("CAP_NET_ADMIN CAP_NET_RAW"));
    }

    #[test]
    fn cap_lower_case_detect() {
        assert!(cap_string_has_net_admin("cap_net_admin"));
    }

    #[test]
    fn cap_bounding_mixed() {
        assert!(cap_string_has_net_admin(
            "cap_net_bind_service cap_net_admin"
        ));
    }

    // --- lab_ip_looks_exact_token ---

    #[test]
    fn lab_ip_exact_v4() {
        assert!(lab_ip_looks_exact_token("203.0.113.50"));
    }

    #[test]
    fn lab_ip_rejects_space() {
        assert!(!lab_ip_looks_exact_token("203.0.113.5 0"));
    }

    #[test]
    fn lab_ip_rejects_empty() {
        assert!(!lab_ip_looks_exact_token(""));
    }

    #[test]
    fn lab_ip_v6_token() {
        assert!(lab_ip_looks_exact_token("2001:db8::1"));
    }

    // --- host gate contracts ---

    #[test]
    fn host_mode_only_one_or_true() {
        assert!(host_mode_enabled("1"));
        assert!(host_mode_enabled("true"));
        assert!(!host_mode_enabled(""));
        assert!(!host_mode_enabled("0"));
        assert!(!host_mode_enabled("yes"));
    }

    #[test]
    fn host_runner_exit_gate_is_two() {
        assert_eq!(host_runner_exit_code(false, 0), 2);
        assert_eq!(host_runner_exit_code(false, 5), 2);
    }

    #[test]
    fn host_runner_exit_fail_is_one() {
        assert_eq!(host_runner_exit_code(true, 1), 1);
        assert_eq!(host_runner_exit_code(true, 0), 0);
    }

    #[test]
    fn base_url_empty_fails_when_host_on() {
        assert!(base_url_missing_is_fail(true, ""));
        assert!(!base_url_missing_is_fail(true, "https://127.0.0.1"));
        assert!(!base_url_missing_is_fail(false, ""));
    }

    #[test]
    fn lab_ip_required_unless_skip_ban() {
        assert!(lab_ip_required_for_ban(false, ""));
        assert!(!lab_ip_required_for_ban(true, ""));
        assert!(!lab_ip_required_for_ban(false, "203.0.113.50"));
    }

    // --- lab_ip_ban_precheck (wire exact-token before nft) ---

    #[test]
    fn lab_ip_ban_precheck_ok_exact_token() {
        assert_eq!(
            lab_ip_ban_precheck(false, "203.0.113.50"),
            LabIpBanPrecheck::Ok
        );
    }

    #[test]
    fn lab_ip_ban_precheck_missing_when_ban_on() {
        assert_eq!(lab_ip_ban_precheck(false, ""), LabIpBanPrecheck::Missing);
    }

    #[test]
    fn lab_ip_ban_precheck_non_token_fails_before_nft() {
        // Named contract: ban track on + space-bleed LAB_IP is Fail, not nft call.
        assert_eq!(
            lab_ip_ban_precheck(false, "203.0.113.5 0"),
            LabIpBanPrecheck::NotExactToken
        );
        assert_eq!(
            lab_ip_ban_precheck(false, "203.0.113.5,203.0.113.6"),
            LabIpBanPrecheck::NotExactToken
        );
    }

    #[test]
    fn lab_ip_ban_precheck_skipped_when_skip_ban() {
        assert_eq!(lab_ip_ban_precheck(true, ""), LabIpBanPrecheck::Skipped);
        assert_eq!(
            lab_ip_ban_precheck(true, "not a token"),
            LabIpBanPrecheck::Skipped
        );
    }

    // --- mdwe_row (inactive unit must not PASS) ---

    #[test]
    fn mdwe_inactive_with_yes_is_not_pass() {
        // Named contract: inactive unit + property "yes" must not be Pass.
        assert_eq!(mdwe_row(false, "yes"), MdweRow::FailInactive);
        assert_eq!(mdwe_row(false, ""), MdweRow::FailInactive);
        assert_eq!(mdwe_row(false, "no"), MdweRow::FailInactive);
    }

    #[test]
    fn mdwe_active_yes_is_pass() {
        assert_eq!(mdwe_row(true, "yes"), MdweRow::Pass);
    }

    #[test]
    fn mdwe_active_not_yes_is_fail_value() {
        assert_eq!(mdwe_row(true, "no"), MdweRow::FailValue);
        assert_eq!(mdwe_row(true, ""), MdweRow::FailValue);
        assert_eq!(mdwe_row(true, "unset"), MdweRow::FailValue);
    }
}
