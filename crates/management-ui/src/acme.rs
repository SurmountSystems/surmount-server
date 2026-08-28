//! In-process ACME (Let's Encrypt-capable) for management-ui HTTPS.
//!
//! **Default off.** CI and hermetic tests never require a live ACME directory
//! or real DNS. Product challenge for this slice is **DNS-01** (fits
//! redirect-only product :80). HTTP-01 on product :80 remains parked (Q-EDGE).
//! TLS-ALPN-01 is residual (ALPN routing on the rustls acceptor).
//!
//! # What `DnsProvider` / `SURMOUNT_ACME_DNS_PROVIDER` means
//!
//! In this crate **`DnsProvider` is the ACME DNS-01 *challenge adapter***: the
//! component that creates and deletes `_acme-challenge` TXT records so a CA can
//! validate domain control. It is **not** Surmount as a commercial DNS product
//! brand, and it is **not** a mandatory third-party DNS vendor (Cloudflare,
//! Route53, etc.).
//!
//! Surmount runs on an **operator-chosen host** with DNS under **operator
//! control** (self-hosted or whatever the operator already runs). The first
//! non-mock adapter is therefore **neutral**:
//!
//! - `none` (default): reuse valid host PEMs only; no issuance path.
//! - `mock` / `test` / `lab`: hermetic self-signed issuer (not live LE; refused
//!   against production Let's Encrypt directory).
//! - `external-hook` / `hook`: run an **operator-owned absolute executable**
//!   (`SURMOUNT_ACME_DNS_HOOK`) with argv `set|clear|wait` (see
//!   [`ExternalHookDnsProvider`]). The hook talks to **your** DNS (nsupdate,
//!   your API, etc.). No commercial DNS SDK crates in this tree.
//!
//! # Why external-hook first
//!
//! We are the stack/provider for the mail/services host; we do not assume a
//! specific public DNS vendor. An executable hook keeps DNS credentials and
//! API choice on the host (never in git), stays fail-closed without shell
//! interpolation, and is hermetically testable with a temp script.
//!
//! # Early renew
//!
//! When ACME is enabled, leaf PEMs are **not** reused once remaining lifetime
//! is under `SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY` (scaffold default **30**
//! days, common Let's Encrypt operator practice; not locked CA law).
//!
//! # Residual (not this module alone)
//!
//! Operator host enable + live LE cutover, PEM hot-reload without restart,
//! TLS-ALPN-01, multi-service shared cert / mail-plane share (Q-EDGE-1).
//!
//! Crate choice: **`instant-acme` 0.8** (async pure-Rust ACME client; rustls
//! 0.23 / aws-lc-rs default features; maintained by djc/cpu). We own challenge
//! wiring via a mockable [`DnsProvider`] trait rather than coupling to a
//! higher-level rustls-acme acceptor. Static PEM load path remains first-class.
//!
//! On successful issuance (real or mock), cert + key PEMs are written to the
//! configured host paths (same shape as `SURMOUNT_TLS_CERT` / `SURMOUNT_TLS_KEY`)
//! with fail-closed private key mode (`mode & 0o077 == 0`).
//!
//! Startup honesty: PEMs load once; renew requires process restart (hot-reload
//! residual). Secrets (account credentials, PEMs) never in git.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::tls::{
    SecretFileModeStatus, TlsPaths, require_owner_only_secret_file, secret_file_mode_status,
};

/// Default ACME is off (product + CI).
pub const DEFAULT_ACME_ENABLE: bool = false;

/// Scaffold default: reissue when remaining leaf lifetime is under this many days.
///
/// Common Let's Encrypt operator practice (~30 days before expiry). Not locked
/// CA policy; override with `SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY`.
pub const DEFAULT_ACME_RENEW_DAYS_BEFORE_EXPIRY: u64 = 30;

/// Default timeout for one external-hook DNS-01 invocation (seconds).
pub const DEFAULT_ACME_DNS_HOOK_TIMEOUT_SECS: u64 = 60;

/// Minimum allowed hook timeout (seconds).
pub const MIN_ACME_DNS_HOOK_TIMEOUT_SECS: u64 = 1;

/// Maximum allowed hook timeout (seconds). Caps self-DoS from huge config.
pub const MAX_ACME_DNS_HOOK_TIMEOUT_SECS: u64 = 600;

/// Hook `wait` exit code: command not implemented; treat as success after `set`.
///
/// Operator must block inside `set` until the TXT is publicly visible when
/// `wait` is unsupported.
pub const HOOK_WAIT_UNSUPPORTED_EXIT: i32 = 2;

/// Fixed minimal PATH for hook children after `env_clear` (no parent secrets).
///
/// Enough for common shebang interpreters and host tools on NixOS/FHS. Hooks
/// that need more must use absolute paths or a wrapper that sets its own env.
pub const HOOK_MINIMAL_PATH: &str =
    "/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/usr/bin:/bin";

/// Challenge type for this product slice. HTTP-01 is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcmeChallenge {
    /// DNS-01 (`_acme-challenge.<domain>` TXT). Primary for redirect-only :80.
    Dns01,
}

impl AcmeChallenge {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "dns-01" | "dns01" | "dns" => Ok(Self::Dns01),
            "http-01" | "http01" | "http" => Err(
                "ACME HTTP-01 on product :80 is parked (Q-EDGE). Use dns-01 \
                 (SURMOUNT_ACME_CHALLENGE=dns-01) or external host PEMs"
                    .into(),
            ),
            "tls-alpn-01" | "tls-alpn" | "alpn" => Err(
                "ACME TLS-ALPN-01 is residual for this slice (not wired). Use \
                 dns-01 or external host PEMs"
                    .into(),
            ),
            other => Err(format!(
                "unknown SURMOUNT_ACME_CHALLENGE {other:?}; only dns-01 is \
                 supported in this build"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dns01 => "dns-01",
        }
    }
}

/// How DNS-01 TXT records are published (ACME challenge adapter, not a DNS brand).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DnsProviderKind {
    /// No DNS-01 adapter. Issuance fails closed unless PEMs already usable.
    #[default]
    None,
    /// Hermetic / lab mock only. Never talks to a public ACME directory.
    Mock,
    /// Operator-owned host executable (`SURMOUNT_ACME_DNS_HOOK`); see
    /// [`ExternalHookDnsProvider`].
    ExternalHook,
}

impl DnsProviderKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "none" | "off" => Ok(Self::None),
            "mock" | "test" | "lab" => Ok(Self::Mock),
            "external-hook" | "external_hook" | "hook" => Ok(Self::ExternalHook),
            other => Err(format!(
                "unknown SURMOUNT_ACME_DNS_PROVIDER {other:?}; use none, mock, or \
                 external-hook (DNS-01 challenge adapter; not a commercial DNS brand)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Mock => "mock",
            Self::ExternalHook => "external-hook",
        }
    }
}

/// Runtime ACME configuration (env / Nix). Default everything off / empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcmeConfig {
    pub enable: bool,
    /// ACME directory URL (e.g. Let's Encrypt staging or production).
    pub directory_url: String,
    /// Contact email for account registration (empty when disabled).
    pub email: String,
    /// Identifiers to request on the order.
    pub domains: Vec<String>,
    /// Host path for serialized ACME account credentials JSON (never in git).
    pub account_credentials_path: PathBuf,
    pub challenge: AcmeChallenge,
    pub dns_provider: DnsProviderKind,
    /// Absolute path to operator DNS-01 hook executable (`external-hook` only).
    pub dns_hook_path: PathBuf,
    /// Timeout for one hook invocation (seconds).
    pub dns_hook_timeout_secs: u64,
    /// Reissue when remaining leaf lifetime is under this many days.
    pub renew_days_before_expiry: u64,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enable: DEFAULT_ACME_ENABLE,
            directory_url: String::new(),
            email: String::new(),
            domains: Vec::new(),
            account_credentials_path: PathBuf::new(),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::None,
            dns_hook_path: PathBuf::new(),
            dns_hook_timeout_secs: DEFAULT_ACME_DNS_HOOK_TIMEOUT_SECS,
            renew_days_before_expiry: DEFAULT_ACME_RENEW_DAYS_BEFORE_EXPIRY,
        }
    }
}

impl AcmeConfig {
    /// Parse from process environment. Default-off; no live directory required.
    pub fn from_env() -> Result<Self, String> {
        Self::from_env_map(|k| std::env::var(k).ok())
    }

    /// Testable parse: `get` returns env values.
    pub fn from_env_map<F>(mut get: F) -> Result<Self, String>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let enable = match get("SURMOUNT_ACME_ENABLE") {
            Some(v) => parse_truthy(&v),
            None => DEFAULT_ACME_ENABLE,
        };

        let directory_url = get("SURMOUNT_ACME_DIRECTORY")
            .unwrap_or_default()
            .trim()
            .to_string();
        let email = get("SURMOUNT_ACME_EMAIL")
            .unwrap_or_default()
            .trim()
            .to_string();
        let domains = parse_domains(get("SURMOUNT_ACME_DOMAINS").unwrap_or_default().as_str());
        let account_credentials_path = PathBuf::from(
            get("SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH")
                .unwrap_or_default()
                .trim(),
        );
        let challenge = match get("SURMOUNT_ACME_CHALLENGE") {
            Some(v) if !v.trim().is_empty() => AcmeChallenge::parse(&v)?,
            _ => AcmeChallenge::Dns01,
        };
        let dns_provider = match get("SURMOUNT_ACME_DNS_PROVIDER") {
            Some(v) if !v.trim().is_empty() => DnsProviderKind::parse(&v)?,
            _ => DnsProviderKind::None,
        };
        let dns_hook_path = PathBuf::from(get("SURMOUNT_ACME_DNS_HOOK").unwrap_or_default().trim());
        let dns_hook_timeout_secs = match get("SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS") {
            Some(v) if !v.trim().is_empty() => parse_hook_timeout_secs(&v)?,
            _ => DEFAULT_ACME_DNS_HOOK_TIMEOUT_SECS,
        };
        let renew_days_before_expiry = match get("SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY") {
            Some(v) if !v.trim().is_empty() => {
                parse_u64_env("SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY", &v)?
            }
            _ => DEFAULT_ACME_RENEW_DAYS_BEFORE_EXPIRY,
        };

        let cfg = Self {
            enable,
            directory_url,
            email,
            domains,
            account_credentials_path,
            challenge,
            dns_provider,
            dns_hook_path,
            dns_hook_timeout_secs,
            renew_days_before_expiry,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Fail closed when enable is set with incomplete required fields.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enable {
            return Ok(());
        }
        if self.domains.is_empty() {
            return Err(
                "SURMOUNT_ACME_ENABLE is on but SURMOUNT_ACME_DOMAINS is empty \
                 (comma-separated hostnames required; fail-closed)"
                    .into(),
            );
        }
        for d in &self.domains {
            if d.contains('*') {
                return Err(format!(
                    "wildcard ACME domain {d:?} is not supported in this build \
                     (DNS-01 TXT name would be wrong for *.host; refuse until \
                     wildcard path is designed; fail-closed)"
                ));
            }
        }
        if self.email.is_empty() {
            return Err(
                "SURMOUNT_ACME_ENABLE is on but SURMOUNT_ACME_EMAIL is empty \
                 (ACME contact required; fail-closed)"
                    .into(),
            );
        }
        if self.directory_url.is_empty() {
            return Err(
                "SURMOUNT_ACME_ENABLE is on but SURMOUNT_ACME_DIRECTORY is empty \
                 (use Let's Encrypt staging first; fail-closed)"
                    .into(),
            );
        }
        // Live ACME directory over cleartext HTTP is refused (MITM once issuer runs).
        if !self.directory_url.starts_with("https://") {
            return Err(format!(
                "SURMOUNT_ACME_DIRECTORY must be an https:// URL (got {:?}; fail-closed)",
                self.directory_url
            ));
        }
        if path_is_blank(&self.account_credentials_path) {
            return Err(
                "SURMOUNT_ACME_ENABLE is on but SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH \
                 is empty (host path for account JSON; never in git; fail-closed)"
                    .into(),
            );
        }
        if !self.account_credentials_path.is_absolute() {
            return Err(format!(
                "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH must be an absolute host path \
                 (got {:?}; never in git; fail-closed)",
                self.account_credentials_path
            ));
        }
        // Challenge already refused HTTP-01 / TLS-ALPN at parse.
        if self.challenge != AcmeChallenge::Dns01 {
            return Err("only ACME DNS-01 is supported in this build".into());
        }
        // Lab mock must never target production Let's Encrypt (fail closed).
        if self.dns_provider == DnsProviderKind::Mock
            && is_lets_encrypt_production_directory(&self.directory_url)
        {
            return Err(
                "SURMOUNT_ACME_DNS_PROVIDER=mock cannot use the production Let's Encrypt \
                 directory (lab/self-signed only; use staging or a non-production directory; \
                 fail-closed)"
                    .into(),
            );
        }
        // external-hook: absolute operator executable path required when enabled.
        if self.dns_provider == DnsProviderKind::ExternalHook {
            if path_is_blank(&self.dns_hook_path) {
                return Err(
                    "SURMOUNT_ACME_DNS_PROVIDER=external-hook requires SURMOUNT_ACME_DNS_HOOK \
                     (absolute path to operator-owned executable; never in git as a secret; \
                     fail-closed)"
                        .into(),
                );
            }
            if !self.dns_hook_path.is_absolute() {
                return Err(format!(
                    "SURMOUNT_ACME_DNS_HOOK must be an absolute host path \
                     (got {:?}; fail-closed)",
                    self.dns_hook_path
                ));
            }
            if !(MIN_ACME_DNS_HOOK_TIMEOUT_SECS..=MAX_ACME_DNS_HOOK_TIMEOUT_SECS)
                .contains(&self.dns_hook_timeout_secs)
            {
                return Err(format!(
                    "SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS must be in {}..={} (got {}; fail-closed)",
                    MIN_ACME_DNS_HOOK_TIMEOUT_SECS,
                    MAX_ACME_DNS_HOOK_TIMEOUT_SECS,
                    self.dns_hook_timeout_secs
                ));
            }
        }
        Ok(())
    }
}

fn parse_u64_env(name: &str, raw: &str) -> Result<u64, String> {
    raw.trim().parse::<u64>().map_err(|e| {
        format!(
            "{name} must be a non-negative integer (got {:?}: {e}; fail-closed)",
            raw.trim()
        )
    })
}

fn parse_hook_timeout_secs(raw: &str) -> Result<u64, String> {
    let v = parse_u64_env("SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS", raw)?;
    if !(MIN_ACME_DNS_HOOK_TIMEOUT_SECS..=MAX_ACME_DNS_HOOK_TIMEOUT_SECS).contains(&v) {
        return Err(format!(
            "SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS must be in {}..={} (got {}; fail-closed)",
            MIN_ACME_DNS_HOOK_TIMEOUT_SECS, MAX_ACME_DNS_HOOK_TIMEOUT_SECS, v
        ));
    }
    Ok(v)
}

/// True when the directory URL is production Let's Encrypt (not staging).
fn is_lets_encrypt_production_directory(url: &str) -> bool {
    let u = url.trim();
    u == directories::LETS_ENCRYPT_PRODUCTION
        || u.starts_with("https://acme-v02.api.letsencrypt.org/")
}

fn parse_truthy(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_domains(raw: &str) -> Vec<String> {
    raw.split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn path_is_blank(p: &Path) -> bool {
    p.as_os_str().is_empty()
}

/// DNS-01 TXT challenge adapter (not a commercial DNS brand).
///
/// Implementations publish `_acme-challenge` TXT so a CA can validate control.
/// First real (non-mock) product adapter: [`ExternalHookDnsProvider`].
pub trait DnsProvider: Send + Sync {
    fn set_txt<'a>(
        &'a self,
        fqdn: &'a str,
        value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

    fn clear_txt<'a>(
        &'a self,
        fqdn: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;

    /// Wait until the TXT record is visible for CA validation (propagation).
    ///
    /// Called after [`set_txt`] and **before** ACME `set_ready` so the CA does
    /// not race empty DNS. Mock may fail while "not yet propagated" for hermetic
    /// tests. External-hook: see [`ExternalHookDnsProvider`] wait contract.
    fn wait_propagated<'a>(
        &'a self,
        fqdn: &'a str,
        value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

/// In-memory DNS provider for hermetic tests and lab mock.
#[derive(Debug, Default)]
pub struct MockDnsProvider {
    records: Mutex<Vec<(String, String)>>,
    /// When > 0, next `wait_propagated` calls fail and decrement (propagation lag).
    remaining_propagation_fails: Mutex<u32>,
}

impl MockDnsProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hermetic: next `n` `wait_propagated` calls fail (then succeed if record set).
    #[cfg(test)]
    pub fn with_propagation_fails(n: u32) -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            remaining_propagation_fails: Mutex::new(n),
        }
    }

    /// Hermetic: set remaining `wait_propagated` failures mid-flight (multi-name tests).
    #[cfg(test)]
    pub fn set_propagation_fails(&self, n: u32) {
        *self
            .remaining_propagation_fails
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = n;
    }

    #[cfg(test)]
    pub fn records(&self) -> Vec<(String, String)> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl DnsProvider for MockDnsProvider {
    fn set_txt<'a>(
        &'a self,
        fqdn: &'a str,
        value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        let fqdn = fqdn.to_string();
        let value = value.to_string();
        Box::pin(async move {
            self.records
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((fqdn, value));
            Ok(())
        })
    }

    fn clear_txt<'a>(
        &'a self,
        fqdn: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        let fqdn = fqdn.to_string();
        Box::pin(async move {
            let mut g = self.records.lock().unwrap_or_else(|e| e.into_inner());
            g.retain(|(n, _)| n != &fqdn);
            Ok(())
        })
    }

    fn wait_propagated<'a>(
        &'a self,
        fqdn: &'a str,
        value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        let fqdn = fqdn.to_string();
        let value = value.to_string();
        Box::pin(async move {
            {
                let mut fails = self
                    .remaining_propagation_fails
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if *fails > 0 {
                    *fails -= 1;
                    return Err(format!(
                        "mock DNS not yet propagated for {fqdn} (hermetic lag)"
                    ));
                }
            }
            let g = self.records.lock().unwrap_or_else(|e| e.into_inner());
            let ok = g.iter().any(|(n, v)| n == &fqdn && v == &value);
            if ok {
                Ok(())
            } else {
                Err(format!(
                    "mock DNS TXT missing or wrong value for {fqdn} (not propagated)"
                ))
            }
        })
    }
}

/// Operator-owned executable DNS-01 challenge adapter (`external-hook` / `hook`).
///
/// # Protocol (argv only; no shell)
///
/// Path must be absolute, **regular file** (symlinks refused), executable, and
/// not group/world-writable. Re-validated before every spawn (TOCTOU reduce).
/// Invoked as:
///
/// | argv | Meaning | Success exit |
/// |------|---------|--------------|
/// | `set <fqdn> <txt_value>` | Publish TXT | 0 |
/// | `clear <fqdn>` | Remove TXT | 0 |
/// | `wait <fqdn> <txt_value>` | Optional propagation check | 0 = ready |
///
/// **`wait` contract:**
/// - exit **0**: TXT is visible; proceed.
/// - exit **2** ([`HOOK_WAIT_UNSUPPORTED_EXIT`]): hook does not implement wait;
///   treat as success immediately. Operator must block inside `set` until the
///   record is publicly visible before returning 0 from `set`.
/// - other non-zero: brief limited retries (not multi-minute DNS polling), then
///   error. Long propagation belongs inside the operator hook (`set` or a real
///   `wait` that blocks as long as needed within the hook timeout).
///
/// # Security
///
/// - Child env is **cleared**; only a fixed minimal [`HOOK_MINIMAL_PATH`] is set
///   (UI session secrets / tokens must not leak into the hook).
/// - stdout/stderr discarded (hooks must not rely on product logging credentials).
/// - Product logs exit code + verb/fqdn only (not stderr body).
/// - Challenge TXT value is public in DNS; do not put account secrets on argv.
/// - Prefer hook path outside ACME PEM `ReadWritePaths` parents (store or
///   read-only `/run` path). Ambient unit capabilities (e.g. CAP_NET_BIND_SERVICE)
///   may still be inherited by the child when the unit grants them; residual
///   (hooks should not need low-port bind).
#[derive(Debug, Clone)]
pub struct ExternalHookDnsProvider {
    hook_path: PathBuf,
    timeout: Duration,
}

impl ExternalHookDnsProvider {
    /// Validate path and build provider. Fail closed if not absolute / not a
    /// safe executable regular file / timeout out of range.
    pub fn new(hook_path: PathBuf, timeout_secs: u64) -> Result<Self, String> {
        validate_hook_executable(&hook_path)?;
        if !(MIN_ACME_DNS_HOOK_TIMEOUT_SECS..=MAX_ACME_DNS_HOOK_TIMEOUT_SECS)
            .contains(&timeout_secs)
        {
            return Err(format!(
                "ACME DNS hook timeout must be in {}..={} seconds (got {}; fail-closed)",
                MIN_ACME_DNS_HOOK_TIMEOUT_SECS, MAX_ACME_DNS_HOOK_TIMEOUT_SECS, timeout_secs
            ));
        }
        Ok(Self {
            hook_path,
            timeout: Duration::from_secs(timeout_secs),
        })
    }

    /// Child command after `env_clear`: PATH only. Parent session secret must
    /// not be inherited.
    fn hook_command(&self, args: &[&str]) -> std::process::Command {
        use std::process::Stdio;
        let mut cmd = std::process::Command::new(&self.hook_path);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_clear()
            .env("PATH", HOOK_MINIMAL_PATH);
        cmd
    }

    async fn run_hook(&self, args: &[&str]) -> Result<i32, String> {
        use tokio::process::Command;

        // Re-validate immediately before spawn (mode/path TOCTOU reduce).
        validate_hook_executable(&self.hook_path)?;

        let verb = args.first().copied().unwrap_or("?");
        let fqdn = args.get(1).copied().unwrap_or("-");

        // Overlay/tmp can return ETXTBSY (26) if we exec a just-written
        // hook. Retry a few times; do not log hook stdout/stderr.
        let mut status = None;
        for attempt in 0..5u32 {
            let mut cmd = Command::from(self.hook_command(args));
            cmd.kill_on_drop(true);
            match tokio::time::timeout(self.timeout, cmd.status()).await {
                Ok(Ok(s)) => {
                    status = Some(s);
                    break;
                }
                Ok(Err(e)) => {
                    let busy = e.raw_os_error() == Some(26);
                    if busy && attempt + 1 < 5 {
                        tokio::time::sleep(Duration::from_millis(20u64 * u64::from(attempt + 1)))
                            .await;
                        continue;
                    }
                    return Err(format!(
                        "ACME DNS hook exec failed (path={}; verb={verb}): {e}",
                        self.hook_path.display()
                    ));
                }
                Err(_) => {
                    return Err(format!(
                        "ACME DNS hook timed out after {}s (path={}; verb={verb}; fqdn={fqdn})",
                        self.timeout.as_secs(),
                        self.hook_path.display()
                    ));
                }
            }
        }
        let status = status.ok_or_else(|| {
            format!(
                "ACME DNS hook exec failed (path={}; verb={verb}): ETXTBSY retries exhausted",
                self.hook_path.display()
            )
        })?;

        let code = status.code().unwrap_or(-1);
        if code != 0 {
            // Log exit + verb/fqdn only (never stderr body; may contain secrets).
            tracing::warn!(
                path = %self.hook_path.display(),
                verb = verb,
                fqdn = fqdn,
                exit = code,
                "ACME DNS hook non-zero exit"
            );
        }
        Ok(code)
    }

    async fn run_required(&self, args: &[&str]) -> Result<(), String> {
        let code = self.run_hook(args).await?;
        if code == 0 {
            Ok(())
        } else {
            let verb = args.first().copied().unwrap_or("?");
            let fqdn = args.get(1).copied().unwrap_or("-");
            Err(format!(
                "ACME DNS hook verb={verb} fqdn={fqdn} exited {code} (path={}; fail-closed)",
                self.hook_path.display()
            ))
        }
    }

    async fn run_wait(&self, fqdn: &str, value: &str) -> Result<(), String> {
        // Brief product-side retries only (not multi-minute public DNS polling).
        // Long propagation must live inside the operator hook.
        const MAX_ATTEMPTS: u32 = 3;
        const BACKOFF_MS: u64 = 200;
        let mut last_err = String::new();
        for attempt in 0..MAX_ATTEMPTS {
            let code = self.run_hook(&["wait", fqdn, value]).await?;
            if code == 0 {
                return Ok(());
            }
            if code == HOOK_WAIT_UNSUPPORTED_EXIT {
                // Operator blocked in set; treat as propagated.
                return Ok(());
            }
            last_err = format!(
                "ACME DNS hook wait exited {code} (path={}; attempt {}/{})",
                self.hook_path.display(),
                attempt + 1,
                MAX_ATTEMPTS
            );
            if attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(Duration::from_millis(BACKOFF_MS)).await;
            }
        }
        Err(format!("{last_err}; fail-closed"))
    }
}

fn validate_hook_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    if path.as_os_str().is_empty() {
        return Err("ACME DNS hook path is empty (fail-closed)".into());
    }
    if !path.is_absolute() {
        return Err(format!(
            "ACME DNS hook path must be absolute (got {:?}; fail-closed)",
            path
        ));
    }
    let meta = std::fs::symlink_metadata(path).map_err(|e| {
        format!(
            "ACME DNS hook missing or unreadable at {}: {e} (fail-closed)",
            path.display()
        )
    })?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "ACME DNS hook at {} is a symlink (refused; require regular executable; fail-closed)",
            path.display()
        ));
    }
    if !meta.is_file() {
        return Err(format!(
            "ACME DNS hook path is not a regular file: {} (fail-closed)",
            path.display()
        ));
    }
    let mode = meta.permissions().mode();
    if mode & 0o111 == 0 {
        return Err(format!(
            "ACME DNS hook at {} is not executable (mode {:04o}; fail-closed)",
            path.display(),
            mode & 0o777
        ));
    }
    // Refuse group/world-writable: untrusted writers could replace the binary.
    if mode & 0o022 != 0 {
        return Err(format!(
            "ACME DNS hook at {} is group/world-writable (mode {:04o}; refuse; fail-closed)",
            path.display(),
            mode & 0o777
        ));
    }
    Ok(())
}

/// Truncate lossy UTF-8 on **char boundaries** (never panic mid-codepoint).
///
/// Kept for safe diagnostics helpers; product hook path does **not** log
/// stderr bodies (credentials risk).
#[cfg(test)]
fn truncate_hook_bytes(bytes: &[u8], max_chars: usize) -> String {
    let lossy = String::from_utf8_lossy(bytes);
    let flat: String = lossy
        .chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect();
    let count = flat.chars().count();
    if count <= max_chars {
        flat
    } else {
        let prefix: String = flat.chars().take(max_chars).collect();
        format!("{prefix}...")
    }
}

impl DnsProvider for ExternalHookDnsProvider {
    fn set_txt<'a>(
        &'a self,
        fqdn: &'a str,
        value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { self.run_required(&["set", fqdn, value]).await })
    }

    fn clear_txt<'a>(
        &'a self,
        fqdn: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { self.run_required(&["clear", fqdn]).await })
    }

    fn wait_propagated<'a>(
        &'a self,
        fqdn: &'a str,
        value: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { self.run_wait(fqdn, value).await })
    }
}

/// DNS-01 challenge FQDN for a domain identifier (no trailing dot).
///
/// **Wildcards unsupported:** pass the bare DNS label only (not `*.example`).
/// Config validate refuses `*` in domains; live path also refuses wildcard authz.
pub fn dns01_txt_name(domain: &str) -> String {
    let d = domain.trim().trim_end_matches('.');
    // Defense in depth if a caller passes `*.host` despite validate refuse.
    let d = d.strip_prefix("*.").unwrap_or(d);
    format!("_acme-challenge.{d}")
}

/// Issued certificate material (PEM text).
#[derive(Debug, Clone)]
pub struct IssuedCert {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Request for an ACME (or mock) order.
#[derive(Debug, Clone)]
pub struct IssueRequest {
    pub domains: Vec<String>,
    /// Contact for live account create (`instant-acme`). Unused by mock issuer.
    #[allow(dead_code)]
    pub email: String,
    /// ACME directory URL for live issuer. Unused by mock issuer.
    #[allow(dead_code)]
    pub directory_url: String,
    /// Host path for account credentials JSON. Unused by mock issuer.
    #[allow(dead_code)]
    pub account_credentials_path: PathBuf,
}

/// Pluggable issuer. Production: [`InstantAcmeDns01Issuer`]. Hermetic: [`MockAcmeIssuer`].
pub trait AcmeIssuer: Send + Sync {
    fn issue<'a>(
        &'a self,
        req: &'a IssueRequest,
    ) -> Pin<Box<dyn Future<Output = Result<IssuedCert, String>> + Send + 'a>>;
}

/// Hermetic issuer: self-signed PEMs via rcgen. No network. Lab/tests only.
#[derive(Debug, Default)]
pub struct MockAcmeIssuer {
    pub dns: Option<Arc<MockDnsProvider>>,
}

impl MockAcmeIssuer {
    pub fn with_dns(dns: Arc<MockDnsProvider>) -> Self {
        Self { dns: Some(dns) }
    }
}

impl AcmeIssuer for MockAcmeIssuer {
    fn issue<'a>(
        &'a self,
        req: &'a IssueRequest,
    ) -> Pin<Box<dyn Future<Output = Result<IssuedCert, String>> + Send + 'a>> {
        Box::pin(async move {
            if req.domains.is_empty() {
                return Err("mock ACME issuer requires at least one domain".into());
            }
            // Exercise DNS-01 name + wait_propagated path when a mock provider is attached.
            if let Some(dns) = &self.dns {
                for d in &req.domains {
                    let name = dns01_txt_name(d);
                    let val = "mock-key-authorization";
                    dns.set_txt(&name, val).await?;
                    // Retry once if mock is configured with a single lag fail.
                    match dns.wait_propagated(&name, val).await {
                        Ok(()) => {}
                        Err(_) => {
                            dns.wait_propagated(&name, val).await?;
                        }
                    }
                }
            }
            let (cert_pem, key_pem) = mock_self_signed_pems(&req.domains)?;
            if let Some(dns) = &self.dns {
                for d in &req.domains {
                    let _ = dns.clear_txt(&dns01_txt_name(d)).await;
                }
            }
            Ok(IssuedCert { cert_pem, key_pem })
        })
    }
}

fn mock_self_signed_pems(domains: &[String]) -> Result<(String, String), String> {
    let mut params = rcgen::CertificateParams::new(domains.to_vec())
        .map_err(|e| format!("mock ACME rcgen params: {e}"))?;
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, domains[0].as_str());
    // Short-lived so expiry tests can use a separate helper; default is fine.
    let key_pair = rcgen::KeyPair::generate().map_err(|e| format!("mock ACME keypair: {e}"))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| format!("mock ACME self-sign: {e}"))?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

/// Live Let's Encrypt-capable issuer using `instant-acme` + DNS-01.
///
/// Requires a [`DnsProvider`] that can publish public TXT records (product:
/// [`ExternalHookDnsProvider`]). Without a working adapter, order validation
/// fails closed (no silent cleartext). Not exercised against the public
/// internet in CI (hermetic tests use mock issuer or construct without `issue`).
pub struct InstantAcmeDns01Issuer {
    dns: Arc<dyn DnsProvider>,
}

impl InstantAcmeDns01Issuer {
    pub fn new(dns: Arc<dyn DnsProvider>) -> Self {
        Self { dns }
    }
}

impl AcmeIssuer for InstantAcmeDns01Issuer {
    fn issue<'a>(
        &'a self,
        req: &'a IssueRequest,
    ) -> Pin<Box<dyn Future<Output = Result<IssuedCert, String>> + Send + 'a>> {
        Box::pin(async move { issue_with_instant_acme(req, self.dns.as_ref()).await })
    }
}

/// Clear every DNS-01 TXT FQDN we published (best-effort; never fail the caller).
///
/// Live path always runs this after order work so fail-after-`set_txt` (propagation,
/// set_ready, poll_ready, finalize, poll_certificate) does not leave TXT records.
async fn clear_published_txt(dns: &dyn DnsProvider, names: &[String]) {
    for n in names {
        let _ = dns.clear_txt(n).await;
    }
}

/// `set_txt` then **immediately** track the FQDN so cleanup runs even if
/// `wait_propagated` / later steps fail before an older `published.push`.
async fn dns01_set_txt_and_track(
    dns: &dyn DnsProvider,
    txt_name: &str,
    txt_value: &str,
    published: &mut Vec<String>,
) -> Result<(), String> {
    dns.set_txt(txt_name, txt_value).await?;
    published.push(txt_name.to_string());
    Ok(())
}

/// Publish TXT, track FQDN, wait for propagation (product live DNS-01 step).
///
/// On any error after a successful `set_txt`, the name is already in `published`
/// for [`clear_published_txt`].
async fn dns01_publish_wait(
    dns: &dyn DnsProvider,
    txt_name: &str,
    txt_value: &str,
    published: &mut Vec<String>,
) -> Result<(), String> {
    dns01_set_txt_and_track(dns, txt_name, txt_value, published).await?;
    dns.wait_propagated(txt_name, txt_value)
        .await
        .map_err(|e| format!("ACME DNS-01 propagation wait failed: {e}"))
}

async fn issue_with_instant_acme(
    req: &IssueRequest,
    dns: &dyn DnsProvider,
) -> Result<IssuedCert, String> {
    use instant_acme::{Account, ChallengeType, Identifier, NewAccount, NewOrder, RetryPolicy};

    if req.domains.is_empty() {
        return Err("instant-acme DNS-01 issuer requires at least one domain".into());
    }

    // Ensure process-level rustls provider before hyper-rustls ClientConfig
    // (Account::builder). Without this, rustls panics when no automatic
    // default is selected. Idempotent if main already installed aws-lc-rs.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Account::builder uses default hyper-rustls client (aws-lc-rs crypto).
    let builder = Account::builder().map_err(|e| format!("ACME account builder failed: {e}"))?;

    let contact = format!("mailto:{}", req.email);
    let (account, credentials) = if !path_is_blank(&req.account_credentials_path)
        && req.account_credentials_path.exists()
    {
        // Fail closed: same owner-only regular-file contract as TLS private key.
        require_owner_only_secret_file(&req.account_credentials_path, "ACME account credentials")?;
        let raw = std::fs::read_to_string(&req.account_credentials_path).map_err(|e| {
            format!(
                "read ACME account credentials at {}: {e}",
                req.account_credentials_path.display()
            )
        })?;
        let creds: instant_acme::AccountCredentials = serde_json::from_str(&raw).map_err(|e| {
            format!(
                "parse ACME account credentials at {}: {e}",
                req.account_credentials_path.display()
            )
        })?;
        let account = builder
            .from_credentials(creds)
            .await
            .map_err(|e| format!("restore ACME account from credentials: {e}"))?;
        // Keep existing credentials file as-is.
        (account, None)
    } else {
        let (account, creds) = builder
            .create(
                &NewAccount {
                    contact: &[contact.as_str()],
                    terms_of_service_agreed: true,
                    only_return_existing: false,
                },
                req.directory_url.clone(),
                None,
            )
            .await
            .map_err(|e| format!("ACME account create failed: {e}"))?;
        (account, Some(creds))
    };

    if let Some(creds) = credentials {
        write_account_credentials(&req.account_credentials_path, &creds)?;
    }

    // DNS-01: track each FQDN immediately after set_txt; always clear on exit
    // (success or any failure after publish: wait, set_ready, poll, finalize, cert).
    let mut published: Vec<String> = Vec::new();
    let order_result = async {
        let identifiers: Vec<Identifier> = req
            .domains
            .iter()
            .map(|d| Identifier::Dns(d.clone()))
            .collect();
        let mut order = account
            .new_order(&NewOrder::new(identifiers.as_slice()))
            .await
            .map_err(|e| format!("ACME new_order failed: {e}"))?;

        let mut authorizations = order.authorizations();
        while let Some(result) = authorizations.next().await {
            let mut authz = result.map_err(|e| format!("ACME authorization: {e}"))?;
            match authz.status {
                instant_acme::AuthorizationStatus::Pending => {}
                instant_acme::AuthorizationStatus::Valid => continue,
                other => {
                    return Err(format!("ACME authorization unexpected status: {other:?}"));
                }
            }
            let mut challenge = authz
                .challenge(ChallengeType::Dns01)
                .ok_or_else(|| "ACME order has no DNS-01 challenge".to_string())?;
            // Prefer raw DNS label; refuse wildcards (Display would be `*.host`).
            let aid = challenge.identifier();
            if aid.wildcard {
                return Err(
                    "wildcard ACME identifiers are not supported for DNS-01 in this build \
                     (fail-closed)"
                        .into(),
                );
            }
            let dns_label = match aid.identifier {
                Identifier::Dns(name) => name.clone(),
                other => {
                    return Err(format!(
                        "ACME DNS-01 expects a DNS identifier, got {other:?}; fail-closed"
                    ));
                }
            };
            let txt_name = dns01_txt_name(&dns_label);
            let txt_value = challenge.key_authorization().dns_value();
            // Track FQDN immediately after set_txt (before wait_propagated).
            dns01_publish_wait(dns, &txt_name, &txt_value, &mut published).await?;
            challenge
                .set_ready()
                .await
                .map_err(|e| format!("ACME challenge set_ready: {e}"))?;
        }

        let status = order
            .poll_ready(&RetryPolicy::default())
            .await
            .map_err(|e| format!("ACME poll_ready: {e}"))?;
        if status != instant_acme::OrderStatus::Ready {
            return Err(format!(
                "ACME order not ready after DNS-01 (status={status:?}); fail-closed"
            ));
        }

        let key_pem = order
            .finalize()
            .await
            .map_err(|e| format!("ACME finalize (CSR/key): {e}"))?;
        let cert_pem = order
            .poll_certificate(&RetryPolicy::default())
            .await
            .map_err(|e| format!("ACME poll_certificate: {e}"))?;

        Ok(IssuedCert { cert_pem, key_pem })
    }
    .await;
    clear_published_txt(dns, &published).await;
    order_result
}

fn write_account_credentials(
    path: &Path,
    creds: &instant_acme::AccountCredentials,
) -> Result<(), String> {
    if path_is_blank(path) {
        return Err("ACME account credentials path is empty".into());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|e| {
            format!(
                "create ACME account credentials parent {}: {e}",
                parent.display()
            )
        })?;
    }
    let json = serde_json::to_string_pretty(creds)
        .map_err(|e| format!("serialize ACME account credentials: {e}"))?;
    write_secret_file(path, json.as_bytes())?;
    Ok(())
}

/// Write key (0600) then cert (0644) PEMs via same-dir temp + rename.
///
/// Order: **key first**, then cert, so a crash mid-write does not leave a new
/// cert paired with an old/missing key as the only recoverable state as often.
/// Fail-closed key mode enforced after write.
pub fn write_issued_pems(paths: &TlsPaths, issued: &IssuedCert) -> Result<(), String> {
    if path_is_blank(&paths.cert_path) || path_is_blank(&paths.key_path) {
        return Err("TLS cert/key paths are empty; cannot write ACME PEMs".into());
    }
    require_parent_dir_exists(&paths.cert_path, "TLS certificate")?;
    require_parent_dir_exists(&paths.key_path, "TLS private key")?;

    // Key (secret) first, then public cert.
    write_secret_file(&paths.key_path, issued.key_pem.as_bytes())?;
    write_public_pem_file(&paths.cert_path, issued.cert_pem.as_bytes())?;

    // Fail-closed: same private-key mode contract as static PEM load path.
    paths.require_files_exist()?;
    // Product design: cert mode 0644 after ACME write.
    assert_cert_mode_0644(&paths.cert_path)?;
    Ok(())
}

fn require_parent_dir_exists(path: &Path, label: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
    {
        return Err(format!(
            "{label} parent directory missing: {} (create host path before ACME write)",
            parent.display()
        ));
    }
    Ok(())
}

fn assert_cert_mode_0644(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|e| {
        format!(
            "stat TLS certificate after ACME write at {}: {e}",
            path.display()
        )
    })?;
    let mode = meta.permissions().mode() & 0o777;
    if mode != 0o644 {
        return Err(format!(
            "TLS certificate at {} mode {:04o} after write; expected 0644",
            path.display(),
            mode
        ));
    }
    Ok(())
}

/// Atomic-ish public PEM write (temp in same dir + rename); mode 0644.
fn write_public_pem_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if path.exists() {
        let meta = std::fs::symlink_metadata(path)
            .map_err(|e| format!("stat existing TLS certificate {}: {e}", path.display()))?;
        if meta.file_type().is_symlink() {
            return Err(format!(
                "TLS certificate at {} is a symlink (refused; require regular file)",
                path.display()
            ));
        }
        if !meta.is_file() {
            return Err(format!(
                "TLS certificate path is not a regular file: {}",
                path.display()
            ));
        }
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(
        ".surmount-cert-tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let write_result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(0o644);
        #[cfg(target_os = "linux")]
        {
            opts.custom_flags(libc_o_nofollow());
        }
        let mut f = opts
            .open(&tmp)
            .map_err(|e| format!("open temp certificate {} for write: {e}", tmp.display()))?;
        f.write_all(bytes)
            .map_err(|e| format!("write temp certificate {}: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("sync temp certificate {}: {e}", tmp.display()))?;
        drop(f);
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))
            .map_err(|e| format!("set temp certificate {} mode 0644: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            format!(
                "rename temp certificate {} -> {}: {e}",
                tmp.display(),
                path.display()
            )
        })?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    write_result
}

/// Write secret via same-dir tempfile 0600 + rename. Refuse symlink / non-regular
/// / group-world-readable existing target (no silent overwrite of bad modes).
fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if path.exists() {
        match secret_file_mode_status(path, "secret file") {
            SecretFileModeStatus::OwnerOnlyOk => {}
            SecretFileModeStatus::Missing => {}
            SecretFileModeStatus::HardFail(msg) => {
                return Err(format!(
                    "refuse overwrite of insecure or non-regular secret at {}: {msg}",
                    path.display()
                ));
            }
        }
    }

    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let tmp = parent.join(format!(
        ".surmount-secret-tmp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));

    let write_result = (|| {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(0o600);
        #[cfg(target_os = "linux")]
        {
            opts.custom_flags(libc_o_nofollow());
        }
        let mut f = opts.open(&tmp).map_err(|e| {
            format!(
                "open temp secret {} for write (mode 0600): {e}",
                tmp.display()
            )
        })?;
        f.write_all(bytes)
            .map_err(|e| format!("write temp secret {}: {e}", tmp.display()))?;
        f.sync_all()
            .map_err(|e| format!("sync temp secret {}: {e}", tmp.display()))?;
        drop(f);
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("set temp secret {} mode 0600: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            format!(
                "rename temp secret {} -> {}: {e}",
                tmp.display(),
                path.display()
            )
        })?;
        // Re-assert final path mode.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("set secret file {} mode 0600: {e}", path.display()))?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    write_result
}

/// O_NOFOLLOW when available (Linux); 0 elsewhere so OpenOptions still builds.
fn libc_o_nofollow() -> i32 {
    #[cfg(target_os = "linux")]
    {
        libc::O_NOFOLLOW
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Presence of a public PEM path as a regular file (mode bits not checked).
#[derive(Debug, Clone, PartialEq, Eq)]
enum RegularFilePresence {
    Missing,
    RegularOk,
    HardFail(String),
}

fn regular_file_presence(path: &Path, label: &str) -> RegularFilePresence {
    if path_is_blank(path) {
        return RegularFilePresence::HardFail(format!("{label} path is empty"));
    }
    match std::fs::symlink_metadata(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => RegularFilePresence::Missing,
        Err(err) => RegularFilePresence::HardFail(format!(
            "{label} missing or unreadable at {}: {err}",
            path.display()
        )),
        Ok(meta) if meta.file_type().is_symlink() => RegularFilePresence::HardFail(format!(
            "{label} at {} is a symlink (refused; require regular file)",
            path.display()
        )),
        Ok(meta) if !meta.is_file() => RegularFilePresence::HardFail(format!(
            "{label} path is not a regular file: {}",
            path.display()
        )),
        Ok(_) => RegularFilePresence::RegularOk,
    }
}

/// True when cert+key load, leaf not yet in the renew window, and (when provided)
/// leaf SAN/CN covers every required domain.
///
/// **Early-renew window:** when `renew_days_before_expiry > 0`, a leaf with
/// remaining lifetime strictly less than that many days is **unusable** (caller
/// reissues). Scaffold default is [`DEFAULT_ACME_RENEW_DAYS_BEFORE_EXPIRY`] (30).
/// Zero means only full notAfter (and not-yet-valid) fail the check.
/// Hot-reload residual: restart after renew. See EDGE_AND_TLS / OPS.
///
/// Missing files => false (caller may issue). World-readable key / non-regular
/// => hard Err (structured mode check; not string match).
pub fn pem_material_usable(
    paths: &TlsPaths,
    now: SystemTime,
    required_domains: Option<&[String]>,
    renew_days_before_expiry: u64,
) -> Result<bool, String> {
    if path_is_blank(&paths.cert_path) || path_is_blank(&paths.key_path) {
        return Ok(false);
    }

    // Structured key mode: hard fail on group/world or non-regular; missing = not usable.
    // No string matching on error text (SecretFileModeStatus enum).
    match secret_file_mode_status(&paths.key_path, "TLS private key") {
        SecretFileModeStatus::Missing => return Ok(false),
        SecretFileModeStatus::HardFail(msg) => return Err(msg),
        SecretFileModeStatus::OwnerOnlyOk => {}
    }
    // Cert may be world-readable (0644); only require regular file (not symlink).
    match regular_file_presence(&paths.cert_path, "TLS certificate") {
        RegularFilePresence::Missing => return Ok(false),
        RegularFilePresence::HardFail(msg) => return Err(msg),
        RegularFilePresence::RegularOk => {}
    }

    let cert_bytes = std::fs::read(&paths.cert_path).map_err(|e| {
        format!(
            "read TLS certificate for expiry check at {}: {e}",
            paths.cert_path.display()
        )
    })?;
    if leaf_cert_needs_reissue(&cert_bytes, now, renew_days_before_expiry)? {
        return Ok(false);
    }
    if let Some(domains) = required_domains
        && !domains.is_empty()
        && !leaf_covers_domains(&cert_bytes, domains)?
    {
        return Ok(false);
    }
    // Ensure key PEM parses by reusing rustls load path shape (cheap parse).
    let key_bytes = std::fs::read(&paths.key_path).map_err(|e| {
        format!(
            "read TLS private key for usability check at {}: {e}",
            paths.key_path.display()
        )
    })?;
    use rustls::pki_types::{PrivateKeyDer, pem::PemObject};
    PrivateKeyDer::from_pem_slice(&key_bytes).map_err(|e| {
        format!(
            "invalid TLS private key PEM at {}: {e}",
            paths.key_path.display()
        )
    })?;
    Ok(true)
}

/// Parse leaf certificate PEM; true if not-yet-valid, past notAfter, or inside
/// the early-renew window (`remaining < renew_days_before_expiry` days).
fn leaf_cert_needs_reissue(
    cert_pem_or_der: &[u8],
    now: SystemTime,
    renew_days_before_expiry: u64,
) -> Result<bool, String> {
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    use x509_parser::prelude::*;

    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem_or_der)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("invalid certificate PEM for expiry check: {e}"))?;
    let leaf = certs
        .first()
        .ok_or_else(|| "no certificates in PEM for expiry check".to_string())?;

    let (_, parsed) = X509Certificate::from_der(leaf.as_ref())
        .map_err(|e| format!("x509 parse for expiry check: {e}"))?;
    let validity = parsed.validity();

    let now_secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| format!("system clock before unix epoch: {e}"))?
        .as_secs() as i64;

    // ASN.1 Time -> unix via timestamp() on x509-parser ASN1Time.
    let not_before = validity.not_before.timestamp();
    let not_after = validity.not_after.timestamp();
    if now_secs < not_before {
        return Ok(true); // treat not-yet-valid as unusable (reissue)
    }
    if now_secs >= not_after {
        return Ok(true);
    }
    if renew_days_before_expiry > 0 {
        let remaining_secs = not_after.saturating_sub(now_secs);
        // Compare in whole days: remaining < threshold => reissue.
        let remaining_days = remaining_secs / 86_400;
        if remaining_days < renew_days_before_expiry as i64 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True when every required domain appears as a DNS SAN or CN on the leaf.
fn leaf_covers_domains(cert_pem_or_der: &[u8], required: &[String]) -> Result<bool, String> {
    let names = leaf_dns_names(cert_pem_or_der)?;
    let names_l: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    Ok(required.iter().all(|d| {
        let want = d.trim().trim_end_matches('.').to_ascii_lowercase();
        names_l.iter().any(|n| n == &want)
    }))
}

fn leaf_dns_names(cert_pem_or_der: &[u8]) -> Result<Vec<String>, String> {
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    use x509_parser::prelude::*;

    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem_or_der)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("invalid certificate PEM for SAN check: {e}"))?;
    let leaf = certs
        .first()
        .ok_or_else(|| "no certificates in PEM for SAN check".to_string())?;
    let (_, parsed) = X509Certificate::from_der(leaf.as_ref())
        .map_err(|e| format!("x509 parse for SAN check: {e}"))?;

    let mut names = Vec::new();
    if let Some(cn) = parsed
        .subject()
        .iter_common_name()
        .next()
        .and_then(|a| a.as_str().ok())
    {
        names.push(cn.to_string());
    }
    if let Ok(Some(san)) = parsed.subject_alternative_name() {
        for name in &san.value.general_names {
            if let GeneralName::DNSName(dns) = name {
                names.push(dns.to_string());
            }
        }
    }
    Ok(names)
}

/// Outcome of HTTPS material preparation at process start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TlsMaterialDecision {
    /// ACME disabled; existing PEMs required and present.
    StaticPemsOk,
    /// ACME on; existing PEMs still valid (not expired; domains covered when set).
    ReusedExistingPems,
    /// ACME on; issued and wrote PEMs to configured paths.
    /// `mock` true => lab self-signed issuer (not WebPKI).
    IssuedAndWrotePems { mock: bool },
}

/// Prepare TLS PEMs for HTTPS listen.
///
/// - ACME **off**: require existing PEMs (fail closed). Same as historical path.
/// - ACME **on**: reuse PEMs when not in early-renew window, not expired, and
///   domains covered; else issue.
/// - Issuance failure: fail closed with clear error (no silent cleartext).
///
/// Hot-reload residual: restart after renew to pick up new PEMs.
pub async fn ensure_tls_material(
    paths: &TlsPaths,
    acme: &AcmeConfig,
    issuer: Option<&dyn AcmeIssuer>,
) -> Result<TlsMaterialDecision, String> {
    if !acme.enable {
        paths.require_files_exist()?;
        return Ok(TlsMaterialDecision::StaticPemsOk);
    }

    let now = SystemTime::now();
    let domains = if acme.domains.is_empty() {
        None
    } else {
        Some(acme.domains.as_slice())
    };
    if pem_material_usable(paths, now, domains, acme.renew_days_before_expiry)? {
        return Ok(TlsMaterialDecision::ReusedExistingPems);
    }

    let issuer = issuer.ok_or_else(|| {
        "SURMOUNT_ACME_ENABLE is on, PEMs missing/expired/in-renew-window/domain-mismatch, \
         but no ACME issuer is configured (set SURMOUNT_ACME_DNS_PROVIDER=mock for lab, \
         or external-hook with SURMOUNT_ACME_DNS_HOOK for live DNS-01; fail-closed)"
            .to_string()
    })?;

    let req = IssueRequest {
        domains: acme.domains.clone(),
        email: acme.email.clone(),
        directory_url: acme.directory_url.clone(),
        account_credentials_path: acme.account_credentials_path.clone(),
    };
    let issued = issuer
        .issue(&req)
        .await
        .map_err(|e| format!("ACME issuance failed (fail-closed; no silent cleartext): {e}"))?;
    write_issued_pems(paths, &issued)?;
    let mock = matches!(acme.dns_provider, DnsProviderKind::Mock);
    Ok(TlsMaterialDecision::IssuedAndWrotePems { mock })
}

/// Select issuer for process start from config.
///
/// - `dns_provider=mock` => [`MockAcmeIssuer`] (lab / hermetic; **not** live LE)
/// - `dns_provider=none` => no issuer (reuse PEMs only; issue fails closed)
/// - `dns_provider=external-hook` => [`InstantAcmeDns01Issuer`] +
///   [`ExternalHookDnsProvider`] (validates hook path is executable)
///
/// Returns `Err` when external-hook path is unusable (fail closed at startup).
/// Callers that inject a custom issuer (tests) bypass this.
pub fn issuer_from_config(acme: &AcmeConfig) -> Result<Option<Box<dyn AcmeIssuer>>, String> {
    if !acme.enable {
        return Ok(None);
    }
    match acme.dns_provider {
        DnsProviderKind::Mock => {
            let dns = Arc::new(MockDnsProvider::new());
            Ok(Some(Box::new(MockAcmeIssuer::with_dns(dns))))
        }
        DnsProviderKind::None => Ok(None),
        DnsProviderKind::ExternalHook => {
            let dns = Arc::new(ExternalHookDnsProvider::new(
                acme.dns_hook_path.clone(),
                acme.dns_hook_timeout_secs,
            )?);
            Ok(Some(Box::new(InstantAcmeDns01Issuer::new(dns))))
        }
    }
}

/// Let's Encrypt directory URL helpers (ops docs + tests). Not auto-selected.
#[allow(dead_code)]
pub mod directories {
    /// Staging (rate-limit friendly). Prefer first on a new host.
    pub const LETS_ENCRYPT_STAGING: &str = "https://acme-staging-v02.api.letsencrypt.org/directory";
    /// Production (ops residual; use staging first).
    pub const LETS_ENCRYPT_PRODUCTION: &str = "https://acme-v02.api.letsencrypt.org/directory";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(prefix: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn acme_config_default_off() {
        let cfg = AcmeConfig::from_env_map(|_| None).unwrap();
        assert!(!cfg.enable);
        assert_eq!(cfg.enable, DEFAULT_ACME_ENABLE);
        assert!(cfg.domains.is_empty());
        assert!(cfg.directory_url.is_empty());
        assert_eq!(cfg.challenge, AcmeChallenge::Dns01);
        assert_eq!(cfg.dns_provider, DnsProviderKind::None);
    }

    #[test]
    fn acme_config_enable_requires_domains_email_directory_account_path() {
        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("DOMAINS") || err.contains("domains"), "{err}");

        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("true".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("EMAIL") || err.contains("email"), "{err}");

        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("yes".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("DIRECTORY") || err.contains("directory"),
            "{err}"
        );

        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("on".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("ACCOUNT_CREDENTIALS") || err.contains("credentials"),
            "{err}"
        );
    }

    #[test]
    fn acme_config_enable_complete_ok() {
        let cfg = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test,mail.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            "SURMOUNT_ACME_DNS_PROVIDER" => Some("mock".into()),
            "SURMOUNT_ACME_CHALLENGE" => Some("dns-01".into()),
            _ => None,
        })
        .unwrap();
        assert!(cfg.enable);
        assert_eq!(cfg.domains.len(), 2);
        assert_eq!(cfg.dns_provider, DnsProviderKind::Mock);
        assert_eq!(cfg.challenge, AcmeChallenge::Dns01);
    }

    #[test]
    fn acme_http01_challenge_refused() {
        let err = AcmeChallenge::parse("http-01").unwrap_err();
        assert!(err.contains("parked") || err.contains("HTTP-01"), "{err}");
    }

    #[test]
    fn dns01_txt_name_format() {
        assert_eq!(
            dns01_txt_name("services.example.test"),
            "_acme-challenge.services.example.test"
        );
        assert_eq!(
            dns01_txt_name("services.example.test."),
            "_acme-challenge.services.example.test"
        );
    }

    #[tokio::test]
    async fn mock_dns_provider_set_and_clear() {
        let dns = MockDnsProvider::new();
        dns.set_txt("_acme-challenge.example.test", "abc")
            .await
            .unwrap();
        assert_eq!(dns.records().len(), 1);
        dns.clear_txt("_acme-challenge.example.test").await.unwrap();
        assert!(dns.records().is_empty());
    }

    #[tokio::test]
    async fn acme_disabled_requires_existing_pems() {
        let dir = temp_dir("acme-disabled-pems");
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        let acme = AcmeConfig::default();
        assert!(!acme.enable);
        let err = ensure_tls_material(&paths, &acme, None).await.unwrap_err();
        assert!(
            err.contains("missing") || err.contains("unreadable"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn acme_disabled_ok_with_existing_pems() {
        let dir = temp_dir("acme-disabled-ok");
        let paths = write_self_signed(&dir, &["localhost".into()]);
        let acme = AcmeConfig::default();
        let decision = ensure_tls_material(&paths, &acme, None).await.unwrap();
        assert_eq!(decision, TlsMaterialDecision::StaticPemsOk);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn mock_acme_issues_and_writes_pems_with_owner_only_key() {
        let dir = temp_dir("acme-mock-issue");
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        let acme = AcmeConfig {
            enable: true,
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            email: "ops@example.test".into(),
            domains: vec!["services.example.test".into()],
            account_credentials_path: dir.join("account.json"),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::Mock,
            ..AcmeConfig::default()
        };
        let dns = Arc::new(MockDnsProvider::new());
        let issuer = MockAcmeIssuer::with_dns(dns.clone());
        let decision = ensure_tls_material(&paths, &acme, Some(&issuer))
            .await
            .expect("mock issue");
        assert_eq!(
            decision,
            TlsMaterialDecision::IssuedAndWrotePems { mock: true }
        );
        assert!(paths.cert_path.is_file());
        assert!(paths.key_path.is_file());
        let key_mode = std::fs::metadata(&paths.key_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(key_mode, 0o600, "ACME-written key must be owner-only");
        let cert_mode = std::fs::metadata(&paths.cert_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(cert_mode, 0o644, "ACME-written cert must be 0644");
        // DNS mock was exercised then cleared.
        assert!(dns.records().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn acme_reuses_valid_existing_pems_without_issuer() {
        let dir = temp_dir("acme-reuse");
        let paths = write_self_signed(&dir, &["services.example.test".into()]);
        let acme = AcmeConfig {
            enable: true,
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            email: "ops@example.test".into(),
            domains: vec!["services.example.test".into()],
            account_credentials_path: dir.join("account.json"),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::None,
            // Default early-renew is 30d; rcgen default notAfter is far enough.
            ..AcmeConfig::default()
        };
        // No issuer: must still succeed when PEMs usable.
        let decision = ensure_tls_material(&paths, &acme, None).await.unwrap();
        assert_eq!(decision, TlsMaterialDecision::ReusedExistingPems);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn acme_enabled_missing_pems_without_issuer_fails_closed() {
        let dir = temp_dir("acme-no-issuer");
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        let acme = AcmeConfig {
            enable: true,
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            email: "ops@example.test".into(),
            domains: vec!["services.example.test".into()],
            account_credentials_path: dir.join("account.json"),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::None,
            ..AcmeConfig::default()
        };
        let err = ensure_tls_material(&paths, &acme, None).await.unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("no ACME issuer") || err.contains("issuer"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn acme_written_key_permission_refuse_still_holds() {
        let dir = temp_dir("acme-keymode");
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        let issued = IssuedCert {
            cert_pem: "-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n".into(),
            key_pem: format!(
                "{}BEGIN PRIVATE KEY{}\nMIIB\n{}END PRIVATE KEY{}\n",
                "-".repeat(5),
                "-".repeat(5),
                "-".repeat(5),
                "-".repeat(5)
            ),
        };
        // write_issued_pems always sets 0600; force world-readable after write
        // and prove require_files_exist (used by write path and load path) refuses.
        write_secret_file(&paths.key_path, issued.key_pem.as_bytes()).unwrap();
        std::fs::write(&paths.cert_path, issued.cert_pem.as_bytes()).unwrap();
        std::fs::set_permissions(&paths.key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = paths.require_files_exist().unwrap_err();
        assert!(err.contains("group/world"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn issuer_from_config_mock_and_none() {
        let mut acme = AcmeConfig {
            enable: true,
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            email: "ops@example.test".into(),
            domains: vec!["services.example.test".into()],
            account_credentials_path: PathBuf::from("/tmp/acme-account.json"),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::Mock,
            ..AcmeConfig::default()
        };
        assert!(issuer_from_config(&acme).unwrap().is_some());
        acme.dns_provider = DnsProviderKind::None;
        assert!(issuer_from_config(&acme).unwrap().is_none());
        acme.enable = false;
        assert!(issuer_from_config(&acme).unwrap().is_none());
    }

    #[test]
    fn pem_material_usable_false_when_missing() {
        let paths = TlsPaths::new("/no/such/cert.pem", "/no/such/key.pem");
        assert!(!pem_material_usable(&paths, SystemTime::now(), None, 30).unwrap());
    }

    #[test]
    fn pem_material_usable_true_for_fresh_self_signed() {
        let dir = temp_dir("acme-usable");
        let paths = write_self_signed(&dir, &["localhost".into()]);
        assert!(pem_material_usable(&paths, SystemTime::now(), None, 30).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pem_material_usable_false_for_expired_cert() {
        let dir = temp_dir("acme-expired");
        let paths = write_expired_self_signed(&dir, &["localhost".into()]);
        assert!(
            !pem_material_usable(&paths, SystemTime::now(), None, 30).unwrap(),
            "expired leaf must be unusable so ACME can reissue"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pem_material_usable_false_when_within_early_renew_window() {
        // notAfter = now + 1 day, threshold 30 days => remaining < 30 => unusable.
        let dir = temp_dir("acme-early-renew");
        let paths = write_self_signed_not_after_days(&dir, &["localhost".into()], 1);
        assert!(
            !pem_material_usable(&paths, SystemTime::now(), None, 30).unwrap(),
            "leaf with 1 day remaining must reissue when renew threshold is 30 days"
        );
        // Same cert is usable if threshold is 0 (full notAfter only).
        assert!(
            pem_material_usable(&paths, SystemTime::now(), None, 0).unwrap(),
            "threshold 0 keeps material usable until full notAfter"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn ensure_tls_material_reissues_when_within_early_renew_window() {
        let dir = temp_dir("acme-reissue-window");
        let paths = write_self_signed_not_after_days(&dir, &["services.example.test".into()], 1);
        let acme = AcmeConfig {
            enable: true,
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            email: "ops@example.test".into(),
            domains: vec!["services.example.test".into()],
            account_credentials_path: dir.join("account.json"),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::Mock,
            renew_days_before_expiry: 30,
            ..AcmeConfig::default()
        };
        let issuer = MockAcmeIssuer::with_dns(Arc::new(MockDnsProvider::new()));
        let decision = ensure_tls_material(&paths, &acme, Some(&issuer))
            .await
            .expect("reissue in early-renew window");
        assert_eq!(
            decision,
            TlsMaterialDecision::IssuedAndWrotePems { mock: true }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pem_material_usable_refuses_world_readable_key() {
        let dir = temp_dir("acme-usable-mode");
        let paths = write_self_signed(&dir, &["localhost".into()]);
        std::fs::set_permissions(&paths.key_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = pem_material_usable(&paths, SystemTime::now(), None, 30).unwrap_err();
        assert!(err.contains("group/world"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pem_material_usable_false_when_san_does_not_cover_domains() {
        let dir = temp_dir("acme-san-mismatch");
        let paths = write_self_signed(&dir, &["a.example.test".into()]);
        let required = vec!["b.example.test".into()];
        assert!(
            !pem_material_usable(&paths, SystemTime::now(), Some(required.as_slice()), 30).unwrap(),
            "cert for a.example must not be reusable when domains require b.example"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pem_material_usable_true_when_san_covers_domains() {
        let dir = temp_dir("acme-san-cover");
        let paths = write_self_signed(&dir, &["services.example.test".into()]);
        let required = vec!["services.example.test".into()];
        assert!(
            pem_material_usable(&paths, SystemTime::now(), Some(required.as_slice()), 30).unwrap()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn acme_config_refuses_wildcard_domains() {
        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("*.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("wildcard") || err.contains("*"), "{err}");
    }

    #[test]
    fn acme_config_refuses_http_directory_url() {
        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some("http://acme.example.test/directory".into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("https://") || err.contains("https"), "{err}");
    }

    #[test]
    fn acme_config_refuses_relative_account_credentials_path() {
        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => Some("relative/account.json".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn dns01_txt_name_strips_wildcard_prefix_defense_in_depth() {
        assert_eq!(
            dns01_txt_name("*.example.test"),
            "_acme-challenge.example.test"
        );
    }

    #[tokio::test]
    async fn mock_dns_wait_propagated_fails_until_lag_exhausted() {
        let dns = MockDnsProvider::with_propagation_fails(1);
        dns.set_txt("_acme-challenge.example.test", "val")
            .await
            .unwrap();
        let err = dns
            .wait_propagated("_acme-challenge.example.test", "val")
            .await
            .unwrap_err();
        assert!(err.contains("not yet propagated"), "{err}");
        dns.wait_propagated("_acme-challenge.example.test", "val")
            .await
            .expect("second wait should succeed after lag");
    }

    /// Fail after successful set_txt (wait_propagated): tracked FQDN must still
    /// be cleared (same helpers as live `issue_with_instant_acme`).
    #[tokio::test]
    async fn dns01_fail_after_set_txt_clears_published_records() {
        let dns = MockDnsProvider::with_propagation_fails(1);
        let name = "_acme-challenge.example.test";
        let val = "key-authorization-value";
        let mut published = Vec::new();
        let result = dns01_publish_wait(&dns, name, val, &mut published).await;
        // Always clear tracked names (product live path does the same after order body).
        clear_published_txt(&dns, &published).await;
        assert!(result.is_err(), "wait_propagated lag must fail: {result:?}");
        let err = result.unwrap_err();
        assert!(
            err.contains("propagation") || err.contains("not yet propagated"),
            "{err}"
        );
        assert_eq!(
            published,
            vec![name.to_string()],
            "FQDN must be tracked immediately after set_txt, before wait fails"
        );
        assert!(
            dns.records().is_empty(),
            "clear_txt must run after fail-after-set_txt; leftover={:?}",
            dns.records()
        );
    }

    /// Multi-name: first TXT published, second fails wait; both tracked names cleared.
    #[tokio::test]
    async fn dns01_fail_on_later_name_clears_earlier_published() {
        let dns = MockDnsProvider::with_propagation_fails(0);
        let mut published = Vec::new();
        let r1 = dns01_publish_wait(
            &dns,
            "_acme-challenge.a.example.test",
            "val-a",
            &mut published,
        )
        .await;
        assert!(r1.is_ok(), "first name should publish+wait: {r1:?}");
        dns.set_propagation_fails(1);
        let r2 = dns01_publish_wait(
            &dns,
            "_acme-challenge.b.example.test",
            "val-b",
            &mut published,
        )
        .await;
        clear_published_txt(&dns, &published).await;
        assert!(r2.is_err(), "second name must fail wait after set_txt");
        assert_eq!(published.len(), 2, "both FQDNs tracked after each set_txt");
        assert!(
            dns.records().is_empty(),
            "both a and b TXT must be cleared; leftover={:?}",
            dns.records()
        );
    }

    #[tokio::test]
    async fn mock_issuer_retries_once_after_propagation_lag() {
        let dns = Arc::new(MockDnsProvider::with_propagation_fails(1));
        let issuer = MockAcmeIssuer::with_dns(dns);
        let req = IssueRequest {
            domains: vec!["services.example.test".into()],
            email: "ops@example.test".into(),
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            account_credentials_path: PathBuf::from("/tmp/unused-account.json"),
        };
        let issued = issuer
            .issue(&req)
            .await
            .expect("mock issue after lag retry");
        assert!(issued.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(issued.key_pem.contains("BEGIN"));
    }

    #[test]
    fn write_secret_file_refuses_insecure_existing() {
        let dir = temp_dir("acme-secret-refuse");
        let path = dir.join("secret.pem");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = write_secret_file(&path, b"new").unwrap_err();
        assert!(
            err.contains("refuse overwrite") || err.contains("group/world"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_issued_pems_sets_cert_0644_and_key_0600() {
        let dir = temp_dir("acme-write-modes");
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        let (cert_pem, key_pem) = mock_self_signed_pems(&["localhost".into()]).unwrap();
        write_issued_pems(&paths, &IssuedCert { cert_pem, key_pem }).unwrap();
        let cert_mode = std::fs::metadata(&paths.cert_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        let key_mode = std::fs::metadata(&paths.key_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(cert_mode, 0o644);
        assert_eq!(key_mode, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn require_owner_only_secret_file_refuses_world_readable_account() {
        let dir = temp_dir("acme-account-mode");
        let path = dir.join("account.json");
        std::fs::write(&path, b"{}").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = require_owner_only_secret_file(&path, "ACME account credentials").unwrap_err();
        assert!(err.contains("group/world"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn instant_acme_dns01_issuer_constructs_without_network() {
        // Live path type must stay wired; do not call issue() (would hit network).
        let dns: Arc<dyn DnsProvider> = Arc::new(MockDnsProvider::new());
        let _issuer = InstantAcmeDns01Issuer::new(dns);
    }

    fn write_self_signed(dir: &Path, domains: &[String]) -> TlsPaths {
        let (cert_pem, key_pem) = mock_self_signed_pems(domains).unwrap();
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        std::fs::write(&paths.cert_path, cert_pem).unwrap();
        write_secret_file(&paths.key_path, key_pem.as_bytes()).unwrap();
        paths
    }

    fn write_expired_self_signed(dir: &Path, domains: &[String]) -> TlsPaths {
        let mut params = rcgen::CertificateParams::new(domains.to_vec()).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, domains[0].as_str());
        // notAfter in the past relative to SystemTime::now().
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::days(40);
        params.not_after = now - time::Duration::days(10);
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        std::fs::write(&paths.cert_path, cert.pem()).unwrap();
        write_secret_file(&paths.key_path, key_pair.serialize_pem().as_bytes()).unwrap();
        paths
    }

    /// Self-signed with notAfter = now + `days_until_expiry` (for early-renew tests).
    fn write_self_signed_not_after_days(
        dir: &Path,
        domains: &[String],
        days_until_expiry: i64,
    ) -> TlsPaths {
        let mut params = rcgen::CertificateParams::new(domains.to_vec()).unwrap();
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, domains[0].as_str());
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now - time::Duration::days(1);
        params.not_after = now + time::Duration::days(days_until_expiry);
        let key_pair = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key_pair).unwrap();
        let paths = TlsPaths::new(dir.join("cert.pem"), dir.join("key.pem"));
        std::fs::write(&paths.cert_path, cert.pem()).unwrap();
        write_secret_file(&paths.key_path, key_pair.serialize_pem().as_bytes()).unwrap();
        paths
    }

    /// Hermetic hook script: logs argv lines; implements set/clear/wait per body.
    /// Write+fsync+rename so exec is not ETXTBSY (os error 26) on overlay/tmp.
    fn write_hook_script(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::io::Write;
        let path = dir.join(name);
        let tmp = dir.join(format!(".{name}.{}.writing", std::process::id()));
        let script = format!("#!/bin/sh\nset -eu\n{body}\n");
        {
            let mut f = std::fs::File::create(&tmp).unwrap();
            f.write_all(script.as_bytes()).unwrap();
            f.sync_all().unwrap();
        }
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::rename(&tmp, &path).unwrap();
        path
    }

    #[test]
    fn dns_provider_kind_parses_external_hook_aliases() {
        assert_eq!(
            DnsProviderKind::parse("external-hook").unwrap(),
            DnsProviderKind::ExternalHook
        );
        assert_eq!(
            DnsProviderKind::parse("hook").unwrap(),
            DnsProviderKind::ExternalHook
        );
        assert_eq!(DnsProviderKind::ExternalHook.as_str(), "external-hook");
    }

    #[test]
    fn acme_config_external_hook_incomplete_fails() {
        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            "SURMOUNT_ACME_DNS_PROVIDER" => Some("external-hook".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("DNS_HOOK") || err.contains("hook"), "{err}");

        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            "SURMOUNT_ACME_DNS_PROVIDER" => Some("hook".into()),
            "SURMOUNT_ACME_DNS_HOOK" => Some("relative/hook.sh".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn acme_config_external_hook_complete_ok() {
        let cfg = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            "SURMOUNT_ACME_DNS_PROVIDER" => Some("external-hook".into()),
            "SURMOUNT_ACME_DNS_HOOK" => Some("/run/surmount/acme-dns-hook".into()),
            "SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS" => Some("45".into()),
            "SURMOUNT_ACME_RENEW_DAYS_BEFORE_EXPIRY" => Some("14".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(cfg.dns_provider, DnsProviderKind::ExternalHook);
        assert_eq!(
            cfg.dns_hook_path,
            PathBuf::from("/run/surmount/acme-dns-hook")
        );
        assert_eq!(cfg.dns_hook_timeout_secs, 45);
        assert_eq!(cfg.renew_days_before_expiry, 14);
    }

    #[test]
    fn acme_config_mock_refuses_production_le_directory() {
        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_PRODUCTION.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            "SURMOUNT_ACME_DNS_PROVIDER" => Some("mock".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("mock") && (err.contains("production") || err.contains("Let's Encrypt")),
            "{err}"
        );
    }

    #[test]
    fn acme_config_default_renew_days_is_30() {
        let cfg = AcmeConfig::from_env_map(|_| None).unwrap();
        assert_eq!(
            cfg.renew_days_before_expiry,
            DEFAULT_ACME_RENEW_DAYS_BEFORE_EXPIRY
        );
        assert_eq!(
            cfg.dns_hook_timeout_secs,
            DEFAULT_ACME_DNS_HOOK_TIMEOUT_SECS
        );
    }

    #[tokio::test]
    async fn external_hook_set_clear_argv_and_wait_unsupported() {
        let dir = temp_dir("acme-hook-set-clear");
        let log = dir.join("argv.log");
        let log_path = log.display().to_string();
        let hook = write_hook_script(
            &dir,
            "hook.sh",
            &format!(
                r#"log="{log_path}"
printf '%s\n' "$*" >> "$log"
case "$1" in
  set) exit 0 ;;
  clear) exit 0 ;;
  wait) exit 2 ;;
  *) exit 1 ;;
esac"#
            ),
        );
        let dns = ExternalHookDnsProvider::new(hook, 10).unwrap();
        dns.set_txt("_acme-challenge.example.test", "tok-value")
            .await
            .unwrap();
        dns.wait_propagated("_acme-challenge.example.test", "tok-value")
            .await
            .expect("exit 2 wait must succeed as unsupported");
        dns.clear_txt("_acme-challenge.example.test").await.unwrap();
        let log_body = std::fs::read_to_string(&log).unwrap();
        assert!(
            log_body.contains("set _acme-challenge.example.test tok-value"),
            "set argv: {log_body}"
        );
        assert!(
            log_body.contains("wait _acme-challenge.example.test tok-value"),
            "wait argv: {log_body}"
        );
        assert!(
            log_body.contains("clear _acme-challenge.example.test"),
            "clear argv: {log_body}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn external_hook_wait_fail_then_clear_on_cleanup_path() {
        let dir = temp_dir("acme-hook-wait-fail");
        let log = dir.join("argv.log");
        let log_path = log.display().to_string();
        // wait always fails (exit 1) so publish_wait errors; clear still runs.
        let hook = write_hook_script(
            &dir,
            "hook-fail-wait.sh",
            &format!(
                r#"log="{log_path}"
printf '%s\n' "$*" >> "$log"
case "$1" in
  set) exit 0 ;;
  clear) exit 0 ;;
  wait) exit 1 ;;
  *) exit 1 ;;
esac"#
            ),
        );
        let dns = ExternalHookDnsProvider::new(hook, 30).unwrap();
        let name = "_acme-challenge.example.test";
        let val = "tok";
        let mut published = Vec::new();
        dns01_set_txt_and_track(&dns, name, val, &mut published)
            .await
            .expect("set_txt must succeed before wait fail");
        assert_eq!(
            published,
            vec![name.to_string()],
            "FQDN must be tracked immediately after set_txt"
        );
        let result = dns.wait_propagated(name, val).await;
        clear_published_txt(&dns, &published).await;
        assert!(result.is_err(), "wait fail must error: {result:?}");
        let log_body = std::fs::read_to_string(&log).unwrap();
        assert!(
            log_body.contains("clear _acme-challenge.example.test"),
            "cleanup clear must run after wait fail: {log_body}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn issuer_from_config_external_hook_requires_executable_mock_does_not() {
        // Distinguisher: mock needs no filesystem hook; external-hook fails without
        // a valid executable and succeeds with one (live InstantAcmeDns01Issuer).
        let mut acme = AcmeConfig {
            enable: true,
            directory_url: directories::LETS_ENCRYPT_STAGING.into(),
            email: "ops@example.test".into(),
            domains: vec!["services.example.test".into()],
            account_credentials_path: PathBuf::from("/run/surmount-secrets/acme/account.json"),
            challenge: AcmeChallenge::Dns01,
            dns_provider: DnsProviderKind::Mock,
            ..AcmeConfig::default()
        };
        assert!(
            issuer_from_config(&acme).unwrap().is_some(),
            "mock issuer must not require SURMOUNT_ACME_DNS_HOOK"
        );

        acme.dns_provider = DnsProviderKind::ExternalHook;
        acme.dns_hook_path = PathBuf::from("/no/such/surmount-acme-dns-hook");
        let err = match issuer_from_config(&acme) {
            Err(e) => e,
            Ok(_) => panic!("external-hook without executable must fail-closed"),
        };
        assert!(
            err.contains("hook") || err.contains("missing") || err.contains("unreadable"),
            "external-hook without executable must fail-closed: {err}"
        );

        let dir = temp_dir("acme-hook-issuer");
        let hook = write_hook_script(
            &dir,
            "hook-ok.sh",
            r#"case "$1" in
  set|clear) exit 0 ;;
  wait) exit 2 ;;
  *) exit 1 ;;
esac"#,
        );
        acme.dns_hook_path = hook;
        acme.dns_hook_timeout_secs = 10;
        let issuer = issuer_from_config(&acme)
            .expect("valid hook path")
            .expect("external-hook must select live InstantAcmeDns01Issuer");
        // Do not call issue() (would hit network).
        drop(issuer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_hook_refuses_relative_or_non_executable() {
        let dir = temp_dir("acme-hook-validate");
        let rel = PathBuf::from("not/absolute/hook");
        let err = ExternalHookDnsProvider::new(rel, 10).unwrap_err();
        assert!(err.contains("absolute"), "{err}");

        let path = dir.join("noexec");
        std::fs::write(&path, b"#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let err = ExternalHookDnsProvider::new(path, 10).unwrap_err();
        assert!(err.contains("executable") || err.contains("mode"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_hook_refuses_group_or_world_writable() {
        let dir = temp_dir("acme-hook-writable");
        let path = dir.join("world-write.sh");
        std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
        // Executable + world-writable: refuse.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).unwrap();
        let err = ExternalHookDnsProvider::new(path.clone(), 10).unwrap_err();
        assert!(
            err.contains("group/world-writable") || err.contains("writable"),
            "{err}"
        );
        // Owner-only write + group/other execute (0755) is ok.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        ExternalHookDnsProvider::new(path, 10).expect("0755 must be accepted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_hook_refuses_symlink() {
        let dir = temp_dir("acme-hook-symlink");
        let target = write_hook_script(&dir, "real-hook.sh", "exit 0\n");
        let link = dir.join("hook-link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let err = ExternalHookDnsProvider::new(link, 10).unwrap_err();
        assert!(err.contains("symlink"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_hook_timeout_capped_1_to_600() {
        let dir = temp_dir("acme-hook-timeout");
        let hook = write_hook_script(&dir, "t.sh", "exit 0\n");
        let err = ExternalHookDnsProvider::new(hook.clone(), 0).unwrap_err();
        assert!(err.contains("1") || err.contains("timeout"), "{err}");
        let err = ExternalHookDnsProvider::new(hook, 601).unwrap_err();
        assert!(err.contains("600") || err.contains("timeout"), "{err}");

        let err = AcmeConfig::from_env_map(|k| match k {
            "SURMOUNT_ACME_ENABLE" => Some("1".into()),
            "SURMOUNT_ACME_DOMAINS" => Some("services.example.test".into()),
            "SURMOUNT_ACME_EMAIL" => Some("ops@example.test".into()),
            "SURMOUNT_ACME_DIRECTORY" => Some(directories::LETS_ENCRYPT_STAGING.into()),
            "SURMOUNT_ACME_ACCOUNT_CREDENTIALS_PATH" => {
                Some("/run/surmount-secrets/acme/account.json".into())
            }
            "SURMOUNT_ACME_DNS_PROVIDER" => Some("external-hook".into()),
            "SURMOUNT_ACME_DNS_HOOK" => Some("/run/surmount/acme-dns-hook".into()),
            "SURMOUNT_ACME_DNS_HOOK_TIMEOUT_SECS" => Some("9999".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(err.contains("600") || err.contains("TIMEOUT"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn external_hook_child_does_not_see_parent_session_secret() {
        let dir = temp_dir("acme-hook-env");
        let hook = write_hook_script(&dir, "env-check.sh", "exit 0\n");
        let dns = ExternalHookDnsProvider::new(hook, 10).unwrap();
        let cmd = dns.hook_command(&["set", "_acme-challenge.example.test", "tok"]);
        let names: Vec<std::ffi::OsString> =
            cmd.get_envs().map(|(k, _)| k.to_os_string()).collect();
        assert_eq!(names, vec![std::ffi::OsString::from("PATH")]);
        assert!(
            cmd.get_envs()
                .all(|(k, _)| k != std::ffi::OsStr::new("SURMOUNT_SESSION_SECRET")),
            "hook child env must not include SURMOUNT_SESSION_SECRET"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn truncate_hook_bytes_utf8_char_boundary_safe() {
        // Multi-byte UTF-8 (emoji + CJK) must not panic when max cuts mid-scalar.
        let bytes = "ab\u{1F600}\u{4E2D}cd".as_bytes();
        let t = truncate_hook_bytes(bytes, 3);
        assert!(t.ends_with("..."), "{t}");
        assert_eq!(t.chars().count(), 3 + 3); // 3 chars + "..."
        // Empty / short paths.
        assert_eq!(truncate_hook_bytes(b"hi", 10), "hi");
        assert_eq!(truncate_hook_bytes(b"x\ny", 10), "x y");
    }
}
