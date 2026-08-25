//! Nostr auth foundation: NIP-98 verify, npub allowlist, HMAC session cookies.
//!
//! Uses **rust-nostr** (`nostr` crate with `nip98`) — not JS NDK / NPM.
//! Server never holds nsec; only verifies client-provided kind 27235 events.
//!
//! Scaffold defaults (Q-AUTH-1 still open for product answers):
//! - Session: signed HTTP-only cookie (HMAC-SHA256 over payload; secret from env).
//!   Durable server session store remains open.
//! - Bootstrap allowlist: env `SURMOUNT_NOSTR_ALLOWLIST` and optional file
//!   `SURMOUNT_NOSTR_ALLOWLIST_FILE` (same parse rules; env wins when non-empty).
//!   First-operator bootstrap product UX remains open.
//! - Empty allowlist + auth enabled = fail-closed (nobody authenticates).
//! - Key-loss recovery: not invented here.

use std::collections::HashSet;
use std::fmt;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::{STANDARD as B64_STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use nostr::nips::nip19::ToBech32;
pub use nostr::nips::nip98::{HttpData, HttpMethod};
use nostr::prelude::{Event, EventBuilder, JsonUtil, Keys, Kind, PublicKey, TagKind, TagStandard};
use nostr::{Timestamp, Url as NostrUrl};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Cookie name for the scaffold signed session.
pub const SESSION_COOKIE_NAME: &str = "surmount_session";

/// Double-submit CSRF cookie (**HttpOnly**, Path=/, SameSite=Lax).
///
/// The mail page (and session JSON) embeds the token so the client can send
/// `X-CSRF-Token` without reading `document.cookie`. HttpOnly matches
/// `surmount_session` so browsers that drop JS-readable cookies still keep
/// this one. Paired with [`CSRF_HEADER_NAME`].
pub const CSRF_COOKIE_NAME: &str = "surmount_csrf";

/// Request header clients must send with cookie-authenticated mutations (logout, …).
pub const CSRF_HEADER_NAME: &str = "x-csrf-token";

/// NIP-98 HTTP auth event kind (27235).
pub const NIP98_KIND: u16 = 27235;

type HmacSha256 = Hmac<Sha256>;

/// Product auth mode from `SURMOUNT_AUTH_MODE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthMode {
    /// Open console (dev/local default). Not public-safe alone.
    #[default]
    Off,
    /// Gate admin HTML + JSON APIs behind session or valid NIP-98.
    Nostr,
}

impl AuthMode {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "none" | "disabled" | "0" | "false" => Ok(Self::Off),
            "nostr" | "on" | "1" | "true" => Ok(Self::Nostr),
            other => Err(format!(
                "SURMOUNT_AUTH_MODE={other:?} invalid (expected off|nostr)"
            )),
        }
    }

    pub fn is_nostr(self) -> bool {
        matches!(self, Self::Nostr)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Nostr => "nostr",
        }
    }
}

/// Auth-related config (scaffold).
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub mode: AuthMode,
    /// Lowercase hex pubkeys (32-byte x-only, 64 hex chars).
    pub allowlist: HashSet<String>,
    /// HMAC key bytes; required non-empty when `mode == Nostr`.
    pub session_secret: Option<Vec<u8>>,
    pub session_ttl_secs: u64,
    /// Optional absolute base URL for NIP-98 `u` matching (behind reverse proxy).
    pub public_base_url: Option<String>,
    /// Max |now - created_at| for NIP-98 events (seconds). Default 300.
    pub nip98_max_skew_secs: u64,
}

impl AuthConfig {
    pub fn off() -> Self {
        Self {
            mode: AuthMode::Off,
            allowlist: HashSet::new(),
            session_secret: None,
            session_ttl_secs: 86_400,
            public_base_url: None,
            nip98_max_skew_secs: 300,
        }
    }

    /// Fail closed when mode is Nostr without a session secret.
    pub fn validate(&self) -> Result<(), String> {
        if self.mode.is_nostr() {
            match &self.session_secret {
                Some(s) if !s.is_empty() => {}
                _ => {
                    return Err(
                        "SURMOUNT_AUTH_MODE=nostr requires non-empty SURMOUNT_SESSION_SECRET \
                         (fail-closed; host-only secret, never in git)"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }

    /// Start-time auth validation: secret when nostr, plus public-edge footgun guard.
    ///
    /// Call with the primary `SURMOUNT_LISTEN` address. Lab escape
    /// `allow_public_auth_off` mirrors `SURMOUNT_ALLOW_PUBLIC_AUTH_OFF` (never
    /// production default).
    pub fn validate_for_listen(
        &self,
        listen: SocketAddr,
        allow_public_auth_off: bool,
    ) -> Result<(), String> {
        self.validate()?;
        require_auth_when_public_edge(self.mode, listen, allow_public_auth_off)
    }
}

/// True when the primary listen address is a public-facing edge bind.
///
/// - Unspecified (`0.0.0.0` / `::`): world bind => public.
/// - Loopback, RFC1918 / unique-local private, link-local: lab-safe for auth-off.
/// - Other addresses (global unicast, including TEST-NET docs ranges): public.
pub fn listen_is_public_primary_edge(addr: SocketAddr) -> bool {
    use std::net::IpAddr;
    match addr.ip() {
        IpAddr::V4(v4) => {
            if v4.is_unspecified() {
                return true;
            }
            // Loopback, RFC1918 private, link-local: not a public edge for this guard.
            !(v4.is_loopback() || v4.is_private() || v4.is_link_local())
        }
        IpAddr::V6(v6) => {
            if v6.is_unspecified() {
                return true;
            }
            // Loopback, unique local (fc00::/7), link-local: lab-safe for auth-off.
            !(v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local())
        }
    }
}

/// Refuse `AuthMode::Off` on a public primary listen (open-console footgun).
///
/// Loopback / private binds may keep auth-off for local `just dev` and CI.
/// Public bind requires `AuthMode::Nostr` (or lab-only `allow_public_auth_off`).
pub fn require_auth_when_public_edge(
    mode: AuthMode,
    listen: SocketAddr,
    allow_public_auth_off: bool,
) -> Result<(), String> {
    if mode != AuthMode::Off {
        return Ok(());
    }
    if allow_public_auth_off {
        return Ok(());
    }
    if !listen_is_public_primary_edge(listen) {
        return Ok(());
    }
    Err(format!(
        "SURMOUNT_AUTH_MODE=off is not allowed on public primary listen {listen} \
         (open console is not public-safe). Set SURMOUNT_AUTH_MODE=nostr with \
         SURMOUNT_SESSION_SECRET + allowlist, bind loopback/private only, or \
         lab-only SURMOUNT_ALLOW_PUBLIC_AUTH_OFF=1 (fail-closed)"
    ))
}

/// Parse `SURMOUNT_NOSTR_ALLOWLIST`: comma/space/newline separated bech32 npub or hex.
/// Empty input => empty set (fail-closed when auth is on).
pub fn parse_allowlist(raw: &str) -> Result<HashSet<String>, String> {
    let mut out = HashSet::new();
    for token in raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|t| !t.is_empty())
    {
        out.insert(normalize_pubkey_token(token)?);
    }
    Ok(out)
}

/// Load allowlist from a host file (same token rules as [`parse_allowlist`]).
///
/// File contents are comma/space/newline separated npub or hex. Unreadable or
/// invalid tokens are config errors (fail-closed). Empty file => empty set.
pub fn parse_allowlist_file(path: &Path) -> Result<HashSet<String>, String> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "SURMOUNT_NOSTR_ALLOWLIST_FILE={} unreadable: {e} (fail-closed)",
            path.display()
        )
    })?;
    parse_allowlist(&contents).map_err(|e| {
        format!(
            "SURMOUNT_NOSTR_ALLOWLIST_FILE={} parse error: {e}",
            path.display()
        )
    })
}

/// Resolve allowlist: non-empty env string wins; else optional file path; else empty.
///
/// - `env_raw` non-empty after token parse => use env set (env wins).
/// - else if `file_path` is `Some` non-empty path => load file (fail-closed on IO/parse).
/// - else empty HashSet (fail-closed when mode=nostr).
pub fn resolve_allowlist(
    env_raw: &str,
    file_path: Option<&str>,
) -> Result<HashSet<String>, String> {
    let from_env = parse_allowlist(env_raw)?;
    if !from_env.is_empty() {
        return Ok(from_env);
    }
    match file_path.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => parse_allowlist_file(Path::new(p)),
        None => Ok(HashSet::new()),
    }
}

/// Stable log label for [`AuthError`] (no secrets; safe for structured tracing).
pub fn auth_error_kind(err: &AuthError) -> &'static str {
    match err {
        AuthError::Malformed => "malformed",
        AuthError::WrongKind => "wrong_kind",
        AuthError::BadSignature => "bad_signature",
        AuthError::Skew => "skew",
        AuthError::UrlMismatch => "url_mismatch",
        AuthError::MethodMismatch => "method_mismatch",
        AuthError::NotAllowlisted => "not_allowlisted",
        AuthError::MissingTag(_) => "missing_tag",
        AuthError::SessionInvalid => "session_invalid",
        AuthError::SessionExpired => "session_expired",
        AuthError::AuthOff => "auth_off",
        AuthError::Config => "config",
        AuthError::Csrf => "csrf",
    }
}

/// Normalize one npub (bech32) or hex pubkey to lowercase hex.
///
/// Never echo the raw token. `nsec1` is refused with a fixed phrase.
pub fn normalize_pubkey_token(token: &str) -> Result<String, String> {
    let t = token.trim();
    if t.is_empty() {
        return Err("invalid npub".into());
    }
    if t.to_ascii_lowercase().starts_with("nsec1") {
        return Err("nsec is not allowed".into());
    }
    // PublicKey::parse accepts hex and bech32 npub. Do not include the token
    // or parser text in the error (create/grant return this string in JSON).
    let pk = PublicKey::parse(t).map_err(|_| "invalid npub".to_string())?;
    Ok(pk.to_hex().to_ascii_lowercase())
}

pub fn allowlist_contains(allowlist: &HashSet<String>, hex_pubkey: &str) -> bool {
    allowlist.contains(&hex_pubkey.to_ascii_lowercase())
}

/// Why NIP-98 / session verification failed (for tests and logs; no secret data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    Malformed,
    WrongKind,
    BadSignature,
    Skew,
    UrlMismatch,
    MethodMismatch,
    NotAllowlisted,
    MissingTag(&'static str),
    SessionInvalid,
    SessionExpired,
    AuthOff,
    Config,
    /// Missing/mismatched double-submit CSRF token on a state-changing route.
    Csrf,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => write!(f, "malformed auth material"),
            Self::WrongKind => write!(f, "wrong event kind (expected {NIP98_KIND})"),
            Self::BadSignature => write!(f, "bad event signature"),
            Self::Skew => write!(f, "event timestamp outside allowed skew"),
            Self::UrlMismatch => write!(f, "NIP-98 u tag does not match request URL"),
            Self::MethodMismatch => write!(f, "NIP-98 method tag does not match request method"),
            Self::NotAllowlisted => write!(f, "pubkey not in allowlist"),
            Self::MissingTag(t) => write!(f, "missing tag {t}"),
            Self::SessionInvalid => write!(f, "invalid session cookie"),
            Self::SessionExpired => write!(f, "session expired"),
            Self::AuthOff => write!(f, "auth mode is off"),
            Self::Config => write!(f, "auth config error"),
            Self::Csrf => write!(f, "CSRF token missing or mismatch"),
        }
    }
}

/// Result of a successful NIP-98 verify (hex pubkey).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    pub hex_pubkey: String,
}

impl VerifiedIdentity {
    pub fn npub(&self) -> Result<String, String> {
        let pk = PublicKey::parse(&self.hex_pubkey).map_err(|e| e.to_string())?;
        pk.to_bech32().map_err(|e| e.to_string())
    }
}

/// Parse event from `Authorization: Nostr <base64>` or raw base64 / JSON body.
pub fn parse_event_from_auth_header(header: &str) -> Result<Event, AuthError> {
    let header = header.trim();
    let b64 = if let Some(rest) = header.strip_prefix("Nostr ") {
        rest.trim()
    } else if let Some(rest) = header.strip_prefix("nostr ") {
        rest.trim()
    } else {
        return Err(AuthError::Malformed);
    };
    if b64.is_empty() {
        return Err(AuthError::Malformed);
    }
    decode_event_b64_or_json(b64)
}

/// Decode base64 (standard or URL-safe) of event JSON, or bare event JSON.
pub fn decode_event_b64_or_json(input: &str) -> Result<Event, AuthError> {
    let input = input.trim();
    if input.starts_with('{') {
        return Event::from_json(input).map_err(|_| AuthError::Malformed);
    }
    // Try standard base64 then URL-safe.
    let bytes = B64_STANDARD
        .decode(input)
        .or_else(|_| URL_SAFE_NO_PAD.decode(input))
        .or_else(|_| {
            // Some clients pad URL-safe; try with STANDARD_NO_PAD / URL_SAFE with pad.
            base64::engine::general_purpose::URL_SAFE
                .decode(input)
                .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(input))
        })
        .map_err(|_| AuthError::Malformed)?;
    Event::from_json(bytes).map_err(|_| AuthError::Malformed)
}

/// Verify NIP-98 event (kind, signature, skew, u, method). Does not check allowlist.
///
/// Callers then accept the key if it is on the host allowlist **or** the
/// Surmount console account map.
pub fn verify_nip98_event_unlisted(
    event: &Event,
    expected_url: &str,
    expected_method: &str,
    now_unix: u64,
    max_skew_secs: u64,
) -> Result<VerifiedIdentity, AuthError> {
    if event.kind != Kind::HttpAuth && u16::from(event.kind) != NIP98_KIND {
        return Err(AuthError::WrongKind);
    }

    // Cryptographic verify (id + schnorr sig).
    event.verify().map_err(|_| AuthError::BadSignature)?;

    let created = event.created_at.as_u64();
    if now_unix.abs_diff(created) > max_skew_secs {
        return Err(AuthError::Skew);
    }

    let authorized_url = event
        .tags
        .find_standardized(TagKind::u())
        .and_then(|tag| match tag {
            TagStandard::AbsoluteURL(u) => Some(u.to_string()),
            _ => None,
        })
        .ok_or(AuthError::MissingTag("u"))?;

    if !urls_match_for_nip98(&authorized_url, expected_url) {
        return Err(AuthError::UrlMismatch);
    }

    let authorized_method = event
        .tags
        .find_standardized(TagKind::Method)
        .and_then(|tag| match tag {
            TagStandard::Method(m) => Some(m.to_string()),
            _ => None,
        })
        .ok_or(AuthError::MissingTag("method"))?;

    if !authorized_method.eq_ignore_ascii_case(expected_method) {
        return Err(AuthError::MethodMismatch);
    }

    let hex_pubkey = event.pubkey.to_hex().to_ascii_lowercase();
    Ok(VerifiedIdentity { hex_pubkey })
}

/// Verify NIP-98 event against expected absolute URL, HTTP method, skew, and allowlist.
///
/// Skew policy (documented): `|now - created_at| <= max_skew_secs` (default 300).
/// Symmetric window (past and future) for clock skew. Tests cover both sides.
pub fn verify_nip98_event(
    event: &Event,
    expected_url: &str,
    expected_method: &str,
    now_unix: u64,
    max_skew_secs: u64,
    allowlist: &HashSet<String>,
) -> Result<VerifiedIdentity, AuthError> {
    let identity = verify_nip98_event_unlisted(
        event,
        expected_url,
        expected_method,
        now_unix,
        max_skew_secs,
    )?;
    if !allowlist_contains(allowlist, &identity.hex_pubkey) {
        return Err(AuthError::NotAllowlisted);
    }
    Ok(identity)
}

/// Loose URL compare: normalize trailing slash on path-only roots.
fn urls_match_for_nip98(authorized: &str, expected: &str) -> bool {
    fn norm(s: &str) -> String {
        let t = s.trim();
        // Strip trailing slash except for scheme://host only handled by Url if available.
        if t.len() > 1 && t.ends_with('/') {
            t.trim_end_matches('/').to_string()
        } else {
            t.to_string()
        }
    }
    // Prefer Url equality when both parse.
    match (
        NostrUrl::parse(authorized.trim()),
        NostrUrl::parse(expected.trim()),
    ) {
        (Ok(a), Ok(e)) => a == e || norm(authorized) == norm(expected),
        _ => norm(authorized) == norm(expected),
    }
}

// --- Session cookie (HMAC scaffold) ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionPayload {
    /// Lowercase hex pubkey.
    pub sub: String,
    /// Expiry unix seconds.
    pub exp: u64,
}

/// Encode signed session cookie value: `base64url(payload_json).base64url(hmac)`.
pub fn encode_session_cookie(payload: &SessionPayload, secret: &[u8]) -> Result<String, AuthError> {
    if secret.is_empty() {
        return Err(AuthError::Config);
    }
    let json = serde_json::to_vec(payload).map_err(|_| AuthError::Malformed)?;
    let body = URL_SAFE_NO_PAD.encode(&json);
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::Config)?;
    mac.update(body.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    Ok(format!("{body}.{sig}"))
}

/// Verify signed session cookie; enforce exp against `now_unix`.
pub fn decode_session_cookie(
    cookie_value: &str,
    secret: &[u8],
    now_unix: u64,
) -> Result<SessionPayload, AuthError> {
    if secret.is_empty() {
        return Err(AuthError::Config);
    }
    let (body, sig_b64) = cookie_value
        .split_once('.')
        .ok_or(AuthError::SessionInvalid)?;
    if body.is_empty() || sig_b64.is_empty() {
        return Err(AuthError::SessionInvalid);
    }
    let got = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| AuthError::SessionInvalid)?;
    // Constant-time MAC check (hmac::Mac::verify_slice).
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::Config)?;
    mac.update(body.as_bytes());
    mac.verify_slice(&got)
        .map_err(|_| AuthError::SessionInvalid)?;
    let json = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| AuthError::SessionInvalid)?;
    let payload: SessionPayload =
        serde_json::from_slice(&json).map_err(|_| AuthError::SessionInvalid)?;
    if payload.exp <= now_unix {
        return Err(AuthError::SessionExpired);
    }
    if normalize_pubkey_token(&payload.sub).is_err() {
        // Still accept hex we stored; reject garbage.
        if payload.sub.len() != 64 || !payload.sub.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AuthError::SessionInvalid);
        }
    }
    Ok(payload)
}

/// Build `Set-Cookie` value for session (HttpOnly; SameSite=Lax; Path=/).
///
/// `Secure` is set when `secure` is true (HTTPS listen). SameSite=Lax is always
/// present (mitigates many cross-site POSTs; CSRF double-submit is defense-in-depth).
pub fn session_set_cookie_header(cookie_value: &str, max_age_secs: u64, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "{SESSION_COOKIE_NAME}={cookie_value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure_flag}"
    )
}

/// Clear session cookie.
pub fn session_clear_cookie_header(secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure_flag}")
}

/// Cryptographically random token (32 bytes, base64url no pad) for CSRF or CSP nonce.
pub fn random_urlsafe_token() -> Result<String, AuthError> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| AuthError::Config)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Alias for CSP nonce generation (same entropy as CSRF tokens).
pub fn new_csp_nonce() -> Result<String, AuthError> {
    random_urlsafe_token()
}

/// Alias for new double-submit CSRF token.
pub fn new_csrf_token() -> Result<String, AuthError> {
    random_urlsafe_token()
}

/// Build `Set-Cookie` for CSRF double-submit token.
///
/// HttpOnly + Path=/ + SameSite=Lax + optional Secure (same jar policy as
/// the session cookie). The HTML form embeds the token for the header.
pub fn csrf_set_cookie_header(token: &str, max_age_secs: u64, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!(
        "{CSRF_COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}{secure_flag}"
    )
}

/// Clear CSRF cookie (must match Path / HttpOnly / SameSite / Secure).
pub fn csrf_clear_cookie_header(secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };
    format!("{CSRF_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure_flag}")
}

/// Constant-time equality for token strings (length must match).
pub fn constant_time_eq_str(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Double-submit CSRF: cookie value must match header (or optional body field).
///
/// Empty/missing either side fails closed. Tokens must be non-empty and equal
/// in constant time.
pub fn verify_csrf_double_submit(
    cookie_token: Option<&str>,
    submitted_token: Option<&str>,
) -> Result<(), AuthError> {
    let cookie = cookie_token.map(str::trim).filter(|s| !s.is_empty());
    let submitted = submitted_token.map(str::trim).filter(|s| !s.is_empty());
    match (cookie, submitted) {
        (Some(c), Some(s)) if constant_time_eq_str(c, s) => Ok(()),
        _ => Err(AuthError::Csrf),
    }
}

/// Purpose bytes for session-bound CSRF HMAC (distinct from the session MAC).
/// Present in the running binary so deploy proof can grep this exact string.
pub const CSRF_SESSION_MAC_PURPOSE: &[u8] = b"surmount-csrf-session-v1";

/// CSRF synchronizer token bound to a session cookie via HMAC-SHA256.
///
/// Not the session MAC. The mail page embeds this value; POST compares the
/// submitted header/body to it. A second `surmount_csrf` cookie is optional.
pub fn session_bound_csrf_token(session_cookie: &str, secret: &[u8]) -> Result<String, AuthError> {
    if secret.is_empty() || session_cookie.is_empty() {
        return Err(AuthError::Config);
    }
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::Config)?;
    mac.update(CSRF_SESSION_MAC_PURPOSE);
    mac.update(&[0u8]);
    mac.update(session_cookie.as_bytes());
    Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

/// Session-bound CSRF: submitted token must match HMAC(session cookie).
///
/// Missing session, missing/empty submitted token, or mismatch is
/// [`AuthError::Csrf`]. Does not require a `surmount_csrf` cookie.
pub fn verify_csrf_session_bound(
    session_cookie: Option<&str>,
    secret: &[u8],
    submitted_token: Option<&str>,
) -> Result<(), AuthError> {
    let session = session_cookie.map(str::trim).filter(|s| !s.is_empty());
    let submitted = submitted_token.map(str::trim).filter(|s| !s.is_empty());
    match (session, submitted) {
        (Some(sess), Some(got)) => {
            let expected = session_bound_csrf_token(sess, secret)?;
            if constant_time_eq_str(&expected, got) {
                Ok(())
            } else {
                Err(AuthError::Csrf)
            }
        }
        _ => Err(AuthError::Csrf),
    }
}

/// Lean baseline CSP for SSR admin (no third-party hosts).
///
/// Inline login script is allowed only via `script-src 'nonce-…'`.
/// Inline DOGE CSS uses `style-src 'self' 'unsafe-inline'` (no third-party).
/// `frame-ancestors 'none'` pairs with `X-Frame-Options: DENY`.
pub fn baseline_csp(script_nonce: &str) -> String {
    // Nonce is base64url from our generator (alphanumeric + _ -); refuse quotes.
    let nonce = script_nonce
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>();
    format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         font-src 'self'; \
         object-src 'none'; \
         base-uri 'self'; \
         form-action 'self'; \
         frame-ancestors 'none'"
    )
}

/// Request-scoped CSP nonce carried through Axum extensions (middleware → HTML).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CspNonce(pub String);

impl CspNonce {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Extract named cookie from `Cookie` header string.
pub fn cookie_value<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(name)
            && let Some(v) = rest.strip_prefix('=')
        {
            return Some(v);
        }
    }
    None
}

/// Unix now (seconds).
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Build absolute request URL for NIP-98 `u` matching.
pub fn absolute_request_url(
    public_base_url: Option<&str>,
    scheme: &str,
    host: &str,
    path_and_query: &str,
) -> String {
    if let Some(base) = public_base_url {
        let base = base.trim().trim_end_matches('/');
        let path = if path_and_query.starts_with('/') {
            path_and_query
        } else {
            // defensive
            path_and_query
        };
        return format!("{base}{path}");
    }
    let host = host.trim();
    let path = if path_and_query.is_empty() {
        "/"
    } else {
        path_and_query
    };
    format!("{scheme}://{host}{path}")
}

/// Sanitize post-login `next=` (open-redirect guard).
///
/// Allows only same-origin **relative** paths: a single leading `/`, not `//`,
/// not backslashes, not schemes. Anything else becomes `/`.
///
/// Used by the login page JS (mirrored logic) and unit-tested here as the SoT.
pub fn sanitize_login_next(raw: &str) -> &str {
    let t = raw.trim();
    if t.is_empty() {
        return "/";
    }
    // Absolute URL or scheme-relative must not win.
    if t.contains("://") {
        return "/";
    }
    // Single leading slash only (rejects "", "foo", "http...", and "//...").
    if !t.starts_with('/') || t.starts_with("//") {
        return "/";
    }
    // Backslash forms (`/\`, `/\\evil`, encoded-ish host tricks in some UAs).
    if t.contains('\\') {
        return "/";
    }
    // Control characters (CR/LF injection, etc.).
    if t.chars().any(|c| c.is_control()) {
        return "/";
    }
    t
}

/// Paths that stay public even when `AuthMode::Nostr`.
pub fn is_public_path(path: &str) -> bool {
    matches!(
        path,
        "/health"
            | "/api/health"
            | "/api/v1/auth/challenge"
            | "/api/v1/auth/session"
            | "/api/v1/auth/logout"
            | "/login"
            // MTA-STS policy body (RFC 8461); receivers fetch without session.
            | "/.well-known/mta-sts.txt"
    )
}

/// Whether the path is treated as HTML (redirect to login) vs JSON API (401).
pub fn is_html_path(path: &str) -> bool {
    !path.starts_with("/api/")
}

/// Sign a NIP-98 event for tests/helpers (sync; never used with server nsec in product).
pub fn sign_nip98_event(
    keys: &Keys,
    url: &str,
    method: HttpMethod,
    created_at: Option<u64>,
) -> Result<Event, String> {
    let url = NostrUrl::parse(url).map_err(|e| e.to_string())?;
    let data = HttpData::new(url, method);
    let mut builder = EventBuilder::http_auth(data);
    if let Some(ts) = created_at {
        builder = builder.custom_created_at(Timestamp::from_secs(ts));
    }
    builder.sign_with_keys(keys).map_err(|e| e.to_string())
}

/// Encode event as `Authorization: Nostr <standard base64>` value (includes prefix).
pub fn event_to_nostr_authorization(event: &Event) -> String {
    let encoded = B64_STANDARD.encode(event.as_json());
    format!("Nostr {encoded}")
}

/// Challenge JSON for clients building NIP-98 events for session exchange.
#[derive(Debug, Clone, Serialize)]
pub struct ChallengeResponse {
    pub method: &'static str,
    pub url: String,
    pub created_at_window_secs: u64,
    pub kind: u16,
    pub auth_mode: &'static str,
    pub note: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_keys() -> Keys {
        Keys::generate()
    }

    #[test]
    fn parse_allowlist_npub_and_hex() {
        let keys = fixture_keys();
        let hex = keys.public_key().to_hex();
        let npub = keys.public_key().to_bech32().unwrap();
        let set = parse_allowlist(&format!("{npub}, {hex}")).unwrap();
        assert_eq!(set.len(), 1, "same key via npub+hex should dedupe");
        assert!(allowlist_contains(&set, &hex));

        let set2 = parse_allowlist(&hex).unwrap();
        assert!(allowlist_contains(&set2, &hex));

        let empty = parse_allowlist("  , \n ").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn parse_allowlist_rejects_garbage() {
        let err = parse_allowlist("not-a-key").unwrap_err();
        assert!(err.contains("invalid"), "{err}");
    }

    /// Named contract: nsec is refused without echoing the token; other
    /// parse failures are a fixed "invalid npub" with no token.
    #[test]
    fn normalize_pubkey_token_refuses_nsec_without_echo() {
        let nsec = concat!("nsec", "1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq").to_string();
        let err = normalize_pubkey_token(nsec).unwrap_err();
        assert_eq!(err, "nsec is not allowed");
        assert!(!err.contains(nsec), "must not echo nsec: {err}");
        let mixed = "NSEC1not-a-real-secret-token";
        let err = normalize_pubkey_token(mixed).unwrap_err();
        assert_eq!(err, "nsec is not allowed");
        assert!(!err.to_ascii_lowercase().contains("nsec1not"));
        let garbage = "not-an-npub-or-hex";
        let err = normalize_pubkey_token(garbage).unwrap_err();
        assert_eq!(err, "invalid npub");
        assert!(!err.contains(garbage), "must not echo token: {err}");
        let err = normalize_pubkey_token("").unwrap_err();
        assert_eq!(err, "invalid npub");
    }

    /// Named contract: env non-empty wins over file; empty both = fail-closed empty.
    #[test]
    fn resolve_allowlist_env_wins_over_file() {
        let keys = fixture_keys();
        let env_hex = keys.public_key().to_hex();
        let other = fixture_keys().public_key().to_hex();
        let dir =
            std::env::temp_dir().join(format!("surmount-allowlist-env-win-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("allowlist.txt");
        std::fs::write(&file, &other).unwrap();
        let set = resolve_allowlist(&env_hex, Some(file.to_str().unwrap())).unwrap();
        assert_eq!(set.len(), 1);
        assert!(allowlist_contains(&set, &env_hex));
        assert!(!allowlist_contains(&set, &other));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Named contract: empty env falls through to file path (same HashSet parse).
    #[test]
    fn resolve_allowlist_file_when_env_empty() {
        let keys = fixture_keys();
        let hex = keys.public_key().to_hex();
        let dir =
            std::env::temp_dir().join(format!("surmount-allowlist-file-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("allowlist.txt");
        std::fs::write(&file, format!("{hex}\n")).unwrap();
        let set = resolve_allowlist("  \n  ", Some(file.to_str().unwrap())).unwrap();
        assert!(allowlist_contains(&set, &hex));
        let empty = resolve_allowlist("", None).unwrap();
        assert!(empty.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_allowlist_missing_file_fail_closed() {
        let err = resolve_allowlist("", Some("/tmp/surmount-nostr-allowlist-does-not-exist-xyz"))
            .unwrap_err();
        assert!(
            err.contains("unreadable") || err.contains("fail-closed"),
            "{err}"
        );
    }

    #[test]
    fn auth_error_kind_is_stable_and_secret_free() {
        assert_eq!(
            auth_error_kind(&AuthError::NotAllowlisted),
            "not_allowlisted"
        );
        assert_eq!(auth_error_kind(&AuthError::Malformed), "malformed");
        assert_eq!(auth_error_kind(&AuthError::Csrf), "csrf");
    }

    #[test]
    fn nip98_valid_event_accept_when_allowlisted() {
        let keys = fixture_keys();
        let hex = keys.public_key().to_hex().to_ascii_lowercase();
        let allow = parse_allowlist(&hex).unwrap();
        let url = "http://127.0.0.1:8080/api/v1/auth/session";
        let now = 1_700_000_000u64;
        let event = sign_nip98_event(&keys, url, HttpMethod::POST, Some(now)).unwrap();
        let id = verify_nip98_event(&event, url, "POST", now, 300, &allow).unwrap();
        assert_eq!(id.hex_pubkey, hex);
    }

    #[test]
    fn nip98_wrong_key_not_allowlisted_rejects() {
        let keys = fixture_keys();
        let other = fixture_keys();
        let allow = parse_allowlist(&other.public_key().to_hex()).unwrap();
        let url = "http://127.0.0.1:8080/api/v1/auth/session";
        let now = 1_700_000_000u64;
        let event = sign_nip98_event(&keys, url, HttpMethod::POST, Some(now)).unwrap();
        let err = verify_nip98_event(&event, url, "POST", now, 300, &allow).unwrap_err();
        assert_eq!(err, AuthError::NotAllowlisted);
    }

    #[test]
    fn nip98_empty_allowlist_fail_closed() {
        let keys = fixture_keys();
        let allow = HashSet::new();
        let url = "http://127.0.0.1:8080/api/v1/auth/session";
        let now = 1_700_000_000u64;
        let event = sign_nip98_event(&keys, url, HttpMethod::POST, Some(now)).unwrap();
        let err = verify_nip98_event(&event, url, "POST", now, 300, &allow).unwrap_err();
        assert_eq!(err, AuthError::NotAllowlisted);
    }

    #[test]
    fn nip98_bad_signature_rejects() {
        let keys = fixture_keys();
        let allow = parse_allowlist(&keys.public_key().to_hex()).unwrap();
        let url = "http://127.0.0.1:8080/api/v1/auth/session";
        let now = 1_700_000_000u64;
        let mut event = sign_nip98_event(&keys, url, HttpMethod::POST, Some(now)).unwrap();
        // Tamper content without re-signing: re-parse JSON and flip a sig nibble if possible,
        // or use a second key's event with wrong kind path — easiest: alter sig via serde.
        let mut json: serde_json::Value = serde_json::from_str(&event.as_json()).unwrap();
        let sig = json.get_mut("sig").unwrap().as_str().unwrap().to_string();
        let mut chars: Vec<char> = sig.chars().collect();
        // Flip last hex digit.
        let last = chars.len() - 1;
        chars[last] = if chars[last] == '0' { '1' } else { '0' };
        json["sig"] = serde_json::Value::String(chars.into_iter().collect());
        event = Event::from_json(json.to_string()).unwrap();
        let err = verify_nip98_event(&event, url, "POST", now, 300, &allow).unwrap_err();
        assert_eq!(err, AuthError::BadSignature);
    }

    #[test]
    fn nip98_expired_beyond_skew_rejects() {
        let keys = fixture_keys();
        let allow = parse_allowlist(&keys.public_key().to_hex()).unwrap();
        let url = "http://127.0.0.1:8080/api/v1/auth/session";
        let created = 1_700_000_000u64;
        let now = created + 301; // default max skew 300
        let event = sign_nip98_event(&keys, url, HttpMethod::POST, Some(created)).unwrap();
        let err = verify_nip98_event(&event, url, "POST", now, 300, &allow).unwrap_err();
        assert_eq!(err, AuthError::Skew);
    }

    #[test]
    fn nip98_future_beyond_skew_rejects() {
        let keys = fixture_keys();
        let allow = parse_allowlist(&keys.public_key().to_hex()).unwrap();
        let url = "http://127.0.0.1:8080/api/v1/auth/session";
        let now = 1_700_000_000u64;
        let created = now + 400;
        let event = sign_nip98_event(&keys, url, HttpMethod::POST, Some(created)).unwrap();
        let err = verify_nip98_event(&event, url, "POST", now, 300, &allow).unwrap_err();
        assert_eq!(err, AuthError::Skew);
    }

    #[test]
    fn session_cookie_round_trip() {
        let secret = b"test-session-secret-32-bytes-long!!";
        let now = 1_700_000_000u64;
        let payload = SessionPayload {
            sub: "aa".repeat(32),
            exp: now + 3600,
        };
        let cookie = encode_session_cookie(&payload, secret).unwrap();
        let decoded = decode_session_cookie(&cookie, secret, now).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn session_cookie_tamper_rejects() {
        let secret = b"test-session-secret-32-bytes-long!!";
        let now = 1_700_000_000u64;
        let payload = SessionPayload {
            sub: "bb".repeat(32),
            exp: now + 3600,
        };
        let mut cookie = encode_session_cookie(&payload, secret).unwrap();
        cookie.push('x');
        assert_eq!(
            decode_session_cookie(&cookie, secret, now).unwrap_err(),
            AuthError::SessionInvalid
        );
    }

    #[test]
    fn session_cookie_expired_rejects() {
        let secret = b"test-session-secret-32-bytes-long!!";
        let now = 1_700_000_000u64;
        let payload = SessionPayload {
            sub: "cc".repeat(32),
            exp: now,
        };
        let cookie = encode_session_cookie(&payload, secret).unwrap();
        assert_eq!(
            decode_session_cookie(&cookie, secret, now).unwrap_err(),
            AuthError::SessionExpired
        );
    }

    #[test]
    fn auth_mode_parse() {
        assert_eq!(AuthMode::parse("off").unwrap(), AuthMode::Off);
        assert_eq!(AuthMode::parse("nostr").unwrap(), AuthMode::Nostr);
        assert!(AuthMode::parse("oauth").is_err());
    }

    #[test]
    fn auth_config_nostr_requires_secret() {
        let mut cfg = AuthConfig::off();
        cfg.mode = AuthMode::Nostr;
        assert!(cfg.validate().is_err());
        cfg.session_secret = Some(b"x".to_vec());
        assert!(cfg.validate().is_ok());
    }

    /// Named contract: public primary edge + AuthMode::Off is refused (footgun guard).
    /// Loopback/private binds may keep auth-off for lab/CI; world bind / global IP may not.
    #[test]
    fn public_primary_edge_auth_off_is_refused() {
        use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

        let loopback = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8090);
        let private = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5)), 8090);
        let wildcard = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 443);
        let public = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)), 443);
        let v6_wildcard = SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 443);

        assert!(!listen_is_public_primary_edge(loopback));
        assert!(!listen_is_public_primary_edge(private));
        assert!(listen_is_public_primary_edge(wildcard));
        assert!(listen_is_public_primary_edge(public));
        assert!(listen_is_public_primary_edge(v6_wildcard));

        // Auth off on loopback / private is OK (local just dev / lab).
        assert!(require_auth_when_public_edge(AuthMode::Off, loopback, false).is_ok());
        assert!(require_auth_when_public_edge(AuthMode::Off, private, false).is_ok());

        // Public bind + off must fail closed.
        let err = require_auth_when_public_edge(AuthMode::Off, wildcard, false).unwrap_err();
        assert!(
            err.contains("AUTH_MODE") && (err.contains("public") || err.contains("off")),
            "{err}"
        );
        let err_pub = require_auth_when_public_edge(AuthMode::Off, public, false).unwrap_err();
        assert!(
            err_pub.contains("AUTH_MODE") || err_pub.contains("public"),
            "{err_pub}"
        );

        // Nostr on public is OK (secret checked separately by validate).
        assert!(require_auth_when_public_edge(AuthMode::Nostr, wildcard, false).is_ok());

        // Lab escape allows intentional public+off (tests only; never production default).
        assert!(require_auth_when_public_edge(AuthMode::Off, wildcard, true).is_ok());

        // AuthConfig::validate_for_listen wires the same contract.
        let off = AuthConfig::off();
        assert!(off.validate_for_listen(loopback, false).is_ok());
        let err = off.validate_for_listen(wildcard, false).unwrap_err();
        assert!(
            err.contains("AUTH_MODE") || err.contains("public") || err.contains("off"),
            "{err}"
        );
        assert!(off.validate_for_listen(wildcard, true).is_ok());
    }

    #[test]
    fn public_paths_include_health_and_auth() {
        assert!(is_public_path("/health"));
        assert!(is_public_path("/api/v1/auth/session"));
        assert!(is_public_path("/login"));
        assert!(is_public_path("/.well-known/mta-sts.txt"));
        assert!(!is_public_path("/"));
        assert!(!is_public_path("/api/v1/domains"));
    }

    #[test]
    fn absolute_url_prefers_public_base() {
        let u = absolute_request_url(
            Some("https://services.example.test"),
            "http",
            "127.0.0.1:8080",
            "/api/v1/auth/session",
        );
        assert_eq!(u, "https://services.example.test/api/v1/auth/session");
    }

    /// Named contract: login `next=` only same-origin relative paths (G1 open-redirect).
    #[test]
    fn sanitize_login_next_rejects_open_redirects() {
        assert_eq!(sanitize_login_next("/"), "/");
        assert_eq!(sanitize_login_next("/domains"), "/domains");
        assert_eq!(sanitize_login_next("/system?x=1"), "/system?x=1");
        assert_eq!(sanitize_login_next("  /mail  "), "/mail");

        // Protocol-relative and multi-slash.
        assert_eq!(sanitize_login_next("//evil.example"), "/");
        assert_eq!(sanitize_login_next("//evil.example/phish"), "/");
        assert_eq!(sanitize_login_next("///evil"), "/");

        // Schemes and non-relative.
        assert_eq!(sanitize_login_next("https://evil.example/"), "/");
        assert_eq!(sanitize_login_next("http://evil.example"), "/");
        assert_eq!(sanitize_login_next("javascript:alert(1)"), "/");
        assert_eq!(sanitize_login_next("domains"), "/");
        assert_eq!(sanitize_login_next(""), "/");

        // Backslash tricks.
        assert_eq!(sanitize_login_next("/\\evil.example"), "/");
        assert_eq!(sanitize_login_next("/\\/evil.example"), "/");
    }

    #[test]
    fn session_cookie_secure_flag_when_https() {
        let plain = session_set_cookie_header("abc", 60, false);
        assert!(plain.contains("HttpOnly"));
        assert!(plain.contains("SameSite=Lax"));
        assert!(!plain.contains("Secure"));

        let https = session_set_cookie_header("abc", 60, true);
        assert!(https.contains("; Secure"));
        assert!(https.contains("SameSite=Lax"));

        let clear_https = session_clear_cookie_header(true);
        assert!(clear_https.contains("; Secure"));
        assert!(clear_https.contains("Max-Age=0"));
    }

    #[test]
    fn csrf_double_submit_accepts_match_rejects_mismatch() {
        assert!(verify_csrf_double_submit(Some("tok-a"), Some("tok-a")).is_ok());
        assert_eq!(
            verify_csrf_double_submit(Some("tok-a"), Some("tok-b")).unwrap_err(),
            AuthError::Csrf
        );
        assert_eq!(
            verify_csrf_double_submit(None, Some("tok-a")).unwrap_err(),
            AuthError::Csrf
        );
        assert_eq!(
            verify_csrf_double_submit(Some("tok-a"), None).unwrap_err(),
            AuthError::Csrf
        );
        assert_eq!(
            verify_csrf_double_submit(Some(""), Some("")).unwrap_err(),
            AuthError::Csrf
        );
    }

    #[test]
    fn session_bound_csrf_matches_same_session_rejects_wrong() {
        let secret = b"unit-test-session-secret-csrf-v1!!";
        let sess = "payload.sig-not-verified-here";
        let a = session_bound_csrf_token(sess, secret).unwrap();
        let b = session_bound_csrf_token(sess, secret).unwrap();
        assert_eq!(a, b);
        assert_ne!(
            a,
            session_bound_csrf_token("other.session", secret).unwrap()
        );
        assert!(verify_csrf_session_bound(Some(sess), secret, Some(&a)).is_ok());
        assert_eq!(
            verify_csrf_session_bound(Some(sess), secret, Some("nope")).unwrap_err(),
            AuthError::Csrf
        );
        assert_eq!(
            verify_csrf_session_bound(Some(sess), secret, Some("")).unwrap_err(),
            AuthError::Csrf
        );
        assert_eq!(
            verify_csrf_session_bound(None, secret, Some(&a)).unwrap_err(),
            AuthError::Csrf
        );
        assert!(
            std::str::from_utf8(CSRF_SESSION_MAC_PURPOSE).unwrap() == "surmount-csrf-session-v1"
        );
    }

    #[test]
    fn csrf_cookie_header_httponly_samesite_lax() {
        let h = csrf_set_cookie_header("secret-token", 3600, true);
        assert!(h.starts_with(&format!("{CSRF_COOKIE_NAME}=secret-token")));
        assert!(h.contains("SameSite=Lax"));
        assert!(h.contains("; Secure"));
        assert!(h.to_ascii_lowercase().contains("httponly"));
        assert!(h.contains("Path=/"));
        let clear = csrf_clear_cookie_header(true);
        assert!(clear.to_ascii_lowercase().contains("httponly"));
        assert!(clear.contains("Path=/"));
        assert!(clear.contains("Max-Age=0"));
    }

    #[test]
    fn baseline_csp_is_lean_with_nonce_and_frame_ancestors() {
        let csp = baseline_csp("abc123Nonce");
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("script-src 'self' 'nonce-abc123Nonce'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("style-src 'self' 'unsafe-inline'"));
        assert!(!csp.contains("https://"));
        assert!(!csp.contains("cdn."));
    }

    #[test]
    fn random_urlsafe_token_nonempty() {
        let a = random_urlsafe_token().unwrap();
        let b = random_urlsafe_token().unwrap();
        assert!(a.len() >= 32);
        assert_ne!(a, b);
    }
}
