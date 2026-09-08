//! Remote grok-oss attach: SSH + tmux as user grok, not Eternal Terminal.
//! Nested guest (etterminal / hostname / root-justfile) attaches local tmux.
//! Root tmux is a miss: user-<uid>.slice MemoryMax only applies to that uid.

use std::io::{self, IsTerminal};
use std::path::Path;

use crate::ProbeError;
use crate::et::env_is_off;
use crate::shell_quote;

pub const SESSION: &str = "grok-oss";
pub const DEFAULT_USER: &str = "grok";
pub const DEFAULT_HOME: &str = "/home/grok";

pub fn session_user() -> String {
    std::env::var("SURMOUNT_GROK_OSS_USER").unwrap_or_else(|_| DEFAULT_USER.to_string())
}

pub fn session_home() -> String {
    std::env::var("SURMOUNT_GROK_OSS_HOME").unwrap_or_else(|_| DEFAULT_HOME.to_string())
}

pub fn has_live_tty() -> bool {
    if env_is_off("SURMOUNT_GROK_OSS_REQUIRE_TTY") {
        return true;
    }
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub fn refuse_without_live_tty() -> Result<(), ProbeError> {
    if has_live_tty() {
        return Ok(());
    }
    Err(ProbeError::fail(
        "refusing to attach without a live tty (stdin and stdout must be a terminal)",
    ))
}

/// Login name for runuser. Not root: MemoryMax lives on the grok user slice.
pub fn validate_session_user(user: &str) -> Result<&str, ProbeError> {
    let first = user.chars().next();
    let ok = first.is_some_and(|c| c.is_ascii_alphabetic())
        && user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && user.len() <= 32;
    if !ok {
        return Err(ProbeError::fail(format!(
            "grok-oss session user is not a safe login name: {user}"
        )));
    }
    if user == "root" {
        return Err(ProbeError::fail(
            "grok-oss session user must not be root (user slice MemoryMax only applies to the grok uid)",
        ));
    }
    Ok(user)
}

fn validate_abs_path<'a>(path: &'a str, label: &str) -> Result<&'a str, ProbeError> {
    if !path.starts_with('/') || path.contains("..") {
        return Err(ProbeError::fail(format!(
            "{label} must be an absolute path without .."
        )));
    }
    if path
        .chars()
        .any(|c| c.is_ascii_whitespace() || ";|&$`'\"\\<>(){}".contains(c))
    {
        return Err(ProbeError::fail(format!(
            "{label} contains unsafe characters"
        )));
    }
    Ok(path)
}

fn grok_tmux_inner(tmux_bin: &str) -> String {
    let tmux = shell_quote(tmux_bin);
    format!("{tmux} attach -t {SESSION} || {tmux} new -s {SESSION} {SESSION}")
}

/// tmux attach-or-new as user grok with that user's HOME / GROK_HOME.
/// Root tmux would miss user-1988.slice MemoryMax.
pub fn grok_tmux_shell(tmux_bin: &str, user: &str, home: &str) -> Result<String, ProbeError> {
    let user = validate_session_user(user)?;
    let home = validate_abs_path(home, "grok home")?;
    let grok_home = format!("{home}/.grok");
    let inner = grok_tmux_inner(tmux_bin);
    Ok(format!(
        "runuser -u {user} -- env HOME={home} GROK_HOME={grok_home} sh -c {}",
        shell_quote(&inner)
    ))
}

/// `grok-oss running --json` as user grok. No TTY. No HTTP bind.
pub fn grok_running_json_shell(user: &str, home: &str) -> Result<String, ProbeError> {
    let user = validate_session_user(user)?;
    let home = validate_abs_path(home, "grok home")?;
    let grok_home = format!("{home}/.grok");
    Ok(format!(
        "runuser -u {user} -- env HOME={home} GROK_HOME={grok_home} grok-oss running --json"
    ))
}

pub fn local_tmux_cmd(tmux_bin: &str) -> Result<String, ProbeError> {
    grok_tmux_shell(tmux_bin, &session_user(), &session_home())
}

pub fn local_running_json_cmd() -> Result<String, ProbeError> {
    grok_running_json_shell(&session_user(), &session_home())
}

fn push_identity(v: &mut Vec<String>, identity: Option<&Path>) {
    if let Some(id) = identity {
        v.push("-i".to_string());
        v.push(id.display().to_string());
        v.push("-o".to_string());
        v.push("IdentitiesOnly=yes".to_string());
    }
}

/// `ssh -t` [identity] -- target 'runuser -u grok -- env HOME=... tmux ...'
pub fn build_remote_argv(
    ssh_bin: &str,
    target: &str,
    identity: Option<&Path>,
) -> Result<Vec<String>, ProbeError> {
    let mut v = vec![ssh_bin.to_string(), "-t".to_string()];
    push_identity(&mut v, identity);
    v.push("--".to_string());
    v.push(target.to_string());
    v.push(grok_tmux_shell("tmux", &session_user(), &session_home())?);
    Ok(v)
}

/// Key-only SSH, no TTY, print `grok-oss running --json` as user grok.
pub fn build_running_json_argv(
    ssh_bin: &str,
    target: &str,
    identity: Option<&Path>,
) -> Result<Vec<String>, ProbeError> {
    let mut v = vec![ssh_bin.to_string()];
    v.extend(crate::key_only_ssh_opts());
    push_identity(&mut v, identity);
    v.push("--".to_string());
    v.push(target.to_string());
    v.push(local_running_json_cmd()?);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::et::{GuestSessionHints, already_on_mail_guest_from};
    use std::path::Path;

    #[test]
    fn attach_is_as_user_grok_not_root() {
        let argv = build_remote_argv("ssh", "root@example.test", None).expect("argv");
        let joined = argv.join(" ");
        let remote = argv.last().expect("remote command");
        assert!(
            remote.contains("runuser -u grok"),
            "attach must runuser as grok so user-1988.slice MemoryMax applies: {joined}"
        );
        assert!(
            remote.contains("HOME=/home/grok"),
            "grok-oss must see grok HOME, not root: {joined}"
        );
        assert!(
            remote.contains("GROK_HOME=/home/grok/.grok"),
            "GROK_HOME must be grok product dir: {joined}"
        );
        assert!(remote.contains("tmux attach -t grok-oss"), "{joined}");
        assert!(remote.contains("tmux new -s grok-oss grok-oss"), "{joined}");
        assert!(
            !remote.starts_with("tmux "),
            "root tmux is a miss: {joined}"
        );
        assert!(
            !remote.contains("runuser -u root"),
            "must not runuser as root: {joined}"
        );
        assert_eq!(argv[0], "ssh");
        assert_eq!(argv[1], "-t");
    }

    #[test]
    fn session_user_rejects_root() {
        let err = grok_tmux_shell("tmux", "root", "/home/grok").unwrap_err();
        assert!(err.message.contains("must not be root"), "{}", err.message);
    }

    #[test]
    fn remote_argv_is_ssh_t_tmux_not_et() {
        let argv = build_remote_argv("ssh", "root@example.test", None).expect("argv");
        let joined = argv.join(" ");
        assert_eq!(argv[0], "ssh");
        assert_eq!(argv[1], "-t");
        assert!(argv.iter().any(|w| w == "root@example.test"), "{joined}");
        assert!(
            argv.iter().any(|w| w.contains("tmux attach -t grok-oss")),
            "{joined}"
        );
        assert!(
            argv.iter()
                .any(|w| w.contains("tmux new -s grok-oss grok-oss")),
            "{joined}"
        );
        assert!(!argv.iter().any(|w| w == "et"), "{joined}");
        assert!(!joined.contains("-p 2022"), "{joined}");
    }

    #[test]
    fn remote_root_identity() {
        let id = Path::new("/tmp/id_ed25519");
        let argv = build_remote_argv("ssh", "root@example.test", Some(id)).expect("argv");
        let joined = argv.join(" ");
        assert!(joined.contains("-i /tmp/id_ed25519"), "{joined}");
        assert!(joined.contains("IdentitiesOnly=yes"), "{joined}");
    }

    #[test]
    fn local_cmd_is_attach_or_new_as_grok() {
        let cmd = local_tmux_cmd("tmux").expect("cmd");
        assert!(cmd.contains("runuser -u grok"), "{cmd}");
        assert!(cmd.contains("tmux attach -t grok-oss"), "{cmd}");
        assert!(cmd.contains("tmux new -s grok-oss grok-oss"), "{cmd}");
        assert!(!cmd.contains("ssh"), "{cmd}");
        assert!(!cmd.starts_with("tmux "), "root tmux is a miss: {cmd}");
    }

    #[test]
    fn etterminal_parent_means_local_not_ssh() {
        let h = GuestSessionHints {
            parent_etterminal: true,
            ..Default::default()
        };
        assert!(already_on_mail_guest_from(&h));
        let cmd = local_tmux_cmd("tmux").expect("cmd");
        assert!(!cmd.split_whitespace().any(|w| w == "ssh"), "{cmd}");
        assert!(cmd.contains("runuser -u grok"), "{cmd}");
    }

    #[test]
    fn running_json_is_as_user_grok_without_tty_or_bind() {
        let argv = build_running_json_argv("ssh", "root@example.test", None).expect("argv");
        let joined = argv.join(" ");
        let remote = argv.last().expect("remote command");
        assert_eq!(argv[0], "ssh");
        assert!(
            !argv.iter().any(|w| w == "-t"),
            "running --json must not allocate a tty: {joined}"
        );
        assert!(joined.contains("BatchMode=yes"), "{joined}");
        assert!(
            remote.contains("runuser -u grok"),
            "JSON must come from user grok GROK_HOME: {joined}"
        );
        assert!(remote.contains("grok-oss running --json"), "{joined}");
        assert!(
            !remote.contains("tmux"),
            "running JSON is not attach: {joined}"
        );
        assert!(!joined.contains("0.0.0.0"), "{joined}");
        assert!(!joined.contains("listen"), "{joined}");
        assert!(!joined.contains("bind"), "{joined}");
        assert!(!joined.contains(":443"), "{joined}");
        assert!(!joined.contains("dashboard"), "{joined}");
    }
}
