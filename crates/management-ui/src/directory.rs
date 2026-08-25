//! Account directory strategy: list and mutate mail principals without inventing rows.
//!
//! Production default is honest empty (`source: "unavailable"`). Hermetic tests
//! may inject a labeled mock (`source: "mock"`). Live Stalwart
//! (`source: "stalwart"`) is explicit opt-in via `SURMOUNT_DIRECTORY=stalwart`
//! plus a host token (env or file); never default-on; never invents accounts.
//!
//! Mutations (`create_account` / `update_account`) use management JMAP
//! `x:Account/set` on the live backend. Mailbox password set looks up the
//! principal by email and PATCHes a Password credential on that Account
//! (never `x:AccountPassword/set`, which is the API-token principal).
//! HTTP API routes gate mutations behind `AUTH_MODE=nostr` (or lab
//! `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1`) and double-submit CSRF on
//! cookie-authenticated POSTs. Password is never logged, never returned in
//! JSON, never placed on child-process argv. nsec never on server.
//! See `docs/research/stalwart-directory-api.md`.

use std::env;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use surmount_management_ui::auth::AuthMode;

use crate::api::{AccountEntry, AccountsInventory};
use crate::config::redact_onion_in_text;

/// Lean create input (no password / nsec; local-part + domain id).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateAccountInput {
    /// Stalwart account `name` (email local-part).
    pub name: String,
    /// Stalwart `domainId` (required for live; mock accepts any non-empty label).
    pub domain_id: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Lean update input (description patch only; no invented lifecycle fields).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateAccountInput {
    /// Existing principal id.
    pub id: String,
    /// Optional description set.
    #[serde(default)]
    pub description: Option<String>,
}

/// Result of create/update (never invents success rows on failure).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AccountMutationResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AccountEntry>,
    /// Same source labels as list: unavailable / mock / stalwart.
    pub source: &'static str,
    pub note: String,
}

/// Set a mailbox Password credential (lookup by email; never the API-token principal).
///
/// `password` is omitted from [`Debug`] so logs cannot echo the secret.
pub struct SetMailboxPasswordInput {
    /// Mailbox address (`hunter@surmount.systems`). Lookup key; not Account id.
    pub mailbox: String,
    /// New password. Never logged; never serialized on the response path.
    pub password: String,
}

impl std::fmt::Debug for SetMailboxPasswordInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetMailboxPasswordInput")
            .field("mailbox", &self.mailbox)
            .field("password", &"[redacted]")
            .finish()
    }
}

/// Result of a mailbox password set. Never includes the password.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MailboxPasswordResult {
    pub ok: bool,
    /// Same source labels as list: unavailable / mock / stalwart.
    pub source: &'static str,
    /// Echo of the mailbox address only (never the secret).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<String>,
    pub note: String,
}

/// Strategy for listing and mutating mailbox / principal inventory.
///
/// Sources returned in [`AccountsInventory::source`]:
/// - `"unavailable"`: default production (no directory connection)
/// - `"mock"`: hermetic fixture only (tests / explicit opt-in)
/// - `"stalwart"`: live management JMAP client (explicit config only)
///
/// Live listing/mutations are async (HTTP). Default and mock backends complete
/// immediately. Mutations on unavailable fail closed with `ok: false`.
pub trait Directory: Send + Sync {
    fn list_accounts(&self) -> Pin<Box<dyn Future<Output = AccountsInventory> + Send + '_>>;

    fn create_account(
        &self,
        input: CreateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>>;

    fn update_account(
        &self,
        input: UpdateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>>;

    fn set_mailbox_password(
        &self,
        input: SetMailboxPasswordInput,
    ) -> Pin<Box<dyn Future<Output = MailboxPasswordResult> + Send + '_>>;

    /// Resolve Stalwart Domain id for the primary domain name. Never invents an id.
    fn lookup_domain_id(
        &self,
        domain_name: &str,
    ) -> Pin<Box<dyn Future<Output = DomainLookupResult> + Send + '_>>;

    /// Lookup a mailbox principal by email (same path password set uses).
    fn lookup_mailbox(
        &self,
        address: &str,
    ) -> Pin<Box<dyn Future<Output = MailboxLookupResult> + Send + '_>>;
}

/// Result of a Domain id lookup (fail-closed; never send the hostname as id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainLookupResult {
    pub ok: bool,
    pub domain_id: Option<String>,
    pub source: &'static str,
    pub note: String,
}

/// Result of a mailbox lookup by email.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxLookupResult {
    pub ok: bool,
    pub account: Option<AccountEntry>,
    pub source: &'static str,
    pub note: String,
}

/// Local-part for a new mailbox: ASCII `[a-z0-9][a-z0-9._-]*`, no `@`.
/// Always refuses reserved `admin`.
pub fn normalize_mailbox_local_part(raw: &str) -> Result<String, String> {
    let name = raw.trim().to_ascii_lowercase();
    if name.is_empty() {
        return Err("local-part required".into());
    }
    if name.contains('@') {
        return Err("local-part must not contain @".into());
    }
    if name == "admin" {
        return Err("local-part admin is reserved".into());
    }
    let bytes = name.as_bytes();
    let first = bytes[0];
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("local-part must start with a letter or digit".into());
    }
    if !name.bytes().all(|c| {
        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_' || c == b'-'
    }) {
        return Err("local-part must be ASCII [a-z0-9][a-z0-9._-]*".into());
    }
    Ok(name)
}

/// Default production backend: empty list, residual note, no fake rows.
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableDirectory;

impl Directory for UnavailableDirectory {
    fn list_accounts(&self) -> Pin<Box<dyn Future<Output = AccountsInventory> + Send + '_>> {
        Box::pin(async {
            AccountsInventory {
                accounts: Vec::new(),
                source: "unavailable",
                note: "Not connected to mail directory. No live Stalwart accounts listed. \
Create accounts via stalwart-cli, bootstrap admin, or enable directory + auth and \
POST /api/v1/accounts; set SURMOUNT_DIRECTORY=stalwart with a host token to list \
live principals."
                    .into(),
            }
        })
    }

    fn create_account(
        &self,
        _input: CreateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        Box::pin(async {
            AccountMutationResult {
                ok: false,
                account: None,
                source: "unavailable",
                note: "Directory unavailable: create refused (fail-closed). \
Set SURMOUNT_DIRECTORY=mock or stalwart and auth gate (nostr or lab escape)."
                    .into(),
            }
        })
    }

    fn update_account(
        &self,
        _input: UpdateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        Box::pin(async {
            AccountMutationResult {
                ok: false,
                account: None,
                source: "unavailable",
                note: "Directory unavailable: update refused (fail-closed). \
Set SURMOUNT_DIRECTORY=mock or stalwart and auth gate (nostr or lab escape)."
                    .into(),
            }
        })
    }

    fn set_mailbox_password(
        &self,
        _input: SetMailboxPasswordInput,
    ) -> Pin<Box<dyn Future<Output = MailboxPasswordResult> + Send + '_>> {
        Box::pin(async {
            MailboxPasswordResult {
                ok: false,
                source: "unavailable",
                mailbox: None,
                note: "Mailbox password set refused: Stalwart management token \
is not configured on this console (fail-closed). Host needs the loopback URL \
and SURMOUNT_STALWART_TOKEN_FILE (or lab token)."
                    .into(),
            }
        })
    }

    fn lookup_domain_id(
        &self,
        _domain_name: &str,
    ) -> Pin<Box<dyn Future<Output = DomainLookupResult> + Send + '_>> {
        Box::pin(async {
            DomainLookupResult {
                ok: false,
                domain_id: None,
                source: "unavailable",
                note: "Directory unavailable: domain lookup refused (fail-closed).".into(),
            }
        })
    }

    fn lookup_mailbox(
        &self,
        _address: &str,
    ) -> Pin<Box<dyn Future<Output = MailboxLookupResult> + Send + '_>> {
        Box::pin(async {
            MailboxLookupResult {
                ok: false,
                account: None,
                source: "unavailable",
                note: "Directory unavailable: mailbox lookup refused (fail-closed).".into(),
            }
        })
    }
}

/// Hermetic mock: clearly labeled fixture principals for tests only.
///
/// Never the default from [`directory_from_env`]. Fixture addresses use a
/// mock-only domain so they cannot be mistaken for live Stalwart inventory.
/// Interior mutex allows create/update in hermetic tests.
#[derive(Debug)]
pub struct MockDirectory {
    accounts: Mutex<Vec<AccountEntry>>,
    /// Optional notes stored for update tests (id -> description).
    descriptions: Mutex<std::collections::HashMap<String, String>>,
}

impl MockDirectory {
    /// Standard hermetic fixture used by unit / HTTP tests.
    pub fn fixture() -> Self {
        Self {
            accounts: Mutex::new(vec![
                AccountEntry {
                    id: "mock-1".into(),
                    address: "fixture-operator@mock.surmount.test".into(),
                    status: "active".into(),
                },
                AccountEntry {
                    id: "mock-2".into(),
                    address: "fixture-user@mock.surmount.test".into(),
                    status: "active".into(),
                },
            ]),
            descriptions: Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Custom fixture for specialized tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn with_accounts(accounts: Vec<AccountEntry>) -> Self {
        Self {
            accounts: Mutex::new(accounts),
            descriptions: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Directory for MockDirectory {
    fn list_accounts(&self) -> Pin<Box<dyn Future<Output = AccountsInventory> + Send + '_>> {
        Box::pin(async move {
            let accounts = self
                .accounts
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            AccountsInventory {
                accounts,
                source: "mock",
                note: "Hermetic mock directory (fixture principals). Not live Stalwart. \
For tests and explicit SURMOUNT_DIRECTORY=mock only. Mutations available via API when auth allows."
                    .into(),
            }
        })
    }

    fn create_account(
        &self,
        input: CreateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        Box::pin(async move {
            let name = match normalize_mailbox_local_part(&input.name) {
                Ok(n) => n,
                Err(note) => {
                    return AccountMutationResult {
                        ok: false,
                        account: None,
                        source: "mock",
                        note: format!("create refused: {note}"),
                    };
                }
            };
            let domain_id = input.domain_id.trim().to_string();
            if domain_id.is_empty() {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "mock",
                    note: "create refused: name and domain_id required (non-empty)".into(),
                };
            }
            // Mock-only address shape so rows never look like production inventory.
            let id = format!("mock-{name}");
            let address = format!("{name}@mock.surmount.test");
            let entry = AccountEntry {
                id: id.clone(),
                address,
                status: "active".into(),
            };
            {
                let mut lock = self.accounts.lock().unwrap_or_else(|e| e.into_inner());
                if lock
                    .iter()
                    .any(|a| a.id == id || a.address == entry.address)
                {
                    return AccountMutationResult {
                        ok: false,
                        account: None,
                        source: "mock",
                        note: format!("create refused: principal already exists id={id}"),
                    };
                }
                lock.push(entry.clone());
            }
            if let Some(desc) = input.description.filter(|d| !d.trim().is_empty()) {
                self.descriptions
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id, desc);
            }
            AccountMutationResult {
                ok: true,
                account: Some(entry),
                source: "mock",
                note: "Hermetic mock create (not live Stalwart).".into(),
            }
        })
    }

    fn update_account(
        &self,
        input: UpdateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        Box::pin(async move {
            let id = input.id.trim().to_string();
            if id.is_empty() {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "mock",
                    note: "update refused: id required".into(),
                };
            }
            let entry = {
                let lock = self.accounts.lock().unwrap_or_else(|e| e.into_inner());
                lock.iter().find(|a| a.id == id).cloned()
            };
            let Some(entry) = entry else {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "mock",
                    note: format!("update refused: no principal id={id}"),
                };
            };
            // Align with live Stalwart lean surface: description patch required.
            let Some(desc) = input.description else {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "mock",
                    note: "update refused: description patch required (lean surface)".into(),
                };
            };
            self.descriptions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(id.clone(), desc);
            AccountMutationResult {
                ok: true,
                account: Some(entry),
                source: "mock",
                note: "Hermetic mock update (description patch only; not live Stalwart).".into(),
            }
        })
    }

    fn set_mailbox_password(
        &self,
        input: SetMailboxPasswordInput,
    ) -> Pin<Box<dyn Future<Output = MailboxPasswordResult> + Send + '_>> {
        Box::pin(async move {
            let mailbox = input.mailbox.trim().to_string();
            if mailbox.is_empty() || !mailbox.contains('@') {
                return MailboxPasswordResult {
                    ok: false,
                    source: "mock",
                    mailbox: None,
                    note: "password set refused: mailbox address required".into(),
                };
            }
            if input.password.is_empty() {
                return MailboxPasswordResult {
                    ok: false,
                    source: "mock",
                    mailbox: Some(mailbox),
                    note: "password set refused: password required".into(),
                };
            }
            let found = {
                let lock = self.accounts.lock().unwrap_or_else(|e| e.into_inner());
                lock.iter()
                    .find(|a| a.address.eq_ignore_ascii_case(&mailbox))
                    .cloned()
            };
            match found {
                Some(entry) => MailboxPasswordResult {
                    ok: true,
                    source: "mock",
                    mailbox: Some(entry.address),
                    note: "Hermetic mock password set (not live Stalwart). \
Use this password in your mail app."
                        .into(),
                },
                None => MailboxPasswordResult {
                    ok: false,
                    source: "mock",
                    mailbox: Some(mailbox),
                    note: "No mailbox with that address.".into(),
                },
            }
        })
    }

    fn lookup_domain_id(
        &self,
        domain_name: &str,
    ) -> Pin<Box<dyn Future<Output = DomainLookupResult> + Send + '_>> {
        let name = domain_name.trim().to_string();
        Box::pin(async move {
            if name.is_empty() {
                return DomainLookupResult {
                    ok: false,
                    domain_id: None,
                    source: "mock",
                    note: "domain lookup refused: primary domain required".into(),
                };
            }
            DomainLookupResult {
                ok: true,
                domain_id: Some("mock-domain".into()),
                source: "mock",
                note: "Hermetic mock domain id (not live Stalwart).".into(),
            }
        })
    }

    fn lookup_mailbox(
        &self,
        address: &str,
    ) -> Pin<Box<dyn Future<Output = MailboxLookupResult> + Send + '_>> {
        let want = address.trim().to_string();
        Box::pin(async move {
            if want.is_empty() || !want.contains('@') {
                return MailboxLookupResult {
                    ok: false,
                    account: None,
                    source: "mock",
                    note: "mailbox address required".into(),
                };
            }
            let found = {
                let lock = self.accounts.lock().unwrap_or_else(|e| e.into_inner());
                lock.iter()
                    .find(|a| a.address.eq_ignore_ascii_case(&want))
                    .cloned()
            };
            match found {
                Some(account) => MailboxLookupResult {
                    ok: true,
                    account: Some(account),
                    source: "mock",
                    note: "Hermetic mock mailbox lookup.".into(),
                },
                None => MailboxLookupResult {
                    ok: false,
                    account: None,
                    source: "mock",
                    note: "No mailbox with that address.".into(),
                },
            }
        })
    }
}

/// Live Stalwart management-API directory (JMAP `x:Account/query` + `get`).
///
/// Fail-closed listing: HTTP / parse errors yield empty accounts + error note
/// with `source: "stalwart"` (never invents rows). Process start fails closed
/// when `SURMOUNT_DIRECTORY=stalwart` is set without a usable token.
#[derive(Clone)]
pub struct StalwartDirectory {
    http: reqwest::Client,
    /// Base URL without trailing slash (e.g. `http://127.0.0.1:8080`).
    base_url: String,
    /// Bearer token (API key). Never logged.
    token: String,
}

impl StalwartDirectory {
    /// Build from explicit parts (tests / constructor injection).
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            http,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token: token.into(),
        }
    }

    /// POST management JMAP endpoint; returns parsed JSON or status-only error.
    ///
    /// Stalwart 0.16.15 serves this at `{base}/jmap`. `{base}/api` is HTTP 404
    /// (problem+json). Do not use `/api` or `x:AccountPassword/set`.
    async fn post_jmap(&self, body: &Value) -> Result<Value, String> {
        let url = format!("{}/jmap", self.base_url);
        let resp = self
            .http
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| redact_onion_in_text(&format!("HTTP transport error: {e}")))?;

        let status = resp.status();
        // Always consume body so the connection can be reused; never put body
        // text into operator-facing inventory notes (Issue 3 / 14).
        let text = resp
            .text()
            .await
            .map_err(|e| format!("read response body: {e}"))?;

        if !status.is_success() {
            // Status-code only: no upstream body reflection into SSR/API notes.
            return Err(format!(
                "Stalwart management API HTTP {status} (body omitted from client note)"
            ));
        }

        serde_json::from_str(&text).map_err(|_| "unparseable JMAP JSON".to_string())
    }

    /// POST management JMAP endpoint and map accounts.
    async fn fetch_accounts(&self) -> Result<Vec<AccountEntry>, String> {
        let value = self.post_jmap(&jmap_account_list_request()).await?;
        accounts_from_jmap_response(&value)
    }

    async fn set_create(&self, input: &CreateAccountInput) -> Result<AccountEntry, String> {
        let body = jmap_account_create_request(input);
        let value = self.post_jmap(&body).await?;
        account_from_jmap_set_create(&value, "new1")
    }

    async fn set_update(&self, input: &UpdateAccountInput) -> Result<AccountEntry, String> {
        let body = jmap_account_update_request(input);
        let value = self.post_jmap(&body).await?;
        // Confirmed id only from set updated/notUpdated maps (no invented success).
        account_from_jmap_set_update(&value, &input.id)
    }

    async fn lookup_by_mailbox(&self, mailbox: &str) -> Result<AccountEntry, String> {
        let want = mailbox.trim();
        if want.is_empty() || !want.contains('@') {
            return Err("mailbox address required".into());
        }
        let accounts = self.fetch_accounts().await?;
        accounts
            .into_iter()
            .find(|a| a.address.eq_ignore_ascii_case(want))
            .ok_or_else(|| "No mailbox with that address.".into())
    }

    async fn set_password_credential(&self, id: &str, password: &str) -> Result<(), String> {
        let body = jmap_account_set_password_request(id, password);
        let value = self.post_jmap(&body).await?;
        account_from_jmap_set_update(&value, id).map(|_| ())
    }
}

impl Directory for StalwartDirectory {
    fn list_accounts(&self) -> Pin<Box<dyn Future<Output = AccountsInventory> + Send + '_>> {
        Box::pin(async move {
            match self.fetch_accounts().await {
                Ok(accounts) => {
                    let n = accounts.len();
                    AccountsInventory {
                        accounts,
                        source: "stalwart",
                        note: format!(
                            "Live inventory from Stalwart management JMAP \
(x:Account/query + x:Account/get) at configured SURMOUNT_STALWART_URL. \
{n} principal(s). Mutations: POST/PATCH /api/v1/accounts (auth + CSRF when cookie)."
                        ),
                    }
                }
                Err(err) => AccountsInventory {
                    accounts: Vec::new(),
                    source: "stalwart",
                    note: format!(
                        "Stalwart directory configured but list failed \
(fail-closed empty; no invented rows). {err}"
                    ),
                },
            }
        })
    }

    fn create_account(
        &self,
        input: CreateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        Box::pin(async move {
            let name = match normalize_mailbox_local_part(&input.name) {
                Ok(n) => n,
                Err(note) => {
                    return AccountMutationResult {
                        ok: false,
                        account: None,
                        source: "stalwart",
                        note: format!("create refused: {note}"),
                    };
                }
            };
            let domain_id = input.domain_id.trim();
            if domain_id.is_empty() {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "stalwart",
                    note: "create refused: name and domain_id required (non-empty)".into(),
                };
            }
            let input = CreateAccountInput {
                name,
                domain_id: domain_id.to_string(),
                description: input.description.clone(),
            };
            match self.set_create(&input).await {
                Ok(account) => AccountMutationResult {
                    ok: true,
                    account: Some(account),
                    source: "stalwart",
                    note: "Created via Stalwart management JMAP x:Account/set (create). \
Set the mailbox password from Mail on this console."
                        .into(),
                },
                Err(err) => AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "stalwart",
                    note: format!("Stalwart create failed (fail-closed). {err}"),
                },
            }
        })
    }

    fn update_account(
        &self,
        input: UpdateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        Box::pin(async move {
            let id = input.id.trim();
            if id.is_empty() {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "stalwart",
                    note: "update refused: id required".into(),
                };
            }
            if input.description.is_none() {
                return AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "stalwart",
                    note: "update refused: description patch required (lean surface)".into(),
                };
            }
            match self.set_update(&input).await {
                Ok(account) => AccountMutationResult {
                    ok: true,
                    account: Some(account),
                    source: "stalwart",
                    note: "Updated via Stalwart management JMAP x:Account/set (update).".into(),
                },
                Err(err) => AccountMutationResult {
                    ok: false,
                    account: None,
                    source: "stalwart",
                    note: format!("Stalwart update failed (fail-closed). {err}"),
                },
            }
        })
    }

    fn set_mailbox_password(
        &self,
        input: SetMailboxPasswordInput,
    ) -> Pin<Box<dyn Future<Output = MailboxPasswordResult> + Send + '_>> {
        Box::pin(async move {
            let mailbox = input.mailbox.trim().to_string();
            if mailbox.is_empty() || !mailbox.contains('@') {
                return MailboxPasswordResult {
                    ok: false,
                    source: "stalwart",
                    mailbox: None,
                    note: "password set refused: mailbox address required".into(),
                };
            }
            if input.password.is_empty() {
                return MailboxPasswordResult {
                    ok: false,
                    source: "stalwart",
                    mailbox: Some(mailbox),
                    note: "password set refused: password required".into(),
                };
            }
            let entry = match self.lookup_by_mailbox(&mailbox).await {
                Ok(e) => e,
                Err(err) => {
                    return MailboxPasswordResult {
                        ok: false,
                        source: "stalwart",
                        mailbox: Some(mailbox),
                        note: err,
                    };
                }
            };
            match self
                .set_password_credential(&entry.id, &input.password)
                .await
            {
                Ok(()) => MailboxPasswordResult {
                    ok: true,
                    source: "stalwart",
                    mailbox: Some(entry.address),
                    note: "Password set. Use this password in your mail app.".into(),
                },
                Err(err) => MailboxPasswordResult {
                    ok: false,
                    source: "stalwart",
                    mailbox: Some(entry.address),
                    note: format!("Stalwart password set failed (fail-closed). {err}"),
                },
            }
        })
    }

    fn lookup_domain_id(
        &self,
        domain_name: &str,
    ) -> Pin<Box<dyn Future<Output = DomainLookupResult> + Send + '_>> {
        let want = domain_name.trim().to_string();
        Box::pin(async move {
            if want.is_empty() {
                return DomainLookupResult {
                    ok: false,
                    domain_id: None,
                    source: "stalwart",
                    note: "domain lookup refused: primary domain required".into(),
                };
            }
            match self.post_jmap(&jmap_domain_list_request()).await {
                Ok(body) => match domain_id_from_jmap(&body, &want) {
                    Ok(Some(id)) => DomainLookupResult {
                        ok: true,
                        domain_id: Some(id),
                        source: "stalwart",
                        note: "Domain id from Stalwart x:Domain/query + get.".into(),
                    },
                    Ok(None) => DomainLookupResult {
                        ok: false,
                        domain_id: None,
                        source: "stalwart",
                        note: "Primary domain not found in directory (fail-closed).".into(),
                    },
                    Err(err) => DomainLookupResult {
                        ok: false,
                        domain_id: None,
                        source: "stalwart",
                        note: format!("Stalwart domain lookup failed (fail-closed). {err}"),
                    },
                },
                Err(err) => DomainLookupResult {
                    ok: false,
                    domain_id: None,
                    source: "stalwart",
                    note: format!("Stalwart domain lookup failed (fail-closed). {err}"),
                },
            }
        })
    }

    fn lookup_mailbox(
        &self,
        address: &str,
    ) -> Pin<Box<dyn Future<Output = MailboxLookupResult> + Send + '_>> {
        let want = address.trim().to_string();
        Box::pin(async move {
            match self.lookup_by_mailbox(&want).await {
                Ok(account) => MailboxLookupResult {
                    ok: true,
                    account: Some(account),
                    source: "stalwart",
                    note: "Mailbox found via Stalwart directory.".into(),
                },
                Err(err) => MailboxLookupResult {
                    ok: false,
                    account: None,
                    source: "stalwart",
                    note: err,
                },
            }
        })
    }
}

/// Listing stays honest-empty (`unavailable`); password set still talks to Stalwart.
///
/// Used when `SURMOUNT_DIRECTORY` is unset/unavailable but the host token and
/// loopback URL exist. Accounts page stays offline; the Mail password form works.
pub struct StalwartPasswordOnly {
    inner: StalwartDirectory,
}

impl StalwartPasswordOnly {
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        token: impl Into<String>,
    ) -> Self {
        Self {
            inner: StalwartDirectory::new(http, base_url, token),
        }
    }
}

impl Directory for StalwartPasswordOnly {
    fn list_accounts(&self) -> Pin<Box<dyn Future<Output = AccountsInventory> + Send + '_>> {
        UnavailableDirectory.list_accounts()
    }

    fn create_account(
        &self,
        input: CreateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        self.inner.create_account(input)
    }

    fn update_account(
        &self,
        input: UpdateAccountInput,
    ) -> Pin<Box<dyn Future<Output = AccountMutationResult> + Send + '_>> {
        UnavailableDirectory.update_account(input)
    }

    fn set_mailbox_password(
        &self,
        input: SetMailboxPasswordInput,
    ) -> Pin<Box<dyn Future<Output = MailboxPasswordResult> + Send + '_>> {
        self.inner.set_mailbox_password(input)
    }

    fn lookup_domain_id(
        &self,
        domain_name: &str,
    ) -> Pin<Box<dyn Future<Output = DomainLookupResult> + Send + '_>> {
        self.inner.lookup_domain_id(domain_name)
    }

    fn lookup_mailbox(
        &self,
        address: &str,
    ) -> Pin<Box<dyn Future<Output = MailboxLookupResult> + Send + '_>> {
        self.inner.lookup_mailbox(address)
    }
}

/// JMAP request body: query all accounts then get id/name/emailAddress/@type.
pub fn jmap_account_list_request() -> Value {
    json!({
        "using": [
            "urn:ietf:params:jmap:core",
            "urn:stalwart:jmap"
        ],
        "methodCalls": [
            [
                "x:Account/query",
                {
                    "filter": {},
                    "limit": 500
                },
                "q1"
            ],
            [
                "x:Account/get",
                {
                    "#ids": {
                        "resultOf": "q1",
                        "name": "x:Account/query",
                        "path": "/ids"
                    },
                    "properties": ["id", "name", "emailAddress", "@type"]
                },
                "g1"
            ]
        ]
    })
}

/// JMAP Domain query + get (id + name). Filter client-side by primary domain.
pub fn jmap_domain_list_request() -> Value {
    json!({
        "using": [
            "urn:ietf:params:jmap:core",
            "urn:stalwart:jmap"
        ],
        "methodCalls": [
            [
                "x:Domain/query",
                {
                    "filter": {},
                    "limit": 50
                },
                "dq1"
            ],
            [
                "x:Domain/get",
                {
                    "#ids": {
                        "resultOf": "dq1",
                        "name": "x:Domain/query",
                        "path": "/ids"
                    },
                    "properties": ["id", "name"]
                },
                "dg1"
            ]
        ]
    })
}

/// Pick Domain id whose `name` matches `domain_name` (case-insensitive).
pub fn domain_id_from_jmap(body: &Value, domain_name: &str) -> Result<Option<String>, String> {
    let want = domain_name.trim().to_ascii_lowercase();
    if want.is_empty() {
        return Err("primary domain required".into());
    }
    let responses = body
        .get("methodResponses")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "JMAP response missing methodResponses array".to_string())?;
    for entry in responses {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => continue,
        };
        let name = arr[0].as_str().unwrap_or("");
        if name != "x:Domain/get" && name != "Domain/get" {
            continue;
        }
        let args = &arr[1];
        if let Some(err_type) = args.get("type").and_then(|t| t.as_str())
            && args.get("list").is_none()
        {
            return Err(format!("JMAP method error type={err_type}"));
        }
        let list = args
            .get("list")
            .and_then(|l| l.as_array())
            .ok_or_else(|| "x:Domain/get response missing list array".to_string())?;
        for item in list {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if name == want {
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                return Ok(id);
            }
        }
        return Ok(None);
    }
    for entry in responses {
        if let Some(arr) = entry.as_array()
            && arr.first().and_then(|v| v.as_str()) == Some("error")
        {
            let err_type = arr
                .get(1)
                .and_then(|v| v.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            return Err(format!("JMAP error response type={err_type}"));
        }
    }
    Err("JMAP response had no x:Domain/get list".into())
}

/// JMAP create via `x:Account/set` (User principal; no credentials in lean surface).
pub fn jmap_account_create_request(input: &CreateAccountInput) -> Value {
    let mut create_obj = json!({
        "@type": "User",
        "name": input.name.trim(),
        "domainId": input.domain_id.trim(),
        "aliases": {},
        "credentials": {},
        "memberGroupIds": {},
        "encryptionAtRest": { "@type": "Disabled" },
        "permissions": { "@type": "Inherit" },
        "quotas": {},
        "roles": { "@type": "User" }
    });
    if let Some(desc) = input
        .description
        .as_ref()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
    {
        create_obj["description"] = json!(desc);
    }
    json!({
        "using": [
            "urn:ietf:params:jmap:core",
            "urn:stalwart:jmap"
        ],
        "methodCalls": [
            [
                "x:Account/set",
                {
                    "create": {
                        "new1": create_obj
                    }
                },
                "c1"
            ]
        ]
    })
}

/// JMAP update via `x:Account/set` (description patch only).
pub fn jmap_account_update_request(input: &UpdateAccountInput) -> Value {
    let mut patch = json!({});
    if let Some(desc) = &input.description {
        patch["description"] = json!(desc);
    }
    let id = input.id.trim();
    json!({
        "using": [
            "urn:ietf:params:jmap:core",
            "urn:stalwart:jmap"
        ],
        "methodCalls": [
            [
                "x:Account/set",
                {
                    "update": {
                        id: patch
                    }
                },
                "u1"
            ]
        ]
    })
}

/// JMAP update via `x:Account/set`: Password credential on the mailbox principal.
///
/// Not `x:AccountPassword/set` (that singleton is the authenticated API-token
/// principal). Secret is plaintext in the HTTP body to loopback Stalwart
/// only; the engine hashes it (0.16.15 default: Argon2id). Callers must not
/// log this Value.
pub fn jmap_account_set_password_request(id: &str, secret: &str) -> Value {
    let id = id.trim();
    json!({
        "using": [
            "urn:ietf:params:jmap:core",
            "urn:stalwart:jmap"
        ],
        "methodCalls": [
            [
                "x:Account/set",
                {
                    "update": {
                        id: {
                            "credentials": {
                                "0": {
                                    "@type": "Password",
                                    "secret": secret
                                }
                            }
                        }
                    }
                },
                "u1"
            ]
        ]
    })
}

/// Map `x:Account/set` create response for creation id key (e.g. `new1`).
pub fn account_from_jmap_set_create(
    body: &Value,
    create_key: &str,
) -> Result<AccountEntry, String> {
    let set_args = first_account_set_args(body)?;
    // JMAP set notCreated: { "new1": { type, description } }
    if let Some(not_created) = set_args.get("notCreated").and_then(|v| v.as_object())
        && let Some(err) = not_created.get(create_key)
    {
        let err_type = err
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        // Type only; never reflect full description body into client note.
        return Err(format!("JMAP set notCreated type={err_type}"));
    }
    let created = set_args
        .get("created")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "x:Account/set response missing created map".to_string())?;
    let obj = created
        .get(create_key)
        .ok_or_else(|| format!("x:Account/set created missing key {create_key}"))?;
    let id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "x:Account/set created object missing id".to_string())?
        .to_string();
    let email = obj
        .get("emailAddress")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let address = match (email, name) {
        (Some(e), _) => e,
        (None, Some(n)) => n,
        (None, None) => id.clone(),
    };
    let status = obj
        .get("@type")
        .and_then(|v| v.as_str())
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "user".into());
    Ok(AccountEntry {
        id,
        address,
        status,
    })
}

/// Confirm update succeeded; prefer updated map entry if present.
pub fn account_from_jmap_set_update(body: &Value, id: &str) -> Result<AccountEntry, String> {
    let set_args = first_account_set_args(body)?;
    if let Some(not_updated) = set_args.get("notUpdated").and_then(|v| v.as_object())
        && let Some(err) = not_updated.get(id)
    {
        let err_type = err
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("unknown");
        return Err(format!("JMAP set notUpdated type={err_type}"));
    }
    let updated = set_args
        .get("updated")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "x:Account/set response missing updated map".to_string())?;
    if !updated.contains_key(id) {
        return Err(format!("x:Account/set updated missing id {id}"));
    }
    // Updated map values may be null or partial; do not invent email.
    Ok(AccountEntry {
        id: id.to_string(),
        address: id.to_string(),
        status: "active".into(),
    })
}

fn first_account_set_args(body: &Value) -> Result<&Value, String> {
    let responses = body
        .get("methodResponses")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "JMAP response missing methodResponses array".to_string())?;
    for entry in responses {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => continue,
        };
        let name = arr[0].as_str().unwrap_or("");
        if name == "x:Account/set" || name == "Account/set" {
            let args = &arr[1];
            if let Some(err_type) = args.get("type").and_then(|t| t.as_str())
                && args.get("created").is_none()
                && args.get("updated").is_none()
                && args.get("destroyed").is_none()
            {
                return Err(format!("JMAP method error type={err_type}"));
            }
            return Ok(args);
        }
        if name == "error" {
            let err_type = arr
                .get(1)
                .and_then(|v| v.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            return Err(format!("JMAP error response type={err_type}"));
        }
    }
    Err("JMAP response had no x:Account/set result".into())
}

/// Mutations require Nostr auth mode, or lab escape (same env as live directory list).
///
/// Production: create/update fail closed when `AUTH_MODE=off` unless
/// `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1`. Does not invent Q-AUTH-1.
pub fn require_mutation_auth_coupling(
    auth_mode: AuthMode,
    allow_unauthenticated: bool,
) -> Result<(), String> {
    if auth_mode == AuthMode::Off && !allow_unauthenticated {
        return Err("account mutations require SURMOUNT_AUTH_MODE=nostr \
(or SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1 for lab only; fail-closed). \
Open auth_mode=off must not mutate principals."
            .into());
    }
    Ok(())
}

/// Map a successful JMAP response into account rows (no invent on missing list).
///
/// Looks for the first `x:Account/get` method response with a `list` array.
/// Each object needs at least `id`; address prefers `emailAddress`, else `name`.
pub fn accounts_from_jmap_response(body: &Value) -> Result<Vec<AccountEntry>, String> {
    let responses = body
        .get("methodResponses")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "JMAP response missing methodResponses array".to_string())?;

    // Prefer get; if only query ids present without get, fail closed (need details).
    for entry in responses {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => continue,
        };
        let name = arr[0].as_str().unwrap_or("");
        if name != "x:Account/get" && name != "Account/get" {
            continue;
        }
        let args = &arr[1];
        // JMAP error form: ["error", { type, ... }, "c1"] - method name is still
        // the requested method in some servers; others use "error". Check type.
        if let Some(err_type) = args.get("type").and_then(|t| t.as_str())
            && args.get("list").is_none()
        {
            return Err(format!("JMAP method error type={err_type}"));
        }
        let list = args
            .get("list")
            .and_then(|l| l.as_array())
            .ok_or_else(|| "x:Account/get response missing list array".to_string())?;

        let mut out = Vec::with_capacity(list.len());
        for item in list {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if id.is_empty() {
                continue;
            }
            let email = item
                .get("emailAddress")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let address = match (email, name) {
                (Some(e), _) => e,
                (None, Some(n)) => n,
                (None, None) => id.clone(),
            };
            // Wire field is still `status` for JSON stability; value is principal
            // kind (@type) when present, else "active" (not a lifecycle SoT).
            let status = item
                .get("@type")
                .and_then(|v| v.as_str())
                .map(|t| t.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "active".into());
            out.push(AccountEntry {
                id,
                address,
                status,
            });
        }
        return Ok(out);
    }

    // Surface first error method if present (type only; no full args JSON).
    for entry in responses {
        if let Some(arr) = entry.as_array()
            && arr.first().and_then(|v| v.as_str()) == Some("error")
        {
            let err_type = arr
                .get(1)
                .and_then(|v| v.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("unknown");
            return Err(format!("JMAP error response type={err_type}"));
        }
    }

    Err("JMAP response had no x:Account/get list".into())
}

/// Truncate for diagnostics only. Char-boundary safe (no mid-UTF-8 panic).
/// Not used for client-facing HTTP error notes (those omit body).
#[cfg_attr(not(test), allow(dead_code))]
fn truncate_for_note(s: &str, max_bytes: usize) -> String {
    let t = s.trim();
    if t.len() <= max_bytes {
        return t.to_string();
    }
    // Floor to a char boundary at or before max_bytes.
    let mut end = max_bytes.min(t.len());
    while end > 0 && !t.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &t[..end])
}

/// Parsed `SURMOUNT_DIRECTORY` backend kind (pure; for tests and selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryBackendKind {
    Unavailable,
    Mock,
    Stalwart,
}

/// Parse directory backend from env value (fail-closed on unknown non-empty).
pub fn parse_directory_backend(raw: &str) -> Result<DirectoryBackendKind, String> {
    let mode = raw.trim();
    if mode.is_empty() || mode.eq_ignore_ascii_case("unavailable") {
        Ok(DirectoryBackendKind::Unavailable)
    } else if mode.eq_ignore_ascii_case("mock") {
        Ok(DirectoryBackendKind::Mock)
    } else if mode.eq_ignore_ascii_case("stalwart") {
        Ok(DirectoryBackendKind::Stalwart)
    } else {
        Err(format!(
            "SURMOUNT_DIRECTORY={mode:?} is not a known backend \
(use unavailable, mock, or stalwart; fail-closed)"
        ))
    }
}

/// Non-empty base URL for live Stalwart (trim trailing slash).
pub fn require_stalwart_base_url(base: &str) -> Result<String, String> {
    let base = base.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(
            "SURMOUNT_DIRECTORY=stalwart requires non-empty SURMOUNT_STALWART_URL \
(fail-closed)"
                .into(),
        );
    }
    Ok(base.to_string())
}

/// Live directory + open auth is refused unless lab escape is set.
///
/// Production: `directory=stalwart` requires `authMode=nostr` so principal
/// inventory is not open on the UI bind. Lab:
/// `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1` (or Nix
/// `allowDirectoryUnauthenticated`).
pub fn require_stalwart_auth_coupling(
    auth_mode: AuthMode,
    allow_unauthenticated: bool,
) -> Result<(), String> {
    if auth_mode == AuthMode::Off && !allow_unauthenticated {
        return Err(
            "SURMOUNT_DIRECTORY=stalwart requires SURMOUNT_AUTH_MODE=nostr \
(or SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED=1 for lab only; fail-closed). \
Live principal list must not be open on an unauthenticated bind."
                .into(),
        );
    }
    Ok(())
}

/// Lab escape: open `auth_mode=off` may list/mutate when this is true.
/// Env: `SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED` (never production default).
///
/// Token SoT is [`crate::config::parse_env_flag_truthy`] (same as config
/// `env_bool`): `1` / `true` / `yes` / `on`. Do not re-parse with a divergent set.
pub fn directory_allow_unauthenticated_from_env() -> bool {
    match env::var("SURMOUNT_DIRECTORY_ALLOW_UNAUTHENTICATED") {
        Ok(v) => crate::config::parse_env_flag_truthy(&v),
        Err(_) => false,
    }
}

/// Backend selection from env (fail-closed for explicit misconfig).
///
/// | `SURMOUNT_DIRECTORY` | Result |
/// |----------------------|--------|
/// | unset / empty / `unavailable` | [`UnavailableDirectory`] |
/// | `mock` | hermetic [`MockDirectory`] (never production default) |
/// | `stalwart` | [`StalwartDirectory`] (requires token + auth coupling) |
/// | other non-empty | **Err** (typo fail-closed) |
///
/// Token for stalwart: non-empty `SURMOUNT_STALWART_TOKEN`, else non-empty
/// contents of `SURMOUNT_STALWART_TOKEN_FILE` (raw token file, host-only,
/// regular file, mode not group/world readable).
/// Base URL: `SURMOUNT_STALWART_URL` (same as status probe).
/// Auth: live backend requires `AuthMode::Nostr` unless lab escape env is set.
pub fn directory_from_env(
    http: reqwest::Client,
    auth_mode: AuthMode,
) -> Result<Box<dyn Directory>, String> {
    let mode_raw = env::var("SURMOUNT_DIRECTORY").unwrap_or_default();
    match parse_directory_backend(&mode_raw)? {
        DirectoryBackendKind::Unavailable => {
            // Listing stays honest-empty. If host token + URL exist, password
            // set still talks to Stalwart (services-console mailbox form).
            match optional_stalwart_password_only(http) {
                Some(dir) => Ok(Box::new(dir)),
                None => Ok(directory_unavailable()),
            }
        }
        DirectoryBackendKind::Mock => Ok(directory_mock()),
        DirectoryBackendKind::Stalwart => {
            require_stalwart_auth_coupling(auth_mode, directory_allow_unauthenticated_from_env())?;
            let token = load_stalwart_token()?;
            let base = env::var("SURMOUNT_STALWART_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8080".into());
            let base = require_stalwart_base_url(&base)?;
            Ok(Box::new(StalwartDirectory::new(http, base, token)))
        }
    }
}

/// Build a live Stalwart client when token + URL exist (no fail-closed start).
///
/// Used for mailbox password set while directory listing stays unavailable.
/// Missing token is None (not a process-start error).
fn optional_stalwart_password_only(http: reqwest::Client) -> Option<StalwartPasswordOnly> {
    let token = load_stalwart_token().ok()?;
    if token.is_empty() {
        return None;
    }
    let base = env::var("SURMOUNT_STALWART_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let base = require_stalwart_base_url(&base).ok()?;
    Some(StalwartPasswordOnly::new(http, base, token))
}

/// Load Bearer token: env wins when non-empty; else token file contents.
pub fn load_stalwart_token() -> Result<String, String> {
    if let Ok(v) = env::var("SURMOUNT_STALWART_TOKEN") {
        let t = v.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    match env::var("SURMOUNT_STALWART_TOKEN_FILE") {
        Ok(p) => {
            let p = p.trim();
            if p.is_empty() {
                return Err(
                    "SURMOUNT_DIRECTORY=stalwart: SURMOUNT_STALWART_TOKEN_FILE is empty \
and SURMOUNT_STALWART_TOKEN is unset (fail-closed; host token required)"
                        .into(),
                );
            }
            token_from_file(Path::new(p))
        }
        Err(_) => Err(
            "SURMOUNT_DIRECTORY=stalwart requires SURMOUNT_STALWART_TOKEN or \
SURMOUNT_STALWART_TOKEN_FILE (host-only API key; never in git; fail-closed)"
                .into(),
        ),
    }
}

fn token_from_file(path: &Path) -> Result<String, String> {
    // Startup-only path: may include path + mode in the error string for the
    // journal. Never call this on the request path into HTTP inventory notes
    // (Issue 15).
    require_token_file_mode(path)?;
    let contents = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "SURMOUNT_STALWART_TOKEN_FILE={} unreadable: {e} (fail-closed)",
            path.display()
        )
    })?;
    let t = contents.trim();
    if t.is_empty() {
        return Err(format!(
            "SURMOUNT_STALWART_TOKEN_FILE={} is empty (fail-closed). \
File must contain a non-empty raw token line (not comments only).",
            path.display()
        ));
    }
    // First non-empty non-# line only. Comment-only files fail closed (no
    // fallback to full contents, which would treat "# comment" as the Bearer).
    let line = contents
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .ok_or_else(|| {
            format!(
                "SURMOUNT_STALWART_TOKEN_FILE={} has no token line (fail-closed). \
Need a non-empty non-# line with the raw API token.",
                path.display()
            )
        })?;
    Ok(line.to_string())
}

/// Token file must be a regular file and not group/world readable (`mode & 0o077 == 0`).
/// Mirrors TLS private key check. Expected operator mode e.g. 0600.
fn require_token_file_mode(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| {
        format!(
            "SURMOUNT_STALWART_TOKEN_FILE={} unreadable: {e} (fail-closed)",
            path.display()
        )
    })?;
    if !meta.is_file() {
        return Err(format!(
            "SURMOUNT_STALWART_TOKEN_FILE={} is not a regular file (fail-closed)",
            path.display()
        ));
    }
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(format!(
            "SURMOUNT_STALWART_TOKEN_FILE={} must not be group/world readable \
(mode {:04o}; require owner-only, e.g. 0600; fail-closed)",
            path.display(),
            mode & 0o777
        ));
    }
    Ok(())
}

/// Production / default constructor: always unavailable (ignores env).
pub fn directory_unavailable() -> Box<dyn Directory> {
    Box::new(UnavailableDirectory)
}

/// Labeled mock fixture constructor (tests / explicit env opt-in only).
pub fn directory_mock() -> Box<dyn Directory> {
    Box::new(MockDirectory::fixture())
}

/// Live Stalwart constructor for tests / explicit injection.
#[cfg(test)]
pub fn directory_stalwart(
    http: reqwest::Client,
    base_url: impl Into<String>,
    token: impl Into<String>,
) -> Box<dyn Directory> {
    Box::new(StalwartDirectory::new(http, base_url, token))
}

/// Listing unavailable + live password client (tests / host default with token).
#[cfg(test)]
pub fn directory_stalwart_password_only(
    http: reqwest::Client,
    base_url: impl Into<String>,
    token: impl Into<String>,
) -> Box<dyn Directory> {
    Box::new(StalwartPasswordOnly::new(http, base_url, token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::extract::Request;
    use axum::http::{StatusCode, header};
    use axum::response::Response;
    use axum::routing::post;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Named contract: default directory is honest empty + unavailable.
    #[tokio::test]
    async fn unavailable_directory_is_honest_empty() {
        let inv = UnavailableDirectory.list_accounts().await;
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
        assert!(
            !blob.contains("\"source\":\"stalwart\"") && !blob.contains("\"source\": \"stalwart\""),
            "unavailable must not claim stalwart source: {blob}"
        );
    }

    /// Named contract: mock is labeled and non-empty without looking like live Stalwart.
    #[tokio::test]
    async fn mock_directory_returns_labeled_non_empty_not_stalwart() {
        let inv = MockDirectory::fixture().list_accounts().await;
        assert!(
            !inv.accounts.is_empty(),
            "mock fixture must return principals"
        );
        assert_eq!(
            inv.source, "mock",
            "mock must label source as mock, not unavailable/stalwart"
        );
        assert_ne!(inv.source, "stalwart");
        assert_ne!(inv.source, "unavailable");
        let note = inv.note.to_ascii_lowercase();
        assert!(
            note.contains("mock") || note.contains("hermetic") || note.contains("fixture"),
            "note should mark hermetic mock: {}",
            inv.note
        );
        assert!(
            note.contains("not live") || (note.contains("not") && note.contains("stalwart")),
            "note must not look like live Stalwart: {}",
            inv.note
        );
        for a in &inv.accounts {
            assert!(
                a.address.contains("mock") || a.id.starts_with("mock"),
                "fixture address/id should be mock-labeled: {a:?}"
            );
            assert!(
                !a.address.starts_with("admin@"),
                "fixture must not look like production admin@: {}",
                a.address
            );
        }
        let blob = serde_json::to_string(&inv).unwrap();
        assert!(
            !blob.contains("\"source\":\"stalwart\"") && !blob.contains("\"source\": \"stalwart\""),
            "mock must never claim stalwart source: {blob}"
        );
    }

    #[tokio::test]
    async fn directory_unavailable_constructor_is_empty() {
        let inv = directory_unavailable().list_accounts().await;
        assert_eq!(inv.source, "unavailable");
        assert!(inv.accounts.is_empty());
    }

    #[tokio::test]
    async fn directory_mock_constructor_is_labeled() {
        let inv = directory_mock().list_accounts().await;
        assert_eq!(inv.source, "mock");
        assert!(!inv.accounts.is_empty());
    }

    /// Named contract: JMAP get list maps to AccountEntry without inventing rows.
    #[test]
    fn accounts_from_jmap_maps_list_rows() {
        let body = json!({
            "methodResponses": [
                ["x:Account/query", {"ids": ["u1", "g1"], "total": 2}, "q1"],
                ["x:Account/get", {
                    "list": [
                        {
                            "id": "u1",
                            "name": "alice",
                            "emailAddress": "alice@example.test",
                            "@type": "User"
                        },
                        {
                            "id": "g1",
                            "name": "staff",
                            "emailAddress": "staff@example.test",
                            "@type": "Group"
                        }
                    ],
                    "notFound": []
                }, "g1"]
            ]
        });
        let rows = accounts_from_jmap_response(&body).expect("parse");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "u1");
        assert_eq!(rows[0].address, "alice@example.test");
        assert_eq!(rows[0].status, "user");
        assert_eq!(rows[1].id, "g1");
        assert_eq!(rows[1].address, "staff@example.test");
        assert_eq!(rows[1].status, "group");
        assert!(!rows.iter().any(|a| a.address.starts_with("admin@")));
    }

    /// Named contract: missing get list is an error (caller fail-closes empty).
    #[test]
    fn accounts_from_jmap_missing_get_is_err() {
        let body = json!({
            "methodResponses": [
                ["x:Account/query", {"ids": ["u1"]}, "q1"]
            ]
        });
        let err = accounts_from_jmap_response(&body).unwrap_err();
        assert!(
            err.contains("no x:Account/get") || err.contains("missing"),
            "expected missing get error: {err}"
        );
    }

    /// Named contract: JMAP error method fails closed (no invented accounts).
    #[test]
    fn accounts_from_jmap_error_method_is_err() {
        let body = json!({
            "methodResponses": [
                ["error", {"type": "forbidden"}, "q1"]
            ]
        });
        let err = accounts_from_jmap_response(&body).unwrap_err();
        assert!(
            err.to_ascii_lowercase().contains("error") || err.contains("forbidden"),
            "expected error surface: {err}"
        );
    }

    /// Named contract: live client maps wire response to source=stalwart rows.
    #[tokio::test]
    async fn stalwart_directory_lists_from_jmap_wire_mock() {
        let seen_auth = Arc::new(Mutex::new(None::<String>));
        let seen_auth_h = seen_auth.clone();

        let app = Router::new().route(
            "/jmap",
            post(move |req: Request| {
                let seen_auth = seen_auth_h.clone();
                async move {
                    let auth = req
                        .headers()
                        .get(header::AUTHORIZATION)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    *seen_auth.lock().await = auth;

                    let body = json!({
                        "methodResponses": [
                            ["x:Account/query", {"ids": ["live-1"], "total": 1}, "q1"],
                            ["x:Account/get", {
                                "list": [{
                                    "id": "live-1",
                                    "name": "ops",
                                    "emailAddress": "ops@example.test",
                                    "@type": "User"
                                }],
                                "notFound": []
                            }, "g1"]
                        ],
                        "sessionState": "test"
                    });
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap()
                }
            }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "test-token-xyz");
        let inv = dir.list_accounts().await;

        assert_eq!(inv.source, "stalwart");
        assert_eq!(inv.accounts.len(), 1);
        assert_eq!(inv.accounts[0].id, "live-1");
        assert_eq!(inv.accounts[0].address, "ops@example.test");
        assert_eq!(inv.accounts[0].status, "user");
        assert!(!inv.note.to_ascii_lowercase().contains("fail"));
        assert!(
            !serde_json::to_string(&inv).unwrap().contains("admin@"),
            "must not invent admin@"
        );

        let auth = seen_auth.lock().await.clone();
        assert_eq!(
            auth.as_deref(),
            Some("Bearer test-token-xyz"),
            "must send Bearer token"
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: live client fail-closes empty on HTTP error (source still stalwart).
    #[tokio::test]
    async fn stalwart_directory_fail_closed_empty_on_server_error() {
        let app = Router::new().route(
            "/jmap",
            post(|| async {
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from(r#"{"error":"unauthorized"}"#))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "bad-token");
        let inv = dir.list_accounts().await;

        assert_eq!(inv.source, "stalwart");
        assert!(
            inv.accounts.is_empty(),
            "must not invent accounts on error; got {:?}",
            inv.accounts
        );
        let note = inv.note.to_ascii_lowercase();
        assert!(
            note.contains("fail") || note.contains("failed") || note.contains("401"),
            "note should explain failure: {}",
            inv.note
        );
        assert!(
            !inv.note.contains("admin@"),
            "error note must not invent admin@"
        );
        // Body of upstream response must not appear in client note (status phrase ok).
        assert!(
            !inv.note.contains(r#"{"error""#) && !inv.note.contains("leaked"),
            "must not reflect JSON body: {}",
            inv.note
        );

        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: live client fail-closes empty when nothing listens (transport).
    #[tokio::test]
    async fn stalwart_directory_fail_closed_empty_when_unreachable() {
        // Bind then drop so the port is closed (connection refused).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(2))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "token");
        let inv = dir.list_accounts().await;
        assert_eq!(inv.source, "stalwart");
        assert!(inv.accounts.is_empty());
        assert!(
            inv.note.to_ascii_lowercase().contains("fail")
                || inv.note.to_ascii_lowercase().contains("error")
                || inv.note.to_ascii_lowercase().contains("transport"),
            "note should explain transport failure: {}",
            inv.note
        );
    }

    /// Named contract: token file load rejects missing path (fail-closed).
    #[test]
    fn token_from_file_missing_fail_closed() {
        let err = token_from_file(Path::new("/no/such/surmount-stalwart-token-test")).unwrap_err();
        assert!(
            err.contains("unreadable") || err.contains("fail-closed"),
            "expected fail-closed: {err}"
        );
    }

    /// Named contract: token file reads first non-empty non-comment line.
    #[test]
    fn token_from_file_reads_first_line() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("surmount-token-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("token");
        std::fs::write(&path, "# comment\n\nsecret-token-value\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms).unwrap();
        let t = token_from_file(&path).unwrap();
        assert_eq!(t, "secret-token-value");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// Named contract: jmap request uses management Account methods + stalwart capability.
    #[test]
    fn jmap_account_list_request_shape() {
        let body = jmap_account_list_request();
        let s = body.to_string();
        assert!(s.contains("x:Account/query"));
        assert!(s.contains("x:Account/get"));
        assert!(s.contains("urn:stalwart:jmap"));
        assert!(s.contains("urn:ietf:params:jmap:core"));
    }

    /// Named contract (Issue 1): comment-only token file is fail-closed (no # as Bearer).
    #[test]
    fn token_from_file_comment_only_fail_closed() {
        let dir = std::env::temp_dir().join(format!(
            "surmount-token-comment-only-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("token");
        std::fs::write(&path, "# only\n# still comment\n\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o600);
            std::fs::set_permissions(&path, perms).unwrap();
        }
        let err = token_from_file(&path).unwrap_err();
        assert!(
            err.contains("no token line") || err.contains("fail-closed"),
            "comment-only must fail-closed, got: {err}"
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// Named contract (Issue 2): truncate never panics on multi-byte UTF-8 at cut.
    #[test]
    fn truncate_for_note_utf8_safe_mid_codepoint() {
        // Build a string longer than 200 bytes where a multi-byte char straddles 200.
        let mut s = String::new();
        while s.len() < 195 {
            s.push('a');
        }
        s.push('\u{1F600}'); // 4-byte emoji; if max=200 and cut mid-codepoint, panic
        s.push_str("tail-more-content-and-secret-body");
        assert!(
            s.len() > 200,
            "fixture must exceed 200 bytes, got {}",
            s.len()
        );
        let out = truncate_for_note(&s, 200);
        assert!(out.ends_with("...") || out.len() <= 203);
        // Must not panic; must not include arbitrary long tail secrets blindly if truncated.
        assert!(out.len() <= s.len());
    }

    /// Named contract (Issue 10): unknown directory mode is fail-closed.
    #[test]
    fn parse_directory_backend_unknown_fail_closed() {
        let err = parse_directory_backend("bogus-backend").unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("not a known"),
            "unknown mode must fail-closed: {err}"
        );
        assert_eq!(
            parse_directory_backend("").unwrap(),
            DirectoryBackendKind::Unavailable
        );
        assert_eq!(
            parse_directory_backend("unavailable").unwrap(),
            DirectoryBackendKind::Unavailable
        );
        assert_eq!(
            parse_directory_backend("mock").unwrap(),
            DirectoryBackendKind::Mock
        );
        assert_eq!(
            parse_directory_backend("stalwart").unwrap(),
            DirectoryBackendKind::Stalwart
        );
    }

    /// Named contract: empty stalwart base URL fail-closed.
    #[test]
    fn require_stalwart_base_url_empty_fail_closed() {
        let err = require_stalwart_base_url("  ").unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("non-empty"),
            "{err}"
        );
        assert_eq!(
            require_stalwart_base_url("http://127.0.0.1:8080/").unwrap(),
            "http://127.0.0.1:8080"
        );
    }

    /// Named contract (Issue 5): live directory + auth off refuses without lab escape.
    #[test]
    fn require_stalwart_auth_coupling_fail_closed_when_auth_off() {
        use surmount_management_ui::auth::AuthMode;
        let err = require_stalwart_auth_coupling(AuthMode::Off, false).unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("nostr"),
            "expected auth coupling fail-closed: {err}"
        );
        assert!(require_stalwart_auth_coupling(AuthMode::Nostr, false).is_ok());
        assert!(require_stalwart_auth_coupling(AuthMode::Off, true).is_ok());
    }

    /// Named contract (Issue 4): world-readable token file fail-closed.
    #[test]
    fn token_from_file_group_readable_fail_closed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("surmount-token-mode-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("token");
        std::fs::write(&path, "secret-token\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
        let err = token_from_file(&path).unwrap_err();
        assert!(
            err.contains("group/world") || err.contains("fail-closed") || err.contains("mode"),
            "group/world readable must fail-closed: {err}"
        );
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    /// Named contract (Issue 3): HTTP error notes are status-code only (no body leak).
    #[tokio::test]
    async fn stalwart_directory_http_error_note_has_no_body() {
        let app = Router::new().route(
            "/jmap",
            post(|| async {
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from(
                        r#"{"error":"leaked-secret-xyz","detail":"nope"}"#,
                    ))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "bad-token");
        let inv = dir.list_accounts().await;
        assert_eq!(inv.source, "stalwart");
        assert!(inv.accounts.is_empty());
        assert!(
            inv.note.contains("401") || inv.note.to_ascii_lowercase().contains("http"),
            "note should mention HTTP status: {}",
            inv.note
        );
        assert!(
            !inv.note.contains("leaked-secret-xyz"),
            "client note must not include upstream body: {}",
            inv.note
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: unavailable mutations fail closed (no invent success).
    #[tokio::test]
    async fn unavailable_directory_mutations_fail_closed() {
        let create = UnavailableDirectory
            .create_account(CreateAccountInput {
                name: "alice".into(),
                domain_id: "d1".into(),
                description: None,
            })
            .await;
        assert!(!create.ok);
        assert!(create.account.is_none());
        assert_eq!(create.source, "unavailable");
        let update = UnavailableDirectory
            .update_account(UpdateAccountInput {
                id: "x".into(),
                description: Some("n".into()),
            })
            .await;
        assert!(!update.ok);
        assert!(update.account.is_none());
        let pw = UnavailableDirectory
            .set_mailbox_password(SetMailboxPasswordInput {
                mailbox: "alice@example.test".into(),
                password: concat!("unit-test-only-", "secret").into(),
            })
            .await;
        assert!(!pw.ok);
        assert_eq!(pw.source, "unavailable");
        assert!(!pw.note.contains("unit-test-only-secret"));
    }

    /// Named contract: mock create/update mutates fixture without claiming stalwart.
    #[tokio::test]
    async fn mock_directory_create_and_update() {
        let dir = MockDirectory::fixture();
        let before = dir.list_accounts().await.accounts.len();
        let created = dir
            .create_account(CreateAccountInput {
                name: "newop".into(),
                domain_id: "mock-domain".into(),
                description: Some("ops".into()),
            })
            .await;
        assert!(created.ok, "create should succeed: {}", created.note);
        assert_eq!(created.source, "mock");
        let acc = created.account.expect("created account");
        assert_eq!(acc.id, "mock-newop");
        assert!(acc.address.contains("mock"));
        assert!(!acc.address.starts_with("admin@"));
        let after = dir.list_accounts().await;
        assert_eq!(after.accounts.len(), before + 1);
        assert_eq!(after.source, "mock");

        let updated = dir
            .update_account(UpdateAccountInput {
                id: acc.id.clone(),
                description: Some("ops-2".into()),
            })
            .await;
        assert!(updated.ok, "update should succeed: {}", updated.note);
        assert_eq!(updated.source, "mock");

        // Align with live: empty description patch fails closed (no silent no-op success).
        let empty_patch = dir
            .update_account(UpdateAccountInput {
                id: acc.id.clone(),
                description: None,
            })
            .await;
        assert!(
            !empty_patch.ok,
            "empty description patch must fail-closed like Stalwart: {}",
            empty_patch.note
        );
        assert!(
            empty_patch
                .note
                .to_ascii_lowercase()
                .contains("description"),
            "note should mention description patch: {}",
            empty_patch.note
        );

        let dup = dir
            .create_account(CreateAccountInput {
                name: "newop".into(),
                domain_id: "mock-domain".into(),
                description: None,
            })
            .await;
        assert!(!dup.ok, "duplicate create must fail-closed");
    }

    /// Named contract: lab-escape flag SoT accepts the same truthy tokens as config env_bool
    /// (including `on`), so process-start coupling and mutation config cannot drift.
    #[test]
    fn directory_allow_unauthenticated_flag_sot_matches_env_bool_tokens() {
        use crate::config::parse_env_flag_truthy;
        for tok in ["1", "true", "TRUE", "yes", "on", "On", "  on  "] {
            assert!(
                parse_env_flag_truthy(tok),
                "token {tok:?} must be truthy (shared SoT)"
            );
        }
        for tok in ["0", "false", "no", "off", "", "maybe"] {
            assert!(!parse_env_flag_truthy(tok), "token {tok:?} must be falsey");
        }
        // directory_allow_unauthenticated_from_env must use the same parser path
        // (compile-time/docs contract: body calls parse_env_flag_truthy).
        let _ = directory_allow_unauthenticated_from_env;
    }

    /// Named contract: mutation auth coupling fails closed when auth off without lab escape.
    #[test]
    fn require_mutation_auth_coupling_fail_closed_when_auth_off() {
        let err = require_mutation_auth_coupling(AuthMode::Off, false).unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("nostr"),
            "expected mutation auth fail-closed: {err}"
        );
        assert!(require_mutation_auth_coupling(AuthMode::Nostr, false).is_ok());
        assert!(require_mutation_auth_coupling(AuthMode::Off, true).is_ok());
    }

    /// Named contract: jmap create request shape uses x:Account/set and no password fields.
    #[test]
    fn jmap_account_create_request_shape_no_secrets() {
        let body = jmap_account_create_request(&CreateAccountInput {
            name: "alice".into(),
            domain_id: "dom-1".into(),
            description: Some("desk".into()),
        });
        let s = body.to_string();
        assert!(s.contains("x:Account/set"));
        assert!(s.contains("alice"));
        assert!(s.contains("dom-1"));
        assert!(s.contains("desk"));
        assert!(!s.contains("password") && !s.contains("nsec") && !s.contains("secret"));
        assert!(s.contains("urn:stalwart:jmap"));
    }

    /// Named contract: live create maps wire-mock set response (no body leak on error).
    #[tokio::test]
    async fn stalwart_directory_create_from_jmap_wire_mock() {
        let app = Router::new().route(
            "/jmap",
            post(|req: Request| async move {
                let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
                    .await
                    .unwrap_or_default();
                let incoming: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
                let s = incoming.to_string();
                assert!(s.contains("x:Account/set") || s.contains("Account/set"));
                let body = json!({
                    "methodResponses": [
                        ["x:Account/set", {
                            "created": {
                                "new1": {
                                    "id": "live-created-1",
                                    "name": "alice",
                                    "emailAddress": "alice@example.test",
                                    "@type": "User"
                                }
                            },
                            "notCreated": {}
                        }, "c1"]
                    ]
                });
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "test-token");
        let result = dir
            .create_account(CreateAccountInput {
                name: "alice".into(),
                domain_id: "d1".into(),
                description: None,
            })
            .await;
        assert!(result.ok, "create should succeed: {}", result.note);
        assert_eq!(result.source, "stalwart");
        let acc = result.account.expect("account");
        assert_eq!(acc.id, "live-created-1");
        assert_eq!(acc.address, "alice@example.test");
        assert!(!result.note.contains("admin@"));
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: live create fail-closed on HTTP error (status only, no body).
    #[tokio::test]
    async fn stalwart_directory_create_fail_closed_on_http_error() {
        let app = Router::new().route(
            "/jmap",
            post(|| async {
                Response::builder()
                    .status(StatusCode::FORBIDDEN)
                    .body(Body::from(r#"{"error":"leaked-secret-create"}"#))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "tok");
        let result = dir
            .create_account(CreateAccountInput {
                name: "bob".into(),
                domain_id: "d1".into(),
                description: None,
            })
            .await;
        assert!(!result.ok);
        assert!(result.account.is_none());
        assert_eq!(result.source, "stalwart");
        assert!(
            !result.note.contains("leaked-secret-create"),
            "must not reflect body: {}",
            result.note
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: live update maps wire-mock set updated map.
    #[tokio::test]
    async fn stalwart_directory_update_from_jmap_wire_mock() {
        let app = Router::new().route(
            "/jmap",
            post(|| async {
                let body = json!({
                    "methodResponses": [
                        ["x:Account/set", {
                            "updated": { "live-1": null },
                            "notUpdated": {}
                        }, "u1"]
                    ]
                });
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartDirectory::new(http, format!("http://{addr}"), "tok");
        let result = dir
            .update_account(UpdateAccountInput {
                id: "live-1".into(),
                description: Some("patched".into()),
            })
            .await;
        assert!(result.ok, "update should succeed: {}", result.note);
        assert_eq!(result.source, "stalwart");
        assert_eq!(
            result.account.as_ref().map(|a| a.id.as_str()),
            Some("live-1")
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: password PATCH targets mailbox Account credentials, not AccountPassword.
    ///
    /// Surmount sends the plaintext secret. Stalwart 0.16.15 hashes it with
    /// Authentication.passwordHashAlgorithm (default Argon2id). We do not
    /// pre-hash in Axum.
    #[test]
    fn jmap_account_set_password_request_targets_account_not_singleton() {
        let body = jmap_account_set_password_request("c", "unit-test-only-secret");
        let s = body.to_string();
        assert!(s.contains("x:Account/set"));
        assert!(!s.contains("AccountPassword"));
        assert!(s.contains("Password"));
        assert!(s.contains("unit-test-only-secret"));
        assert!(
            !s.contains("$argon2"),
            "must send plaintext secret; engine hashes (default Argon2id)"
        );
        assert!(s.contains("\"c\""));
    }

    /// Named contract: Debug of password input never includes the secret.
    #[test]
    fn set_mailbox_password_input_debug_redacts_secret() {
        let input = SetMailboxPasswordInput {
            mailbox: "hunter@surmount.systems".into(),
            password: concat!("unit-test-only-", "secret").into(),
        };
        let dbg = format!("{input:?}");
        assert!(dbg.contains("hunter@surmount.systems"));
        assert!(dbg.contains("[redacted]"));
        assert!(!dbg.contains("unit-test-only-secret"));
    }

    /// Named contract: mock password set by email; secret never in result note.
    #[tokio::test]
    async fn mock_set_mailbox_password_by_email() {
        let dir = MockDirectory::fixture();
        let ok = dir
            .set_mailbox_password(SetMailboxPasswordInput {
                mailbox: "fixture-operator@mock.surmount.test".into(),
                password: concat!("unit-test-only-", "secret").into(),
            })
            .await;
        assert!(ok.ok, "{}", ok.note);
        assert_eq!(ok.source, "mock");
        assert_eq!(
            ok.mailbox.as_deref(),
            Some("fixture-operator@mock.surmount.test")
        );
        assert!(!ok.note.contains("unit-test-only-secret"));
        let blob = serde_json::to_string(&ok).unwrap();
        assert!(!blob.contains("unit-test-only-secret"));

        let missing = dir
            .set_mailbox_password(SetMailboxPasswordInput {
                mailbox: "nobody@mock.surmount.test".into(),
                password: concat!("unit-test-only-", "secret").into(),
            })
            .await;
        assert!(!missing.ok);
        assert!(missing.note.contains("No mailbox"));
        assert!(!missing.note.contains("unit-test-only-secret"));
    }

    /// Named contract: listing unavailable + token still sets password via Stalwart.
    #[tokio::test]
    async fn stalwart_password_only_lists_empty_but_sets_password() {
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen_h = seen.clone();
        let app = Router::new().route(
            "/jmap",
            post(move |req: Request| {
                let seen = seen_h.clone();
                async move {
                    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
                        .await
                        .unwrap_or_default();
                    let incoming: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
                    let s = incoming.to_string();
                    seen.lock().await.push(s.clone());
                    let is_set = s.contains("x:Account/set") || s.contains("Account/set");
                    let body = if is_set {
                        json!({
                            "methodResponses": [
                                ["x:Account/set", {
                                    "updated": { "c": null },
                                    "notUpdated": {}
                                }, "u1"]
                            ]
                        })
                    } else {
                        json!({
                            "methodResponses": [
                                ["x:Account/query", {"ids": ["c"]}, "q1"],
                                ["x:Account/get", {
                                    "list": [{
                                        "id": "c",
                                        "name": "hunter",
                                        "emailAddress": "hunter@surmount.systems",
                                        "@type": "User"
                                    }]
                                }, "g1"]
                            ]
                        })
                    };
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartPasswordOnly::new(http, format!("http://{addr}"), "test-token");
        let listed = dir.list_accounts().await;
        assert_eq!(listed.source, "unavailable");
        assert!(listed.accounts.is_empty());

        let result = dir
            .set_mailbox_password(SetMailboxPasswordInput {
                mailbox: "hunter@surmount.systems".into(),
                password: concat!("unit-test-only-", "secret").into(),
            })
            .await;
        assert!(result.ok, "{}", result.note);
        assert_eq!(result.source, "stalwart");
        assert_eq!(result.mailbox.as_deref(), Some("hunter@surmount.systems"));
        assert!(!result.note.contains("unit-test-only-secret"));
        assert!(!result.note.contains("AccountPassword"));

        let calls = seen.lock().await;
        assert!(
            calls.iter().any(|c| c.contains("x:Account/set")
                && c.contains("Password")
                && c.contains("unit-test-only-secret")
                && !c.contains("AccountPassword")),
            "expected Account credential PATCH; calls={calls:?}"
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: Stalwart 0.16.15 management JMAP is POST /jmap.
    ///
    /// Live engine returns HTTP 404 problem+json on POST /api for the same
    /// x:Account/query body. Password set must use /jmap (not /api, not
    /// x:AccountPassword/set). Fixture of the live 404 path.
    #[tokio::test]
    async fn set_mailbox_password_uses_jmap_when_api_returns_404() {
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen_h = seen.clone();
        let app = Router::new()
            .route(
                "/api",
                post(|| async {
                    // Live 0.16.15 shape: type/status/title/detail (no JMAP).
                    let body = json!({
                        "type": "about:blank",
                        "status": 404,
                        "title": "Not Found",
                        "detail": "fixture-not-a-secret"
                    });
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap()
                }),
            )
            .route(
                "/jmap",
                post(move |req: Request| {
                    let seen = seen_h.clone();
                    async move {
                        let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
                            .await
                            .unwrap_or_default();
                        let incoming: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
                        let s = incoming.to_string();
                        seen.lock().await.push(s.clone());
                        let is_set = s.contains("x:Account/set") || s.contains("Account/set");
                        let body = if is_set {
                            json!({
                                "methodResponses": [
                                    ["x:Account/set", {
                                        "updated": { "c": null },
                                        "notUpdated": {}
                                    }, "u1"]
                                ]
                            })
                        } else {
                            json!({
                                "methodResponses": [
                                    ["x:Account/query", {"ids": ["c"]}, "q1"],
                                    ["x:Account/get", {
                                        "list": [{
                                            "id": "c",
                                            "name": "hunter",
                                            "emailAddress": "hunter@surmount.systems",
                                            "@type": "User"
                                        }]
                                    }, "g1"]
                                ]
                            })
                        };
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_TYPE, "application/json")
                            .body(Body::from(body.to_string()))
                            .unwrap()
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartPasswordOnly::new(http, format!("http://{addr}"), "test-token");
        let result = dir
            .set_mailbox_password(SetMailboxPasswordInput {
                mailbox: "hunter@surmount.systems".into(),
                password: concat!("unit-test-only-", "secret").into(),
            })
            .await;
        assert!(
            result.ok,
            "password set must succeed via /jmap when /api 404s: {}",
            result.note
        );
        assert_eq!(result.source, "stalwart");
        assert_eq!(result.mailbox.as_deref(), Some("hunter@surmount.systems"));
        assert!(
            !result.note.contains("404"),
            "must not surface the /api 404: {}",
            result.note
        );
        assert!(!result.note.contains("unit-test-only-secret"));
        assert!(!result.note.contains("AccountPassword"));
        assert!(!result.note.contains("fixture-not-a-secret"));
        let calls = seen.lock().await;
        assert!(
            calls.iter().any(|c| c.contains("x:Account/query")
                || c.contains("Account/query")
                || c.contains("x:Account/get")),
            "expected lookup on /jmap; calls={calls:?}"
        );
        assert!(
            calls.iter().any(|c| c.contains("x:Account/set")
                && c.contains("Password")
                && c.contains("unit-test-only-secret")
                && !c.contains("AccountPassword")),
            "expected Account credential PATCH on /jmap; calls={calls:?}"
        );
        serve.abort();
        let _ = serve.await;
    }

    /// Named contract: local-part charset, reserved admin, no @.
    #[test]
    fn normalize_mailbox_local_part_refuses_admin_and_bad_charset() {
        assert_eq!(normalize_mailbox_local_part("Person").unwrap(), "person");
        assert_eq!(normalize_mailbox_local_part("a.b_c-1").unwrap(), "a.b_c-1");
        let admin = normalize_mailbox_local_part("admin").unwrap_err();
        assert!(admin.contains("reserved"), "{admin}");
        let at = normalize_mailbox_local_part("a@b").unwrap_err();
        assert!(at.contains('@') || at.contains("local-part"), "{at}");
        let bad = normalize_mailbox_local_part("no spaces").unwrap_err();
        assert!(bad.contains("ASCII") || bad.contains("local-part"), "{bad}");
        assert!(normalize_mailbox_local_part("").is_err());
    }

    /// Named contract: mock create refuses reserved admin before writing.
    #[tokio::test]
    async fn mock_create_refuses_reserved_admin() {
        let dir = MockDirectory::fixture();
        let before = dir.list_accounts().await.accounts.len();
        let created = dir
            .create_account(CreateAccountInput {
                name: "admin".into(),
                domain_id: "mock-domain".into(),
                description: None,
            })
            .await;
        assert!(!created.ok);
        assert!(created.note.contains("reserved"), "{}", created.note);
        assert_eq!(dir.list_accounts().await.accounts.len(), before);
    }

    /// Named contract: password-only create hits /jmap; list stays empty.
    #[tokio::test]
    async fn stalwart_password_only_create_hits_jmap_list_stays_empty() {
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen_h = seen.clone();
        let app = Router::new().route(
            "/jmap",
            post(move |req: Request| {
                let seen = seen_h.clone();
                async move {
                    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
                        .await
                        .unwrap_or_default();
                    let incoming: Value = serde_json::from_slice(&bytes).unwrap_or(json!({}));
                    let s = incoming.to_string();
                    seen.lock().await.push(s.clone());
                    let is_create = s.contains("x:Account/set") && s.contains("new1");
                    let is_domain = s.contains("x:Domain/");
                    let body = if is_create {
                        json!({
                            "methodResponses": [
                                ["x:Account/set", {
                                    "created": {
                                        "new1": {
                                            "id": "z",
                                            "name": "person",
                                            "emailAddress": "person@example.test",
                                            "@type": "User"
                                        }
                                    }
                                }, "c1"]
                            ]
                        })
                    } else if is_domain {
                        json!({
                            "methodResponses": [
                                ["x:Domain/query", {"ids": ["b"]}, "dq1"],
                                ["x:Domain/get", {
                                    "list": [{
                                        "id": "b",
                                        "name": "example.test"
                                    }]
                                }, "dg1"]
                            ]
                        })
                    } else {
                        json!({
                            "methodResponses": [
                                ["x:Account/query", {"ids": []}, "q1"],
                                ["x:Account/get", { "list": [] }, "g1"]
                            ]
                        })
                    };
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve = tokio::spawn(async move {
            axum::serve(listener, app).await.ok();
        });
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap();
        let dir = StalwartPasswordOnly::new(http, format!("http://{addr}"), "test-token");
        let listed = dir.list_accounts().await;
        assert_eq!(listed.source, "unavailable");
        assert!(listed.accounts.is_empty());

        let domain = dir.lookup_domain_id("example.test").await;
        assert!(domain.ok, "{}", domain.note);
        assert_eq!(domain.domain_id.as_deref(), Some("b"));

        let created = dir
            .create_account(CreateAccountInput {
                name: "person".into(),
                domain_id: domain.domain_id.unwrap(),
                description: None,
            })
            .await;
        assert!(created.ok, "{}", created.note);
        assert_eq!(created.source, "stalwart");
        let listed_after = dir.list_accounts().await;
        assert_eq!(listed_after.source, "unavailable");
        assert!(listed_after.accounts.is_empty());

        let calls = seen.lock().await;
        assert!(
            calls.iter().any(|c| c.contains("x:Account/set")
                && c.contains("\"@type\":\"User\"")
                && !c.contains("AccountPassword")
                && !c.contains("nsec")),
            "create must hit /jmap User set; calls={calls:?}"
        );
        serve.abort();
        let _ = serve.await;
    }

    #[test]
    fn domain_id_from_jmap_matches_name() {
        let body = json!({
            "methodResponses": [
                ["x:Domain/query", {"ids": ["b"]}, "dq1"],
                ["x:Domain/get", {
                    "list": [
                        {"id": "a", "name": "other.test"},
                        {"id": "b", "name": "Example.TEST"}
                    ]
                }, "dg1"]
            ]
        });
        assert_eq!(
            domain_id_from_jmap(&body, "example.test")
                .unwrap()
                .as_deref(),
            Some("b")
        );
        assert_eq!(domain_id_from_jmap(&body, "missing.test").unwrap(), None);
    }
}
