//! Stalwart operator drivers (DKIM, free :443, mail-plane TLS, tokens, recovery).
//!
//! Never logs STALWART_TOKEN or password values. Hermetic tests use fake CLI only.

mod add_token;
mod bootstrap;
mod cli;
mod dkim;
mod error;
mod free443;
mod json;
mod mail_tls;
mod recovery;
mod token;

pub use add_token::run as run_add_token;
pub use bootstrap::run as run_bootstrap;
pub use dkim::run as run_register_dkim;
pub use error::{Result, ToolError};
pub use free443::run as run_free_443;
pub use json::{account_query_has_admin, file_cert_id_in_query, find_domain_id};
pub use mail_tls::run as run_point_mail_tls;
pub use recovery::run as run_recovery_unlock;
