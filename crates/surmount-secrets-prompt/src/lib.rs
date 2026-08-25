//! Domain A secret intake helpers for Surmount operators.
//!
//! Writes private staging trees compatible with `nix run .#secrets-install-host`
//! (`attributes` + `secret` mode 0600). Optional Secret Service store is done
//! via subprocess `secret-tool` in the binary (S5 libsecret still parked).
//!
//! Never log secret values. Tests use synthetic payloads only.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Allowlisted kinds this tool can create.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    NamecheapApi,
    /// Sovereign Hybrid Compute customer user-api key (rDNS / operate scope).
    ShcApi,
    StalwartToken,
    SessionSecret,
    VaultwardenAdmin,
    /// DiskStation AFP LAN password. Laptop Secret Service only. Never install to Domain B.
    SynologyAfp,
}

/// 5-bay Denver office DiskStation (canonical synology-afp host id).
pub const SYNOLOGY_AFP_HOST_DS1513: &str = "DS1513";
/// 6-bay Denver office DiskStation (canonical synology-afp host id).
pub const SYNOLOGY_AFP_HOST_DS3018XS: &str = "DS3018xs";

/// Canonical synology-afp host ids (not a generic diskstation name, not an IP).
pub fn synology_afp_canonical_hosts() -> &'static [&'static str] {
    &[SYNOLOGY_AFP_HOST_DS1513, SYNOLOGY_AFP_HOST_DS3018XS]
}

/// Allow only DS1513 and DS3018xs for kind synology-afp.
///
/// Legacy `diskstation` is refused with a re-store hint. Case must match.
pub fn validate_synology_afp_host(host: &str) -> Result<()> {
    validate_host_id(host)?;
    if host == SYNOLOGY_AFP_HOST_DS1513 || host == SYNOLOGY_AFP_HOST_DS3018XS {
        return Ok(());
    }
    if host.eq_ignore_ascii_case("diskstation") {
        bail!(
            "synology-afp host {host:?} is the old generic id. \
             Re-store the LAN password under DS1513 (5-bay) or DS3018xs (6-bay). \
             Example: just secrets-prompt -- synology-afp --host DS1513 \
             --secret-service --no-staging"
        );
    }
    bail!(
        "synology-afp host must be DS1513 or DS3018xs (got {host:?}). \
         Do not use a generic diskstation id or an IP."
    );
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::NamecheapApi => "namecheap-api",
            Kind::ShcApi => "shc-api",
            Kind::StalwartToken => "stalwart-token",
            Kind::SessionSecret => "session-secret",
            Kind::VaultwardenAdmin => "vaultwarden-admin",
            Kind::SynologyAfp => "synology-afp",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "namecheap-api" => Ok(Kind::NamecheapApi),
            "shc-api" => Ok(Kind::ShcApi),
            "stalwart-token" => Ok(Kind::StalwartToken),
            "session-secret" => Ok(Kind::SessionSecret),
            "vaultwarden-admin" => Ok(Kind::VaultwardenAdmin),
            "synology-afp" => Ok(Kind::SynologyAfp),
            other => bail!(
                "unknown kind {other:?}; allowlisted: namecheap-api, shc-api, \
                 stalwart-token, session-secret, vaultwarden-admin, synology-afp"
            ),
        }
    }

    /// Laptop Secret Service only. `secrets-install-host` must refuse these kinds.
    pub fn is_laptop_only(self) -> bool {
        matches!(self, Kind::SynologyAfp)
    }

    /// Conventional Domain B path for this kind (install bridge map).
    /// Durable root `/var/lib/surmount/secrets` survives reboot (S8 option A).
    /// Ephemeral `/run/surmount-secrets` remains allowlisted for optional use.
    /// Laptop-only kinds return empty (no Domain B path).
    pub fn default_path(self) -> &'static str {
        match self {
            Kind::NamecheapApi => "/var/lib/surmount/secrets/acme/namecheap.env",
            Kind::ShcApi => "/var/lib/surmount/secrets/rdns/shc.env",
            Kind::StalwartToken => "/var/lib/surmount/secrets/ui/stalwart-api-token",
            Kind::SessionSecret => "/var/lib/surmount/secrets/ui/session-secret",
            Kind::VaultwardenAdmin => "/var/lib/surmount/secrets/vaultwarden/admin.env",
            Kind::SynologyAfp => "",
        }
    }
}

/// Non-secret Namecheap fields assembled into KEY=value env body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamecheapFields {
    pub api_user: String,
    pub api_key: String,
    pub client_ip: String,
    pub sld: String,
    pub tld: String,
    /// Optional; when empty, omitted from the body.
    pub user_name: Option<String>,
}

/// SHC customer user-api KEY=value body fields (rDNS / operate scope).
///
/// Product name is SHC user API; Blesta is the portal brand. Prefer an operate
/// scoped key (`shc_live_...`); never invent or generate the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShcFields {
    pub api_key: String,
    /// API base URL (no trailing slash). Default: SHC user-api v2 base.
    pub api_base: String,
    /// Optional Blesta service id for the VM. When empty, omitted (list /vm).
    pub service_id: Option<String>,
}

/// Default SHC user-api v2 base (OpenAPI / KB; no trailing slash).
pub const SHC_DEFAULT_API_BASE: &str = "https://blesta.sovereignhybridcompute.com/user-api/v2";

/// Common multi-part public suffixes Namecheap treats as one TLD field.
/// Longest-match wins when splitting a full domain into SLD + TLD.
const MULTI_PART_TLDS: &[&str] = &[
    "co.uk", "org.uk", "me.uk", "net.uk", "ac.uk", "gov.uk", "com.au", "net.au", "org.au", "id.au",
    "co.nz", "net.nz", "org.nz", "co.jp", "or.jp", "ne.jp", "com.br", "net.br", "org.br", "co.za",
    "org.za", "web.za", "com.mx", "org.mx", "co.in", "net.in", "org.in", "com.sg", "com.hk",
    "com.tw", "co.kr", "or.kr", "com.ar", "com.tr",
];

/// Split a full domain into Namecheap SLD + TLD.
///
/// Prefer the registered domain (for example `example.com` or `example.co.uk`),
/// not a hostname like `mail.example.com` (that becomes SLD=`mail.example`).
///
/// Rules:
/// - Strip whitespace and a trailing dot; lowercase labels.
/// - If the domain ends with a known multi-part TLD (for example `co.uk`), that
///   whole suffix is TLD and the label(s) before it are SLD.
/// - Otherwise the last label is TLD and everything before it is SLD.
///
/// Examples: `example.com` -> (`example`, `com`); `example.co.uk` ->
/// (`example`, `co.uk`); `mail.example.com` -> (`mail.example`, `com`).
pub fn split_domain(domain: &str) -> Result<(String, String)> {
    let mut d = domain.trim().to_ascii_lowercase();
    while d.ends_with('.') {
        d.pop();
    }
    if d.is_empty() {
        bail!("domain must not be empty");
    }
    if d.contains('=') || d.contains('\n') || d.contains(' ') {
        bail!("domain must not contain spaces, '=', or newlines");
    }
    if d.contains('/') || d.contains(':') {
        bail!("domain must be a bare DNS name (no URL scheme or path)");
    }
    if !d.contains('.') {
        bail!(
            "domain needs a dot (for example example.com). \
             Or pass --sld and --tld separately."
        );
    }

    // Longest multi-part TLD match.
    let mut best: Option<&str> = None;
    for tld in MULTI_PART_TLDS {
        let suffix = format!(".{tld}");
        if d.ends_with(&suffix)
            && d.len() > suffix.len()
            && best.map(|b| tld.len() > b.len()).unwrap_or(true)
        {
            best = Some(*tld);
        }
    }

    let (sld, tld) = if let Some(tld) = best {
        let prefix_len = d.len() - tld.len() - 1; // drop trailing '.'
        let sld = d[..prefix_len].to_string();
        (sld, tld.to_string())
    } else {
        let (left, right) = d.rsplit_once('.').expect("contains '.'");
        (left.to_string(), right.to_string())
    };

    if sld.is_empty() || tld.is_empty() {
        bail!("could not split domain {domain:?} into SLD and TLD");
    }
    if sld.starts_with('.') || sld.ends_with('.') || sld.contains("..") {
        bail!("invalid SLD from domain {domain:?}");
    }
    if tld.contains("..") || tld.starts_with('.') {
        bail!("invalid TLD from domain {domain:?}");
    }
    Ok((sld, tld))
}

/// Resolve SLD/TLD from optional full domain and/or separate flags.
/// Explicit `--sld` + `--tld` win when both are set. `--domain` alone is split.
pub fn resolve_sld_tld(
    domain: Option<&str>,
    sld: Option<&str>,
    tld: Option<&str>,
) -> Result<Option<(String, String)>> {
    let domain = domain.map(str::trim).filter(|s| !s.is_empty());
    let sld = sld.map(str::trim).filter(|s| !s.is_empty());
    let tld = tld.map(str::trim).filter(|s| !s.is_empty());

    match (domain, sld, tld) {
        (_, Some(s), Some(t)) => Ok(Some((s.to_string(), t.to_string()))),
        (Some(d), None, None) => split_domain(d).map(Some),
        (None, None, None) => Ok(None),
        (Some(_), Some(_), None) | (Some(_), None, Some(_)) => bail!(
            "when using --domain with --sld or --tld, provide both --sld and --tld \
             (or only --domain)"
        ),
        (None, Some(_), None) | (None, None, Some(_)) => {
            bail!("provide both --sld and --tld, or a single --domain (for example example.com)")
        }
    }
}

// --- Plain-English field help (interactive prompts + tests) ---

/// Intro banner for interactive Namecheap intake (stderr).
pub fn namecheap_intro_banner(host: &str) -> String {
    format!(
        "\n\
=== Surmount Namecheap API secret intake ===\n\
This writes a private staging file for host \"{host}\" with your Namecheap\n\
API settings. Secret values (ApiKey) are not echoed as you type and are not\n\
printed afterward. Install to the server later with secrets-install-host.\n\
Fields marked optional can be left blank with Enter.\n"
    )
}

/// Intro banner for interactive SHC customer API intake (stderr).
pub fn shc_intro_banner(host: &str) -> String {
    format!(
        "\n\
=== Surmount SHC customer API secret intake ===\n\
This writes a private staging file for host \"{host}\" with your Sovereign\n\
Hybrid Compute (SHC) customer user-api key. Use this for PTR/rDNS automation\n\
(and other operate-scoped customer API calls). The ApiKey is not echoed as you\n\
type and is not printed afterward.\n\
\n\
Create an operate-scoped key in the SHC portal (API Keys page). Prefer operate\n\
scope (cannot spend). Do NOT invent a key. --generate is refused.\n\
\n\
Install to the server later with secrets-install-host, or keep Domain A on the\n\
laptop and point nix run .#surmount-shc at the staging secret / env file.\n\
Fields marked optional can be left blank with Enter.\n"
    )
}

/// Explanation for SHC ApiKey.
pub fn explain_shc_api_key() -> &'static str {
    "ApiKey is a customer key from the SHC portal API Keys page \
     (Authorization: Bearer shc_live_...). Prefer operate scope. Input is hidden. \
     Never invent or generate this value."
}

/// Explanation for SHC API base URL.
pub fn explain_shc_api_base() -> &'static str {
    "ApiBase is the SHC user-api base URL without a trailing slash. Default is \
     https://blesta.sovereignhybridcompute.com/user-api/v2 . Leave blank to use \
     the default unless you have a lab override."
}

/// Explanation for optional SHC ServiceId.
pub fn explain_shc_service_id() -> &'static str {
    "ServiceId is the Blesta service id for your VM (integer). Optional: leave \
     blank if you will list VMs later with the rDNS tool (GET /vm). Set it when \
     you already know the id so set/list rDNS can skip discovery."
}

/// Prompt labels for SHC fields.
pub fn prompt_label_shc_api_key() -> &'static str {
    "ApiKey (shc_live_...; input hidden)"
}

pub fn prompt_label_shc_api_base() -> &'static str {
    "ApiBase (Enter for default user-api/v2)"
}

pub fn prompt_label_shc_service_id() -> &'static str {
    "ServiceId (optional Blesta VM service id; Enter to skip)"
}

/// Plain-English next steps after writing Domain A `shc-api` staging.
/// Never includes secret values.
pub fn next_steps_after_shc_api(staging: &Path, host: &str) -> String {
    let staging_disp = staging.display();
    let path_b = Kind::ShcApi.default_path();
    format!(
        "\n\
Next steps (secret values not shown):\n\
  1) Domain A staging is ready under:\n\
       {staging_disp}\n\
     (item: shc-api; host: {host})\n\
  2) Optional install to durable Domain B on the VPS:\n\
       just secrets-install-host -- --from-staging {staging_disp} \\\n\
         --host-id {host} --target root@YOUR_HOST \\\n\
         --require-kind shc-api\n\
     Destination: {path_b}\n\
  3) Set PTR/rDNS (default dry-run; never prints ApiKey):\n\
       # From laptop using Domain A staging secret path, or Domain B path:\n\
       just rdns-shc -- --list\n\
       just rdns-shc -- --hostname mail.surmount.systems --ip YOUR_VPS_IP\n\
       just rdns-shc -- --live --hostname mail.surmount.systems --ip YOUR_VPS_IP\n\
     Credentials: SURMOUNT_RDNS_SHC_ENV or default Domain B path.\n\
  4) FCrDNS: forward A for mail.surmount.systems must already point at that IP\n\
     (Namecheap forward zone is separate). Provider console rDNS still valid.\n\
  Docs: docs/DNS.md (PTR/rDNS), docs/OPS.md, docs/SECRETS.md\n"
    )
}

/// Intro banner for interactive Stalwart admin / management API token intake.
pub fn stalwart_token_intro_banner(host: &str) -> String {
    format!(
        "\n\
=== Surmount Stalwart admin token intake ===\n\
Paste the admin credential Stalwart already accepts (first-boot admin password\n\
or an API token the engine already knows) for host \"{host}\".\n\
Input is hidden. The value is not printed afterward.\n\
\n\
Do NOT invent a new random token here. Random generate is not engine\n\
registration. free-443 HTTP 401 means the Domain B value is wrong/unknown.\n\
\n\
This writes private Domain A staging only. Domain B install is a separate step\n\
(or use: just add-stalwart-token -- --target root@HOST).\n"
    )
}

/// Short explanation before the no-echo Stalwart token prompt.
pub fn explain_stalwart_token() -> &'static str {
    "Paste a credential Stalwart already accepts as admin/API (single line). \
     Not a new random string you invent. Input is hidden."
}

/// Plain-English next steps after writing Domain A `stalwart-token` staging.
/// Never includes secret values. Paths and host id only.
pub fn next_steps_after_stalwart_token(staging: &Path, host: &str) -> String {
    let staging_disp = staging.display();
    let path_b = Kind::StalwartToken.default_path();
    format!(
        "\n\
Next steps (secret values not shown):\n\
  1) Domain A staging is ready under:\n\
       {staging_disp}\n\
     (item: stalwart-token; host: {host})\n\
  2) Install to durable Domain B on the VPS:\n\
       just secrets-install-host -- --from-staging {staging_disp} \\\n\
         --host-id {host} --target root@YOUR_HOST \\\n\
         --require-kind stalwart-token\n\
     Or one flow: just add-stalwart-token -- --host {host} --target root@YOUR_HOST\n\
     Destination: {path_b}\n\
  3) Free Stalwart public :443 (needs the engine-accepted value above):\n\
       just free-stalwart-public-443 -- --dry-run\n\
       # live on operator host only:\n\
       just free-stalwart-public-443 -- --live --restart\n\
  4) generate != engine registration. Live free-443 401 means fix the value,\n\
     not the free-443 script. Docs: docs/OPS.md (add Stalwart token), docs/SECRETS.md\n"
    )
}

/// Explanation for Surmount logical host id (not a Namecheap field).
pub fn explain_host_id() -> &'static str {
    "Logical host id is Surmount's inventory name for this machine \
     (for example: surmount-1). It is NOT your Namecheap username, domain name, \
     or IP address."
}

/// Explanation for Namecheap ApiUser.
pub fn explain_api_user() -> &'static str {
    "ApiUser is usually your Namecheap account username (the same name you use \
     to log in for many personal accounts)."
}

/// Explanation for optional Namecheap UserName.
pub fn explain_user_name() -> &'static str {
    "UserName is Namecheap's API UserName. For most personal accounts it matches \
     ApiUser, so you can leave it blank. Set it only if you use a Namecheap \
     sub-user with its own API UserName."
}

/// Explanation for domain / SLD+TLD.
pub fn explain_domain() -> &'static str {
    "Enter the registered domain (for example example.com). We split it into \
     SLD (second-level, the name before the public suffix) and TLD (top-level, \
     such as com or co.uk). Example: example.com -> SLD=example, TLD=com. \
     Prefer the apex you registered, not a host like mail.example.com."
}

/// Explanation for Namecheap ApiKey.
pub fn explain_api_key() -> &'static str {
    "ApiKey comes from Namecheap: Profile -> Tools -> API Access. Input is hidden."
}

/// Explanation for ClientIp.
pub fn explain_client_ip() -> &'static str {
    "ClientIp is the public IP of the machine that will call the Namecheap API \
     (your laptop for local tools, or the VPS public IP for host ACME). It is \
     not your domain name and not Namecheap's IP. Whitelist that same IP under \
     Namecheap API Access. If unsure, look up your public IP (for example \
     whatismyip) from the machine that will make the API calls."
}

/// Prompt label for host id.
pub fn prompt_label_host_id() -> &'static str {
    "Logical host id (Surmount inventory name)"
}

/// Prompt label for ApiUser.
pub fn prompt_label_api_user() -> &'static str {
    "ApiUser (Namecheap account username)"
}

/// Prompt label for optional UserName.
pub fn prompt_label_user_name() -> &'static str {
    "UserName (optional; Enter to skip if same as ApiUser)"
}

/// Prompt label for full domain.
pub fn prompt_label_domain() -> &'static str {
    "Domain (for example example.com)"
}

/// Prompt label for ClientIp.
pub fn prompt_label_client_ip() -> &'static str {
    "ClientIp (public IP of the API caller)"
}

/// Prompt label for ApiKey.
pub fn prompt_label_api_key() -> &'static str {
    "ApiKey (input hidden)"
}

/// Validate a logical host id (no control chars, not empty, not option-shaped).
pub fn validate_host_id(host: &str) -> Result<()> {
    if host.is_empty() {
        bail!("host id must not be empty");
    }
    if host.starts_with('-') {
        bail!("host id must not start with '-'");
    }
    if host.chars().any(|c| c.is_control()) {
        bail!("host id must not contain control characters");
    }
    if host.contains('=') || host.contains('\n') {
        bail!("host id must not contain '=' or newlines");
    }
    Ok(())
}

/// Validate an absolute host path for surmount.path (string-level checks).
pub fn validate_host_path(path: &str) -> Result<()> {
    if path.is_empty() {
        bail!("surmount.path must not be empty");
    }
    if !path.starts_with('/') {
        bail!("surmount.path must be absolute (start with /)");
    }
    if path.contains('\n') || path.chars().any(|c| c.is_control()) {
        bail!("surmount.path must not contain control characters");
    }
    for part in path.split('/').filter(|p| !p.is_empty()) {
        if part == "." || part == ".." {
            bail!("surmount.path must not contain '.' or '..' segments");
        }
    }
    let under_durable =
        path.starts_with("/var/lib/surmount/secrets/") || path == "/var/lib/surmount/secrets";
    let under_run = path.starts_with("/run/surmount-secrets/") || path == "/run/surmount-secrets";
    let under_sops = path.starts_with("/var/lib/sops-nix/") || path == "/var/lib/sops-nix";
    if !under_durable && !under_run && !under_sops {
        bail!(
            "surmount.path must be under /var/lib/surmount/secrets (durable), \
             /run/surmount-secrets (ephemeral), or /var/lib/sops-nix (got {path})"
        );
    }
    Ok(())
}

/// Fail closed if secret payload is empty or only whitespace.
pub fn require_nonempty_secret(secret: &str) -> Result<()> {
    if secret.is_empty() || secret.trim().is_empty() {
        bail!("secret payload is empty; refuse to write");
    }
    Ok(())
}

/// Build attributes file body (`key=value` lines, trailing newline).
pub fn build_attributes(kind: Kind, host: &str, path: &str) -> Result<String> {
    if kind == Kind::SynologyAfp {
        validate_synology_afp_host(host)?;
    } else {
        validate_host_id(host)?;
    }
    if kind.is_laptop_only() {
        return Ok(format!(
            "surmount.kind={}\nsurmount.host={}\n",
            kind.as_str(),
            host
        ));
    }
    validate_host_path(path)?;
    Ok(format!(
        "surmount.kind={}\nsurmount.host={}\nsurmount.path={}\n",
        kind.as_str(),
        host,
        path
    ))
}

/// Assemble shc.env KEY=value body for SHC customer user-api. Never logs the key.
pub fn build_shc_body(fields: &ShcFields) -> Result<String> {
    require_nonempty_secret(&fields.api_key)?;
    if fields.api_key.contains('\n') {
        bail!("ApiKey must be a single line");
    }
    let api_key = fields.api_key.trim();
    if api_key.is_empty() {
        bail!("ApiKey must not be empty");
    }

    let mut api_base = fields.api_base.trim().to_string();
    if api_base.is_empty() {
        api_base = SHC_DEFAULT_API_BASE.to_string();
    }
    while api_base.ends_with('/') {
        api_base.pop();
    }
    if api_base.contains('\n') || api_base.contains('=') {
        bail!("ApiBase must not contain newlines or '='");
    }
    if !(api_base.starts_with("https://") || api_base.starts_with("http://")) {
        bail!("ApiBase must be an http(s) URL");
    }

    let mut out = format!("ApiKey={api_key}\nApiBase={api_base}\n");
    if let Some(ref sid) = fields.service_id {
        let sid = sid.trim();
        if !sid.is_empty() {
            if sid.contains('\n') || sid.contains('=') {
                bail!("ServiceId must not contain newlines or '='");
            }
            // Blesta service ids are positive integers; accept digits only.
            if !sid.chars().all(|c| c.is_ascii_digit()) {
                bail!("ServiceId must be a positive integer (digits only)");
            }
            if sid == "0" {
                bail!("ServiceId must be a positive integer (not zero)");
            }
            out.push_str(&format!("ServiceId={sid}\n"));
        }
    }
    Ok(out)
}

/// Assemble namecheap.env KEY=value body. Never logs the key.
pub fn build_namecheap_body(fields: &NamecheapFields) -> Result<String> {
    if fields.api_user.trim().is_empty() {
        bail!("ApiUser must not be empty");
    }
    require_nonempty_secret(&fields.api_key)?;
    if fields.client_ip.trim().is_empty() {
        bail!("ClientIp must not be empty");
    }
    if fields.sld.trim().is_empty() {
        bail!("SLD must not be empty");
    }
    if fields.tld.trim().is_empty() {
        bail!("TLD must not be empty");
    }
    for (label, v) in [
        ("ApiUser", fields.api_user.as_str()),
        ("ClientIp", fields.client_ip.as_str()),
        ("SLD", fields.sld.as_str()),
        ("TLD", fields.tld.as_str()),
    ] {
        if v.contains('\n') || v.contains('=') {
            bail!("{label} must not contain newlines or '='");
        }
    }
    if fields.api_key.contains('\n') {
        bail!("ApiKey must be a single line");
    }
    let mut out = format!(
        "ApiUser={}\nApiKey={}\nClientIp={}\nSLD={}\nTLD={}\n",
        fields.api_user.trim(),
        fields.api_key.trim(),
        fields.client_ip.trim(),
        fields.sld.trim(),
        fields.tld.trim()
    );
    if let Some(ref un) = fields.user_name {
        let un = un.trim();
        if !un.is_empty() {
            if un.contains('\n') || un.contains('=') {
                bail!("UserName must not contain newlines or '='");
            }
            out.push_str(&format!("UserName={un}\n"));
        }
    }
    Ok(out)
}

/// Vaultwarden EnvironmentFile body: `ADMIN_TOKEN=<token>\n`.
pub fn build_vaultwarden_admin_body(token: &str) -> Result<String> {
    require_nonempty_secret(token)?;
    if token.contains('\n') {
        bail!("vaultwarden-admin token must be a single line");
    }
    Ok(format!("ADMIN_TOKEN={}\n", token.trim()))
}

/// Single-line secret kinds (stalwart-token, session-secret raw).
pub fn build_single_line_secret(secret: &str) -> Result<String> {
    require_nonempty_secret(secret)?;
    let line = secret.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        bail!("secret payload is empty; refuse to write");
    }
    if line.contains('\n') {
        bail!("secret must be a single line");
    }
    Ok(line.to_string())
}

/// Generate a session HMAC secret (`openssl rand -hex 32` shape: 64 hex chars).
pub fn generate_session_secret_hex() -> Result<String> {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf)
        .map_err(|e| anyhow::anyhow!("getrandom for session-secret: {e}"))?;
    let mut hex = String::with_capacity(64);
    for b in buf {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

/// Generate a random token suitable for vaultwarden-admin (64 hex chars).
pub fn generate_token_hex() -> Result<String> {
    generate_session_secret_hex()
}

/// Default Domain A staging root: `$XDG_DATA_HOME/surmount/staging` or
/// `~/.local/share/surmount/staging`.
pub fn default_staging_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("surmount").join("staging");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("surmount")
            .join("staging");
    }
    PathBuf::from("surmount-staging")
}

/// Written staging item paths (for operator log lines; no secret content).
#[derive(Debug, Clone)]
pub struct StagingWrite {
    pub item_dir: PathBuf,
    pub attributes_path: PathBuf,
    pub secret_path: PathBuf,
}

/// Write one staging item: `<staging>/<item_id>/{attributes,secret}` mode 0600.
///
/// Directory is created mode 0700 when possible. Secret must be non-empty.
/// Refuses if `secret` path would be a symlink after write (postcondition).
pub fn write_staging_item(
    staging: &Path,
    item_id: &str,
    attributes: &str,
    secret: &str,
) -> Result<StagingWrite> {
    require_nonempty_secret(secret)?;
    validate_item_id(item_id)?;

    let item_dir = staging.join(item_id);
    fs::create_dir_all(&item_dir)
        .with_context(|| format!("create staging item dir {}", item_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&item_dir, fs::Permissions::from_mode(0o700));
    }

    let attributes_path = item_dir.join("attributes");
    let secret_path = item_dir.join("secret");

    write_mode_0600(&attributes_path, attributes.as_bytes())
        .with_context(|| format!("write attributes {}", attributes_path.display()))?;
    write_mode_0600(&secret_path, secret.as_bytes())
        .with_context(|| format!("write secret {}", secret_path.display()))?;

    if secret_path
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        let _ = fs::remove_file(&secret_path);
        bail!("refused: secret path is a symlink after write");
    }

    Ok(StagingWrite {
        item_dir,
        attributes_path,
        secret_path,
    })
}

fn validate_item_id(item_id: &str) -> Result<()> {
    if item_id.is_empty() {
        bail!("item id must not be empty");
    }
    if item_id.contains('/') || item_id.contains('\\') || item_id == ".." || item_id == "." {
        bail!("item id must be a single path segment (got {item_id:?})");
    }
    if item_id.chars().any(|c| c.is_control()) {
        bail!("item id must not contain control characters");
    }
    Ok(())
}

fn write_mode_0600(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Read secret bytes from a file path (batch mode).
pub fn read_secret_file(path: &Path) -> Result<String> {
    let s =
        fs::read_to_string(path).with_context(|| format!("read secret file {}", path.display()))?;
    require_nonempty_secret(&s)?;
    Ok(s)
}

/// Read secret from a raw file descriptor (batch mode; tests control the fd).
///
/// Takes ownership of `fd` and closes it when the read finishes.
#[cfg(unix)]
pub fn read_secret_from_fd(fd: i32) -> Result<String> {
    use std::os::unix::io::FromRawFd;
    // Safety: exclusive ownership of `fd` for this call; File Drop closes it.
    let mut f = unsafe { File::from_raw_fd(fd) };
    let mut buf = String::new();
    io::Read::read_to_string(&mut f, &mut buf).context("read secret from fd")?;
    drop(f);
    require_nonempty_secret(&buf)?;
    Ok(buf)
}

/// Build `secret-tool store` argv (without the binary name). Stdin gets secret.
pub fn secret_tool_store_args(kind: Kind, host: &str, path: &str) -> Result<Vec<String>> {
    if kind == Kind::SynologyAfp {
        validate_synology_afp_host(host)?;
    } else {
        validate_host_id(host)?;
    }
    let label = format!("surmount {} {}", kind.as_str(), host);
    if kind.is_laptop_only() {
        return Ok(vec![
            "store".into(),
            format!("--label={label}"),
            "surmount.kind".into(),
            kind.as_str().into(),
            "surmount.host".into(),
            host.into(),
        ]);
    }
    validate_host_path(path)?;
    Ok(vec![
        "store".into(),
        format!("--label={label}"),
        "surmount.kind".into(),
        kind.as_str().into(),
        "surmount.host".into(),
        host.into(),
        "surmount.path".into(),
        path.into(),
    ])
}

/// GNOME/GVFS schema Nautilus uses to remember AFP (and other) network passwords.
pub const GNOME_NETWORK_PASSWORD_SCHEMA: &str = "org.gnome.keyring.NetworkPassword";

/// Default AFP user for MailPlus (operator workstation).
pub const DEFAULT_AFP_USER: &str = "hunter";

/// True when `host` is a dotted-quad IPv4 (octets 0-255).
pub fn is_ipv4_dotted_quad(host: &str) -> bool {
    let mut parts = host.split('.');
    let mut n = 0u8;
    for part in parts.by_ref() {
        n = n.saturating_add(1);
        if n > 4 {
            return false;
        }
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        if part.parse::<u8>().is_err() {
            return false;
        }
    }
    n == 4
}

/// True when `host` is an IPv4 or IPv6 literal (optional `[brackets]`).
pub fn is_ip_literal(host: &str) -> bool {
    let s = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    s.parse::<std::net::IpAddr>().is_ok()
}

/// Hostname or IPv4 used in an AFP URI (not a Secret Service host id).
pub fn validate_afp_reach_host(host: &str) -> Result<()> {
    if host.is_empty() {
        bail!("AFP host is empty");
    }
    if host.contains('/') || host.contains('@') || host.contains(char::is_whitespace) {
        bail!("AFP host must be a hostname or IPv4, not a URI");
    }
    if host.chars().all(|c| c.is_ascii_digit() || c == '.') {
        if !is_ipv4_dotted_quad(host) {
            bail!("AFP host IPv4 is not a valid dotted quad: {host}");
        }
        return Ok(());
    }
    Ok(())
}

/// Host from `afp://[user@]host/share` (no password).
pub fn afp_uri_host(uri: &str) -> Result<String> {
    let rest = match uri.strip_prefix("afp://") {
        Some(r) => r,
        None => bail!("AFP URI must start with afp://"),
    };
    let after_auth = match rest.split_once('@') {
        Some((_, hostpart)) => hostpart,
        None => rest,
    };
    let host = after_auth
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    validate_afp_reach_host(host)?;
    Ok(host.to_string())
}

/// Laptop-private map `HOST_ID=afp-host`. Missing file is not an error.
pub fn read_afp_host_hint(path: &Path, host_id: &str) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(path).with_context(|| format!("read AFP hint {}", path.display()))?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((k, v)) = trimmed.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.split('#').next().unwrap_or(v).trim();
        if k == host_id && !v.is_empty() {
            validate_afp_reach_host(v)?;
            return Ok(Some(v.to_string()));
        }
    }
    Ok(None)
}

/// Default laptop-private AFP host hint (never git).
pub fn default_afp_hint_path() -> PathBuf {
    match std::env::var("SURMOUNT_AFP_HINT_FILE") {
        Ok(p) if !p.is_empty() => return PathBuf::from(p),
        _ => {}
    }
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|h| PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("surmount").join("diskstation-afp-hosts")
}

/// AFP hosts to remember in GNOME NetworkPassword (Nautilus / raw gio).
///
/// Canonical `server` is the Secret Service host id (`DS1513` / `DS3018xs`)
/// and `<id>.local`. `--afp-host`, URI host, and the laptop hint may still be
/// IPv4/IPv6 for *reachability* (mount uses them), but those literals are never
/// stored as NetworkPassword `server` (LAN IPv4 can be dynamic).
pub fn collect_afp_network_servers(
    secret_host: &str,
    afp_host: Option<&str>,
    uri: Option<&str>,
    hint_file: Option<&Path>,
) -> Result<Vec<String>> {
    validate_synology_afp_host(secret_host)?;
    if let Some(h) = afp_host.map(str::trim).filter(|s| !s.is_empty()) {
        validate_afp_reach_host(h)?;
    }
    if let Some(u) = uri.map(str::trim).filter(|s| !s.is_empty()) {
        let _ = afp_uri_host(u)?;
    }
    if let Some(path) = hint_file {
        let _ = read_afp_host_hint(path, secret_host)?;
    }
    Ok(vec![
        secret_host.to_string(),
        format!("{secret_host}.local"),
    ])
}

/// NetworkPassword `server` must be a canonical host id or `<id>.local`.
pub fn validate_afp_network_password_server(server: &str) -> Result<()> {
    if is_ip_literal(server) {
        bail!(
            "NetworkPassword server must be the host id (DS1513 / DS3018xs) or <id>.local, \
             never an IP (got {server:?})"
        );
    }
    let ok = synology_afp_canonical_hosts()
        .iter()
        .any(|id| server == *id || server == format!("{id}.local"));
    if !ok {
        bail!("NetworkPassword server must be DS1513, DS3018xs, or <id>.local (got {server:?})");
    }
    Ok(())
}

/// `secret-tool store` argv for GNOME NetworkPassword (password on stdin).
pub fn network_password_store_args(server: &str, user: &str) -> Result<Vec<String>> {
    validate_afp_network_password_server(server)?;
    let user = user.trim();
    if user.is_empty() {
        bail!("AFP user is empty");
    }
    if user.contains(char::is_whitespace) {
        bail!("AFP user must be a single token");
    }
    Ok(vec![
        "store".into(),
        format!("--label=Password for {user} on {server}"),
        "xdg:schema".into(),
        GNOME_NETWORK_PASSWORD_SCHEMA.into(),
        "protocol".into(),
        "afp".into(),
        "server".into(),
        server.into(),
        "user".into(),
        user.into(),
    ])
}

/// After a successful secret store, wipe paste buffers (best-effort).
///
/// Empty stdin to `xclip -selection clipboard`, `xclip -selection primary`,
/// and `pbcopy` when those binaries exist. `wl-copy --clear` when present.
/// Never pipes a secret. Missing tools do not fail the store.
pub fn wipe_paste_buffers() {
    wipe_xclip_selection("clipboard");
    wipe_xclip_selection("primary");
    wipe_empty_stdin("pbcopy", &[]);
    let _ = Command::new("wl-copy")
        .arg("--clear")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn wipe_xclip_selection(selection: &str) {
    wipe_empty_stdin("xclip", &["-selection", selection]);
}

fn wipe_empty_stdin(bin: &str, args: &[&str]) {
    let mut child = match Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"");
    }
    let _ = child.wait();
}

/// Intro banner for DiskStation AFP LAN password intake (no-echo paste).
pub fn synology_afp_intro_banner(host: &str) -> String {
    format!(
        "\n\
=== Surmount DiskStation AFP password intake ===\n\
Paste the LAN password for AFP on logical host \"{host}\" \
(must be DS1513 or DS3018xs).\n\
Input is hidden. The value is not printed afterward.\n\
\n\
This stays in laptop GNOME Secret Service only. secrets-install-host refuses\n\
this kind (never copy the NAS password to the mail host).\n\
--generate is refused: inventing a random string is not the DiskStation password.\n\
Passwords may differ per NAS. Store one item per host id.\n\
\n\
Lookup later (never put the password on argv):\n\
  secret-tool lookup surmount.kind synology-afp surmount.host {host}\n\
Also stores GNOME NetworkPassword so Nautilus / raw gio remember.\n\
Mount: just diskstation-afp-mount -- --host {host}\n"
    )
}

/// Short explanation before the no-echo DiskStation AFP password prompt.
pub fn explain_synology_afp() -> &'static str {
    "Paste the DiskStation LAN password used for AFP (single line). \
     Not a new random string. Input is hidden. Laptop Secret Service only."
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn empty_secret_fail_closed() {
        let err = require_nonempty_secret("").unwrap_err();
        assert!(err.to_string().contains("empty"));
        let err = require_nonempty_secret("   \n").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn write_staging_refuses_empty_secret() {
        let dir = tempfile_dir();
        let attrs = build_attributes(
            Kind::StalwartToken,
            "mail-lab",
            Kind::StalwartToken.default_path(),
        )
        .unwrap();
        let err = write_staging_item(&dir, "stalwart-token", &attrs, "").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn staging_write_shape_namecheap() {
        let dir = tempfile_dir();
        let fields = NamecheapFields {
            api_user: "labuser".into(),
            api_key: concat!("SYNTHETIC-KEY-", "not-real-9f3c").into(),
            client_ip: "203.0.113.10".into(),
            sld: "example".into(),
            tld: "test".into(),
            user_name: Some("labuser".into()),
        };
        let body = build_namecheap_body(&fields).unwrap();
        assert!(body.contains("ApiUser=labuser\n"));
        assert!(body.contains("ApiKey=SYNTHETIC-KEY-not-real-9f3c\n"));
        assert!(body.contains("ClientIp=203.0.113.10\n"));
        assert!(body.contains("SLD=example\n"));
        assert!(body.contains("TLD=test\n"));
        assert!(body.contains("UserName=labuser\n"));

        let attrs = build_attributes(
            Kind::NamecheapApi,
            "mail-lab",
            Kind::NamecheapApi.default_path(),
        )
        .unwrap();
        let w = write_staging_item(&dir, "namecheap-api", &attrs, &body).unwrap();

        assert!(w.attributes_path.is_file());
        assert!(w.secret_path.is_file());
        assert!(
            !w.secret_path
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink()
        );

        let attr_txt = fs::read_to_string(&w.attributes_path).unwrap();
        assert!(attr_txt.contains("surmount.kind=namecheap-api\n"));
        assert!(attr_txt.contains("surmount.host=mail-lab\n"));
        assert!(attr_txt.contains("surmount.path=/var/lib/surmount/secrets/acme/namecheap.env\n"));

        let secret_txt = fs::read_to_string(&w.secret_path).unwrap();
        assert_eq!(secret_txt, body);

        let mode = fs::metadata(&w.secret_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "secret mode want 0600 got {mode:o}");

        let attr_mode = fs::metadata(&w.attributes_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(attr_mode, 0o600);
    }

    #[test]
    fn vaultwarden_admin_wraps_token() {
        let body = build_vaultwarden_admin_body("synth-token-abc").unwrap();
        assert_eq!(body, "ADMIN_TOKEN=synth-token-abc\n");
        let err = build_vaultwarden_admin_body("").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn session_secret_generate_hex_len() {
        let s = generate_session_secret_hex().unwrap();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
        let built = build_single_line_secret(&s).unwrap();
        assert_eq!(built, s);
    }

    #[test]
    fn default_paths_match_install_bridge() {
        assert_eq!(
            Kind::NamecheapApi.default_path(),
            "/var/lib/surmount/secrets/acme/namecheap.env"
        );
        assert_eq!(
            Kind::ShcApi.default_path(),
            "/var/lib/surmount/secrets/rdns/shc.env"
        );
        assert_eq!(
            Kind::StalwartToken.default_path(),
            "/var/lib/surmount/secrets/ui/stalwart-api-token"
        );
        assert_eq!(
            Kind::SessionSecret.default_path(),
            "/var/lib/surmount/secrets/ui/session-secret"
        );
        assert_eq!(
            Kind::VaultwardenAdmin.default_path(),
            "/var/lib/surmount/secrets/vaultwarden/admin.env"
        );
        assert_eq!(Kind::SynologyAfp.default_path(), "");
        assert!(Kind::SynologyAfp.is_laptop_only());
        assert!(!Kind::StalwartToken.is_laptop_only());
    }

    #[test]
    fn staging_write_shape_shc() {
        let dir = tempfile_dir();
        let fields = ShcFields {
            api_key: concat!("shc_live_SYNTHETIC-", "not-real-aa11bb22").into(),
            api_base: SHC_DEFAULT_API_BASE.into(),
            service_id: Some("12345".into()),
        };
        let body = build_shc_body(&fields).unwrap();
        assert!(body.contains("ApiKey=shc_live_SYNTHETIC-not-real-aa11bb22\n"));
        assert!(body.contains(&format!("ApiBase={SHC_DEFAULT_API_BASE}\n")));
        assert!(body.contains("ServiceId=12345\n"));

        let attrs =
            build_attributes(Kind::ShcApi, "surmount-1", Kind::ShcApi.default_path()).unwrap();
        let w = write_staging_item(&dir, "shc-api", &attrs, &body).unwrap();

        let attr_txt = fs::read_to_string(&w.attributes_path).unwrap();
        assert!(attr_txt.contains("surmount.kind=shc-api\n"));
        assert!(attr_txt.contains("surmount.host=surmount-1\n"));
        assert!(attr_txt.contains("surmount.path=/var/lib/surmount/secrets/rdns/shc.env\n"));

        let secret_txt = fs::read_to_string(&w.secret_path).unwrap();
        assert_eq!(secret_txt, body);

        let mode = fs::metadata(&w.secret_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn shc_body_defaults_and_validates() {
        let body = build_shc_body(&ShcFields {
            api_key: concat!("shc_live", "_x").into(),
            api_base: "".into(),
            service_id: None,
        })
        .unwrap();
        assert!(body.contains(&format!("ApiBase={SHC_DEFAULT_API_BASE}\n")));
        assert!(!body.contains("ServiceId="));

        // Trailing slash stripped.
        let body = build_shc_body(&ShcFields {
            api_key: concat!("shc_live", "_x").into(),
            api_base: format!("{SHC_DEFAULT_API_BASE}/"),
            service_id: Some("9".into()),
        })
        .unwrap();
        assert!(body.contains(&format!("ApiBase={SHC_DEFAULT_API_BASE}\n")));
        assert!(body.contains("ServiceId=9\n"));

        assert!(
            build_shc_body(&ShcFields {
                api_key: "".into(),
                api_base: SHC_DEFAULT_API_BASE.into(),
                service_id: None,
            })
            .is_err()
        );
        assert!(
            build_shc_body(&ShcFields {
                api_key: "k".into(),
                api_base: "not-a-url".into(),
                service_id: None,
            })
            .is_err()
        );
        assert!(
            build_shc_body(&ShcFields {
                api_key: "k".into(),
                api_base: SHC_DEFAULT_API_BASE.into(),
                service_id: Some("abc".into()),
            })
            .is_err()
        );
        assert!(
            build_shc_body(&ShcFields {
                api_key: "k".into(),
                api_base: SHC_DEFAULT_API_BASE.into(),
                service_id: Some("0".into()),
            })
            .is_err()
        );
    }

    #[test]
    fn host_and_path_validation() {
        assert!(validate_host_id("mail-lab").is_ok());
        assert!(validate_host_id("").is_err());
        assert!(validate_host_id("-evil").is_err());
        assert!(validate_host_path("/var/lib/surmount/secrets/ui/session-secret").is_ok());
        assert!(validate_host_path("/run/surmount-secrets/ui/session-secret").is_ok());
        assert!(validate_host_path("/etc/passwd").is_err());
        assert!(validate_host_path("/run/surmount-secrets/../etc/passwd").is_err());
        assert!(validate_host_path("relative").is_err());
    }

    #[test]
    fn secret_tool_args_shape() {
        let args = secret_tool_store_args(
            Kind::SessionSecret,
            "mail-lab",
            Kind::SessionSecret.default_path(),
        )
        .unwrap();
        assert_eq!(args[0], "store");
        assert!(args.iter().any(|a| a == "surmount.kind"));
        assert!(args.iter().any(|a| a == "session-secret"));
        assert!(args.iter().any(|a| a == "surmount.host"));
        assert!(args.iter().any(|a| a == "mail-lab"));
    }

    #[test]
    fn synology_afp_kind_parse_and_label() {
        let k = Kind::parse("synology-afp").expect("synology-afp must be an allowlisted kind");
        assert_eq!(k.as_str(), "synology-afp");
        assert!(
            k.is_laptop_only(),
            "DiskStation LAN password stays on the laptop"
        );
        let args = secret_tool_store_args(k, "DS1513", "").unwrap();
        assert_eq!(args[0], "store");
        assert!(
            args.iter()
                .any(|a| a == "--label=surmount synology-afp DS1513"),
            "label must be surmount synology-afp DS1513, got {args:?}"
        );
        assert!(args.iter().any(|a| a == "surmount.kind"));
        assert!(args.iter().any(|a| a == "synology-afp"));
        assert!(args.iter().any(|a| a == "surmount.host"));
        assert!(args.iter().any(|a| a == "DS1513"));
        assert!(
            !args.iter().any(|a| a == "surmount.path"),
            "laptop-only store must omit surmount.path (lookup is kind+host only)"
        );
    }

    #[test]
    fn synology_afp_attributes_omit_host_path() {
        let k = Kind::parse("synology-afp").unwrap();
        let attrs = build_attributes(k, "DS1513", "").unwrap();
        assert!(attrs.contains("surmount.kind=synology-afp\n"));
        assert!(attrs.contains("surmount.host=DS1513\n"));
        assert!(
            !attrs.contains("surmount.path="),
            "laptop-only kind has no Domain B path: {attrs}"
        );
    }

    #[test]
    fn synology_afp_host_ids_are_ds1513_and_ds3018xs() {
        assert!(
            validate_synology_afp_host("DS1513").is_ok(),
            "DS1513 is the 5-bay canonical host id"
        );
        assert!(
            validate_synology_afp_host("DS3018xs").is_ok(),
            "DS3018xs is the 6-bay canonical host id"
        );
        let err = validate_synology_afp_host("diskstation").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("DS1513") && msg.contains("DS3018xs"),
            "legacy diskstation must name both canonical ids, got {msg}"
        );
        assert!(validate_synology_afp_host("DiskStation").is_err());
        assert!(validate_synology_afp_host("ds1513").is_err());

        let k = Kind::SynologyAfp;
        let args = secret_tool_store_args(k, "DS1513", "").unwrap();
        assert!(
            args.iter()
                .any(|a| a == "--label=surmount synology-afp DS1513"),
            "label must be surmount synology-afp DS1513, got {args:?}"
        );
        assert!(args.iter().any(|a| a == "DS1513"));
        let args = secret_tool_store_args(k, "DS3018xs", "").unwrap();
        assert!(
            args.iter()
                .any(|a| a == "--label=surmount synology-afp DS3018xs"),
            "label must be surmount synology-afp DS3018xs, got {args:?}"
        );
        assert!(secret_tool_store_args(k, "diskstation", "").is_err());

        let attrs = build_attributes(k, "DS1513", "").unwrap();
        assert!(attrs.contains("surmount.host=DS1513\n"));
        assert!(build_attributes(k, "diskstation", "").is_err());
    }

    #[test]
    fn network_password_store_args_shape_host_id() {
        let args = network_password_store_args("DS1513", "hunter").unwrap();
        assert_eq!(args[0], "store");
        assert!(
            args.iter()
                .any(|a| a == "--label=Password for hunter on DS1513")
        );
        assert!(args.iter().any(|a| a == "xdg:schema"));
        assert!(args.iter().any(|a| a == GNOME_NETWORK_PASSWORD_SCHEMA));
        assert!(args.windows(2).any(|w| w[0] == "protocol" && w[1] == "afp"));
        assert!(
            args.windows(2)
                .any(|w| w[0] == "server" && w[1] == "DS1513")
        );
        assert!(args.windows(2).any(|w| w[0] == "user" && w[1] == "hunter"));
        assert!(
            args.iter()
                .all(|a| !a.contains("SYNTH") && !a.contains(concat!("password", "="))),
            "password must never appear on argv: {args:?}"
        );
        let mdns = network_password_store_args("DS3018xs.local", "hunter").unwrap();
        assert!(
            mdns.windows(2)
                .any(|w| w[0] == "server" && w[1] == "DS3018xs.local")
        );
    }

    #[test]
    fn network_password_store_args_refuses_ip_literal_server() {
        let err = network_password_store_args("192.0.2.10", "hunter").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("host id") || msg.contains("DS1513") || msg.contains("never"),
            "refuse IPv4 as NetworkPassword server: {msg}"
        );
        assert!(network_password_store_args("2001:db8::10", "hunter").is_err());
    }

    #[test]
    fn collect_afp_network_servers_host_id_not_ipv4() {
        let got = collect_afp_network_servers("DS1513", Some("192.0.2.10"), None, None).unwrap();
        assert!(
            got.iter().any(|s| s == "DS1513"),
            "NetworkPassword server must be host id, got {got:?}"
        );
        assert!(got.iter().any(|s| s == "DS1513.local"), "got {got:?}");
        assert!(
            got.iter().all(|s| s != "192.0.2.10" && !s.contains(':')),
            "IPv4/IPv6 must not be NetworkPassword server: {got:?}"
        );
        let only = collect_afp_network_servers("DS3018xs", None, None, None).unwrap();
        assert!(only.iter().any(|s| s == "DS3018xs"), "got {only:?}");
        assert!(only.iter().any(|s| s == "DS3018xs.local"), "got {only:?}");
        let from_uri = collect_afp_network_servers(
            "DS1513",
            None,
            Some("afp://hunter@192.0.2.11/MailPlus"),
            None,
        )
        .unwrap();
        assert!(
            from_uri.iter().all(|s| s != "192.0.2.11"),
            "URI IPv4 must not be NetworkPassword server: {from_uri:?}"
        );
        assert!(from_uri.iter().any(|s| s == "DS1513"), "got {from_uri:?}");
        let v6 = collect_afp_network_servers("DS1513", Some("2001:db8::10"), None, None).unwrap();
        assert!(
            v6.iter().all(|s| s != "2001:db8::10"),
            "IPv6 must not be NetworkPassword server: {v6:?}"
        );
        assert!(v6.iter().any(|s| s == "DS1513"), "got {v6:?}");
    }

    #[test]
    fn collect_afp_network_servers_hint_file() {
        let dir = tempfile_dir();
        let hint = dir.join("diskstation-afp-hosts");
        fs::write(&hint, "DS1513=192.0.2.10\nDS3018xs=192.0.2.11\n").unwrap();
        let got = collect_afp_network_servers("DS1513", None, None, Some(&hint)).unwrap();
        assert!(
            got.iter().all(|s| s != "192.0.2.10"),
            "hint IPv4 must not be NetworkPassword server: {got:?}"
        );
        assert!(got.iter().any(|s| s == "DS1513"), "got {got:?}");
        assert!(got.iter().any(|s| s == "DS1513.local"), "got {got:?}");
    }

    #[test]
    fn namecheap_rejects_empty_api_key() {
        let fields = NamecheapFields {
            api_user: "u".into(),
            api_key: "".into(),
            client_ip: "203.0.113.1".into(),
            sld: "ex".into(),
            tld: "test".into(),
            user_name: None,
        };
        assert!(build_namecheap_body(&fields).is_err());
    }

    #[test]
    fn split_domain_simple_and_multipart() {
        assert_eq!(
            split_domain("example.com").unwrap(),
            ("example".into(), "com".into())
        );
        assert_eq!(
            split_domain("Example.COM.").unwrap(),
            ("example".into(), "com".into())
        );
        assert_eq!(
            split_domain("example.co.uk").unwrap(),
            ("example".into(), "co.uk".into())
        );
        assert_eq!(
            split_domain("mail.example.com").unwrap(),
            ("mail.example".into(), "com".into())
        );
        assert_eq!(
            split_domain("shop.example.co.uk").unwrap(),
            ("shop.example".into(), "co.uk".into())
        );
        assert!(split_domain("").is_err());
        assert!(split_domain("nodot").is_err());
        assert!(split_domain("https://example.com").is_err());
    }

    #[test]
    fn resolve_sld_tld_prefers_explicit_pair() {
        let got = resolve_sld_tld(Some("ignored.com"), Some("apex"), Some("test")).unwrap();
        assert_eq!(got, Some(("apex".into(), "test".into())));
        let got = resolve_sld_tld(Some("example.com"), None, None).unwrap();
        assert_eq!(got, Some(("example".into(), "com".into())));
        assert!(resolve_sld_tld(None, None, None).unwrap().is_none());
        assert!(resolve_sld_tld(None, Some("only-sld"), None).is_err());
        assert!(resolve_sld_tld(Some("example.com"), Some("x"), None).is_err());
    }

    #[test]
    fn field_help_is_plain_english() {
        let host = explain_host_id();
        assert!(host.contains("NOT your Namecheap"));
        assert!(host.contains("surmount-1") || host.contains("inventory"));
        assert!(!host.contains('—'));
        assert!(!host.contains('–'));

        assert!(explain_api_user().contains("account username"));
        assert!(
            explain_user_name().contains("sub-user") || explain_user_name().contains("ApiUser")
        );
        assert!(explain_domain().contains("example.com"));
        assert!(explain_domain().contains("SLD=example"));
        assert!(explain_api_key().contains("API Access"));
        assert!(explain_client_ip().contains("public IP"));
        assert!(explain_client_ip().contains("not your domain"));

        let banner = namecheap_intro_banner("mail-lab");
        assert!(banner.contains("mail-lab"));
        assert!(banner.contains("not echoed") || banner.contains("not printed"));
        assert!(!banner.contains('—'));
        assert!(!banner.contains('–'));

        let st = stalwart_token_intro_banner("surmount-1");
        assert!(st.contains("surmount-1"));
        assert!(st.contains("already accepts") || st.contains("engine"));
        assert!(st.contains("NOT invent") || st.contains("Do NOT invent"));
        assert!(!st.contains('—'));
        assert!(!st.contains('–'));
        assert!(explain_stalwart_token().contains("already accepts"));

        let steps =
            next_steps_after_stalwart_token(Path::new("/tmp/surmount-staging-synth"), "surmount-1");
        assert!(steps.contains("secrets-install-host"));
        assert!(steps.contains("stalwart-token"));
        assert!(steps.contains("free-stalwart-public-443"));
        assert!(steps.contains("/var/lib/surmount/secrets/ui/stalwart-api-token"));
        assert!(steps.contains("add-stalwart-token"));
        assert!(!steps.contains('—'));
        // No synthetic secret-looking dump.
        assert!(!steps.contains(concat!("password", "=")));
        assert!(!steps.contains(concat!("TOKEN", "=")));

        let shc_banner = shc_intro_banner("surmount-1");
        assert!(shc_banner.contains("surmount-1"));
        assert!(shc_banner.contains("SHC") || shc_banner.contains("Hybrid"));
        assert!(shc_banner.contains("not invent") || shc_banner.contains("NOT invent"));
        assert!(!shc_banner.contains('—'));
        assert!(explain_shc_api_key().contains("shc_live_"));
        assert!(explain_shc_api_base().contains("user-api"));
        assert!(
            explain_shc_service_id().contains("optional")
                || explain_shc_service_id().contains("Optional")
        );

        let shc_steps =
            next_steps_after_shc_api(Path::new("/tmp/surmount-staging-synth"), "surmount-1");
        assert!(shc_steps.contains("shc-api"));
        assert!(shc_steps.contains("rdns-shc"));
        assert!(shc_steps.contains("/var/lib/surmount/secrets/rdns/shc.env"));
        assert!(shc_steps.contains("mail.surmount.systems"));
        assert!(!shc_steps.contains('—'));
        assert!(!shc_steps.contains("shc_live_"));
    }

    fn tempfile_dir() -> PathBuf {
        let mut p = std::env::temp_dir();
        let unique = format!(
            "surmount-secrets-prompt-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        p.push(unique);
        fs::create_dir_all(&p).unwrap();
        p
    }
}
