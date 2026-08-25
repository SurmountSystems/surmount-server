use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser};
use serde_json::Value;

use crate::api::{Transport, write_with_confirm};
use crate::cred::{Credentials, DEFAULT_HOSTNAME, resolve_cred_path, staging_secret_from_env};
use crate::die;
use crate::jsonutil::{
    TicketPriority, as_object, data_value, display_opt, json_string_field, parse_ipv4,
    rdns_clear_body, rdns_set_body, ticket_request_body,
};
use crate::mock::MockHttp;

const USAGE: &str = "\
Usage:
  surmount-shc [--dry-run|--live] --list
  surmount-shc [--dry-run|--live] --list-vms
  surmount-shc [--dry-run|--live] [--hostname FQDN] --ip IPV4
  surmount-shc [--dry-run|--live] --clear --ip IPV4
  surmount-shc --list-departments
  surmount-shc [--dry-run|--live] --open-ticket --department-id N \\
      --subject TEXT (--message TEXT | --message-file PATH) [--priority LEVEL]
  surmount-shc --help

Set or list reverse DNS (PTR), or open a support ticket, via the SHC
customer user-api. Default is dry-run (print plan / list; no write).
Pass --live to apply.

  --list               GET /vm/{serviceId}/rdns (eligible IPs + current ptr/pending)
  --list-vms           GET /vm (service ids when ServiceId not set in credentials)
  --list-departments   GET /support/departments (client-visible; SHC Team is id 1)
  --open-ticket        POST /support/tickets (department_id, subject, message)
  --department-id N    Support department (required for --open-ticket)
  --subject TEXT       Ticket subject (required for --open-ticket)
  --message TEXT       Ticket body (or use --message-file)
  --message-file PATH  Ticket body from a regular file
  --priority LEVEL     Optional: emergency, critical, high, medium, low
  --hostname FQDN      PTR hostname (default: mail.surmount.systems)
  --ip IPV4            Address to set or clear (required for set/clear)
  --service-id N       Blesta service id (else credentials ServiceId= or list-vms)
  --clear              DELETE PTR for --ip (confirm-gated when --live)
  --live               Apply POST/DELETE with confirmation dance
  --dry-run            Default; plan only

Credentials (KEY=value; never in git):
  /var/lib/surmount/secrets/rdns/shc.env
  Override: SURMOUNT_RDNS_SHC_ENV
  Domain A staging secret also accepted when Domain B file missing:
    $XDG_DATA_HOME/surmount/staging/shc-api/secret
    ~/.local/share/surmount/staging/shc-api/secret

Body fields:
  ApiKey=shc_live_...     (required; never printed)
  ApiBase=https://...     (optional; default user-api/v2)
  ServiceId=12345         (optional)

Safety:
  - Never print ApiKey
  - Prefer operate-scoped key (cannot spend)
  - FCrDNS: hostname must already A-resolve to the IP (API enforces 422)
  - Provider console rDNS remains valid
  - Namecheap is forward zone only (not this tool)

Hermetic self-test: cargo test -p surmount-shc (mock API; never live SHC as CI green)
Docs: docs/DNS.md, docs/OPS.md
";

#[derive(Debug, Parser)]
#[command(
    name = "rdns-shc",
    about = "SHC customer user-api reverse-DNS (PTR) and support tickets",
    long_about = USAGE,
    after_long_help = USAGE,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Apply POST/DELETE with confirmation dance.
    #[arg(long, action = ArgAction::SetTrue, overrides_with = "dry_run")]
    pub live: bool,

    /// Default; plan only.
    #[arg(long, action = ArgAction::SetTrue, overrides_with = "live")]
    pub dry_run: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub list: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub list_vms: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub list_departments: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub open_ticket: bool,

    #[arg(long, action = ArgAction::SetTrue)]
    pub clear: bool,

    /// PTR hostname (default: mail.surmount.systems).
    #[arg(long, default_value = DEFAULT_HOSTNAME)]
    pub hostname: String,

    /// Address to set or clear.
    #[arg(long)]
    pub ip: Option<String>,

    /// Blesta service id (else credentials ServiceId=).
    #[arg(long)]
    pub service_id: Option<String>,

    /// Support department (required for --open-ticket).
    #[arg(long)]
    pub department_id: Option<String>,

    /// Ticket subject (required for --open-ticket).
    #[arg(long)]
    pub subject: Option<String>,

    /// Ticket body (or use --message-file).
    #[arg(long)]
    pub message: Option<String>,

    /// Ticket body from a regular file.
    #[arg(long)]
    pub message_file: Option<PathBuf>,

    /// Optional: emergency, critical, high, medium, low.
    #[arg(long)]
    pub priority: Option<String>,
}

pub fn run(cli: Cli) -> Result<i32> {
    let mock_dir = std::env::var_os("SURMOUNT_RDNS_SHC_MOCK_DIR").map(PathBuf::from);
    let verbose = std::env::var("SURMOUNT_RDNS_SHC_VERBOSE").ok().as_deref() == Some("1");
    let env_api_base = std::env::var("SURMOUNT_RDNS_SHC_API_BASE").ok();
    let env_file = std::env::var_os("SURMOUNT_RDNS_SHC_ENV").map(PathBuf::from);
    let xdg = std::env::var("XDG_DATA_HOME").ok();
    let home = std::env::var("HOME").ok();
    let staging = staging_secret_from_env(xdg.as_deref(), home.as_deref());
    let default_exists = std::path::Path::new(crate::DEFAULT_CRED_PATH).is_file();
    let cred_path = resolve_cred_path(env_file.as_deref(), default_exists, staging.as_deref());

    let transport = if let Some(dir) = mock_dir.as_deref() {
        Transport::Mock(MockHttp::ensure(dir)?)
    } else {
        Transport::Live
    };

    let live = cli.live;

    if cli.list_departments {
        return cmd_list_departments(
            &cred_path,
            env_api_base.as_deref(),
            &cli,
            &transport,
            verbose,
        );
    }
    if cli.open_ticket {
        return cmd_open_ticket(
            &cred_path,
            env_api_base.as_deref(),
            &cli,
            &transport,
            live,
            verbose,
        );
    }
    if cli.list_vms {
        return cmd_list_vms(
            &cred_path,
            env_api_base.as_deref(),
            &cli,
            &transport,
            verbose,
        );
    }
    if cli.list {
        return cmd_list_rdns(
            &cred_path,
            env_api_base.as_deref(),
            &cli,
            &transport,
            verbose,
        );
    }
    if cli.clear {
        return cmd_clear(
            &cred_path,
            env_api_base.as_deref(),
            &cli,
            &transport,
            live,
            verbose,
        );
    }
    if cli.ip.is_some() {
        return cmd_set(
            &cred_path,
            env_api_base.as_deref(),
            &cli,
            &transport,
            live,
            verbose,
        );
    }

    let _ = writeln!(std::io::stderr(), "{USAGE}");
    Err(die(
        "specify --list, --list-vms, --list-departments, --open-ticket, --ip (set), or --clear --ip",
    ))
}

fn load(cred_path: &std::path::Path, env_api_base: Option<&str>, cli: &Cli) -> Result<Credentials> {
    Credentials::load(cred_path, env_api_base, cli.service_id.as_deref())
}

fn cmd_list_vms(
    cred_path: &std::path::Path,
    env_api_base: Option<&str>,
    cli: &Cli,
    transport: &Transport,
    verbose: bool,
) -> Result<i32> {
    let creds = load(cred_path, env_api_base, cli)?;
    let out = transport.request(&creds, "GET", "/vm", "", None, verbose)?;
    if out.code != 200 {
        return Err(die(format!("GET /vm failed HTTP {}", out.code)));
    }
    println!("rdns-shc: VMs (credentials path set; ApiKey not shown)");
    print_vms(&out.body);
    Ok(0)
}

fn cmd_list_rdns(
    cred_path: &std::path::Path,
    env_api_base: Option<&str>,
    cli: &Cli,
    transport: &Transport,
    verbose: bool,
) -> Result<i32> {
    let creds = load(cred_path, env_api_base, cli)?;
    let sid = creds.require_service_id()?;
    let path = format!("/vm/{sid}/rdns");
    let out = transport.request(&creds, "GET", &path, "", None, verbose)?;
    if out.code != 200 {
        return Err(die(format!("GET {path} failed HTTP {}", out.code)));
    }
    let mode = match transport {
        Transport::Mock(_) => "mock",
        Transport::Live => "live-api",
    };
    println!("rdns-shc: rDNS for service_id={sid} (mode={mode})");
    print_rdns_records(&out.body);
    Ok(0)
}

fn cmd_set(
    cred_path: &std::path::Path,
    env_api_base: Option<&str>,
    cli: &Cli,
    transport: &Transport,
    live: bool,
    verbose: bool,
) -> Result<i32> {
    let creds = load(cred_path, env_api_base, cli)?;
    let sid = creds.require_service_id()?.to_string();
    let ip = cli
        .ip
        .as_deref()
        .ok_or_else(|| die("--ip is required for set"))?;
    parse_ipv4(ip)?;
    if cli.hostname.is_empty() {
        return Err(die("--hostname is required for set"));
    }
    let body = rdns_set_body(ip, &cli.hostname);
    println!(
        "rdns-shc: plan set PTR service_id={sid} ip={ip} hostname={}",
        cli.hostname
    );
    if !live {
        println!("rdns-shc: dry-run only (no POST). Pass --live to apply (confirm dance).");
        println!(
            "rdns-shc: FCrDNS: ensure {} A-resolves to {ip} before --live.",
            cli.hostname
        );
        return Ok(0);
    }
    let path = format!("/vm/{sid}/rdns");
    let out = write_with_confirm(transport, &creds, "POST", &path, &body, verbose)?;
    println!(
        "rdns-shc: queued set PTR ip={ip} hostname={} HTTP {}",
        cli.hostname, out.code
    );
    print_write_fields(
        &out.body,
        &[
            "status",
            "job_id",
            "pending_public",
            "ip",
            "hostname",
            "service_id",
        ],
    );
    Ok(0)
}

fn cmd_clear(
    cred_path: &std::path::Path,
    env_api_base: Option<&str>,
    cli: &Cli,
    transport: &Transport,
    live: bool,
    verbose: bool,
) -> Result<i32> {
    let creds = load(cred_path, env_api_base, cli)?;
    let sid = creds.require_service_id()?.to_string();
    let ip = cli
        .ip
        .as_deref()
        .ok_or_else(|| die("--ip is required for --clear"))?;
    parse_ipv4(ip)?;
    let body = rdns_clear_body(ip);
    println!("rdns-shc: plan clear PTR service_id={sid} ip={ip}");
    if !live {
        println!("rdns-shc: dry-run only (no DELETE). Pass --live to apply (confirm dance).");
        return Ok(0);
    }
    let path = format!("/vm/{sid}/rdns");
    let out = write_with_confirm(transport, &creds, "DELETE", &path, &body, verbose)?;
    println!("rdns-shc: queued clear PTR ip={ip} HTTP {}", out.code);
    Ok(0)
}

fn cmd_list_departments(
    cred_path: &std::path::Path,
    env_api_base: Option<&str>,
    cli: &Cli,
    transport: &Transport,
    verbose: bool,
) -> Result<i32> {
    let creds = load(cred_path, env_api_base, cli)?;
    let out = transport.request(&creds, "GET", "/support/departments", "", None, verbose)?;
    if out.code != 200 {
        return Err(die(format!(
            "GET /support/departments failed HTTP {}",
            out.code
        )));
    }
    println!("rdns-shc: support departments (credentials path set; ApiKey not shown)");
    print_departments(&out.body);
    Ok(0)
}

fn cmd_open_ticket(
    cred_path: &std::path::Path,
    env_api_base: Option<&str>,
    cli: &Cli,
    transport: &Transport,
    live: bool,
    verbose: bool,
) -> Result<i32> {
    let creds = load(cred_path, env_api_base, cli)?;
    let dept = cli
        .department_id
        .as_deref()
        .ok_or_else(|| die("--department-id is required for --open-ticket"))?;
    if !dept.bytes().all(|b| b.is_ascii_digit()) || dept.is_empty() {
        return Err(die("--department-id must be digits"));
    }
    let subject = cli
        .subject
        .as_deref()
        .ok_or_else(|| die("--subject is required for --open-ticket"))?;
    let mut message = cli.message.clone().unwrap_or_default();
    if let Some(path) = cli.message_file.as_ref() {
        let meta = fs::symlink_metadata(path)
            .map_err(|_| die(format!("--message-file missing: {}", path.display())))?;
        if meta.file_type().is_symlink() {
            return Err(die(
                "--message-file must be a regular file (symlink refused)",
            ));
        }
        if !meta.is_file() {
            return Err(die(format!("--message-file missing: {}", path.display())));
        }
        message = fs::read_to_string(path)
            .map_err(|_| die(format!("--message-file unreadable: {}", path.display())))?;
    }
    if message.is_empty() {
        return Err(die(
            "--message or --message-file is required for --open-ticket",
        ));
    }
    let priority = match cli.priority.as_deref() {
        None | Some("") => None,
        Some(p) => Some(TicketPriority::parse(p)?),
    };
    let body = ticket_request_body(dept, subject, &message, priority)?;
    println!("rdns-shc: plan open ticket department_id={dept} subject={subject}");
    if !live {
        println!("rdns-shc: dry-run only (no POST). Pass --live to apply (confirm dance).");
        return Ok(0);
    }
    let out = write_with_confirm(
        transport,
        &creds,
        "POST",
        "/support/tickets",
        &body,
        verbose,
    )?;
    println!("rdns-shc: opened ticket HTTP {}", out.code);
    print_ticket(&out.body);
    Ok(0)
}

fn parse_root(body: &str) -> Option<Value> {
    serde_json::from_str(body).ok()
}

fn print_vms(body: &str) {
    let Some(root) = parse_root(body) else {
        println!("{}", truncate(body, 2000));
        return;
    };
    let data = data_value(&root);
    let rows: Vec<&Value> = match data {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![data],
        _ => {
            println!("{}", truncate(body, 2000));
            return;
        }
    };
    for row in rows {
        if as_object(row).is_none() {
            continue;
        }
        let sid = json_string_field(row, &["service_id", "id"]);
        let label = json_string_field(row, &["label", "hostname", "name"]);
        let status = json_string_field(row, &["status"]);
        println!("  service_id={sid} label={label} status={status}");
    }
}

fn print_rdns_records(body: &str) {
    let Some(root) = parse_root(body) else {
        println!("{}", truncate(body, 4000));
        return;
    };
    let data = data_value(&root);
    let recs = data.get("records").and_then(Value::as_array);
    let Some(recs) = recs else {
        println!(
            "{}",
            truncate(
                &serde_json::to_string_pretty(&root).unwrap_or_else(|_| body.to_string()),
                4000
            )
        );
        return;
    };
    for r in recs {
        let ip = json_string_field(r, &["ip"]);
        let ptr = r
            .get("ptr")
            .map(display_opt)
            .unwrap_or_else(|| "None".into());
        let pending = r
            .get("pending")
            .map(display_opt)
            .unwrap_or_else(|| "None".into());
        println!("  ip={ip} ptr={ptr} pending={pending}");
    }
}

fn print_departments(body: &str) {
    let Some(root) = parse_root(body) else {
        println!("{}", truncate(body, 2000));
        return;
    };
    let data = data_value(&root);
    let rows: Vec<&Value> = match data {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![data],
        _ => Vec::new(),
    };
    for row in rows {
        if as_object(row).is_none() {
            continue;
        }
        let did = json_string_field(row, &["id", "department_id"]);
        let name = json_string_field(row, &["name"]);
        let desc = json_string_field(row, &["description"]);
        println!("  department_id={did} name={name} description={desc}");
    }
}

fn print_write_fields(body: &str, keys: &[&str]) {
    let Some(root) = parse_root(body) else {
        return;
    };
    let data = data_value(&root);
    if as_object(data).is_none() {
        return;
    }
    for k in keys {
        if let Some(v) = data.get(*k) {
            if !v.is_null() {
                let shown = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                println!("  {k}={shown}");
            }
        }
    }
}

fn print_ticket(body: &str) {
    let Some(root) = parse_root(body) else {
        return;
    };
    let data = data_value(&root);
    let data = if as_object(data).is_some() {
        data
    } else {
        &root
    };
    let tid = json_string_field(data, &["ticket_id", "id", "code"]);
    let dept = json_string_field(data, &["department_id"]);
    let subj = json_string_field(data, &["subject"]);
    let status = json_string_field(data, &["status"]);
    if !tid.is_empty() {
        println!("  ticket_id={tid}");
    }
    if !dept.is_empty() {
        println!("  department_id={dept}");
    }
    if !subj.is_empty() {
        println!("  subject={subj}");
    }
    if !status.is_empty() {
        println!("  status={status}");
    }
}

fn truncate(s: &str, n: usize) -> &str {
    if s.len() <= n { s } else { &s[..n] }
}
