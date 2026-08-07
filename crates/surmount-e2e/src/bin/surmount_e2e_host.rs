//! Host end-to-end probes (production-shaped). Not a false-green when env unset.
//!
//! Usage:
//!   SURMOUNT_E2E_HOST=1 SURMOUNT_E2E_BASE_URL=https://127.0.0.1 nix run .#e2e-host
//!   just e2e-host
//!
//! Required env (or fail loud):
//!   SURMOUNT_E2E_HOST=1       must be set (else exit 2)
//!   SURMOUNT_E2E_BASE_URL     required for track 1 health (else FAIL, not silent SKIP)
//!
//! Ban track (when SURMOUNT_E2E_SKIP_BAN is not 1):
//!   SURMOUNT_E2E_LAB_IP       required lab IPv4; FAIL if unset or not in surmount-ban4
//!
//! Optional env: see RESIDUAL.md / docs/EDGE_AND_TLS.md.

use std::env;
use std::process::{Command, Stdio};

use surmount_e2e::pure::{
    LabIpBanPrecheck, MdweRow, base_url_missing_is_fail, cap_string_has_net_admin,
    host_mode_enabled, host_runner_exit_code, hostport_from_base_url, lab_ip_ban_precheck,
    mdwe_row,
};
use surmount_e2e::report::{Counters, Status};
use surmount_e2e::tls_check::{TlsCheckError, check_tls};

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let host_env = env::var("SURMOUNT_E2E_HOST").unwrap_or_default();
    if !host_mode_enabled(&host_env) {
        eprint_host_gate();
        return host_runner_exit_code(false, 0);
    }

    let ui_unit =
        env::var("SURMOUNT_E2E_UI_UNIT").unwrap_or_else(|_| "surmount-management-ui".into());
    let arti_unit = env::var("SURMOUNT_E2E_ARTI_UNIT")
        .unwrap_or_else(|_| "surmount-arti-hidden-service".into());
    let helper_sock = env::var("SURMOUNT_E2E_HELPER_SOCK")
        .unwrap_or_else(|_| "/run/surmount/nft-ban-helper.sock".into());
    let base_url = env::var("SURMOUNT_E2E_BASE_URL").unwrap_or_default();
    let mut tls_host = env::var("SURMOUNT_E2E_TLS_HOST").unwrap_or_default();
    let tls_sni = env::var("SURMOUNT_E2E_TLS_SNI").unwrap_or_default();
    let lab_ip = env::var("SURMOUNT_E2E_LAB_IP").unwrap_or_default();
    let onion = env::var("SURMOUNT_E2E_ONION").unwrap_or_default();
    let socks = env::var("TOR_SOCKS").unwrap_or_else(|_| "127.0.0.1:9050".into());
    let skip_ban = env::var("SURMOUNT_E2E_SKIP_BAN").unwrap_or_default() == "1";
    let tls_insecure = env::var("SURMOUNT_E2E_TLS_INSECURE").unwrap_or_default() == "1";

    let mut c = Counters::default();
    // Set on ban track; summary always prints (n/a | skipped | UNPROVEN).
    let ban_drop_status;
    let mut ui_unit_active = false;

    println!("==============================================");
    println!(" Surmount host end-to-end");
    println!("==============================================");
    println!("Honesty: host probe green is deploy proof for checked rows only.");
    println!("  Local self-signed / temp Tor keys are a different layer (nix run .#e2e).");
    println!("  Set existence / lab membership != live traffic drop (see ban_drop summary).");
    println!();

    // --- Track 1: HTTPS + process hardening ---
    println!("== track 1: public HTTPS + MemoryDenyWriteExecute hardening ==");

    if base_url_missing_is_fail(true, &base_url) {
        c.row(
            Status::Fail,
            "SURMOUNT_E2E_BASE_URL required",
            Some("set e.g. https://127.0.0.1 or https://services.example (no silent health skip)"),
        );
    } else {
        let health_url = format!("{}/health", base_url.trim_end_matches('/'));
        if which("curl").is_none() {
            c.row(Status::Fail, "TLS/HTTP health", Some("curl not found"));
        } else {
            let health_body = unique_temp_path("surmount-e2e-host-health");
            let mut args: Vec<String> = vec![
                "-sS".into(),
                "--max-time".into(),
                "15".into(),
                "-o".into(),
                health_body.display().to_string(),
                "-w".into(),
                "%{http_code}".into(),
            ];
            if tls_insecure {
                args.insert(0, "-k".into());
            }
            args.push(health_url.clone());
            let out = Command::new("curl")
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output();
            let _ = std::fs::remove_file(&health_body);
            let code = match out {
                Ok(o) => {
                    let s = String::from_utf8_lossy(&o.stdout);
                    // Last three digits if present.
                    let digits: String = s
                        .chars()
                        .rev()
                        .take(3)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    if digits.len() == 3 && digits.chars().all(|ch| ch.is_ascii_digit()) {
                        digits
                    } else {
                        s.trim().to_string()
                    }
                }
                Err(_) => "000".into(),
            };
            if code == "200" {
                c.row(
                    Status::Pass,
                    &format!("TLS/HTTP health {health_url} -> 200"),
                    None,
                );
            } else {
                c.row(
                    Status::Fail,
                    &format!("TLS/HTTP health {health_url}"),
                    Some(&format!("HTTP {code}")),
                );
            }
        }
    }

    if which("systemctl").is_some() {
        if systemctl_is_active(&ui_unit) {
            ui_unit_active = true;
            c.row(Status::Pass, &format!("UI unit active ({ui_unit})"), None);
        } else {
            c.row(
                Status::Fail,
                &format!("UI unit active ({ui_unit})"),
                Some("not active"),
            );
        }

        let mdwe = systemctl_show(&ui_unit, "MemoryDenyWriteExecute");
        match mdwe_row(ui_unit_active, &mdwe) {
            MdweRow::Pass => {
                c.row(
                    Status::Pass,
                    &format!(
                        "MemoryDenyWriteExecute=yes on {ui_unit} (no writable+executable memory)"
                    ),
                    None,
                );
            }
            MdweRow::FailInactive => {
                c.row(
                    Status::Fail,
                    &format!("MemoryDenyWriteExecute on {ui_unit}"),
                    Some("unit not active; MDWE assert requires active unit"),
                );
            }
            MdweRow::FailValue => {
                c.row(
                    Status::Fail,
                    &format!("MemoryDenyWriteExecute on {ui_unit}"),
                    Some(&format!(
                        "got '{}' want yes",
                        if mdwe.is_empty() { "unset" } else { &mdwe }
                    )),
                );
            }
        }

        if !ui_unit_active {
            c.row(
                Status::Fail,
                "UI has no CAP_NET_ADMIN",
                Some("unit not active; capability assert requires active unit"),
            );
        } else {
            let amp = systemctl_show(&ui_unit, "AmbientCapabilities");
            let capb = systemctl_show(&ui_unit, "CapabilityBoundingSet");
            if amp.is_empty() && capb.is_empty() {
                c.row(
                    Status::Fail,
                    "UI has no CAP_NET_ADMIN",
                    Some("capability properties empty on active unit"),
                );
            } else if cap_string_has_net_admin(&amp) {
                c.row(
                    Status::Fail,
                    "UI has no CAP_NET_ADMIN",
                    Some(&format!("AmbientCapabilities includes net_admin: {amp}")),
                );
            } else if cap_string_has_net_admin(&capb) {
                c.row(
                    Status::Fail,
                    "UI has no CAP_NET_ADMIN",
                    Some(&format!("CapabilityBoundingSet includes net_admin: {capb}")),
                );
            } else {
                c.row(
                    Status::Pass,
                    "UI AmbientCapabilities and CapabilityBoundingSet have no CAP_NET_ADMIN",
                    None,
                );
            }
        }

        if systemctl_is_active("nginx") {
            c.row(
                Status::Fail,
                "nginx not product edge when web off",
                Some("nginx unit is active (dual-run or leftover?)"),
            );
        } else {
            c.row(
                Status::Pass,
                "nginx inactive (not product edge when web off / dual-run unused)",
                None,
            );
        }
    } else {
        c.row(
            Status::Skip,
            "systemd unit probes",
            Some("systemctl not available (run on host)"),
        );
    }

    if tls_host.is_empty() && !base_url.is_empty() {
        tls_host = hostport_from_base_url(&base_url);
    }

    if !tls_host.is_empty() {
        let sni = if tls_sni.is_empty() {
            None
        } else {
            Some(tls_sni.as_str())
        };
        match check_tls(&tls_host, sni) {
            Ok(()) => {
                c.row(
                    Status::Pass,
                    &format!("check-tls {tls_host} (handshake; non-expired certificate)"),
                    None,
                );
            }
            Err(TlsCheckError::OpensslMissing) => {
                c.row(Status::Fail, "check-tls", Some("openssl not found"));
            }
            Err(e) => {
                c.row(
                    Status::Fail,
                    &format!("check-tls {tls_host}"),
                    Some(&format!("{e:?}")),
                );
            }
        }
    } else {
        c.row(
            Status::Fail,
            "check-tls target",
            Some("could not derive TLS host (set SURMOUNT_E2E_TLS_HOST)"),
        );
    }

    println!();
    println!("  cert files: operator-placed certificate and key on host only; key mode");
    println!("  must not be group/world readable; never in git. Automatic issuance on");
    println!("  product port 80 still parked (Q-EDGE). External PEMs lean until Q-CA answered.");

    // --- Track 2: Arti ---
    println!();
    println!("== track 2: Arti live Tor (surmount-arti ownership) ==");

    if which("systemctl").is_some() {
        if systemctl_cat_exists(&arti_unit) {
            if systemctl_is_active(&arti_unit) {
                c.row(
                    Status::Pass,
                    &format!("Arti unit active ({arti_unit})"),
                    None,
                );
                println!("  note: unit active != onion published on the Tor network");
            } else {
                c.row(
                    Status::Fail,
                    &format!("Arti unit active ({arti_unit})"),
                    Some("not active (startDaemon / HS state dir?)"),
                );
            }
            let user = systemctl_show(&arti_unit, "User");
            if user == "surmount-arti" {
                c.row(Status::Pass, "Arti unit User=surmount-arti", None);
            } else if !user.is_empty() {
                c.row(
                    Status::Fail,
                    "Arti unit User=surmount-arti",
                    Some(&format!("got User={user}")),
                );
            } else {
                c.row(
                    Status::Fail,
                    "Arti unit User=surmount-arti",
                    Some("could not read User= (unit missing?)"),
                );
            }
        } else {
            c.row(
                Status::Skip,
                &format!("Arti unit {arti_unit}"),
                Some("unit not installed on this host"),
            );
        }
    } else {
        c.row(
            Status::Skip,
            "Arti systemd probes",
            Some("systemctl not available"),
        );
    }

    if !onion.is_empty() {
        let (socks_host, socks_port) = socks.rsplit_once(':').unwrap_or(("127.0.0.1", "9050"));
        if which("curl").is_none() {
            c.row(Status::Fail, "Onion fetch", Some("curl not found"));
        } else {
            let onion_body = unique_temp_path("surmount-e2e-onion-health");
            let st = Command::new("curl")
                .args([
                    "-sS",
                    "--max-time",
                    "45",
                    "--socks5-hostname",
                    &format!("{socks_host}:{socks_port}"),
                    &format!("http://{onion}/health"),
                    "-o",
                ])
                .arg(&onion_body)
                .status();
            let body_ok = std::fs::metadata(&onion_body)
                .map(|m| m.len() > 0)
                .unwrap_or(false);
            let _ = std::fs::remove_file(&onion_body);
            if st.map(|s| s.success()).unwrap_or(false) && body_ok {
                c.row(
                    Status::Pass,
                    "Onion fetch /health via Tor SOCKS (published proof)",
                    None,
                );
            } else {
                c.row(
                    Status::Fail,
                    "Onion fetch /health via Tor SOCKS",
                    Some("set working TOR_SOCKS and SURMOUNT_E2E_ONION"),
                );
            }
        }
    } else {
        c.row(
            Status::Skip,
            "Onion fetch",
            Some("set SURMOUNT_E2E_ONION for published proof (unit active alone is not enough)"),
        );
    }

    println!("  HS identity keys: under onionServiceStateDir, owned/writable by surmount-arti;");
    println!("  never in git. Hardening after real arti proxy: compare unit hardening to docs;");
    println!("  do not silently weaken. Admin/JMAP onion stanzas stay off (Q-ARTI-2/3 parked).");

    // --- Track 3: ban helper + kernel firewall ---
    println!();
    println!("== track 3: ban helper + kernel firewall (preflight + lab membership) ==");
    println!("  honesty: set existence is preflight only; lab membership != traffic drop");

    if skip_ban {
        ban_drop_status = "skipped";
        c.row(
            Status::Skip,
            "ban / kernel firewall rows",
            Some("SURMOUNT_E2E_SKIP_BAN=1 (host green does not claim drop)"),
        );
    } else {
        ban_drop_status = "UNPROVEN";

        let sock_path = std::path::Path::new(&helper_sock);
        // Unix socket: metadata file_type is socket, or exists as path.
        if is_unix_socket(sock_path) {
            c.row(
                Status::Pass,
                &format!("ban helper Unix socket present ({helper_sock})"),
                None,
            );
        } else {
            c.row(
                Status::Fail,
                "ban helper Unix socket",
                Some(&format!("missing {helper_sock} (nftHelper + socket unit?)")),
            );
        }

        if which("systemctl").is_some() {
            if systemctl_is_active("surmount-nft-ban-helper.socket") {
                c.row(Status::Pass, "surmount-nft-ban-helper.socket active", None);
            } else {
                c.row(
                    Status::Fail,
                    "surmount-nft-ban-helper.socket active",
                    Some("socket unit not active"),
                );
            }
        }

        if which("nft").is_some() {
            if nft_list_set("surmount-ban4") {
                c.row(
                    Status::Pass,
                    "preflight: kernel set inet surmount_guard surmount-ban4 exists (not drop proof)",
                    None,
                );
            } else {
                c.row(
                    Status::Fail,
                    "preflight: kernel set surmount-ban4",
                    Some("nft list failed (accessControl.nftSets?)"),
                );
            }
            if nft_list_set("surmount-ban6") {
                c.row(
                    Status::Pass,
                    "preflight: kernel set inet surmount_guard surmount-ban6 exists (not drop proof)",
                    None,
                );
            } else {
                c.row(
                    Status::Fail,
                    "preflight: kernel set surmount-ban6",
                    Some("nft list failed"),
                );
            }
        } else {
            c.row(
                Status::Fail,
                "nft set list",
                Some("nft command not found (needed for ban track when not skipped)"),
            );
        }

        match lab_ip_ban_precheck(false, &lab_ip) {
            LabIpBanPrecheck::Missing => {
                c.row(
                    Status::Fail,
                    "SURMOUNT_E2E_LAB_IP required",
                    Some(
                        "set a safe lab address, or SURMOUNT_E2E_SKIP_BAN=1 (never lock out the operator)",
                    ),
                );
            }
            LabIpBanPrecheck::NotExactToken => {
                c.row(
                    Status::Fail,
                    "SURMOUNT_E2E_LAB_IP not an exact IP token",
                    Some(
                        "use a single IPv4/IPv6 address (no spaces/commas); exact nft element only",
                    ),
                );
            }
            LabIpBanPrecheck::Skipped => {
                // Ban track is on in this branch; Skipped should not appear.
            }
            LabIpBanPrecheck::Ok => {
                println!("  lab IP: {lab_ip}");
                println!(
                    "  After membership: prove traffic from LAB_IP is dropped (lab source), then unban/cleanup."
                );
                println!(
                    "  Cleanup (helper when available; remove-ban is idempotent if element absent):"
                );
                println!("    surmount-nft-ban-helper remove-ban {lab_ip}");
                if which("nft").is_some() {
                    // Exact element query only. Never substring grep list output.
                    if nft_get_element_ban4(&lab_ip) {
                        c.row(
                            Status::Pass,
                            "lab IP membership in surmount-ban4 (exact element; ban_drop=UNPROVEN)",
                            None,
                        );
                        println!(
                            "  note: membership is not traffic-drop proof. Cleanup LAB_IP when done."
                        );
                    } else {
                        c.row(
                            Status::Fail,
                            "lab IP membership in surmount-ban4",
                            Some("exact element not present; signal ban for LAB_IP then re-run"),
                        );
                    }
                }
            }
        }
    }

    println!();
    println!("==============================================");
    println!(" Host end-to-end summary");
    println!("==============================================");
    println!("  pass={}  fail={}  skip={}", c.pass, c.fail, c.skip);
    println!("  ban_drop={ban_drop_status}");
    println!();
    println!("Pins retained:");
    println!("  - host green != claim from nix run .#e2e alone");
    println!("  - unit active != onion published without fetch proof");
    println!("  - CAP_NET_ADMIN only on ban helper, never UI (ambient and bounding)");
    println!("  - ban_drop=UNPROVEN means membership/preflight only; not live drop");
    println!("  - Q-ACL / Q-EDGE ACME / Q-ARTI-2/3 not invented here");
    println!();

    host_runner_exit_code(true, c.fail)
}

fn eprint_host_gate() {
    eprintln!(
        r#"error: host end-to-end env unset

  nix run .#e2e-host / just e2e-host refuse to mark host rows as pass when
  SURMOUNT_E2E_HOST is not set. This is intentional (no false-green cutover).

  Local comprehensive end-to-end (no VPS):
    nix run .#e2e
    just e2e

  Host mode (on the operator VPS or against a staging deploy):
    SURMOUNT_E2E_HOST=1 \
      SURMOUNT_E2E_BASE_URL=https://127.0.0.1 \
      SURMOUNT_E2E_LAB_IP=203.0.113.50 \
      nix run .#e2e-host

  BASE_URL is required when host mode is on (health cannot be silently skipped).
  LAB_IP is required unless SURMOUNT_E2E_SKIP_BAN=1.

  See docs/EDGE_AND_TLS.md (host end-to-end recipes) and RESIDUAL.md.
"#
    );
}

fn which(bin: &str) -> Option<std::path::PathBuf> {
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

fn systemctl_is_active(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemctl_cat_exists(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["cat", unit])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn systemctl_show(unit: &str, prop: &str) -> String {
    let out = Command::new("systemctl")
        .args(["show", unit, "-p", prop, "--value"])
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn nft_list_set(name: &str) -> bool {
    Command::new("nft")
        .args(["list", "set", "inet", "surmount_guard", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn nft_get_element_ban4(ip: &str) -> bool {
    // Exact element: `nft get element inet surmount_guard surmount-ban4 { IP }`
    Command::new("nft")
        .args([
            "get",
            "element",
            "inet",
            "surmount_guard",
            "surmount-ban4",
            &format!("{{ {ip} }}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn is_unix_socket(path: &std::path::Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::metadata(path) {
        Ok(m) => m.file_type().is_socket(),
        Err(_) => false,
    }
}

/// Unique body path under temp_dir (PID + label) so parallel host e2e runs do not clobber.
fn unique_temp_path(label: &str) -> std::path::PathBuf {
    env::temp_dir().join(format!("{label}.{}.body", std::process::id()))
}
