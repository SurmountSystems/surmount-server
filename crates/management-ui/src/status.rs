//! Public systems dashboard for Host `status.{primary}` (Axum, not nginx).
//!
//! Not the operator console. Rows come from a product inventory plus live
//! systemd ActiveState / SubState (and optional HTTP samples). Host uptime
//! comes from `/proc/uptime`. No RAM, disk, IP, onion, or cookie bytes.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;
use std::process::Command;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::config::AppConfig;
use crate::mta_sts::{MtaStsMode, mta_sts_policy_hostname};
use crate::redirect::{host_for_url_authority, status_hostname};

/// One named systems row the dashboard emits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemsComponent {
    pub name: String,
    pub role: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
}

/// Representative systems status body. Tests assert these field names exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemsStatusPayload {
    pub surface: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub host: String,
    pub primary_domain: String,
    pub uptime: String,
    pub systems: Vec<SystemsComponent>,
}

/// Live or injected sample for one unit / HTTP check.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComponentProbe {
    pub active_state: String,
    pub sub_state: String,
    pub load_state: String,
    pub http_status: Option<u16>,
}

/// One product row the dashboard knows how to name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemsInventoryItem {
    pub id: &'static str,
    pub role: &'static str,
    pub unit: Option<&'static str>,
    pub href: Option<String>,
}

/// Product units this dashboard may probe. Not Vaultwarden, Lake, or Splora mainnet.
pub const DASHBOARD_UNITS: &[&str] = &[
    "stalwart-mail.service",
    "surmount-management-ui.service",
    "surmount-arti-hidden-service.service",
    "sshd.service",
    "surmount-scram.service",
    "fail2ban.service",
    "splora-testnet3.service",
    "splora-testnet4.service",
    "splora-mutinynet.service",
    "splora-liquid.service",
];

/// Ordinary English status from systemd ActiveState / SubState / HTTP samples.
///
/// Never returns a constant "ok". HTTP client errors win over a running unit.
pub fn classify_component_status(probe: &ComponentProbe) -> String {
    if let Some(code) = probe.http_status
        && code >= 400
    {
        return format!("HTTP {code}");
    }
    let load = probe.load_state.trim();
    let active = probe.active_state.trim();
    let sub = probe.sub_state.trim();
    if load.eq_ignore_ascii_case("not-found") {
        return "not installed".to_string();
    }
    if active.eq_ignore_ascii_case("failed") || sub.eq_ignore_ascii_case("failed") {
        return "failed".to_string();
    }
    if sub.eq_ignore_ascii_case("listening") {
        return "listening".to_string();
    }
    if active.eq_ignore_ascii_case("inactive") {
        return "inactive".to_string();
    }
    if active.eq_ignore_ascii_case("activating") {
        return "starting".to_string();
    }
    if active.eq_ignore_ascii_case("deactivating") {
        return "stopping".to_string();
    }
    if active.eq_ignore_ascii_case("reloading") {
        return "reloading".to_string();
    }
    if active.eq_ignore_ascii_case("maintenance") {
        return "maintenance".to_string();
    }
    if active.eq_ignore_ascii_case("active") && sub.eq_ignore_ascii_case("running") {
        return "running".to_string();
    }
    if active.eq_ignore_ascii_case("active") && sub.eq_ignore_ascii_case("exited") {
        return "exited".to_string();
    }
    if active.eq_ignore_ascii_case("active") {
        return "running".to_string();
    }
    if !sub.is_empty() {
        return sub.to_ascii_lowercase();
    }
    if !active.is_empty() {
        return active.to_ascii_lowercase();
    }
    "unknown".to_string()
}

/// Standing number law: seconds; minutes at most 60 minutes; N.N hours through
/// 24 hours; N.N days after that.
pub fn format_uptime_seconds(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return "unknown".to_string();
    }
    let secs = seconds.floor() as u64;
    if secs < 60 {
        if secs == 1 {
            return "1 second".to_string();
        }
        return format!("{secs} seconds");
    }
    if secs <= 3600 {
        let minutes = secs / 60;
        if minutes == 1 {
            return "1 minute".to_string();
        }
        return format!("{minutes} minutes");
    }
    if secs <= 86_400 {
        return format!("{:.1} hours", seconds / 3600.0);
    }
    format!("{:.1} days", seconds / 86_400.0)
}

/// First field of `/proc/uptime` (seconds.centiseconds).
pub fn parse_proc_uptime(contents: &str) -> Option<f64> {
    let first = contents.split_whitespace().next()?;
    let n: f64 = first.parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    Some(n)
}

/// Read host uptime seconds from a `/proc/uptime`-shaped file.
pub fn read_proc_uptime_file(path: &Path) -> Option<f64> {
    std::fs::read_to_string(path)
        .ok()
        .as_deref()
        .and_then(parse_proc_uptime)
}

/// `https://{hostname}{path}` for a DNS hostname. Refuses IP literals.
pub fn https_public_href(host: &str, path: &str) -> Option<String> {
    let authority = host_for_url_authority(host);
    if authority.is_empty() {
        return None;
    }
    if authority.parse::<IpAddr>().is_ok() {
        return None;
    }
    if authority.starts_with('[') {
        return None;
    }
    if authority.chars().all(|c| c.is_ascii_digit() || c == '.') && authority.contains('.') {
        // Dotted numeric that did not parse as IPv4 still must not become an href.
        return None;
    }
    let host = authority.to_ascii_lowercase();
    if host == "localhost" {
        return None;
    }
    let path = if path.is_empty() {
        "/"
    } else if path.starts_with('/') {
        path
    } else {
        return None;
    };
    Some(format!("https://{host}{path}"))
}

/// Parse `systemctl show -p Id -p ActiveState -p SubState -p LoadState` text.
pub fn parse_systemctl_show(text: &str) -> BTreeMap<String, ComponentProbe> {
    let mut map = BTreeMap::new();
    let mut id = String::new();
    let mut probe = ComponentProbe::default();
    let flush = |id: &mut String,
                 probe: &mut ComponentProbe,
                 map: &mut BTreeMap<String, ComponentProbe>| {
        if !id.is_empty() {
            map.insert(std::mem::take(id), std::mem::take(probe));
        }
        *probe = ComponentProbe::default();
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush(&mut id, &mut probe, &mut map);
            continue;
        }
        if let Some(v) = line.strip_prefix("Id=") {
            if !id.is_empty() {
                flush(&mut id, &mut probe, &mut map);
            }
            id = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("ActiveState=") {
            probe.active_state = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("SubState=") {
            probe.sub_state = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("LoadState=") {
            probe.load_state = v.trim().to_string();
        }
    }
    flush(&mut id, &mut probe, &mut map);
    map
}

fn splora_portal_host(config: &AppConfig) -> String {
    if let Some(h) = config
        .splora_proxy
        .portal_host
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return host_for_url_authority(h).to_ascii_lowercase();
    }
    let primary = host_for_url_authority(&config.primary_domain);
    if primary.is_empty() {
        return String::new();
    }
    format!("splora.{}", primary.to_ascii_lowercase())
}

fn classify_tls_state(https: bool) -> String {
    if https {
        "listening".to_string()
    } else {
        "plain HTTP".to_string()
    }
}

fn classify_mta_sts_state(mode: MtaStsMode) -> String {
    match mode {
        MtaStsMode::Off => "off".to_string(),
        MtaStsMode::Testing => "testing".to_string(),
        MtaStsMode::Enforce => "enforcing".to_string(),
    }
}

/// Named services this Host actually runs. Splora mainnet, Vaultwarden, and Lake
/// stay off this list unless a later inventory finds them installed.
pub fn systems_status_inventory(config: &AppConfig) -> Vec<SystemsInventoryItem> {
    let mail = https_public_href(&config.mail_hostname, "/");
    let services = https_public_href(&config.services_hostname, "/");
    let status = https_public_href(&status_hostname(&config.primary_domain), "/");
    let site = https_public_href(&config.primary_domain, "/");
    let mta = https_public_href(
        &mta_sts_policy_hostname(&config.primary_domain),
        "/.well-known/mta-sts.txt",
    );
    let portal = splora_portal_host(config);
    let splora_testnet = https_public_href(&portal, "/testnet/");
    let splora_testnet4 = https_public_href(&portal, "/testnet4/");
    let splora_mutinynet = https_public_href(&portal, "/mutinynet/");
    let splora_liquid = https_public_href(&portal, "/liquid/");
    vec![
        SystemsInventoryItem {
            id: "mail",
            role: "mail",
            unit: Some("stalwart-mail.service"),
            href: mail,
        },
        SystemsInventoryItem {
            id: "edge",
            role: "HTTPS edge",
            unit: Some("surmount-management-ui.service"),
            href: site.clone(),
        },
        SystemsInventoryItem {
            id: "tls",
            role: "certificate",
            unit: None,
            href: site.clone(),
        },
        SystemsInventoryItem {
            id: "mta_sts",
            role: "MTA-STS policy",
            unit: None,
            href: mta,
        },
        SystemsInventoryItem {
            id: "console",
            role: "operator console",
            unit: Some("surmount-management-ui.service"),
            href: services,
        },
        SystemsInventoryItem {
            id: "status",
            role: "systems dashboard",
            unit: Some("surmount-management-ui.service"),
            href: status,
        },
        SystemsInventoryItem {
            id: "site",
            role: "public site",
            unit: Some("surmount-management-ui.service"),
            href: site,
        },
        SystemsInventoryItem {
            id: "arti",
            role: "hidden service",
            unit: Some("surmount-arti-hidden-service.service"),
            href: None,
        },
        SystemsInventoryItem {
            id: "sshd",
            role: "SSH",
            unit: Some("sshd.service"),
            href: None,
        },
        SystemsInventoryItem {
            id: "splora-testnet3",
            role: "Splora testnet3",
            unit: Some("splora-testnet3.service"),
            href: splora_testnet,
        },
        SystemsInventoryItem {
            id: "splora-testnet4",
            role: "Splora testnet4",
            unit: Some("splora-testnet4.service"),
            href: splora_testnet4,
        },
        SystemsInventoryItem {
            id: "splora-mutinynet",
            role: "Splora mutinynet",
            unit: Some("splora-mutinynet.service"),
            href: splora_mutinynet,
        },
        SystemsInventoryItem {
            id: "splora-liquid",
            role: "Splora liquid",
            unit: Some("splora-liquid.service"),
            href: splora_liquid,
        },
        SystemsInventoryItem {
            id: "scram",
            role: "builder memory guard",
            unit: Some("surmount-scram.service"),
            href: None,
        },
        SystemsInventoryItem {
            id: "fail2ban",
            role: "intrusion prevention",
            unit: Some("fail2ban.service"),
            href: None,
        },
    ]
}

/// Build the payload from config, formatted uptime, and unit/HTTP samples.
pub fn systems_status_from_probes(
    config: &AppConfig,
    uptime: &str,
    probes: &BTreeMap<String, ComponentProbe>,
) -> SystemsStatusPayload {
    let host = status_hostname(&config.primary_domain);
    let https = config.listen_mode.is_https();
    let systems = systems_status_inventory(config)
        .into_iter()
        .map(|item| {
            let state = match item.id {
                "tls" => classify_tls_state(https),
                "mta_sts" => classify_mta_sts_state(config.mta_sts_mode),
                _ => {
                    let probe = item
                        .unit
                        .and_then(|u| probes.get(u))
                        .cloned()
                        .unwrap_or_default();
                    classify_component_status(&probe)
                }
            };
            SystemsComponent {
                name: item.id.to_string(),
                role: item.role.to_string(),
                state,
                href: item.href,
            }
        })
        .collect();
    SystemsStatusPayload {
        surface: "systems_status",
        service: "surmount",
        version: env!("CARGO_PKG_VERSION"),
        host,
        primary_domain: config.primary_domain.clone(),
        uptime: uptime.to_string(),
        systems,
    }
}

fn read_host_uptime() -> String {
    #[cfg(test)]
    {
        if let Ok(raw) = std::env::var("SURMOUNT_STATUS_UPTIME_SECONDS")
            && let Ok(n) = raw.parse::<f64>()
        {
            return format_uptime_seconds(n);
        }
        return format_uptime_seconds(720.0);
    }
    #[cfg(not(test))]
    {
        read_proc_uptime_file(Path::new("/proc/uptime"))
            .map(format_uptime_seconds)
            .unwrap_or_else(|| "unknown".to_string())
    }
}

fn live_unit_probes() -> BTreeMap<String, ComponentProbe> {
    #[cfg(test)]
    {
        if std::env::var_os("SURMOUNT_STATUS_SYSTEMCTL").is_none() {
            return BTreeMap::new();
        }
    }
    let systemctl =
        std::env::var("SURMOUNT_STATUS_SYSTEMCTL").unwrap_or_else(|_| "systemctl".to_string());
    let mut cmd = Command::new(systemctl);
    cmd.args([
        "show",
        "-p",
        "Id",
        "-p",
        "ActiveState",
        "-p",
        "SubState",
        "-p",
        "LoadState",
        "--no-pager",
    ]);
    cmd.args(DASHBOARD_UNITS);
    match cmd.output() {
        Ok(out) => parse_systemctl_show(&String::from_utf8_lossy(&out.stdout)),
        Err(_) => BTreeMap::new(),
    }
}

/// Build the payload the status Host handler actually emits.
pub fn systems_status_from_config(config: &AppConfig) -> SystemsStatusPayload {
    systems_status_from_probes(config, &read_host_uptime(), &live_unit_probes())
}

/// Serve HTML dashboard, JSON (`/status.json` or Accept), or closed 404.
pub fn serve_systems_status(config: &AppConfig, path: &str, accept: Option<&str>) -> Response {
    let path_only = path.split('?').next().unwrap_or(path);
    let payload = systems_status_from_config(config);
    let want_json = path_only == "/status.json"
        || (path_only == "/"
            && accept.is_some_and(|a| a.to_ascii_lowercase().contains("application/json")));
    if path_only == "/status.json" || (path_only == "/" && want_json) {
        let body = serde_json::to_string(&payload).unwrap_or_else(|_| {
            r#"{"surface":"systems_status","service":"surmount","systems":[]}"#.into()
        });
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
            body,
        )
            .into_response();
    }
    if path_only == "/" {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            render_systems_status_html(&payload),
        )
            .into_response();
    }
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn linked_text(text: &str, href: Option<&str>) -> String {
    let t = html_escape(text);
    match href {
        Some(h) if !h.is_empty() => format!("<a href=\"{}\">{t}</a>", html_escape(h)),
        _ => t,
    }
}

/// Public HTML dashboard. Named systems fields appear in the body.
pub fn render_systems_status_html(payload: &SystemsStatusPayload) -> String {
    let host = html_escape(&payload.host);
    let host_link = linked_text(
        &payload.host,
        https_public_href(&payload.host, "/").as_deref(),
    );
    let primary_link = linked_text(
        &payload.primary_domain,
        https_public_href(&payload.primary_domain, "/").as_deref(),
    );
    let version = html_escape(payload.version);
    let uptime = html_escape(&payload.uptime);
    let mut rows = String::new();
    for c in &payload.systems {
        let name = html_escape(&c.name);
        let role = html_escape(&c.role);
        let state = html_escape(&c.state);
        let label = linked_text(&c.name, c.href.as_deref());
        rows.push_str(&format!(
            "<tr data-system=\"{name}\"><th>{label}</th><td data-role=\"{role}\">{role}</td>\
<td data-state=\"{state}\">{state}</td></tr>\n"
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html lang="en" data-theme="doge" data-surface="systems-status">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<meta name="color-scheme" content="only dark"/>
<meta name="robots" content="index,follow"/>
<title>Surmount systems status · {host}</title>
<style>
:root, [data-theme="doge"] {{
  color-scheme: only dark;
  --bg: #000000;
  --fg: #FFFFFF;
  --muted: #00FFFF;
  --accent: #00FFFF;
  --ok: #00FF00;
  --border: #FFFFFF;
}}
* {{ box-sizing: border-box; }}
body {{
  margin: 0;
  font-family: ui-sans-serif, system-ui, sans-serif;
  background: var(--bg);
  color: var(--fg);
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}}
main {{
  max-width: 40rem;
  margin: 2rem auto;
  padding: 0 1rem;
  flex: 1;
}}
.eyebrow {{ color: var(--muted); letter-spacing: 0.08em; text-transform: uppercase; font-size: 0.75rem; }}
h1 {{ font-size: 1.5rem; margin: 0.5rem 0 1rem; }}
.host {{ color: var(--muted); }}
a {{ color: var(--accent); }}
a:hover {{ color: var(--ok); }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ text-align: left; padding: 0.5rem 0; border-bottom: 1px solid var(--border); }}
th {{ color: var(--ok); }}
th a {{ color: var(--ok); }}
footer {{ padding: 1rem; color: var(--muted); font-size: 0.75rem; }}
</style>
</head>
<body data-theme="doge" data-surface="systems-status">
<main>
<p class="eyebrow">Surmount</p>
<h1>Systems status</h1>
<p class="host">{host_link}</p>
<p>Primary domain {primary_link}. Public systems dashboard on the Axum HTTPS edge. Not the operator console.</p>
<p class="uptime">Host uptime {uptime}.</p>
<table>
<thead><tr><th>system</th><th>role</th><th>status</th></tr></thead>
<tbody>
{rows}</tbody>
</table>
</main>
<footer>surmount {version} · surface systems_status · service surmount</footer>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;

    fn probe(active: &str, sub: &str, load: &str, http: Option<u16>) -> ComponentProbe {
        ComponentProbe {
            active_state: active.into(),
            sub_state: sub.into(),
            load_state: load.into(),
            http_status: http,
        }
    }

    /// Named contract: payload is non-empty and names the systems the handler emits.
    #[test]
    fn systems_status_payload_emits_named_fields() {
        let cfg = AppConfig::example_test();
        let payload = systems_status_from_config(&cfg);
        assert_eq!(payload.surface, "systems_status");
        assert_eq!(payload.service, "surmount");
        assert!(!payload.version.is_empty());
        assert_eq!(payload.host, "status.example.test");
        assert_eq!(payload.primary_domain, "example.test");
        assert!(
            !payload.systems.is_empty(),
            "systems list must be non-empty"
        );
        let names: Vec<&str> = payload.systems.iter().map(|c| c.name.as_str()).collect();
        for must in ["mail", "edge", "tls", "mta_sts", "console"] {
            assert!(
                names.contains(&must),
                "handler must emit system {must}; got {names:?}"
            );
        }
        let html = render_systems_status_html(&payload);
        assert!(html.contains("data-surface=\"systems-status\""));
        assert!(html.contains("Systems status"));
        assert!(html.contains("status.example.test"));
        assert!(html.contains("data-system=\"mail\""));
        assert!(html.contains("data-system=\"edge\""));
        assert!(html.contains("data-system=\"tls\""));
        assert!(!html.to_ascii_lowercase().contains("operator console nav"));
        assert!(
            !html.contains("href=\"/mail\"") && !html.contains("href=\"/mail/"),
            "must not link to the operator console mail path: {html}"
        );
        assert!(!html.contains("Dashboard Overview"));
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["surface"], "systems_status");
        assert!(json["systems"].as_array().unwrap().len() >= 5);
    }

    /// Named contract: GET / calls the shipped handler and is 200 non-empty.
    #[tokio::test]
    async fn serve_systems_status_root_is_200_from_real_handler() {
        let cfg = AppConfig::example_test();
        let payload = systems_status_from_config(&cfg);
        let resp = serve_systems_status(&cfg, "/", None);
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!body.is_empty(), "systems status body must be non-empty");
        assert_eq!(body, render_systems_status_html(&payload));
        assert!(body.contains(&payload.host));
        assert!(body.contains("data-surface=\"systems-status\""));
        assert!(!body.contains("Dashboard Overview"));
    }

    /// Named contract: JSON is the handler payload, not a hardcoded live blob.
    #[tokio::test]
    async fn serve_systems_status_json_matches_handler_payload() {
        let cfg = AppConfig::example_test();
        let expected = serde_json::to_value(systems_status_from_config(&cfg)).unwrap();
        let resp = serve_systems_status(&cfg, "/status.json", None);
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 64 * 1024)
            .await
            .unwrap();
        assert!(!bytes.is_empty(), "JSON body must be non-empty");
        let got: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(got, expected, "JSON must be the live handler payload");
        assert_eq!(got["surface"], "systems_status");
        assert!(got["systems"].as_array().unwrap().len() >= 5);
    }

    /// Named contract: operator console API on the status Host is closed 404.
    #[tokio::test]
    async fn serve_systems_status_console_api_is_404() {
        let cfg = AppConfig::example_test();
        let resp = serve_systems_status(&cfg, "/api/v1/system", None);
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Names, hosts, and public URLs are real https hrefs for hosts this product serves.
    #[test]
    fn systems_status_html_includes_https_href_for_named_host() {
        let cfg = AppConfig::example_test();
        let payload = systems_status_from_config(&cfg);
        let html = render_systems_status_html(&payload);
        assert!(
            html.contains("href=\"https://status.example.test/\""),
            "status host must be a clickable https link: {html}"
        );
        assert!(
            html.contains("href=\"https://mail.example.test/\""),
            "mail host must be a clickable https link: {html}"
        );
        assert!(
            html.contains("href=\"https://services.example.test/\""),
            "services host must be a clickable https link: {html}"
        );
        assert!(
            html.contains("href=\"https://example.test/\""),
            "site host must be a clickable https link: {html}"
        );
        assert!(
            html.contains("href=\"https://splora.example.test/testnet/\""),
            "Splora testnet path must be a clickable https link: {html}"
        );
        assert!(
            !html.contains("href=\"#\""),
            "placeholder hash hrefs are not enough: {html}"
        );
        assert!(
            !html.contains("href=\"https://127."),
            "must not put loopback IPs in hrefs: {html}"
        );
        let hrefs: Vec<&str> = payload
            .systems
            .iter()
            .filter_map(|c| c.href.as_deref())
            .collect();
        assert!(
            hrefs.iter().any(|h| h.starts_with("https://")),
            "payload must carry at least one https href: {hrefs:?}"
        );
        assert!(https_public_href("127.0.0.1", "/").is_none());
        assert!(https_public_href("::1", "/").is_none());
        assert_eq!(
            https_public_href("mail.example.test", "/"),
            Some("https://mail.example.test/".into())
        );
    }

    /// Status strings come from the classifier given ActiveState/HTTP samples.
    #[test]
    fn classify_component_status_uses_unit_and_http_samples_not_constant_ok() {
        assert_eq!(
            classify_component_status(&probe("active", "running", "loaded", None)),
            "running"
        );
        assert_eq!(
            classify_component_status(&probe("failed", "failed", "loaded", None)),
            "failed"
        );
        assert_eq!(
            classify_component_status(&probe("inactive", "dead", "loaded", None)),
            "inactive"
        );
        assert_eq!(
            classify_component_status(&probe("active", "listening", "loaded", None)),
            "listening"
        );
        assert_eq!(
            classify_component_status(&probe("active", "running", "loaded", Some(502))),
            "HTTP 502"
        );
        assert_eq!(
            classify_component_status(&probe("inactive", "dead", "not-found", None)),
            "not installed"
        );
        for sample in [
            probe("active", "running", "loaded", None),
            probe("failed", "failed", "loaded", None),
            probe("inactive", "dead", "loaded", None),
            probe("active", "running", "loaded", Some(502)),
        ] {
            let got = classify_component_status(&sample);
            assert_ne!(got, "ok", "must not hard-code ok; got {got} for {sample:?}");
        }
        let cfg = AppConfig::example_test();
        let mut probes = BTreeMap::new();
        probes.insert(
            "stalwart-mail.service".into(),
            probe("failed", "failed", "loaded", None),
        );
        probes.insert(
            "surmount-management-ui.service".into(),
            probe("active", "running", "loaded", Some(502)),
        );
        let payload = systems_status_from_probes(&cfg, "12 minutes", &probes);
        let mail = payload
            .systems
            .iter()
            .find(|c| c.name == "mail")
            .expect("mail row");
        assert_eq!(mail.state, "failed");
        let edge = payload
            .systems
            .iter()
            .find(|c| c.name == "edge")
            .expect("edge row");
        assert_eq!(edge.state, "HTTP 502");
        let html = render_systems_status_html(&payload);
        assert!(html.contains("data-state=\"failed\""));
        assert!(html.contains("data-state=\"HTTP 502\""));
        assert!(!html.contains("data-state=\"ok\""));
    }

    /// Uptime formatter uses standing number law. Quote the exact strings.
    #[test]
    fn format_uptime_seconds_matches_standing_number_law() {
        assert_eq!(format_uptime_seconds(45.0), "45 seconds");
        assert_eq!(format_uptime_seconds(720.0), "12 minutes");
        assert_eq!(format_uptime_seconds(5400.0), "1.5 hours");
        assert_eq!(format_uptime_seconds(90_000.0), "1.0 days");
        assert_eq!(parse_proc_uptime("12345.67 88888.00\n"), Some(12345.67));
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "surmount-status-uptime-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, "5400.12 100.00\n").unwrap();
        assert_eq!(read_proc_uptime_file(&path), Some(5400.12));
        let _ = std::fs::remove_file(&path);
        let cfg = AppConfig::example_test();
        let payload = systems_status_from_probes(&cfg, "1.5 hours", &BTreeMap::new());
        let html = render_systems_status_html(&payload);
        assert!(
            html.contains("Host uptime 1.5 hours."),
            "HTML must show the formatted uptime: {html}"
        );
        assert_eq!(payload.uptime, "1.5 hours");
    }

    /// Inventory names mail, UI, status, Arti, and Splora testnet (not mainnet).
    #[test]
    fn systems_status_inventory_lists_mail_ui_status_arti_splora_testnet() {
        let cfg = AppConfig::example_test();
        let items = systems_status_inventory(&cfg);
        let ids: Vec<&str> = items.iter().map(|i| i.id).collect();
        for must in [
            "mail",
            "console",
            "status",
            "arti",
            "splora-testnet3",
            "splora-testnet4",
            "splora-mutinynet",
            "splora-liquid",
            "sshd",
        ] {
            assert!(
                ids.contains(&must),
                "inventory must name {must}; got {ids:?}"
            );
        }
        assert!(
            !ids.iter().any(|id| id.contains("mainnet")),
            "must not revive Splora mainnet: {ids:?}"
        );
        assert!(
            !ids.iter().any(|id| *id == "vaultwarden" || *id == "lake"),
            "must not invent Vaultwarden or Lake: {ids:?}"
        );
        let payload = systems_status_from_config(&cfg);
        let names: Vec<&str> = payload.systems.iter().map(|c| c.name.as_str()).collect();
        assert!(names.len() > 5, "more than a stub handful: {names:?}");
        for must in ["mail", "console", "status", "arti", "splora-testnet3"] {
            assert!(
                names.contains(&must),
                "payload must name {must}; got {names:?}"
            );
        }
        let show = parse_systemctl_show(
            "Id=stalwart-mail.service\nLoadState=loaded\nActiveState=active\nSubState=running\n\n\
Id=sshd.service\nLoadState=loaded\nActiveState=failed\nSubState=failed\n",
        );
        assert_eq!(show["stalwart-mail.service"].active_state, "active");
        assert_eq!(show["sshd.service"].active_state, "failed");
    }
}
