//! Just recipe bodies, second ten names.
//! fmt-write, test, clippy, dev, e2e, e2e-host, publish, deploy,
//! deploy-dry-run, deploy-host. Commands are the same programs the
//! justfile ran. e2e and e2e-host use the flake package output so the
//! recipe app does not call itself.

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::error::ToolError;
use crate::paths::discover_repo_root;
use crate::recipe_batch_01::{self, flake_system, run_in};

const BATCH: &[&str] = &[
    "fmt-write",
    "test",
    "clippy",
    "dev",
    "e2e",
    "e2e-host",
    "publish",
    "deploy",
    "deploy-dry-run",
    "deploy-host",
];

const DEPLOY_TARGET: &str = "root@surmount-1";

pub fn dispatch(args: &[String]) -> Option<Result<(), ToolError>> {
    let (name, rest) = recipe_batch_01::split_invocation(args)?;
    if !BATCH.contains(&name.as_str()) {
        return None;
    }
    Some(run(&name, &rest))
}

fn run(name: &str, args: &[String]) -> Result<(), ToolError> {
    match name {
        "fmt-write" => recipe_batch_01::run_fmt(true),
        "test" => cargo(&["test".to_string()]),
        "clippy" => recipe_batch_01::run_clippy(),
        "dev" => run_dev(),
        "e2e" => nix_run_package("e2e", args),
        "e2e-host" => nix_run_package("e2e-host", args),
        "publish" => run_publish(args),
        "deploy" => run_deploy(false, args),
        "deploy-dry-run" => run_deploy(true, args),
        "deploy-host" => run_deploy_host(args),
        other => Err(ToolError::fail(format!(
            "surmount-deploy-host: batch 2 has no recipe {other}"
        ))),
    }
}

fn cargo(args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    run_in(&root, "cargo", args)
}

fn nix_run_package(attr: &str, args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let system = flake_system()?;
    let mut cmd = vec![
        "run".to_string(),
        format!(".#packages.{system}.{attr}"),
        "--".to_string(),
    ];
    cmd.extend(args.iter().cloned());
    run_in(&root, "nix", &cmd)
}

fn run_deploy(dry_run: bool, args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = Vec::new();
    // The closure build is the later remote `nix build`. This `nix run`
    // opens the vendor store. A root client with a non-empty
    // build-users-group chowns that store directory. An empty group
    // skips the chown. The build still runs as this process.
    if dry_run {
        if let Some(store) = crate::paths::outer_vendor_store_uri() {
            cmd.push("--store".to_string());
            cmd.push(store);
            cmd.push("--option".to_string());
            cmd.push("build-users-group".to_string());
            cmd.push(String::new());
        }
    }
    cmd.extend([
        "run".to_string(),
        ".#surmount-deploy-host".to_string(),
        "--".to_string(),
    ]);
    if dry_run {
        cmd.push("--dry-run".to_string());
    }
    cmd.push("--target".to_string());
    cmd.push(DEPLOY_TARGET.to_string());
    cmd.push("--host-local".to_string());
    // The directory is on the machine this process is running on.
    cmd.push(crate::machine_host_local::host_local_dir_for_this_machine().to_string());
    cmd.extend(args.iter().cloned());
    run_in(&root, "nix", &cmd)
}

fn run_deploy_host(args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = vec![
        "run".to_string(),
        ".#surmount-deploy-host".to_string(),
        "--".to_string(),
    ];
    cmd.extend(args.iter().cloned());
    run_in(&root, "nix", &cmd)
}

fn run_publish(args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    nice_status(
        &root,
        &[
            "flake".to_string(),
            "update".to_string(),
            "surmount-site".to_string(),
        ],
    )?;
    let store = nice_stdout(
        &root,
        &[
            "build".to_string(),
            "--print-build-logs".to_string(),
            "--no-link".to_string(),
            "--print-out-paths".to_string(),
            ".#surmount-public-site".to_string(),
        ],
    )?;
    let site_root = store
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .ok_or_else(|| ToolError::fail("publish: nix build printed no store path"))?;
    let mut cmd_args = vec![
        "run".to_string(),
        ".#surmount-deploy-static-sites".to_string(),
        "--".to_string(),
    ];
    cmd_args.extend(args.iter().cloned());
    let status = Command::new("nix")
        .args(&cmd_args)
        .current_dir(&root)
        .env("SURMOUNT_PUBLIC_SITE_ROOT", site_root)
        .status()?;
    if status.success() {
        return Ok(());
    }
    let code = status.code().unwrap_or(1);
    Err(ToolError {
        message: format!("command exited {code}"),
        exit_code: code,
    })
}

fn nice_status(root: &Path, nix_args: &[String]) -> Result<(), ToolError> {
    let args = nice_args(nix_args);
    run_in(root, "nice", &args)
}

fn nice_stdout(root: &Path, nix_args: &[String]) -> Result<String, ToolError> {
    let args = nice_args(nix_args);
    let out = Command::new("nice")
        .args(&args)
        .current_dir(root)
        .stderr(Stdio::inherit())
        .output()?;
    if !out.status.success() {
        let code = out.status.code().unwrap_or(1);
        return Err(ToolError {
            message: format!("command exited {code}"),
            exit_code: code,
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn nice_args(nix_args: &[String]) -> Vec<String> {
    let mut args = vec!["-n".to_string(), "19".to_string(), "nix".to_string()];
    args.extend(nix_args.iter().cloned());
    args
}

fn run_dev() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let listen = env_or("SURMOUNT_LISTEN", "127.0.0.1:8080");
    let domain = env_or("SURMOUNT_PRIMARY_DOMAIN", "demo.local");
    let mail = env_or("SURMOUNT_MAIL_HOSTNAME", "mail.demo.local");
    let services = env_or("SURMOUNT_SERVICES_HOSTNAME", "services.demo.local");
    let stalwart = env_or("SURMOUNT_STALWART_URL", "http://127.0.0.1:8081");
    let mut onion = env_or("SURMOUNT_ONION_URL", "");
    if onion.is_empty() {
        if let Ok(path) = env::var("SURMOUNT_ONION_HOSTNAME_FILE") {
            if !path.is_empty() {
                if let Some(line) = read_onion_hostname(&path) {
                    onion = line;
                }
            }
        }
    }
    stop_prior_ui(&listen);
    println!("dev: management console -> http://{listen}/  (Stalwart probe {stalwart})");
    if onion.is_empty() {
        println!(
            "dev: onion not provisioned (host: surmount.artiHiddenService + hostname under onionServiceStateDir; lab: SURMOUNT_ONION_URL or SURMOUNT_ONION_HOSTNAME_FILE)"
        );
    } else {
        println!("dev: onion surface -> {onion}");
    }
    let _ = std::io::stdout().flush();
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-p",
        "surmount-management-ui",
        "--bin",
        "surmount-management-ui",
    ])
    .current_dir(&root)
    .env("SURMOUNT_LISTEN", &listen)
    .env("SURMOUNT_PRIMARY_DOMAIN", &domain)
    .env("SURMOUNT_MAIL_HOSTNAME", &mail)
    .env("SURMOUNT_SERVICES_HOSTNAME", &services)
    .env("SURMOUNT_STALWART_URL", &stalwart);
    if !onion.is_empty() {
        cmd.env("SURMOUNT_ONION_URL", &onion);
    }
    let status = cmd.status()?;
    if status.success() {
        return Ok(());
    }
    let code = status.code().unwrap_or(1);
    Err(ToolError {
        message: format!("command exited {code}"),
        exit_code: code,
    })
}

fn env_or(key: &str, default: &str) -> String {
    match env::var(key) {
        Ok(value) if !value.is_empty() => value,
        _ => default.to_string(),
    }
}

fn read_onion_hostname(path: &str) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let line: String = text.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if line.is_empty() { None } else { Some(line) }
}

fn stop_prior_ui(listen: &str) {
    let port = listen.rsplit(':').next().unwrap_or(listen);
    if port.is_empty() {
        return;
    }
    let Ok(out) = Command::new("ss").args(["-ltnp"]).output() else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for pid in pids_on_port(&text, port) {
        if !cmdline_has_ui(&pid) {
            continue;
        }
        println!("dev: stopping prior surmount-management-ui (pid {pid}) on {listen}");
        let _ = Command::new("kill").arg(&pid).status();
        thread::sleep(Duration::from_millis(400));
    }
}

fn pids_on_port(ss_text: &str, port: &str) -> Vec<String> {
    let marker = format!(":{port} ");
    let mut pids = Vec::new();
    for line in ss_text.lines() {
        if !line.contains(&marker) {
            continue;
        }
        let Some(pid) = pid_of(line) else {
            continue;
        };
        if !pids.contains(&pid) {
            pids.push(pid);
        }
    }
    pids
}

fn pid_of(line: &str) -> Option<String> {
    let idx = line.find("pid=")?;
    let rest = &line[idx + 4..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(digits)
    }
}

fn cmdline_has_ui(pid: &str) -> bool {
    let path = format!("/proc/{pid}/cmdline");
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes).replace('\0', " ");
    text.contains("surmount-management-ui")
}
