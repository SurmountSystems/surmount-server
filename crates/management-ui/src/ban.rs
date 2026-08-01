//! Ban decision layer + backends for merciless access control (first product path).
//!
//! - Pure decisions: Allow / Whitelisted / RateLimited / Banned / BanCandidate
//! - Whitelist never banned; last-used touched on allowed traffic from a match
//! - Memory backend for hermetic tests and default app-level reject
//! - Optional nft sync via [`crate::nft_helper::BanNftApply`]: hermetic
//!   `NftExec` mock, direct `ProcessNftExec` (unsupported on UI without caps),
//!   or **least-privilege helper client** (`HelperNftClient` ->
//!   `surmount-nft-ban-helper`). UI default path does **not** need CAP_NET_ADMIN.
//! - Enforcement default **off** (lean private); dry-run or enforce by operator
//! - **DryRun records app bans only; does not mutate nft** (no helper/process exec)
//! - **Enforce** + helper/exec: fail-closed on sync error (apply-first; no
//!   durable write if privileged apply fails)
//!
//! Policy knobs **Q-ACL-1..6 remain open** (unauthorized signals, duration,
//! IPv6 granularity, mail AUTH, etc.). This module does not invent answers.
//!
//! Dual-run IP keys: same contract as rate limit (`client_ip_for_rate_limit`:
//! X-Real-IP only from loopback peers; XFF ignored).

use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::rate_limit::RateLimitDecision;

/// nftables table name (inet family) used by the Nix access-control fragment.
pub const NFT_TABLE: &str = "surmount_guard";
/// IPv4 ban set (module + helper must match).
pub const NFT_SET_BAN4: &str = "surmount-ban4";
/// IPv6 ban set.
pub const NFT_SET_BAN6: &str = "surmount-ban6";
/// IPv4 whitelist set (never ban; last-used is app-side hygiene).
pub const NFT_SET_WHITELIST4: &str = "surmount-whitelist4";
/// IPv6 whitelist set.
pub const NFT_SET_WHITELIST6: &str = "surmount-whitelist6";

/// How strongly the edge applies ban decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanEnforcement {
    /// Default: compute decisions for tests/hooks; no 403, no durable ban write.
    Off,
    /// Record intended bans (memory + optional nft command log); do not 403.
    DryRun,
    /// 403 banned clients; persist bans via backend; optional nft sync.
    Enforce,
}

impl BanEnforcement {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "0" | "false" | "no" => Ok(Self::Off),
            "dry-run" | "dryrun" | "dry_run" => Ok(Self::DryRun),
            "enforce" | "on" | "1" | "true" | "yes" => Ok(Self::Enforce),
            other => Err(format!(
                "invalid SURMOUNT_BAN_ENFORCEMENT={other:?}; expected off|dry-run|enforce"
            )),
        }
    }

    pub fn applies_rejects(self) -> bool {
        matches!(self, Self::Enforce)
    }

    pub fn records_bans(self) -> bool {
        matches!(self, Self::DryRun | Self::Enforce)
    }
}

/// Why a ban signal was raised (auth residual; Q-ACL-1 open).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BanReason {
    /// Explicit unauthorized / auth-failure style signal (definition open).
    Unauthorized,
    /// Escalation after rate-limit pressure (threshold not product-locked).
    RateLimitEscalation,
}

impl BanReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::RateLimitEscalation => "rate_limit_escalation",
        }
    }
}

/// Outcome of combining whitelist, ban list, and rate limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessDecision {
    Allowed,
    Whitelisted,
    RateLimited {
        retry_after: Duration,
    },
    Banned,
    /// Unauthorized / ban-signal path only ([`decide_ban_signal`] /
    /// [`BanGuard::signal_unauthorized`]); not emitted by ordinary GETs.
    BanCandidate {
        reason: BanReason,
    },
}

/// Durable / host ban backend.
pub trait BanBackend: Send + Sync {
    fn is_whitelisted(&self, ip: IpAddr) -> bool;
    fn is_banned(&self, ip: IpAddr) -> bool;
    /// Insert ban if not whitelisted. Whitelist => Ok(false) no-op.
    fn ban(&self, ip: IpAddr, reason: BanReason) -> Result<bool, String>;
    /// Touch last-used when `ip` matches whitelist. Returns true if matched.
    fn touch_whitelist_last_used(&self, ip: IpAddr, now: SystemTime) -> bool;
    fn whitelist_last_used(&self, ip: IpAddr) -> Option<SystemTime>;
}

/// Pure combine: whitelist wins; then ban; then rate limit.
pub fn decide_access(
    ip: IpAddr,
    backend: &dyn BanBackend,
    rate: RateLimitDecision,
) -> AccessDecision {
    if backend.is_whitelisted(ip) {
        return AccessDecision::Whitelisted;
    }
    if backend.is_banned(ip) {
        return AccessDecision::Banned;
    }
    match rate {
        RateLimitDecision::Allowed { .. } => AccessDecision::Allowed,
        RateLimitDecision::Limited { retry_after } => AccessDecision::RateLimited { retry_after },
    }
}

/// Pure view of an unauthorized / ban signal **before** enforcement recording.
///
/// - Whitelist => [`AccessDecision::Whitelisted`] (never a candidate)
/// - Already banned => [`AccessDecision::Banned`]
/// - Else => [`AccessDecision::BanCandidate`] (ordinary request path never emits this)
///
/// Does not write durable state. Pair with [`apply_ban_signal`] /
/// [`BanGuard::signal_unauthorized`]. Q-ACL-1 (which surfaces raise this) stays open.
pub fn decide_ban_signal(
    backend: &dyn BanBackend,
    ip: IpAddr,
    reason: BanReason,
) -> AccessDecision {
    if backend.is_whitelisted(ip) {
        return AccessDecision::Whitelisted;
    }
    if backend.is_banned(ip) {
        return AccessDecision::Banned;
    }
    AccessDecision::BanCandidate { reason }
}

/// Apply a ban signal under enforcement policy. Whitelist never banned.
pub fn apply_ban_signal(
    enforcement: BanEnforcement,
    backend: &dyn BanBackend,
    ip: IpAddr,
    reason: BanReason,
) -> Result<BanSignalOutcome, String> {
    if backend.is_whitelisted(ip) {
        return Ok(BanSignalOutcome::SkippedWhitelisted);
    }
    if !enforcement.records_bans() {
        return Ok(BanSignalOutcome::IgnoredOff);
    }
    let wrote = backend.ban(ip, reason)?;
    Ok(if wrote {
        BanSignalOutcome::Recorded { reason }
    } else {
        BanSignalOutcome::AlreadyBanned
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanSignalOutcome {
    IgnoredOff,
    SkippedWhitelisted,
    AlreadyBanned,
    Recorded { reason: BanReason },
}

// --- CIDR helpers (no extra crate) ------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhitelistEntry {
    pub network: IpAddr,
    pub prefix_len: u8,
    pub label: String,
}

impl WhitelistEntry {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("empty whitelist entry".into());
        }
        let (addr_s, prefix_s) = match raw.split_once('/') {
            Some((a, p)) => (a.trim(), Some(p.trim())),
            None => (raw, None),
        };
        let network: IpAddr = addr_s
            .parse()
            .map_err(|e| format!("whitelist entry {raw:?}: bad IP: {e}"))?;
        let prefix_len = match prefix_s {
            None => match network {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            },
            Some(p) => p
                .parse::<u8>()
                .map_err(|e| format!("whitelist entry {raw:?}: bad prefix: {e}"))?,
        };
        let max = match network {
            IpAddr::V4(_) => 32u8,
            IpAddr::V6(_) => 128u8,
        };
        if prefix_len > max {
            return Err(format!(
                "whitelist entry {raw:?}: prefix {prefix_len} > {max}"
            ));
        }
        Ok(Self {
            network,
            prefix_len,
            label: raw.to_string(),
        })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(n), IpAddr::V4(a)) => {
                let shift = 32u32.saturating_sub(u32::from(self.prefix_len));
                let mask = if shift >= 32 { 0 } else { u32::MAX << shift };
                (u32::from(n) & mask) == (u32::from(a) & mask)
            }
            (IpAddr::V6(n), IpAddr::V6(a)) => {
                let n = u128::from(n);
                let a = u128::from(a);
                let shift = 128u32.saturating_sub(u32::from(self.prefix_len));
                let mask = if shift >= 128 { 0 } else { u128::MAX << shift };
                (n & mask) == (a & mask)
            }
            _ => false,
        }
    }
}

/// Parse comma-separated whitelist CIDRs (empty => none).
pub fn parse_whitelist_cidrs(raw: &str) -> Result<Vec<WhitelistEntry>, String> {
    let mut out = Vec::new();
    for part in raw.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        out.push(WhitelistEntry::parse(p)?);
    }
    Ok(out)
}

// --- Memory + optional file state -------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BanStateFile {
    bans: HashMap<String, BanRecord>,
    /// IP string -> unix secs last_used for whitelist hygiene.
    whitelist_last_used: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BanRecord {
    reason: BanReason,
    at_unix: u64,
}

fn system_time_to_unix(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn unix_to_system_time(secs: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(secs)
}

struct MemoryInner {
    whitelist: Vec<WhitelistEntry>,
    bans: HashMap<IpAddr, BanRecord>,
    whitelist_last_used: HashMap<IpAddr, SystemTime>,
    state_path: Option<PathBuf>,
}

impl MemoryInner {
    fn load(whitelist: Vec<WhitelistEntry>, state_path: Option<PathBuf>) -> Result<Self, String> {
        let mut inner = Self {
            whitelist,
            bans: HashMap::new(),
            whitelist_last_used: HashMap::new(),
            state_path: state_path.clone(),
        };
        if let Some(path) = state_path {
            if path.exists() {
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| format!("read ban state {}: {e}", path.display()))?;
                let file: BanStateFile = serde_json::from_str(&raw).map_err(|e| {
                    format!("ban state {} is corrupt (fail-closed): {e}", path.display())
                })?;
                for (k, rec) in file.bans {
                    let ip: IpAddr = k.parse().map_err(|e| {
                        format!("ban state {} bad ban key {k:?}: {e}", path.display())
                    })?;
                    inner.bans.insert(ip, rec);
                }
                for (k, secs) in file.whitelist_last_used {
                    let ip: IpAddr = k.parse().map_err(|e| {
                        format!("ban state {} bad whitelist key {k:?}: {e}", path.display())
                    })?;
                    inner
                        .whitelist_last_used
                        .insert(ip, unix_to_system_time(secs));
                }
            }
        }
        Ok(inner)
    }

    fn persist_locked(&self) -> Result<(), String> {
        let Some(path) = &self.state_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create ban state parent {}: {e}", parent.display()))?;
        }
        let mut file = BanStateFile::default();
        for (ip, rec) in &self.bans {
            file.bans.insert(ip.to_string(), rec.clone());
        }
        for (ip, t) in &self.whitelist_last_used {
            file.whitelist_last_used
                .insert(ip.to_string(), system_time_to_unix(*t));
        }
        let raw =
            serde_json::to_string_pretty(&file).map_err(|e| format!("serialize ban state: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, raw)
            .map_err(|e| format!("write ban state temp {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("rename ban state {}: {e}", path.display()))?;
        Ok(())
    }

    fn match_whitelist(&self, ip: IpAddr) -> bool {
        self.whitelist.iter().any(|e| e.contains(ip))
    }
}

/// In-memory ban/whitelist store; optional JSON file durability.
pub struct MemoryBanBackend {
    inner: Mutex<MemoryInner>,
}

impl MemoryBanBackend {
    pub fn new(whitelist: Vec<WhitelistEntry>) -> Self {
        Self {
            inner: Mutex::new(MemoryInner {
                whitelist,
                bans: HashMap::new(),
                whitelist_last_used: HashMap::new(),
                state_path: None,
            }),
        }
    }

    pub fn with_state_path(
        whitelist: Vec<WhitelistEntry>,
        state_path: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        let path = state_path.into();
        let inner = MemoryInner::load(whitelist, Some(path))?;
        Ok(Self {
            inner: Mutex::new(inner),
        })
    }

    /// Remove a ban (used to roll back memory when nft sync fails).
    pub fn unban(&self, ip: IpAddr) -> Result<(), String> {
        let mut g = self.inner.lock().expect("ban mutex");
        g.bans.remove(&ip);
        g.persist_locked()
    }
}

impl BanBackend for MemoryBanBackend {
    fn is_whitelisted(&self, ip: IpAddr) -> bool {
        self.inner.lock().expect("ban mutex").match_whitelist(ip)
    }

    fn is_banned(&self, ip: IpAddr) -> bool {
        self.inner.lock().expect("ban mutex").bans.contains_key(&ip)
    }

    fn ban(&self, ip: IpAddr, reason: BanReason) -> Result<bool, String> {
        let mut g = self.inner.lock().expect("ban mutex");
        if g.match_whitelist(ip) {
            return Ok(false);
        }
        if g.bans.contains_key(&ip) {
            return Ok(false);
        }
        g.bans.insert(
            ip,
            BanRecord {
                reason,
                at_unix: system_time_to_unix(SystemTime::now()),
            },
        );
        g.persist_locked()?;
        Ok(true)
    }

    fn touch_whitelist_last_used(&self, ip: IpAddr, now: SystemTime) -> bool {
        let mut g = self.inner.lock().expect("ban mutex");
        if !g.match_whitelist(ip) {
            return false;
        }
        g.whitelist_last_used.insert(ip, now);
        // Best-effort persist; hygiene must not break request path.
        let _ = g.persist_locked();
        true
    }

    fn whitelist_last_used(&self, ip: IpAddr) -> Option<SystemTime> {
        self.inner
            .lock()
            .expect("ban mutex")
            .whitelist_last_used
            .get(&ip)
            .copied()
    }
}

// --- nft exec interface (hermetic via mock) ---------------------------------

/// Runs `nft` argv fragments (no shell). Production may use process exec;
/// CI uses [`RecordingNftExec`].
pub trait NftExec: Send + Sync {
    fn run_nft(&self, args: &[String]) -> Result<(), String>;
}

/// Records argv lists; never touches the host firewall (hermetic tests).
#[cfg(test)]
#[derive(Debug, Default)]
pub struct RecordingNftExec {
    pub calls: Mutex<Vec<Vec<String>>>,
}

#[cfg(test)]
impl NftExec for RecordingNftExec {
    fn run_nft(&self, args: &[String]) -> Result<(), String> {
        self.calls
            .lock()
            .expect("nft record mutex")
            .push(args.to_vec());
        Ok(())
    }
}

/// No-op runner (nft sync disabled).
#[derive(Debug, Default)]
pub struct NoopNftExec;

impl NftExec for NoopNftExec {
    fn run_nft(&self, _args: &[String]) -> Result<(), String> {
        Ok(())
    }
}

/// Canonical set names for Nix / ops contract checks.
pub fn nft_product_set_names() -> &'static [(&'static str, &'static str)] {
    // Keep whitelist constants linked in non-test builds (product name SoT).
    &[
        (NFT_TABLE, "table"),
        (NFT_SET_BAN4, "ban4"),
        (NFT_SET_BAN6, "ban6"),
        (NFT_SET_WHITELIST4, "whitelist4"),
        (NFT_SET_WHITELIST6, "whitelist6"),
    ]
}

/// Build `nft add element inet <table> <set> { <ip> }` argv (no binary path).
pub fn nft_add_ban_args(ip: IpAddr) -> Vec<String> {
    let set = match ip {
        IpAddr::V4(_) => NFT_SET_BAN4,
        IpAddr::V6(_) => NFT_SET_BAN6,
    };
    vec![
        "add".into(),
        "element".into(),
        "inet".into(),
        NFT_TABLE.into(),
        set.into(),
        "{".into(),
        ip.to_string(),
        "}".into(),
    ]
}

/// Build `nft delete element inet <table> <set> { <ip> }` argv (no binary path).
/// Mirrors [`nft_add_ban_args`] for lab unban / cleanup via the ban helper.
pub fn nft_remove_ban_args(ip: IpAddr) -> Vec<String> {
    let set = match ip {
        IpAddr::V4(_) => NFT_SET_BAN4,
        IpAddr::V6(_) => NFT_SET_BAN6,
    };
    vec![
        "delete".into(),
        "element".into(),
        "inet".into(),
        NFT_TABLE.into(),
        set.into(),
        "{".into(),
        ip.to_string(),
        "}".into(),
    ]
}

/// Memory decisions + optional nft add-element on ban (via [`BanNftApply`]).
pub struct NftBanBackend<A: BanNftApply> {
    memory: MemoryBanBackend,
    apply: A,
    /// When true, ban() also applies nft add-element (Enforce + helper/exec only).
    nft_sync: bool,
}

impl<A: BanNftApply> NftBanBackend<A> {
    pub fn new(memory: MemoryBanBackend, apply: A, nft_sync: bool) -> Self {
        Self {
            memory,
            apply,
            nft_sync,
        }
    }

    #[cfg(test)]
    pub fn apply(&self) -> &A {
        &self.apply
    }
}

impl<A: BanNftApply> BanBackend for NftBanBackend<A> {
    fn is_whitelisted(&self, ip: IpAddr) -> bool {
        self.memory.is_whitelisted(ip)
    }

    fn is_banned(&self, ip: IpAddr) -> bool {
        self.memory.is_banned(ip)
    }

    fn ban(&self, ip: IpAddr, reason: BanReason) -> Result<bool, String> {
        // Apply-then-durable when nft_sync: privileged host path first so a
        // crash after memory.persist cannot leave app 403 without host drop.
        // If apply fails, memory is never written (no rollback needed).
        // Residual crash window: apply ok then process dies before persist
        // (host drop without app ban). Re-signal self-heals when apply treats
        // already-present nft element as success (see nft_element_already_present).
        if self.memory.is_whitelisted(ip) {
            return Ok(false);
        }
        if self.memory.is_banned(ip) {
            return Ok(false);
        }
        if self.nft_sync {
            self.apply
                .add_ban_element(ip)
                .map_err(|e| format!("nft sync failed (memory not written): {e}"))?;
        }
        self.memory.ban(ip, reason)
    }

    fn touch_whitelist_last_used(&self, ip: IpAddr, now: SystemTime) -> bool {
        self.memory.touch_whitelist_last_used(ip, now)
    }

    fn whitelist_last_used(&self, ip: IpAddr) -> Option<SystemTime> {
        self.memory.whitelist_last_used(ip)
    }
}

// Re-export apply surface for callers / tests.
pub use crate::nft_helper::{BanNftApply, HelperNftClient, NftExecApply, NoopNftApply};

// --- Config / capability gate -----------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BanBackendKind {
    Memory,
    Nft,
}

impl BanBackendKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "memory" | "mem" => Ok(Self::Memory),
            "nft" | "nftables" => Ok(Self::Nft),
            other => Err(format!(
                "invalid SURMOUNT_BAN_BACKEND={other:?}; expected memory|nft"
            )),
        }
    }
}

/// Static config for building the ban subsystem at startup.
#[derive(Debug, Clone)]
pub struct BanConfig {
    pub enforcement: BanEnforcement,
    pub backend_kind: BanBackendKind,
    pub whitelist: Vec<WhitelistEntry>,
    pub state_path: Option<PathBuf>,
    /// Direct in-process `nft` exec (unsupported on UI without CAP_NET_ADMIN).
    pub nft_exec_enabled: bool,
    /// Path to `nft` binary (helper unit env and/or direct exec).
    pub nft_bin: Option<PathBuf>,
    /// Product elevation path: absolute UDS to socket-activated helper.
    /// Preferred over process spawn (NNP-safe; UI never gains CAP_NET_ADMIN).
    pub nft_helper_sock: Option<PathBuf>,
    /// Process-spawn helper path (tests / unsupported under UI NoNewPrivileges).
    /// Do not use as the host elevation path; file caps cannot elevate under NNP.
    pub nft_helper_bin: Option<PathBuf>,
}

impl BanConfig {
    /// True when a helper sock or process bin is configured.
    pub fn helper_configured(&self) -> bool {
        self.nft_helper_sock.is_some() || self.nft_helper_bin.is_some()
    }

    /// True when Enforce would invoke helper or direct nft (nft mutate path).
    pub fn nft_sync_on_enforce(&self) -> bool {
        self.backend_kind == BanBackendKind::Nft
            && (self.helper_configured() || self.nft_exec_enabled)
    }

    /// Fail-closed gate for privileged nft paths.
    ///
    /// - Helper/exec require `backend=nft` (Memory never applies; silent sock
    ///   install is fail-closed).
    /// - **DryRun / Off never require helper or nft bin** (DryRun must not mutate).
    /// - Helper (sock|bin) and direct nft_exec are mutually exclusive.
    /// - Enforce + helper: absolute sock and/or existing process bin; always
    ///   require absolute existing `nft_bin` (helper unit / process needs it).
    /// - Sock path need not exist at UI start (socket unit may create it);
    ///   connect fail-closes at apply time.
    pub fn validate_capable(&self) -> Result<(), String> {
        if self.helper_configured() && self.nft_exec_enabled {
            return Err(
                "SURMOUNT_BAN_NFT_HELPER(_SOCK) and SURMOUNT_BAN_NFT_EXEC are mutually \
                 exclusive (fail-closed); prefer UDS helper (no CAP_NET_ADMIN on UI)"
                    .into(),
            );
        }

        // Memory never constructs HelperNftClient / NftExecApply. Fail-closed
        // if operator configured a privileged path that would be a no-op.
        if self.backend_kind != BanBackendKind::Nft {
            if self.helper_configured() || self.nft_exec_enabled {
                return Err("SURMOUNT_BAN_NFT_HELPER(_SOCK)|NFT_EXEC requires \
                     SURMOUNT_BAN_BACKEND=nft (fail-closed); memory backend never \
                     applies host drop"
                    .into());
            }
            return Ok(());
        }

        let helper = self.helper_configured();
        let direct = self.nft_exec_enabled;
        if !helper && !direct {
            // nft kind but sync-off: app-level memory bans only (no host drop).
            return Ok(());
        }

        // Only Enforce mutates nft; DryRun/Off skip capable bin checks.
        if !matches!(self.enforcement, BanEnforcement::Enforce) {
            return Ok(());
        }

        if helper {
            if let Some(s) = &self.nft_helper_sock {
                if s.as_os_str().is_empty() || !s.is_absolute() {
                    return Err(
                        "SURMOUNT_BAN_NFT_HELPER_SOCK must be an absolute path (fail-closed)"
                            .into(),
                    );
                }
            }
            if let Some(h) = &self.nft_helper_bin {
                if h.as_os_str().is_empty() || !h.is_absolute() {
                    return Err(
                        "SURMOUNT_BAN_NFT_HELPER must be an absolute path (fail-closed)".into(),
                    );
                }
                // Process path: require exists (tests / unsupported host elevation).
                if self.nft_helper_sock.is_none() && !h.exists() {
                    return Err(format!(
                        "SURMOUNT_BAN_NFT_HELPER requires existing helper at {} \
                         (fail-closed; enforcement=enforce). Prefer \
                         SURMOUNT_BAN_NFT_HELPER_SOCK (socket-activated unit).",
                        h.display()
                    ));
                }
            }
            // Helper (sock or process) always needs nft bin for the privileged side.
            return match &self.nft_bin {
                Some(p) if !p.as_os_str().is_empty() && p.is_absolute() && p.exists() => Ok(()),
                Some(p) if p.is_absolute() => Err(format!(
                    "SURMOUNT_BAN_NFT_BIN missing at {} (fail-closed; helper needs nft)",
                    p.display()
                )),
                _ => Err(
                    "Enforce + nft helper requires SURMOUNT_BAN_NFT_BIN absolute \
                     existing path (fail-closed)"
                        .into(),
                ),
            };
        }

        // Direct nft_exec path (unsupported on default UI unit).
        match &self.nft_bin {
            Some(p) if !p.as_os_str().is_empty() && p.is_absolute() => {
                if !p.exists() {
                    return Err(format!(
                        "SURMOUNT_BAN_NFT_EXEC with backend=nft requires existing \
                         nft binary at {} (fail-closed; enforcement=enforce)",
                        p.display()
                    ));
                }
                Ok(())
            }
            _ => Err(
                "SURMOUNT_BAN_NFT_EXEC=1 with SURMOUNT_BAN_BACKEND=nft requires \
                 SURMOUNT_BAN_NFT_BIN absolute path to nft (fail-closed)"
                    .into(),
            ),
        }
    }
}

/// Parse a tri-state-ish env bool: unset => default; known truthy/falsey;
/// anything else fail-closed (not silent skip).
pub fn parse_env_bool_strict(raw: &str, var_name: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" => Err(format!(
            "{var_name} is set but empty (fail-closed); use 0|false|no|off or 1|true|yes|on"
        )),
        "0" | "false" | "no" | "off" => Ok(false),
        "1" | "true" | "yes" | "on" => Ok(true),
        other => Err(format!(
            "invalid {var_name}={other:?}; expected 0|false|no|off or 1|true|yes|on (fail-closed)"
        )),
    }
}

/// Parse ban config from environment (invalid values fail closed, not skip).
pub fn ban_config_from_env(get: impl Fn(&str) -> Option<String>) -> Result<BanConfig, String> {
    let enforcement = match get("SURMOUNT_BAN_ENFORCEMENT") {
        None => BanEnforcement::Off,
        Some(s) => BanEnforcement::parse(&s)?,
    };
    let backend_kind = match get("SURMOUNT_BAN_BACKEND") {
        None => BanBackendKind::Memory,
        Some(s) => BanBackendKind::parse(&s)?,
    };
    let whitelist = match get("SURMOUNT_BAN_WHITELIST") {
        None => Vec::new(),
        Some(s) => parse_whitelist_cidrs(&s)?,
    };
    let state_path = match get("SURMOUNT_BAN_STATE_PATH") {
        None => None,
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                let p = PathBuf::from(t);
                if !p.is_absolute() {
                    return Err(format!(
                        "SURMOUNT_BAN_STATE_PATH must be absolute (fail-closed); got {t:?}"
                    ));
                }
                Some(p)
            }
        }
    };
    let nft_exec_enabled = match get("SURMOUNT_BAN_NFT_EXEC") {
        None => false,
        Some(s) => parse_env_bool_strict(&s, "SURMOUNT_BAN_NFT_EXEC")?,
    };
    let nft_bin = match get("SURMOUNT_BAN_NFT_BIN") {
        None => None,
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(PathBuf::from(t))
            }
        }
    };
    let nft_helper_bin = match get("SURMOUNT_BAN_NFT_HELPER") {
        None => None,
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                let p = PathBuf::from(t);
                if !p.is_absolute() {
                    return Err(format!(
                        "SURMOUNT_BAN_NFT_HELPER must be absolute (fail-closed); got {t:?}"
                    ));
                }
                Some(p)
            }
        }
    };
    let nft_helper_sock = match get("SURMOUNT_BAN_NFT_HELPER_SOCK") {
        None => None,
        Some(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                let p = PathBuf::from(t);
                if !p.is_absolute() {
                    return Err(format!(
                        "SURMOUNT_BAN_NFT_HELPER_SOCK must be absolute (fail-closed); got {t:?}"
                    ));
                }
                Some(p)
            }
        }
    };

    let cfg = BanConfig {
        enforcement,
        backend_kind,
        whitelist,
        state_path,
        nft_exec_enabled,
        nft_bin,
        nft_helper_sock,
        nft_helper_bin,
    };
    cfg.validate_capable()?;
    Ok(cfg)
}

/// Process-exec nft runner (direct path; only when operator enables nft_exec).
///
/// Residual: argv shape is hermetic-tested only (`RecordingNftExec`). Real
/// `nft` parser acceptance is host/helper smoke, not CI. UI unit has no
/// CAP_NET_ADMIN by default; **prefer [`HelperNftClient`]** +
/// `surmount-nft-ban-helper` over in-process exec.
pub struct ProcessNftExec {
    bin: PathBuf,
}

impl ProcessNftExec {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self { bin: bin.into() }
    }
}

impl NftExec for ProcessNftExec {
    fn run_nft(&self, args: &[String]) -> Result<(), String> {
        // Capture stderr so callers can detect already-present (File exists).
        let output = std::process::Command::new(&self.bin)
            .args(args)
            .output()
            .map_err(|e| format!("spawn nft {}: {e}", self.bin.display()))?;
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            Err(format!(
                "nft {args:?} exited with {}: stderr={} stdout={}",
                output.status,
                stderr.trim(),
                stdout.trim()
            ))
        }
    }
}

/// True when nft error text means the set element is already present.
///
/// Used so apply-then-durable crash recovery can re-signal: second `nft add
/// element` typically returns "File exists" / EEXIST; treat as apply success
/// so durable app ban can catch up.
pub fn nft_element_already_present(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("file exists")
        || lower.contains("already exists")
        || lower.contains("element already")
        || lower.contains("eexist")
}

/// True when nft error text means the set **element** is already absent.
///
/// Idempotent `delete element` for lab unban/cleanup: missing element is
/// success (mirror of EEXIST-on-add). Deliberately does **not** treat missing
/// table/set infrastructure as success (mirror add path: `table does not exist`
/// is not EEXIST success). Kernel messages vary by version.
pub fn nft_element_already_absent(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    // Infrastructure missing: fail-closed (do not hide host misconfig on cleanup).
    if lower.contains("table does not exist")
        || lower.contains("set does not exist")
        || lower.contains("no such table")
        || lower.contains("no such set")
        || lower.contains("did you mean table")
    {
        return false;
    }
    // Element-oriented wording.
    if lower.contains("no such element")
        || lower.contains("element does not exist")
        || lower.contains("element not found")
        || lower.contains("element is not in")
    {
        return true;
    }
    // Common nft delete-missing-element when table/set exist:
    // "Error: Could not process rule: No such file or directory"
    // Require process-rule context so bare "no such file" / "not found" alone
    // is not success.
    if lower.contains("process rule")
        && (lower.contains("no such file") || lower.contains("enoent"))
    {
        return true;
    }
    false
}

/// Runtime guard held in AppState (erased backend).
pub struct BanGuard {
    pub enforcement: BanEnforcement,
    backend: Box<dyn BanBackend>,
}

impl BanGuard {
    pub fn from_config(cfg: BanConfig) -> Result<Self, String> {
        // Product set-name SoT must stay linked (Nix module contract).
        if nft_product_set_names().is_empty() {
            return Err("internal: nft product set names empty".into());
        }

        let memory = match &cfg.state_path {
            Some(p) => MemoryBanBackend::with_state_path(cfg.whitelist.clone(), p)?,
            None => MemoryBanBackend::new(cfg.whitelist.clone()),
        };

        let backend: Box<dyn BanBackend> = match cfg.backend_kind {
            BanBackendKind::Memory => Box::new(memory),
            BanBackendKind::Nft => {
                // DryRun/Off: never mutate nft (helper or direct). Enforce only.
                let nft_sync =
                    cfg.nft_sync_on_enforce() && matches!(cfg.enforcement, BanEnforcement::Enforce);
                if nft_sync {
                    // Prefer UDS (socket-activated oneshot). Process bin is
                    // tests-only / unsupported under UI NoNewPrivileges.
                    if let Some(sock) = cfg.nft_helper_sock.clone() {
                        let client = HelperNftClient::unix_socket(sock);
                        Box::new(NftBanBackend::new(memory, client, true))
                    } else if let Some(helper) = cfg.nft_helper_bin.clone() {
                        let client =
                            HelperNftClient::process(helper).with_nft_bin(cfg.nft_bin.clone());
                        Box::new(NftBanBackend::new(memory, client, true))
                    } else if cfg.nft_exec_enabled {
                        let bin = cfg.nft_bin.clone().ok_or_else(|| {
                            "nft exec enabled but SURMOUNT_BAN_NFT_BIN unset".to_string()
                        })?;
                        let apply = NftExecApply::new(ProcessNftExec::new(bin));
                        Box::new(NftBanBackend::new(memory, apply, true))
                    } else {
                        Box::new(NftBanBackend::new(memory, NoopNftApply, false))
                    }
                } else {
                    // App-level bans only; nft set names still documented for host.
                    Box::new(NftBanBackend::new(memory, NoopNftApply, false))
                }
            }
        };

        Ok(Self {
            enforcement: cfg.enforcement,
            backend,
        })
    }

    /// Test / dry helper: build with an explicit backend.
    pub fn with_backend(enforcement: BanEnforcement, backend: Box<dyn BanBackend>) -> Self {
        Self {
            enforcement,
            backend,
        }
    }

    pub fn backend(&self) -> &dyn BanBackend {
        self.backend.as_ref()
    }

    pub fn is_whitelisted(&self, ip: IpAddr) -> bool {
        self.backend.is_whitelisted(ip)
    }

    pub fn is_banned(&self, ip: IpAddr) -> bool {
        self.backend.is_banned(ip)
    }

    pub fn decide(&self, ip: IpAddr, rate: RateLimitDecision) -> AccessDecision {
        decide_access(ip, self.backend.as_ref(), rate)
    }

    /// Touch whitelist last-used when request matched whitelist.
    pub fn on_request_allowed(&self, ip: IpAddr) {
        let _ = self
            .backend
            .touch_whitelist_last_used(ip, SystemTime::now());
    }

    pub fn signal_ban(&self, ip: IpAddr, reason: BanReason) -> Result<BanSignalOutcome, String> {
        apply_ban_signal(self.enforcement, self.backend.as_ref(), ip, reason)
    }

    /// Unauthorized / auth-failure style signal (Q-ACL-1 open for full surface list).
    ///
    /// Returns the pre-apply decision ([`AccessDecision::BanCandidate`] when the IP
    /// is neither whitelisted nor already banned) plus the enforcement outcome.
    /// Off: candidate only, no durable write. DryRun/Enforce: record when eligible.
    /// Whitelist never banned. No Nostr / session product here.
    pub fn signal_unauthorized(
        &self,
        ip: IpAddr,
    ) -> Result<(AccessDecision, BanSignalOutcome), String> {
        let reason = BanReason::Unauthorized;
        let decision = decide_ban_signal(self.backend.as_ref(), ip, reason);
        let outcome = apply_ban_signal(self.enforcement, self.backend.as_ref(), ip, reason)?;
        Ok((decision, outcome))
    }
}

/// Whether middleware should 403 for a Banned decision.
pub fn should_reject_banned(enforcement: BanEnforcement, decision: &AccessDecision) -> bool {
    enforcement.applies_rejects() && matches!(decision, AccessDecision::Banned)
}

// --- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rate_limit::RateLimitDecision;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::time::Duration;

    fn ip4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn whitelist_never_banned_and_last_used_touched() {
        let wl = parse_whitelist_cidrs("203.0.113.10/32").unwrap();
        let backend = MemoryBanBackend::new(wl);
        let home = ip4(203, 0, 113, 10);
        let evil = ip4(198, 51, 100, 1);

        assert!(backend.is_whitelisted(home));
        assert!(!backend.is_whitelisted(evil));

        // Ban signal on whitelist is a no-op.
        let out = apply_ban_signal(
            BanEnforcement::Enforce,
            &backend,
            home,
            BanReason::Unauthorized,
        )
        .unwrap();
        assert_eq!(out, BanSignalOutcome::SkippedWhitelisted);
        assert!(!backend.is_banned(home));

        // Ban evil works under enforce recording.
        let out = apply_ban_signal(
            BanEnforcement::Enforce,
            &backend,
            evil,
            BanReason::Unauthorized,
        )
        .unwrap();
        assert_eq!(
            out,
            BanSignalOutcome::Recorded {
                reason: BanReason::Unauthorized
            }
        );
        assert!(backend.is_banned(evil));

        let t0 = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert!(backend.touch_whitelist_last_used(home, t0));
        assert_eq!(backend.whitelist_last_used(home), Some(t0));
        assert!(!backend.touch_whitelist_last_used(evil, t0));
    }

    #[test]
    fn decide_access_order_whitelist_ban_rate() {
        let backend = MemoryBanBackend::new(parse_whitelist_cidrs("10.0.0.0/8").unwrap());
        let home = ip4(10, 1, 2, 3);
        let other = ip4(203, 0, 113, 9);

        assert_eq!(
            decide_access(
                home,
                &backend,
                RateLimitDecision::Limited {
                    retry_after: Duration::from_secs(5)
                }
            ),
            AccessDecision::Whitelisted
        );

        backend.ban(other, BanReason::RateLimitEscalation).unwrap();
        assert_eq!(
            decide_access(other, &backend, RateLimitDecision::Allowed { remaining: 3 }),
            AccessDecision::Banned
        );

        let clean = ip4(198, 51, 100, 50);
        assert_eq!(
            decide_access(
                clean,
                &backend,
                RateLimitDecision::Limited {
                    retry_after: Duration::from_secs(9)
                }
            ),
            AccessDecision::RateLimited {
                retry_after: Duration::from_secs(9)
            }
        );
        assert_eq!(
            decide_access(clean, &backend, RateLimitDecision::Allowed { remaining: 1 }),
            AccessDecision::Allowed
        );
    }

    #[test]
    fn enforcement_off_does_not_record_ban() {
        let backend = MemoryBanBackend::new(vec![]);
        let ip = ip4(203, 0, 113, 1);
        let out =
            apply_ban_signal(BanEnforcement::Off, &backend, ip, BanReason::Unauthorized).unwrap();
        assert_eq!(out, BanSignalOutcome::IgnoredOff);
        assert!(!backend.is_banned(ip));
        assert!(!should_reject_banned(
            BanEnforcement::Off,
            &AccessDecision::Banned
        ));
        assert!(should_reject_banned(
            BanEnforcement::Enforce,
            &AccessDecision::Banned
        ));
        assert!(!should_reject_banned(
            BanEnforcement::DryRun,
            &AccessDecision::Banned
        ));
    }

    #[test]
    fn nft_backend_records_add_element_without_root() {
        let memory = MemoryBanBackend::new(vec![]);
        let apply = NftExecApply::new(RecordingNftExec::default());
        let backend = NftBanBackend::new(memory, apply, true);
        let ip = ip4(203, 0, 113, 77);
        assert!(backend.ban(ip, BanReason::Unauthorized).unwrap());
        let calls = backend.apply().exec().calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let args = &calls[0];
        assert_eq!(args[0], "add");
        assert_eq!(args[1], "element");
        assert_eq!(args[2], "inet");
        assert_eq!(args[3], NFT_TABLE);
        assert_eq!(args[4], NFT_SET_BAN4);
        assert!(args.iter().any(|a| a == "203.0.113.77"));

        // IPv6 set name.
        let memory6 = MemoryBanBackend::new(vec![]);
        let apply6 = NftExecApply::new(RecordingNftExec::default());
        let backend6 = NftBanBackend::new(memory6, apply6, true);
        let ip6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
        assert!(backend6.ban(ip6, BanReason::Unauthorized).unwrap());
        let c6 = backend6.apply().exec().calls.lock().unwrap();
        assert_eq!(c6[0][4], NFT_SET_BAN6);
    }

    #[test]
    fn nft_remove_ban_args_mirror_add_shape() {
        let ip = ip4(198, 51, 100, 20);
        let add = nft_add_ban_args(ip);
        let rem = nft_remove_ban_args(ip);
        assert_eq!(add[0], "add");
        assert_eq!(rem[0], "delete");
        assert_eq!(&add[1..], &rem[1..]);
        let ip6 = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 9));
        assert_eq!(nft_remove_ban_args(ip6)[4], NFT_SET_BAN6);
    }

    #[test]
    fn nft_element_already_absent_detects_common_messages() {
        assert!(nft_element_already_absent(
            "Error: Could not process rule: No such file or directory"
        ));
        assert!(nft_element_already_absent("No such element in set"));
        assert!(nft_element_already_absent("element does not exist"));
        assert!(nft_element_already_absent("element not found in set"));
        assert!(!nft_element_already_absent("permission denied"));
        assert!(!nft_element_already_absent("File exists"));
        // Infrastructure miss must stay fail-closed (mirror add-side table miss).
        assert!(!nft_element_already_absent("table does not exist"));
        assert!(!nft_element_already_absent("set does not exist"));
        assert!(!nft_element_already_absent(
            "Error: No such file or directory; did you mean table ‘filter’ in family inet?"
        ));
        // Bare phrases without element/process-rule context are not success.
        assert!(!nft_element_already_absent("not found"));
        assert!(!nft_element_already_absent("does not exist"));
    }

    #[test]
    fn nft_whitelist_skips_nft_call() {
        let memory = MemoryBanBackend::new(parse_whitelist_cidrs("203.0.113.0/24").unwrap());
        let apply = NftExecApply::new(RecordingNftExec::default());
        let backend = NftBanBackend::new(memory, apply, true);
        let ip = ip4(203, 0, 113, 50);
        assert!(!backend.ban(ip, BanReason::Unauthorized).unwrap());
        assert!(backend.apply().exec().calls.lock().unwrap().is_empty());
    }

    #[test]
    fn file_state_persists_bans_and_last_used() {
        let dir = std::env::temp_dir().join(format!(
            "surmount-ban-test-{}-{}",
            std::process::id(),
            system_time_to_unix(SystemTime::now())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ban-state.json");
        let wl = parse_whitelist_cidrs("192.0.2.10").unwrap();
        let b1 = MemoryBanBackend::with_state_path(wl.clone(), &path).unwrap();
        let evil = ip4(198, 51, 100, 9);
        b1.ban(evil, BanReason::Unauthorized).unwrap();
        let t0 = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        b1.touch_whitelist_last_used(ip4(192, 0, 2, 10), t0);
        drop(b1);

        let b2 = MemoryBanBackend::with_state_path(wl, &path).unwrap();
        assert!(b2.is_banned(evil));
        assert_eq!(b2.whitelist_last_used(ip4(192, 0, 2, 10)), Some(t0));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_state_file_fail_closed() {
        let dir = std::env::temp_dir().join(format!(
            "surmount-ban-bad-{}-{}",
            std::process::id(),
            system_time_to_unix(SystemTime::now())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ban-state.json");
        std::fs::write(&path, "not-json{{{").unwrap();
        let err = match MemoryBanBackend::with_state_path(vec![], &path) {
            Ok(_) => panic!("expected corrupt state to fail closed"),
            Err(e) => e,
        };
        assert!(
            err.contains("corrupt") || err.contains("fail-closed"),
            "{err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ban_config_enforce_nft_exec_without_bin_fail_closed() {
        let err = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("enforce".into()),
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_EXEC" => Some("1".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("NFT_BIN"),
            "{err}"
        );
    }

    #[test]
    fn ban_config_default_off_memory() {
        let cfg = ban_config_from_env(|_| None).unwrap();
        assert_eq!(cfg.enforcement, BanEnforcement::Off);
        assert_eq!(cfg.backend_kind, BanBackendKind::Memory);
        assert!(cfg.whitelist.is_empty());
        assert!(!cfg.nft_exec_enabled);
        assert!(cfg.nft_helper_bin.is_none());
    }

    #[test]
    fn ban_config_invalid_enforcement_fail_closed() {
        let err = ban_config_from_env(|k| {
            if k == "SURMOUNT_BAN_ENFORCEMENT" {
                Some("maybe".into())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err.contains("SURMOUNT_BAN_ENFORCEMENT"), "{err}");
    }

    #[test]
    fn ban_config_relative_state_path_fail_closed() {
        let err = ban_config_from_env(|k| {
            if k == "SURMOUNT_BAN_STATE_PATH" {
                Some("relative/ban.json".into())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn cidr_v4_and_v6_match() {
        let e = WhitelistEntry::parse("198.51.100.0/24").unwrap();
        assert!(e.contains(ip4(198, 51, 100, 200)));
        assert!(!e.contains(ip4(198, 51, 101, 1)));
        let e6 = WhitelistEntry::parse("2001:db8::/32").unwrap();
        assert!(e6.contains(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 99))));
        assert!(!e6.contains(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb9, 0, 0, 0, 0, 0, 1))));
    }

    #[test]
    fn nft_set_name_constants_match_product_docs() {
        // Contract: Nix module fragment must use these exact set names.
        assert_eq!(NFT_SET_BAN4, "surmount-ban4");
        assert_eq!(NFT_SET_BAN6, "surmount-ban6");
        assert_eq!(NFT_SET_WHITELIST4, "surmount-whitelist4");
        assert_eq!(NFT_SET_WHITELIST6, "surmount-whitelist6");
        assert_eq!(NFT_TABLE, "surmount_guard");
        let names: Vec<&str> = nft_product_set_names().iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"surmount-ban4"));
        assert!(names.contains(&"surmount-whitelist6"));
    }

    #[test]
    fn dry_run_records_ban_but_should_not_reject() {
        let backend = MemoryBanBackend::new(vec![]);
        let ip = ip4(203, 0, 113, 2);
        apply_ban_signal(
            BanEnforcement::DryRun,
            &backend,
            ip,
            BanReason::Unauthorized,
        )
        .unwrap();
        assert!(backend.is_banned(ip));
        assert!(!should_reject_banned(
            BanEnforcement::DryRun,
            &AccessDecision::Banned
        ));
    }

    #[test]
    fn ban_config_invalid_nft_exec_fail_closed() {
        // Contract: unknown SURMOUNT_BAN_NFT_EXEC is not silent false.
        let err = ban_config_from_env(|k| {
            if k == "SURMOUNT_BAN_NFT_EXEC" {
                Some("maybe".into())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(
            err.contains("SURMOUNT_BAN_NFT_EXEC") && err.contains("fail-closed"),
            "{err}"
        );

        let err2 = ban_config_from_env(|k| {
            if k == "SURMOUNT_BAN_NFT_EXEC" {
                Some("dry-run".into())
            } else {
                None
            }
        })
        .unwrap_err();
        assert!(err2.contains("SURMOUNT_BAN_NFT_EXEC"), "{err2}");

        // Explicit false still ok.
        let cfg = ban_config_from_env(|k| {
            if k == "SURMOUNT_BAN_NFT_EXEC" {
                Some("off".into())
            } else {
                None
            }
        })
        .unwrap();
        assert!(!cfg.nft_exec_enabled);
    }

    #[test]
    fn nft_ban_does_not_write_memory_when_nft_exec_fails() {
        struct FailingNftExec;
        impl NftExec for FailingNftExec {
            fn run_nft(&self, _args: &[String]) -> Result<(), String> {
                Err("simulated nft failure".into())
            }
        }

        let memory = MemoryBanBackend::new(vec![]);
        let apply = NftExecApply::new(FailingNftExec);
        let backend = NftBanBackend::new(memory, apply, true);
        let ip = ip4(203, 0, 113, 88);
        let err = backend.ban(ip, BanReason::Unauthorized).unwrap_err();
        assert!(err.contains("nft") || err.contains("not written"), "{err}");
        assert!(
            !backend.is_banned(ip),
            "memory must not stay banned after nft failure"
        );
    }

    /// Crash-window recovery: nft "File exists" / already-present is apply ok.
    #[test]
    fn nft_ban_already_present_element_writes_memory() {
        struct AlreadyPresentNftExec;
        impl NftExec for AlreadyPresentNftExec {
            fn run_nft(&self, _args: &[String]) -> Result<(), String> {
                Err(
                    "Error: Could not process rule: File exists\nnft exited with exit status: 1"
                        .into(),
                )
            }
        }

        let memory = MemoryBanBackend::new(vec![]);
        let apply = NftExecApply::new(AlreadyPresentNftExec);
        let backend = NftBanBackend::new(memory, apply, true);
        let ip = ip4(203, 0, 113, 77);
        assert!(backend.ban(ip, BanReason::Unauthorized).unwrap());
        assert!(
            backend.is_banned(ip),
            "durable app ban must stick when host element already present"
        );
    }

    #[test]
    fn nft_element_already_present_detects_common_messages() {
        assert!(nft_element_already_present(
            "Error: Could not process rule: File exists"
        ));
        assert!(nft_element_already_present(
            "EEXIST: element already in set"
        ));
        assert!(nft_element_already_present("set element already exists"));
        assert!(!nft_element_already_present("permission denied"));
        assert!(!nft_element_already_present("table does not exist"));
    }

    #[test]
    fn ban_config_helper_with_memory_backend_fail_closed() {
        let err = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("enforce".into()),
            "SURMOUNT_BAN_BACKEND" => Some("memory".into()),
            "SURMOUNT_BAN_NFT_HELPER_SOCK" => Some("/run/surmount/nft-ban-helper.sock".into()),
            "SURMOUNT_BAN_NFT_BIN" => Some("/bin/sh".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("BACKEND=nft") || err.contains("memory"),
            "{err}"
        );
    }

    #[test]
    fn ban_config_dry_run_does_not_require_nft_bin() {
        // DryRun must not mutate nft; missing bin is fine (no fail-closed).
        let cfg = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("dry-run".into()),
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_EXEC" => Some("1".into()),
            "SURMOUNT_BAN_NFT_BIN" => Some("/nonexistent/surmount-nft-bin-missing".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(cfg.enforcement, BanEnforcement::DryRun);
        assert!(cfg.nft_exec_enabled);
        let guard = BanGuard::from_config(cfg).unwrap();
        let ip = ip4(203, 0, 113, 3);
        guard.signal_ban(ip, BanReason::Unauthorized).unwrap();
        assert!(guard.is_banned(ip));
        assert!(!should_reject_banned(
            BanEnforcement::DryRun,
            &AccessDecision::Banned
        ));
    }

    #[test]
    fn ban_config_helper_and_exec_mutually_exclusive() {
        let err = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_EXEC" => Some("1".into()),
            "SURMOUNT_BAN_NFT_BIN" => Some("/run/current-system/sw/bin/nft".into()),
            "SURMOUNT_BAN_NFT_HELPER_SOCK" => Some("/run/surmount/nft-ban-helper.sock".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("mutually exclusive") || err.contains("fail-closed"),
            "{err}"
        );
    }

    #[test]
    fn ban_config_enforce_helper_process_missing_fail_closed() {
        let err = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("enforce".into()),
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_HELPER" => {
                Some("/nonexistent/surmount-nft-ban-helper-missing".into())
            }
            "SURMOUNT_BAN_NFT_BIN" => Some("/bin/sh".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("fail-closed") || err.contains("existing") || err.contains("HELPER"),
            "{err}"
        );
    }

    #[test]
    fn ban_config_enforce_helper_without_nft_bin_fail_closed() {
        let err = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("enforce".into()),
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_HELPER_SOCK" => Some("/run/surmount/nft-ban-helper.sock".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(
            err.contains("NFT_BIN") || err.contains("fail-closed"),
            "{err}"
        );
    }

    #[test]
    fn ban_config_enforce_helper_sock_ok_without_sock_file() {
        // Sock inode may appear when socket unit starts; nft bin must exist.
        let cfg = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("enforce".into()),
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_HELPER_SOCK" => {
                Some("/run/surmount/nft-ban-helper-missing-ok.sock".into())
            }
            "SURMOUNT_BAN_NFT_BIN" => Some("/bin/sh".into()),
            _ => None,
        })
        .unwrap();
        assert!(cfg.nft_helper_sock.is_some());
        assert!(cfg.nft_bin.is_some());
    }

    #[test]
    fn ban_config_dry_run_helper_missing_sock_ok() {
        let cfg = ban_config_from_env(|k| match k {
            "SURMOUNT_BAN_ENFORCEMENT" => Some("dry-run".into()),
            "SURMOUNT_BAN_BACKEND" => Some("nft".into()),
            "SURMOUNT_BAN_NFT_HELPER_SOCK" => Some("/run/surmount/nft-ban-helper.sock".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(cfg.enforcement, BanEnforcement::DryRun);
    }

    #[test]
    fn ban_guard_dry_run_does_not_spawn_helper_side_effect() {
        let dir = std::env::temp_dir().join(format!(
            "surmount-ban-dry-se-{}-{}",
            std::process::id(),
            system_time_to_unix(SystemTime::now())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let side = dir.join("spawned");
        let helper = dir.join("helper-touch");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let script = format!(
                "#!/bin/sh\ntouch '{}'\necho '{{\"ok\":true}}'\n",
                side.display()
            );
            std::fs::write(&helper, script).unwrap();
            let mut perms = std::fs::metadata(&helper).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&helper, perms).unwrap();
        }
        #[cfg(not(unix))]
        {
            let _ = (&side, &helper);
            return;
        }

        let cfg = BanConfig {
            enforcement: BanEnforcement::DryRun,
            backend_kind: BanBackendKind::Nft,
            whitelist: vec![],
            state_path: None,
            nft_exec_enabled: false,
            nft_bin: Some(PathBuf::from("/bin/sh")),
            nft_helper_sock: None,
            nft_helper_bin: Some(helper.clone()),
        };
        let guard = BanGuard::from_config(cfg).unwrap();
        let ip = ip4(203, 0, 113, 66);
        guard.signal_ban(ip, BanReason::Unauthorized).unwrap();
        assert!(guard.is_banned(ip));
        assert!(
            !side.exists(),
            "DryRun must not spawn helper (side-effect file present)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dry_run_nft_sync_flag_false_even_with_helper_path() {
        struct CountingApply(std::sync::atomic::AtomicUsize);
        impl BanNftApply for CountingApply {
            fn add_ban_element(&self, _ip: IpAddr) -> Result<(), String> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }
        let memory = MemoryBanBackend::new(vec![]);
        let apply = CountingApply(std::sync::atomic::AtomicUsize::new(0));
        let backend = NftBanBackend::new(memory, apply, false);
        let ip = ip4(203, 0, 113, 4);
        assert!(backend.ban(ip, BanReason::Unauthorized).unwrap());
        assert_eq!(
            backend.apply().0.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        assert!(backend.is_banned(ip));
    }

    /// Named contract: unauthorized signal yields BanCandidate (not ordinary GET);
    /// whitelist never becomes a candidate or ban; Off does not record; DryRun/Enforce do.
    #[test]
    fn unauthorized_signal_ban_candidate_whitelist_and_enforcement() {
        let wl = parse_whitelist_cidrs("203.0.113.10/32").unwrap();
        let backend = MemoryBanBackend::new(wl);
        let home = ip4(203, 0, 113, 10);
        let evil = ip4(198, 51, 100, 1);

        // Pure decide: whitelist wins; clean IP is BanCandidate.
        assert_eq!(
            decide_ban_signal(&backend, home, BanReason::Unauthorized),
            AccessDecision::Whitelisted
        );
        assert_eq!(
            decide_ban_signal(&backend, evil, BanReason::Unauthorized),
            AccessDecision::BanCandidate {
                reason: BanReason::Unauthorized
            }
        );

        // Off: candidate view, no durable ban.
        let off =
            BanGuard::with_backend(BanEnforcement::Off, Box::new(MemoryBanBackend::new(vec![])));
        let (dec, out) = off.signal_unauthorized(evil).unwrap();
        assert_eq!(
            dec,
            AccessDecision::BanCandidate {
                reason: BanReason::Unauthorized
            }
        );
        assert_eq!(out, BanSignalOutcome::IgnoredOff);
        assert!(!off.is_banned(evil));

        // DryRun: candidate + record, no 403 contract.
        let dry = BanGuard::with_backend(
            BanEnforcement::DryRun,
            Box::new(MemoryBanBackend::new(vec![])),
        );
        let (dec, out) = dry.signal_unauthorized(evil).unwrap();
        assert_eq!(
            dec,
            AccessDecision::BanCandidate {
                reason: BanReason::Unauthorized
            }
        );
        assert_eq!(
            out,
            BanSignalOutcome::Recorded {
                reason: BanReason::Unauthorized
            }
        );
        assert!(dry.is_banned(evil));
        assert!(!should_reject_banned(
            BanEnforcement::DryRun,
            &AccessDecision::Banned
        ));

        // Enforce: candidate + record; later decide is Banned + reject.
        let mem = MemoryBanBackend::new(vec![]);
        let enf = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(mem));
        let (dec, out) = enf.signal_unauthorized(evil).unwrap();
        assert_eq!(
            dec,
            AccessDecision::BanCandidate {
                reason: BanReason::Unauthorized
            }
        );
        assert_eq!(
            out,
            BanSignalOutcome::Recorded {
                reason: BanReason::Unauthorized
            }
        );
        assert!(enf.is_banned(evil));
        assert_eq!(
            enf.decide(evil, RateLimitDecision::Allowed { remaining: 1 }),
            AccessDecision::Banned
        );
        assert!(should_reject_banned(
            BanEnforcement::Enforce,
            &AccessDecision::Banned
        ));

        // Whitelist immunity via hook.
        let wl_backend = MemoryBanBackend::new(parse_whitelist_cidrs("203.0.113.10/32").unwrap());
        let wl_guard = BanGuard::with_backend(BanEnforcement::Enforce, Box::new(wl_backend));
        let (dec, out) = wl_guard.signal_unauthorized(home).unwrap();
        assert_eq!(dec, AccessDecision::Whitelisted);
        assert_eq!(out, BanSignalOutcome::SkippedWhitelisted);
        assert!(!wl_guard.is_banned(home));

        // Already banned: decision Banned, not a fresh BanCandidate.
        let (dec2, out2) = enf.signal_unauthorized(evil).unwrap();
        assert_eq!(dec2, AccessDecision::Banned);
        assert_eq!(out2, BanSignalOutcome::AlreadyBanned);
    }
}
