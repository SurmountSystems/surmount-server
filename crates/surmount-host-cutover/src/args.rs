use std::env;
use std::path::PathBuf;

use crate::error::ToolError;
use crate::inventory::PATH_ACME_ACCOUNT;

pub const USAGE: &str = r#"Usage:
  surmount-host-cutover --help
  surmount-host-cutover --dry-run --staging DIR --host-id ID [options]
  surmount-host-cutover --live --staging DIR --host-id ID --target HOST \
    --host-local DIR [options]
  surmount-host-cutover --step STEP --staging DIR --host-id ID [options]
  surmount-host-cutover --inventory-only --staging DIR [options]
  surmount-host-cutover --emit-fragments-only --emit-fragments DIR [options]

Host cutover driver (A0 + laptop step ladder). Default mode is dry-run: validate
inventory, plan secrets-install and deploy-host, optionally emit private
host-local fragments, print smoke recipes. No remote mutation unless --live.

Named steps (--step; laptop-driven ladder; dry-run default):
  material   Inventory / optional generate non-CA material
  prep       Render host profile (ACME + public listen) into host-local
  install    secrets-install Domain B material (durable default; /run optional)
  free-443   Free Stalwart public :443 (on VPS via SSH when --target set)
  dns        Plan/apply A/AAAA from profile domains (laptop Namecheap; dry-run)
  deploy     deploy-host switch + loopback smoke
  prove      Checklist + optional public HTTPS prove (never claims without proof)
  le-prod    Explicit LE production directory flip + re-render (no auto-flip)
  all        Full dry compose of safe planning steps (honesty printer each)

Each step dry-run prints: what it will do, what it will NOT claim (no "HTTPS
done", no "LE issued", no "engine token registered"), and the next command.

Honesty: never claim live LE issued, mail token registered inside
the engine, or public HTTPS proven unless a real proof command succeeded.
This driver will NOT claim public HTTPS proven.

Required for inventory / cutover:
  --staging DIR          Private secrets staging (outside public git tree)
  --host-id ID           Logical host id (never a committed public IP)

Live cutover (--live) also requires:
  --target HOST          SSH target (user@host or host); must not start with '-'
  --host-local DIR       Private host-local overlay (SSH keys, hardware, etc.)

Modes:
  --dry-run              Explicit dry-run (default)
  --live                 Perform install + deploy (requires target + host-local)
  --inventory-only       Run material inventory only; exit its code
  --emit-fragments-only  Write fragments under --emit-fragments and exit

Profile / path:
  --with-vaultwarden     S7b profile: require VW admin token; emit enable
                         fragment; print VW smoke. Default is https-only.
  --acme-path            ACME path: PEMs not required in inventory
  --host-profile PATH    Non-secret private host profile (TOML-like).
  --free-443             After secrets-install, plan/run free-stalwart-public-443
  --generate-material    Generate missing non-CA material into staging
                         (session-secret, stalwart-token, vaultwarden-admin
                         when VW profile). Never generates CA/PEMs.
                         Never generates namecheap-api or PEMs.
  --require-namecheap    Require namecheap-api kind on install (ACME DNS-01).
  --skip-deploy          Plan/run secrets-install only (no deploy-host)
  --skip-secrets-install Plan/run deploy-host only (still validates inventory)
  --dest-root DIR        Hermetic secrets-install local dest (no SSH)
  -h, --help             Show this help.
"#;

fn flag_true(key: &str) -> bool {
    matches!(
        env::var(key).as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES") | Ok("on") | Ok("ON")
    )
}

#[derive(Debug, Clone)]
pub struct CutoverOpts {
    pub dry_run: bool,
    pub live: bool,
    pub inventory_only: bool,
    pub emit_only: bool,
    pub step: Option<String>,
    pub staging: Option<PathBuf>,
    pub host_id: Option<String>,
    pub target: Option<String>,
    pub host_local: Option<PathBuf>,
    pub with_vw: bool,
    pub acme_path: bool,
    pub dns_hook_path: Option<String>,
    pub acme_account_path: String,
    pub host_profile: Option<PathBuf>,
    pub emit_fragments: Option<PathBuf>,
    pub generate_material: bool,
    pub free_443: bool,
    pub skip_deploy: bool,
    pub skip_secrets: bool,
    pub dest_root: Option<PathBuf>,
    pub client_ip: Option<String>,
    pub require_namecheap: bool,
    pub dns_a: Option<String>,
    pub dns_aaaa: Option<String>,
    pub le_directory: Option<String>,
}

pub fn parse_args<I, S>(args: I) -> Result<Parse, ToolError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let rest: Vec<String> = iter.map(|s| s.as_ref().to_string()).collect();

    let mut o = CutoverOpts {
        dry_run: true,
        live: false,
        inventory_only: false,
        emit_only: false,
        step: env::var("SURMOUNT_CUTOVER_STEP")
            .ok()
            .filter(|s| !s.is_empty()),
        staging: env::var("SURMOUNT_SECRETS_STAGING")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        host_id: env::var("SURMOUNT_SECRETS_HOST_ID")
            .ok()
            .filter(|s| !s.is_empty()),
        target: env::var("SURMOUNT_SECRETS_TARGET")
            .ok()
            .or_else(|| env::var("SURMOUNT_DEPLOY_TARGET").ok())
            .filter(|s| !s.is_empty()),
        host_local: env::var("SURMOUNT_HOST_LOCAL_DIR")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        with_vw: flag_true("SURMOUNT_CUTOVER_WITH_VAULTWARDEN"),
        acme_path: flag_true("SURMOUNT_CUTOVER_ACME_PATH"),
        dns_hook_path: None,
        acme_account_path: PATH_ACME_ACCOUNT.to_string(),
        host_profile: env::var("SURMOUNT_CUTOVER_HOST_PROFILE")
            .ok()
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        emit_fragments: None,
        generate_material: false,
        free_443: false,
        skip_deploy: false,
        skip_secrets: false,
        dest_root: None,
        client_ip: None,
        require_namecheap: false,
        dns_a: None,
        dns_aaaa: None,
        le_directory: None,
    };

    let mut i = 0usize;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => return Ok(Parse::Help),
            "--dry-run" => {
                o.dry_run = true;
                o.live = false;
                i += 1;
            }
            "--live" => {
                o.live = true;
                o.dry_run = false;
                i += 1;
            }
            "--inventory-only" => {
                o.inventory_only = true;
                i += 1;
            }
            "--emit-fragments-only" => {
                o.emit_only = true;
                i += 1;
            }
            "--step" => o.step = Some(take(&rest, &mut i, "--step")?),
            "--staging" => o.staging = Some(PathBuf::from(take(&rest, &mut i, "--staging")?)),
            "--host-id" => o.host_id = Some(take(&rest, &mut i, "--host-id")?),
            "--target" => o.target = Some(take(&rest, &mut i, "--target")?),
            "--host-local" => {
                o.host_local = Some(PathBuf::from(take(&rest, &mut i, "--host-local")?))
            }
            "--with-vaultwarden" => {
                o.with_vw = true;
                i += 1;
            }
            "--acme-path" => {
                o.acme_path = true;
                i += 1;
            }
            "--dns-hook-path" => o.dns_hook_path = Some(take(&rest, &mut i, "--dns-hook-path")?),
            "--acme-account-path" => {
                o.acme_account_path = take(&rest, &mut i, "--acme-account-path")?;
            }
            "--host-profile" => {
                o.host_profile = Some(PathBuf::from(take(&rest, &mut i, "--host-profile")?));
            }
            "--emit-fragments" => {
                o.emit_fragments = Some(PathBuf::from(take(&rest, &mut i, "--emit-fragments")?));
            }
            "--generate-material" => {
                o.generate_material = true;
                i += 1;
            }
            "--free-443" => {
                o.free_443 = true;
                i += 1;
            }
            "--skip-deploy" => {
                o.skip_deploy = true;
                i += 1;
            }
            "--skip-secrets-install" => {
                o.skip_secrets = true;
                i += 1;
            }
            "--dest-root" => o.dest_root = Some(PathBuf::from(take(&rest, &mut i, "--dest-root")?)),
            "--client-ip" => o.client_ip = Some(take(&rest, &mut i, "--client-ip")?),
            "--require-namecheap" => {
                o.require_namecheap = true;
                i += 1;
            }
            "--dns-a" => o.dns_a = Some(take(&rest, &mut i, "--dns-a")?),
            "--dns-aaaa" => o.dns_aaaa = Some(take(&rest, &mut i, "--dns-aaaa")?),
            "--le-directory" => o.le_directory = Some(take(&rest, &mut i, "--le-directory")?),
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
    }
    Ok(Parse::Run(o))
}

pub enum Parse {
    Help,
    Run(CutoverOpts),
}

fn take(rest: &[String], i: &mut usize, flag: &str) -> Result<String, ToolError> {
    if *i + 1 >= rest.len() {
        return Err(ToolError::fail(format!("{flag} requires a value")));
    }
    let v = rest[*i + 1].clone();
    *i += 2;
    Ok(v)
}
