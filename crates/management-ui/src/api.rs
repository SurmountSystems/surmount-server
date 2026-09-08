//! JSON API routes (health, domain/account inventory, JMAP placeholder, Stalwart probe).
//!
//! Inventory helpers are shared with SSR pages so HTML and JSON stay consistent.
//! Accounts never invent live mailboxes without a real directory connection.

use std::collections::BTreeMap;
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
///
/// Union, dedupe, stable-sort: primary + mail + services, derived
/// `www.{primary}` and `mta-sts.{primary}`, extra mail hostnames, then
/// every `static_vhosts` key. First role wins when a name appears twice.
pub fn domains_inventory(config: &AppConfig) -> DomainsInventory {
    let mut by_name: BTreeMap<String, String> = BTreeMap::new();
    let mut insert = |name: &str, role: &str| {
        let host = name.trim().to_ascii_lowercase();
        if host.is_empty() {
            return;
        }
        by_name.entry(host).or_insert_with(|| role.to_string());
    };
    insert(&config.primary_domain, "primary");
    insert(&config.mail_hostname, "mail_hostname");
    insert(&config.services_hostname, "services_hostname");
    insert(&format!("www.{}", config.primary_domain), "www");
    insert(&format!("mta-sts.{}", config.primary_domain), "mta_sts");
    for host in &config.extra_mail_hostnames {
        insert(host, "extra_mail_hostname");
    }
    for host in config.static_vhosts.keys() {
        insert(host, "static_vhost");
    }
    let domains = by_name
        .into_iter()
        .map(|(name, role)| DomainEntry { name, role })
        .collect();
    DomainsInventory {
        domains,
        source: "config",
        note: "Inventory from deploy/config (SURMOUNT_* hostnames, static vhosts, \
extra mail hostnames), not Stalwart directory. Wire directory API when available."
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

/// JSON body for `POST /api/v1/accounts` (portal create + optional password/npub).
///
/// Client `domain_id` is ignored. Server looks up Domain id from the primary
/// domain. Password/confirm/npub/role are optional so lab create stays lean.
#[derive(Deserialize)]
pub struct CreateAccountBody {
    pub name: String,
    /// Ignored when present. Kept so older clients still deserialize.
    #[serde(default)]
    pub domain_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub confirm: Option<String>,
    /// Optional npub (bech32 or hex). Empty means IMAP/SMTP only.
    #[serde(default)]
    pub npub: Option<String>,
    /// Console role: `user` (default) or `administrator`. Not a Stalwart role.
    #[serde(default)]
    pub role: Option<String>,
    /// Session-bound CSRF when using a session cookie (header preferred).
    #[serde(default)]
    pub csrf: Option<String>,
}

impl std::fmt::Debug for CreateAccountBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateAccountBody")
            .field("name", &self.name)
            .field("domain_id", &self.domain_id.as_ref().map(|_| "[ignored]"))
            .field("description", &self.description)
            .field("password", &self.password.as_ref().map(|_| "[redacted]"))
            .field("confirm", &self.confirm.as_ref().map(|_| "[redacted]"))
            .field("npub", &self.npub.as_ref().map(|_| "[present]"))
            .field("role", &self.role)
            .field("csrf", &self.csrf.as_ref().map(|_| "[present]"))
            .finish()
    }
}

/// JSON body for `POST /api/v1/accounts/console` (attach/clear npub + role).
#[derive(Deserialize)]
pub struct GrantConsoleBody {
    pub mailbox: String,
    #[serde(default)]
    pub npub: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub csrf: Option<String>,
}

impl std::fmt::Debug for GrantConsoleBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GrantConsoleBody")
            .field("mailbox", &self.mailbox)
            .field("npub", &self.npub.as_ref().map(|_| "[present]"))
            .field("role", &self.role)
            .field("csrf", &self.csrf.as_ref().map(|_| "[present]"))
            .finish()
    }
}

/// JSON body for `PATCH /api/v1/accounts/{id}` (description patch; optional csrf).
#[derive(Debug, Deserialize)]
pub struct UpdateAccountBody {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub csrf: Option<String>,
}

/// JSON body for `POST /api/v1/accounts/password` (mailbox password set).
///
/// Password fields are never written back into the JSON response.
#[derive(Deserialize)]
pub struct SetMailboxPasswordBody {
    pub mailbox: String,
    pub password: String,
    pub confirm: String,
    #[serde(default)]
    pub csrf: Option<String>,
}

impl std::fmt::Debug for SetMailboxPasswordBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetMailboxPasswordBody")
            .field("mailbox", &self.mailbox)
            .field("password", &"[redacted]")
            .field("confirm", &"[redacted]")
            .field("csrf", &self.csrf.as_ref().map(|_| "[present]"))
            .finish()
    }
}

/// JSON body for `POST /api/v1/accounts/nwc` (save or clear NWC URI).
///
/// Empty `uri` clears the stored connection. The URI is never written into
/// the JSON response.
#[derive(Deserialize)]
pub struct SetNwcBody {
    pub mailbox: String,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub csrf: Option<String>,
}

impl std::fmt::Debug for SetNwcBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetNwcBody")
            .field("mailbox", &self.mailbox)
            .field("uri", &self.uri.as_ref().map(|_| "[redacted]"))
            .field("csrf", &self.csrf.as_ref().map(|_| "[present]"))
            .finish()
    }
}

/// Create principal via directory strategy. Handler enforces auth coupling + CSRF
/// at the route layer in `main`. Administrator only unless lab auth-off escape.
pub async fn create_account_via_directory(
    state: &AppState,
    body: CreateAccountBody,
    principal: Option<&surmount_management_ui::console_accounts::RequestPrincipal>,
) -> (StatusCode, Json<Value>) {
    use crate::directory::{
        CreateAccountInput, SetMailboxPasswordInput, normalize_mailbox_local_part,
        require_mutation_auth_coupling,
    };
    use surmount_management_ui::auth::normalize_pubkey_token;
    use surmount_management_ui::console_accounts::{
        ConsoleRole, load_console_accounts, update_console_accounts,
    };

    if let Err(msg) = require_mutation_auth_coupling(
        state.config.auth.mode,
        state.config.allow_directory_unauthenticated,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": msg})),
        );
    }
    if state.config.auth.mode.is_nostr() && !principal.is_some_and(|p| p.is_administrator()) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "ok": false,
                "error": "Administrator role required to create a mailbox."
            })),
        );
    }

    let local = match normalize_mailbox_local_part(&body.name) {
        Ok(n) => n,
        Err(msg) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": msg})),
            );
        }
    };
    let address = format!("{}@{}", local, state.config.primary_domain);

    let password = body
        .password
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let confirm = body
        .confirm
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (
        password,
        confirm,
        body.password.is_some(),
        body.confirm.is_some(),
    ) {
        (Some(p), Some(c), _, _) if p != c => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "Passwords do not match."})),
            );
        }
        (None, _, true, _) | (_, None, _, true) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": "Password required."})),
            );
        }
        _ => {}
    }

    let npub_hex = match body
        .npub
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match normalize_pubkey_token(raw) {
            Ok(hex) => Some(hex),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok": false, "error": format!("Invalid npub: {e}")})),
                );
            }
        },
        None => None,
    };
    let role = match ConsoleRole::parse(body.role.as_deref().unwrap_or("")) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": e})),
            );
        }
    };

    if let Some(hex) = npub_hex.as_deref() {
        match load_console_accounts(&state.config.console_accounts_path) {
            Ok(map) if map.has_npub(hex) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "ok": false,
                        "error": "npub already bound to another mailbox"
                    })),
                );
            }
            Ok(_) => {}
            // Unreadable / refused map: still create the mailbox. The write
            // below fails closed (console_saved: false). Do not abort here.
            Err(_) => {}
        }
    }

    let domain = state
        .directory
        .lookup_domain_id(&state.config.primary_domain)
        .await;
    if !domain.ok {
        let status = if domain.source == "unavailable" {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::BAD_REQUEST
        };
        return (
            status,
            Json(json!({
                "ok": false,
                "source": domain.source,
                "error": domain.note,
            })),
        );
    }
    let domain_id = domain.domain_id.unwrap_or_default();

    let result = state
        .directory
        .create_account(CreateAccountInput {
            name: local.clone(),
            domain_id,
            description: body.description,
        })
        .await;
    if !result.ok {
        return mutation_result_to_response(result);
    }

    let mut password_set = false;
    let mut password_note = None;
    if let Some(secret) = password {
        let lookup_addr = result
            .account
            .as_ref()
            .map(|a| a.address.clone())
            .unwrap_or_else(|| address.clone());
        let pw = state
            .directory
            .set_mailbox_password(SetMailboxPasswordInput {
                mailbox: lookup_addr,
                password: secret.to_string(),
            })
            .await;
        password_set = pw.ok;
        if !pw.ok {
            password_note = Some(pw.note);
        }
    }

    let mut console_saved = true;
    let mut console_note = None;
    let map_err = update_console_accounts(&state.config.console_accounts_path, |file| {
        file.upsert_mailbox(&address, npub_hex.clone(), role)
    })
    .err();
    if let Some(e) = map_err {
        console_saved = false;
        console_note = Some(format!("Mailbox exists; console login was not saved. {e}"));
    }

    let mut note = if password.is_some() && !password_set {
        password_note.unwrap_or_else(|| {
            "Mailbox exists; password was not set. Finish on the password card.".into()
        })
    } else if !console_saved {
        console_note.unwrap_or_else(|| "Mailbox exists; console login was not saved.".into())
    } else if password_set {
        format!("Created {address} and set the password. Evolution User Name is the full address.")
    } else {
        result.note
    };
    if password.is_some() && !password_set && !note.to_ascii_lowercase().contains("mailbox exists")
    {
        note = format!("Mailbox exists; password was not set. Finish on the password card. {note}");
    }
    if npub_hex.is_none() && console_saved && password_set {
        note.push_str(" No npub: IMAP/SMTP only.");
    }

    let mut body = json!({
        "ok": true,
        "created": true,
        "password_set": password_set,
        "console_saved": console_saved,
        "address": address,
        "source": result.source,
        "note": note,
    });
    if let Some(acc) = result.account {
        body["account"] = json!(acc);
    }
    if !console_saved || (password.is_some() && !password_set) {
        body["ok"] = json!(false);
        body["error"] = json!(note);
    }
    (StatusCode::OK, Json(body))
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

/// Attach or clear a console npub and set role on an existing mailbox.
pub async fn grant_console_via_directory(
    state: &AppState,
    body: GrantConsoleBody,
    principal: Option<&surmount_management_ui::console_accounts::RequestPrincipal>,
) -> (StatusCode, Json<Value>) {
    use crate::directory::require_mutation_auth_coupling;
    use surmount_management_ui::auth::normalize_pubkey_token;
    use surmount_management_ui::console_accounts::{
        ConsoleRole, load_console_accounts, update_console_accounts,
    };

    if let Err(msg) = require_mutation_auth_coupling(
        state.config.auth.mode,
        state.config.allow_directory_unauthenticated,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": msg})),
        );
    }
    if state.config.auth.mode.is_nostr() && !principal.is_some_and(|p| p.is_administrator()) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "ok": false,
                "error": "Administrator role required to grant console login."
            })),
        );
    }

    let mailbox = body.mailbox.trim().to_string();
    if mailbox.is_empty() || !mailbox.contains('@') {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "Mailbox address required (user@domain)."
            })),
        );
    }

    // Validate npub before directory lookup so garbage is 400 even when
    // listing is unavailable. Portal login is the console map, not Stalwart.
    let npub_hex = match body
        .npub
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => match normalize_pubkey_token(raw) {
            Ok(hex) => Some(hex),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"ok": false, "error": format!("Invalid npub: {e}")})),
                );
            }
        },
        None => None,
    };
    let role = match ConsoleRole::parse(body.role.as_deref().unwrap_or("")) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": e})),
            );
        }
    };

    let looked = state.directory.lookup_mailbox(&mailbox).await;
    let map = match load_console_accounts(&state.config.console_accounts_path) {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": e})),
            );
        }
    };
    let address = if looked.ok {
        looked
            .account
            .as_ref()
            .map(|a| a.address.clone())
            .unwrap_or_else(|| mailbox.to_ascii_lowercase())
    } else if map.by_mailbox(&mailbox).is_some() {
        // IMAP-only map row (for example hunter) without a live directory hit.
        mailbox.to_ascii_lowercase()
    } else if looked.source == "unavailable" {
        // Live default: directory listing off. Operator names an existing
        // mailbox; bind writes the map so that npub can log into the portal.
        mailbox.to_ascii_lowercase()
    } else {
        let status = StatusCode::BAD_REQUEST;
        return (
            status,
            Json(json!({
                "ok": false,
                "source": looked.source,
                "error": looked.note,
            })),
        );
    };
    if let Some(hex) = npub_hex.as_deref() {
        if let Some(existing) = map.by_npub(hex)
            && existing.mailbox_normalized().as_deref() != Some(address.as_str())
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "ok": false,
                    "error": "npub already bound to another mailbox"
                })),
            );
        }
    }
    if let Err(e) = update_console_accounts(&state.config.console_accounts_path, |file| {
        file.upsert_mailbox(&address, npub_hex.clone(), role)
    }) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": e})),
        );
    }
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "mailbox": address,
            "role": role.as_str(),
            "console_login": npub_hex.is_some(),
            "note": if npub_hex.is_some() {
                "Console login saved for this mailbox."
            } else {
                "Console npub cleared. Mailbox is IMAP/SMTP only."
            },
        })),
    )
}

/// Set mailbox password via directory (auth + CSRF enforced in the route layer).
///
/// Rejects confirm mismatch before talking to Stalwart. Response never includes
/// the password or confirm values. Console User may set only their bound address.
pub async fn set_mailbox_password_via_directory(
    state: &AppState,
    body: SetMailboxPasswordBody,
    principal: Option<&surmount_management_ui::console_accounts::RequestPrincipal>,
) -> (StatusCode, Json<Value>) {
    use crate::directory::{SetMailboxPasswordInput, require_mutation_auth_coupling};
    use surmount_management_ui::console_accounts::ConsoleRole;

    if let Err(msg) = require_mutation_auth_coupling(
        state.config.auth.mode,
        state.config.allow_directory_unauthenticated,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": msg})),
        );
    }
    // Nostr with no resolved principal (NIP-98 URL miss, leftover none)
    // is 403 before any directory call. Lab auth-off stays coupling-only.
    if state.config.auth.mode.is_nostr() && principal.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "ok": false,
                "error": "No console role for this session."
            })),
        );
    }

    let mailbox = body.mailbox.trim().to_string();
    if mailbox.is_empty() || !mailbox.contains('@') {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "Mailbox address required (user@domain)."
            })),
        );
    }
    // Administrator: any mailbox. User: bound mailbox only. Role None
    // (leftover cookie, no map row, not on the live allowlist) is 403.
    if let Some(p) = principal {
        match p.role {
            Some(ConsoleRole::Administrator) => {}
            Some(ConsoleRole::User) => {
                let allowed = p
                    .bound_mailbox()
                    .is_some_and(|own| own.eq_ignore_ascii_case(&mailbox));
                if !allowed {
                    return (
                        StatusCode::FORBIDDEN,
                        Json(json!({
                            "ok": false,
                            "error": "Users may set only their own mailbox password."
                        })),
                    );
                }
            }
            None => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "ok": false,
                        "error": "No console role for this session."
                    })),
                );
            }
        }
    }
    if body.password != body.confirm {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "Passwords do not match."
            })),
        );
    }
    if body.password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "Password required."
            })),
        );
    }

    let result = state
        .directory
        .set_mailbox_password(SetMailboxPasswordInput {
            mailbox,
            password: body.password,
        })
        .await;
    if result.ok
        && let Some(addr) = result.mailbox.as_deref()
    {
        let _ = surmount_management_ui::console_accounts::update_console_accounts(
            &state.config.console_accounts_path,
            |file| {
                file.mark_password_set(addr);
                Ok(())
            },
        );
    }
    password_result_to_response(result)
}

/// Save or clear a contributor NWC URI (wallet, not login).
pub async fn set_nwc_via_directory(
    state: &AppState,
    body: SetNwcBody,
    principal: Option<&surmount_management_ui::console_accounts::RequestPrincipal>,
) -> (StatusCode, Json<Value>) {
    use crate::directory::require_mutation_auth_coupling;
    use surmount_management_ui::console_accounts::ConsoleRole;
    use surmount_management_ui::nwc::{clear_nwc_uri, nwc_connected, save_nwc_uri};

    if let Err(msg) = require_mutation_auth_coupling(
        state.config.auth.mode,
        state.config.allow_directory_unauthenticated,
    ) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok": false, "error": msg})),
        );
    }
    if state.config.auth.mode.is_nostr() && principal.is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "ok": false,
                "error": "No console role for this session."
            })),
        );
    }

    let mailbox = body.mailbox.trim().to_string();
    if mailbox.is_empty() || !mailbox.contains('@') {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": "Mailbox address required (user@domain)."
            })),
        );
    }
    if let Some(p) = principal {
        match p.role {
            Some(ConsoleRole::Administrator) => {}
            Some(ConsoleRole::User) => {
                let allowed = p
                    .bound_mailbox()
                    .is_some_and(|own| own.eq_ignore_ascii_case(&mailbox));
                if !allowed {
                    return (
                        StatusCode::FORBIDDEN,
                        Json(json!({
                            "ok": false,
                            "error": "Users may set only their own wallet connection."
                        })),
                    );
                }
            }
            None => {
                return (
                    StatusCode::FORBIDDEN,
                    Json(json!({
                        "ok": false,
                        "error": "No console role for this session."
                    })),
                );
            }
        }
    }

    let uri = body.uri.as_deref().map(str::trim).unwrap_or("");
    if uri.is_empty() {
        if let Err(e) = clear_nwc_uri(&state.config.nwc_store_path, &mailbox) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"ok": false, "error": e})),
            );
        }
        return (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "mailbox": mailbox,
                "connected": false,
                "note": "Wallet connection cleared."
            })),
        );
    }
    if let Err(e) = save_nwc_uri(&state.config.nwc_store_path, &mailbox, uri) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"ok": false, "error": e})),
        );
    }
    let connected = nwc_connected(&state.config.nwc_store_path, &mailbox).unwrap_or(true);
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "mailbox": mailbox,
            "connected": connected,
            "note": "Wallet connected. Payments use this NWC wallet."
        })),
    )
}

fn password_result_to_response(
    result: crate::directory::MailboxPasswordResult,
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
    if let Some(mailbox) = result.mailbox {
        body["mailbox"] = json!(mailbox);
    }
    if !result.ok {
        body["error"] = json!(result.note);
    }
    (status, Json(body))
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

/// Thin system status for operators (JSON). Includes onion surface status and
/// Vaultwarden link when configured. Never secrets / admin token / nsec.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SystemStatus {
    pub service: &'static str,
    pub version: &'static str,
    /// Operator-published onion URL (`http://….onion`) or null when not configured.
    pub onion: Option<String>,
    /// `configured` | `hostname_missing` | `not_provisioned` (host Arti path).
    pub onion_status: &'static str,
    /// Currently loaded clearnet Host -> onion discovery map (start-of-process).
    pub onion_discovery: crate::onion_discovery::OnionDiscoveryDump,
    /// True when an operator-published Vaultwarden URL is set (domain C).
    pub vaultwarden_configured: bool,
    /// Operator-published Vaultwarden URL, or null when residual not configured.
    pub vaultwarden_url: Option<String>,
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
    let vaultwarden_url = config.vaultwarden_url.clone();
    SystemStatus {
        service: "surmount-management-ui",
        version: env!("CARGO_PKG_VERSION"),
        onion: config.onion_url.clone(),
        onion_status: config.onion_surface.status_slug(),
        onion_discovery: config.onion_discovery.dump(),
        vaultwarden_configured: vaultwarden_url.is_some(),
        vaultwarden_url,
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

    fn sample_config() -> AppConfig {
        AppConfig::example_test()
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

    /// Named contract: inventory is the config union (primary trio, derived
    /// www/mta-sts, static vhost keys, extra mail hostnames), still source
    /// config, never Stalwart. A length-3 primary-only helper fails this.
    #[test]
    fn domains_inventory_is_config_source() {
        let mut cfg = sample_config();
        let root = std::path::PathBuf::from("/var/lib/surmount/static-sites/fixture");
        for host in [
            "yiffa.app",
            "www.yiffa.app",
            "baxterartworks.com",
            "www.baxterartworks.com",
            "cryptoquick.com",
            "www.cryptoquick.com",
        ] {
            cfg.static_vhosts.insert(host.into(), root.clone());
        }
        cfg.extra_mail_hostnames = vec!["mail.cryptoquick.com".into()];
        let inv = domains_inventory(&cfg);
        assert_eq!(inv.source, "config");
        assert_ne!(inv.source, "stalwart");
        let names: Vec<&str> = inv.domains.iter().map(|d| d.name.as_str()).collect();
        for must in [
            "example.test",
            "www.example.test",
            "mta-sts.example.test",
            "mail.example.test",
            "services.example.test",
            "yiffa.app",
            "baxterartworks.com",
            "cryptoquick.com",
            "www.cryptoquick.com",
            "mail.cryptoquick.com",
        ] {
            assert!(
                names.contains(&must),
                "config union must include {must}; got {names:?}"
            );
        }
        assert!(
            inv.domains.len() > 3,
            "union must not be the old three-row helper; got {} rows: {names:?}",
            inv.domains.len()
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "primary" && d.name == "example.test")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "www" && d.name == "www.example.test")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "mta_sts" && d.name == "mta-sts.example.test")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "static_vhost" && d.name == "yiffa.app")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "static_vhost" && d.name == "baxterartworks.com")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "static_vhost" && d.name == "cryptoquick.com")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| d.role == "static_vhost" && d.name == "www.cryptoquick.com")
        );
        assert!(
            inv.domains
                .iter()
                .any(|d| { d.role == "extra_mail_hostname" && d.name == "mail.cryptoquick.com" })
        );
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "inventory names must be stable-sorted");
        let unique = names.len()
            == names
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len();
        assert!(unique, "inventory names must be unique: {names:?}");
        let blob = serde_json::to_string(&inv).unwrap();
        assert!(
            !blob.contains("\"source\":\"stalwart\""),
            "must not claim Stalwart: {blob}"
        );
    }

    /// Named contract: system status exposes onion only when configured.
    #[test]
    fn system_status_onion_null_when_unset() {
        let s = system_status_from_config(&sample_config());
        assert!(s.onion.is_none());
        assert_eq!(s.onion_status, "not_provisioned");
        let blob = serde_json::to_string(&s).unwrap();
        assert!(
            blob.contains("\"onion\":null") || blob.contains("\"onion\": null"),
            "expected null onion: {blob}"
        );
        assert!(
            blob.contains("\"onion_status\":\"not_provisioned\""),
            "expected not_provisioned status: {blob}"
        );
    }

    #[test]
    fn system_status_onion_when_set() {
        let mut cfg = sample_config();
        let url = "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwx.onion";
        cfg.onion_url = Some(url.into());
        cfg.onion_surface = crate::config::OnionSurface::Configured { url: url.into() };
        cfg.onion_discovery = crate::onion_discovery::build_onion_discovery(
            &cfg.primary_domain,
            &cfg.services_hostname,
            Some(url),
            true,
            true,
            Vec::new(),
            &[],
            &[],
            &[] as &[&str],
        );
        let s = system_status_from_config(&cfg);
        assert_eq!(s.onion.as_deref(), Some(url));
        assert_eq!(s.onion_status, "configured");
        assert!(s.onion_discovery.onion_location_enabled);
        assert!(s.onion_discovery.alt_svc_enabled);
        let hosts: Vec<_> = s
            .onion_discovery
            .mappings
            .iter()
            .map(|m| m.clearnet_host.as_str())
            .collect();
        assert!(hosts.contains(&"example.test"));
        assert!(hosts.contains(&"www.example.test"));
        assert!(hosts.contains(&"services.example.test"));
        assert!(!hosts.contains(&"mail.example.test"));
        let blob = serde_json::to_string(&s).unwrap();
        assert!(
            blob.contains("\"onion_discovery\""),
            "dump must be machine-readable JSON: {blob}"
        );
    }

    /// Named contract: hostname_missing when path set but no address material.
    #[test]
    fn system_status_onion_hostname_missing() {
        let mut cfg = sample_config();
        cfg.onion_url = None;
        cfg.onion_surface = crate::config::OnionSurface::HostnameMissing {
            path: Some("/run/surmount-secrets/arti/onion-service/hostname".into()),
            hs_state_dir: Some("/run/surmount-secrets/arti/onion-service".into()),
        };
        let s = system_status_from_config(&cfg);
        assert!(s.onion.is_none());
        assert_eq!(s.onion_status, "hostname_missing");
    }

    /// Named contract: Vaultwarden residual when URL unset (no invent; no secrets).
    #[test]
    fn system_status_vaultwarden_unset() {
        let s = system_status_from_config(&sample_config());
        assert!(!s.vaultwarden_configured);
        assert!(s.vaultwarden_url.is_none());
        let blob = serde_json::to_string(&s).unwrap();
        assert!(
            blob.contains("\"vaultwarden_configured\":false")
                || blob.contains("\"vaultwarden_configured\": false"),
            "expected configured false: {blob}"
        );
        assert!(
            blob.contains("\"vaultwarden_url\":null") || blob.contains("\"vaultwarden_url\": null"),
            "expected null vaultwarden_url: {blob}"
        );
        assert!(
            !blob.to_ascii_lowercase().contains("admin_token")
                && !blob.contains("ADMIN_TOKEN")
                && !blob.contains("nsec"),
            "must not leak secrets: {blob}"
        );
    }

    /// Named contract: Vaultwarden configured marker + URL when set.
    #[test]
    fn system_status_vaultwarden_when_set() {
        let mut cfg = sample_config();
        cfg.vaultwarden_url = Some("http://127.0.0.1:8222".into());
        let s = system_status_from_config(&cfg);
        assert!(s.vaultwarden_configured);
        assert_eq!(s.vaultwarden_url.as_deref(), Some("http://127.0.0.1:8222"));
        let blob = serde_json::to_string(&s).unwrap();
        assert!(
            blob.contains("http://127.0.0.1:8222"),
            "expected vault url in json: {blob}"
        );
        assert!(
            !blob.contains("ADMIN_TOKEN") && !blob.contains("admin_token"),
            "must not include admin token: {blob}"
        );
    }
}
