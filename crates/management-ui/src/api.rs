//! JSON API routes (health, domain/account inventory, JMAP placeholder, Stalwart probe).
//!
//! Inventory helpers are shared with SSR pages so HTML and JSON stay consistent.
//! Accounts never invent live mailboxes without a real directory connection.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::AppState;
use crate::config::AppConfig;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "surmount-management-ui",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// One row in the domain inventory (config mirror until Stalwart directory is wired).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DomainEntry {
    pub name: String,
    pub role: String,
}

/// Domains inventory shared by JSON API and SSR.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DomainsInventory {
    pub domains: Vec<DomainEntry>,
    /// Where rows came from: currently always `"config"`.
    pub source: &'static str,
    pub note: String,
}

/// Build config-backed domain inventory (not a live Stalwart directory list).
pub fn domains_inventory(config: &AppConfig) -> DomainsInventory {
    DomainsInventory {
        domains: vec![
            DomainEntry {
                name: config.primary_domain.clone(),
                role: "primary".into(),
            },
            DomainEntry {
                name: config.mail_hostname.clone(),
                role: "mail_hostname".into(),
            },
            DomainEntry {
                name: config.services_hostname.clone(),
                role: "services_hostname".into(),
            },
        ],
        source: "config",
        note: "Inventory from deploy/config (SURMOUNT_* hostnames), not Stalwart directory. \
Wire directory API when available."
            .into(),
    }
}

pub async fn list_domains(State(state): State<Arc<AppState>>) -> Json<DomainsInventory> {
    Json(domains_inventory(&state.config))
}

/// Mailbox / principal inventory. Honest empty when directory is not connected.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AccountEntry {
    pub id: String,
    pub address: String,
    /// Principal kind when known (Stalwart `@type`, e.g. `user` / `group`),
    /// else a placeholder like `active` for mock. Not a full lifecycle SoT.
    /// JSON key remains `status` for wire stability; UI labels it as kind.
    pub status: String,
}

/// Accounts inventory shared by JSON API and SSR.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AccountsInventory {
    pub accounts: Vec<AccountEntry>,
    /// `"unavailable"` (default), `"mock"` (hermetic fixture), `"stalwart"` (live).
    pub source: &'static str,
    pub note: String,
}

/// Default (unavailable) accounts inventory helper for pure/unit tests.
/// Live request paths use `state.directory.list_accounts()` so hermetic mock
/// and future Stalwart backends apply.
///
/// Residual live path: Stalwart 0.16 management via JMAP/`stalwart-cli query account`.
/// See `docs/research/stalwart-directory-api.md`. Trait + mock + live client shipped;
/// live requires explicit `SURMOUNT_DIRECTORY=stalwart` + host token.
#[cfg_attr(not(test), allow(dead_code))]
pub fn accounts_inventory(_config: &AppConfig) -> AccountsInventory {
    // Sync helper for pure unit tests: unavailable path only (no runtime).
    AccountsInventory {
        accounts: Vec::new(),
        source: "unavailable",
        note: "Not connected to mail directory. No live Stalwart accounts listed. \
Create accounts via stalwart-cli or bootstrap admin; set SURMOUNT_DIRECTORY=stalwart \
with a host token to list live principals."
            .into(),
    }
}

pub async fn list_accounts(State(state): State<Arc<AppState>>) -> Json<AccountsInventory> {
    Json(state.directory.list_accounts().await)
}

/// JSON body for `POST /api/v1/accounts` (lean create; optional csrf for double-submit).
#[derive(Debug, Deserialize)]
pub struct CreateAccountBody {
    pub name: String,
    pub domain_id: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Double-submit CSRF when using session cookie (header preferred).
    #[serde(default)]
    pub csrf: Option<String>,
}

/// JSON body for `PATCH /api/v1/accounts/{id}` (description patch; optional csrf).
#[derive(Debug, Deserialize)]
pub struct UpdateAccountBody {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub csrf: Option<String>,
}

/// Create principal via directory strategy. Handler enforces auth coupling + CSRF
/// at the route layer in `main` (this module only maps directory results).
pub async fn create_account_via_directory(
    state: &AppState,
    body: CreateAccountBody,
) -> (StatusCode, Json<Value>) {
    use crate::directory::{CreateAccountInput, require_mutation_auth_coupling};

    if let Err(msg) = require_mutation_auth_coupling(
        state.config.auth.mode,
        state.config.allow_directory_unauthenticated,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": msg})),
        );
    }

    let result = state
        .directory
        .create_account(CreateAccountInput {
            name: body.name,
            domain_id: body.domain_id,
            description: body.description,
        })
        .await;
    mutation_result_to_response(result)
}

/// Update principal via directory strategy (auth + CSRF enforced in route layer).
pub async fn update_account_via_directory(
    state: &AppState,
    id: String,
    body: UpdateAccountBody,
) -> (StatusCode, Json<Value>) {
    use crate::directory::{UpdateAccountInput, require_mutation_auth_coupling};

    if let Err(msg) = require_mutation_auth_coupling(
        state.config.auth.mode,
        state.config.allow_directory_unauthenticated,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": msg})),
        );
    }

    let result = state
        .directory
        .update_account(UpdateAccountInput {
            id,
            description: body.description,
        })
        .await;
    mutation_result_to_response(result)
}

fn mutation_result_to_response(
    result: crate::directory::AccountMutationResult,
) -> (StatusCode, Json<Value>) {
    let status = if result.ok {
        StatusCode::OK
    } else if result.source == "unavailable" {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::BAD_REQUEST
    };
    let mut body = json!({
        "ok": result.ok,
        "source": result.source,
        "note": result.note,
    });
    if let Some(acc) = result.account {
        body["account"] = json!(acc);
    } else {
        body["error"] = json!(result.note);
    }
    (status, Json(body))
}

/// Thin system status for operators (JSON). Includes optional onion when configured.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SystemStatus {
    pub service: &'static str,
    pub version: &'static str,
    /// Operator-published onion URL (`http://….onion`) or null when unset.
    pub onion: Option<String>,
    pub primary_domain: String,
    pub mail_hostname: String,
    pub services_hostname: String,
    pub listen: String,
    pub stalwart_url: String,
    pub ban_enforcement: String,
}

pub fn system_status_from_config(config: &AppConfig) -> SystemStatus {
    use surmount_management_ui::ban::BanEnforcement;
    let ban = match config.ban.enforcement {
        BanEnforcement::Off => "off",
        BanEnforcement::DryRun => "dry-run",
        BanEnforcement::Enforce => "enforce",
    };
    SystemStatus {
        service: "surmount-management-ui",
        version: env!("CARGO_PKG_VERSION"),
        onion: config.onion_url.clone(),
        primary_domain: config.primary_domain.clone(),
        mail_hostname: config.mail_hostname.clone(),
        services_hostname: config.services_hostname.clone(),
        listen: config.listen.to_string(),
        stalwart_url: config.stalwart_url.clone(),
        ban_enforcement: ban.to_string(),
    }
}

pub async fn system_status(State(state): State<Arc<AppState>>) -> Json<SystemStatus> {
    Json(system_status_from_config(&state.config))
}

#[derive(Debug, Deserialize)]
pub struct JmapRequest {
    /// Opaque JSON body placeholder for future JMAP method calls.
    #[serde(flatten)]
    #[allow(dead_code)]
    pub body: Value,
}

/// Placeholder JMAP proxy. Returns 501 until auth + upstream forwarding land.
pub async fn jmap_proxy_placeholder(
    State(state): State<Arc<AppState>>,
    Json(_req): Json<JmapRequest>,
) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "error": "jmap_proxy_not_implemented",
            "message": "JMAP proxy not implemented. Point clients at Stalwart HTTP or implement proxy.",
            "stalwart_url": state.config.stalwart_url,
        })),
    )
}

/// Result of a best-effort HTTP probe against local Stalwart.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StalwartStatus {
    pub reachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub url: String,
}

/// Probe Stalwart HTTP root. Shared by JSON API and SSR pages.
/// Probe `error` text is onion-redacted so JSON and SSR tails never leak
/// full v3/legacy labels if the transport error embeds a URL.
pub async fn probe_stalwart(state: &AppState) -> StalwartStatus {
    use crate::config::redact_onion_in_text;

    let url = state.config.stalwart_url.trim_end_matches('/').to_string();
    match state.http.get(format!("{url}/")).send().await {
        Ok(resp) => StalwartStatus {
            reachable: true,
            status: Some(resp.status().as_u16()),
            error: None,
            url,
        },
        Err(err) => StalwartStatus {
            reachable: false,
            status: None,
            error: Some(redact_onion_in_text(&err.to_string())),
            url,
        },
    }
}

/// Best-effort reachability probe against local Stalwart HTTP.
pub async fn stalwart_status(State(state): State<Arc<AppState>>) -> Json<StalwartStatus> {
    Json(probe_stalwart(&state).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::directory::{Directory, MockDirectory, UnavailableDirectory};
    use crate::tls::ListenMode;
    use std::net::SocketAddr;
    use std::time::Duration;
    use surmount_management_ui::ban::{BanBackendKind, BanConfig, BanEnforcement};

    fn sample_config() -> AppConfig {
        AppConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 8090)),
            http_redirect_listen: None,
            local_cleartext_listen: None,
            listen_mode: ListenMode::PlainHttp,
            redirect_http_to_https: false,
            redirect_allowed_hosts: vec!["services.example.test".into()],
            https_allow_cleartext_escape: false,
            primary_domain: "example.test".into(),
            mail_hostname: "mail.example.test".into(),
            services_hostname: "services.example.test".into(),
            stalwart_url: "http://127.0.0.1:8080".into(),
            onion_url: None,
            rate_limit_max_requests: 0,
            rate_limit_window: Duration::from_secs(60),
            rate_limit_max_keys: 1000,
            ban: BanConfig {
                enforcement: BanEnforcement::Off,
                backend_kind: BanBackendKind::Memory,
                whitelist: vec![],
                state_path: None,
                nft_exec_enabled: false,
                nft_bin: None,
                nft_helper_sock: None,
                nft_helper_bin: None,
            },
            auth: surmount_management_ui::auth::AuthConfig::off(),
            allow_directory_unauthenticated: false,
        }
    }

    /// Named contract: accounts API does not invent fake live Stalwart mailboxes.
    #[test]
    fn accounts_inventory_is_honest_empty_without_directory() {
        let inv = accounts_inventory(&sample_config());
        assert!(
            inv.accounts.is_empty(),
            "must not invent accounts without directory; got {:?}",
            inv.accounts
        );
        assert_eq!(inv.source, "unavailable");
        let note = inv.note.to_ascii_lowercase();
        assert!(
            note.contains("not connected") || note.contains("directory"),
            "note should explain directory residual: {}",
            inv.note
        );
        let blob = serde_json::to_string(&inv).unwrap();
        assert!(
            !blob.contains("admin@"),
            "must not present fake admin@ mailbox as real: {blob}"
        );
    }

    /// Named contract: mock directory is labeled and never claims live Stalwart.
    #[tokio::test]
    async fn mock_directory_inventory_is_labeled_not_stalwart() {
        let inv = MockDirectory::fixture().list_accounts().await;
        assert_eq!(inv.source, "mock");
        assert!(!inv.accounts.is_empty());
        assert_ne!(inv.source, "stalwart");
        let blob = serde_json::to_string(&inv).unwrap();
        assert!(
            !blob.contains("\"source\":\"stalwart\""),
            "must not claim stalwart: {blob}"
        );
        // Same helper path as production default remains unavailable.
        assert_eq!(
            UnavailableDirectory.list_accounts().await.source,
            "unavailable"
        );
    }

    #[test]
    fn domains_inventory_is_config_source() {
        let inv = domains_inventory(&sample_config());
        assert_eq!(inv.source, "config");
        assert_eq!(inv.domains.len(), 3);
        assert!(inv.domains.iter().any(|d| d.role == "primary"));
        assert!(
            inv.domains
                .iter()
                .any(|d| d.name == "services.example.test")
        );
    }

    /// Named contract: system status exposes onion only when configured.
    #[test]
    fn system_status_onion_null_when_unset() {
        let s = system_status_from_config(&sample_config());
        assert!(s.onion.is_none());
        let blob = serde_json::to_string(&s).unwrap();
        assert!(
            blob.contains("\"onion\":null") || blob.contains("\"onion\": null"),
            "expected null onion: {blob}"
        );
    }

    #[test]
    fn system_status_onion_when_set() {
        let mut cfg = sample_config();
        let url = "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion";
        cfg.onion_url = Some(url.into());
        let s = system_status_from_config(&cfg);
        assert_eq!(s.onion.as_deref(), Some(url));
    }
}
