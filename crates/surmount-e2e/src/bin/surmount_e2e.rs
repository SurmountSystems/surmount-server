//! Local comprehensive end-to-end harness (no NixOS, no root, no secrets in git).
//!
//! Proves the shipped feature matrix Local column via existing cargo contracts.
//! Optional Tor / hidden-service deep row when a service-capable arti and a Tor
//! client path are present; never commits keys or .onion addresses.
//!
//! Usage:
//!   nix run .#e2e
//!   just e2e
//!
//! Env:
//!   SURMOUNT_E2E_TOR=0   force-skip optional Tor deep row
//!   SURMOUNT_E2E_TOR=1   require Tor deep row (fail if tools missing or publish fails)
//!   ARTI_BIN             path to service-capable arti (default: arti on PATH)
//!   TOR_SOCKS            host:port for SOCKS5H (default: 127.0.0.1:9050)
//!   SURMOUNT_E2E_ROOT    repo root override (default: walk from cwd)
//!
//! Honesty: local green is not public cutover, surmount-arti host ownership,
//! or a live kernel firewall drop.

use std::env;
use std::fs;
use std::io::BufRead;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use surmount_e2e::report::{Counters, Status};

/// Critical contract anchors (must remain listed). Package green alone is not
/// enough if an anchor was deleted or renamed away.
const HERMETIC_ANCHORS: &[&str] = &[
    "https_serves_health_over_tls_with_temp_self_signed_pems",
    "https_plus_local_cleartext_api_serves_health_on_both",
    "middleware_returns_429_with_retry_after",
    "middleware_whitelist_bypasses_rate_limit_and_touches_last_used",
    "signal_unauthorized_hook_whitelist_and_enforce_record",
    "surface_audit_404_and_501_do_not_auto_ban",
    // Honest JMAP proxy boundary (501 body + no webmail route).
    "jmap_proxy_returns_honest_501_body",
    // Nostr auth foundation (management-ui bin/lib contracts).
    "auth_mode_off_root_ok_without_cookie",
    "auth_mode_nostr_gates_without_cookie",
    "auth_session_from_nip98_then_me_and_root",
    // Onion display: unset must not invent an address.
    "system_status_onion_null_when_unset",
    "onion_unset_shows_residual_not_invented",
    // Directory honesty: unavailable default, mock label, live wire-mock + fail-closed.
    "unavailable_directory_is_honest_empty",
    "mock_directory_returns_labeled_non_empty_not_stalwart",
    "stalwart_directory_lists_from_jmap_wire_mock",
    "stalwart_directory_fail_closed_empty_on_server_error",
    "stalwart_directory_fail_closed_empty_when_unreachable",
    "accounts_api_respects_live_stalwart_directory",
    "accounts_api_live_directory_unreachable_is_empty_not_invented",
    "auth_mode_nostr_gates_accounts_api_with_directory_injected",
    // Account create/update mutations (shared build_router; auth + CSRF + 503).
    "account_create_auth_off_fail_closed_without_lab_escape",
    "account_create_cookie_auth_requires_csrf",
    "account_create_lab_escape_mock_succeeds",
    "account_create_unavailable_directory_service_unavailable",
    "token_from_file_comment_only_fail_closed",
    "truncate_for_note_utf8_safe_mid_codepoint",
    "admin_shell_ssr_and_health_via_shared_router",
    "helper_client_unix_socket_roundtrip",
    "enforce_helper_refuse_leaves_memory_unbanned",
    "apply_remove_ban_records_delete_element_args",
    "apply_remove_ban_absent_element_is_success",
    "require_files_exist_rejects_world_readable_key",
    "load_rustls_server_config_accepts_self_signed_pems",
];

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let root = match find_repo_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return 1;
        }
    };
    let crates = root.join("crates");
    if !crates.join("Cargo.toml").is_file() {
        eprintln!(
            "error: crates/Cargo.toml not found under {} (set SURMOUNT_E2E_ROOT)",
            root.display()
        );
        return 1;
    }

    println!("==============================================");
    println!(" Surmount local end-to-end (hermetic matrix)");
    println!("==============================================");
    println!("cwd: {}", crates.display());
    println!("Honesty: local green != public cutover / surmount-arti ownership / live kernel drop");
    println!();

    let mut c = Counters::default();
    let mut hermetic_ok = true;

    // --- hermetic cargo + anchors ---
    println!("== hermetic cargo (management-ui feature matrix) ==");
    println!("  note: one package run + named anchors (not five independent filters)");

    let list_out = Command::new("cargo")
        .args(["test", "-p", "surmount-management-ui", "--", "--list"])
        .current_dir(&crates)
        .output();

    let test_list = match list_out {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            s
        }
        Err(e) => {
            c.row(
                Status::Fail,
                "hermetic cargo test --list",
                Some(&format!("cargo not runnable: {e}")),
            );
            hermetic_ok = false;
            String::new()
        }
    };

    let mut anchors_ok = true;
    for anchor in HERMETIC_ANCHORS {
        if !test_list.contains(anchor) {
            c.row(
                Status::Fail,
                &format!("hermetic anchor present: {anchor}"),
                Some("missing from cargo test -- --list"),
            );
            anchors_ok = false;
            hermetic_ok = false;
        }
    }
    if anchors_ok {
        println!(
            "  anchors present: {} critical contracts listed",
            HERMETIC_ANCHORS.len()
        );
    }

    let cargo_status = Command::new("cargo")
        .args(["test", "-p", "surmount-management-ui"])
        .current_dir(&crates)
        .status();

    match cargo_status {
        Ok(st) if st.success() => {
            if anchors_ok {
                c.row(
                    Status::Pass,
                    "hermetic management-ui cargo (matrix A-D contracts; anchors verified)",
                    None,
                );
                println!(
                    "  covers: health+TLS self-signed PEMs, local cleartext API+https dual bind,"
                );
                println!(
                    "          rate-limit/X-Real-IP, ban/helper UDS + remove_ban, surface audit"
                );
                println!("          (404/501 no auto-ban), Nostr auth off/gate/NIP-98 session,");
                println!(
                    "          onion unset residual, unauthorized BanCandidate stub, Leptos SSR"
                );
                println!(
                    "  E preflight (Nix Arti config shape) stays under just check / module-eval"
                );
            } else {
                c.row(
                    Status::Fail,
                    "hermetic management-ui cargo",
                    Some("package green but required anchors missing"),
                );
            }
        }
        Ok(_) => {
            hermetic_ok = false;
            c.row(
                Status::Fail,
                "hermetic management-ui cargo (matrix A-D)",
                Some("cargo test -p surmount-management-ui"),
            );
        }
        Err(e) => {
            hermetic_ok = false;
            c.row(
                Status::Fail,
                "hermetic management-ui cargo (matrix A-D)",
                Some(&format!("cargo spawn failed: {e}")),
            );
        }
    }

    // --- pure helpers (Rust unit tests; replaces bash pure lib) ---
    println!();
    println!("== hermetic e2e pure helpers (Rust) ==");
    let pure_status = Command::new("cargo")
        .args(["test", "-p", "surmount-e2e", "--lib"])
        .current_dir(&crates)
        .status();
    match pure_status {
        Ok(st) if st.success() => {
            c.row(
                Status::Pass,
                "e2e pure helpers (hostport/CAP/lab-ip parse; no VPS)",
                None,
            );
        }
        Ok(_) => {
            hermetic_ok = false;
            c.row(
                Status::Fail,
                "e2e pure helpers",
                Some("cargo test -p surmount-e2e --lib"),
            );
        }
        Err(e) => {
            hermetic_ok = false;
            c.row(
                Status::Fail,
                "e2e pure helpers",
                Some(&format!("cargo spawn failed: {e}")),
            );
        }
    }

    // --- optional Tor deep row ---
    println!();
    println!("== optional: local Tor hidden-service deep row ==");
    let tor_mode = env::var("SURMOUNT_E2E_TOR").unwrap_or_else(|_| "try".into());
    let mut tor_rc_ok = true;

    match tor_mode.as_str() {
        "0" | "false" | "off" | "no" => {
            c.row(
                Status::Skip,
                "E. Local Tor publish+fetch",
                Some(&format!("SURMOUNT_E2E_TOR={tor_mode}")),
            );
        }
        _ => {
            if !run_tor_deep(&crates, &tor_mode, &mut c) {
                tor_rc_ok = false;
            }
        }
    }

    println!();
    println!("==============================================");
    println!(" Local end-to-end summary");
    println!("==============================================");
    println!("  pass={}  fail={}  skip={}", c.pass, c.fail, c.skip);
    println!(
        "  required hermetic: {}",
        if hermetic_ok { "OK" } else { "FAILED" }
    );
    println!(
        "  optional Tor deep: {}",
        if tor_rc_ok { "OK_OR_SKIP" } else { "FAILED" }
    );
    println!();
    println!("Pins retained:");
    println!("  - self-signed certificate/key files are correct for local HTTPS e2e");
    println!("  - local Tor temp keys != production surmount-arti ownership / cutover");
    println!("  - no live kernel firewall drop claimed here (host: nix run .#e2e-host)");
    println!("  - just check remains CI quality bar (module-eval); not replaced by e2e");
    println!("  - hermetic rows are package green + listed anchors, not five independent filters");
    println!();

    if !hermetic_ok || !tor_rc_ok {
        return 1;
    }
    0
}

fn find_repo_root() -> Result<PathBuf, String> {
    if let Ok(r) = env::var("SURMOUNT_E2E_ROOT") {
        let p = PathBuf::from(r);
        if p.join("flake.nix").is_file() || p.join("crates/Cargo.toml").is_file() {
            return Ok(p);
        }
        return Err(format!(
            "SURMOUNT_E2E_ROOT={} does not look like repo root",
            p.display()
        ));
    }
    let mut cur = env::current_dir().map_err(|e| format!("cwd: {e}"))?;
    loop {
        if cur.join("flake.nix").is_file() && cur.join("crates/Cargo.toml").is_file() {
            return Ok(cur);
        }
        // Also accept crates/ as cwd (common after cd crates && cargo test).
        if cur.file_name().and_then(|s| s.to_str()) == Some("crates")
            && cur.join("Cargo.toml").is_file()
            && cur
                .parent()
                .map(|p| p.join("flake.nix").is_file())
                .unwrap_or(false)
        {
            return Ok(cur.parent().unwrap().to_path_buf());
        }
        if !cur.pop() {
            break;
        }
    }
    Err(
        "could not find repo root (flake.nix + crates/Cargo.toml); set SURMOUNT_E2E_ROOT or run from the tree"
            .into(),
    )
}

fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr().map(|a| a.port()))
        .unwrap_or(18765)
}

fn which(bin: &str) -> Option<PathBuf> {
    env::var_os("PATH").and_then(|paths| {
        for dir in env::split_paths(&paths) {
            let p = dir.join(bin);
            if p.is_file() {
                return Some(p);
            }
        }
        None
    })
}

fn tcp_open(host: &str, port: u16) -> bool {
    use std::net::ToSocketAddrs;
    let addr = format!("{host}:{port}");
    let Ok(mut addrs) = addr.to_socket_addrs() else {
        return false;
    };
    let Some(a) = addrs.next() else {
        return false;
    };
    std::net::TcpStream::connect_timeout(&a, Duration::from_secs(1)).is_ok()
}

/// Never print raw .onion addresses.
fn redact_onion_tail(file: &Path, lines: usize) {
    let Ok(f) = fs::File::open(file) else {
        return;
    };
    let reader = std::io::BufReader::new(f);
    let all: Vec<String> = reader.lines().map_while(Result::ok).collect();
    let start = all.len().saturating_sub(lines);
    for line in &all[start..] {
        let redacted = redact_onion_line(line);
        eprintln!("{redacted}");
    }
}

fn redact_onion_line(line: &str) -> String {
    // v3 onion: 56 base32 + .onion; also shorter legacy forms.
    let mut out = line.to_string();
    // Simple replace without regex crate: scan for .onion suffixes.
    while let Some(idx) = out.find(".onion") {
        let end = idx + ".onion".len();
        let before = &out[..idx];
        let start = before
            .rfind(|c: char| !matches!(c, 'a'..='z' | '2'..='7'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let candidate = &before[start..];
        if candidate.len() == 56 || candidate.len() == 16 {
            out = format!("{}<onion-redacted>{}", &out[..start], &out[end..]);
        } else {
            // Advance past this .onion to avoid infinite loop.
            out = format!(
                "{}{}",
                &out[..end.min(out.len())],
                if end < out.len() { &out[end..] } else { "" }
            );
            // Force progress: replace this occurrence with placeholder if stuck.
            break;
        }
    }
    out
}

struct TorSession {
    arti_child: Option<std::process::Child>,
    backend_child: Option<std::process::Child>,
    tmp: Option<PathBuf>,
}

impl Drop for TorSession {
    fn drop(&mut self) {
        if let Some(mut c) = self.arti_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(mut c) = self.backend_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if let Some(tmp) = self.tmp.take() {
            let _ = fs::remove_dir_all(tmp);
        }
    }
}

fn run_tor_deep(crates: &Path, tor_mode: &str, c: &mut Counters) -> bool {
    let require = tor_mode == "1";

    let arti_bin = env::var("ARTI_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| which("arti"));

    let Some(arti_bin) = arti_bin.filter(|p| p.is_file()) else {
        c.row(
            Status::Skip,
            "E. Local Tor publish+fetch",
            Some("no service-capable arti (set ARTI_BIN or install artiOnionService)"),
        );
        return !require;
    };

    if which("curl").is_none() {
        c.row(
            Status::Skip,
            "E. Local Tor publish+fetch",
            Some("curl not found"),
        );
        return !require;
    }

    let socks = env::var("TOR_SOCKS").unwrap_or_else(|_| "127.0.0.1:9050".into());
    let (socks_host, socks_port_s) = socks
        .rsplit_once(':')
        .map(|(h, p)| (h.to_string(), p.to_string()))
        .unwrap_or_else(|| ("127.0.0.1".into(), "9050".into()));
    let socks_port: u16 = socks_port_s.parse().unwrap_or(9050);

    if !tcp_open(&socks_host, socks_port) {
        c.row(
            Status::Skip,
            "E. Local Tor publish+fetch",
            Some(&format!(
                "no Tor SOCKS at {socks} (start tor or set TOR_SOCKS)"
            )),
        );
        return !require;
    }

    let mut session = TorSession {
        arti_child: None,
        backend_child: None,
        tmp: None,
    };

    let tmp = env::temp_dir().join(format!("surmount-e2e-tor.{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    if fs::create_dir_all(tmp.join("hs")).is_err() || fs::create_dir_all(tmp.join("cache")).is_err()
    {
        c.row(
            Status::Fail,
            "E. Local Tor publish+fetch",
            Some("could not create temp dir"),
        );
        return false;
    }
    session.tmp = Some(tmp.clone());

    let ui_bin = crates.join("target/debug/surmount-management-ui");
    if !ui_bin.is_file() {
        println!("building management-ui binary for Tor cleartext backend...");
        let st = Command::new("cargo")
            .args([
                "build",
                "-p",
                "surmount-management-ui",
                "--bin",
                "surmount-management-ui",
            ])
            .current_dir(crates)
            .status();
        match st {
            Ok(s) if s.success() => {}
            _ => {
                c.row(
                    Status::Fail,
                    "E. Local Tor publish+fetch",
                    Some("cargo build management-ui failed"),
                );
                return false;
            }
        }
    }

    let backend_port = pick_free_port();
    let backend_log = tmp.join("backend.log");
    let backend_log_file = match fs::File::create(&backend_log) {
        Ok(f) => f,
        Err(e) => {
            c.row(
                Status::Fail,
                "E. Local Tor publish+fetch",
                Some(&format!("backend log: {e}")),
            );
            return false;
        }
    };
    let backend_err = backend_log_file
        .try_clone()
        .unwrap_or_else(|_| fs::File::create("/dev/null").expect("null"));

    let backend = Command::new(&ui_bin)
        .env("SURMOUNT_LISTEN", format!("127.0.0.1:{backend_port}"))
        .env("SURMOUNT_LISTEN_MODE", "http")
        .env("SURMOUNT_RATE_LIMIT_MAX", "0")
        .env("SURMOUNT_BAN_ENFORCEMENT", "off")
        .stdout(Stdio::from(backend_log_file))
        .stderr(Stdio::from(backend_err))
        .spawn();

    match backend {
        Ok(child) => {
            session.backend_child = Some(child);
        }
        Err(e) => {
            c.row(
                Status::Fail,
                "E. Local Tor publish+fetch",
                Some(&format!("backend spawn: {e}")),
            );
            return false;
        }
    }

    let deadline = Instant::now() + Duration::from_secs(15);
    let mut backend_ok = false;
    while Instant::now() < deadline {
        let health = Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                "1",
                &format!("http://127.0.0.1:{backend_port}/health"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if health.map(|s| s.success()).unwrap_or(false) {
            backend_ok = true;
            break;
        }
        if let Some(ref mut b) = session.backend_child
            && let Ok(Some(_)) = b.try_wait()
        {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    if !backend_ok {
        c.row(
            Status::Fail,
            "E. Local Tor publish+fetch",
            Some("cleartext backend did not serve /health"),
        );
        eprintln!("---- backend.log (tail, onion-redacted) ----");
        redact_onion_tail(&backend_log, 40);
        return false;
    }

    let arti_socks_port = pick_free_port();
    let arti_toml = tmp.join("arti.toml");
    let toml = format!(
        r#"# Generated by surmount-e2e. Temp only; never commit.
[application]
watch_configuration = false

[proxy]
socks_listen = "127.0.0.1:{arti_socks_port}"

[storage]
cache_dir = "{cache}"
state_dir = "{hs}"

[storage.permissions]
trust_user = ":current"

[onion_services."surmount-e2e"]
proxy_ports = [
    ["80", "127.0.0.1:{backend_port}"],
    ["*", "destroy"]
]
"#,
        cache = tmp.join("cache").display(),
        hs = tmp.join("hs").display(),
    );
    if fs::write(&arti_toml, toml).is_err() {
        c.row(
            Status::Fail,
            "E. Local Tor publish+fetch",
            Some("write arti.toml failed"),
        );
        return false;
    }

    let arti_log = tmp.join("arti.log");
    let arti_log_file = match fs::File::create(&arti_log) {
        Ok(f) => f,
        Err(e) => {
            c.row(
                Status::Fail,
                "E. Local Tor publish+fetch",
                Some(&format!("arti log: {e}")),
            );
            return false;
        }
    };
    let arti_err = arti_log_file
        .try_clone()
        .unwrap_or_else(|_| fs::File::create("/dev/null").expect("null"));

    let arti = Command::new(&arti_bin)
        .args(["proxy", "-c"])
        .arg(&arti_toml)
        .stdout(Stdio::from(arti_log_file))
        .stderr(Stdio::from(arti_err))
        .spawn();

    let arti = match arti {
        Ok(c) => c,
        Err(e) => {
            c.row(
                Status::Fail,
                "E. Local Tor publish+fetch",
                Some(&format!("arti spawn: {e}")),
            );
            return false;
        }
    };
    session.arti_child = Some(arti);

    let mut onion = String::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        if let Some(ref mut a) = session.arti_child
            && let Ok(Some(_)) = a.try_wait()
        {
            c.row(
                Status::Fail,
                "E. Local Tor publish+fetch",
                Some("arti exited early (may lack onion-service-service feature)"),
            );
            eprintln!("---- arti.log (tail, onion-redacted) ----");
            redact_onion_tail(&arti_log, 60);
            return false;
        }
        onion = find_onion(&tmp.join("hs"), &arti_log);
        if onion.ends_with(".onion") {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }

    if !onion.ends_with(".onion") {
        c.row(
            Status::Fail,
            "E. Local Tor publish+fetch",
            Some("no .onion after wait (process up != published)"),
        );
        eprintln!("---- arti.log (tail, onion-redacted) ----");
        redact_onion_tail(&arti_log, 60);
        return false;
    }

    let fetch_url = format!("http://{onion}/health");
    // Drop onion from shell state before further messaging.
    drop(onion);

    let mut fetch_ok = false;
    let deadline = Instant::now() + Duration::from_secs(90);
    while Instant::now() < deadline {
        let body = tmp.join("fetch.body");
        let ok1 = curl_socks_fetch(&socks_host, &socks_port_s, &fetch_url, &body);
        let ok2 = curl_socks_fetch("127.0.0.1", &arti_socks_port.to_string(), &fetch_url, &body);
        if ok1 || ok2 {
            fetch_ok = true;
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }

    if fetch_ok {
        c.row(
            Status::Pass,
            "E. Local Tor publish+fetch (temp HS keys; not surmount-arti host ownership)",
            None,
        );
        println!("  note: onion fetch succeeded; address not printed; temp keys destroyed on drop");
        return true;
    }

    c.row(
        Status::Fail,
        "E. Local Tor publish+fetch",
        Some("onion seen but /health fetch via SOCKS failed"),
    );
    eprintln!("---- arti.log (tail, onion-redacted) ----");
    redact_onion_tail(&arti_log, 40);
    false
}

fn curl_socks_fetch(socks_host: &str, socks_port: &str, url: &str, body: &Path) -> bool {
    let st = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "10",
            "--socks5-hostname",
            &format!("{socks_host}:{socks_port}"),
            url,
            "-o",
        ])
        .arg(body)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    st.map(|s| s.success()).unwrap_or(false)
        && fs::metadata(body).map(|m| m.len() > 0).unwrap_or(false)
}

fn find_onion(hs_dir: &Path, arti_log: &Path) -> String {
    if let Ok(walker) = walk_files(hs_dir) {
        for f in walker {
            let name = f.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if (name == "hostname" || name.ends_with(".onion"))
                && let Ok(s) = fs::read_to_string(&f)
            {
                let t = s.trim().to_string();
                if t.ends_with(".onion") {
                    return t;
                }
            }
        }
    }
    // Fallback: scan arti.log for v3 onion pattern without printing it.
    if let Ok(text) = fs::read_to_string(arti_log) {
        for word in text.split_whitespace() {
            let w = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.');
            if w.len() == 56 + 6 && w.ends_with(".onion") {
                let base = &w[..56];
                if base.chars().all(|c| matches!(c, 'a'..='z' | '2'..='7')) {
                    return w.to_string();
                }
            }
        }
    }
    String::new()
}

fn walk_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    fn rec(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for ent in fs::read_dir(dir)? {
            let ent = ent?;
            let p = ent.path();
            if p.is_dir() {
                rec(&p, out)?;
            } else {
                out.push(p);
            }
        }
        Ok(())
    }
    rec(dir, &mut out)?;
    Ok(out)
}
