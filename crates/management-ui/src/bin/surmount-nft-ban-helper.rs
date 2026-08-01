//! Least-privilege helper: apply Surmount ban set updates via nft.
//!
//! Product elevation: systemd socket-activated oneshot
//! (`apply-systemd-socket`) with AmbientCapabilities CAP_NET_ADMIN on the
//! **helper unit**. management-ui keeps NoNewPrivileges and talks UDS only
//! (child setcap spawn cannot elevate under NNP).
//!
//! Usage:
//!   surmount-nft-ban-helper ping
//!   surmount-nft-ban-helper add-ban <ip>
//!   surmount-nft-ban-helper remove-ban <ip>         # lab unban; absent = ok
//!   surmount-nft-ban-helper apply-json              # JSON on stdin (tests)
//!   surmount-nft-ban-helper apply-systemd-socket    # connected socket on fd 0
//!
//! Env:
//!   SURMOUNT_BAN_NFT_BIN  absolute path to nft (required for add_ban/remove_ban)
//!
//! Exit 0 + ok JSON on success; non-zero + ok:false on failure.
//! Does not invent Q-ACL policy. Dry-run is a UI concern (UI must not call).

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use surmount_management_ui::ban::ProcessNftExec;
use surmount_management_ui::nft_helper::{
    apply_helper_request, request_from_argv, serve_helper_once, HelperResponse,
};

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().skip(1).collect();
    if argv.is_empty() {
        return fail_print(HelperResponse::failure(
            "usage: surmount-nft-ban-helper ping|add-ban <ip>|remove-ban <ip>|apply-json|apply-systemd-socket",
        ));
    }

    if argv[0] == "apply-systemd-socket" {
        return run_systemd_socket();
    }

    let req = match request_from_argv(&argv) {
        Ok(r) => r,
        Err(e) => return fail_print(HelperResponse::failure(e)),
    };

    // ping needs no nft binary.
    if req.op == "ping" {
        return ok_print(HelperResponse::success());
    }

    let nft_bin = match resolve_nft_bin() {
        Ok(p) => p,
        Err(e) => return fail_print(HelperResponse::failure(e)),
    };

    let exec = ProcessNftExec::new(nft_bin);
    let resp = apply_helper_request(&req, &exec);
    if resp.ok {
        ok_print(resp)
    } else {
        fail_print(resp)
    }
}

fn resolve_nft_bin() -> Result<PathBuf, String> {
    match env::var("SURMOUNT_BAN_NFT_BIN") {
        Ok(s) => {
            let t = s.trim();
            if t.is_empty() {
                return Err("SURMOUNT_BAN_NFT_BIN is empty (fail-closed)".into());
            }
            let p = PathBuf::from(t);
            if !p.is_absolute() {
                return Err(format!(
                    "SURMOUNT_BAN_NFT_BIN must be absolute (fail-closed); got {t:?}"
                ));
            }
            if !p.exists() {
                return Err(format!(
                    "SURMOUNT_BAN_NFT_BIN missing at {} (fail-closed)",
                    p.display()
                ));
            }
            Ok(p)
        }
        Err(_) => Err("SURMOUNT_BAN_NFT_BIN unset (fail-closed); helper cannot run nft".into()),
    }
}

/// systemd Accept=yes: connected client socket is fd 0 (duplex).
fn run_systemd_socket() -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::fd::FromRawFd;
        use std::os::unix::net::UnixStream;

        let nft_bin = match resolve_nft_bin() {
            Ok(p) => p,
            Err(e) => {
                // Best-effort error on the connected socket.
                let _ = write_on_fd0_error(&e);
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        };
        let exec = ProcessNftExec::new(nft_bin);

        // SAFETY: systemd socket Accept=yes passes the connected stream as fd 0.
        // We take ownership for the oneshot lifetime.
        let mut stream = unsafe { UnixStream::from_raw_fd(0) };
        match serve_helper_once(&mut stream, &exec) {
            Ok(resp) if resp.ok => ExitCode::SUCCESS,
            Ok(resp) => {
                if let Some(err) = resp.error {
                    eprintln!("{err}");
                }
                ExitCode::from(1)
            }
            Err(e) => {
                eprintln!("helper socket serve: {e}");
                ExitCode::from(1)
            }
        }
    }
    #[cfg(not(unix))]
    {
        fail_print(HelperResponse::failure(
            "apply-systemd-socket requires unix (fail-closed)",
        ))
    }
}

#[cfg(unix)]
fn write_on_fd0_error(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::fd::FromRawFd;
    use std::os::unix::net::UnixStream;
    // Do not take ownership permanently if we fail before from_raw_fd in main path;
    // here we only use for best-effort write then forget to avoid double-close of
    // the real stdin path if this is called after another take. Only used before
    // from_raw_fd in run_systemd_socket error path for nft bin.
    let mut stream = unsafe { UnixStream::from_raw_fd(0) };
    let line = HelperResponse::failure(msg)
        .to_json_line()
        .unwrap_or_else(|_| r#"{"ok":false,"error":"helper error"}"#.into());
    writeln!(stream, "{line}")?;
    // Leak the fd ownership intentionally: process exits immediately after.
    std::mem::forget(stream);
    Ok(())
}

fn ok_print(resp: HelperResponse) -> ExitCode {
    match resp.to_json_line() {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("helper serialize ok response: {e}");
            ExitCode::from(2)
        }
    }
}

fn fail_print(resp: HelperResponse) -> ExitCode {
    match resp.to_json_line() {
        Ok(line) => {
            println!("{line}");
            if let Some(err) = resp.error {
                eprintln!("{err}");
            }
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("helper serialize error response: {e}");
            ExitCode::from(2)
        }
    }
}
