//! Multi-page management console via **Leptos SSR** (Axum serves HTML strings).
//!
//! Real operator surface: overview, domains, accounts, system, mail, login.
//! SSR-only navigation (`<a href>`). No WASM hydrate / cargo-leptos / NPM.
//! Nostr auth foundation: rust-nostr NIP-98 + session cookie (mode off by
//! default). Full Q-AUTH-1 (key-loss, durable store, bootstrap UX) residual.
//!
//! Theme: **Surmount DOGE** v1.0.0 (pure 3-bit RGB, exactly eight colors).
//! Spec: <https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md>
//! Dark only; no grays or light-mode media queries.
//!
//! When host Arti HS is live and publishes, surface the onion address to the
//! operator (admin UI status and/or documented path). Do not invent a live
//! onion URL here; do not log onion addresses in failure tails.

use std::sync::Arc;

use axum::extract::{Extension, State};
use axum::response::Html;
use leptos::prelude::*;

use crate::api::{
    domains_inventory, probe_stalwart, AccountsInventory, DomainsInventory, StalwartStatus,
};
use crate::config::AppConfig;
use crate::tls::ListenMode;
use crate::AppState;
use surmount_management_ui::auth::CspNonce;
use surmount_management_ui::ban::BanEnforcement;

/// Shared view inputs for admin pages (pure; no live network).
#[derive(Debug, Clone)]
pub struct AdminPageData {
    pub path: String,
    pub primary_domain: String,
    pub mail_hostname: String,
    pub services_hostname: String,
    pub version: String,
    pub listen_summary: String,
    pub rate_limit_summary: String,
    pub ban_enforcement: String,
    pub stalwart_url: String,
    /// Operator-published onion (`http://….onion`) when configured; never invented.
    pub onion_url: Option<String>,
    pub stalwart: StalwartChip,
    pub domains: DomainsInventory,
    pub accounts: AccountsInventory,
    /// Auth mode label for banners (`off` / `nostr`).
    pub auth_mode: String,
}

/// Stalwart reachability for status chips (SSR at request time).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StalwartChip {
    Ok,
    Fail,
    /// Hermetic / no-probe renders (unit tests via `render_admin_shell`).
    /// Not constructed on live request paths (those always probe → Ok/Fail).
    #[cfg_attr(not(test), allow(dead_code))]
    Unknown,
}

impl StalwartChip {
    pub fn from_status(s: &StalwartStatus) -> Self {
        if s.reachable {
            Self::Ok
        } else {
            Self::Fail
        }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Ok => "chip chip-ok",
            Self::Fail => "chip chip-fail",
            Self::Unknown => "chip chip-unknown",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Ok => "Stalwart OK",
            Self::Fail => "Stalwart down",
            Self::Unknown => "Stalwart unknown",
        }
    }
}

fn listen_mode_label(mode: &ListenMode) -> &'static str {
    match mode {
        ListenMode::PlainHttp => "http (plain)",
        ListenMode::Https(_) => "https (rustls)",
    }
}

fn ban_enforcement_label(e: BanEnforcement) -> &'static str {
    match e {
        BanEnforcement::Off => "off",
        BanEnforcement::DryRun => "dry-run",
        BanEnforcement::Enforce => "enforce",
    }
}

fn page_data_from_config(
    config: &AppConfig,
    path: &str,
    chip: StalwartChip,
    accounts: AccountsInventory,
) -> AdminPageData {
    let rate = if config.rate_limit_max_requests == 0 {
        "disabled (max=0)".to_string()
    } else {
        format!(
            "{} req / {}s (max keys {})",
            config.rate_limit_max_requests,
            config.rate_limit_window.as_secs(),
            config.rate_limit_max_keys
        )
    };
    AdminPageData {
        path: path.to_string(),
        primary_domain: config.primary_domain.clone(),
        mail_hostname: config.mail_hostname.clone(),
        services_hostname: config.services_hostname.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        listen_summary: format!(
            "{} · {}",
            config.listen,
            listen_mode_label(&config.listen_mode)
        ),
        rate_limit_summary: rate,
        ban_enforcement: ban_enforcement_label(config.ban.enforcement).to_string(),
        stalwart_url: config.stalwart_url.clone(),
        onion_url: config.onion_url.clone(),
        stalwart: chip,
        domains: domains_inventory(config),
        accounts,
        auth_mode: config.auth.mode.as_str().to_string(),
    }
}

async fn page_data_from_state(state: &AppState, path: &str, chip: StalwartChip) -> AdminPageData {
    page_data_from_config(
        &state.config,
        path,
        chip,
        state.directory.list_accounts().await,
    )
}

fn auth_banner_text(mode: &str) -> String {
    match mode {
        "nostr" => "Auth: Nostr gate enabled (NIP-98 + session cookie; rust-nostr, not JS NDK). \
Empty allowlist is fail-closed. Q-AUTH-1 residual: key-loss, durable session store, \
first-operator bootstrap UX (not invented here). nsec never on server. Host enable: docs/OPS.md."
            .into(),
        _ => "Auth residual: mode=off (open console for local/dev). Not public-safe alone. \
Enable with SURMOUNT_AUTH_MODE=nostr + allowlist (env or file) + session secret. \
Q-AUTH-1 residual remains for key-loss and durable store product answers. See docs/OPS.md."
            .into(),
    }
}

/// Directory honesty line for overview/system (source badge + residual).
fn directory_banner_text(accounts_source: &str) -> String {
    match accounts_source {
        "mock" => "Directory: hermetic mock fixture (not live Stalwart). \
No HTML create-mailbox form; API POST/PATCH /api/v1/accounts when auth allows \
(nostr or lab escape) + CSRF on cookie POSTs."
            .into(),
        "unavailable" => "Directory residual: accounts API stays empty until \
SURMOUNT_DIRECTORY=stalwart + host token. Domains are config inventory only. \
No invent live mailboxes; no HTML create form in this UI."
            .into(),
        "stalwart" => "Directory: live Stalwart management JMAP \
(SURMOUNT_DIRECTORY=stalwart). Empty list means no principals or list failed \
fail-closed (no invented rows). API mutations: POST/PATCH /api/v1/accounts \
behind auth + CSRF when cookie; still no HTML create form."
            .into(),
        other => format!(
            "Directory source: {other}. No create-mailbox HTML form in this UI; \
see docs/OPS.md residual for operator tools."
        ),
    }
}

/// Unit-test / pure overview render (no live probe; chip unknown).
/// Keeps historical name used by hermetic unit tests.
#[cfg(test)]
pub fn render_admin_shell(domain: &str, mail_hostname: &str, version: &str) -> String {
    let services = format!("services.{domain}");
    let data = AdminPageData {
        path: "/".into(),
        primary_domain: domain.to_string(),
        mail_hostname: mail_hostname.to_string(),
        services_hostname: services,
        version: version.to_string(),
        listen_summary: "127.0.0.1:8080 · http (plain)".into(),
        rate_limit_summary: "120 req / 60s (max keys 10000)".into(),
        ban_enforcement: "off".into(),
        stalwart_url: "http://127.0.0.1:8080".into(),
        onion_url: None,
        auth_mode: "off".into(),
        stalwart: StalwartChip::Unknown,
        domains: DomainsInventory {
            domains: vec![
                crate::api::DomainEntry {
                    name: domain.to_string(),
                    role: "primary".into(),
                },
                crate::api::DomainEntry {
                    name: mail_hostname.to_string(),
                    role: "mail_hostname".into(),
                },
                crate::api::DomainEntry {
                    name: format!("services.{domain}"),
                    role: "services_hostname".into(),
                },
            ],
            source: "config",
            note: "Inventory from deploy/config, not Stalwart directory.".into(),
        },
        accounts: AccountsInventory {
            accounts: Vec::new(),
            source: "unavailable",
            note: "Not connected to mail directory.".into(),
        },
    };
    render_overview(&data)
}

pub fn render_overview(data: &AdminPageData) -> String {
    // Contiguous strings avoid Leptos SSR hydration markers (`<!>`).
    let domain = data.primary_domain.clone();
    let services_host = data.services_hostname.clone();
    let mail = data.mail_hostname.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let domain_count = data.domains.domains.len();
    let domains_source = data.domains.source.to_string();
    let status_chip_class = data.stalwart.class().to_string();
    let status_chip_label = data.stalwart.label().to_string();
    let stalwart_after = match data.stalwart {
        StalwartChip::Ok => " Reachable (live probe).".to_string(),
        StalwartChip::Fail => " Unreachable (live probe failed).".to_string(),
        StalwartChip::Unknown => " Not probed in this render.".to_string(),
    };
    let (onion_chip_class, onion_chip_label, onion_after) = match &data.onion_url {
        Some(url) => (
            "chip chip-ok".to_string(),
            "Onion configured".to_string(),
            format!(" Operator-published: {url}"),
        ),
        None => (
            "chip chip-unknown".to_string(),
            "Onion not configured".to_string(),
            " Set SURMOUNT_ONION_URL or SURMOUNT_ONION_HOSTNAME_FILE after Arti publishes. Address is never invented."
                .to_string(),
        ),
    };
    let console_line = format!(
        "Operator console for mail and site operations at {services_host}. \
Stalwart is the mail engine; this service is the Surmount management edge."
    );
    let domain_count_line = format!(
        "{domain_count} domain row(s) from {domains_source} (config inventory, not live directory)."
    );
    let account_count = data.accounts.accounts.len();
    let accounts_line = if data.accounts.source == "unavailable" {
        format!("{account_count} accounts · directory unavailable (nothing invented).")
    } else {
        format!("{account_count} account(s) from {}.", data.accounts.source)
    };
    let primary_line = format!("Primary: {domain}");
    let mail_line = format!("Mail: {mail}");
    let services_line = format!("Services: {services_host}");
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let auth_banner = auth_banner_text(&data.auth_mode);
    let directory_banner = directory_banner_text(data.accounts.source);
    let auth_is_nostr = data.auth_mode == "nostr";

    view! {
        <AdminDocument
            title="Overview · Surmount Services".to_string()
            domain=domain.clone()
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
        >
            <div class="card">
                <h2>"Management console"</h2>
                <p class="banner">{auth_banner}</p>
                {if auth_is_nostr {
                    view! {
                        <p class="banner">
                            "Sign in: "
                            <a href="/login">"/login"</a>
                            " (NIP-07 or curl NIP-98 session exchange)."
                        </p>
                    }
                    .into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <p class="banner">{directory_banner}</p>
                <p class="note">
                    "Ops residual (no fake host data here): "
                    <code>"docs/OPS.md"</code>
                    " · "
                    <code>"docs/SECURITY.md"</code>
                    " auth-failure ban matrix."
                </p>
                <p>{console_line}</p>
            </div>
            <div class="card-grid">
                <div class="card">
                    <h2>"Service status"</h2>
                    <ul class="status-list">
                        <li>
                            <span class="chip chip-ok">"UI OK"</span>
                            " Management UI health (this process)."
                        </li>
                        <li>
                            <span class=status_chip_class>{status_chip_label}</span>
                            {stalwart_after}
                        </li>
                        <li>
                            <span class=onion_chip_class>{onion_chip_label}</span>
                            {onion_after}
                        </li>
                    </ul>
                </div>
                <div class="card">
                    <h2>"Inventory"</h2>
                    <ul class="status-list">
                        <li>{domain_count_line}</li>
                        <li>{accounts_line}</li>
                    </ul>
                </div>
            </div>
            <div class="card">
                <h2>"Hostnames"</h2>
                <ul class="status-list">
                    <li>
                        <code>{primary_line}</code>
                    </li>
                    <li>
                        <code>{mail_line}</code>
                    </li>
                    <li>
                        <code>{services_line}</code>
                    </li>
                </ul>
            </div>
            <div class="card">
                <h2>"Quick links"</h2>
                <ul>
                    <li>
                        <a href="/domains">"Domains"</a>
                        " · config inventory"
                    </li>
                    <li>
                        <a href="/accounts">"Accounts"</a>
                        " · directory inventory (default honest empty; live when configured)"
                    </li>
                    <li>
                        <a href="/system">"System"</a>
                        " · version, listen, rate limit, ban, onion"
                    </li>
                    <li>
                        <a href="/mail">"Mail"</a>
                        " · Stalwart URL and live probe"
                    </li>
                    {if auth_is_nostr {
                        view! {
                            <li>
                                <a href="/login">"Login"</a>
                                " · Nostr NIP-98 / session (mode=nostr)"
                            </li>
                        }
                        .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </ul>
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
                        <a href="/api/v1/system">
                            <code>"GET /api/v1/system"</code>
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
                        <code>"POST /api/v1/jmap (501 not implemented)"</code>
                    </li>
                </ul>
            </div>
        </AdminDocument>
    }
    .to_html()
}

pub fn render_domains_page(data: &AdminPageData) -> String {
    let domain = data.primary_domain.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let note = data.domains.note.clone();
    let source_badge = data.domains.source.to_string();
    let source_line = format!(
        "Source badge: {source_badge}. Rows mirror deploy/config hostnames (SURMOUNT_*), not a live Stalwart directory query."
    );
    let rows: Vec<(String, String, String)> = data
        .domains
        .domains
        .iter()
        .map(|d| {
            (
                d.name.clone(),
                d.role.clone(),
                "config inventory".to_string(),
            )
        })
        .collect();
    let empty = rows.is_empty();
    let empty_msg =
        "No domain rows. Config inventory is empty (unexpected for a normal deploy).".to_string();

    view! {
        <AdminDocument
            title="Domains · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
        >
            <div class="card">
                <h2>"Domains"</h2>
                <p>
                    <span class="badge">"source: config"</span>
                </p>
                <p>{source_line}</p>
                <p class="note">{note}</p>
                <p class="note">
                    "Config inventory vs live directory: this page does not list Stalwart principals or DNS zones from the engine. Use domains here for operator orientation; use stalwart-cli or bootstrap admin for engine directory work."
                </p>
                {if empty {
                    view! { <p class="empty">{empty_msg}</p> }.into_any()
                } else {
                    view! {
                        <table>
                            <thead>
                                <tr>
                                    <th>"Hostname"</th>
                                    <th>"Role"</th>
                                    <th>"Source note"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows
                                    .into_iter()
                                    .map(|(name, role, src)| {
                                        view! {
                                            <tr>
                                                <td>
                                                    <code>{name}</code>
                                                </td>
                                                <td>{role}</td>
                                                <td>{src}</td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()}
                            </tbody>
                        </table>
                    }
                    .into_any()
                }}
            </div>
        </AdminDocument>
    }
    .to_html()
}

pub fn render_accounts_page(data: &AdminPageData) -> String {
    let domain = data.primary_domain.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let note = data.accounts.note.clone();
    let source = data.accounts.source;
    let source_badge = format!("source: {source}");
    let source_line = match source {
        "mock" => format!("Source: {source}. Hermetic mock fixture (not live Stalwart)."),
        "stalwart" => format!(
            "Source: {source}. Live Stalwart management JMAP list (explicit SURMOUNT_DIRECTORY=stalwart)."
        ),
        _ => format!("Source: {source}. Not a live Stalwart mailbox directory."),
    };
    // Production default is honest empty; mock/stalwart may list principals.
    let empty = data.accounts.accounts.is_empty();
    let list_body = if empty {
        if source == "unavailable" {
            "No accounts listed. Directory unavailable; nothing invented.".to_string()
        } else if source == "stalwart" {
            "No accounts listed from live Stalwart (empty engine directory or list failed fail-closed; nothing invented).".to_string()
        } else {
            format!("No accounts listed (source: {source}).")
        }
    } else {
        data.accounts
            .accounts
            .iter()
            // status wire field holds principal kind (@type) when live, not lifecycle.
            .map(|a| format!("{} {} (kind: {})", a.id, a.address, a.status))
            .collect::<Vec<_>>()
            .join("; ")
    };
    let next_steps = match source {
        "mock" => "Mock directory active (tests / SURMOUNT_DIRECTORY=mock). \
API create/update: POST /api/v1/accounts and PATCH /api/v1/accounts/{id} \
(auth gate + CSRF when session cookie). For live principals set \
SURMOUNT_DIRECTORY=stalwart with a host token \
(see docs/research/stalwart-directory-api.md)."
            .to_string(),
        "stalwart" => "Live Stalwart directory client active. List via GET /api/v1/accounts; \
create/update via POST/PATCH /api/v1/accounts (nostr auth or lab escape; \
CSRF on cookie POSTs). Mail passwords still via stalwart-cli. No HTML form."
            .to_string(),
        _ => "What to do next: enable live listing with SURMOUNT_DIRECTORY=stalwart + \
host API token (SURMOUNT_STALWART_TOKEN or TOKEN_FILE), or create principals with \
stalwart-cli / bootstrap admin / authenticated API when directory is on \
(see docs/research/stalwart-directory-api.md)."
            .to_string(),
    };

    view! {
        <AdminDocument
            title="Accounts · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
        >
            <div class="card">
                <h2>"Accounts"</h2>
                <p>
                    <span class="badge">{source_badge}</span>
                </p>
                <p>{source_line}</p>
                <p class="note">{note}</p>
                <p class="empty">{list_body}</p>
                <p class="note">{next_steps}</p>
                <p class="note">
                    "No create-mailbox HTML form in this UI (honesty). Authenticated API mutations when directory is mock/stalwart; else stalwart-cli / bootstrap admin. See docs/OPS.md residual."
                </p>
            </div>
        </AdminDocument>
    }
    .to_html()
}

pub fn render_system_page(data: &AdminPageData) -> String {
    let domain = data.primary_domain.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let version = data.version.clone();
    let listen = data.listen_summary.clone();
    let rate = data.rate_limit_summary.clone();
    let ban = data.ban_enforcement.clone();
    let stalwart_url = data.stalwart_url.clone();
    let (onion_label, onion_body) = match &data.onion_url {
        Some(url) => (
            "Onion (operator-published)".to_string(),
            url.clone(),
        ),
        None => (
            "Onion".to_string(),
            "Not configured. Set SURMOUNT_ONION_URL or SURMOUNT_ONION_HOSTNAME_FILE when Arti publishes. Address is never invented in tree.".to_string(),
        ),
    };
    let onion_is_link = data.onion_url.is_some();
    let onion_href = data.onion_url.clone().unwrap_or_default();
    let onion_href_attr = onion_href.clone();
    let onion_href_text = onion_href;
    let auth_mode = data.auth_mode.clone();
    let auth_detail = auth_banner_text(&data.auth_mode);
    let auth_is_nostr = data.auth_mode == "nostr";
    let directory_banner = directory_banner_text(data.accounts.source);

    view! {
        <AdminDocument
            title="System · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
        >
            <div class="card">
                <h2>"System"</h2>
                <p class="banner">{auth_detail}</p>
                {if auth_is_nostr {
                    view! {
                        <p class="banner">
                            "Login: "
                            <a href="/login">"/login"</a>
                            " when mode=nostr."
                        </p>
                    }
                    .into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <p class="banner">{directory_banner}</p>
                <p class="note">
                    "Ops residual (no fake inventory): "
                    <code>"docs/OPS.md"</code>
                    " · auth-failure ban matrix in "
                    <code>"docs/SECURITY.md"</code>
                    "."
                </p>
                <table class="defs">
                    <tbody>
                        <tr>
                            <th>"Service"</th>
                            <td>
                                <code>"surmount-management-ui"</code>
                            </td>
                        </tr>
                        <tr>
                            <th>"Version"</th>
                            <td>{version}</td>
                        </tr>
                        <tr>
                            <th>"Listen mode"</th>
                            <td>
                                <code>{listen}</code>
                            </td>
                        </tr>
                        <tr>
                            <th>"Rate limit"</th>
                            <td>{rate}</td>
                        </tr>
                        <tr>
                            <th>"Ban enforcement"</th>
                            <td>{ban}</td>
                        </tr>
                        <tr>
                            <th>"Stalwart URL"</th>
                            <td>
                                <code>{stalwart_url}</code>
                            </td>
                        </tr>
                        <tr>
                            <th>{onion_label}</th>
                            <td>
                                {if onion_is_link {
                                    view! {
                                        <a href=onion_href_attr>
                                            <code>{onion_href_text}</code>
                                        </a>
                                    }
                                    .into_any()
                                } else {
                                    view! { <span class="note">{onion_body}</span> }.into_any()
                                }}
                            </td>
                        </tr>
                        <tr>
                            <th>"Auth mode"</th>
                            <td>
                                <code>{auth_mode}</code>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
            <div class="card">
                <h2>"Health and JSON"</h2>
                <ul>
                    <li>
                        <a href="/health">
                            <code>"GET /health"</code>
                        </a>
                        " liveness"
                    </li>
                    <li>
                        <a href="/api/v1/system">
                            <code>"GET /api/v1/system"</code>
                        </a>
                        " system status (includes onion when set)"
                    </li>
                    <li>
                        <a href="/api/v1/stalwart/status">
                            <code>"GET /api/v1/stalwart/status"</code>
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
                </ul>
            </div>
        </AdminDocument>
    }
    .to_html()
}

pub fn render_mail_page(data: &AdminPageData, status: &StalwartStatus) -> String {
    let domain = data.primary_domain.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let host_line = format!("Hostname: {}", data.mail_hostname);
    let url_line = format!("Configured URL (SURMOUNT_STALWART_URL): {}", status.url);
    let reachable = if status.reachable {
        "reachable"
    } else {
        "unreachable"
    };
    let detail = if let Some(code) = status.status {
        format!("HTTP status {code}")
    } else if let Some(ref err) = status.error {
        // Avoid leaking full onion if mis-pointed; redact for display.
        crate::config::redact_onion_in_text(err)
    } else {
        "no detail".to_string()
    };
    let result_class = if status.reachable {
        "chip chip-ok"
    } else {
        "chip chip-fail"
    }
    .to_string();
    let probe_after = format!(" · {detail}");
    let reachable = reachable.to_string();
    let guidance = if status.reachable {
        "Stalwart HTTP answered. Mail protocols still depend on host DNS, firewall, and account setup. JMAP proxy in this UI is 501 (not implemented); point clients at Stalwart HTTP or wait for residual proxy work."
            .to_string()
    } else {
        "When down: start Stalwart, check SURMOUNT_STALWART_URL (module default often http://127.0.0.1:8080), or use SSH tunnel / bootstrap /stalwart-admin/ while bringing the host up. JMAP proxy is 501 (not implemented)."
            .to_string()
    };

    view! {
        <AdminDocument
            title="Mail · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
        >
            <div class="card">
                <h2>"Mail engine"</h2>
                <p>{host_line}</p>
                <p>{url_line}</p>
            </div>
            <div class="card">
                <h2>"Probe result"</h2>
                <p>
                    "Live probe: "
                    <span class=result_class>{reachable}</span>
                    {probe_after}
                </p>
                <p class="note">{guidance}</p>
                <p class="banner">
                    "JMAP residual: POST /api/v1/jmap returns 501 until authenticated proxy lands."
                </p>
                <p>
                    <a href="/api/v1/stalwart/status">
                        <code>"GET /api/v1/stalwart/status"</code>
                    </a>
                    " JSON"
                </p>
            </div>
        </AdminDocument>
    }
    .to_html()
}

fn footer_line(version: &str) -> String {
    format!(
        "surmount-management-ui {version} · Surmount operator console · NixOS + Stalwart + Axum + Leptos SSR"
    )
}

struct NavClasses {
    overview: String,
    domains: String,
    accounts: String,
    system: String,
    mail: String,
}

fn nav_classes(path: &str) -> NavClasses {
    let active = |p: &str| {
        if path == p {
            "active".to_string()
        } else {
            String::new()
        }
    };
    NavClasses {
        overview: active("/"),
        domains: active("/domains"),
        accounts: active("/accounts"),
        system: active("/system"),
        mail: active("/mail"),
    }
}

/// Shared document chrome: header, primary nav, main slot, footer, DOGE CSS.
#[component]
fn AdminDocument(
    title: String,
    domain: String,
    footer: String,
    chip_class: String,
    chip_label: String,
    nav: NavClasses,
    children: Children,
) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="doge">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="color-scheme" content="only dark"/>
                <title>{title}</title>
                <style>{ADMIN_SHELL_CSS}</style>
            </head>
            <body data-surmount-ssr="leptos" data-theme="doge">
                <header>
                    <div class="brand">
                        <h1>"Surmount Services"</h1>
                        <span class="domain">{domain}</span>
                        <span class="pill muted">"local operator"</span>
                    </div>
                    <div class="header-meta">
                        <span class=chip_class>{chip_label}</span>
                    </div>
                </header>
                <nav aria-label="Primary">
                    <a href="/" class=nav.overview>
                        "Overview"
                    </a>
                    <a href="/domains" class=nav.domains>
                        "Domains"
                    </a>
                    <a href="/accounts" class=nav.accounts>
                        "Accounts"
                    </a>
                    <a href="/system" class=nav.system>
                        "System"
                    </a>
                    <a href="/mail" class=nav.mail>
                        "Mail"
                    </a>
                </nav>
                <main>{children()}</main>
                <footer class="site-footer">{footer}</footer>
            </body>
        </html>
    }
}

/// Surmount DOGE v1.0.0 palette only (pure 3-bit RGB; eight hex values).
/// Spec: https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md
/// Semantic roles: cyan accent/muted, yellow links, green/red/yellow chips,
/// white fg, black bg. No grays. No light-mode media queries.
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
  display: flex;
  flex-direction: column;
}
header {
  border-bottom: 1px solid var(--border);
  padding: 1rem 1.5rem;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  flex-wrap: wrap;
}
.brand {
  display: flex;
  align-items: baseline;
  gap: 1rem;
  flex-wrap: wrap;
}
header h1 {
  font-size: 1.15rem;
  font-weight: 600;
  margin: 0;
  letter-spacing: 0.02em;
  color: var(--fg);
}
.domain { color: var(--muted); font-size: 0.9rem; }
nav {
  border-bottom: 1px solid var(--border);
  padding: 0.5rem 1.5rem;
  display: flex;
  gap: 1.25rem;
  flex-wrap: wrap;
}
nav a {
  color: var(--link);
  text-decoration: none;
  font-size: 0.95rem;
  padding: 0.25rem 0;
  border-bottom: 2px solid transparent;
}
nav a:hover { color: var(--accent); }
nav a.active {
  color: var(--fg);
  border-bottom-color: var(--ok);
  font-weight: 600;
}
main {
  max-width: 52rem;
  margin: 0 auto;
  padding: 2rem 1.5rem 2rem;
  width: 100%;
  flex: 1;
}
.card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 0;
  padding: 1.25rem 1.4rem;
  margin-bottom: 1rem;
}
.card-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 0;
}
@media (min-width: 40rem) {
  .card-grid {
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
  }
  .card-grid .card { margin-bottom: 1rem; }
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
.status-list { list-style: none; padding-left: 0; }
.status-list li { margin-bottom: 0.5rem; }
.pill {
  display: inline-block;
  background: var(--bg);
  border: 1px solid var(--muted);
  color: var(--muted);
  font-size: 0.75rem;
  padding: 0.15rem 0.5rem;
  border-radius: 0;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.pill.muted { border-color: var(--muted); color: var(--muted); }
.badge {
  display: inline-block;
  background: var(--bg);
  border: 1px solid var(--accent);
  color: var(--accent);
  font-size: 0.75rem;
  padding: 0.15rem 0.5rem;
  border-radius: 0;
  font-weight: 600;
  letter-spacing: 0.03em;
  text-transform: uppercase;
}
.chip {
  display: inline-block;
  background: var(--bg);
  border: 1px solid var(--border);
  font-size: 0.75rem;
  padding: 0.15rem 0.5rem;
  border-radius: 0;
  font-weight: 600;
  letter-spacing: 0.03em;
  text-transform: uppercase;
}
.chip-ok { border-color: var(--ok); color: var(--ok); }
.chip-fail { border-color: var(--error); color: var(--error); }
.chip-unknown { border-color: var(--link); color: var(--link); }
.banner {
  border: 1px solid var(--link);
  padding: 0.6rem 0.75rem;
  margin: 0 0 0.75rem;
  color: var(--link);
}
.note { font-size: 0.9rem; }
.empty { color: var(--muted); font-style: italic; }
table {
  width: 100%;
  border-collapse: collapse;
  margin-top: 0.75rem;
}
th, td {
  text-align: left;
  padding: 0.4rem 0.5rem;
  border: 1px solid var(--border);
  color: var(--muted);
}
th { color: var(--fg); font-weight: 600; }
table.defs th {
  width: 11rem;
  vertical-align: top;
  color: var(--fg);
}
.site-footer {
  border-top: 1px solid var(--border);
  margin-top: auto;
  padding: 1rem 1.5rem 1.5rem;
  font-size: 0.85rem;
  color: var(--muted);
  max-width: 52rem;
  margin-left: auto;
  margin-right: auto;
  width: 100%;
}
"#;

pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    let status = probe_stalwart(&state).await;
    let chip = StalwartChip::from_status(&status);
    let data = page_data_from_state(&state, "/", chip).await;
    Html(render_overview(&data))
}

pub async fn domains_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let status = probe_stalwart(&state).await;
    let chip = StalwartChip::from_status(&status);
    let data = page_data_from_state(&state, "/domains", chip).await;
    Html(render_domains_page(&data))
}

pub async fn accounts_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let status = probe_stalwart(&state).await;
    let chip = StalwartChip::from_status(&status);
    let data = page_data_from_state(&state, "/accounts", chip).await;
    Html(render_accounts_page(&data))
}

pub async fn system_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let status = probe_stalwart(&state).await;
    let chip = StalwartChip::from_status(&status);
    let data = page_data_from_state(&state, "/system", chip).await;
    Html(render_system_page(&data))
}

pub async fn mail_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let status = probe_stalwart(&state).await;
    let chip = StalwartChip::from_status(&status);
    let data = page_data_from_state(&state, "/mail", chip).await;
    Html(render_mail_page(&data, &status))
}

/// Login page (public when mode=nostr). Optional vanilla NIP-07 script (no NPM).
///
/// CSP nonce comes from security-headers middleware (request extension) so the
/// inline script matches `script-src 'nonce-…'` without third-party hosts.
pub async fn login_page(
    State(state): State<Arc<AppState>>,
    Extension(nonce): Extension<CspNonce>,
) -> Html<String> {
    let data = page_data_from_state(&state, "/login", StalwartChip::Unknown).await;
    let host = state.config.services_hostname.clone();
    Html(render_login_page(&data, &host, nonce.as_str()))
}

/// Pure login HTML (DOGE palette).
///
/// `csp_nonce` is embedded on the script tag (no inline `onclick`; listeners
/// attach from the nonced script so CSP stays tight).
pub fn render_login_page(data: &AdminPageData, services_host: &str, csp_nonce: &str) -> String {
    let domain = data.primary_domain.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let mode = data.auth_mode.clone();
    let mode_note = auth_banner_text(&data.auth_mode);
    let challenge_url = "/api/v1/auth/challenge".to_string();
    let session_path = "/api/v1/auth/session".to_string();
    let services = services_host.to_string();
    let nonce = csp_nonce.to_string();
    // Inline script: optional window.nostr (NIP-07). No NPM / no NDK.
    // No inline event handlers (CSP script-src nonce only).
    let script = r#"
async function surmountNostrLogin() {
  var status = document.getElementById('login-status');
  function set(msg) { if (status) status.textContent = msg; }
  try {
    if (!window.nostr || !window.nostr.signEvent) {
      set('No NIP-07 extension (window.nostr). Use curl/NIP-98 instructions below.');
      return;
    }
    var ch = await fetch('/api/v1/auth/challenge');
    var info = await ch.json();
    var created_at = Math.floor(Date.now() / 1000);
    var event = {
      kind: 27235,
      created_at: created_at,
      tags: [['u', info.url], ['method', info.method || 'POST']],
      content: ''
    };
    var signed = await window.nostr.signEvent(event);
    var b64 = btoa(JSON.stringify(signed));
    var res = await fetch('/api/v1/auth/session', {
      method: 'POST',
      headers: { 'Authorization': 'Nostr ' + b64, 'Content-Type': 'application/json' },
      body: '{}'
    });
    var body = await res.json().catch(function(){ return {}; });
    if (res.ok && body.ok) {
      set('Signed in as ' + (body.npub || 'ok') + '. Redirecting...');
      var params = new URLSearchParams(window.location.search);
      // Same contract as auth::sanitize_login_next (single leading / only).
      var next = params.get('next') || '/';
      if (typeof next !== 'string') next = '/';
      next = next.trim();
      if (!next || next.indexOf('://') !== -1 || next.charAt(0) !== '/' || next.indexOf('//') === 0 || next.indexOf('\\') !== -1) {
        next = '/';
      }
      window.location.href = next;
    } else {
      set('Login failed: ' + (body.error || res.status));
    }
  } catch (e) {
    set('Login error: ' + (e && e.message ? e.message : e));
  }
}
(function(){
  var btn = document.getElementById('surmount-login-btn');
  if (btn) btn.addEventListener('click', function(){ surmountNostrLogin(); });
})();
"#
    .to_string();

    view! {
        <AdminDocument
            title="Login · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
        >
            <div class="card">
                <h2>"Nostr login"</h2>
                <p class="banner">{mode_note}</p>
                <p>
                    "Mode: "
                    <code>{mode.clone()}</code>
                    " · services host "
                    <code>{services}</code>
                </p>
                <p>
                    "Product auth uses "
                    <strong>"rust-nostr"</strong>
                    " on the server (not JS NDK / no NPM). nsec never leaves your client."
                </p>
                {if mode == "nostr" {
                    view! {
                        <div>
                            <p>
                                <button type="button" id="surmount-login-btn">
                                    "Sign in with NIP-07 extension"
                                </button>
                            </p>
                            <p id="login-status" class="note">
                                "Optional: browser extension implementing window.nostr.signEvent."
                            </p>
                            <h3>"curl / NIP-98"</h3>
                            <ol>
                                <li>
                                    "GET "
                                    <code>{challenge_url}</code>
                                    " for absolute "
                                    <code>"u"</code>
                                    " URL and skew window."
                                </li>
                                <li>
                                    "Sign kind "
                                    <code>"27235"</code>
                                    " with tags "
                                    <code>"u"</code>
                                    " = session URL and "
                                    <code>"method"</code>
                                    " = POST."
                                </li>
                                <li>
                                    "POST "
                                    <code>{session_path}</code>
                                    " with header "
                                    <code>"Authorization: Nostr <base64(event JSON)>"</code>
                                    " (or JSON body "
                                    <code>"{{\"event\": ...}}"</code>
                                    ")."
                                </li>
                            </ol>
                            <p class="note">
                                "Allowlist: SURMOUNT_NOSTR_ALLOWLIST (npub or hex). Empty allowlist = fail-closed. \
Session sets CSRF cookie; cookie-authenticated POSTs need X-CSRF-Token."
                            </p>
                            <script nonce=nonce>{script}</script>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <div>
                            <p class="note">
                                "Auth is off (default for just dev). Console is open without login. \
Not public-safe alone. To gate: set SURMOUNT_AUTH_MODE=nostr, SURMOUNT_NOSTR_ALLOWLIST, \
SURMOUNT_SESSION_SECRET."
                            </p>
                            <p>
                                <a href="/">"Back to overview"</a>
                            </p>
                        </div>
                    }
                    .into_any()
                }}
            </div>
        </AdminDocument>
    }
    .to_html()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{accounts_inventory, DomainEntry};

    /// Surmount DOGE v1.0.0 pure 3-bit RGB (exactly eight colors).
    /// Spec: https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md
    const DOGE_HEX: &[&str] = &[
        "#FF0000", "#00FF00", "#0000FF", "#00FFFF", "#FF00FF", "#FFFF00", "#000000", "#FFFFFF",
    ];

    /// Midtones / grays from the pre-DOGE admin shell (must not reappear).
    const FORBIDDEN_HEX: &[&str] = &[
        "#0f1419", "#8b9aab", "#1a2332", "#3d9cf0", "#e7ecf1", "#3ecf8e", "#243044", "#0c1017",
        "#c5d4e8", "#143024",
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

    fn sample_data(path: &str) -> AdminPageData {
        sample_data_with_onion(path, None)
    }

    fn sample_data_with_onion(path: &str, onion: Option<String>) -> AdminPageData {
        AdminPageData {
            path: path.into(),
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            version: "0.1.0".into(),
            listen_summary: "127.0.0.1:8080 · http (plain)".into(),
            rate_limit_summary: "120 req / 60s".into(),
            ban_enforcement: "off".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_url: onion,
            stalwart: StalwartChip::Fail,
            domains: DomainsInventory {
                domains: vec![
                    DomainEntry {
                        name: "example.test".into(),
                        role: "primary".into(),
                    },
                    DomainEntry {
                        name: "mail.example.test".into(),
                        role: "mail_hostname".into(),
                    },
                    DomainEntry {
                        name: "services.example.test".into(),
                        role: "services_hostname".into(),
                    },
                ],
                source: "config",
                note: "Inventory from deploy/config, not Stalwart directory.".into(),
            },
            accounts: AccountsInventory {
                accounts: Vec::new(),
                source: "unavailable",
                note: "Not connected to mail directory.".into(),
            },
            auth_mode: "off".into(),
        }
    }

    fn assert_doge_palette_only(html: &str) {
        assert!(
            html.contains(r#"data-theme="doge""#),
            "admin shell must declare data-theme=doge"
        );
        assert!(
            !html
                .to_ascii_lowercase()
                .contains("@media (prefers-color-scheme: light)"),
            "DOGE is dark-only; must not ship light prefers-color-scheme media queries"
        );
        assert!(
            html.contains("color-scheme: only dark") || html.contains("color-scheme:only dark"),
            "expected color-scheme: only dark for dark-only DOGE"
        );

        for bad in FORBIDDEN_HEX {
            assert!(
                !html
                    .to_ascii_lowercase()
                    .contains(&bad.to_ascii_lowercase()),
                "forbidden pre-DOGE midtone {bad} must not appear in admin shell"
            );
        }

        let colors = collect_hex_colors(html);
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
        assert_doge_palette_only(&html);
    }

    /// Named contract: overview has SSR primary nav to section pages (no JS).
    #[test]
    fn home_has_primary_nav_links() {
        let html = render_admin_shell("example.test", "mail.example.test", "0.1.0");
        for href in ["/domains", "/accounts", "/system", "/mail"] {
            assert!(
                html.contains(&format!(r#"href="{href}""#)),
                "overview must link to {href}; snippet: {}",
                html.chars().take(500).collect::<String>()
            );
        }
    }

    /// Named contract: product chrome is not skeleton cosplay.
    #[test]
    fn home_has_no_skeleton_string() {
        let html = render_admin_shell("example.test", "mail.example.test", "0.1.0");
        assert!(
            !html.to_ascii_lowercase().contains("skeleton"),
            "home HTML must not contain SKELETON/skeleton product branding"
        );
    }

    /// Named contract: every admin HTML page uses only DOGE palette.
    #[test]
    fn all_admin_pages_use_only_doge_palette() {
        let overview = render_overview(&sample_data("/"));
        let domains = render_domains_page(&sample_data("/domains"));
        let accounts = render_accounts_page(&sample_data("/accounts"));
        let system = render_system_page(&sample_data("/system"));
        let mail = render_mail_page(
            &sample_data("/mail"),
            &StalwartStatus {
                reachable: false,
                status: None,
                error: Some("connection refused".into()),
                url: "http://127.0.0.1:8080".into(),
            },
        );
        for (name, html) in [
            ("overview", overview),
            ("domains", domains),
            ("accounts", accounts),
            ("system", system),
            ("mail", mail),
        ] {
            assert_doge_palette_only(&html);
            assert!(
                html.contains(r#"data-surmount-ssr="leptos""#),
                "{name} missing SSR marker"
            );
            assert!(
                !html.to_ascii_lowercase().contains("skeleton"),
                "{name} must not contain skeleton branding"
            );
        }
    }

    /// Named contract: accounts page does not assert fake live Stalwart accounts.
    #[test]
    fn accounts_page_honest_empty_without_directory() {
        let html = render_accounts_page(&sample_data("/accounts"));
        assert!(
            !html.contains("admin@"),
            "accounts page must not invent admin@ mailbox"
        );
        assert!(
            html.contains("unavailable") || html.contains("Not connected"),
            "accounts page should state directory unavailable"
        );
        assert!(
            html.contains("No accounts listed") || html.contains("nothing invented"),
            "accounts page should show honest empty state"
        );
    }

    #[test]
    fn accounts_inventory_shared_is_empty() {
        // Sanity: page data uses same honesty as API helper path.
        let cfg_note = accounts_inventory(&crate::config::AppConfig {
            listen: "127.0.0.1:8080".parse().unwrap(),
            http_redirect_listen: None,
            local_cleartext_listen: None,
            listen_mode: ListenMode::PlainHttp,
            redirect_http_to_https: false,
            redirect_allowed_hosts: vec![],
            https_allow_cleartext_escape: false,
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_url: None,
            rate_limit_max_requests: 0,
            rate_limit_window: std::time::Duration::from_secs(60),
            rate_limit_max_keys: 1,
            ban: surmount_management_ui::ban::BanConfig {
                enforcement: BanEnforcement::Off,
                backend_kind: surmount_management_ui::ban::BanBackendKind::Memory,
                whitelist: vec![],
                state_path: None,
                nft_exec_enabled: false,
                nft_bin: None,
                nft_helper_sock: None,
                nft_helper_bin: None,
            },
            auth: surmount_management_ui::auth::AuthConfig::off(),
            allow_directory_unauthenticated: false,
        });
        assert!(cfg_note.accounts.is_empty());
        assert_eq!(cfg_note.source, "unavailable");
    }

    /// Named contract: when onion is set, system/overview surface the address.
    #[test]
    fn onion_configured_shows_address_on_system_and_overview() {
        let onion =
            "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion".to_string();
        let system = render_system_page(&sample_data_with_onion("/system", Some(onion.clone())));
        assert!(
            system.contains(&onion),
            "system page must show configured onion"
        );
        assert!(
            system.contains("Onion (operator-published)") || system.contains("operator-published"),
            "system should label onion as operator-published"
        );
        let overview = render_overview(&sample_data_with_onion("/", Some(onion.clone())));
        assert!(
            overview.contains("Onion configured"),
            "overview chip when onion set"
        );
        assert!(
            overview.contains(&onion),
            "overview should include configured onion URL"
        );
        assert_doge_palette_only(&system);
        assert_doge_palette_only(&overview);
    }

    /// Named contract: without onion config, do not invent an address.
    #[test]
    fn onion_unset_shows_residual_not_invented() {
        let system = render_system_page(&sample_data("/system"));
        let overview = render_overview(&sample_data("/"));
        assert!(
            system.contains("Not configured") || system.contains("never invented"),
            "system residual when onion unset: {}",
            system.chars().take(800).collect::<String>()
        );
        assert!(
            overview.contains("Onion not configured"),
            "overview chip when onion unset"
        );
        // No fabricated 56-char v3 onion labels in default renders.
        for html in [&system, &overview] {
            assert!(
                !html.contains(".onion"),
                "must not invent .onion address when unset"
            );
        }
    }

    #[test]
    fn overview_shows_directory_unavailable_account_count() {
        let html = render_overview(&sample_data("/"));
        assert!(
            html.contains("directory unavailable") || html.contains("Directory residual"),
            "overview should mention directory unavailable"
        );
        assert!(html.contains("0 accounts") || html.contains("0 account"));
    }

    /// Named contract: mode=nostr surfaces /login; mode=off does not pretend gate.
    #[test]
    fn overview_and_system_auth_mode_honesty_and_login_link() {
        let mut off = sample_data("/");
        off.auth_mode = "off".into();
        let overview_off = render_overview(&off);
        assert!(
            overview_off.contains("mode=off") || overview_off.contains("Auth residual"),
            "overview should reflect auth off"
        );
        assert!(
            overview_off.contains("docs/OPS.md"),
            "overview should link OPS residual path in copy"
        );
        // Login href optional when off; must not claim gate enabled.
        assert!(!overview_off.contains("Nostr gate enabled"));

        let mut nostr = sample_data("/");
        nostr.auth_mode = "nostr".into();
        let overview_nostr = render_overview(&nostr);
        assert!(
            overview_nostr.contains("Nostr gate enabled"),
            "overview must reflect mode=nostr"
        );
        assert!(
            overview_nostr.contains(r#"href="/login""#),
            "overview must link /login when mode=nostr"
        );

        let mut system_nostr = sample_data("/system");
        system_nostr.auth_mode = "nostr".into();
        let system_html = render_system_page(&system_nostr);
        assert!(
            system_html.contains(r#"href="/login""#),
            "system must link /login when mode=nostr"
        );
        assert!(
            system_html.contains("docs/SECURITY.md") || system_html.contains("ban matrix"),
            "system should point at SECURITY ban matrix residual"
        );
        assert_doge_palette_only(&overview_nostr);
        assert_doge_palette_only(&system_html);
    }

    /// Named contract: mail SSR is honest about JMAP proxy 501 (no webmail cosplay).
    #[test]
    fn mail_page_documents_jmap_proxy_501_residual() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(&sample_data("/mail"), &status);
        let lower = html.to_ascii_lowercase();
        assert!(
            lower.contains("501") && lower.contains("jmap"),
            "mail page must document JMAP 501 residual; snippet: {}",
            html.chars().take(600).collect::<String>()
        );
        assert!(
            lower.contains("not implemented") || lower.contains("until authenticated proxy"),
            "mail page must not claim JMAP proxy is live"
        );
        assert!(
            !lower.contains("open webmail") && !lower.contains("compose message"),
            "must not ship fake webmail CTAs before product exists"
        );
        assert_doge_palette_only(&html);
    }

    /// Named contract: accounts page never ships a create-mailbox mutation form.
    #[test]
    fn accounts_page_has_no_create_mailbox_form() {
        let html = render_accounts_page(&sample_data("/accounts"));
        let lower = html.to_ascii_lowercase();
        assert!(
            !lower.contains("<form"),
            "accounts must not pretend to create mailboxes via form"
        );
        assert!(
            html.contains("No create-mailbox") || html.contains("nothing invented"),
            "accounts should state no create form / honest empty"
        );
    }

    #[test]
    fn domains_page_has_hostname_role_source_columns() {
        let html = render_domains_page(&sample_data("/domains"));
        assert!(html.contains("Hostname"));
        assert!(html.contains("Role"));
        assert!(html.contains("Source note"));
        assert!(html.contains("example.test"));
        assert!(html.contains("config inventory"));
    }

    #[test]
    fn accounts_page_has_unavailable_badge_and_next_steps() {
        let html = render_accounts_page(&sample_data("/accounts"));
        assert!(html.contains("source: unavailable") || html.contains("unavailable"));
        assert!(
            html.contains("stalwart-cli") || html.contains("bootstrap"),
            "empty state should guide next operator steps"
        );
        assert!(!html.contains("admin@"));
    }

    #[test]
    fn no_production_onion_hardcoded_in_sample_renders() {
        // Guard: sample/fixture HTML must not embed a real Surmount production onion.
        let pages = [
            render_overview(&sample_data("/")),
            render_system_page(&sample_data("/system")),
            render_mail_page(
                &sample_data("/mail"),
                &StalwartStatus {
                    reachable: false,
                    status: None,
                    error: Some("connection refused".into()),
                    url: "http://127.0.0.1:8080".into(),
                },
            ),
        ];
        for html in pages {
            assert!(
                !html.contains("surmount.onion") && !html.contains("services.surmount"),
                "unexpected production-ish onion/services invent"
            );
            // services.example.test is fine; real .onion host must not appear unless set.
            assert!(!html.contains(".onion"));
        }
    }
}
