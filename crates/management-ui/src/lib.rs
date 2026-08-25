//! Library surface for management-ui binaries and hermetic tests.
//!
//! The primary binary is `surmount-management-ui`. The least-privilege kernel
//! firewall ban helper is `surmount-nft-ban-helper` (no CAP_NET_ADMIN on the UI
//! process; helper applies set membership over a Unix socket).

pub mod auth;
pub mod ban;
pub mod console_accounts;
pub mod nft_helper;
pub mod nwc;
pub mod rate_limit;
