//! Admin shell pages via **Leptos SSR** (Axum serves the HTML string).
//!
//! SSR-only scaffold: no WASM hydrate / cargo-leptos / NPM. Interactive
//! islands and Nostr auth stay residual (SEARCH_AND_UI Phase 1-2).
//!
//! Theme: **Surmount DOGE** v1.0.0 (pure 3-bit RGB, exactly eight colors).
//! Spec: <https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md>
//! Dark only; no grays or light-mode media queries.
//!
//! When host Arti HS is live and publishes, surface the onion address to the
//! operator (admin UI status and/or documented path). Do not invent a live
//! onion URL here; do not log onion addresses in failure tails.

use std::sync::Arc;

use axum::extract::State;
use axum::response::Html;
use leptos::prelude::*;

use crate::AppState;

/// Render the admin home shell to an HTML string (Leptos SSR).
pub fn render_admin_shell(domain: &str, mail_hostname: &str, version: &str) -> String {
    // Prebuild contiguous strings so ssr-only output has no hydration markers
    // (`<!>` from adjacent static text nodes or static+dynamic splits).
    let domain = domain.to_string();
    let services_host = format!("services.{domain}");
    // One text node: multiple adjacent "..." lits in view! emit `<!>` between them.
    let console_lead = "This is the Rust management UI for mail and site operations. \
Stalwart handles SMTP/IMAP/JMAP; this service will grow into the \
primary operator and user-facing surface at "
        .to_string();
    let mail = mail_hostname.to_string();
    let footer = format!("surmount-management-ui {version} · NixOS + Stalwart + Axum + Leptos SSR");
    view! {
        <AdminShell
            domain=domain
            console_lead=console_lead
            services_host=services_host
            mail=mail
            footer=footer
        />
    }
    .to_html()
}

#[component]
fn AdminShell(
    domain: String,
    console_lead: String,
    services_host: String,
    mail: String,
    footer: String,
) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="doge">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="color-scheme" content="only dark"/>
                <title>"Surmount Services"</title>
                <style>{ADMIN_SHELL_CSS}</style>
            </head>
            <body data-surmount-ssr="leptos" data-theme="doge">
                <header>
                    <h1>"Surmount Services"</h1>
                    <span>{domain}</span>
                    <span class="pill">"skeleton"</span>
                </header>
                <main>
                    <div class="card">
                        <h2>"Management console"</h2>
                        <p>
                            {console_lead}
                            <code>{services_host}</code>
                            "."
                        </p>
                    </div>
                    <div class="card">
                        <h2>"API"</h2>
                        <ul>
                            <li>
                                <a href="/health">
                                    <code>"GET /health"</code>
                                </a>
                            </li>
                            <li>
                                <a href="/api/v1/domains">
                                    <code>"GET /api/v1/domains"</code>
                                </a>
                            </li>
                            <li>
                                <a href="/api/v1/accounts">
                                    <code>"GET /api/v1/accounts"</code>
                                </a>
                            </li>
                            <li>
                                <a href="/api/v1/stalwart/status">
                                    <code>"GET /api/v1/stalwart/status"</code>
                                </a>
                            </li>
                            <li>
                                <code>"POST /api/v1/jmap (501 placeholder)"</code>
                            </li>
                        </ul>
                    </div>
                    <div class="card">
                        <h2>"Mail engine"</h2>
                        <p>
                            "Hostname: "
                            <code>{mail}</code>
                            ". Temporary Stalwart admin: SSH tunnel to "
                            <code>"127.0.0.1:8081"</code>
                            ", or path "
                            <code>"/stalwart-admin/"</code>
                            " on this host while bootstrapping."
                        </p>
                    </div>
                    <footer>{footer}</footer>
                </main>
            </body>
        </html>
    }
}

/// Surmount DOGE v1.0.0 palette only (pure 3-bit RGB; eight hex values).
/// Spec: https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md
/// Semantic roles: cyan accent/muted, yellow links, green pill, white fg,
/// black bg (emissive off). No grays. No light-mode media queries.
const ADMIN_SHELL_CSS: &str = r#"
:root, [data-theme="doge"] {
  color-scheme: only dark;
  --bg: #000000;
  --fg: #FFFFFF;
  --muted: #00FFFF;
  --accent: #00FFFF;
  --link: #FFFF00;
  --card: #000000;
  --border: #FFFFFF;
  --ok: #00FF00;
  --error: #FF0000;
  --code-fg: #00FFFF;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif;
  background: var(--bg);
  color: var(--fg);
  line-height: 1.5;
  min-height: 100vh;
}
header {
  border-bottom: 1px solid var(--border);
  padding: 1.25rem 1.5rem;
  display: flex;
  align-items: baseline;
  gap: 1rem;
}
header h1 {
  font-size: 1.15rem;
  font-weight: 600;
  margin: 0;
  letter-spacing: 0.02em;
  color: var(--fg);
}
header span { color: var(--muted); font-size: 0.9rem; }
main {
  max-width: 52rem;
  margin: 0 auto;
  padding: 2rem 1.5rem 4rem;
}
.card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 0;
  padding: 1.25rem 1.4rem;
  margin-bottom: 1rem;
}
h2 { font-size: 1rem; margin: 0 0 0.6rem; color: var(--fg); }
p, li { color: var(--muted); }
a { color: var(--link); }
a:hover { color: var(--accent); }
code {
  background: var(--bg);
  border: 1px solid var(--border);
  padding: 0.1em 0.35em;
  border-radius: 0;
  font-size: 0.9em;
  color: var(--code-fg);
}
ul { padding-left: 1.2rem; }
.pill {
  display: inline-block;
  background: var(--bg);
  border: 1px solid var(--ok);
  color: var(--ok);
  font-size: 0.75rem;
  padding: 0.15rem 0.5rem;
  border-radius: 0;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
footer {
  margin-top: 2rem;
  font-size: 0.85rem;
  color: var(--muted);
}
"#;

pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(render_admin_shell(
        &state.config.primary_domain,
        &state.config.mail_hostname,
        env!("CARGO_PKG_VERSION"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Surmount DOGE v1.0.0 pure 3-bit RGB (exactly eight colors).
    /// Spec: https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md
    const DOGE_HEX: &[&str] = &[
        "#FF0000", "#00FF00", "#0000FF", "#00FFFF", "#FF00FF", "#FFFF00", "#000000",
        "#FFFFFF",
    ];

    /// Midtones / grays from the pre-DOGE admin shell (must not reappear).
    const FORBIDDEN_HEX: &[&str] = &[
        "#0f1419", "#8b9aab", "#1a2332", "#3d9cf0", "#e7ecf1", "#3ecf8e", "#243044",
        "#0c1017", "#c5d4e8", "#143024",
    ];

    fn collect_hex_colors(s: &str) -> Vec<String> {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'#' {
                let start = i + 1;
                let mut end = start;
                while end < bytes.len() && end - start < 8 && bytes[end].is_ascii_hexdigit() {
                    end += 1;
                }
                let len = end - start;
                if len == 3 || len == 6 {
                    out.push(format!("#{}", s[start..end].to_ascii_uppercase()));
                }
                i = end;
            } else {
                i += 1;
            }
        }
        out
    }

    fn is_doge_hex(hex: &str) -> bool {
        DOGE_HEX.iter().any(|d| d.eq_ignore_ascii_case(hex))
    }

    #[test]
    fn render_admin_shell_is_leptos_ssr_html() {
        let html = render_admin_shell("example.test", "mail.example.test", "0.1.0");
        assert!(
            html.contains(r#"data-surmount-ssr="leptos""#),
            "missing Leptos SSR marker"
        );
        assert!(html.contains("Management console"));
        assert!(html.contains("example.test"));
        assert!(html.contains("mail.example.test"));
        assert!(html.contains("Leptos SSR"));
    }

    #[test]
    fn render_admin_shell_services_host_is_contiguous() {
        let html = render_admin_shell("example.test", "mail.example.test", "0.1.0");
        assert!(
            html.contains("services.example.test"),
            "expected contiguous services.{{domain}} without SSR position markers; got snippet: {}",
            html.chars().take(400).collect::<String>()
        );
        assert!(
            !html.contains("<!>"),
            "ssr-only shell should not emit Leptos hydration markers"
        );
    }

    /// Named contract: admin shell CSS/styles use only DOGE eight-color hex;
    /// dark-only (no light prefers-color-scheme); no pre-DOGE midtone grays.
    #[test]
    fn render_admin_shell_uses_only_doge_palette() {
        let html = render_admin_shell("example.test", "mail.example.test", "0.1.0");

        assert!(
            html.contains(r#"data-theme="doge""#),
            "admin shell must declare data-theme=doge"
        );
        assert!(
            !html.to_ascii_lowercase().contains("@media (prefers-color-scheme: light)"),
            "DOGE is dark-only; must not ship light prefers-color-scheme media queries"
        );
        assert!(
            html.contains("color-scheme: only dark") || html.contains("color-scheme:only dark"),
            "expected color-scheme: only dark for dark-only DOGE"
        );

        for bad in FORBIDDEN_HEX {
            assert!(
                !html.to_ascii_lowercase().contains(&bad.to_ascii_lowercase()),
                "forbidden pre-DOGE midtone {bad} must not appear in admin shell"
            );
        }

        let colors = collect_hex_colors(&html);
        assert!(
            !colors.is_empty(),
            "expected hex colors in rendered admin shell CSS"
        );
        for c in &colors {
            assert!(
                is_doge_hex(c),
                "non-DOGE color {c} in admin shell; only pure 3-bit RGB eight allowed"
            );
        }

        let has = |h: &str| colors.iter().any(|c| c.eq_ignore_ascii_case(h));
        assert!(has("#000000"), "DOGE black background required");
        assert!(has("#FFFFFF"), "DOGE white foreground required");
        assert!(
            has("#00FFFF") || has("#FFFF00") || has("#00FF00"),
            "expected at least one DOGE accent (cyan, yellow, or green)"
        );
    }
}
