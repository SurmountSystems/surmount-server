//! Eternal Terminal client argv. Reconnects after sleep/network change.
//! Mullvad stays operator-owned. For root@ targets, pass IdentityFile so
//! Host surmount-1 nixbuilder IdentitiesOnly still reaches root.
//! Nested `just btop` on the mail guest must run local btop, not et to itself.

use std::env;
use std::io::{self, IsTerminal};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::ProbeError;

pub fn target_user_is_root(target: &str) -> bool {
    let t = target.trim();
    t == "root" || t.starts_with("root@")
}

pub fn operator_ssh_identity(target: &str) -> Option<PathBuf> {
    if let Ok(p) = env::var("SURMOUNT_DEPLOY_SSH_IDENTITY") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    if !target_user_is_root(target) {
        return None;
    }
    let home = env::var("HOME").ok()?;
    let id = PathBuf::from(home).join(".ssh/id_ed25519");
    if id.is_file() { Some(id) } else { None }
}

/// Build `et` argv. Does not spawn. Keepalives via et -k 5 (client max).
pub fn build_et_argv(
    et_bin: &str,
    target: &str,
    extra: &[String],
    identity: Option<&Path>,
) -> Vec<String> {
    let mut v = vec![
        et_bin.to_string(),
        "-k".to_string(),
        "5".to_string(),
        "-p".to_string(),
        "2022".to_string(),
    ];
    if let Some(id) = identity {
        v.push("--ssh-option".to_string());
        v.push(format!("IdentityFile={}", id.display()));
    }
    v.extend(extra.iter().cloned());
    v.push(target.to_string());
    v
}

/// Extra et flags for `just btop`.
/// Packaged et 6.2.11: `-e` / `--noexit` with `-c` leaves a shell.
pub fn btop_et_extra() -> Vec<String> {
    vec!["-e".into(), "-c".into(), "btop".into()]
}

/// Mail-host Eternal Terminal target already used by `just deploy`.
/// Not a second host. Port and identity stay in `build_et_argv`.
pub const BTOP_ET_TARGET: &str = "root@surmount-1";

/// Laptop Eternal Terminal client. `command -v et` prints this path
/// when PATH is `$HOME/.nix-profile/bin:/usr/bin`.
/// The recipe does not nix-run, nix-build, or copy a binary to the mail host.
pub const BTOP_ET_CLIENT: &str = "$HOME/.nix-profile/bin/et";

/// Shell default for the root identity `operator_ssh_identity` already
/// selects: `SURMOUNT_DEPLOY_SSH_IDENTITY`, else `$HOME/.ssh/id_ed25519`.
pub const BTOP_ET_IDENTITY_SHELL: &str = "${SURMOUNT_DEPLOY_SSH_IDENTITY:-$HOME/.ssh/id_ed25519}";

/// Laptop `just btop` argv. The program is `$HOME/.nix-profile/bin/et`.
/// Does not nix-run, nix-build, or copy a store path.
/// Nested guest detection stays in `already_on_mail_guest`. This argv
/// is only the laptop launch.
pub fn btop_laptop_et_argv() -> Vec<String> {
    build_et_argv(
        BTOP_ET_CLIENT,
        BTOP_ET_TARGET,
        &btop_et_extra(),
        Some(Path::new(BTOP_ET_IDENTITY_SHELL)),
    )
}

#[derive(Debug, Clone, Default)]
pub struct GuestSessionHints {
    pub force_local: Option<bool>,
    pub parent_etterminal: bool,
    pub guest_marker: bool,
    pub hostname: String,
    pub guest_hostnames: Vec<String>,
}

pub fn env_is_off(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref().map(str::trim),
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("NO")
    )
}

pub fn session_force_local() -> Option<bool> {
    match env::var("SURMOUNT_BTOP_SESSION")
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some("local") | Some("guest") => Some(true),
        Some("remote") | Some("laptop") => Some(false),
        _ => None,
    }
}

pub fn guest_marker_present() -> bool {
    match env::var("SURMOUNT_GUEST_MARKER").ok() {
        Some(v) if v.is_empty() || v == "0" => false,
        Some(v) if v == "1" => true,
        Some(v) => Path::new(&v).is_file(),
        None => Path::new("/etc/surmount/root-justfile").is_file(),
    }
}

pub fn hostname_string() -> String {
    if let Ok(h) = env::var("SURMOUNT_TEST_HOSTNAME") {
        return h.trim().to_string();
    }
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn default_guest_hostnames() -> Vec<String> {
    let mut names = vec!["surmount-1".into(), "mail-vps".into()];
    if let Ok(extra) = env::var("SURMOUNT_MAIL_GUEST_HOSTNAMES") {
        for n in extra.split(',') {
            let n = n.trim();
            if !n.is_empty() {
                names.push(n.to_string());
            }
        }
    }
    names
}

pub fn hostname_is_mail_guest(hostname: &str, names: &[String]) -> bool {
    let h = hostname.trim();
    if h.is_empty() {
        return false;
    }
    names.iter().any(|n| n.eq_ignore_ascii_case(h))
}

fn comm_is_etterminal(comm: &str) -> bool {
    comm.trim().trim_end_matches('\n').trim_matches('\0') == "etterminal"
}

fn ppid_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.rsplit_once(')')?.1;
    let mut fields = rest.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

pub fn ancestor_is_etterminal() -> bool {
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..24 {
        if pid <= 1 {
            return false;
        }
        if let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
            if comm_is_etterminal(&comm) {
                return true;
            }
        }
        match ppid_of(pid) {
            Some(p) if p != pid => pid = p,
            _ => return false,
        }
    }
    false
}

pub fn live_hints() -> GuestSessionHints {
    GuestSessionHints {
        force_local: session_force_local(),
        parent_etterminal: ancestor_is_etterminal(),
        guest_marker: guest_marker_present(),
        hostname: hostname_string(),
        guest_hostnames: default_guest_hostnames(),
    }
}

/// True when this process is already on the mail guest.
/// Nested `just btop` must run local btop, not Eternal Terminal to itself.
/// SSH on a laptop is not enough; require the guest marker, hostname, or
/// an etterminal ancestor.
pub fn already_on_mail_guest_from(h: &GuestSessionHints) -> bool {
    if let Some(v) = h.force_local {
        return v;
    }
    if hostname_is_mail_guest(&h.hostname, &h.guest_hostnames) {
        return true;
    }
    if h.parent_etterminal {
        return true;
    }
    if h.guest_marker {
        return true;
    }
    false
}

pub fn already_on_mail_guest() -> bool {
    already_on_mail_guest_from(&live_hints())
}

pub fn has_live_tty() -> bool {
    if env_is_off("SURMOUNT_BTOP_REQUIRE_TTY") {
        return true;
    }
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

pub fn refuse_without_live_tty() -> Result<(), ProbeError> {
    if has_live_tty() {
        return Ok(());
    }
    Err(ProbeError::fail(
        "refusing to attach without a live tty (stdin and stdout must be a terminal). A frozen last-paint pane is not live. If the clock in a guest btop pane stopped, press q, exit the remote shell, then run just btop from the laptop.",
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EtPortStatus {
    pub tcp22: bool,
    pub tcp2022: bool,
}

pub fn host_from_target(target: &str) -> &str {
    target
        .rsplit_once('@')
        .map(|(_, h)| h)
        .unwrap_or(target)
        .trim()
}

fn env_port_override(name: &str) -> Option<bool> {
    match env::var(name).ok().as_deref().map(str::trim) {
        Some("1") | Some("true") | Some("TRUE") | Some("up") | Some("answers") => Some(true),
        Some("0") | Some("false") | Some("FALSE") | Some("down") => Some(false),
        _ => None,
    }
}

fn probe_tcp_host_port(host: &str, port: u16) -> bool {
    let labeled = format!("{host}:{port}");
    let Ok(addrs) = labeled.to_socket_addrs() else {
        return false;
    };
    for addr in addrs {
        if TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok() {
            return true;
        }
    }
    false
}

pub fn tcp_port_answers(host: &str, port: u16) -> bool {
    let env_name = match port {
        22 => "SURMOUNT_ET_STATUS_TCP22",
        2022 => "SURMOUNT_ET_STATUS_TCP2022",
        _ => "",
    };
    if !env_name.is_empty() {
        if let Some(v) = env_port_override(env_name) {
            return v;
        }
    }
    probe_tcp_host_port(host, port)
}

pub fn collect_et_status(target: &str) -> EtPortStatus {
    let host = host_from_target(target);
    EtPortStatus {
        tcp22: tcp_port_answers(host, 22),
        tcp2022: tcp_port_answers(host, 2022),
    }
}

/// Status lines for TCP 22 and 2022. Never includes a host address.
pub fn format_et_status(st: EtPortStatus) -> String {
    fn line(port: u16, up: bool) -> String {
        if up {
            format!("et-status: TCP {port} answers")
        } else {
            format!("et-status: TCP {port} does not answer")
        }
    }
    let mut out = format!("{}\n{}", line(22, st.tcp22), line(2022, st.tcp2022));
    if !st.tcp2022 {
        out.push_str("\net-status: use SSH/tmux (just grok-oss)");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn root_target_gets_identity_file() {
        let id = Path::new("/tmp/id_ed25519");
        let argv = build_et_argv("et", "root@example.test", &[], Some(id));
        let joined = argv.join(" ");
        assert!(joined.contains("--ssh-option"), "{joined}");
        assert!(joined.contains("IdentityFile=/tmp/id_ed25519"), "{joined}");
        assert!(joined.contains("-k 5"), "{joined}");
        assert!(joined.contains("root@example.test"), "{joined}");
    }

    #[test]
    fn nixbuilder_target_skips_forced_identity() {
        let argv = build_et_argv("et", "nixbuilder@example.test", &[], None);
        let joined = argv.join(" ");
        assert!(!joined.contains("IdentityFile="), "{joined}");
        assert!(joined.contains("nixbuilder@example.test"), "{joined}");
    }

    #[test]
    fn btop_extra_runs_btop_then_leaves_shell() {
        let extra = btop_et_extra();
        assert_eq!(
            extra,
            vec!["-e".to_string(), "-c".to_string(), "btop".to_string()]
        );
    }

    fn deploy_host_btop_source() -> &'static str {
        include_str!("../../../crates/surmount-deploy-host/src/recipe_batch_03.rs")
    }

    fn run_btop_arm(src: &str) -> &str {
        let start = src
            .find("fn run_btop(")
            .expect("recipe_batch_03.rs run_btop arm");
        let rest = &src[start..];
        let end = rest[1..].find("\nfn ").map(|i| i + 1).unwrap_or(rest.len());
        &rest[..end]
    }

    #[test]
    fn laptop_btop_arm_matches_et_argv() {
        let extra = btop_et_extra();
        let argv = build_et_argv(BTOP_ET_CLIENT, "root@surmount-1", &extra, None);
        let src = deploy_host_btop_source();
        let arm = run_btop_arm(src);
        assert!(
            arm.contains(".nix-profile/bin/et"),
            "run_btop does not exec the Eternal Terminal client:\n{arm}"
        );
        assert!(
            arm.contains("run_status(&et"),
            "run_btop does not pass et to the process exec:\n{arm}"
        );
        assert!(
            src.contains("Command::new(bin)"),
            "run_status does not exec the et program"
        );
        assert!(!arm.contains("nix run"), "{arm}");
        assert!(!arm.contains("nix build"), "{arm}");
        assert!(!arm.contains("/nix/store"), "{arm}");
        for token in &argv {
            if token == BTOP_ET_CLIENT || token == "root@surmount-1" {
                continue;
            }
            assert!(
                arm.contains(token.as_str()),
                "run_btop missing {token} from real argv {argv:?}\n{arm}"
            );
        }
        assert!(arm.contains("root@surmount-1"), "{arm}");
    }

    fn shell_join_argv(argv: &[String]) -> String {
        argv.iter()
            .map(|arg| {
                let plain = arg.chars().all(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '/' | ':')
                });
                if plain {
                    arg.clone()
                } else {
                    format!("\"{arg}\"")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn btop_arm_execs_et_not_nix() {
        let argv = btop_laptop_et_argv();
        let via_builder = build_et_argv(
            BTOP_ET_CLIENT,
            BTOP_ET_TARGET,
            &btop_et_extra(),
            Some(Path::new(BTOP_ET_IDENTITY_SHELL)),
        );
        assert_eq!(argv, via_builder);
        assert_eq!(
            argv.first().map(String::as_str),
            Some(BTOP_ET_CLIENT),
            "{argv:?}"
        );
        assert!(
            argv.windows(3)
                .any(|w| w[0] == "-e" && w[1] == "-c" && w[2] == "btop"),
            "{argv:?}"
        );
        assert!(argv.iter().any(|a| a == "-p"), "{argv:?}");
        assert!(argv.iter().any(|a| a == "2022"), "{argv:?}");
        assert!(argv.iter().any(|a| a == "--ssh-option"), "{argv:?}");
        assert!(
            argv.iter().any(|a| a.starts_with("IdentityFile=")),
            "{argv:?}"
        );
        assert!(argv.iter().any(|a| a == BTOP_ET_TARGET), "{argv:?}");
        let joined = argv.join(" ");
        assert!(!joined.contains("nix run"), "{joined}");
        assert!(!joined.contains("nix build"), "{joined}");
        assert!(!joined.contains("nix copy"), "{joined}");
        assert!(!joined.contains("/nix/store"), "{joined}");

        let src = deploy_host_btop_source();
        let arm = run_btop_arm(src);
        assert!(
            arm.contains(".nix-profile/bin/et"),
            "run_btop has no et client exec:\n{arm}"
        );
        assert!(
            arm.contains("run_status(&et"),
            "run_btop does not exec et:\n{arm}"
        );
        assert!(arm.contains("\"-e\""), "{arm}");
        assert!(arm.contains("\"-c\""), "{arm}");
        assert!(arm.contains("\"btop\""), "{arm}");
        assert!(arm.contains("\"-p\""), "{arm}");
        assert!(arm.contains("\"2022\""), "{arm}");
        assert!(arm.contains("\"--ssh-option\""), "{arm}");
        assert!(arm.contains("IdentityFile="), "{arm}");
        assert!(arm.contains("root@surmount-1"), "{arm}");
        assert!(!arm.contains("nix run"), "{arm}");
        assert!(!arm.contains("nix build"), "{arm}");
        assert!(!arm.contains("nix copy"), "{arm}");
        assert!(!arm.contains("/nix/store"), "{arm}");
        assert!(!arm.contains("surmount-btop-host"), "{arm}");
        let just = include_str!("../../../justfile");
        assert!(
            !just.contains("nix run \".#surmount-btop-host\""),
            "just btop must not nix-run the host package"
        );
        let rendered = shell_join_argv(&argv);
        // Visible in the remote test log (--show-output). -e is et --noexit.
        eprintln!("BTOP_LAUNCH_ARGV={rendered}");
        eprintln!("BTOP_LAUNCH_NOEXIT_FLAG=-e");
        eprintln!("BTOP_LAUNCH_REMOTE_COMMAND=-c btop");
        eprintln!("BTOP_LAUNCH_COPY=absent");
    }

    #[test]
    fn force_local_wins() {
        let h = GuestSessionHints {
            force_local: Some(true),
            hostname: "laptop".into(),
            ..Default::default()
        };
        assert!(already_on_mail_guest_from(&h));
    }

    #[test]
    fn force_remote_wins_over_marker() {
        let h = GuestSessionHints {
            force_local: Some(false),
            guest_marker: true,
            parent_etterminal: true,
            hostname: "surmount-1".into(),
            guest_hostnames: vec!["surmount-1".into()],
        };
        assert!(!already_on_mail_guest_from(&h));
    }

    #[test]
    fn marker_means_guest() {
        let h = GuestSessionHints {
            guest_marker: true,
            ..Default::default()
        };
        assert!(already_on_mail_guest_from(&h));
    }

    #[test]
    fn empty_hints_are_not_guest() {
        let h = GuestSessionHints::default();
        assert!(!already_on_mail_guest_from(&h));
    }

    #[test]
    fn hostname_surmount_1() {
        let h = GuestSessionHints {
            hostname: "surmount-1".into(),
            guest_hostnames: vec!["surmount-1".into(), "mail-vps".into()],
            ..Default::default()
        };
        assert!(already_on_mail_guest_from(&h));
    }

    #[test]
    fn etterminal_parent_means_guest() {
        let h = GuestSessionHints {
            parent_etterminal: true,
            ..Default::default()
        };
        assert!(already_on_mail_guest_from(&h));
    }

    #[test]
    fn status_omits_address_when_2022_down() {
        let text = format_et_status(EtPortStatus {
            tcp22: true,
            tcp2022: false,
        });
        assert!(text.contains("TCP 22 answers"), "{text}");
        assert!(text.contains("TCP 2022 does not answer"), "{text}");
        assert!(text.contains("just grok-oss"), "{text}");
        assert!(!text.contains("192.0.2.10"), "{text}");
        assert!(!text.contains('@'), "{text}");
    }

    #[test]
    fn status_both_up_does_not_push_grok_oss() {
        let text = format_et_status(EtPortStatus {
            tcp22: true,
            tcp2022: true,
        });
        assert!(text.contains("TCP 22 answers"), "{text}");
        assert!(text.contains("TCP 2022 answers"), "{text}");
        assert!(!text.contains("just grok-oss"), "{text}");
    }

    #[test]
    fn host_from_target_strips_user() {
        assert_eq!(host_from_target("root@example.test"), "example.test");
        assert_eq!(host_from_target("surmount-1"), "surmount-1");
    }
}
