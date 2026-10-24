//! Public systems dashboard for Host `status.{primary}` (Axum, not nginx).
//!
//! Not the operator console. Payload fields come from process config
//! (listen mode, mail hostname present, MTA-STS mode). No RAM, disk, IP,
//! onion, or other live host facts.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::config::AppConfig;
use crate::mta_sts::MtaStsMode;
use crate::redirect::status_hostname;

/// One named systems row the dashboard emits (name/role/state, not live facts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemsComponent {
    pub name: &'static str,
    pub role: &'static str,
    pub state: String,
}

/// Representative systems status body. Tests assert these field names exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemsStatusPayload {
    pub surface: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub host: String,
    pub primary_domain: String,
    pub systems: Vec<SystemsComponent>,
}

/// Build the payload the status Host handler actually emits.
pub fn systems_status_from_config(config: &AppConfig) -> SystemsStatusPayload {
    let host = status_hostname(&config.primary_domain);
    let edge_state = if config.listen_mode.is_https() {
        "https"
    } else {
        "http"
    };
    let tls_state = if config.listen_mode.is_https() {
        "leaf"
    } else {
        "plain_http"
    };
    let mail_state = if config.mail_hostname.trim().is_empty() {
        "unset"
    } else {
        "configured"
    };
    let mta_state = match config.mta_sts_mode {
        MtaStsMode::Off => "off",
        MtaStsMode::Testing => "testing",
        MtaStsMode::Enforce => "enforce",
    };
    let console_state = if config.services_hostname.trim().is_empty() {
        "unset"
    } else {
        "services_hostname"
    };
    SystemsStatusPayload {
        surface: "systems_status",
        service: "surmount",
        version: env!("CARGO_PKG_VERSION"),
        host,
        primary_domain: config.primary_domain.clone(),
        systems: vec![
            SystemsComponent {
                name: "mail",
                role: "smtp_imap",
                state: mail_state.to_string(),
            },
            SystemsComponent {
                name: "edge",
                role: "https_axum",
                state: edge_state.to_string(),
            },
            SystemsComponent {
                name: "tls",
                role: "certificate",
                state: tls_state.to_string(),
            },
            SystemsComponent {
                name: "mta_sts",
                role: "policy",
                state: mta_state.to_string(),
            },
            SystemsComponent {
                name: "console",
                role: "operator",
                state: console_state.to_string(),
            },
        ],
    }
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

/// Public HTML dashboard. Named systems fields appear in the body.
pub fn render_systems_status_html(payload: &SystemsStatusPayload) -> String {
    let host = html_escape(&payload.host);
    let primary = html_escape(&payload.primary_domain);
    let version = html_escape(payload.version);
    let mut rows = String::new();
    for c in &payload.systems {
        let name = html_escape(c.name);
        let role = html_escape(c.role);
        let state = html_escape(&c.state);
        rows.push_str(&format!(
            "<tr data-system=\"{name}\"><th>{name}</th><td data-role=\"{role}\">{role}</td>\
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
  max-width: 36rem;
  margin: 2rem auto;
  padding: 0 1rem;
  flex: 1;
}}
.eyebrow {{ color: var(--muted); letter-spacing: 0.08em; text-transform: uppercase; font-size: 0.75rem; }}
h1 {{ font-size: 1.5rem; margin: 0.5rem 0 1rem; }}
.host {{ color: var(--muted); }}
table {{ width: 100%; border-collapse: collapse; }}
th, td {{ text-align: left; padding: 0.5rem 0; border-bottom: 1px solid var(--border); }}
th {{ color: var(--ok); }}
footer {{ padding: 1rem; color: var(--muted); font-size: 0.75rem; }}
</style>
</head>
<body data-theme="doge" data-surface="systems-status">
<main>
<p class="eyebrow">Surmount</p>
<h1>Systems status</h1>
<p class="host">{host}</p>
<p>Primary domain {primary}. Public systems dashboard on the Axum HTTPS edge. Not the operator console.</p>
<table>
<thead><tr><th>system</th><th>role</th><th>state</th></tr></thead>
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
        let names: Vec<&str> = payload.systems.iter().map(|c| c.name).collect();
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
        assert!(!html.contains("/mail"));
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
}
