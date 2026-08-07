//! Pure helpers and shared utilities for Surmount end-to-end runners.
//!
//! Host orchestration lives in the `surmount-e2e-host` binary; local hermetic
//! orchestration in `surmount-e2e`. This lib stays free of network/systemctl
//! side effects so unit tests lock parse contracts offline.

pub mod pure;
pub mod report;
pub mod tls_check;

pub use pure::{
    LabIpBanPrecheck, MdweRow, base_url_missing_is_fail, cap_string_has_net_admin,
    host_mode_enabled, host_runner_exit_code, hostport_from_base_url, lab_ip_ban_precheck,
    lab_ip_looks_exact_token, lab_ip_required_for_ban, mdwe_row,
};
