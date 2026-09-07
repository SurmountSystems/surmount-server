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
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use leptos::prelude::*;

use crate::AppState;
use crate::api::{
    AccountsInventory, DomainsInventory, StalwartStatus, domains_inventory, probe_stalwart,
};
use crate::config::{AppConfig, OnionSurface};
use crate::tls::ListenMode;
use surmount_management_ui::auth::{
    CspNonce, SESSION_COOKIE_NAME, cookie_value, csrf_set_cookie_header, decode_session_cookie,
    now_unix, session_bound_csrf_token,
};
use surmount_management_ui::ban::BanEnforcement;
use surmount_management_ui::console_accounts::{load_console_accounts, resolve_console_principal};

/// Who is looking at `/mail`. Administrator sees create + grant; User does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailConsoleView {
    Administrator,
    User { mailbox: Option<String> },
}

impl MailConsoleView {
    pub fn can_manage_people(&self) -> bool {
        matches!(self, Self::Administrator)
    }
}

/// Extra `/mail` data (roster + NWC connected flag). Never includes secrets.
#[derive(Debug, Clone, Default)]
pub struct MailPageExtras {
    pub roster: Vec<MailboxRosterRow>,
    pub nwc_connected: bool,
}

/// One person on the Administrator roster. Password is yes/no, never the secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxRosterRow {
    pub primary: String,
    pub aliases: Vec<String>,
    pub npub_attached: bool,
    pub password_set: bool,
    pub role: String,
}

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
    /// Structured onion surface (configured / hostname missing / not provisioned).
    pub onion_surface: OnionSurface,
    /// Operator-published onion (`http://….onion`) when configured; never invented.
    pub onion_url: Option<String>,
    /// Operator-published Vaultwarden URL (domain C); never invented; no admin token.
    pub vaultwarden_url: Option<String>,
    /// When true, Open vault is same-origin path proxy (not external / not reverse-proxied).
    pub vaultwarden_proxy_enable: bool,
    /// Same-origin href when proxy enabled (e.g. `/vault/`).
    pub vaultwarden_proxy_href: String,
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
        if s.reachable { Self::Ok } else { Self::Fail }
    }

    fn class(self) -> &'static str {
        match self {
            Self::Ok => "chip chip-ok",
            Self::Fail => "chip chip-fail",
            // Neutral (cyan): not probed / hermetic; yellow reserved for true warnings.
            Self::Unknown => "chip chip-neutral",
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
        onion_surface: config.onion_surface.clone(),
        onion_url: config.onion_url.clone(),
        vaultwarden_url: config.vaultwarden_url.clone(),
        vaultwarden_proxy_enable: config.vaultwarden_proxy.enable,
        vaultwarden_proxy_href: if config.vaultwarden_proxy.enable {
            config.vaultwarden_proxy.same_origin_href()
        } else {
            String::new()
        },
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

/// Technical auth detail for System / Login (env names allowed here).
fn auth_system_detail(mode: &str) -> String {
    match mode {
        "nostr" => "Auth mode=nostr: NIP-98 + session cookie (rust-nostr, not JS NDK). \
Empty allowlist is fail-closed. nsec never on server. Host enable: docs/OPS.md. \
Open residual (do not invent here): key-loss recovery, durable session store, \
first-operator bootstrap UX."
            .into(),
        _ => "Auth mode=off: open console for local/dev. Not public-safe alone. \
Console auth is off (local/dev only). Not safe on a public edge. \
To gate: SURMOUNT_AUTH_MODE=nostr, allowlist (env or file), and session secret. \
See docs/OPS.md. Open residual: key-loss, durable store, bootstrap UX."
            .into(),
    }
}

/// Technical directory detail for System / Accounts (env names allowed here).
fn directory_system_detail(accounts_source: &str) -> String {
    match accounts_source {
        "mock" => "Directory: hermetic mock fixture (not live Stalwart). \
Create mailbox form is on /mail (Administrator; optional npub; User or \
Administrator). Map path: /var/lib/surmount/console/accounts.json \
(or SURMOUNT_CONSOLE_ACCOUNTS). /accounts still has no create form. \
API POST/PATCH /api/v1/accounts when auth allows (nostr or lab escape) \
+ CSRF on cookie POSTs."
            .into(),
        "unavailable" => "Mail directory offline (source: unavailable). \
Accounts API stays empty until SURMOUNT_DIRECTORY=stalwart + host token. \
Domains are config inventory only. No invent live mailboxes. \
Create mailbox and attach npub (Grant console login) are on /mail \
(Administrator; session CSRF). Attach writes the console map so that \
npub can log into the portal even when listing is unavailable. \
Map path: /var/lib/surmount/console/accounts.json. \
/accounts still has no create form."
            .into(),
        "stalwart" => "Directory: live Stalwart management JMAP \
(SURMOUNT_DIRECTORY=stalwart). Empty list means no principals or list failed \
fail-closed (no invented rows). Create mailbox form is on /mail \
(Administrator; optional npub; User or Administrator). \
Map path: /var/lib/surmount/console/accounts.json. \
API mutations: POST/PATCH /api/v1/accounts behind auth + CSRF when cookie. \
/accounts still has no create form."
            .into(),
        other => format!(
            "Directory source: {other}. Create mailbox form is on /mail \
(optional npub; two roles). /accounts still has no create form. \
Map path: /var/lib/surmount/console/accounts.json. See docs/OPS.md."
        ),
    }
}

/// Whether Overview should surface attention (health strip + compact alert chips).
/// Includes setup residual and hard service fails (Mail/Stalwart probe Fail or Unknown).
/// Never claim "All services healthy" while any gated surface is not ok.
fn overview_needs_attention(data: &AdminPageData) -> bool {
    data.auth_mode != "nostr"
        || data.accounts.source == "unavailable"
        || data.accounts.source == "mock"
        || matches!(
            data.onion_surface,
            OnionSurface::NotProvisioned | OnionSurface::HostnameMissing { .. }
        )
        || (data.vaultwarden_url.is_none() && !data.vaultwarden_proxy_enable)
        || matches!(data.stalwart, StalwartChip::Fail | StalwartChip::Unknown)
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
        onion_surface: OnionSurface::NotProvisioned,
        onion_url: None,
        vaultwarden_url: None,
        vaultwarden_proxy_enable: false,
        vaultwarden_proxy_href: String::new(),
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
    // DSM-like package home: health strip + large tiles + compact alerts only.
    let domain = data.primary_domain.clone();
    let services_host = data.services_hostname.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let domain_count = data.domains.domains.len();
    let domains_source = data.domains.source.to_string();
    let domain_count_str = domain_count.to_string();
    let account_count = data.accounts.accounts.len();
    let account_count_str = account_count.to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let auth_is_nostr = data.auth_mode == "nostr";
    let show_attention = overview_needs_attention(data);
    let show_auth_attn = data.auth_mode != "nostr";
    let show_dir_attn = data.accounts.source == "unavailable" || data.accounts.source == "mock";
    let show_onion_attn = matches!(
        data.onion_surface,
        OnionSurface::NotProvisioned | OnionSurface::HostnameMissing { .. }
    );
    let show_vault_attn = data.vaultwarden_url.is_none() && !data.vaultwarden_proxy_enable;
    // Operational mail fail is attention, not setup residual only.
    let show_mail_attn = matches!(data.stalwart, StalwartChip::Fail | StalwartChip::Unknown);
    let attention_n = u32::from(show_auth_attn)
        + u32::from(show_dir_attn)
        + u32::from(show_onion_attn)
        + u32::from(show_vault_attn)
        + u32::from(show_mail_attn);
    // Health strip owns the single attention count; alert bar is chips only.
    let health_line = if show_attention {
        format!("Needs attention ({attention_n})")
    } else {
        "All services healthy".to_string()
    };
    let health_class = if show_attention {
        "health-line health-warn".to_string()
    } else {
        "health-line health-ok".to_string()
    };
    let onion_chip_class = match &data.onion_surface {
        OnionSurface::HostnameMissing { .. } => "chip chip-warn".to_string(),
        _ => "chip chip-neutral".to_string(),
    };
    let mail_chip_class = match data.stalwart {
        StalwartChip::Fail => "chip chip-fail".to_string(),
        _ => "chip chip-neutral".to_string(),
    };

    // Mail tile: one-word status from live probe (no essay).
    let (mail_state, mail_status, mail_detail) = match data.stalwart {
        StalwartChip::Ok => ("state-ok", "OK", ""),
        StalwartChip::Fail => ("state-fail", "Down", "Unreachable"),
        StalwartChip::Unknown => ("state-neutral", "Unknown", ""),
    };
    let mail_tile_class = format!("pkg-tile {mail_state}");

    // Onion tile: Off / Ready / Missing from structured surface only.
    let (onion_state, onion_status) = match &data.onion_surface {
        OnionSurface::Configured { .. } => ("state-ok", "Ready"),
        OnionSurface::HostnameMissing { .. } => ("state-warn", "Missing"),
        OnionSurface::NotProvisioned => ("state-neutral", "Off"),
    };
    let onion_tile_class = format!("pkg-tile {onion_state}");

    // Vault tile: Off / Linked. Proxy-on uses same-origin path (not external blank).
    let (vault_state, vault_status, vault_href, vault_external) = if data.vaultwarden_proxy_enable {
        (
            "state-ok",
            "Linked",
            data.vaultwarden_proxy_href.clone(),
            false,
        )
    } else {
        match &data.vaultwarden_url {
            Some(url) => ("state-ok", "Linked", url.clone(), true),
            None => ("state-neutral", "Off", "/system".to_string(), false),
        }
    };
    let vault_tile_class = format!("pkg-tile {vault_state}");

    // Accounts tile: big count; offline when directory unavailable (honest 0).
    let accounts_detail = if data.accounts.source == "unavailable" {
        "offline"
    } else {
        data.accounts.source
    };
    let accounts_state = if data.accounts.source == "unavailable" || data.accounts.source == "mock"
    {
        "state-neutral"
    } else {
        "state-ok"
    };
    let accounts_tile_class = format!("pkg-tile {accounts_state}");
    let domains_tile_class = "pkg-tile state-ok".to_string();
    let console_tile_class = "pkg-tile state-ok".to_string();

    view! {
        <AdminDocument
            title="Overview · Surmount Services".to_string()
            domain=domain.clone()
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
            vaultwarden_url=data.vaultwarden_url.clone().unwrap_or_default()
            vaultwarden_proxy_enable=data.vaultwarden_proxy_enable
            vaultwarden_proxy_href=data.vaultwarden_proxy_href.clone()
        >
            <p class="page-kicker">"Overview"</p>
            <h2 class="page-title">"Dashboard"</h2>
            <div class="health-strip">
                <span class=health_class>{health_line}</span>
                <code class="host-chip">{services_host}</code>
                {if auth_is_nostr {
                    view! {
                        <a href="/login" class="health-login">"Login"</a>
                    }
                    .into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>
            <div class="pkg-grid">
                <a href="/" class=console_tile_class>
                    <span class="pkg-title">"Console"</span>
                    <span class="pkg-status">"OK"</span>
                    <span class="pkg-detail">"Management console"</span>
                </a>
                <a href="/mail" class=mail_tile_class>
                    <span class="pkg-title">"Mail"</span>
                    <span class="pkg-status">{mail_status}</span>
                    {if mail_detail.is_empty() {
                        view! { <span class="pkg-detail"></span> }.into_any()
                    } else {
                        view! { <span class="pkg-detail">{mail_detail}</span> }.into_any()
                    }}
                </a>
                <a href="/domains" class=domains_tile_class>
                    <span class="pkg-title">"Domains"</span>
                    <span class="pkg-status pkg-status-num">{domain_count_str}</span>
                    <span class="pkg-detail">{domains_source}</span>
                </a>
                <a href="/accounts" class=accounts_tile_class>
                    <span class="pkg-title">"Accounts"</span>
                    <span class="pkg-status pkg-status-num">{account_count_str}</span>
                    <span class="pkg-detail">{accounts_detail}</span>
                </a>
                <a href="/system" class=onion_tile_class>
                    <span class="pkg-title">"Onion"</span>
                    <span class="pkg-status">{onion_status}</span>
                    <span class="pkg-detail"></span>
                </a>
                {if vault_external {
                    let vh = vault_href.clone();
                    let vc = vault_tile_class.clone();
                    view! {
                        <a
                            href=vh
                            class=vc
                            rel="noopener noreferrer"
                            target="_blank"
                        >
                            <span class="pkg-title">"Vault"</span>
                            <span class="pkg-status">{vault_status}</span>
                            <span class="pkg-detail"></span>
                        </a>
                    }
                    .into_any()
                } else {
                    view! {
                        <a href=vault_href class=vault_tile_class>
                            <span class="pkg-title">"Vault"</span>
                            <span class="pkg-status">{vault_status}</span>
                            <span class="pkg-detail"></span>
                        </a>
                    }
                    .into_any()
                }}
            </div>
            {if show_attention {
                // Chips only: attention count lives on the health strip (no dual N copy).
                view! {
                    <div class="alert-bar" role="status">
                        <div class="alert-chips">
                            {if show_mail_attn {
                                view! {
                                    <a href="/mail" class=mail_chip_class.clone()>"Mail"</a>
                                }
                                .into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                            {if show_auth_attn {
                                view! {
                                    <a href="/system" class="chip chip-warn">"Auth"</a>
                                }
                                .into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                            {if show_dir_attn {
                                view! {
                                    <a href="/accounts" class="chip chip-neutral">"Directory"</a>
                                }
                                .into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                            {if show_onion_attn {
                                view! {
                                    <a href="/system" class=onion_chip_class.clone()>"Onion"</a>
                                }
                                .into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                            {if show_vault_attn {
                                view! {
                                    <a href="/system" class="chip chip-neutral">"Vault"</a>
                                }
                                .into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                        </div>
                    </div>
                }
                .into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
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
        "Source: {source_badge}. Rows mirror deploy/config hostnames, not a live Stalwart directory query."
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
            vaultwarden_url=data.vaultwarden_url.clone().unwrap_or_default()
            vaultwarden_proxy_enable=data.vaultwarden_proxy_enable
            vaultwarden_proxy_href=data.vaultwarden_proxy_href.clone()
        >
            <p class="page-kicker">"Domains"</p>
            <h2 class="page-title">"Hostname inventory"</h2>
            <div class="card">
                <p>
                    <span class="badge">"source: config"</span>
                </p>
                <p class="lede">{source_line}</p>
                <p class="note">{note}</p>
                <p class="note">
                    "Config inventory only: this page does not list Stalwart principals or DNS zones from the engine. Use stalwart-cli or bootstrap admin for engine directory work."
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
            "Source: {source}. Live Stalwart management JMAP list (explicit host directory)."
        ),
        _ => format!("Source: {source}. Not a live Stalwart mailbox directory."),
    };
    // Production default is honest empty; mock/stalwart may list principals.
    let empty = data.accounts.accounts.is_empty();
    let list_body = if empty {
        if source == "unavailable" {
            "No accounts listed. Mail directory offline; nothing invented.".to_string()
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
        "mock" => "Mock directory active for tests. \
API create/update: POST /api/v1/accounts and PATCH /api/v1/accounts/{id} \
(auth gate + CSRF when session cookie). For live principals, connect Stalwart \
directory with a host token (see docs/research/stalwart-directory-api.md)."
            .to_string(),
        "stalwart" => "Live Stalwart directory client active. List via GET /api/v1/accounts; \
create/update via POST/PATCH /api/v1/accounts (nostr auth or lab escape; \
CSRF on cookie POSTs). Set a mailbox password on Mail."
            .to_string(),
        _ => "What to do next: connect the live mail directory (Stalwart + host API token), \
or create principals with stalwart-cli / bootstrap admin / authenticated API when \
directory is on (see docs/research/stalwart-directory-api.md). \
Set a mailbox password on Mail even when this list is offline."
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
            vaultwarden_url=data.vaultwarden_url.clone().unwrap_or_default()
            vaultwarden_proxy_enable=data.vaultwarden_proxy_enable
            vaultwarden_proxy_href=data.vaultwarden_proxy_href.clone()
        >
            <p class="page-kicker">"Accounts"</p>
            <h2 class="page-title">"Mail accounts"</h2>
            <div class="card">
                <p>
                    <span class="badge">{source_badge}</span>
                </p>
                <p class="lede">{source_line}</p>
                <p class="note">{note}</p>
                {if empty {
                    view! { <p class="empty">{list_body}</p> }.into_any()
                } else {
                    view! { <p class="list-body">{list_body}</p> }.into_any()
                }}
                <p class="note">{next_steps}</p>
                <p class="note">
                    "Set a mailbox password on "
                    <a href="/mail#mailbox-password">"Mail"</a>
                    ". No create-mailbox HTML form on this page."
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
    let (onion_label, onion_body) = match &data.onion_surface {
        OnionSurface::Configured { url } => (
            "Onion (operator-published)".to_string(),
            url.clone(),
        ),
        OnionSurface::HostnameMissing { path, hs_state_dir } => {
            let where_path = path
                .as_deref()
                .or(hs_state_dir.as_deref())
                .unwrap_or("onionServiceStateDir");
            (
                "Onion".to_string(),
                format!(
                    "Hostname material missing at {where_path}. Enable path: surmount.artiHiddenService (place HS identity under onionServiceStateDir; startDaemon when ready). Address is never invented."
                ),
            )
        }
        OnionSurface::NotProvisioned => (
            "Onion".to_string(),
            "Not provisioned on this host. Enable surmount.artiHiddenService (HS keys under onionServiceStateDir; startDaemon when ready). See docs/deploy-host-local.md (B3). Address is never invented.".to_string(),
        ),
    };
    let onion_is_link = data.onion_url.is_some();
    let onion_href = data.onion_url.clone().unwrap_or_default();
    let onion_href_attr = onion_href.clone();
    let onion_href_text = onion_href;
    let (vw_label, vw_body, vw_is_link, vw_href, vw_proxy_mode) = if data.vaultwarden_proxy_enable {
        (
            "Vaultwarden (same-origin path proxy)".to_string(),
            data.vaultwarden_proxy_href.clone(),
            true,
            data.vaultwarden_proxy_href.clone(),
            true,
        )
    } else {
        match &data.vaultwarden_url {
                Some(url) => (
                    "Vaultwarden (operator-published)".to_string(),
                    url.clone(),
                    true,
                    url.clone(),
                    false,
                ),
                None => (
                    "Vaultwarden".to_string(),
                    "Not configured. Host enable: surmount.vaultwarden + admin token EnvironmentFile under /run/surmount-secrets/vaultwarden/ (never in git). Console link via SURMOUNT_VAULTWARDEN_URL. Optional Axum path proxy: SURMOUNT_VAULTWARDEN_PROXY=1 under /vault/. Domain C human password manager only; not Bitwarden Secrets Manager; not deploy-secret activation.".to_string(),
                    false,
                    String::new(),
                    false,
                ),
            }
    };
    let vw_href_attr = vw_href.clone();
    let vw_open_href = vw_href.clone();
    let vw_href_text = vw_href;
    let vw_open_note = if vw_proxy_mode {
        " (same-origin path proxy; Vaultwarden login is SoT on this path)"
    } else {
        " (external; honest link, not reverse-proxied here)"
    };
    let auth_mode = data.auth_mode.clone();
    let auth_detail = auth_system_detail(&data.auth_mode);
    let auth_is_nostr = data.auth_mode == "nostr";
    let auth_is_open = data.auth_mode != "nostr";
    let directory_detail = directory_system_detail(data.accounts.source);
    let primary_host = data.primary_domain.clone();
    let mail_host = data.mail_hostname.clone();
    let services_host = data.services_hostname.clone();

    view! {
        <AdminDocument
            title="System · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            nav=nav
            vaultwarden_url=data.vaultwarden_url.clone().unwrap_or_default()
            vaultwarden_proxy_enable=data.vaultwarden_proxy_enable
            vaultwarden_proxy_href=data.vaultwarden_proxy_href.clone()
        >
            <p class="page-kicker">"System"</p>
            <h2 class="page-title">"Host posture"</h2>
            <p class="lede">
                "Technical detail for this management edge. Overview is the package home; this page holds setup residual, hostnames, and env wiring."
            </p>
            <div class="card">
                <h3 class="card-title">"Hostnames"</h3>
                <dl class="kv">
                    <div class="kv-row">
                        <dt>"Primary"</dt>
                        <dd>
                            <code>{primary_host}</code>
                        </dd>
                    </div>
                    <div class="kv-row">
                        <dt>"Mail"</dt>
                        <dd>
                            <code>{mail_host}</code>
                        </dd>
                    </div>
                    <div class="kv-row">
                        <dt>"Services"</dt>
                        <dd>
                            <code>{services_host}</code>
                        </dd>
                    </div>
                </dl>
            </div>
            {if auth_is_open {
                view! {
                    <div class="card card-attention">
                        <h3 class="card-title">"Auth"</h3>
                        <p class="banner banner-warn">{auth_detail}</p>
                    </div>
                }
                .into_any()
            } else {
                view! {
                    <div class="card">
                        <h3 class="card-title">"Auth"</h3>
                        <p class="note">{auth_detail}</p>
                        {if auth_is_nostr {
                            view! {
                                <p class="note">
                                    "Sign in: "
                                    <a href="/login">"/login"</a>
                                    " (NIP-07 or curl NIP-98)."
                                </p>
                            }
                            .into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                    </div>
                }
                .into_any()
            }}
            <div class="card">
                <h3 class="card-title">"Directory"</h3>
                <p class="note">{directory_detail}</p>
            </div>
            <div class="card">
                <h3 class="card-title">"Configuration"</h3>
                <p class="note">
                    "Runbooks: "
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
                            <th>{vw_label}</th>
                            <td>
                                {if vw_is_link {
                                    view! {
                                        <span>
                                            {if vw_proxy_mode {
                                                view! {
                                                    <a href=vw_href_attr>
                                                        <code>{vw_href_text}</code>
                                                    </a>
                                                    " · "
                                                    <a href=vw_open_href>
                                                        "Open vault"
                                                    </a>
                                                    {vw_open_note}
                                                }
                                                .into_any()
                                            } else {
                                                view! {
                                                    <a href=vw_href_attr rel="noopener noreferrer" target="_blank">
                                                        <code>{vw_href_text}</code>
                                                    </a>
                                                    " · "
                                                    <a href=vw_open_href rel="noopener noreferrer" target="_blank">
                                                        "Open vault"
                                                    </a>
                                                    {vw_open_note}
                                                }
                                                .into_any()
                                            }}
                                        </span>
                                    }
                                    .into_any()
                                } else {
                                    view! { <span class="note">{vw_body}</span> }.into_any()
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
                <h3 class="card-title">"Health and JSON"</h3>
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
                        " system status (onion + vaultwarden when set; no secrets)"
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

/// Mail page including Create mailbox (Administrator) and Set mailbox password.
///
/// `csrf_token` is embedded in a hidden field so the form script does not depend
/// on `document.cookie`. The matching `surmount_csrf` cookie is HttpOnly.
/// Empty when the request has no valid session.
#[cfg(test)]
pub fn render_mail_page(
    data: &AdminPageData,
    status: &StalwartStatus,
    csp_nonce: &str,
    csrf_token: &str,
    view: &MailConsoleView,
) -> String {
    render_mail_page_with(
        data,
        status,
        csp_nonce,
        csrf_token,
        view,
        &MailPageExtras::default(),
    )
}

pub fn render_mail_page_with(
    data: &AdminPageData,
    status: &StalwartStatus,
    csp_nonce: &str,
    csrf_token: &str,
    view: &MailConsoleView,
    extras: &MailPageExtras,
) -> String {
    let domain = data.primary_domain.clone();
    let footer = footer_line(&data.version);
    let chip_class = data.stalwart.class().to_string();
    let chip_label = data.stalwart.label().to_string();
    let path = data.path.clone();
    let nav = nav_classes(&path);
    let host_line = data.mail_hostname.clone();
    let url_line = status.url.clone();
    let nonce = csp_nonce.to_string();
    let csrf_token = csrf_token.to_string();
    let csrf_create = csrf_token.clone();
    let csrf_grant = csrf_token.clone();
    let csrf_nwc = csrf_token.clone();
    let show_people = view.can_manage_people();
    let is_user = matches!(view, MailConsoleView::User { .. });
    let page_title = if is_user {
        "Your mailbox".to_string()
    } else {
        "Mail".to_string()
    };
    let roster = extras.roster.clone();
    let nwc_connected = extras.nwc_connected;
    let nwc_status = if nwc_connected {
        "Wallet connected".to_string()
    } else {
        "Not connected".to_string()
    };
    let default_mailbox = match view {
        MailConsoleView::User {
            mailbox: Some(addr),
        } if !addr.trim().is_empty() => addr.clone(),
        MailConsoleView::User { .. } => String::new(),
        MailConsoleView::Administrator => String::new(),
    };
    let help_mailbox = if default_mailbox.is_empty() {
        "your mailbox address".to_string()
    } else {
        default_mailbox.clone()
    };
    let password_lede =
        "This page sets the password Evolution and iPhone Mail use. It is not Nostr.".to_string();
    let password_card_title = "Password for Evolution and iPhone Mail".to_string();
    let help_local = help_mailbox
        .split('@')
        .next()
        .filter(|s| !s.is_empty() && *s != "your mailbox address")
        .unwrap_or("the local-part")
        .to_string();
    let mailbox_readonly = matches!(view, MailConsoleView::User { .. });
    let primary_preview = format!("local-part@{}", data.primary_domain);
    let preview_domain = data.primary_domain.clone();
    let imap_line = format!("{}:993", data.mail_hostname);
    let smtp_line = format!("{}:465", data.mail_hostname);
    let create_script = r#"
(function(){
  var form = document.getElementById('mailbox-create-form');
  if (!form) return;
  var localEl = document.getElementById('mailbox-create-local');
  var preview = document.getElementById('mailbox-create-preview');
  var domain = (preview && preview.getAttribute('data-domain')) || '';
  function updatePreview() {
    var local = localEl && typeof localEl.value === 'string' ? localEl.value.trim().toLowerCase() : '';
    if (preview) preview.textContent = (local || 'local-part') + '@' + domain;
  }
  if (localEl) localEl.addEventListener('input', updatePreview);
  updatePreview();
  form.addEventListener('submit', function(ev) {
    ev.preventDefault();
    var err = document.getElementById('mailbox-create-error');
    var ok = document.getElementById('mailbox-create-ok');
    function showErr(m) {
      if (err) { err.textContent = m; err.hidden = false; }
      if (ok) ok.hidden = true;
    }
    function showOk(m) {
      if (ok) { ok.textContent = m; ok.hidden = false; }
      if (err) err.hidden = true;
    }
    var local = localEl && typeof localEl.value === 'string' ? localEl.value : '';
    var displayEl = document.getElementById('mailbox-create-display');
    var pwEl = document.getElementById('mailbox-create-password');
    var cfEl = document.getElementById('mailbox-create-confirm');
    var npubEl = document.getElementById('mailbox-create-npub');
    var roleEl = document.getElementById('mailbox-create-role');
    var display = displayEl && typeof displayEl.value === 'string' ? displayEl.value : '';
    var pw = pwEl && typeof pwEl.value === 'string' ? pwEl.value : '';
    var cf = cfEl && typeof cfEl.value === 'string' ? cfEl.value : '';
    var npub = npubEl && typeof npubEl.value === 'string' ? npubEl.value.trim() : '';
    var role = roleEl && typeof roleEl.value === 'string' ? roleEl.value : 'user';
    if (pw !== cf) { showErr('Passwords do not match.'); return; }
    var csrf = '';
    var csrfEl = document.getElementById('mailbox-create-csrf');
    if (csrfEl && typeof csrfEl.value === 'string') {
      csrf = csrfEl.value;
    }
    var payload = { name: local, role: role, csrf: csrf };
    if (pw) { payload.password = pw; payload.confirm = cf; }
    if (display) payload.description = display;
    if (npub) payload.npub = npub;
    fetch('/api/v1/accounts', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
      body: JSON.stringify(payload)
    }).then(function(res) {
      return res.json().then(function(body) { return { res: res, body: body }; }).catch(function() {
        return { res: res, body: {} };
      });
    }).then(function(x) {
      if (x.res.ok && x.body.ok) {
        showOk(x.body.note || ('Created ' + (x.body.address || '') + '.'));
      } else {
        showErr(x.body.error || x.body.note || ('Request failed (' + x.res.status + ')'));
      }
    }).catch(function(e) {
      showErr(e && e.message ? e.message : 'Request failed.');
    });
  });
})();
"#
    .to_string();
    let grant_script = r#"
(function(){
  var form = document.getElementById('mailbox-grant-form');
  if (!form) return;
  form.addEventListener('submit', function(ev) {
    ev.preventDefault();
    var err = document.getElementById('mailbox-grant-error');
    var ok = document.getElementById('mailbox-grant-ok');
    function showErr(m) {
      if (err) { err.textContent = m; err.hidden = false; }
      if (ok) ok.hidden = true;
    }
    function showOk(m) {
      if (ok) { ok.textContent = m; ok.hidden = false; }
      if (err) err.hidden = true;
    }
    var mailboxEl = document.getElementById('mailbox-grant-mailbox');
    var npubEl = document.getElementById('mailbox-grant-npub');
    var roleEl = document.getElementById('mailbox-grant-role');
    var mailbox = mailboxEl && typeof mailboxEl.value === 'string' ? mailboxEl.value : '';
    var npub = npubEl && typeof npubEl.value === 'string' ? npubEl.value.trim() : '';
    var role = roleEl && typeof roleEl.value === 'string' ? roleEl.value : 'user';
    var csrf = '';
    var csrfEl = document.getElementById('mailbox-grant-csrf');
    if (csrfEl && typeof csrfEl.value === 'string') {
      csrf = csrfEl.value;
    }
    fetch('/api/v1/accounts/console', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
      body: JSON.stringify({ mailbox: mailbox, npub: npub, role: role, csrf: csrf })
    }).then(function(res) {
      return res.json().then(function(body) { return { res: res, body: body }; }).catch(function() {
        return { res: res, body: {} };
      });
    }).then(function(x) {
      if (x.res.ok && x.body.ok) {
        showOk(x.body.note || 'Console login saved.');
      } else {
        showErr(x.body.error || x.body.note || ('Request failed (' + x.res.status + ')'));
      }
    }).catch(function(e) {
      showErr(e && e.message ? e.message : 'Request failed.');
    });
  });
})();
"#
    .to_string();
    let password_script = r#"
(function(){
  var form = document.getElementById('mailbox-password-form');
  if (!form) return;
  form.addEventListener('submit', function(ev) {
    ev.preventDefault();
    var err = document.getElementById('mailbox-password-error');
    var ok = document.getElementById('mailbox-password-ok');
    function showErr(m) {
      if (err) { err.textContent = m; err.hidden = false; }
      if (ok) ok.hidden = true;
    }
    function showOk(m) {
      if (ok) { ok.textContent = m; ok.hidden = false; }
      if (err) err.hidden = true;
    }
    var mailboxEl = document.getElementById('mailbox-address');
    var pwEl = document.getElementById('mailbox-password-input');
    var cfEl = document.getElementById('mailbox-password-confirm');
    var mailbox = mailboxEl && typeof mailboxEl.value === 'string' ? mailboxEl.value : '';
    var pw = pwEl && typeof pwEl.value === 'string' ? pwEl.value : '';
    var cf = cfEl && typeof cfEl.value === 'string' ? cfEl.value : '';
    if (pw !== cf) { showErr('Passwords do not match.'); return; }
    if (!pw) { showErr('Password required.'); return; }
    var csrf = '';
    var csrfEl = document.getElementById('mailbox-csrf');
    if (csrfEl && typeof csrfEl.value === 'string') {
      csrf = csrfEl.value;
    }
    if (!csrf) {
      var parts = (document.cookie || '').split(';');
      for (var i = 0; i < parts.length; i++) {
        var p = parts[i].trim();
        if (p.indexOf('surmount_csrf=') === 0) {
          csrf = decodeURIComponent(p.slice('surmount_csrf='.length));
        }
      }
    }
    fetch('/api/v1/accounts/password', {
      method: 'PATCH',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': csrf },
      body: JSON.stringify({ mailbox: mailbox, password: pw, confirm: cf, csrf: csrf })
    }).then(function(res) {
      return res.json().then(function(body) { return { res: res, body: body }; }).catch(function() {
        return { res: res, body: {} };
      });
    }).then(function(x) {
      if (x.res.ok && x.body.ok) {
        showOk('Password set. Use this password in your mail app.');
      } else {
        showErr(x.body.error || x.body.note || ('Request failed (' + x.res.status + ')'));
      }
    }).catch(function(e) {
      showErr(e && e.message ? e.message : 'Request failed.');
    });
  });
})();
"#
    .to_string();
    let nwc_script = r#"
(function(){
  var form = document.getElementById('mailbox-nwc-form');
  if (!form) return;
  function csrf() {
    var el = document.getElementById('mailbox-nwc-csrf');
    return el && typeof el.value === 'string' ? el.value : '';
  }
  function mailbox() {
    var el = document.getElementById('mailbox-address');
    return el && typeof el.value === 'string' ? el.value : '';
  }
  function showErr(m) {
    var err = document.getElementById('mailbox-nwc-error');
    var ok = document.getElementById('mailbox-nwc-ok');
    if (err) { err.textContent = m; err.hidden = false; }
    if (ok) ok.hidden = true;
  }
  function showOk(m) {
    var ok = document.getElementById('mailbox-nwc-ok');
    var err = document.getElementById('mailbox-nwc-error');
    if (ok) { ok.textContent = m; ok.hidden = false; }
    if (err) err.hidden = true;
  }
  form.addEventListener('submit', function(ev) {
    ev.preventDefault();
    var uriEl = document.getElementById('mailbox-nwc-uri');
    var uri = uriEl && typeof uriEl.value === 'string' ? uriEl.value.trim() : '';
    var tok = csrf();
    fetch('/api/v1/accounts/nwc', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': tok },
      body: JSON.stringify({ mailbox: mailbox(), uri: uri, csrf: tok })
    }).then(function(res) {
      return res.json().then(function(body) { return { res: res, body: body }; }).catch(function() {
        return { res: res, body: {} };
      });
    }).then(function(x) {
      if (x.res.ok && x.body.ok) {
        showOk(x.body.note || 'Wallet connected.');
        if (uriEl) uriEl.value = '';
        var st = document.getElementById('mailbox-nwc-status');
        if (st) st.textContent = x.body.connected ? 'Wallet connected' : 'Not connected';
      } else {
        showErr(x.body.error || x.body.note || ('Request failed (' + x.res.status + ')'));
      }
    }).catch(function(e) {
      showErr(e && e.message ? e.message : 'Request failed.');
    });
  });
  var clearBtn = document.getElementById('mailbox-nwc-clear');
  if (clearBtn) {
    clearBtn.addEventListener('click', function() {
      var tok = csrf();
      fetch('/api/v1/accounts/nwc', {
        method: 'POST',
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/json', 'X-CSRF-Token': tok },
        body: JSON.stringify({ mailbox: mailbox(), uri: '', csrf: tok })
      }).then(function(res) {
        return res.json().then(function(body) { return { res: res, body: body }; }).catch(function() {
          return { res: res, body: {} };
        });
      }).then(function(x) {
        if (x.res.ok && x.body.ok) {
          showOk(x.body.note || 'Wallet connection cleared.');
          var st = document.getElementById('mailbox-nwc-status');
          if (st) st.textContent = 'Not connected';
          var uriEl = document.getElementById('mailbox-nwc-uri');
          if (uriEl) uriEl.value = '';
        } else {
          showErr(x.body.error || x.body.note || ('Request failed (' + x.res.status + ')'));
        }
      }).catch(function(e) {
        showErr(e && e.message ? e.message : 'Request failed.');
      });
    });
  }
})();
"#
    .to_string();
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
        "Stalwart HTTP answered. Mail protocols still depend on host DNS, firewall, and account setup. Browser webmail is not shipped yet. Use IMAP and SMTP in Evolution or iPhone Mail, or use JMAP when the authenticated proxy lands."
            .to_string()
    } else {
        "When down: start Stalwart on the host, confirm the configured probe URL, or use SSH tunnel / bootstrap /stalwart-admin/ while bringing the host up. Probe URL env is documented on System."
            .to_string()
    };

    view! {
        <AdminDocument
            title="Mail · Surmount Services".to_string()
            domain=domain
            footer=footer
            chip_class=chip_class
            chip_label=chip_label
            vaultwarden_url=data.vaultwarden_url.clone().unwrap_or_default()
            vaultwarden_proxy_enable=data.vaultwarden_proxy_enable
            vaultwarden_proxy_href=data.vaultwarden_proxy_href.clone()
            nav=nav
        >
            <p class="page-kicker">"Mail"</p>
            <h2 class="page-title">{page_title}</h2>
            <p class="note">{password_lede.clone()}</p>
            <div class="card card-primary" id="mailbox-password">
                <h3 class="card-title">{password_card_title}</h3>
                <form id="mailbox-password-form" class="form-stack" action="#" method="post">
                    <input
                        type="hidden"
                        name="csrf"
                        id="mailbox-csrf"
                        value=csrf_token
                        autocomplete="off"
                    />
                    <label for="mailbox-address">"Mailbox address"</label>
                    <input
                        id="mailbox-address"
                        name="mailbox"
                        type="email"
                        value=default_mailbox
                        placeholder="full email"
                        autocomplete="username"
                        readonly=mailbox_readonly
                    />
                    <label for="mailbox-password-input">"New password"</label>
                    <input
                        id="mailbox-password-input"
                        name="password"
                        type="password"
                        autocomplete="new-password"
                    />
                    <label for="mailbox-password-confirm">"Confirm password"</label>
                    <input
                        id="mailbox-password-confirm"
                        name="confirm"
                        type="password"
                        autocomplete="new-password"
                    />
                    <button type="submit">"Set mailbox password"</button>
                </form>
                <p id="mailbox-password-error" class="form-error" hidden></p>
                <p id="mailbox-password-ok" class="form-ok" hidden></p>
                <h3 class="card-title">"Mail app settings"</h3>
                <dl class="kv">
                    <div class="kv-row">
                        <dt>"Username"</dt>
                        <dd>
                            "that full address "
                            <code>{help_mailbox.clone()}</code>
                        </dd>
                    </div>
                    <div class="kv-row">
                        <dt>"IMAP"</dt>
                        <dd>
                            <code>{host_line.clone()}</code>
                            " port 993 SSL"
                        </dd>
                    </div>
                    <div class="kv-row">
                        <dt>"SMTP"</dt>
                        <dd>
                            <code>{host_line.clone()}</code>
                            " port 465 SSL"
                        </dd>
                    </div>
                </dl>
                <p class="note">
                    "Not STARTTLS. Not OAuth. Normal password (not OAuth2)."
                </p>
                <p class="note">
                    "Evolution User Name must be the full address "
                    <code>{help_mailbox.clone()}</code>
                    {format!(". Do not use {help_local} alone.")}
                </p>
                <p class="note">
                    "TLS is Let's Encrypt. IMAP "
                    <code>{imap_line}</code>
                    " SSL. SMTP "
                    <code>{smtp_line}</code>
                    " SSL. Do not use a local-part username."
                </p>
                <p class="note">
                    "After a good login, the first folder scan of this imported mailbox can take a long time. Scanning folders is not by itself a failed login."
                </p>
                <p class="note">
                    "Evolution may show Failed to authenticate with IMAP server said BYE: Connection timed out when the first open waits too long. That is a wait, not by itself a bad password. Keep User Name as "
                    <code>{help_mailbox}</code>
                    ", and retry the folder."
                </p>
                <script nonce=nonce.clone()>{password_script}</script>
            </div>
            {if show_people {
                view! {
                    <div class="card" id="mailbox-create">
                        <h3 class="card-title">"Make a new mailbox"</h3>
                        <p class="note">
                            "Make a new mailbox. They can set the password after they sign in."
                        </p>
                        <form id="mailbox-create-form" class="form-stack" action="#" method="post">
                            <input
                                type="hidden"
                                name="csrf"
                                id="mailbox-create-csrf"
                                value=csrf_create
                                autocomplete="off"
                            />
                            <label for="mailbox-create-local">"Local-part"</label>
                            <input
                                id="mailbox-create-local"
                                name="local"
                                type="text"
                                autocomplete="off"
                                spellcheck="false"
                                autocapitalize="none"
                            />
                            <p class="note">
                                "Address preview: "
                                <code id="mailbox-create-preview" data-domain=preview_domain>
                                    {primary_preview}
                                </code>
                            </p>
                            <label for="mailbox-create-display">"Display name (optional)"</label>
                            <input
                                id="mailbox-create-display"
                                name="description"
                                type="text"
                                autocomplete="off"
                            />
                            <label for="mailbox-create-password">"Password (optional)"</label>
                            <input
                                id="mailbox-create-password"
                                name="password"
                                type="password"
                                autocomplete="new-password"
                            />
                            <label for="mailbox-create-confirm">"Confirm password"</label>
                            <input
                                id="mailbox-create-confirm"
                                name="confirm"
                                type="password"
                                autocomplete="new-password"
                            />
                            <label for="mailbox-create-npub">"Optional npub"</label>
                            <input
                                id="mailbox-create-npub"
                                name="npub"
                                type="text"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="bech32 npub or hex (empty = IMAP/SMTP only)"
                            />
                            <label for="mailbox-create-role">"Console role"</label>
                            <select id="mailbox-create-role" name="role">
                                <option value="user" selected>
                                    "User"
                                </option>
                                <option value="administrator">
                                    "Administrator"
                                </option>
                            </select>
                            <button type="submit" id="mailbox-create-submit">
                                "Create mailbox"
                            </button>
                        </form>
                        <p id="mailbox-create-error" class="form-error" hidden></p>
                        <p id="mailbox-create-ok" class="form-ok" hidden></p>
                        <script nonce=nonce.clone()>{create_script}</script>
                    </div>
                    <div class="card" id="mailbox-grant">
                        <h3 class="card-title">"Grant console login"</h3>
                        <p class="note">
                            "You attach npub so they can sign in to this website. They set their own mailbox password after they sign in. Paste their npub1... and bind it to their primary mailbox. Empty npub clears website login (IMAP only)."
                        </p>
                        <form id="mailbox-grant-form" class="form-stack" action="#" method="post">
                            <input
                                type="hidden"
                                name="csrf"
                                id="mailbox-grant-csrf"
                                value=csrf_grant
                                autocomplete="off"
                            />
                            <label for="mailbox-grant-mailbox">"Mailbox address"</label>
                            <input
                                id="mailbox-grant-mailbox"
                                name="mailbox"
                                type="email"
                                value=String::new()
                                placeholder="full email"
                                autocomplete="username"
                            />
                            <label for="mailbox-grant-npub">"npub (bech32)"</label>
                            <input
                                id="mailbox-grant-npub"
                                name="npub"
                                type="text"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="npub1... (empty clears portal login)"
                            />
                            <label for="mailbox-grant-role">"Console role"</label>
                            <select id="mailbox-grant-role" name="role">
                                <option value="user" selected>
                                    "User"
                                </option>
                                <option value="administrator">
                                    "Administrator"
                                </option>
                            </select>
                            <button type="submit" id="mailbox-grant-submit">
                                "Save console login"
                            </button>
                        </form>
                        <p id="mailbox-grant-error" class="form-error" hidden></p>
                        <p id="mailbox-grant-ok" class="form-ok" hidden></p>
                        <script nonce=nonce.clone()>{grant_script}</script>
                    </div>
                    <div class="card" id="mailbox-roster">
                        <h3 class="card-title">"Mailbox roster"</h3>
                        <p class="note">
                            "People who can sign in to this website (Nostr) will show here. IMAP passwords are the card above. Primary address, aliases of that person, website login, IMAP password yes/no, User vs Administrator. Empty is OK. Aliases of one person are not extra people."
                        </p>
                        <table>
                            <thead>
                                <tr>
                                    <th>"Primary"</th>
                                    <th>"Aliases"</th>
                                    <th>"Portal login"</th>
                                    <th>"IMAP password"</th>
                                    <th>"Role"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {if roster.is_empty() {
                                    view! {
                                        <tr>
                                            <td colspan="5" class="empty">
                                                "People who can sign in to this website (Nostr) will show here. IMAP passwords are the card above."
                                            </td>
                                        </tr>
                                    }
                                    .into_any()
                                } else {
                                    view! {
                                        {roster
                                            .iter()
                                            .map(|row| {
                                                let aliases = if row.aliases.is_empty() {
                                                    "none".to_string()
                                                } else {
                                                    row.aliases.join(", ")
                                                };
                                                let npub = if row.npub_attached { "yes" } else { "no" };
                                                let pw = if row.password_set { "yes" } else { "no" };
                                                let role = if row.role.eq_ignore_ascii_case("administrator") {
                                                    "Administrator".to_string()
                                                } else {
                                                    "User".to_string()
                                                };
                                                let primary = row.primary.clone();
                                                view! {
                                                    <tr>
                                                        <td>
                                                            <code>{primary}</code>
                                                        </td>
                                                        <td>{aliases}</td>
                                                        <td>{npub.to_string()}</td>
                                                        <td>{pw.to_string()}</td>
                                                        <td>{role}</td>
                                                    </tr>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    }
                                    .into_any()
                                }}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any()
            } else {
                view! { <span hidden></span> }.into_any()
            }}
            {if is_user {
                view! {
                    <div class="card" id="mailbox-nwc">
                        <h3 class="card-title">"Lightning wallet (NWC)"</h3>
                        <p class="note">
                            "Optional Nostr Wallet Connect. Payments use your wallet via NWC. This is not login and not your mailbox password. Alby and other NWC extensions are wallets, not the IMAP password form."
                        </p>
                        <p class="note">
                            "Status: "
                            <span id="mailbox-nwc-status">{nwc_status}</span>
                        </p>
                        <form id="mailbox-nwc-form" class="form-stack" action="#" method="post">
                            <input
                                type="hidden"
                                name="csrf"
                                id="mailbox-nwc-csrf"
                                value=csrf_nwc
                                autocomplete="off"
                            />
                            <label for="mailbox-nwc-uri">"NWC connection string"</label>
                            <input
                                id="mailbox-nwc-uri"
                                name="uri"
                                type="password"
                                autocomplete="off"
                                spellcheck="false"
                                placeholder="nostr+walletconnect://..."
                            />
                            <button type="submit" id="mailbox-nwc-save">
                                "Save wallet"
                            </button>
                            <button type="button" id="mailbox-nwc-clear">
                                "Clear wallet"
                            </button>
                        </form>
                        <p id="mailbox-nwc-error" class="form-error" hidden></p>
                        <p id="mailbox-nwc-ok" class="form-ok" hidden></p>
                        <script nonce=nonce.clone()>{nwc_script}</script>
                    </div>
                }
                .into_any()
            } else {
                view! { <span hidden></span> }.into_any()
            }}
            <details class="card">
                <summary class="card-title">"Technical"</summary>
                <p class="note">"For later. Probe, JMAP, and status JSON."</p>
                <h3 class="card-title">"Reachability"</h3>
                <p>
                    "Live probe: "
                    <span class=result_class>{reachable}</span>
                    {probe_after}
                </p>
                <p class="note">{guidance}</p>
                <h3 class="card-title">"Endpoints"</h3>
                <dl class="kv">
                    <div class="kv-row">
                        <dt>"Mail hostname"</dt>
                        <dd>
                            <code>{host_line}</code>
                        </dd>
                    </div>
                    <div class="kv-row">
                        <dt>"Probe URL"</dt>
                        <dd>
                            <code>{url_line}</code>
                        </dd>
                    </div>
                </dl>
                <p class="note">
                    "JMAP proxy: POST /api/v1/jmap returns 501 until authenticated proxy lands (not implemented)."
                </p>
                <p>
                    <a href="/api/v1/stalwart/status">
                        <code>"GET /api/v1/stalwart/status"</code>
                    </a>
                    " JSON"
                </p>
            </details>
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
/// `vaultwarden_url`: non-empty = vault nav link; empty = residual "Vault" -> /system.
/// When `vaultwarden_proxy_enable`, nav uses same-origin path (no target blank).
#[component]
fn AdminDocument(
    title: String,
    domain: String,
    footer: String,
    chip_class: String,
    chip_label: String,
    nav: NavClasses,
    /// Operator-published Vaultwarden URL; empty string = residual not configured.
    #[prop(optional, default = String::new())]
    vaultwarden_url: String,
    /// When true, vault nav is same-origin path proxy (not external).
    #[prop(optional, default = false)]
    vaultwarden_proxy_enable: bool,
    /// Same-origin href when proxy enabled.
    #[prop(optional, default = String::new())]
    vaultwarden_proxy_href: String,
    children: Children,
) -> impl IntoView {
    let vw_configured = vaultwarden_proxy_enable || !vaultwarden_url.trim().is_empty();
    let vw_same_origin = vaultwarden_proxy_enable;
    let vw_href_nav = if vaultwarden_proxy_enable && !vaultwarden_proxy_href.trim().is_empty() {
        vaultwarden_proxy_href
    } else if !vaultwarden_url.trim().is_empty() {
        vaultwarden_url
    } else {
        "/system".to_string()
    };
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
                        <span class="header-role">"Operator console"</span>
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
                    {if vw_configured {
                        if vw_same_origin {
                            view! {
                                <a href=vw_href_nav class="nav-vault">
                                    "Vault"
                                </a>
                            }
                            .into_any()
                        } else {
                            view! {
                                <a href=vw_href_nav rel="noopener noreferrer" target="_blank" class="nav-vault">
                                    "Vault"
                                </a>
                            }
                            .into_any()
                        }
                    } else {
                        view! {
                            <a href="/system" class="nav-vault nav-vault-residual">
                                "Vault"
                            </a>
                        }
                        .into_any()
                    }}
                </nav>
                <main>{children()}</main>
                <footer class="site-footer">{footer}</footer>
            </body>
        </html>
    }
}

/// Surmount DOGE v1.0.0 palette only (pure 3-bit RGB; eight hex values).
/// Spec: https://github.com/SurmountSystems/specs/blob/main/0001_DOGE.md
/// Semantic roles: cyan accent/muted, yellow links + true warnings, green/red
/// chips, cyan neutral for not-provisioned, white fg, black bg. No grays.
/// No light-mode media queries.
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
  --space-1: 0.25rem;
  --space-2: 0.5rem;
  --space-3: 0.75rem;
  --space-4: 1rem;
  --space-5: 1.5rem;
  --space-6: 2rem;
  --text-xs: 0.75rem;
  --text-sm: 0.875rem;
  --text-base: 1rem;
  --text-lg: 1.25rem;
  --text-xl: 1.5rem;
  --measure: 56rem;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif;
  background: var(--bg);
  color: var(--fg);
  line-height: 1.55;
  font-size: var(--text-base);
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}
header {
  border-bottom: 1px solid var(--border);
  padding: var(--space-4) var(--space-5);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  flex-wrap: wrap;
}
.brand {
  display: flex;
  align-items: baseline;
  gap: var(--space-3);
  flex-wrap: wrap;
}
header h1 {
  font-size: var(--text-lg);
  font-weight: 600;
  margin: 0;
  letter-spacing: 0.01em;
  color: var(--fg);
}
.domain {
  color: var(--muted);
  font-size: var(--text-sm);
  font-weight: 500;
}
.header-role {
  color: var(--muted);
  font-size: var(--text-xs);
  font-weight: 500;
  letter-spacing: 0.02em;
  opacity: 0.9;
}
.header-meta { display: flex; align-items: center; gap: var(--space-2); }
nav {
  border-bottom: 1px solid var(--border);
  padding: var(--space-2) var(--space-5);
  display: flex;
  gap: var(--space-5);
  flex-wrap: wrap;
}
nav a {
  color: var(--link);
  text-decoration: none;
  font-size: var(--text-sm);
  padding: var(--space-2) 0;
  border-bottom: 2px solid transparent;
}
nav a:hover { color: var(--accent); }
nav a.active {
  color: var(--fg);
  border-bottom-color: var(--ok);
  font-weight: 600;
}
nav a.nav-vault-residual { color: var(--muted); }
main {
  max-width: var(--measure);
  margin: 0 auto;
  padding: var(--space-6) var(--space-5) var(--space-5);
  width: 100%;
  flex: 1;
}
.page-kicker {
  margin: 0 0 var(--space-1);
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--muted);
}
.page-title {
  margin: 0 0 var(--space-3);
  font-size: var(--text-xl);
  font-weight: 600;
  letter-spacing: 0.01em;
  color: var(--fg);
  line-height: 1.25;
}
.lede {
  margin: 0 0 var(--space-5);
  font-size: var(--text-sm);
  color: var(--muted);
  max-width: 42rem;
}
.health-strip {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
  margin: 0 0 var(--space-4);
  padding: var(--space-3) 0;
  border-bottom: 1px solid var(--border);
}
.health-line {
  font-size: var(--text-sm);
  font-weight: 600;
  letter-spacing: 0.02em;
}
.health-ok { color: var(--ok); }
.health-warn { color: var(--link); }
.host-chip {
  font-size: var(--text-xs);
  color: var(--muted);
  background: var(--bg);
  border: 1px solid var(--border);
  padding: 0.15rem 0.45rem;
}
.health-login {
  font-size: var(--text-sm);
  margin-left: auto;
}
.pkg-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: var(--space-3);
  margin: 0 0 var(--space-4);
}
@media (min-width: 36rem) {
  .pkg-grid { grid-template-columns: 1fr 1fr; }
}
@media (min-width: 52rem) {
  .pkg-grid { grid-template-columns: 1fr 1fr 1fr; }
}
.pkg-tile {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  text-decoration: none;
  color: var(--fg);
  background: var(--card);
  border: 1px solid var(--border);
  border-left: 4px solid var(--muted);
  padding: var(--space-4);
  min-height: 7.5rem;
}
.pkg-tile:hover {
  border-color: var(--accent);
  color: var(--fg);
}
.pkg-tile:focus-visible {
  outline: 2px solid var(--link);
  outline-offset: 2px;
}
.pkg-tile.state-ok { border-left-color: var(--ok); }
.pkg-tile.state-fail { border-left-color: var(--error); }
.pkg-tile.state-warn { border-left-color: var(--link); }
.pkg-tile.state-neutral { border-left-color: var(--muted); }
.pkg-title {
  font-size: var(--text-sm);
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--fg);
}
.pkg-status {
  font-size: var(--text-xl);
  font-weight: 600;
  line-height: 1.15;
  color: var(--fg);
  letter-spacing: 0.01em;
}
.pkg-status-num {
  font-variant-numeric: tabular-nums;
  font-size: 1.75rem;
}
.pkg-detail {
  font-size: var(--text-xs);
  color: var(--muted);
  margin-top: auto;
  min-height: 1em;
}
.alert-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
  margin: 0 0 var(--space-4);
  border: 1px solid var(--link);
  background: var(--card);
}
.alert-chips {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  align-items: center;
}
.alert-chips a.chip {
  text-decoration: none;
  min-width: auto;
}
.alert-chips a.chip:hover { color: var(--accent); border-color: var(--accent); }
.card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 0;
  padding: var(--space-4) var(--space-5);
  margin-bottom: var(--space-4);
}
.card-primary {
  border-color: var(--accent);
}
.card-attention {
  border-color: var(--link);
}
.card-muted {
  border-color: var(--muted);
}
.card-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 0;
  margin-bottom: 0;
}
@media (min-width: 40rem) {
  .card-grid {
    grid-template-columns: 1fr 1fr;
    gap: var(--space-4);
  }
  .card-grid .card { margin-bottom: var(--space-4); }
}
h2 {
  font-size: var(--text-sm);
  font-weight: 600;
  margin: 0 0 var(--space-3);
  color: var(--fg);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
h2.page-title {
  font-size: var(--text-xl);
  letter-spacing: 0.01em;
  text-transform: none;
  margin: 0 0 var(--space-3);
  line-height: 1.25;
}
h3, h3.card-title {
  font-size: var(--text-sm);
  font-weight: 600;
  margin: 0 0 var(--space-3);
  color: var(--fg);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
a:focus-visible,
button:focus-visible,
nav a:focus-visible,
.pkg-tile:focus-visible {
  outline: 2px solid var(--link);
  outline-offset: 2px;
}
code:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}
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
.list-body {
  color: var(--muted);
  font-size: var(--text-sm);
  margin: var(--space-3) 0;
}
.link-row {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2) var(--space-5);
}
.link-row li { margin: 0; }
.kv {
  margin: 0;
}
.kv-row {
  display: grid;
  grid-template-columns: 7rem 1fr;
  gap: var(--space-2) var(--space-3);
  padding: var(--space-2) 0;
  border-bottom: 1px solid var(--border);
}
.kv-row:last-child { border-bottom: 0; }
.kv-row dt {
  margin: 0;
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--fg);
}
.kv-row dd {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-sm);
}
.pill {
  display: inline-block;
  background: var(--bg);
  border: 1px solid var(--muted);
  color: var(--muted);
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
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
  font-size: var(--text-xs);
  padding: 0.2rem 0.55rem;
  border-radius: 0;
  font-weight: 600;
  letter-spacing: 0.03em;
  text-transform: uppercase;
  min-width: 5.5rem;
  text-align: center;
  flex: 0 0 auto;
}
.chip-ok { border-color: var(--ok); color: var(--ok); }
.chip-fail { border-color: var(--error); color: var(--error); }
.chip-warn { border-color: var(--link); color: var(--link); }
.chip-neutral { border-color: var(--muted); color: var(--muted); }
.banner {
  border: 1px solid var(--link);
  padding: var(--space-3) var(--space-4);
  margin: 0 0 var(--space-3);
  color: var(--link);
  font-size: var(--text-sm);
}
.banner-warn {
  border-color: var(--link);
  color: var(--link);
}
.note { font-size: var(--text-sm); margin: var(--space-2) 0; }
.empty {
  color: var(--muted);
  font-style: italic;
  font-size: var(--text-sm);
  margin: var(--space-4) 0;
  padding: var(--space-4);
  border: 1px solid var(--muted);
}
table {
  width: 100%;
  border-collapse: collapse;
  margin-top: var(--space-3);
  font-size: var(--text-sm);
}
th, td {
  text-align: left;
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--border);
  color: var(--muted);
}
th { color: var(--fg); font-weight: 600; }
table.defs th {
  width: 11rem;
  vertical-align: top;
  color: var(--fg);
}
button {
  font: inherit;
  font-size: var(--text-sm);
  font-weight: 600;
  background: var(--bg);
  color: var(--link);
  border: 1px solid var(--link);
  padding: var(--space-2) var(--space-4);
  cursor: pointer;
}
button:hover {
  color: var(--accent);
  border-color: var(--accent);
}
.form-stack {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  max-width: 28rem;
  margin: 0 0 var(--space-4);
}
.form-stack label {
  font-size: var(--text-xs);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--fg);
  margin-top: var(--space-2);
}
.form-stack input {
  font: inherit;
  font-size: var(--text-sm);
  background: var(--bg);
  color: var(--fg);
  border: 1px solid var(--border);
  padding: var(--space-2);
}
.form-stack input:focus-visible {
  outline: 2px solid var(--link);
  outline-offset: 2px;
}
.form-error { color: var(--error); font-size: var(--text-sm); }
.form-ok { color: var(--ok); font-size: var(--text-sm); }
.site-footer {
  border-top: 1px solid var(--border);
  margin-top: auto;
  padding: var(--space-4) var(--space-5) var(--space-5);
  font-size: var(--text-xs);
  color: var(--muted);
  max-width: var(--measure);
  margin-left: auto;
  margin-right: auto;
  width: 100%;
}
"#;

/// Public main-site page for apex / www (not the operator console).
///
/// DOGE dark theme, yellow UNDER CONSTRUCTION headline, no admin nav, no
/// operator-console explanation. Operator console lives only on the
/// services hostname.
pub fn render_coming_soon(primary_domain: &str, version: &str) -> String {
    let domain = primary_domain.trim().to_string();
    let footer = footer_line(version);
    let title = if domain.is_empty() {
        "Surmount - UNDER CONSTRUCTION".to_string()
    } else {
        format!("Surmount - UNDER CONSTRUCTION · {domain}")
    };
    let show_domain = !domain.is_empty();
    // Contiguous strings avoid Leptos SSR hydration markers.
    view! {
        <!DOCTYPE html>
        <html lang="en" data-theme="doge" data-surface="coming-soon">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="color-scheme" content="only dark"/>
                <meta name="robots" content="index,follow"/>
                <title>{title}</title>
                <style>{COMING_SOON_CSS}</style>
            </head>
            <body data-surmount-ssr="leptos" data-theme="doge" data-surface="coming-soon">
                <main class="coming-soon">
                    <p class="eyebrow">"Surmount"</p>
                    <h1>"UNDER CONSTRUCTION"</h1>
                    {show_domain.then(|| {
                        view! { <p class="domain-line">{domain.clone()}</p> }
                    })}
                </main>
                <footer class="site-footer">{footer}</footer>
            </body>
        </html>
    }
    .to_html()
}

/// Minimal DOGE stylesheet for the public UNDER CONSTRUCTION surface (no admin chrome).
const COMING_SOON_CSS: &str = r#"
:root, [data-theme="doge"] {
  color-scheme: only dark;
  --bg: #000000;
  --fg: #FFFFFF;
  --muted: #00FFFF;
  --accent: #00FFFF;
  --warn: #FFFF00;
  --border: #FFFFFF;
  --space-2: 0.5rem;
  --space-4: 1rem;
  --space-5: 1.5rem;
  --space-6: 2rem;
  --text-xs: 0.75rem;
  --text-sm: 0.875rem;
  --text-base: 1rem;
  --text-lg: 1.25rem;
  --text-xl: 1.5rem;
  --text-2xl: 2rem;
  --measure: 36rem;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, sans-serif;
  background: var(--bg);
  color: var(--fg);
  line-height: 1.55;
  font-size: var(--text-base);
  min-height: 100vh;
  display: flex;
  flex-direction: column;
}
main.coming-soon {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  max-width: var(--measure);
  margin: 0 auto;
  padding: var(--space-6) var(--space-5);
  width: 100%;
}
.eyebrow {
  margin: 0 0 var(--space-2);
  color: var(--muted);
  font-size: var(--text-sm);
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}
main.coming-soon h1 {
  margin: 0 0 var(--space-4);
  font-size: var(--text-2xl);
  font-weight: 650;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--warn);
}
.domain-line {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-sm);
}
.site-footer {
  border-top: 1px solid var(--border);
  margin-top: auto;
  padding: var(--space-4) var(--space-5) var(--space-5);
  font-size: var(--text-xs);
  color: var(--muted);
  max-width: var(--measure);
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

pub async fn mail_page(
    State(state): State<Arc<AppState>>,
    Extension(nonce): Extension<CspNonce>,
    headers: HeaderMap,
) -> Response {
    let status = probe_stalwart(&state).await;
    let chip = StalwartChip::from_status(&status);
    let data = page_data_from_state(&state, "/mail", chip).await;
    let (csrf_token, set_csrf) = mail_page_csrf(&state, &headers);
    let view = match mail_console_view(&state, &headers) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Html(
                    "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"/><title>Unavailable</title></head><body><p>Console account map unavailable.</p></body></html>",
                ),
            )
                .into_response();
        }
    };
    let extras = mail_page_extras(&state, &view);
    let html = render_mail_page_with(&data, &status, nonce.as_str(), &csrf_token, &view, &extras);
    let mut response = Html(html).into_response();
    // F5 must re-fetch this HTML (hidden field + script), not a pre-patch cache.
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(h) = set_csrf
        && let Ok(v) = HeaderValue::from_str(&h)
    {
        response.headers_mut().append(header::SET_COOKIE, v);
    }
    response
}

/// Embed a session-bound CSRF token (HMAC of the session cookie). Optionally
/// Set-Cookie the same value as a belt; POST password does not require that
/// cookie.
fn mail_page_extras(state: &AppState, view: &MailConsoleView) -> MailPageExtras {
    let map = load_console_accounts(&state.config.console_accounts_path).unwrap_or_default();
    let roster = if view.can_manage_people() {
        map.accounts
            .iter()
            .filter_map(|e| {
                let primary = e.mailbox_normalized()?;
                Some(MailboxRosterRow {
                    primary,
                    aliases: e.aliases.clone(),
                    npub_attached: e.npub_normalized().is_some(),
                    password_set: e.password_set,
                    role: e.role.as_str().to_string(),
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let nwc_connected = match view {
        MailConsoleView::User {
            mailbox: Some(addr),
        } if !addr.trim().is_empty() => {
            surmount_management_ui::nwc::nwc_connected(&state.config.nwc_store_path, addr)
                .unwrap_or(false)
        }
        _ => false,
    };
    MailPageExtras {
        roster,
        nwc_connected,
    }
}

fn mail_console_view(state: &AppState, headers: &HeaderMap) -> Result<MailConsoleView, String> {
    if !state.config.auth.mode.is_nostr() {
        return Ok(MailConsoleView::Administrator);
    }
    let Some(secret) = state
        .config
        .auth
        .session_secret
        .as_ref()
        .filter(|s| !s.is_empty())
    else {
        return Ok(MailConsoleView::User { mailbox: None });
    };
    let Some(session) = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| cookie_value(c, SESSION_COOKIE_NAME))
    else {
        return Ok(MailConsoleView::User { mailbox: None });
    };
    let Ok(payload) = decode_session_cookie(session, secret, now_unix()) else {
        return Ok(MailConsoleView::User { mailbox: None });
    };
    let map = load_console_accounts(&state.config.console_accounts_path)?;
    let principal = resolve_console_principal(&payload.sub, &state.config.auth.allowlist, &map);
    if principal.is_administrator() {
        Ok(MailConsoleView::Administrator)
    } else {
        Ok(MailConsoleView::User {
            mailbox: principal.mailbox,
        })
    }
}

fn mail_page_csrf(state: &AppState, headers: &HeaderMap) -> (String, Option<String>) {
    if !mail_session_is_valid(state, headers) {
        return (String::new(), None);
    }
    let Some(secret) = state
        .config
        .auth
        .session_secret
        .as_ref()
        .filter(|s| !s.is_empty())
    else {
        return (String::new(), None);
    };
    let Some(session) = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| cookie_value(c, SESSION_COOKIE_NAME))
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return (String::new(), None);
    };
    let Ok(tok) = session_bound_csrf_token(session, secret) else {
        return (String::new(), None);
    };
    let secure = state.config.listen_mode.is_https();
    let set = csrf_set_cookie_header(&tok, state.config.auth.session_ttl_secs, secure);
    (tok, Some(set))
}

fn mail_session_is_valid(state: &AppState, headers: &HeaderMap) -> bool {
    if !state.config.auth.mode.is_nostr() {
        return false;
    }
    let Some(secret) = state
        .config
        .auth
        .session_secret
        .as_ref()
        .filter(|s| !s.is_empty())
    else {
        return false;
    };
    let Some(cookie_hdr) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(val) = cookie_value(cookie_hdr, SESSION_COOKIE_NAME) else {
        return false;
    };
    decode_session_cookie(val, secret, now_unix()).is_ok()
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
    let mode_note = auth_system_detail(&data.auth_mode);
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
      if ((next === '/' || next === '') && body.role === 'user') {
        next = '/mail';
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
            vaultwarden_url=data.vaultwarden_url.clone().unwrap_or_default()
            vaultwarden_proxy_enable=data.vaultwarden_proxy_enable
            vaultwarden_proxy_href=data.vaultwarden_proxy_href.clone()
        >
            <p class="page-kicker">"Login"</p>
            <h2 class="page-title">"Nostr login"</h2>
            <div class="card">
                <p class="note">{mode_note}</p>
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
    use crate::api::{DomainEntry, accounts_inventory};

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
        sample_data_with_onion_and_vault(path, onion, None)
    }

    fn sample_data_with_vault(path: &str, vault: Option<String>) -> AdminPageData {
        sample_data_with_onion_and_vault(path, None, vault)
    }

    fn sample_data_with_onion_and_vault(
        path: &str,
        onion: Option<String>,
        vaultwarden_url: Option<String>,
    ) -> AdminPageData {
        let onion_surface = OnionSurface::from_url_opt(onion.clone());
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
            onion_surface,
            onion_url: onion,
            vaultwarden_url,
            vaultwarden_proxy_enable: false,
            vaultwarden_proxy_href: String::new(),
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

    /// Healthy host-ish Overview: auth on, live directory, onion + vault linked.
    fn sample_healthy_overview() -> AdminPageData {
        let onion =
            "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion".to_string();
        let mut data = sample_data_with_onion_and_vault(
            "/",
            Some(onion),
            Some("http://127.0.0.1:8222".into()),
        );
        data.auth_mode = "nostr".into();
        data.stalwart = StalwartChip::Ok;
        data.accounts = AccountsInventory {
            accounts: Vec::new(),
            source: "stalwart",
            note: "Live directory connected (honest empty list).".into(),
        };
        data
    }

    /// Named contract: public apex page is yellow UNDER CONSTRUCTION, not operator console.
    #[test]
    fn render_coming_soon_is_public_not_operator_console() {
        let html = render_coming_soon("example.test", "0.1.0");
        let lower = html.to_ascii_lowercase();
        assert!(
            html.contains("UNDER CONSTRUCTION") || lower.contains("under construction"),
            "must show UNDER CONSTRUCTION"
        );
        assert!(
            html.contains("#FFFF00") || html.contains("#ffff00"),
            "UNDER CONSTRUCTION headline must be DOGE yellow (#FFFF00): {html}"
        );
        assert!(
            html.contains(r#"data-surface="coming-soon""#),
            "must mark coming-soon surface"
        );
        assert!(html.contains("example.test"), "must show primary domain");
        assert!(
            !html.contains("Operator console"),
            "must not claim operator console"
        );
        assert!(
            !html.contains("This is the public site, not the operator console"),
            "must not explain operator console on the public internet"
        );
        assert!(
            !html.contains("Something solid is on the way"),
            "must not keep the old public-vs-console lede"
        );
        assert!(
            !html.contains("Dashboard")
                && !html.contains("Stalwart OK")
                && !html.contains(r#"href="/domains""#)
                && !html.contains(r#"href="/accounts""#)
                && !html.contains(r#"aria-label="Primary""#),
            "must not include operator nav or console chrome: {html}"
        );
        assert_doge_palette_only(&html);
    }

    #[test]
    fn render_admin_shell_is_leptos_ssr_html() {
        let html = render_admin_shell("example.test", "mail.example.test", "0.1.0");
        assert!(
            html.contains(r#"data-surmount-ssr="leptos""#),
            "missing Leptos SSR marker"
        );
        assert!(
            html.contains("Dashboard"),
            "overview must show Dashboard title"
        );
        assert!(
            html.contains("pkg-grid") && html.contains("pkg-tile"),
            "overview must show package tile grid"
        );
        assert!(html.contains("example.test"));
        assert!(html.contains("mail.example.test") || html.contains("services.example.test"));
        assert!(html.contains("Leptos SSR"));
        assert!(
            html.contains("Operator console"),
            "header meta should be quiet Operator console"
        );
        assert!(
            !html.to_ascii_lowercase().contains("local operator"),
            "must not shout LOCAL OPERATOR cosplay pill"
        );
        assert!(
            html.contains("chip-neutral") || html.contains("state-neutral"),
            "not-provisioned / unknown use neutral state styling"
        );
        assert!(
            html.contains(":focus-visible") || html.contains("focus-visible"),
            "shell CSS must include focus-visible rings"
        );
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
            "",
            "",
            &MailConsoleView::Administrator,
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
            acme: crate::acme::AcmeConfig::default(),
            mta_sts_mode: crate::mta_sts::MtaStsMode::Off,
            mta_sts_max_age: 86_400,
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_surface: OnionSurface::NotProvisioned,
            onion_url: None,
            onion_discovery: crate::onion_discovery::OnionDiscoveryConfig::empty(),
            vaultwarden_url: None,
            vaultwarden_proxy: crate::proxy_vaultwarden::VaultwardenProxyConfig::default(),
            splora_proxy: crate::proxy_vaultwarden::splora::SploraProxyConfig::default(),
            http3: crate::tls::http3::Http3Config::default(),
            apex_public_root: None,
            static_vhosts: Default::default(),
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
            allow_public_auth_off: false,
            console_accounts_path: crate::config::unused_console_accounts_path(),
            nwc_store_path: crate::config::unused_nwc_store_path(),
        });
        assert!(cfg_note.accounts.is_empty());
        assert_eq!(cfg_note.source, "unavailable");
    }

    /// Named contract: when onion is set, system shows address; Overview tile says Ready.
    /// Overview is status-only (no full onion URL dump on package home).
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
            system.contains("Onion (operator-published)"),
            "system should label onion as operator-published"
        );
        let overview = render_overview(&sample_data_with_onion("/", Some(onion.clone())));
        // Co-located Onion tile: title + Ready status + state-ok class on same anchor.
        assert!(
            overview.contains(r#"class="pkg-tile state-ok""#)
                && overview.contains(r#"class="pkg-title">Onion"#)
                && overview.contains(r#"class="pkg-status">Ready"#),
            "overview onion tile Ready with state-ok (co-located markers)"
        );
        assert!(
            !overview.contains(r#"class="pkg-status">Missing"#)
                && !overview.contains("hostname missing"),
            "configured must not show Missing residual"
        );
        // Full address lives on System only; package home must not leak the onion URL.
        assert!(
            !overview.contains(&onion) && !overview.contains(".onion"),
            "overview must not dump configured onion URL (status-only)"
        );
        assert_doge_palette_only(&system);
        assert_doge_palette_only(&overview);
    }

    /// Named contract: without onion config, do not invent an address.
    /// System residual points at surmount.artiHiddenService; Overview tile says Off.
    #[test]
    fn onion_unset_shows_residual_not_invented() {
        let system = render_system_page(&sample_data("/system"));
        let overview = render_overview(&sample_data("/"));
        assert!(
            system.contains("Not provisioned"),
            "system residual when onion unset: {}",
            system.chars().take(800).collect::<String>()
        );
        assert!(
            system.contains("surmount.artiHiddenService"),
            "system residual must name real enable path: {}",
            system.chars().take(1200).collect::<String>()
        );
        assert!(
            !system.contains("SURMOUNT_ONION_URL") && !system.contains("local Arti demo"),
            "must not sell demo env stub as product residual"
        );
        assert!(
            overview.contains("Onion") && overview.contains("Off"),
            "overview onion tile Off when unset"
        );
        assert!(
            overview.contains("state-neutral"),
            "not_provisioned onion tile uses state-neutral"
        );
        assert!(
            !overview.contains(">Missing<") && !overview.contains("hostname missing"),
            "not_provisioned must not claim Missing"
        );
        assert!(
            !overview.contains("SURMOUNT_ONION_URL") && !overview.contains("local Arti demo"),
            "overview must not sell demo env stub"
        );
        assert!(
            !overview.contains("surmount.artiHiddenService"),
            "overview must not essay onion enable path (System owns it)"
        );
        // No fabricated 56-char v3 onion labels in default renders.
        for html in [&system, &overview] {
            assert!(
                !html.contains(".onion"),
                "must not invent .onion address when unset"
            );
        }
    }

    /// Named contract: hostname_missing residual names host path on System; Overview tile Missing.
    #[test]
    fn onion_hostname_missing_shows_host_path_residual() {
        let host_path = "/run/surmount-secrets/arti/onion-service/hostname";
        let mut data = sample_data("/system");
        data.onion_surface = OnionSurface::HostnameMissing {
            path: Some(host_path.into()),
            hs_state_dir: Some("/run/surmount-secrets/arti/onion-service".into()),
        };
        data.onion_url = None;
        let system = render_system_page(&data);
        assert!(
            system.contains("Hostname material missing"),
            "system residual for missing hostname: {}",
            system.chars().take(1200).collect::<String>()
        );
        assert!(
            system.contains(host_path),
            "system must surface host hostname path: {}",
            system.chars().take(1200).collect::<String>()
        );
        assert!(
            system.contains("surmount.artiHiddenService")
                || system.contains("onionServiceStateDir"),
            "must name real enable path"
        );
        assert!(!system.contains("SURMOUNT_ONION_URL"));
        assert!(!system.contains(".onion"));
        assert!(
            !system.contains("Not provisioned on this host"),
            "hostname_missing must not use not_provisioned body"
        );
        // auth nostr so Auth alert chip is absent; onion tile carries warn state.
        let mut overview_data = sample_data("/");
        overview_data.auth_mode = "nostr".into();
        overview_data.onion_surface = OnionSurface::HostnameMissing {
            path: Some(host_path.into()),
            hs_state_dir: None,
        };
        let overview = render_overview(&overview_data);
        assert!(
            overview.contains(r#"class="pkg-title">Onion"#)
                && overview.contains(r#"class="pkg-status">Missing"#),
            "overview onion tile Missing when hostname material absent"
        );
        assert!(
            overview.contains(r#"class="pkg-tile state-warn""#)
                && overview.contains(r#"class="pkg-status">Missing"#),
            "hostname_missing onion tile uses state-warn"
        );
        // Chip severity aligns with tile: Missing is warn, not neutral-only.
        assert!(
            overview.contains(r#"class="chip chip-warn">Onion"#),
            "Onion HostnameMissing alert chip uses chip-warn"
        );
        assert!(
            !overview.contains(r#"class="chip chip-neutral">Onion"#),
            "Onion HostnameMissing must not use neutral chip class"
        );
        assert!(
            !overview.contains(r#"class="chip chip-warn">Auth"#),
            "auth_mode=nostr must not inject Auth alert chip-warn"
        );
        assert!(
            !overview.contains("surmount.artiHiddenService"),
            "overview must not essay enable path for hostname_missing"
        );
    }

    /// Named contract: Vaultwarden residual when URL unset (SSR markers; no invent).
    #[test]
    fn vaultwarden_unset_shows_residual_not_invented() {
        let system = render_system_page(&sample_data("/system"));
        let overview = render_overview(&sample_data("/"));
        assert!(
            system.contains("Vaultwarden") && system.contains("Not configured"),
            "system residual when vault unset: {}",
            system.chars().take(1200).collect::<String>()
        );
        assert!(
            system.contains("SURMOUNT_VAULTWARDEN_URL"),
            "system technical residual may name vault URL env"
        );
        assert!(
            overview.contains("Vault") && overview.contains("Off"),
            "overview vault tile Off when unset"
        );
        assert!(
            !overview.contains("SURMOUNT_VAULTWARDEN_URL"),
            "overview must not primary residual vault env"
        );
        assert!(
            system.contains("nav-vault-residual") || system.contains("Vault"),
            "nav always includes Vault place"
        );
        for html in [&system, &overview] {
            assert!(
                !html.contains("ADMIN_TOKEN") && !html.contains("admin_token"),
                "must not leak admin token"
            );
            assert!(!html.contains("Open vault"), "no Open vault without URL");
        }
    }

    /// Named contract: when Vaultwarden URL set, system Open vault; Overview tile Linked.
    #[test]
    fn vaultwarden_configured_shows_open_vault_link() {
        let url = "http://127.0.0.1:8222".to_string();
        let system = render_system_page(&sample_data_with_vault("/system", Some(url.clone())));
        assert!(
            system.contains(&url),
            "system page must show configured vault url"
        );
        assert!(
            system.contains("Open vault"),
            "system should offer Open vault external link"
        );
        assert!(
            system.contains("Vaultwarden (operator-published)")
                || system.contains("operator-published"),
            "system should label vault as operator-published"
        );
        assert!(
            system.contains("not reverse-proxied here"),
            "external link mode must stay honest about no path proxy"
        );
        let overview = render_overview(&sample_data_with_vault("/", Some(url.clone())));
        assert!(
            overview.contains("Vault") && overview.contains("Linked"),
            "overview vault tile Linked when URL set"
        );
        assert!(
            overview.contains(&url),
            "overview vault tile href includes configured vault URL"
        );
        // Nav Vault is external when configured (target blank).
        assert!(
            overview.contains("nav-vault") || overview.contains("Vault"),
            "nav includes Vault"
        );
        for html in [&system, &overview] {
            assert!(
                !html.contains("ADMIN_TOKEN") && !html.contains("admin_token"),
                "configured vault SSR must not leak admin token"
            );
        }
        assert_doge_palette_only(&system);
        assert_doge_palette_only(&overview);
    }

    /// Named contract: proxy-on Open vault is same-origin path, not "not reverse-proxied".
    #[test]
    fn vaultwarden_proxy_on_same_origin_open_vault() {
        let mut data = sample_data("/system");
        data.vaultwarden_proxy_enable = true;
        data.vaultwarden_proxy_href = "/vault/".into();
        data.vaultwarden_url = Some("https://services.example.test/vault".into());
        let system = render_system_page(&data);
        assert!(
            system.contains("Open vault"),
            "proxy-on system must offer Open vault"
        );
        assert!(
            system.contains("/vault/") || system.contains("same-origin"),
            "proxy-on must surface same-origin path: {}",
            system.chars().take(1500).collect::<String>()
        );
        assert!(
            system.contains("same-origin path proxy"),
            "proxy-on label must not claim external-only"
        );
        assert!(
            !system.contains("not reverse-proxied here"),
            "proxy-on must drop not-reverse-proxied honesty when path proxy is on"
        );
        assert!(
            !system.contains("ADMIN_TOKEN") && !system.contains("admin_token"),
            "proxy-on SSR must not leak admin token"
        );
        let mut overview = sample_data("/");
        overview.vaultwarden_proxy_enable = true;
        overview.vaultwarden_proxy_href = "/vault/".into();
        let html = render_overview(&overview);
        assert!(
            html.contains("Vault") && html.contains("Linked"),
            "overview vault tile Linked when path proxy on"
        );
        assert!(
            html.contains("/vault/"),
            "overview vault tile uses same-origin path"
        );
        assert_doge_palette_only(&system);
        assert_doge_palette_only(&html);
    }

    #[test]
    fn overview_shows_directory_unavailable_account_count() {
        let html = render_overview(&sample_data("/"));
        assert!(
            html.contains(r#"class="pkg-title">Accounts"#) && html.contains("offline"),
            "accounts tile marks directory offline when unavailable"
        );
        // Co-located numeric zero on Accounts tile (not bare "0" from version/port).
        assert!(
            html.contains(r#"class="pkg-title">Accounts"#)
                && html.contains(r#"class="pkg-status pkg-status-num">0"#),
            "honest zero account count on Accounts package tile"
        );
        // Compact alert chip for Directory (no long residual essay on Overview).
        assert!(
            html.contains(r#"class="chip chip-neutral">Directory"#)
                && html.contains(r#"class="alert-bar""#),
            "directory residual is compact alert chip, not diary"
        );
    }

    /// Named contract: Overview is DSM-like package home, not env residual diary.
    /// Health strip + package tiles; compact alert chips only; no SURMOUNT_* primary residual.
    #[test]
    fn overview_operator_language_no_env_primary_residual() {
        let html = render_overview(&sample_data("/"));
        assert!(
            html.contains("Dashboard") || html.contains("Operator dashboard"),
            "overview must show operator-facing dashboard title"
        );
        assert!(
            html.contains(r#"class="health-strip""#) && html.contains("Needs attention (5)"),
            "overview must show one-line health strip with residual count"
        );
        assert!(
            html.contains(r#"class="pkg-grid""#) && html.contains("pkg-tile"),
            "overview must show package tile grid"
        );
        // Package tiles present with honest labels.
        for title in ["Console", "Mail", "Domains", "Accounts", "Onion", "Vault"] {
            assert!(
                html.contains(&format!(r#"class="pkg-title">{title}"#)),
                "package tile {title} required"
            );
        }
        assert!(
            html.contains("Management console"),
            "console tile identifies management console"
        );
        // Residual sample: compact alert bar with short chips only (no essays).
        // Require attribute form so CSS-only `.alert-bar {` cannot false-green.
        // Chip labels use class="chip …">Name so nav links cannot false-green.
        assert!(
            html.contains(r#"class="alert-bar""#),
            "residual sample must include compact alert bar element"
        );
        assert!(
            html.contains(r#"class="chip chip-warn">Auth"#),
            "auth residual is short Auth chip"
        );
        assert!(
            html.contains(r#"class="chip chip-neutral">Directory"#),
            "directory residual chip present"
        );
        assert!(
            html.contains(r#"class="chip chip-neutral">Onion"#),
            "onion residual chip present"
        );
        assert!(
            html.contains(r#"class="chip chip-neutral">Vault"#),
            "vault residual chip present"
        );
        assert!(
            html.contains(r#"class="chip chip-fail">Mail"#),
            "mail fail residual chip present"
        );
        assert!(
            html.contains(r#"class="pkg-title">Onion"#)
                && html.contains(r#"class="pkg-status">Off"#),
            "onion tile Off when not provisioned"
        );
        assert!(
            html.contains(r#"class="pkg-title">Vault"#)
                && html.contains(r#"class="pkg-status">Off"#),
            "vault tile Off when not linked"
        );
        // Health strip / tiles before alert bar.
        let health_pos = html.find(r#"class="health-strip""#).expect("health strip");
        let alert_pos = html.find(r#"class="alert-bar""#).expect("alert bar");
        assert!(
            health_pos < alert_pos,
            "health strip must appear before alert bar"
        );
        let grid_pos = html.find(r#"class="pkg-grid""#).expect("pkg grid");
        assert!(
            grid_pos < alert_pos,
            "package tiles must appear before alert bar"
        );
        // Removed essay / config-dump surfaces.
        for banned_ui in [
            "Stalwart is the mail engine",
            "Hostnames",
            "Navigate",
            "Service status",
            "Setup residual",
            "Domains are deploy/config inventory",
            "surmount.artiHiddenService",
            "Console auth is off",
            "Mail directory offline",
            "items need attention",
        ] {
            assert!(
                !html.contains(banned_ui),
                "overview must not include essay/config dump: {banned_ui}"
            );
        }
        // No env-var primary residual on Overview for these surfaces.
        for banned in [
            "SURMOUNT_AUTH_MODE",
            "SURMOUNT_DIRECTORY",
            "SURMOUNT_VAULTWARDEN_URL",
            "SURMOUNT_ONION_URL",
            "SURMOUNT_STALWART_TOKEN",
            "Q-AUTH-1",
            "Ops residual",
            "Auth residual",
            "Directory residual",
        ] {
            assert!(
                !html.contains(banned),
                "overview must not primary residual with {banned}"
            );
        }
        // Full API laundry list and invent metrics stay off Overview.
        assert!(
            !html.contains("POST /api/v1/jmap"),
            "overview should not be an API endpoint wall"
        );
        assert!(
            !html.contains("GET /api/v1/accounts"),
            "overview must not dump API catalog"
        );
        assert!(
            !html.contains("CPU") && !html.contains("load average"),
            "overview must not invent resource widgets"
        );
        assert!(
            html.contains(r#"class="page-title""#),
            "page title class attribute present (not CSS-only)"
        );
    }

    /// Named contract: health strip must not claim all healthy when Mail/Stalwart is Fail.
    /// Setup residual can be clean while the live mail probe is down.
    #[test]
    fn overview_health_strip_honest_when_mail_fail() {
        let mut data = sample_healthy_overview();
        data.stalwart = StalwartChip::Fail;
        let html = render_overview(&data);
        assert!(
            html.contains("Down") && html.contains("Mail"),
            "mail tile still shows Down when probe fails"
        );
        assert!(
            !html.contains("All services healthy"),
            "health strip must not claim all healthy when Mail is Down"
        );
        assert!(
            html.contains("Needs attention"),
            "health strip must surface Needs attention when Mail is Down"
        );
        assert!(
            html.contains(r#"class="alert-bar""#),
            "mail fail opens compact alert bar"
        );
        // Mail operational chip (setup residual chips must stay absent in this isolation).
        assert!(
            html.contains(r#"class="chip chip-fail">Mail"#),
            "Mail alert chip when Stalwart Fail"
        );
        assert!(
            !html.contains(r#"class="chip chip-warn">Auth"#),
            "auth healthy: no Auth chip"
        );
        assert!(
            !html.contains(r#"class="chip chip-neutral">Directory"#),
            "directory healthy: no Directory chip"
        );
        assert!(
            !html.contains(r#"class="chip chip-neutral">Onion"#)
                && !html.contains(r#"class="chip chip-warn">Onion"#),
            "onion healthy: no Onion chip"
        );
        assert!(
            !html.contains(r#"class="chip chip-neutral">Vault"#),
            "vault healthy: no Vault chip"
        );
        assert!(
            html.contains("Needs attention (1)"),
            "isolated mail fail is exactly one attention item"
        );
        // Count lives on health strip only (no dual "N items need attention" on bar).
        assert!(
            !html.contains("items need attention"),
            "alert bar must not repeat attention count language"
        );
    }

    /// Named contract: compact alert bar for residual; absent when host is healthy.
    #[test]
    fn overview_attention_present_when_residual_absent_when_healthy() {
        let residual = render_overview(&sample_data("/"));
        assert!(
            residual.contains(r#"class="alert-bar""#),
            "default residual sample must show compact alert bar element"
        );
        // Residual sample: auth off, directory unavailable, onion off, vault off, mail Fail.
        assert!(
            residual.contains("Needs attention (5)"),
            "default residual sample attention count is 5 (auth+dir+onion+vault+mail)"
        );
        assert!(
            residual.contains(r#"class="chip chip-warn">Auth"#),
            "auth chip in alert bar"
        );
        assert!(
            residual.contains(r#"class="chip chip-neutral">Directory"#),
            "directory chip in alert bar"
        );
        assert!(
            residual.contains(r#"class="chip chip-neutral">Onion"#),
            "onion residual chip present"
        );
        assert!(
            residual.contains(r#"class="chip chip-neutral">Vault"#),
            "vault residual chip present"
        );
        assert!(
            residual.contains(r#"class="chip chip-fail">Mail"#),
            "mail fail chip present on residual sample"
        );
        assert!(
            !residual.contains("Setup residual"),
            "no Attention essay lede"
        );
        assert!(
            !residual.contains(r#"class="card card-attention""#),
            "compact model uses alert-bar, not card-attention essay card"
        );
        // Dual count collapsed: strip owns N; bar has chips only.
        assert!(
            !residual.contains("items need attention"),
            "alert bar must not repeat N items need attention"
        );

        let healthy = render_overview(&sample_healthy_overview());
        // CSS may still define .alert-bar; require the rendered element class attribute.
        assert!(
            !healthy.contains(r#"class="alert-bar""#),
            "healthy overview must not show alert bar element"
        );
        assert!(
            !healthy.contains("Setup residual")
                && !healthy.contains("items need attention")
                && !healthy.contains("Needs attention"),
            "healthy overview must not show attention copy"
        );
        assert!(
            healthy.contains("All services healthy"),
            "healthy strip says all healthy"
        );
        assert!(
            healthy.contains("Ready") && healthy.contains("Linked"),
            "healthy sample onion Ready + vault Linked"
        );
        assert!(
            healthy.contains("pkg-grid") && healthy.contains("Console"),
            "healthy sample still has package tiles"
        );
    }

    /// Named contract: System holds technical env residual; Overview does not.
    #[test]
    fn system_holds_env_detail_overview_does_not() {
        let overview = render_overview(&sample_data("/"));
        let system = render_system_page(&sample_data("/system"));
        for env in [
            "SURMOUNT_AUTH_MODE",
            "SURMOUNT_DIRECTORY",
            "SURMOUNT_VAULTWARDEN_URL",
        ] {
            assert!(!overview.contains(env), "overview must not contain {env}");
            assert!(
                system.contains(env),
                "system technical residual must name {env}"
            );
        }
        assert!(
            !overview.contains("SURMOUNT_ONION_URL"),
            "overview never sells onion env stub"
        );
        assert!(
            !system.contains("SURMOUNT_ONION_URL"),
            "system onion residual uses arti module path, not demo env"
        );
        // Hostnames live on System after Overview redesign.
        assert!(
            system.contains("Hostnames")
                && system.contains("Primary")
                && system.contains("services.example.test"),
            "system holds hostnames table moved off Overview"
        );
        assert!(
            !overview.contains(r#"class="card-title">Hostnames"#)
                && !overview.contains(">Hostnames<"),
            "overview must not show Hostnames card"
        );
    }

    /// Named contract: mode=nostr surfaces /login; mode=off does not pretend gate.
    #[test]
    fn overview_and_system_auth_mode_honesty_and_login_link() {
        let mut off = sample_data("/");
        off.auth_mode = "off".into();
        let overview_off = render_overview(&off);
        assert!(
            overview_off.contains(r#"class="chip chip-warn">Auth"#)
                && overview_off.contains(r#"class="alert-bar""#),
            "overview should surface Auth alert chip when auth is off"
        );
        // Login href optional when off; must not claim gate enabled.
        assert!(!overview_off.contains("Nostr gate enabled"));

        let mut nostr = sample_data("/");
        nostr.auth_mode = "nostr".into();
        let overview_nostr = render_overview(&nostr);
        // When auth is ok, alert omits Auth; health strip still links Login.
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
            system_html.contains("mode=nostr"),
            "system should document nostr gate when enabled"
        );
        assert!(
            system_html.contains("docs/SECURITY.md") || system_html.contains("ban matrix"),
            "system should point at SECURITY ban matrix residual"
        );
        let system_off = render_system_page(&sample_data("/system"));
        assert!(
            system_off.contains("SURMOUNT_AUTH_MODE"),
            "system auth-off detail names enable env"
        );
        assert!(
            system_off.contains("Console auth is off"),
            "system holds product auth-off copy moved off Overview"
        );
        assert_doge_palette_only(&overview_nostr);
        assert_doge_palette_only(&system_html);
    }

    /// Named contract: mock directory still surfaces compact alert (aligned with show_dir_attn).
    #[test]
    fn overview_mock_directory_shows_attention() {
        let mut data = sample_healthy_overview();
        data.accounts = AccountsInventory {
            accounts: Vec::new(),
            source: "mock",
            note: "Hermetic mock.".into(),
        };
        // Keep other surfaces healthy so only mock drives attention.
        let html = render_overview(&data);
        assert!(
            html.contains(r#"class="alert-bar""#),
            "mock directory must open compact alert bar"
        );
        assert!(
            html.contains("Needs attention (1)"),
            "mock-only residual is exactly one attention item"
        );
        assert!(
            html.contains(r#"class="chip chip-neutral">Directory"#),
            "mock directory surfaces Directory alert chip"
        );
        for forbidden_chip in [
            r#"class="chip chip-warn">Auth"#,
            r#"class="chip chip-fail">Mail"#,
            r#"class="chip chip-neutral">Mail"#,
            r#"class="chip chip-neutral">Onion"#,
            r#"class="chip chip-warn">Onion"#,
            r#"class="chip chip-neutral">Vault"#,
        ] {
            assert!(
                !html.contains(forbidden_chip),
                "mock isolation: unexpected chip marker {forbidden_chip}"
            );
        }
        assert!(
            html.contains(r#"class="pkg-title">Accounts"#) && html.contains("mock"),
            "accounts tile shows mock source honestly"
        );
        assert!(
            !html.contains("items need attention"),
            "count lives on health strip only"
        );
    }

    /// Named contract: accounts list uses empty class only when the list is empty.
    #[test]
    fn accounts_page_empty_class_only_when_empty() {
        let empty = render_accounts_page(&sample_data("/accounts"));
        assert!(
            empty.contains(r#"class="empty""#),
            "empty directory should use empty class"
        );

        let mut populated = sample_data("/accounts");
        populated.accounts = AccountsInventory {
            accounts: vec![crate::api::AccountEntry {
                id: "a1".into(),
                address: "ops@example.test".into(),
                status: "individual".into(),
            }],
            source: "mock",
            note: "Mock fixture.".into(),
        };
        let html = render_accounts_page(&populated);
        assert!(
            html.contains("ops@example.test"),
            "populated list should show address"
        );
        assert!(
            !html.contains(r#"class="empty""#),
            "populated accounts must not use empty class"
        );
        assert!(
            html.contains("list-body"),
            "populated accounts use list-body class"
        );
    }

    /// Named contract: /mail has a Set mailbox password card (operator console).
    #[test]
    fn mail_page_renders_set_mailbox_password_form() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "",
            &MailConsoleView::Administrator,
        );
        let lower = html.to_ascii_lowercase();
        assert!(
            html.contains("Set mailbox password"),
            "mail page must keep the Set mailbox password button; snippet: {}",
            html.chars().take(800).collect::<String>()
        );
        let addr = html
            .find(r#"id="mailbox-address""#)
            .expect("mailbox-address");
        let addr_win = html.get(addr..).and_then(|s| s.get(..500)).unwrap_or("");
        assert!(
            !addr_win.contains("hunter@surmount.systems"),
            "mailbox field must not prefill a living address; window={addr_win}"
        );
        assert!(
            addr_win.contains("placeholder")
                && addr_win.to_ascii_lowercase().contains("full email"),
            "mailbox field placeholder must say full email"
        );
        assert!(
            lower.contains("type=\"password\"") || lower.contains("type='password'"),
            "must render password inputs"
        );
        assert!(
            html.contains("IMAP")
                && html.contains("993")
                && html.to_ascii_lowercase().contains("ssl"),
            "must show IMAP port 993 SSL"
        );
        assert!(
            html.contains("SMTP") && html.contains("465"),
            "must show SMTP port 465 SSL"
        );
        assert!(
            lower.contains("self-signed")
                || lower.contains("let's encrypt")
                || lower.contains("does not cover mail"),
            "must mention mail-port TLS warning"
        );
        assert!(
            !lower.contains("mail passwords still via stalwart-cli"),
            "portal password form is the primary path"
        );
        assert_doge_palette_only(&html);
    }

    /// Named contract: Evolution IMAP login facts on /mail.
    ///
    /// User Name must be the full address (not the local-part alone).
    /// Authentication is Normal password, not OAuth2. Public mail TLS is
    /// Let's Encrypt.
    #[test]
    fn mail_page_states_evolution_imap_login_facts() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "",
            &MailConsoleView::Administrator,
        );
        assert!(
            html.contains("full address") || html.contains("that full address"),
            "must state the login name is the full mailbox address; snippet: {}",
            html.chars().take(900).collect::<String>()
        );
        assert!(
            !html.contains("hunter@surmount.systems"),
            "must not hardcode a living mailbox as the login example"
        );
        assert!(
            html.contains("Normal password") && html.contains("OAuth2"),
            "must say Normal password, not OAuth2"
        );
        assert!(
            html.contains("Evolution"),
            "mail app help must name Evolution"
        );
        let lower = html.to_ascii_lowercase();
        assert!(
            lower.contains("let's encrypt") || lower.contains("lets encrypt"),
            "must say TLS is Let's Encrypt"
        );
    }

    /// Named contract: Evolution may wrap Stalwart IMAP BYE timeout as auth.
    ///
    /// Live Imap.timeoutAnonymous is the unauthenticated next-read timer.
    /// Stalwart writes `* BYE Connection timed out.` when it fires. Evolution
    /// may show Failed to authenticate even when the password is set.
    /// The /mail card must say that is a wait, not by itself a bad password.
    #[test]
    fn mail_page_states_evolution_may_wrap_imap_bye_timeout_as_auth() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "",
            &MailConsoleView::Administrator,
        );
        assert!(
            html.contains("Failed to authenticate")
                && html.contains("BYE")
                && html.contains("Connection timed out"),
            "must name the Evolution banner that wraps Stalwart BYE timeout; \
             snippet: {}",
            html.chars().take(1200).collect::<String>()
        );
        assert!(
            html.contains("not by itself a bad password") || html.contains("not a bad password"),
            "must say the BYE timeout banner is not by itself a bad password"
        );
        assert!(
            html.contains("first folder scan") && html.contains("long time"),
            "must keep the first-folder-scan wait note"
        );
    }

    /// Named contract: password INPUT id is unique (not shared with the card).
    ///
    /// Accounts links to `/mail#mailbox-password` on the card. If the INPUT
    /// reuses that id, `getElementById('mailbox-password')` returns the DIV
    /// (no `.value`), so the client treats the password as empty and reports
    /// a false mismatch against confirm.
    #[test]
    fn mail_page_password_input_id_is_unique_and_script_reads_input() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "",
            &MailConsoleView::Administrator,
        );

        assert_eq!(
            html.matches(r#"id="mailbox-password""#).count(),
            1,
            "id=mailbox-password must appear once (card fragment only); \
             sharing it with the password INPUT makes getElementById return the DIV"
        );
        assert!(
            html.contains(r#"id="mailbox-password""#),
            "card must keep #mailbox-password for Accounts fragment links"
        );

        assert_eq!(
            html.matches(r#"id="mailbox-password-input""#).count(),
            1,
            "password INPUT must have unique id mailbox-password-input"
        );
        assert!(
            html.contains(r#"id="mailbox-password-input""#) && html.contains(r#"name="password""#),
            "password INPUT id mailbox-password-input must exist"
        );

        assert_eq!(
            html.matches(r#"id="mailbox-password-confirm""#).count(),
            1,
            "confirm field id must be unique"
        );

        let reads_input = html.contains("getElementById('mailbox-password-input')")
            || html.contains(r#"getElementById("mailbox-password-input")"#);
        assert!(
            reads_input,
            "form script must read the password INPUT id, not the card DIV"
        );
        assert!(
            !html.contains("getElementById('mailbox-password')"),
            "script must not getElementById the card id for the password value"
        );
        assert!(
            html.contains("Passwords do not match.")
                && (html.contains("Password required.") || html.contains("password required")),
            "client must keep mismatch and empty-password checks"
        );
        assert!(
            !html.contains(r#"action="/api/v1/accounts/password""#),
            "native submit must not POST form-urlencoded at the JSON API"
        );
    }

    /// Named contract: the mail form embeds a CSRF token the script can read
    /// without `document.cookie` (Brave, missing cookie, older sessions).
    #[test]
    fn mail_page_embeds_csrf_and_script_reads_embedded_token() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::Administrator,
        );
        assert!(
            html.contains(r#"name="csrf""#) || html.contains("name='csrf'"),
            "mail form must embed a hidden csrf field; snippet: {}",
            html.chars().take(500).collect::<String>()
        );
        assert!(
            html.contains(r#"id="mailbox-csrf""#),
            "hidden CSRF input must use id mailbox-csrf"
        );
        assert!(
            html.contains(r#"value="unit-test-csrf-token""#)
                || html.contains("value='unit-test-csrf-token'"),
            "embedded CSRF value must match the token passed to render"
        );
        let reads_embedded = html.contains("getElementById('mailbox-csrf')")
            || html.contains(r#"getElementById("mailbox-csrf")"#)
            || html.contains(r#"querySelector('[name="csrf"]')"#)
            || html.contains(r#"querySelector("[name='csrf']")"#)
            || html.contains("meta[name=\"csrf-token\"]")
            || html.contains("meta[name='csrf-token']");
        assert!(
            reads_embedded,
            "form script must read the embedded CSRF token, not only document.cookie"
        );
        assert!(
            html.contains("document.cookie"),
            "cookie fallback may remain, but must not be the only CSRF source"
        );
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
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "",
            &MailConsoleView::Administrator,
        );
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

    /// Named contract: /mail Create mailbox card (Administrator) uses unique ids.
    #[test]
    fn mail_page_renders_create_mailbox_form() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::Administrator,
        );
        assert!(
            html.contains("Create mailbox"),
            "mail page must title the create card; snippet: {}",
            html.chars().take(800).collect::<String>()
        );
        assert!(
            html.contains(r#"id="mailbox-create-form""#),
            "create form id mailbox-create-form"
        );
        assert!(
            html.contains(r#"id="mailbox-create-local""#)
                && !html.contains(r#"id="mailbox-create-local" type="email""#),
            "local-part must not be type=email"
        );
        assert!(
            html.contains("mailbox-create-preview") && html.contains("@"),
            "live preview of local@primary_domain"
        );
        assert!(
            html.contains(r#"id="mailbox-create-password""#)
                && html.contains(r#"id="mailbox-create-confirm""#),
            "create password + confirm unique ids"
        );
        assert!(
            html.contains("Password (optional)")
                || html.contains("optional password")
                || html.contains("Optional password"),
            "create copy must say password is optional; snippet: {}",
            html.chars().take(1200).collect::<String>()
        );
        assert!(
            html.contains("var payload = { name: local, role: role, csrf: csrf }")
                && html.contains("if (pw) { payload.password = pw; payload.confirm = cf; }"),
            "create script must omit password when empty so grant-then-self-set works"
        );
        assert!(
            !html.contains("var payload = { name: local, password: pw, confirm: cf"),
            "create script must not always send password"
        );
        let create_script_start = html
            .find("id=\"mailbox-create-form\"")
            .expect("create form");
        let create_script_end = html[create_script_start..]
            .find("id=\"mailbox-grant-form\"")
            .map(|i| create_script_start + i)
            .unwrap_or(html.len());
        let create_chunk = &html[create_script_start..create_script_end];
        assert!(
            !create_chunk.contains("Password required."),
            "create JS must not require a password; they set it after login"
        );
        assert!(
            html.contains(r#"id="mailbox-create-npub""#),
            "optional npub field"
        );
        assert!(
            html.contains(r#"id="mailbox-create-role""#)
                && html.contains(r#"value="user""#)
                && (html.contains("selected") || html.contains("User")),
            "role select defaults to User"
        );
        assert_eq!(
            html.matches(r#"id="mailbox-password""#).count(),
            1,
            "create card must not reuse #mailbox-password as an input"
        );
        assert!(
            html.contains("/api/v1/accounts")
                && (html.contains("getElementById('mailbox-create-csrf')")
                    || html.contains(r#"getElementById("mailbox-create-csrf")"#)),
            "create script posts with embedded CSRF"
        );
        assert!(
            html.contains("Grant console login") && html.contains(r#"id="mailbox-grant-form""#),
            "Administrator also sees grant-console card"
        );
        assert!(
            html.contains("npub1")
                && (html.contains("placeholder=\"npub1")
                    || html.contains("placeholder='npub1")
                    || html.contains("npub1...")),
            "grant form must ask for bech32 npub1"
        );
        assert_doge_palette_only(&html);
    }

    /// Named contract: User view hides create and grant; password card remains.
    #[test]
    fn mail_page_user_view_hides_create_and_grant() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::User {
                mailbox: Some("person@example.test".into()),
            },
        );
        assert!(
            !html.contains(r#"id="mailbox-create-form""#),
            "User must not see create form"
        );
        assert!(
            !html.contains(r#"id="mailbox-grant-form""#),
            "User must not see grant-console form"
        );
        assert!(
            html.contains("Set mailbox password"),
            "User still sets their own mailbox password"
        );
        assert!(
            html.contains("person@example.test"),
            "User password field prefills their bound mailbox"
        );
        assert!(
            !html.contains("You are the operator"),
            "User chrome must not say the viewer is the operator"
        );
        assert!(
            html.contains("person@example.test")
                && html.contains("Mail app settings")
                && html.contains("full address"),
            "Evolution help must use the bound mailbox, not a hardcoded hunter address"
        );
        let help_idx = html.find("Mail app settings").unwrap_or(0);
        let help = html.get(help_idx..).unwrap_or("");
        assert!(
            !help.contains("hunter@surmount.systems"),
            "User Evolution help must not hardcode hunter"
        );
    }

    /// Named contract: Administrator /mail two-role story + mailbox roster.
    ///
    /// Boss attaches npub. Contributors set their own IMAP password after
    /// login. Roster uses the primary address (aliases are not extra people).
    #[test]
    fn mail_page_administrator_two_role_story_and_roster() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let extras = MailPageExtras {
            roster: vec![MailboxRosterRow {
                primary: "person@example.test".into(),
                aliases: vec!["alias@example.test".into()],
                npub_attached: true,
                password_set: false,
                role: "user".into(),
            }],
            nwc_connected: false,
        };
        let html = render_mail_page_with(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::Administrator,
            &extras,
        );
        let lower = html.to_ascii_lowercase();
        assert!(
            html.contains("You attach npub")
                || html.contains("you attach npub")
                || html.contains("attach their npub"),
            "boss copy must say you attach npub; snippet: {}",
            html.chars().take(900).collect::<String>()
        );
        assert!(
            html.contains("person@example.test") && html.contains("alias@example.test"),
            "roster must list primary and aliases, not as extra people"
        );
        assert!(
            (html.contains("they set") || html.contains("They set")) && lower.contains("password"),
            "boss copy must say they set the mailbox password"
        );
        assert!(
            html.contains("Mailbox roster") || html.contains("id=\"mailbox-roster\""),
            "Administrator /mail must show a mailbox roster"
        );
        assert!(
            html.contains("Primary")
                && html.contains("Aliases")
                && (html.contains("npub") || html.contains("Portal login"))
                && (html.contains("Password") || html.contains("IMAP password"))
                && html.contains("Role"),
            "roster columns: primary, aliases, npub, password yes/no, role"
        );
        assert!(
            !html.contains("you are not the admin") && !html.contains("You are not the admin"),
            "never tell the operator they are not the admin"
        );
        assert_doge_palette_only(&html);
    }

    /// Named contract: User /mail after grant is self-serve password, not create.
    #[test]
    fn mail_page_user_self_serve_password_and_evolution() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::User {
                mailbox: Some("person@example.test".into()),
            },
        );
        assert!(
            !html.contains(r#"id="mailbox-create-form""#),
            "User must not see the Administrator create-mailbox form"
        );
        assert!(
            !html.contains(r#"id="mailbox-grant-form""#),
            "User must not see Grant console login"
        );
        assert!(
            html.contains("Your mailbox") || html.contains("your mailbox password"),
            "User first-run is their mailbox, not the admin wall"
        );
        assert!(
            html.contains("Set mailbox password")
                || html.contains("Set or change")
                || html.contains("IMAP password"),
            "User still has the self-serve password card"
        );
        assert!(
            html.contains("Evolution"),
            "mail app help must name Evolution"
        );
        assert!(
            html.contains("mail.example.test:993") || html.contains(":993"),
            "must show IMAPS 993"
        );
        assert!(html.contains(":465"), "must show SMTPS 465");
        assert!(
            html.to_ascii_lowercase().contains("let's encrypt")
                || html.to_ascii_lowercase().contains("lets encrypt"),
            "must say TLS is Let's Encrypt"
        );
        assert!(
            !html.contains("You are the operator"),
            "User chrome must not say the viewer is the operator"
        );
        assert_doge_palette_only(&html);
    }

    /// Named contract: User /mail has an NWC wallet card (not login, not IMAP).
    #[test]
    fn mail_page_user_nwc_card_not_login() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::User {
                mailbox: Some("person@example.test".into()),
            },
        );
        assert!(
            html.contains("id=\"mailbox-nwc-form\"") || html.contains("Nostr Wallet Connect"),
            "User /mail must offer an NWC card"
        );
        assert!(
            html.contains("nostr+walletconnect://") || html.contains("nostr+walletconnect"),
            "NWC field must mention the walletconnect URI scheme"
        );
        let lower = html.to_ascii_lowercase();
        assert!(
            lower.contains("not login") || lower.contains("not your mailbox password"),
            "NWC copy must distinguish wallet from login and IMAP password"
        );
        assert!(
            !html.contains("nsec1"),
            "page must not ship an nsec example"
        );
    }

    /// Named contract: leftover / unbound User view does not prefill hunter.
    #[test]
    fn mail_page_unbound_user_does_not_prefill_hunter() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::User { mailbox: None },
        );
        let needle = r#"id="mailbox-address""#;
        let idx = html.find(needle).expect("password mailbox input");
        let window = html.get(idx..).and_then(|s| s.get(..400)).unwrap_or("");
        assert!(
            !window.contains("hunter@surmount.systems"),
            "unbound User must not prefill hunter; window={window}"
        );
    }

    /// Named contract: Administrator /mail leads with IMAP password (dad test).
    ///
    /// This page sets the password Evolution and iPhone Mail use. Not Nostr.
    /// Reachability, roster, and endpoints must not lead the page.
    #[test]
    fn mail_page_administrator_leads_with_imap_password_dad_ux() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::Administrator,
        );
        let pw = html
            .find(r#"id="mailbox-password""#)
            .expect("password card id=mailbox-password");
        let reach = html.find("Reachability").expect("Reachability");
        let roster = html.find("Mailbox roster").expect("Mailbox roster");
        let endpoints = html.find("Endpoints").expect("Endpoints");
        assert!(pw < reach, "password card must appear before Reachability");
        assert!(
            pw < roster,
            "password card must appear before Mailbox roster"
        );
        assert!(pw < endpoints, "password card must appear before Endpoints");

        let lower = html.to_ascii_lowercase();
        assert!(
            lower.contains("evolution") && lower.contains("iphone") && lower.contains("not nostr"),
            "dad lede must say this page sets the Evolution and iPhone Mail password, \
             not Nostr; snippet: {}",
            html.chars().take(900).collect::<String>()
        );
        assert!(
            html.contains("Password for Evolution and iPhone Mail"),
            "first card title must name Evolution and iPhone Mail"
        );
        assert!(
            !html.contains("MUA") && !html.contains("MUAs"),
            "must not say MUA"
        );
        assert!(
            !html.contains("No portal rows yet."),
            "empty roster must not say No portal rows yet"
        );
        assert!(
            html.contains("People who can sign in to this website")
                && html.contains("Nostr")
                && html.contains("IMAP passwords are the card above"),
            "empty roster must say people who can sign in (Nostr) will show here; \
             IMAP passwords are the card above"
        );

        let needle = r#"id="mailbox-address""#;
        let idx = html.find(needle).expect("password mailbox input");
        let window = html.get(idx..).and_then(|s| s.get(..500)).unwrap_or("");
        assert!(
            !window.contains("hunter@surmount.systems"),
            "Administrator must not prefill a living mailbox; window={window}"
        );
        assert!(
            window.contains("placeholder") && window.to_ascii_lowercase().contains("full email"),
            "address placeholder must say full email; window={window}"
        );

        let settings = html
            .find("Mail app settings")
            .expect("Mail app settings under the password form");
        assert!(
            settings > pw,
            "Mail app settings must sit with the password card"
        );
        let settings_chunk = html.get(settings..).unwrap_or("");
        let settings_lower = settings_chunk.to_ascii_lowercase();
        assert!(
            settings_chunk.contains("993")
                && settings_lower.contains("ssl")
                && settings_chunk.contains("465"),
            "dad IMAP/SMTP lines must name hostname ports 993 and 465 SSL"
        );
        assert!(
            settings_lower.contains("not starttls") && settings_lower.contains("not oauth"),
            "dad settings must say not STARTTLS and not OAuth"
        );

        let create = html
            .find(r#"id="mailbox-create""#)
            .expect("create mailbox card");
        assert!(
            pw < create,
            "create mailbox must come after the password card"
        );
        assert!(
            html.contains("Make a new mailbox"),
            "create copy must say Make a new mailbox, not engine jargon"
        );
        assert!(
            html.contains("<details") && (html.contains("Technical") || html.contains("For later")),
            "Reachability/Endpoints/JMAP must sit in Technical / For later details"
        );
        assert!(
            html.contains(r#"id="mailbox-password-form""#)
                && html.contains(r#"id="mailbox-csrf""#)
                && html.contains(r#"name="csrf""#)
                && html.contains(r#"id="mailbox-password-input""#)
                && html.contains(r#"id="mailbox-password-confirm""#),
            "CSRF and existing POST field ids must stay"
        );
    }

    /// Named contract: User /mail also leads with IMAP password and dad words.
    #[test]
    fn mail_page_user_leads_with_imap_password_dad_ux() {
        let status = StalwartStatus {
            reachable: true,
            status: Some(200),
            error: None,
            url: "http://127.0.0.1:8080".into(),
        };
        let html = render_mail_page(
            &sample_data("/mail"),
            &status,
            "",
            "unit-test-csrf-token",
            &MailConsoleView::User {
                mailbox: Some("person@example.test".into()),
            },
        );
        let pw = html
            .find(r#"id="mailbox-password""#)
            .expect("password card");
        let reach = html.find("Reachability").expect("Reachability");
        let endpoints = html.find("Endpoints").expect("Endpoints");
        assert!(
            pw < reach,
            "User password card must appear before Reachability"
        );
        assert!(
            pw < endpoints,
            "User password card must appear before Endpoints"
        );
        let lower = html.to_ascii_lowercase();
        assert!(
            lower.contains("evolution") && lower.contains("iphone") && lower.contains("not nostr"),
            "User dad lede must name Evolution and iPhone Mail, not Nostr"
        );
        assert!(
            !html.contains("MUA") && !html.contains("MUAs"),
            "User /mail must not say MUA"
        );
        assert!(
            html.contains("Mail app settings")
                && lower.contains("not starttls")
                && lower.contains("not oauth"),
            "User mail app settings in dad words: not STARTTLS, not OAuth"
        );
    }

    /// Named contract: User session with next=/ lands on /mail.
    #[test]
    fn login_page_user_role_lands_on_mail() {
        let mut data = sample_data("/login");
        data.auth_mode = "nostr".into();
        let html = render_login_page(&data, "services.example.test", "");
        let decoded = html
            .replace("&#39;", "'")
            .replace("&quot;", "\"")
            .replace("&amp;", "&");
        assert!(
            decoded.contains("body.role === 'user'") && decoded.contains("next = '/mail'"),
            "User login with next=/ must land on /mail"
        );
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
                "",
                "",
                &MailConsoleView::Administrator,
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
