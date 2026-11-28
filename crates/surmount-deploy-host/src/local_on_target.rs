//! Local exec when this process is already root on the deploy target.
//!
//! `ssh_command_args` and `rsync_remote` in `run.rs` call these functions.
//! A match does not spawn ssh and does not read an identity file.
//! A non-match keeps the ssh argv. Nix build failure stays a non-zero exit.

use std::fs;

/// System bash on the mail host. Same path `run.rs` used for `bash -s`.
/// sshd would hand that argv to root's login shell. Local exec starts it
/// directly, so fish is not in the way.
pub const LOCAL_SHELL: &str = "/run/current-system/sw/bin/bash";

/// Unreadable `/proc/self/status` is not root. Local exec stays off.
const UNKNOWN_EUID: u32 = u32::MAX;

/// True when the operator asked for a root login (rebuild needs root, not nixbuilder).
pub(crate) fn target_user_is_root(target: &str) -> bool {
    let t = target.trim();
    t == "root" || t.starts_with("root@")
}

/// SSH host part. `root@surmount-1` is `surmount-1`. A bare host stays as written.
pub(crate) fn host_from_target(target: &str) -> &str {
    let t = target.trim();
    t.rsplit_once('@').map(|(_, host)| host.trim()).unwrap_or(t)
}

/// Where this process is, for the local-exec decision. Tests pass this in.
/// Production reads `/etc/hostname` and `/proc/self/status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecHere<'a> {
    pub hostname: &'a str,
    pub euid: u32,
}

/// True when `target` is a root login on this machine and this process is root.
///
/// The host compare is the SSH host part, not a fixed name. An empty
/// hostname or an euid other than 0 stays on the ssh path. That is the
/// laptop case: root's euid on horizon must still ssh to the mail host.
pub fn already_root_on_target(target: &str, here: ExecHere<'_>) -> bool {
    if here.euid != 0 {
        return false;
    }
    if !target_user_is_root(target) {
        return false;
    }
    let host = host_from_target(target);
    let name = here.hostname.trim();
    if host.is_empty() || name.is_empty() {
        return false;
    }
    host == name
}

pub(crate) fn current_hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .ok()
        .map(|text| text.trim().to_string())
        .unwrap_or_default()
}

pub(crate) fn current_euid() -> u32 {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return UNKNOWN_EUID;
    };
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("Uid:") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let _real = parts.next();
        return parts
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(UNKNOWN_EUID);
    }
    UNKNOWN_EUID
}

/// Argv `ssh_command_args` spawns. Local means no ssh binary and no target.
pub fn remote_exec_argv(
    ssh_bin: &str,
    ssh_extra: &[String],
    target: &str,
    remote_args: &[&str],
    local_shell: &str,
    here: ExecHere<'_>,
) -> Vec<String> {
    if already_root_on_target(target, here) {
        local_argv(remote_args, local_shell)
    } else {
        ssh_argv(ssh_bin, ssh_extra, target, remote_args)
    }
}

/// One remote string is a shell command (`mkdir -p ...`), same as ssh.
/// `bash -s` and any other explicit argv run as that program.
pub(crate) fn local_argv(remote_args: &[&str], local_shell: &str) -> Vec<String> {
    if remote_args.len() == 1 {
        vec![
            local_shell.to_string(),
            "-c".to_string(),
            remote_args[0].to_string(),
        ]
    } else if remote_args.is_empty() {
        vec![
            local_shell.to_string(),
            "-c".to_string(),
            "exit 1".to_string(),
        ]
    } else {
        remote_args.iter().map(|s| (*s).to_string()).collect()
    }
}

pub(crate) fn ssh_argv(
    ssh_bin: &str,
    extra: &[String],
    target: &str,
    remote_args: &[&str],
) -> Vec<String> {
    let mut argv = Vec::with_capacity(4 + extra.len() + remote_args.len());
    argv.push(ssh_bin.to_string());
    argv.extend(extra.iter().cloned());
    argv.push("--".to_string());
    argv.push(target.to_string());
    argv.extend(remote_args.iter().map(|s| (*s).to_string()));
    argv
}

/// False when rsync must copy to a local directory and must not pass `-e ssh`.
pub fn rsync_invokes_ssh(target: &str, here: ExecHere<'_>) -> bool {
    !already_root_on_target(target, here)
}

/// `user@host:dir/` on the ssh path. `dir/` when already root on that host.
pub fn rsync_dest(target: &str, remote_dir: &str, here: ExecHere<'_>) -> String {
    let dir = remote_dir.trim_end_matches('/');
    if rsync_invokes_ssh(target, here) {
        format!("{target}:{dir}/")
    } else {
        format!("{dir}/")
    }
}

/// Checkout the mail host already builds from. A dry-run already in this
/// directory must not rsync the tree.
pub const MAIL_HOST_BUILD_CHECKOUT: &str = "/mnt/hdd/projects/server";

fn trim_dir(path: &str) -> &str {
    path.trim().trim_end_matches('/')
}

/// True when `checkout` is the directory the Nix build should use and
/// this process is already on that target host.
///
/// Hostname match is the check, not euid. A non-root process on the
/// target would otherwise ssh the tree back to itself. A push from
/// horizon does not match, so that push still sends source.
pub fn same_host_checkout_skips_tree_copy(
    checkout: &str,
    target: &str,
    here: ExecHere<'_>,
) -> bool {
    if trim_dir(checkout) != MAIL_HOST_BUILD_CHECKOUT {
        return false;
    }
    let host = host_from_target(target);
    let name = here.hostname.trim();
    !host.is_empty() && !name.is_empty() && host == name
}

/// Directory the Nix build reads. The same-host checkout stays put.
/// Any other source still builds in `remote_dir` after the source copy.
pub fn nix_build_dir(checkout: &str, remote_dir: &str, target: &str, here: ExecHere<'_>) -> String {
    if same_host_checkout_skips_tree_copy(checkout, target, here) {
        trim_dir(checkout).to_string()
    } else {
        trim_dir(remote_dir).to_string()
    }
}
