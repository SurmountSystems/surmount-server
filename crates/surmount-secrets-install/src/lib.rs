//! Domain A -> Domain B deploy-secrets install bridge and Vaultwarden export.
//!
//! Never logs secret values. Hermetic tests use --dest-root / --from-fixture only.

mod error;
mod export;
mod install;
mod kinds;
mod paths;

pub use error::{Result, ToolError};
pub use export::run as run_export;
pub use install::{run as run_install, run_from_staging};
pub use kinds::{leaf_install_mode, rewrite_namecheap_client_ip, validate_kind};
pub use paths::{DURABLE_MATERIAL, discover_repo_root, normalize_host_path};
