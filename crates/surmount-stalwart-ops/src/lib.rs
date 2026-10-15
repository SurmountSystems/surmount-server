//! Stalwart operator drivers (DKIM, free :443, mail-plane TLS, tokens, recovery).
//!
//! Never logs STALWART_TOKEN, API tokens, or password values. Hermetic tests
//! use fake CLI only. The lab VM directory setup and SMTP acceptance bins do
//! not print the API token or the message body.

mod add_token;
mod bootstrap;
mod cli;
mod dkim;
mod error;
mod free443;
mod json;
mod mail_tls;
mod recovery;
mod smtp_accept;
mod token;
mod vm_directory;

pub use add_token::run as run_add_token;
pub use bootstrap::run as run_bootstrap;
pub use dkim::run as run_register_dkim;
pub use error::{Result, ToolError};
pub use free443::run as run_free_443;
pub use json::{account_query_has_admin, file_cert_id_in_query, find_domain_id};
pub use mail_tls::run as run_point_mail_tls;
pub use recovery::run as run_recovery_unlock;
pub use smtp_accept::{SmtpAcceptReport, run_vm_lab_smtp_accept, smtp_accept_io};
pub use token::write_api_token_file;
pub use vm_directory::{VmLabDirectory, run_vm_lab_directory, run_vm_lab_directory_guest};
