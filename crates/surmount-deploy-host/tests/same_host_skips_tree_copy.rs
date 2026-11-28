//! A dry-run already in `/mnt/hdd/projects/server` on the target host must
//! not build an rsync of that tree onto `/root/surmount-server` or
//! `root@surmount-1:/root/surmount-server/`.
//!
//! `tree_rsync_argv` is the function that would build that command.
//! `rsync_remote` returns before it builds one when the checkout is
//! already that directory on this host. A push from horizon still sends
//! source, and that argv excludes `vendor/`. A non-zero nix build stays
//! an error.

use std::path::Path;

use surmount_deploy_host::{
    ExecHere, nix_build_dir, nix_build_exit, same_host_checkout_skips_tree_copy, tree_rsync_argv,
};

const CHECKOUT: &str = "/mnt/hdd/projects/server";
const REMOTE_DIR: &str = "/root/surmount-server";
const TARGET: &str = "root@surmount-1";
const LOCAL_DEST: &str = "/root/surmount-server/";
const SSH_DEST: &str = "root@surmount-1:/root/surmount-server/";

fn here(hostname: &str, euid: u32) -> ExecHere<'_> {
    ExecHere { hostname, euid }
}

fn plan(checkout: &str, hostname: &str, euid: u32) -> Option<Vec<String>> {
    tree_rsync_argv(
        Path::new(checkout),
        TARGET,
        REMOTE_DIR,
        here(hostname, euid),
    )
}

fn is_self_copy(argv: &[String]) -> bool {
    let has_src = argv.iter().any(|arg| arg.trim_end_matches('/') == CHECKOUT);
    let has_dest = argv.iter().any(|arg| {
        arg == LOCAL_DEST
            || arg == SSH_DEST
            || arg.trim_end_matches('/') == REMOTE_DIR
            || arg.trim_end_matches('/') == "root@surmount-1:/root/surmount-server"
    });
    has_src && has_dest
}

/// Copied paths only. `--exclude vendor/` is not a copied path.
fn vendor_is_copied(argv: &[String]) -> bool {
    let mut skip_value = false;
    for arg in argv {
        if skip_value {
            skip_value = false;
            continue;
        }
        if arg == "--exclude" || arg == "-e" {
            skip_value = true;
            continue;
        }
        if arg == "rsync" || arg.starts_with('-') {
            continue;
        }
        if arg.contains("vendor") {
            return true;
        }
    }
    false
}

#[test]
fn same_host_checkout_does_not_build_the_tree_rsync() {
    let root_on_target = plan(CHECKOUT, "surmount-1", 0);
    assert!(
        root_on_target.is_none(),
        "tree_rsync_argv still builds a copy: {root_on_target:?}"
    );
    let rendered = root_on_target
        .as_ref()
        .map(|argv| argv.join(" "))
        .unwrap_or_default();
    assert!(
        !rendered.contains("/mnt/hdd/projects/server/"),
        "{rendered}"
    );
    assert!(!rendered.contains("/root/surmount-server"), "{rendered}");
    assert!(
        !rendered.contains("root@surmount-1:/root/surmount-server/"),
        "{rendered}"
    );
    assert!(same_host_checkout_skips_tree_copy(
        CHECKOUT,
        TARGET,
        here("surmount-1", 0)
    ));
    assert_eq!(
        nix_build_dir(CHECKOUT, REMOTE_DIR, TARGET, here("surmount-1", 0)),
        CHECKOUT
    );
    let slashed = plan("/mnt/hdd/projects/server/", "surmount-1", 0);
    assert!(slashed.is_none(), "{slashed:?}");
    assert!(!vendor_is_copied(&root_on_target.unwrap_or_default()));
}

#[test]
fn same_host_non_root_does_not_build_the_ssh_rsync() {
    let argv = plan(CHECKOUT, "surmount-1", 1000);
    assert!(
        argv.is_none(),
        "tree_rsync_argv still builds ssh rsync: {argv:?}"
    );
    assert!(same_host_checkout_skips_tree_copy(
        CHECKOUT,
        TARGET,
        here("surmount-1", 1000)
    ));
}

#[test]
fn on_target_plan_never_pairs_checkout_with_copy_dest() {
    for euid in [0_u32, 1000] {
        if let Some(argv) = plan(CHECKOUT, "surmount-1", euid) {
            assert!(
                !is_self_copy(&argv),
                "tree_rsync_argv built the self copy: {argv:?}"
            );
            assert!(
                !vendor_is_copied(&argv),
                "vendor is in the copy set: {argv:?}"
            );
        }
    }
}

#[test]
fn horizon_push_sends_source_and_excludes_vendor() {
    let checkout = "/home/hunter/Projects/surmount/surmount-server";
    let argv = plan(checkout, "horizon", 0).expect("horizon still copies source");
    assert!(
        argv.iter().any(|arg| arg == SSH_DEST),
        "horizon still rsyncs source to the mail host: {argv:?}"
    );
    assert!(
        argv.windows(2)
            .any(|pair| pair[0] == "--exclude" && pair[1] == "vendor/"),
        "vendor must be excluded from the horizon push: {argv:?}"
    );
    assert!(!vendor_is_copied(&argv), "{argv:?}");
    assert!(argv.iter().any(|arg| arg == "--delete"), "{argv:?}");
    assert!(!is_self_copy(&argv), "{argv:?}");
    assert!(!same_host_checkout_skips_tree_copy(
        checkout,
        TARGET,
        here("horizon", 0)
    ));
    assert_eq!(
        nix_build_dir(checkout, REMOTE_DIR, TARGET, here("horizon", 0)),
        REMOTE_DIR
    );
}

#[test]
fn nonzero_nix_build_is_still_an_error() {
    let err = nix_build_exit(1).unwrap_err();
    assert_ne!(err.exit_code, 0);
    assert!(err.message.contains("nix build failed"), "{}", err.message);
    assert!(err.message.contains("was not switched"), "{}", err.message);
    assert!(nix_build_exit(0).is_ok());
    let err7 = nix_build_exit(7).unwrap_err();
    assert!(err7.message.contains("exit 7"), "{}", err7.message);
    assert_ne!(err7.exit_code, 0);
}
