//! Operator-driven NixOS deploy driver and post-switch smoke.
//!
//! Never logs secret values. Never embeds real host IPs or keys.

mod args;
mod error;
mod keys;
mod local_on_target;
mod machine_host_local;
mod nix_conf;
mod nixos_rebuild_bin;
mod paths;
mod recipe_batch_01;
mod recipe_batch_02;
mod recipe_batch_03;
mod recipe_batch_04;
mod recipe_batch_05;
mod run;
mod smoke;
mod smoke_unit_wait;
mod tool_path;
mod update_maintenance;
mod update_pin_refresh;
mod vendor_recipe;

pub use args::{ALLOWED_SECRETS_KINDS, DeployOpts, Mode, USAGE, parse_args, validate_secrets_kind};
pub use error::ToolError;
pub use keys::{check_host_local, count_authorized_key_lines};
pub use local_on_target::{
    ExecHere, LOCAL_SHELL, already_root_on_target, nix_build_dir, remote_exec_argv, rsync_dest,
    rsync_invokes_ssh, same_host_checkout_skips_tree_copy,
};
pub use paths::{
    assert_safe_token, discover_repo_root, is_path_under_root, is_public_product_path, realpath_m,
};
pub use run::{nix_build_exit, run, tree_rsync_argv};
pub use smoke::{SMOKE_USAGE, parse_listen_to_loopback_health, run_smoke};
