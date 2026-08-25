use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::ToolError;

pub const SMOKE_USAGE: &str = r#"Usage:
  surmount-deploy-host-post-switch-smoke [--help]

Post-switch smoke (intended to run on the NixOS host after nixos-rebuild).
Checks:
  - current system generation (report /run/current-system)
  - systemctl is-active: sshd, stalwart-mail, surmount-management-ui
  - if UI inactive: systemctl start surmount-management-ui
  - curl loopback /health (day-1 default http://127.0.0.1:8090/health;
    port may follow SURMOUNT_LISTEN when it is a loopback/bind-all host:port.
    When SURMOUNT_LISTEN_MODE=https and the listen port is 443, curl
    https://127.0.0.1:443/health with -sk; never HTTP on :443)
  - TLS PEM path presence when https edge or SURMOUNT_TLS_* is configured
    (durable default /var/lib/surmount/secrets/tls/{cert,key}.pem; never logs
    PEM contents)
  - optional ACME enable note from SURMOUNT_ACME_ENABLE (disabled = static PEMs)
  - when https edge: ss listen proof for :443 (always required) and :80 when
    HTTP->HTTPS redirect is on (product production default for https edge).
    Soft NOTE (not FAIL) when unit env shows redirect clearly off
    (SURMOUNT_REDIRECT_HTTP_TO_HTTPS=false) or smoke override forces off.

Never invents a public hostname or public IP for health. Prints clear
PASS:/FAIL: lines. Exit 0 only when required checks pass. Never prints
secret values (tokens, PEM bodies, account JSON).

Env (test / override only; not for production secrets):
  SURMOUNT_SMOKE_SYSTEMCTL   override systemctl binary (hermetic tests)
  SURMOUNT_SMOKE_CURL        override curl binary (hermetic tests)
  SURMOUNT_SMOKE_READLINK    override readlink binary (hermetic tests)
  SURMOUNT_SMOKE_SS          override ss binary (hermetic tests)
  SURMOUNT_SMOKE_SKIP_HEALTH=1  skip /health curl (report SKIP)
  SURMOUNT_SMOKE_SKIP_PEM=1     skip TLS PEM presence checks
  SURMOUNT_SMOKE_SKIP_LISTEN=1  skip :80/:443 listen checks
  SURMOUNT_SMOKE_TLS_CERT / SURMOUNT_SMOKE_TLS_KEY
                            override PEM paths (hermetic; labels only)
  SURMOUNT_SMOKE_REQUIRE_PUBLIC_LISTEN=1
                            force :443 listen checks even without https env
                            (:80 still follows redirect on/off below)
  SURMOUNT_SMOKE_REDIRECT_HTTP_TO_HTTPS=true|false
                            override unit redirect flag for :80 require/soft
"#;

const DEFAULT_TLS_CERT: &str = "/var/lib/surmount/secrets/tls/cert.pem";
const DEFAULT_TLS_KEY: &str = "/var/lib/surmount/secrets/tls/key.pem";

pub fn run_smoke<I, S>(args: I) -> Result<(), ToolError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    for a in iter {
        match a.as_ref() {
            "-h" | "--help" => {
                print!("{SMOKE_USAGE}");
                return Ok(());
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
    }
    smoke_body()
}

fn env_tool(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn skip_flag(key: &str) -> bool {
    matches!(env::var(key).as_deref(), Ok("1"))
}

struct Counters {
    fail: i32,
}

impl Counters {
    fn pass(&mut self, msg: &str) {
        println!("PASS: {msg}");
    }
    fn fail_line(&mut self, msg: &str) {
        eprintln!("FAIL: {msg}");
        self.fail += 1;
    }
    fn note(&mut self, msg: &str) {
        println!("NOTE: {msg}");
    }
}

fn smoke_body() -> Result<(), ToolError> {
    let systemctl = env_tool("SURMOUNT_SMOKE_SYSTEMCTL", "systemctl");
    let curl = env_tool("SURMOUNT_SMOKE_CURL", "curl");
    let readlink = env_tool("SURMOUNT_SMOKE_READLINK", "readlink");
    let ss_cmd = env_tool("SURMOUNT_SMOKE_SS", "ss");
    let mut c = Counters { fail: 0 };

    println!("deploy-host-post-switch-smoke: begin");

    check_generation(&readlink, &mut c);
    check_unit_active(&systemctl, "sshd", &mut c);
    check_unit_active(&systemctl, "stalwart-mail", &mut c);
    check_ui(&systemctl, &mut c);

    let env_blob = unit_environment(&systemctl);
    let listen_env = unit_env_get(&env_blob, "SURMOUNT_LISTEN");
    let listen_mode = unit_env_get(&env_blob, "SURMOUNT_LISTEN_MODE");
    let mut tls_cert_env = unit_env_get(&env_blob, "SURMOUNT_TLS_CERT");
    let mut tls_key_env = unit_env_get(&env_blob, "SURMOUNT_TLS_KEY");
    let acme_enable_env = unit_env_get(&env_blob, "SURMOUNT_ACME_ENABLE");
    let mut redirect_http_env = unit_env_get(&env_blob, "SURMOUNT_REDIRECT_HTTP_TO_HTTPS");

    if let Ok(v) = env::var("SURMOUNT_SMOKE_TLS_CERT") {
        if !v.is_empty() {
            tls_cert_env = Some(v);
        }
    }
    if let Ok(v) = env::var("SURMOUNT_SMOKE_TLS_KEY") {
        if !v.is_empty() {
            tls_key_env = Some(v);
        }
    }
    if let Ok(v) = env::var("SURMOUNT_SMOKE_REDIRECT_HTTP_TO_HTTPS") {
        if !v.is_empty() {
            redirect_http_env = Some(v);
        }
    }

    let https_edge = matches!(listen_mode.as_deref(), Some("https" | "HTTPS"));
    check_health(&curl, listen_env.as_deref(), listen_mode.as_deref(), &mut c);
    check_pems(
        https_edge,
        tls_cert_env.as_deref(),
        tls_key_env.as_deref(),
        &mut c,
    );
    note_acme(acme_enable_env.as_deref(), &mut c);
    check_listen(&ss_cmd, https_edge, redirect_http_env.as_deref(), &mut c);

    println!("deploy-host-post-switch-smoke: end (fail={})", c.fail);
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    if c.fail != 0 {
        Err(ToolError {
            message: format!("post-switch smoke failed (fail={})", c.fail),
            exit_code: 1,
        })
    } else {
        Ok(())
    }
}

fn check_generation(readlink: &str, c: &mut Counters) {
    let run = Command::new(readlink)
        .args(["-f", "/run/current-system"])
        .output();
    if let Ok(out) = run {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                c.pass(&format!("generation current: {s}"));
                return;
            }
            c.fail_line("generation: /run/current-system empty");
            return;
        }
    }
    let run = Command::new(readlink)
        .args(["-f", "/nix/var/nix/profiles/system"])
        .output();
    if let Ok(out) = run {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                c.pass(&format!("generation current (profiles/system): {s}"));
                return;
            }
        }
    }
    c.fail_line("generation: cannot resolve /run/current-system or profiles/system");
}

fn is_active(systemctl: &str, unit: &str) -> String {
    let out = Command::new(systemctl).args(["is-active", unit]).output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn check_unit_active(systemctl: &str, unit: &str, c: &mut Counters) {
    let st = is_active(systemctl, unit);
    if st == "active" {
        c.pass(&format!("unit {unit} is active"));
    } else {
        let shown = if st.is_empty() {
            "unknown"
        } else {
            st.as_str()
        };
        c.fail_line(&format!("unit {unit} is not active (is-active={shown})"));
    }
}

fn check_ui(systemctl: &str, c: &mut Counters) {
    let st = is_active(systemctl, "surmount-management-ui");
    if st == "active" {
        c.pass("unit surmount-management-ui is active");
        return;
    }
    let shown = if st.is_empty() {
        "unknown"
    } else {
        st.as_str()
    };
    c.note(&format!(
        "unit surmount-management-ui is not active (is-active={shown}); attempting start"
    ));
    let start = Command::new(systemctl)
        .args(["start", "surmount-management-ui"])
        .status();
    match start {
        Ok(s) if s.success() => {
            let st2 = is_active(systemctl, "surmount-management-ui");
            if st2 == "active" {
                c.pass("unit surmount-management-ui started and is active");
            } else {
                let shown = if st2.is_empty() {
                    "unknown"
                } else {
                    st2.as_str()
                };
                c.fail_line(&format!(
                    "unit surmount-management-ui start returned 0 but is-active={shown}"
                ));
            }
        }
        _ => c.fail_line("unit surmount-management-ui systemctl start failed"),
    }
}

fn unit_environment(systemctl: &str) -> String {
    let out = Command::new(systemctl)
        .args([
            "show",
            "-p",
            "Environment",
            "surmount-management-ui",
            "--value",
        ])
        .output();
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

fn unit_env_get(blob: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    for token in blob.split_whitespace() {
        if let Some(rest) = token.strip_prefix(&prefix) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Returns (url, not_loopback_host).
pub fn parse_listen_to_loopback_health(
    listen: &str,
    listen_mode: Option<&str>,
) -> Option<(String, bool)> {
    let mut listen = listen.trim();
    if listen.is_empty() {
        return None;
    }
    listen = listen
        .strip_prefix("http://")
        .or_else(|| listen.strip_prefix("https://"))
        .unwrap_or(listen);
    if let Some((h, _)) = listen.split_once('/') {
        listen = h;
    }
    let (host, port_s) = if listen.starts_with('[') {
        let rest = listen.strip_prefix('[')?;
        let (host, after) = rest.split_once(']')?;
        let port_s = after.strip_prefix(':')?;
        (host.to_string(), port_s.to_string())
    } else if listen.matches(':').count() == 1 {
        let (h, p) = listen.split_once(':')?;
        (h.to_string(), p.to_string())
    } else {
        return None;
    };
    let Ok(port) = port_s.parse::<u32>() else {
        return None;
    };
    if !(1..=65535).contains(&port) {
        return None;
    }
    let mut scheme = "http";
    if matches!(listen_mode, Some("https" | "HTTPS")) {
        scheme = "https";
    }
    if port == 443 {
        scheme = "https";
    }
    let url = format!("{scheme}://127.0.0.1:{port}/health");
    let loopback = matches!(
        host.as_str(),
        "127.0.0.1" | "localhost" | "::1" | "0.0.0.0" | "*" | ""
    );
    Some((url, !loopback))
}

fn check_health(curl: &str, listen_env: Option<&str>, listen_mode: Option<&str>, c: &mut Counters) {
    let mut health_url = "http://127.0.0.1:8090/health".to_string();
    if let Some(listen) = listen_env.filter(|s| !s.is_empty()) {
        match parse_listen_to_loopback_health(listen, listen_mode) {
            Some((url, not_loopback)) => {
                health_url = url;
                if not_loopback {
                    c.note(&format!(
                        "SURMOUNT_LISTEN={listen} is not loopback/bind-all; health uses loopback port only (no public URL)"
                    ));
                } else {
                    c.note(&format!(
                        "health URL from SURMOUNT_LISTEN port: {health_url}"
                    ));
                }
            }
            None => {
                c.note(&format!(
                    "SURMOUNT_LISTEN={listen} not parseable as host:port; using default {health_url}"
                ));
            }
        }
    } else {
        c.note(&format!(
            "SURMOUNT_LISTEN unset on unit; using default {health_url}"
        ));
    }

    if skip_flag("SURMOUNT_SMOKE_SKIP_HEALTH") {
        c.note("SURMOUNT_SMOKE_SKIP_HEALTH=1: skipping /health curl");
        c.pass("health check skipped (test override)");
        return;
    }
    let mut args: Vec<&str> = vec!["-sf", "--max-time", "10"];
    let https = health_url.starts_with("https://");
    if https {
        args = vec!["-skf", "--max-time", "10"];
    }
    let status = Command::new(curl)
        .args(&args)
        .arg(&health_url)
        .stdout(std::process::Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => c.pass(&format!("health {health_url}")),
        _ => c.fail_line(&format!("health curl failed: {health_url}")),
    }
}

fn check_pems(
    https_edge: bool,
    tls_cert_env: Option<&str>,
    tls_key_env: Option<&str>,
    c: &mut Counters,
) {
    if skip_flag("SURMOUNT_SMOKE_SKIP_PEM") {
        c.note("SURMOUNT_SMOKE_SKIP_PEM=1: skipping TLS PEM path checks");
        c.pass("tls pem check skipped (test override)");
        return;
    }
    let pem_required = https_edge || tls_cert_env.is_some() || tls_key_env.is_some();
    if !pem_required {
        c.note("tls pem check skipped (listen mode not https and no SURMOUNT_TLS_* paths)");
        c.pass("tls pem check not required (loopback/http day-1 path)");
        return;
    }
    let tls_cert = tls_cert_env
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_TLS_CERT);
    let tls_key = tls_key_env
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_TLS_KEY);
    if Path::new(tls_cert).is_file() {
        c.pass(&format!("tls cert path exists: {tls_cert}"));
    } else {
        c.fail_line(&format!("tls cert path missing: {tls_cert}"));
    }
    if Path::new(tls_key).is_file() {
        c.pass(&format!("tls key path exists: {tls_key}"));
        if let Ok(meta) = fs::metadata(tls_key) {
            let mode = meta.permissions().mode() & 0o777;
            let g = (mode >> 3) & 0o7;
            let o = mode & 0o7;
            if g != 0 || o != 0 {
                c.note(&format!(
                    "tls key mode {mode:o} is not owner-only (prefer 0600); path label only"
                ));
            }
        }
    } else {
        c.fail_line(&format!("tls key path missing: {tls_key}"));
    }
}

fn note_acme(state: Option<&str>, c: &mut Counters) {
    match state {
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON") => {
            c.note(&format!(
                "SURMOUNT_ACME_ENABLE={}: in-process ACME enabled (PEMs may be issued by ACME)",
                state.unwrap()
            ));
            c.pass("acme enable noted (enabled)");
        }
        Some("0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF") => {
            c.note(&format!(
                "SURMOUNT_ACME_ENABLE={}: ACME disabled (static host PEMs expected)",
                state.unwrap()
            ));
            c.pass("acme enable noted (disabled; static PEMs)");
        }
        None | Some("") => {
            c.note("SURMOUNT_ACME_ENABLE unset on unit (ACME treated as off / static PEMs path)");
            c.pass("acme enable noted (unset; static PEMs path)");
        }
        Some(other) => {
            c.note(&format!(
                "SURMOUNT_ACME_ENABLE={other}: non-standard value (labels only)"
            ));
            c.pass("acme enable noted (non-standard)");
        }
    }
}

fn ss_has_port(ss_out: &str, port: u16) -> bool {
    let needle = format!(":{port}");
    for line in ss_out.lines() {
        let mut rest = line;
        while let Some(idx) = rest.find(&needle) {
            let after = &rest[idx + needle.len()..];
            if after
                .chars()
                .next()
                .map(|c| !c.is_ascii_digit())
                .unwrap_or(true)
            {
                return true;
            }
            rest = &rest[idx + 1..];
        }
    }
    false
}

fn check_listen(ss_cmd: &str, https_edge: bool, redirect_http_env: Option<&str>, c: &mut Counters) {
    let require_public = matches!(
        env::var("SURMOUNT_SMOKE_REQUIRE_PUBLIC_LISTEN").as_deref(),
        Ok("1")
    );
    if skip_flag("SURMOUNT_SMOKE_SKIP_LISTEN") {
        c.note("SURMOUNT_SMOKE_SKIP_LISTEN=1: skipping :80/:443 listen checks");
        c.pass("listen check skipped (test override)");
        return;
    }
    let listen_required = https_edge || require_public;
    if !listen_required {
        c.note("listen :80/:443 check skipped (not https edge; day-1 loopback path)");
        c.pass("listen check not required (loopback/http day-1 path)");
        return;
    }

    let redirect_on = match redirect_http_env {
        Some("0" | "false" | "FALSE" | "no" | "NO" | "off" | "OFF") => false,
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON") | None | Some("") => true,
        Some(other) => {
            c.note(&format!(
                "SURMOUNT_REDIRECT_HTTP_TO_HTTPS={other}: non-standard; treating :80 as required"
            ));
            true
        }
    };

    let ss_path = PathBuf::from(ss_cmd);
    if ss_cmd == "ss" {
        // PATH lookup happens at spawn; missing binary fails below.
    } else if !ss_path.is_file() && which_ok(ss_cmd).is_none() {
        c.fail_line(&format!(
            "listen: ss not found ({ss_cmd}); cannot prove :80/:443"
        ));
        return;
    }

    let out = Command::new(ss_cmd).args(["-lntp"]).output();
    match out {
        Ok(o) if o.status.success() => {
            let body = String::from_utf8_lossy(&o.stdout);
            if ss_has_port(&body, 443) {
                c.pass("listen :443 present (ss)");
            } else {
                c.fail_line("listen :443 not observed in ss (https edge expects product TLS bind)");
            }
            if ss_has_port(&body, 80) {
                c.pass("listen :80 present (ss)");
            } else if redirect_on {
                c.fail_line(
                    "listen :80 not observed in ss (redirect-to-https on; expects redirect-only :80 or free port for it)",
                );
            } else {
                c.note("listen :80 not observed (redirect-to-https off; :80 check soft)");
                c.pass("listen :80 not required (redirect off)");
            }
        }
        Ok(o) => {
            let rc = o.status.code().unwrap_or(1);
            c.fail_line(&format!("listen: ss failed (rc={rc})"));
        }
        Err(_) => {
            c.fail_line(&format!(
                "listen: ss not found ({ss_cmd}); cannot prove :80/:443"
            ));
        }
    }
}

fn which_ok(name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(name);
    if p.is_file() {
        return Some(p);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_listen_http_8090() {
        let (url, not_lb) = parse_listen_to_loopback_health("127.0.0.1:8090", None).unwrap();
        assert_eq!(url, "http://127.0.0.1:8090/health");
        assert!(!not_lb);
    }

    #[test]
    fn public_listen_maps_loopback_port_only() {
        let (url, not_lb) = parse_listen_to_loopback_health("203.0.113.10:8443", None).unwrap();
        assert_eq!(url, "http://127.0.0.1:8443/health");
        assert!(not_lb);
    }

    #[test]
    fn https_edge_443_uses_https_scheme() {
        let (url, _) = parse_listen_to_loopback_health("0.0.0.0:443", Some("https")).unwrap();
        assert_eq!(url, "https://127.0.0.1:443/health");
    }

    #[test]
    fn ss_port_match_does_not_hit_4430() {
        assert!(ss_has_port("LISTEN 0 128 0.0.0.0:443 0.0.0.0:*", 443));
        assert!(!ss_has_port("LISTEN 0 128 0.0.0.0:4430 0.0.0.0:*", 443));
        assert!(ss_has_port("LISTEN *:80 *:*", 80));
    }
}
