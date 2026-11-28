//! A dry-run already executing as root on the target host does not build
//! an ssh argv to that host. The nix build command still runs, as local
//! `bash -s` (script on stdin) or `bash -c` (one shell string).
//!
//! This file does not spawn ssh. It does not read or write a private key.

use surmount_deploy_host::{
    ExecHere, LOCAL_SHELL, already_root_on_target, remote_exec_argv, rsync_dest, rsync_invokes_ssh,
};

fn here(hostname: &str, euid: u32) -> ExecHere<'_> {
    ExecHere { hostname, euid }
}

fn bash_s() -> [&'static str; 2] {
    [LOCAL_SHELL, "-s"]
}

fn plan(target: &str, remote_args: &[&str], hostname: &str, euid: u32) -> Vec<String> {
    let extra = vec!["-i".to_string(), "/root/.ssh/id_ed25519".to_string()];
    remote_exec_argv(
        "ssh",
        &extra,
        target,
        remote_args,
        LOCAL_SHELL,
        here(hostname, euid),
    )
}

fn assert_ssh_to(argv: &[String], target: &str) {
    assert_eq!(argv.first().map(String::as_str), Some("ssh"), "{argv:?}");
    let dash = argv.iter().position(|arg| arg == "--").expect("ssh --");
    assert_eq!(
        argv.get(dash + 1).map(String::as_str),
        Some(target),
        "{argv:?}"
    );
}

fn assert_local_no_ssh(argv: &[String]) {
    assert!(
        argv.iter().all(|arg| {
            arg != "ssh"
                && arg != "--"
                && arg != "-i"
                && !arg.contains("id_ed25519")
                && !arg.contains('@')
        }),
        "{argv:?}"
    );
    assert_eq!(
        argv.first().map(String::as_str),
        Some(LOCAL_SHELL),
        "{argv:?}"
    );
}

#[test]
fn root_on_the_target_does_not_ssh_for_the_dry_run_build() {
    // `ssh_bash_stdin_command` uses this argv for the dry-run nix build.
    // The build script is written to that process's stdin. It is not dropped.
    let argv = plan("root@surmount-1", &bash_s(), "surmount-1", 0);
    assert_eq!(argv, vec![LOCAL_SHELL.to_string(), "-s".to_string()]);
    assert_local_no_ssh(&argv);
    assert!(already_root_on_target(
        "root@surmount-1",
        here("surmount-1", 0)
    ));
}

#[test]
fn root_on_the_target_keeps_a_shell_command_without_ssh() {
    let command = "mkdir -p /root/surmount-server";
    let argv = plan("root@surmount-1", &[command], "surmount-1", 0);
    assert_eq!(
        argv,
        vec![
            LOCAL_SHELL.to_string(),
            "-c".to_string(),
            command.to_string(),
        ]
    );
    assert_local_no_ssh(&argv);
}

#[test]
fn local_shell_string_keeps_the_nix_build_text() {
    let command = "nice -n 19 nix build --no-link --print-out-paths";
    let argv = plan("root@surmount-1", &[command], "surmount-1", 0);
    assert_eq!(argv[0], LOCAL_SHELL);
    assert_eq!(argv[1], "-c");
    assert_eq!(argv[2], command);
    assert!(argv[2].contains("nix build"), "{argv:?}");
    assert_local_no_ssh(&argv);
}

#[test]
fn the_check_is_the_target_host_not_a_hardcoded_name() {
    let argv = plan("root@mail.example", &bash_s(), "mail.example", 0);
    assert_eq!(argv, vec![LOCAL_SHELL.to_string(), "-s".to_string()]);
    assert_local_no_ssh(&argv);
    assert!(!already_root_on_target(
        "root@mail.example",
        here("surmount-1", 0)
    ));
}

#[test]
fn horizon_root_still_builds_ssh_to_the_mail_host() {
    let argv = plan("root@surmount-1", &bash_s(), "horizon", 0);
    assert_ssh_to(&argv, "root@surmount-1");
    assert!(argv.iter().any(|arg| arg == LOCAL_SHELL), "{argv:?}");
    assert!(argv.iter().any(|arg| arg == "-s"), "{argv:?}");
    assert!(!already_root_on_target(
        "root@surmount-1",
        here("horizon", 0)
    ));
}

#[test]
fn non_root_on_the_target_still_builds_ssh() {
    let argv = plan("root@surmount-1", &bash_s(), "surmount-1", 1000);
    assert_ssh_to(&argv, "root@surmount-1");
    assert!(
        argv.iter().any(|arg| arg.contains("id_ed25519")),
        "{argv:?}"
    );
}

#[test]
fn root_on_the_host_does_not_replace_a_nixbuilder_login() {
    let argv = plan("nixbuilder@surmount-1", &bash_s(), "surmount-1", 0);
    assert_ssh_to(&argv, "nixbuilder@surmount-1");
}

#[test]
fn bare_root_user_is_not_this_host() {
    assert!(!already_root_on_target("root", here("surmount-1", 0)));
    let argv = plan("root", &["true"], "surmount-1", 0);
    assert_ssh_to(&argv, "root");
}

#[test]
fn empty_hostname_stays_on_ssh() {
    assert!(!already_root_on_target("root@surmount-1", here("", 0)));
    assert!(!already_root_on_target("root@surmount-1", here("  ", 0)));
}

#[test]
fn local_rsync_has_no_ssh_destination() {
    let on_target = here("surmount-1", 0);
    assert!(!rsync_invokes_ssh("root@surmount-1", on_target));
    assert_eq!(
        rsync_dest("root@surmount-1", "/root/surmount-server", on_target),
        "/root/surmount-server/"
    );
    assert_eq!(
        rsync_dest("root@surmount-1", "/root/surmount-server/", on_target),
        "/root/surmount-server/"
    );
    let generic = here("mail.example", 0);
    assert!(!rsync_invokes_ssh("root@mail.example", generic));
    assert_eq!(
        rsync_dest("root@mail.example", "/srv/mail", generic),
        "/srv/mail/"
    );
}

#[test]
fn laptop_rsync_still_uses_ssh_syntax() {
    let laptop = here("horizon", 0);
    assert!(rsync_invokes_ssh("root@surmount-1", laptop));
    assert_eq!(
        rsync_dest("root@surmount-1", "/root/surmount-server/", laptop),
        "root@surmount-1:/root/surmount-server/"
    );
    let user = here("surmount-1", 1000);
    assert!(rsync_invokes_ssh("root@surmount-1", user));
    assert_eq!(
        rsync_dest("root@surmount-1", "/root/surmount-server", user),
        "root@surmount-1:/root/surmount-server/"
    );
}
