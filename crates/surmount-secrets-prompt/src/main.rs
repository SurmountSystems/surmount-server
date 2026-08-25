//! Interactive (and batch) Domain A secret intake for Surmount operators.
//!
//! Prompts without echoing secrets (rpassword). Writes private staging for
//! `secrets-install-host --from-staging`. Optional `secret-tool store`.
//! Never prints secret values after entry.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use surmount_secrets_prompt::{
    Kind, NamecheapFields, SHC_DEFAULT_API_BASE, ShcFields, build_attributes, build_namecheap_body,
    build_shc_body, build_single_line_secret, build_vaultwarden_admin_body,
    collect_afp_network_servers, default_afp_hint_path, default_staging_dir, explain_api_key,
    explain_api_user, explain_client_ip, explain_domain, explain_host_id, explain_shc_api_base,
    explain_shc_api_key, explain_shc_service_id, explain_stalwart_token, explain_synology_afp,
    explain_user_name, generate_session_secret_hex, generate_token_hex, namecheap_intro_banner,
    network_password_store_args, next_steps_after_shc_api, next_steps_after_stalwart_token,
    prompt_label_api_key, prompt_label_api_user, prompt_label_client_ip, prompt_label_domain,
    prompt_label_host_id, prompt_label_shc_api_base, prompt_label_shc_api_key,
    prompt_label_shc_service_id, prompt_label_user_name, read_secret_file, require_nonempty_secret,
    resolve_sld_tld, secret_tool_store_args, shc_intro_banner, split_domain,
    stalwart_token_intro_banner, synology_afp_intro_banner, validate_host_id,
    validate_synology_afp_host, wipe_paste_buffers, write_staging_item,
};

#[cfg(unix)]
use surmount_secrets_prompt::read_secret_from_fd;

#[derive(Debug, Parser)]
#[command(
    name = "surmount-secrets-prompt",
    about = "Interactive no-echo secret intake into Domain A staging (optional secret-tool)",
    long_about = "Collect deploy secret material without pasting into chat or shell history.\n\
\n\
Writes a private staging tree for nix run .#secrets-install-host. Secret values\n\
are not echoed as you type and are never printed after entry. Synthetic tests\n\
only in CI.\n\
\n\
namecheap-api (interactive):\n\
  Guided prompts in plain English for each Namecheap field. Prefer a full\n\
  domain like example.com (we split into SLD + TLD). ApiKey is hidden.\n\
\n\
shc-api (interactive):\n\
  Sovereign Hybrid Compute customer user-api key (operate scope preferred)\n\
  for PTR/rDNS automation. Paste shc_live_... from the portal API Keys page.\n\
  --generate is refused. Optional ApiBase and ServiceId.\n\
\n\
synology-afp (interactive):\n\
  DiskStation AFP LAN password. Laptop GNOME Secret Service only. Paste no-echo.\n\
  --generate is refused. secrets-install-host refuses this kind (never copy to\n\
  the mail host). Host id must be DS1513 (5-bay) or DS3018xs (6-bay), not an IP.\n\
  Also stores GNOME NetworkPassword (protocol=afp) so Nautilus / raw gio remember.\n\
\n\
Logical host id (--host):\n\
  Surmount inventory name for the machine (for example surmount-1). Not your\n\
  Namecheap username, domain, or IP.\n\
\n\
Batch mode: pass field flags/env and --secret-file / --secret-fd / env for the\n\
secret. See --help on each subcommand."
)]
struct Cli {
    #[command(subcommand)]
    kind: KindCmd,

    /// Surmount inventory name for this host (for example surmount-1).
    /// Not a Namecheap field: not ApiUser, not the domain, not an IP.
    #[arg(long, global = true, env = "SURMOUNT_SECRETS_HOST_ID")]
    host: Option<String>,

    /// Staging root directory (default: $XDG_DATA_HOME/surmount/staging).
    #[arg(long, global = true, env = "SURMOUNT_SECRETS_STAGING")]
    staging: Option<PathBuf>,

    /// Override Domain B surmount.path (default: conventional path for kind).
    #[arg(long, global = true)]
    path: Option<String>,

    /// Staging item subdirectory name (default: kind string).
    #[arg(long, global = true)]
    item_id: Option<String>,

    /// Also store via `secret-tool store` (subprocess; fails clear if missing).
    #[arg(long, global = true)]
    secret_service: bool,

    /// Skip staging write (requires --secret-service).
    #[arg(long, global = true)]
    no_staging: bool,

    /// Non-interactive: fields from flags/env; secret from file/fd/env.
    #[arg(long, global = true)]
    batch: bool,

    /// Batch: read secret payload from this file.
    #[arg(long, global = true)]
    secret_file: Option<PathBuf>,

    /// Batch: read secret payload from this file descriptor number.
    #[arg(long, global = true)]
    secret_fd: Option<i32>,

    /// Generate a random secret (session-secret / vaultwarden-admin).
    #[arg(long, global = true)]
    generate: bool,
}

#[derive(Debug, Subcommand)]
enum KindCmd {
    /// Namecheap API env for DNS-01 / zone tools (guided interactive fields).
    ///
    /// Collects ApiUser, ApiKey (hidden), ClientIp, domain (or SLD+TLD), and
    /// optional UserName. Run intake on your laptop. ClientIp is the public IP
    /// of the machine that will call Namecheap (VPS for live ACME; laptop for
    /// local zone tools), not where you type the secret. Whitelist that caller
    /// IP under Namecheap Profile -> Tools -> API Access.
    #[command(
        name = "namecheap-api",
        long_about = "Namecheap API env (KEY=value body for namecheap.env).\n\
\n\
Interactive mode explains each field in plain English. Prefer --domain\n\
example.com (or type a full domain when prompted); we split into SLD + TLD\n\
(example.com -> SLD=example, TLD=com; example.co.uk -> SLD=example, TLD=co.uk).\n\
You can still pass --sld and --tld separately for scripts.\n\
\n\
Fields:\n\
  ApiUser   Usually your Namecheap account username (login name for many accounts).\n\
  UserName  Optional API UserName; leave blank if same as ApiUser (sub-users differ).\n\
  Domain    Registered domain; split into SLD (second-level) + TLD (top-level).\n\
  ClientIp  Public IP of the API caller (laptop or VPS), not the domain.\n\
  ApiKey    From Namecheap Profile -> Tools -> API Access (never echoed).\n\
\n\
Logical host id (--host) is Surmount inventory (for example surmount-1), not\n\
a Namecheap field."
    )]
    NamecheapApi {
        /// Namecheap ApiUser (usually your account username / login name).
        #[arg(long, env = "SURMOUNT_NC_API_USER")]
        api_user: Option<String>,
        /// Public IP of the machine that will call Namecheap (not the domain).
        /// For Domain B ACME hook env, use the VPS public IP.
        #[arg(long, env = "SURMOUNT_NC_CLIENT_IP")]
        client_ip: Option<String>,
        /// Full registered domain (for example example.com). Split into SLD+TLD.
        /// Alternative to passing --sld and --tld separately.
        #[arg(long, env = "SURMOUNT_NC_DOMAIN")]
        domain: Option<String>,
        /// Second-level domain (SLD). Prefer --domain example.com instead.
        #[arg(long, env = "SURMOUNT_NC_SLD")]
        sld: Option<String>,
        /// Top-level domain (TLD), for example com or co.uk. Prefer --domain.
        #[arg(long, env = "SURMOUNT_NC_TLD")]
        tld: Option<String>,
        /// Optional Namecheap API UserName (blank if same as ApiUser).
        #[arg(long, env = "SURMOUNT_NC_USER_NAME")]
        user_name: Option<String>,
    },
    /// SHC customer user-api key for PTR/rDNS (and operate-scoped customer API).
    ///
    /// Paste an operate-scoped key from the SHC portal API Keys page
    /// (`shc_live_...`). Never invent or --generate. Optional ApiBase and
    /// ServiceId. Default Domain B path: /var/lib/surmount/secrets/rdns/shc.env
    #[command(
        name = "shc-api",
        long_about = "Sovereign Hybrid Compute (SHC) customer user-api credentials.\n\
\n\
Writes a KEY=value body (ApiKey, ApiBase, optional ServiceId) for rDNS tools\n\
and other operate-scoped customer API calls. Blesta is the portal brand; the\n\
product kind id is shc-api.\n\
\n\
Interactive mode explains each field. ApiKey is hidden. Prefer operate scope\n\
(cannot spend). Create the key in the SHC portal under API Keys. Do NOT invent\n\
a key; --generate is refused.\n\
\n\
Fields:\n\
  ApiKey     Bearer key shc_live_... from the portal (never echoed).\n\
  ApiBase    Default https://blesta.sovereignhybridcompute.com/user-api/v2\n\
  ServiceId  Optional Blesta VM service id (integer); leave blank to list later.\n\
\n\
Durable Domain B path (default): /var/lib/surmount/secrets/rdns/shc.env\n\
\n\
Operator flow:\n\
  just secrets-prompt -- shc-api --host surmount-1\n\
  just secrets-install-host -- --from-staging ... --require-kind shc-api\n\
  just rdns-shc -- --list\n\
  just rdns-shc -- --live --hostname mail.surmount.systems --ip YOUR_VPS_IP\n\
\n\
Docs: docs/DNS.md (PTR/rDNS via SHC), docs/SECRETS.md, docs/OPS.md\n\
KB: https://blesta.sovereignhybridcompute.com/plugin/support_manager/knowledgebase/view/16/the-customer-api-and-mcp/\n\
OpenAPI: https://blesta.sovereignhybridcompute.com/user-api/openapi.json"
    )]
    ShcApi {
        /// SHC user-api base URL without trailing slash (default: production v2).
        #[arg(long, env = "SURMOUNT_SHC_API_BASE")]
        api_base: Option<String>,
        /// Optional Blesta service id for the VM (digits only).
        #[arg(long, env = "SURMOUNT_SHC_SERVICE_ID")]
        service_id: Option<String>,
    },
    /// Stalwart admin / management API token (single line, no-echo paste).
    ///
    /// Paste a credential Stalwart already accepts. Not random generate.
    /// Prefer: just add-stalwart-token (defaults host, prints next steps).
    #[command(
        name = "stalwart-token",
        long_about = "Stalwart admin / management API token for free-443 and the UI.\n\
\n\
Paste a credential the engine already accepts (first-boot admin password or an\n\
API token Stalwart already knows). Input is hidden; the value is never printed\n\
after entry. Writes Domain A private staging for secrets-install-host.\n\
\n\
--generate is refused for this kind: inventing a random string is not engine\n\
registration. Live free-443 HTTP 401 means Domain B has a wrong/unknown value.\n\
\n\
Durable Domain B path (default): /var/lib/surmount/secrets/ui/stalwart-api-token\n\
\n\
One-command operator flow (prompt + optional install):\n\
  just add-stalwart-token -- --host surmount-1\n\
  just add-stalwart-token -- --host surmount-1 --target root@YOUR_HOST\n\
\n\
Logical host id (--host) is Surmount inventory (for example surmount-1)."
    )]
    StalwartToken,
    /// UI session HMAC secret (generate or paste no-echo).
    #[command(name = "session-secret")]
    SessionSecret,
    /// Vaultwarden ADMIN_TOKEN EnvironmentFile line.
    #[command(name = "vaultwarden-admin")]
    VaultwardenAdmin,
    /// DiskStation AFP LAN password (laptop Secret Service only).
    ///
    /// Paste the LAN password used for AFP. --generate is refused.
    /// Never installed to the mail host.
    #[command(
        name = "synology-afp",
        long_about = "DiskStation AFP LAN password for gio/gvfs mounts.\n\
\n\
Paste the password (no-echo). --generate is refused: inventing a random string\n\
is not the DiskStation LAN password.\n\
\n\
Laptop GNOME Secret Service only. Label: surmount synology-afp <host>\n\
Host id must be DS1513 or DS3018xs (one item per NAS). Lookup:\n\
  secret-tool lookup surmount.kind synology-afp surmount.host DS1513\n\
  secret-tool lookup surmount.kind synology-afp surmount.host DS3018xs\n\
\n\
Also stores org.gnome.keyring.NetworkPassword (protocol=afp, user=hunter,\n\
server=DS1513 or DS3018xs, plus <id>.local). --afp-host / --uri / hint may\n\
be IPv4 for the mount URI only; IPv4 is never stored as NetworkPassword\n\
server (LAN addresses can change). Optional laptop-private hint\n\
~/.local/share/surmount/diskstation-afp-hosts (never git).\n\
\n\
nix run .#secrets-install-host refuses this kind (never copy the NAS password\n\
to the mail host). Prefer --secret-service --no-staging.\n\
\n\
Operator flow:\n\
  just secrets-prompt -- synology-afp --host DS1513 --secret-service --no-staging\n\
  just secrets-prompt -- synology-afp --host DS1513 --afp-host <IPv4-or-hostname> \\\n\
    --secret-service --no-staging\n\
  just secrets-prompt -- synology-afp --host DS3018xs --secret-service --no-staging\n\
  just diskstation-afp-mount -- --host DS1513\n\
\n\
Docs: docs/SECRETS.md, docs/OPS.md, docs/MIGRATION.md"
    )]
    SynologyAfp {
        /// AFP URI host (hostname or IPv4). Not the Secret Service --host label.
        #[arg(long, env = "SURMOUNT_AFP_HOST")]
        afp_host: Option<String>,
        /// Full AFP URI; the host is used for GNOME NetworkPassword remember.
        #[arg(long)]
        uri: Option<String>,
        /// AFP user stored on the NetworkPassword item (default hunter).
        #[arg(long, env = "SURMOUNT_AFP_USER", default_value = "hunter")]
        afp_user: String,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("surmount-secrets-prompt: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let host = resolve_host(&cli)?;
    validate_host_id(&host)?;

    let kind = match &cli.kind {
        KindCmd::NamecheapApi { .. } => Kind::NamecheapApi,
        KindCmd::ShcApi { .. } => Kind::ShcApi,
        KindCmd::StalwartToken => Kind::StalwartToken,
        KindCmd::SessionSecret => Kind::SessionSecret,
        KindCmd::VaultwardenAdmin => Kind::VaultwardenAdmin,
        KindCmd::SynologyAfp { .. } => Kind::SynologyAfp,
    };

    if kind == Kind::SynologyAfp {
        validate_synology_afp_host(&host)?;
    }

    if kind.is_laptop_only() && cli.path.is_some() {
        bail!(
            "--path is refused for {}: laptop-only; no Domain B install path. \
             secrets-install-host refuses this kind.",
            kind.as_str()
        );
    }

    let path = cli
        .path
        .clone()
        .unwrap_or_else(|| kind.default_path().to_string());
    let item_id = cli
        .item_id
        .clone()
        .unwrap_or_else(|| kind.as_str().to_string());
    let staging = cli.staging.clone().unwrap_or_else(default_staging_dir);

    if cli.no_staging && !cli.secret_service {
        bail!("--no-staging requires --secret-service (nowhere else to put the secret)");
    }

    let secret_body = match &cli.kind {
        KindCmd::NamecheapApi {
            api_user,
            client_ip,
            domain,
            sld,
            tld,
            user_name,
        } => collect_namecheap(
            &cli, &host, api_user, client_ip, domain, sld, tld, user_name,
        )?,
        KindCmd::ShcApi {
            api_base,
            service_id,
        } => collect_shc(&cli, &host, api_base, service_id)?,
        KindCmd::StalwartToken => {
            if !cli.batch {
                eprint!("{}", stalwart_token_intro_banner(&host));
                eprintln!("{}", explain_stalwart_token());
            }
            let raw = collect_single_secret(
                &cli,
                "Stalwart admin token (engine-accepted)",
                false,
                "SURMOUNT_SECRET",
            )?;
            build_single_line_secret(&raw)?
        }
        KindCmd::SessionSecret => {
            let raw = collect_single_secret(&cli, "session secret", true, "SURMOUNT_SECRET")?;
            build_single_line_secret(&raw)?
        }
        KindCmd::VaultwardenAdmin => {
            let raw =
                collect_single_secret(&cli, "Vaultwarden admin token", true, "SURMOUNT_SECRET")?;
            build_vaultwarden_admin_body(&raw)?
        }
        KindCmd::SynologyAfp { .. } => {
            if cli.generate {
                bail!(
                    "--generate is refused for synology-afp: inventing a random value is not \
                     the DiskStation LAN password. Paste the AFP password (no-echo)."
                );
            }
            if !cli.secret_service {
                bail!(
                    "synology-afp requires --secret-service (laptop GNOME Secret Service only; \
                     never install to the mail host). Example: just secrets-prompt -- \
                     synology-afp --host DS1513 --secret-service --no-staging"
                );
            }
            if !cli.batch {
                eprint!("{}", synology_afp_intro_banner(&host));
                eprintln!("{}", explain_synology_afp());
            }
            let raw = collect_single_secret(
                &cli,
                "DiskStation AFP LAN password",
                false,
                "SURMOUNT_SECRET",
            )?;
            build_single_line_secret(&raw)?
        }
    };

    require_nonempty_secret(&secret_body)?;
    let attributes = build_attributes(kind, &host, &path)?;

    if !cli.no_staging {
        let w = write_staging_item(&staging, &item_id, &attributes, &secret_body)?;
        // Metadata only; never print secret body.
        eprintln!(
            "surmount-secrets-prompt: wrote staging item={} kind={} host={} path={} dir={}",
            item_id,
            kind.as_str(),
            host,
            path,
            w.item_dir.display()
        );
    }

    if cli.secret_service {
        store_secret_tool(kind, &host, &path, &secret_body)?;
        if kind.is_laptop_only() {
            eprintln!(
                "surmount-secrets-prompt: stored via secret-tool kind={} host={} (laptop-only; no Domain B path)",
                kind.as_str(),
                host
            );
            if kind == Kind::SynologyAfp {
                store_synology_afp_network_passwords(&cli, &host, &secret_body)?;
            }
        } else {
            eprintln!(
                "surmount-secrets-prompt: stored via secret-tool kind={} host={} path={}",
                kind.as_str(),
                host,
                path
            );
        }
    }

    // Operator next steps after Domain A write (stalwart-token needs install + free-443).
    if matches!(kind, Kind::StalwartToken) && !cli.no_staging && !cli.batch {
        eprint!("{}", next_steps_after_stalwart_token(&staging, &host));
    }
    if matches!(kind, Kind::ShcApi) && !cli.no_staging && !cli.batch {
        eprint!("{}", next_steps_after_shc_api(&staging, &host));
    }

    Ok(())
}

fn resolve_host(cli: &Cli) -> Result<String> {
    if let Some(h) = cli
        .host
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Ok(h.to_string());
    }
    if cli.batch {
        bail!(
            "--host is required in batch mode (Surmount inventory name, for example surmount-1). \
             It is not your Namecheap username, domain, or IP. \
             Set --host or SURMOUNT_SECRETS_HOST_ID."
        );
    }
    eprintln!("{}", explain_host_id());
    let host = prompt_visible(prompt_label_host_id())?;
    if host.trim().is_empty() {
        bail!("logical host id is required (Surmount inventory name, for example surmount-1)");
    }
    Ok(host.trim().to_string())
}

#[allow(clippy::too_many_arguments)]
fn collect_shc(
    cli: &Cli,
    host: &str,
    api_base: &Option<String>,
    service_id: &Option<String>,
) -> Result<String> {
    if cli.generate {
        bail!(
            "--generate is refused for shc-api: inventing a random value is not a portal key. \
             Paste an operate-scoped key from the SHC portal API Keys page (shc_live_...)."
        );
    }

    if !cli.batch {
        eprint!("{}", shc_intro_banner(host));
    }

    let api_base = if !cli.batch && flag_empty(api_base) {
        eprintln!("{}", explain_shc_api_base());
        let line = prompt_visible(prompt_label_shc_api_base())?;
        if line.trim().is_empty() {
            SHC_DEFAULT_API_BASE.to_string()
        } else {
            line
        }
    } else if let Some(b) = api_base
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        b.to_string()
    } else {
        SHC_DEFAULT_API_BASE.to_string()
    };

    let service_id = match service_id {
        Some(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ if cli.batch => std::env::var("SURMOUNT_SHC_SERVICE_ID")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        _ => {
            eprintln!("{}", explain_shc_service_id());
            let line = prompt_visible(prompt_label_shc_service_id())?;
            if line.trim().is_empty() {
                None
            } else {
                Some(line.trim().to_string())
            }
        }
    };

    let api_key = if cli.batch {
        read_batch_secret(cli, "SURMOUNT_SHC_API_KEY")?
    } else {
        eprintln!("{}", explain_shc_api_key());
        prompt_secret(prompt_label_shc_api_key())?
    };

    let fields = ShcFields {
        api_key,
        api_base,
        service_id,
    };
    build_shc_body(&fields)
}

#[allow(clippy::too_many_arguments)]
fn collect_namecheap(
    cli: &Cli,
    host: &str,
    api_user: &Option<String>,
    client_ip: &Option<String>,
    domain: &Option<String>,
    sld: &Option<String>,
    tld: &Option<String>,
    user_name: &Option<String>,
) -> Result<String> {
    if cli.generate {
        bail!("--generate is not valid for namecheap-api (paste ApiKey only)");
    }

    if !cli.batch {
        eprint!("{}", namecheap_intro_banner(host));
    }

    let api_user = if !cli.batch && flag_empty(api_user) {
        eprintln!("{}", explain_api_user());
        prompt_or_flag(cli.batch, prompt_label_api_user(), api_user, false)?
    } else {
        prompt_or_flag(cli.batch, "ApiUser", api_user, false)?
    };

    let (sld, tld) = resolve_domain_fields(cli, domain, sld, tld)?;

    let client_ip = if !cli.batch && flag_empty(client_ip) {
        eprintln!("{}", explain_client_ip());
        prompt_or_flag(cli.batch, prompt_label_client_ip(), client_ip, false)?
    } else {
        prompt_or_flag(cli.batch, "ClientIp", client_ip, false)?
    };

    let user_name = match user_name {
        Some(u) if !u.is_empty() => Some(u.clone()),
        _ if cli.batch => std::env::var("SURMOUNT_NC_USER_NAME")
            .ok()
            .filter(|s| !s.is_empty()),
        _ => {
            eprintln!("{}", explain_user_name());
            let line = prompt_visible(prompt_label_user_name())?;
            if line.trim().is_empty() {
                None
            } else {
                Some(line)
            }
        }
    };

    let api_key = if cli.batch {
        read_batch_secret(cli, "SURMOUNT_NC_API_KEY")?
    } else {
        eprintln!("{}", explain_api_key());
        prompt_secret(prompt_label_api_key())?
    };

    let fields = NamecheapFields {
        api_user,
        api_key,
        client_ip,
        sld,
        tld,
        user_name,
    };
    build_namecheap_body(&fields)
}

fn flag_empty(flag: &Option<String>) -> bool {
    flag.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true)
}

fn resolve_domain_fields(
    cli: &Cli,
    domain: &Option<String>,
    sld: &Option<String>,
    tld: &Option<String>,
) -> Result<(String, String)> {
    // Env fallback for domain when flag unset (clap env already fills domain).
    let domain_flag = domain.clone().filter(|s| !s.trim().is_empty());
    let sld_flag = sld.clone().filter(|s| !s.trim().is_empty());
    let tld_flag = tld.clone().filter(|s| !s.trim().is_empty());

    if let Some(pair) = resolve_sld_tld(
        domain_flag.as_deref(),
        sld_flag.as_deref(),
        tld_flag.as_deref(),
    )? {
        if !cli.batch {
            eprintln!(
                "Using SLD={} TLD={} (Namecheap second-level + top-level).",
                pair.0, pair.1
            );
        }
        return Ok(pair);
    }

    if cli.batch {
        bail!(
            "batch mode needs --domain example.com (or both --sld and --tld). \
             SLD is the name before the public suffix; TLD is com, co.uk, and so on."
        );
    }

    eprintln!("{}", explain_domain());
    let line = prompt_visible(prompt_label_domain())?;
    let line = line.trim();
    if line.is_empty() {
        bail!("domain is required (for example example.com)");
    }
    // If the operator typed only a single label, ask for TLD separately.
    if !line.contains('.') {
        eprintln!(
            "That looks like only the second-level name. Enter the top-level part next \
             (for example com, or co.uk)."
        );
        let tld_line = prompt_visible("TLD (for example com or co.uk)")?;
        let tld_line = tld_line.trim();
        if tld_line.is_empty() {
            bail!("TLD is required when domain has no dot");
        }
        eprintln!(
            "Using SLD={} TLD={} (Namecheap second-level + top-level).",
            line, tld_line
        );
        return Ok((line.to_string(), tld_line.to_string()));
    }
    let pair = split_domain(line)?;
    eprintln!(
        "Using SLD={} TLD={} (Namecheap second-level + top-level).",
        pair.0, pair.1
    );
    Ok(pair)
}

fn collect_single_secret(
    cli: &Cli,
    label: &str,
    allow_generate: bool,
    env_key: &str,
) -> Result<String> {
    if cli.generate {
        if !allow_generate {
            if matches!(cli.kind, KindCmd::StalwartToken) {
                bail!(
                    "--generate is refused for stalwart-token: inventing a random value is not \
                     Stalwart engine registration. Paste a credential the engine already accepts \
                     (first-boot admin or known API token). See: just add-stalwart-token --help"
                );
            }
            if matches!(cli.kind, KindCmd::ShcApi { .. }) {
                bail!(
                    "--generate is refused for shc-api: inventing a random value is not a portal \
                     key. Paste an operate-scoped key from the SHC portal API Keys page."
                );
            }
            if matches!(cli.kind, KindCmd::SynologyAfp { .. }) {
                bail!(
                    "--generate is refused for synology-afp: inventing a random value is not \
                     the DiskStation LAN password. Paste the AFP password (no-echo)."
                );
            }
            bail!("--generate is not supported for this kind");
        }
        if matches!(cli.kind, KindCmd::VaultwardenAdmin) {
            return generate_token_hex();
        }
        return generate_session_secret_hex();
    }

    if cli.batch {
        return read_batch_secret(cli, env_key);
    }

    // Interactive: offer generate for session-secret / vaultwarden when allow_generate.
    if allow_generate {
        eprint!("{label}: [g]enerate or [p]aste? [g/p]: ");
        let _ = io::stderr().flush();
        let mut line = String::new();
        io::stdin()
            .lock()
            .read_line(&mut line)
            .context("read generate/paste choice")?;
        let c = line.trim().to_ascii_lowercase();
        if c.is_empty() || c == "g" || c == "generate" {
            let v = if matches!(cli.kind, KindCmd::VaultwardenAdmin) {
                generate_token_hex()?
            } else {
                generate_session_secret_hex()?
            };
            eprintln!("surmount-secrets-prompt: generated {label} (not printed)");
            return Ok(v);
        }
    }

    prompt_secret(&format!("{label} (input hidden)"))
}

fn read_batch_secret(cli: &Cli, env_key: &str) -> Result<String> {
    if let Some(ref path) = cli.secret_file {
        return read_secret_file(path);
    }
    if let Some(fd) = cli.secret_fd {
        #[cfg(unix)]
        {
            return read_secret_from_fd(fd);
        }
        #[cfg(not(unix))]
        {
            let _ = fd;
            bail!("--secret-fd is only supported on Unix");
        }
    }
    if let Ok(v) = std::env::var(env_key) {
        require_nonempty_secret(&v)?;
        return Ok(v);
    }
    if env_key != "SURMOUNT_SECRET"
        && let Ok(v) = std::env::var("SURMOUNT_SECRET")
    {
        require_nonempty_secret(&v)?;
        return Ok(v);
    }
    bail!("batch mode needs --secret-file, --secret-fd, or env {env_key} (or SURMOUNT_SECRET)");
}

fn prompt_or_flag(batch: bool, label: &str, flag: &Option<String>, secret: bool) -> Result<String> {
    if let Some(v) = flag
        && !v.is_empty()
    {
        return Ok(v.clone());
    }
    if batch {
        bail!("batch mode missing required field for {label}");
    }
    if secret {
        prompt_secret(label)
    } else {
        prompt_visible(label)
    }
}

fn prompt_visible(label: &str) -> Result<String> {
    eprint!("{label}: ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .with_context(|| format!("read {label}"))?;
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn prompt_secret(label: &str) -> Result<String> {
    // rpassword reads from tty and does not echo.
    let v = rpassword::prompt_password(format!("{label}: ")).context("read secret (no-echo)")?;
    require_nonempty_secret(&v)?;
    Ok(v)
}

fn store_synology_afp_network_passwords(cli: &Cli, secret_host: &str, secret: &str) -> Result<()> {
    let (afp_host, uri, afp_user) = match &cli.kind {
        KindCmd::SynologyAfp {
            afp_host,
            uri,
            afp_user,
        } => (afp_host.as_deref(), uri.as_deref(), afp_user.as_str()),
        _ => bail!("internal error: NetworkPassword store is synology-afp only"),
    };
    let hint = default_afp_hint_path();
    let servers = collect_afp_network_servers(secret_host, afp_host, uri, Some(hint.as_path()))?;
    for server in servers {
        let args = network_password_store_args(&server, afp_user)?;
        store_secret_tool_args(&args, secret)?;
        eprintln!(
            "surmount-secrets-prompt: stored GNOME NetworkPassword protocol=afp user={} server={}",
            afp_user, server
        );
    }
    Ok(())
}

fn store_secret_tool(kind: Kind, host: &str, path: &str, secret: &str) -> Result<()> {
    let args = secret_tool_store_args(kind, host, path)?;
    store_secret_tool_args(&args, secret)
}

fn store_secret_tool_args(args: &[String], secret: &str) -> Result<()> {
    let bin = which_secret_tool()?;
    let mut child = Command::new(&bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn secret-tool (is libsecret tools installed?). Staging still available.")?;

    {
        let mut stdin = child.stdin.take().context("secret-tool stdin not piped")?;
        stdin
            .write_all(secret.as_bytes())
            .context("write secret to secret-tool stdin")?;
        // secret-tool often wants a trailing newline for password items.
        if !secret.ends_with('\n') {
            stdin.write_all(b"\n").ok();
        }
    }

    let output = child.wait_with_output().context("wait for secret-tool")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        // Do not echo possible secret leakage; stderr from secret-tool is usually safe.
        bail!(
            "secret-tool store failed (exit {:?}): {}. Staging write (if any) was kept.",
            output.status.code(),
            err.trim()
        );
    }
    // Best-effort: never fail the store because wipe tools are missing.
    wipe_paste_buffers();
    Ok(())
}

fn which_secret_tool() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("SURMOUNT_SECRET_TOOL") {
        let pb = PathBuf::from(&p);
        if pb.is_file() {
            return Ok(pb);
        }
        bail!("SURMOUNT_SECRET_TOOL set but not a file: {p}");
    }
    // PATH lookup without shell.
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let cand = PathBuf::from(dir).join("secret-tool");
            if cand.is_file() {
                return Ok(cand);
            }
        }
    }
    bail!(
        "secret-tool not found on PATH. Install libsecret tools (secret-tool), \
         or omit --secret-service and use staging only. \
         Example staging: secrets-install-host --from-staging ..."
    );
}
