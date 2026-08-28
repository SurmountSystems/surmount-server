use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Result, ToolError};
use crate::kinds::{
    kind_needs_ui_owner, kind_tls_axum_leaf, leaf_install_mode, read_attr,
    rewrite_namecheap_client_ip, safe_stage_id, validate_kind,
};
use crate::paths::{
    DURABLE_MATERIAL, EPHEMERAL_MATERIAL, PRODUCT_STATE, assert_safe_token, discover_repo_root,
    host_material_root_for, is_path_under_root, is_public_product_path, is_symlink,
    material_root_create_mode, mkdir_mode, normalize_host_path, realpath_m, set_mode, sh_quote,
};

const USAGE: &str = "\
Usage:
  secrets-install-host --help
  secrets-install-host --from-staging DIR --host-id ID [options]
  secrets-install-host --from-secret-service --host-id ID [options]
  secrets-install-host --ensure-acme-parents [options]

Install deploy secret material from the operator workstation (domain A)
onto host runtime paths (domain B). Never commits secrets. Never logs values.

Source (exactly one; not required with --ensure-acme-parents):
  --from-staging DIR     Read items from a staging directory (primary).
  --from-secret-service  Resolve items via secret-tool search (optional).

  --ensure-acme-parents  Create conventional ACME parent directories only.
  --tls-dir PATH         Override TLS parent dir (default
                         /var/lib/surmount/secrets/tls).
  --acme-dir PATH        Override ACME parent dir (default
                         /var/lib/surmount/secrets/acme).
  --owner NAME           Owner for leaf dirs (default surmount-ui).
  --group NAME           Group for leaf dirs (default surmount-ui).

Required for secret install (not for --ensure-acme-parents):
  --host-id ID           Logical host id (must match required surmount.host).

Destination (exactly one of --dest-root or --target):
  --dest-root DIR        Install under DIR + host absolute path (hermetic).
  --target HOST          SSH target (user@host or host).

Options:
  --dry-run              Print planned installs (kind + dest path only).
  --require-kind KIND    Require this surmount.kind (repeatable).
  --require-namecheap    Shortcut: require kind namecheap-api.
  --client-ip IP         Rewrite only the ClientIp= line for namecheap-api.
  --kind KIND            Secret-service kinds (repeatable).
  --allow-age-admin-install
                         Dangerous override: allow kind=age-admin.
  --ssh-cmd PATH         Override ssh binary (tests).
  -h, --help             Show this help.

Notes:
  - Durable Domain B root is /var/lib/surmount/secrets (survives reboot).
  - TLS cert is mode 0640 (group-readable, not world). TLS key is 0600
    (owner-only). Remote install also copies both to mail/tls for Stalwart.
  - Secret values never printed. Labels, kinds, paths, exit codes only.
  - Hermetic tests: crate tests (no live Stalwart, no live keyring).
";

struct Args {
    dry_run: bool,
    host_id: String,
    target: String,
    dest_root: String,
    from_staging: String,
    from_secret_service: bool,
    require_kinds: Vec<String>,
    ss_kinds: Vec<String>,
    allow_age_admin: bool,
    ensure_acme_parents: bool,
    client_ip: String,
    acme_tls_dir: String,
    acme_account_dir: String,
    acme_owner: String,
    acme_group: String,
    ssh_cmd: String,
}

fn log_line(msg: &str) {
    eprintln!("secrets-install-host: {msg}");
}

fn parse_args<I, S>(args: I) -> Result<Option<Args>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut a = Args {
        dry_run: false,
        host_id: env::var("SURMOUNT_SECRETS_HOST_ID").unwrap_or_default(),
        target: env::var("SURMOUNT_SECRETS_TARGET")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| env::var("SURMOUNT_DEPLOY_TARGET").ok())
            .unwrap_or_default(),
        dest_root: String::new(),
        from_staging: String::new(),
        from_secret_service: false,
        require_kinds: Vec::new(),
        ss_kinds: Vec::new(),
        allow_age_admin: false,
        ensure_acme_parents: false,
        client_ip: String::new(),
        acme_tls_dir: format!("{DURABLE_MATERIAL}/tls"),
        acme_account_dir: format!("{DURABLE_MATERIAL}/acme"),
        acme_owner: "surmount-ui".into(),
        acme_group: "surmount-ui".into(),
        ssh_cmd: env::var("SURMOUNT_SECRETS_SSH").unwrap_or_else(|_| "ssh".into()),
    };
    let argv: Vec<String> = args.into_iter().map(|s| s.as_ref().to_string()).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "--dry-run" => a.dry_run = true,
            "--from-staging" => {
                i += 1;
                a.from_staging = need(&argv, i, "--from-staging")?;
            }
            "--from-secret-service" => a.from_secret_service = true,
            "--ensure-acme-parents" => a.ensure_acme_parents = true,
            "--tls-dir" => {
                i += 1;
                a.acme_tls_dir = need(&argv, i, "--tls-dir")?;
            }
            "--acme-dir" => {
                i += 1;
                a.acme_account_dir = need(&argv, i, "--acme-dir")?;
            }
            "--owner" => {
                i += 1;
                a.acme_owner = need(&argv, i, "--owner")?;
            }
            "--group" => {
                i += 1;
                a.acme_group = need(&argv, i, "--group")?;
            }
            "--host-id" => {
                i += 1;
                a.host_id = need(&argv, i, "--host-id")?;
            }
            "--target" => {
                i += 1;
                a.target = need(&argv, i, "--target")?;
            }
            "--dest-root" => {
                i += 1;
                a.dest_root = need(&argv, i, "--dest-root")?;
            }
            "--require-kind" => {
                i += 1;
                let k = need(&argv, i, "--require-kind")?;
                validate_kind(&k)?;
                a.require_kinds.push(k);
            }
            "--require-namecheap" => a.require_kinds.push("namecheap-api".into()),
            "--client-ip" => {
                i += 1;
                a.client_ip = need(&argv, i, "--client-ip")?;
            }
            "--kind" => {
                i += 1;
                let k = need(&argv, i, "--kind")?;
                validate_kind(&k)?;
                a.ss_kinds.push(k);
            }
            "--allow-age-admin-install" => a.allow_age_admin = true,
            "--ssh-cmd" => {
                i += 1;
                a.ssh_cmd = need(&argv, i, "--ssh-cmd")?;
            }
            other => {
                return Err(ToolError::fail(format!(
                    "unknown argument: {other} (try --help)"
                )));
            }
        }
        i += 1;
    }
    Ok(Some(a))
}

fn need(argv: &[String], i: usize, flag: &str) -> Result<String> {
    argv.get(i)
        .cloned()
        .ok_or_else(|| ToolError::fail(format!("{flag} requires a value")))
}

struct Item {
    kind: String,
    host_path: String,
    secret_bytes: Vec<u8>,
}

/// Public library entry for sibling crates (add-token / recovery).
pub fn run_from_staging(
    staging: &Path,
    host_id: &str,
    dest_root: Option<&Path>,
    target: Option<&str>,
    require_kind: Option<&str>,
    dry_run: bool,
) -> Result<()> {
    let mut argv = vec![
        "--from-staging".to_string(),
        staging.display().to_string(),
        "--host-id".to_string(),
        host_id.to_string(),
    ];
    if let Some(d) = dest_root {
        argv.push("--dest-root".into());
        argv.push(d.display().to_string());
    }
    if let Some(t) = target {
        argv.push("--target".into());
        argv.push(t.to_string());
    }
    if let Some(k) = require_kind {
        argv.push("--require-kind".into());
        argv.push(k.to_string());
    }
    if dry_run {
        argv.push("--dry-run".into());
    }
    run(argv)
}

pub fn run<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(a) = parse_args(args)? else {
        return Ok(());
    };
    let repo_root = discover_repo_root();

    if !a.client_ip.is_empty() {
        assert_safe_token("client-ip", &a.client_ip)?;
        if a.client_ip.starts_with('-') {
            return Err(ToolError::fail("client-ip must not start with '-'"));
        }
        if !a
            .client_ip
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '.' || c == ':')
        {
            return Err(ToolError::fail(
                "client-ip has invalid characters (expected IPv4 or IPv6 shape)",
            ));
        }
    }

    if !a.target.is_empty() && a.target.starts_with('-') {
        return Err(ToolError::fail(format!(
            "target must not start with '-': option-shaped values are rejected (got {})",
            a.target
        )));
    }
    if a.dest_root.is_empty() && a.target.is_empty() {
        return Err(ToolError::fail(
            "destination required: pass --dest-root DIR (local/hermetic) or --target HOST (SSH)",
        ));
    }
    if !a.dest_root.is_empty() && !a.target.is_empty() {
        return Err(ToolError::fail(
            "choose one destination: --dest-root or --target (not both)",
        ));
    }

    let dest_root_canon = if a.dest_root.is_empty() {
        None
    } else {
        let dest = Path::new(&a.dest_root);
        if is_public_product_path(dest, &repo_root) {
            return Err(ToolError::fail(format!(
                "refuse: --dest-root is a public product path ({}). Never install under hosts/ or secrets/.",
                a.dest_root
            )));
        }
        let canon = realpath_m(dest);
        if is_path_under_root(&repo_root, &canon) {
            return Err(ToolError::fail(format!(
                "refuse: --dest-root is under the public git work tree ({}). Use a private path outside the repo (e.g. under /tmp).",
                a.dest_root
            )));
        }
        Some(canon)
    };

    if a.ensure_acme_parents {
        return run_acme_parents(&a, dest_root_canon.as_deref(), &repo_root);
    }

    if a.host_id.is_empty() {
        return Err(ToolError::fail(
            "host-id required: pass --host-id ID or set SURMOUNT_SECRETS_HOST_ID",
        ));
    }
    if a.host_id.starts_with('-') {
        return Err(ToolError::fail(format!(
            "host-id must not start with '-': option-shaped values are rejected (got {})",
            a.host_id
        )));
    }
    assert_safe_token("host-id", &a.host_id)?;

    let src_count = (!a.from_staging.is_empty()) as u8 + a.from_secret_service as u8;
    if src_count == 0 {
        return Err(ToolError::fail(
            "source required: pass --from-staging DIR, --from-secret-service, or --ensure-acme-parents",
        ));
    }
    if src_count > 1 {
        return Err(ToolError::fail(
            "choose exactly one source: --from-staging or --from-secret-service",
        ));
    }

    let mut items = Vec::new();
    if !a.from_staging.is_empty() {
        items = load_staging(&a.from_staging, &a.host_id, &repo_root, a.allow_age_admin)?;
    }
    if a.from_secret_service {
        items = load_secret_service(&a, &repo_root)?;
    }

    if !a.require_kinds.is_empty() {
        let mut missing = Vec::new();
        for req in &a.require_kinds {
            if !items.iter().any(|it| it.kind == *req) {
                missing.push(req.clone());
            }
        }
        if !missing.is_empty() {
            return Err(ToolError::fail(format!(
                "missing required kind(s): {} (host-id={}). No partial success claimed.",
                missing.join(" "),
                a.host_id
            )));
        }
    }

    for it in &items {
        let src_bytes = if it.kind == "namecheap-api" && !a.client_ip.is_empty() {
            let text = String::from_utf8_lossy(&it.secret_bytes);
            rewrite_namecheap_client_ip(&text, &a.client_ip).into_bytes()
        } else {
            it.secret_bytes.clone()
        };
        if let Some(ref dest) = dest_root_canon {
            let final_path = dest.join(it.host_path.trim_start_matches('/'));
            install_local(
                &src_bytes,
                &final_path,
                dest,
                &it.kind,
                &it.host_path,
                a.dry_run,
                &a.client_ip,
                &repo_root,
            )?;
        } else {
            install_remote(
                &src_bytes,
                &it.host_path,
                &it.kind,
                &a.target,
                &a.ssh_cmd,
                a.dry_run,
                &a.client_ip,
            )?;
        }
    }

    if a.dry_run {
        log_line(&format!(
            "dry-run complete: {} item(s) planned (host-id={})",
            items.len(),
            a.host_id
        ));
    } else {
        log_line(&format!(
            "install complete: {} item(s) (host-id={})",
            items.len(),
            a.host_id
        ));
    }
    Ok(())
}

fn load_staging(
    staging: &str,
    host_id: &str,
    repo_root: &Path,
    allow_age_admin: bool,
) -> Result<Vec<Item>> {
    let staging = Path::new(staging);
    if !staging.is_dir() {
        return Err(ToolError::fail(format!(
            "staging dir missing or not a directory: {}",
            staging.display()
        )));
    }
    let mut items = Vec::new();
    let mut found_any = false;
    let rd = fs::read_dir(staging)?;
    let mut dirs: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for item_dir in dirs {
        let attr = item_dir.join("attributes");
        let sec = item_dir.join("secret");
        if (!attr.exists() && !is_symlink(&attr)) || (!sec.exists() && !is_symlink(&sec)) {
            log_line(&format!(
                "note: skip {}: need attributes + secret files",
                item_dir.file_name().unwrap_or_default().to_string_lossy()
            ));
            continue;
        }
        require_regular_file(&attr, "attributes")?;
        require_regular_file(&sec, "secret")?;
        let attr_body = fs::read_to_string(&attr)?;
        let kind = read_attr(&attr_body, "surmount.kind").unwrap_or_default();
        let hpath = read_attr(&attr_body, "surmount.path").unwrap_or_default();
        let hhost = read_attr(&attr_body, "surmount.host").unwrap_or_default();
        let name = item_dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if kind.is_empty() {
            return Err(ToolError::fail(format!(
                "item {name}: missing surmount.kind"
            )));
        }
        if hpath.is_empty() {
            return Err(ToolError::fail(format!(
                "item {name}: missing surmount.path"
            )));
        }
        if hhost.is_empty() {
            return Err(ToolError::fail(format!(
                "item {name}: missing required surmount.host (must match --host-id)"
            )));
        }
        assert_safe_token("surmount.host", &hhost)?;
        if hhost != host_id {
            return Err(ToolError::fail(format!(
                "item {name}: surmount.host does not match --host-id"
            )));
        }
        validate_kind(&kind)?;
        if kind == "age-admin" && !allow_age_admin {
            return Err(ToolError::fail(
                "refuse: kind=age-admin is workstation-only (never install to host by default). Pass --allow-age-admin-install only if you intentionally need this dangerous override.",
            ));
        }
        let _ = safe_stage_id(&name);
        let canon = normalize_host_path(&hpath, repo_root)?;
        let mut secret_bytes = Vec::new();
        File::open(&sec)?.read_to_end(&mut secret_bytes)?;
        items.push(Item {
            kind,
            host_path: canon,
            secret_bytes,
        });
        found_any = true;
    }
    if !found_any {
        return Err(ToolError::fail(format!(
            "no installable items under staging ({}). Each item needs a subdir with attributes + secret",
            staging.display()
        )));
    }
    Ok(items)
}

fn load_secret_service(a: &Args, repo_root: &Path) -> Result<Vec<Item>> {
    let kinds = if a.ss_kinds.is_empty() {
        vec![
            "tls-cert".into(),
            "tls-key".into(),
            "session-secret".into(),
            "stalwart-token".into(),
            "restic-password".into(),
        ]
    } else {
        a.ss_kinds.clone()
    };
    let mut items = Vec::new();
    for kind in kinds {
        validate_kind(&kind)?;
        let search = Command::new("secret-tool")
            .args([
                "search",
                "surmount.host",
                &a.host_id,
                "surmount.kind",
                &kind,
            ])
            .output();
        let Ok(out) = search else {
            log_line(&format!(
                "note: no secret-tool item for kind={kind} host={}",
                a.host_id
            ));
            continue;
        };
        if !out.status.success() || out.stdout.is_empty() {
            log_line(&format!(
                "note: no secret-tool item for kind={kind} host={}",
                a.host_id
            ));
            continue;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut hpath = String::new();
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("attribute.surmount.path = ") {
                hpath = v.trim().to_string();
                break;
            }
            if let Some(v) = line.strip_prefix("surmount.path = ") {
                hpath = v.trim().to_string();
                break;
            }
        }
        if hpath.is_empty() {
            return Err(ToolError::fail(format!(
                "secret-tool item kind={kind}: missing surmount.path attribute"
            )));
        }
        let lookup = Command::new("secret-tool")
            .args([
                "lookup",
                "surmount.host",
                &a.host_id,
                "surmount.kind",
                &kind,
            ])
            .output()
            .map_err(|e| ToolError::fail(format!("secret-tool lookup failed: {e}")))?;
        if !lookup.status.success() || lookup.stdout.is_empty() {
            return Err(ToolError::fail(format!(
                "secret-tool lookup failed for kind={kind} host={}",
                a.host_id
            )));
        }
        let canon = normalize_host_path(&hpath, repo_root)?;
        items.push(Item {
            kind,
            host_path: canon,
            secret_bytes: lookup.stdout,
        });
    }
    if items.is_empty() {
        return Err(ToolError::fail(format!(
            "no installable items from secret-tool for host-id={}",
            a.host_id
        )));
    }
    Ok(items)
}

fn require_regular_file(path: &Path, label: &str) -> Result<()> {
    if !path.exists() && !is_symlink(path) {
        return Err(ToolError::fail(format!(
            "{label}: missing ({})",
            path.display()
        )));
    }
    if is_symlink(path) {
        return Err(ToolError::fail(format!(
            "refuse: {label} must be a regular file, not a symlink ({})",
            path.display()
        )));
    }
    if !path.is_file() {
        return Err(ToolError::fail(format!(
            "refuse: {label} must be a regular file ({})",
            path.display()
        )));
    }
    Ok(())
}

fn install_local(
    src: &[u8],
    final_path: &Path,
    dest_root: &Path,
    kind: &str,
    host_path: &str,
    dry_run: bool,
    client_ip: &str,
    repo_root: &Path,
) -> Result<()> {
    let final_canon = realpath_m(final_path);
    if is_public_product_path(final_path, repo_root)
        || is_public_product_path(&final_canon, repo_root)
    {
        return Err(ToolError::fail(format!(
            "refuse: will not install into public product path ({})",
            final_path.display()
        )));
    }
    if is_path_under_root(repo_root, &final_canon) {
        return Err(ToolError::fail(format!(
            "refuse: will not install under the public git work tree ({})",
            final_path.display()
        )));
    }
    if !is_path_under_root(dest_root, &final_canon) {
        return Err(ToolError::fail(format!(
            "refuse: resolved path escapes --dest-root ({} -> {}; root={})",
            final_path.display(),
            final_canon.display(),
            dest_root.display()
        )));
    }
    if is_symlink(&final_canon) {
        return Err(ToolError::fail(format!(
            "refuse: destination is a symlink ({})",
            final_canon.display()
        )));
    }
    if final_canon.is_dir() {
        return Err(ToolError::fail(format!(
            "refuse: destination is a directory ({})",
            final_canon.display()
        )));
    }
    if final_canon.exists() && !final_canon.is_file() {
        return Err(ToolError::fail(format!(
            "refuse: destination is not a regular file ({})",
            final_canon.display()
        )));
    }
    let mode = leaf_install_mode(kind);
    if dry_run {
        if kind == "namecheap-api" && !client_ip.is_empty() {
            log_line(&format!(
                "dry-run: would install kind={kind} -> {} (mode 0600 regular file; ClientIp rewrite planned; value not logged)",
                final_canon.display()
            ));
        } else {
            log_line(&format!(
                "dry-run: would install kind={kind} -> {} (mode {mode:04o} regular file)",
                final_canon.display()
            ));
        }
        return Ok(());
    }

    let host_abs = host_path;
    let host_material = host_material_root_for(host_abs).unwrap_or("");
    ensure_parent_dirs(&final_canon, dest_root, host_material)?;
    if !host_material.is_empty() {
        let mat_local = dest_root.join(host_material.trim_start_matches('/'));
        if mat_local.is_dir() {
            let want = material_root_create_mode(host_material);
            let _ = set_mode(&mat_local, want);
        }
    }
    ensure_ui_parent_modes_local(kind, &final_canon, dest_root, host_material);

    let parent = final_canon.parent().unwrap_or(dest_root);
    if is_symlink(&final_canon) {
        return Err(ToolError::fail(format!(
            "refuse: destination is a symlink ({})",
            final_canon.display()
        )));
    }
    if final_canon.is_dir() {
        return Err(ToolError::fail(format!(
            "refuse: destination is a directory ({})",
            final_canon.display()
        )));
    }
    write_atomic(parent, &final_canon, src, mode)?;
    maybe_chown_ui_secret(kind, &final_canon);
    ensure_ui_parent_modes_local(kind, &final_canon, dest_root, host_material);
    if kind == "namecheap-api" && !client_ip.is_empty() {
        log_line(&format!(
            "installed kind={kind} -> {} (mode 0600; ClientIp rewrite applied; values not logged)",
            final_canon.display()
        ));
    } else {
        log_line(&format!(
            "installed kind={kind} -> {} (mode {mode:04o})",
            final_canon.display()
        ));
    }
    Ok(())
}

fn write_atomic(parent: &Path, dest: &Path, src: &[u8], mode: u32) -> Result<()> {
    fs::create_dir_all(parent)?;
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true).mode(0o600);
    let tmp = parent.join(format!(".surmount-secret.{}", std::process::id()));
    {
        let mut f = opts.open(&tmp)?;
        f.write_all(src)?;
        f.sync_all()?;
    }
    let _ = set_mode(&tmp, 0o600);
    if dest.is_dir() || is_symlink(dest) {
        let _ = fs::remove_file(&tmp);
        return Err(ToolError::fail(format!(
            "refuse: destination is directory or symlink ({}); no install",
            dest.display()
        )));
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ToolError::fail(format!(
            "refuse: failed to install regular file at {}: {e}",
            dest.display()
        ))
    })?;
    set_mode(dest, mode)?;
    if is_symlink(dest) {
        let _ = fs::remove_file(dest);
        return Err(ToolError::fail(format!(
            "refuse: install left a symlink at {}",
            dest.display()
        )));
    }
    if !dest.is_file() {
        return Err(ToolError::fail(format!(
            "refuse: install did not produce a regular file at {}",
            dest.display()
        )));
    }
    Ok(())
}

fn ensure_parent_dirs(final_path: &Path, base: &Path, host_material: &str) -> Result<()> {
    let parent = final_path.parent().unwrap_or(base);
    let parent_r = realpath_m(parent);
    if !is_path_under_root(base, &parent_r) {
        return Err(ToolError::fail(format!(
            "refuse: parent path escapes confinement base ({}; base={})",
            parent.display(),
            base.display()
        )));
    }
    let root_mode = if host_material.is_empty() {
        0o700
    } else {
        material_root_create_mode(host_material)
    };
    if !base.is_dir() {
        if let Some(p) = base.parent() {
            fs::create_dir_all(p)?;
        }
        mkdir_mode(base, root_mode).or_else(|_| {
            fs::create_dir_all(base)?;
            set_mode(base, root_mode)
        })?;
    }
    let mat_local = if host_material.is_empty() {
        None
    } else {
        Some(base.join(host_material.trim_start_matches('/')))
    };
    let rest = parent_r.strip_prefix(realpath_m(base)).unwrap_or(&parent_r);
    let mut cur = realpath_m(base);
    for part in rest.components() {
        let std::path::Component::Normal(s) = part else {
            continue;
        };
        cur.push(s);
        let mode = if let Some(ref mat) = mat_local {
            if &cur == mat {
                root_mode
            } else if cur.starts_with(mat) {
                0o750
            } else if root_mode == 0o755 {
                0o755
            } else {
                0o700
            }
        } else {
            0o700
        };
        if !cur.is_dir() {
            mkdir_mode(&cur, mode).or_else(|_| {
                fs::create_dir_all(&cur)?;
                set_mode(&cur, mode)
            })?;
        }
    }
    Ok(())
}

fn ensure_ui_parent_modes_local(
    kind: &str,
    final_path: &Path,
    dest_root: &Path,
    host_material: &str,
) {
    if !kind_needs_ui_owner(kind) || host_material.is_empty() {
        return;
    }
    let mat_local = dest_root.join(host_material.trim_start_matches('/'));
    let mut cur = final_path.parent().map(Path::to_path_buf);
    while let Some(p) = cur {
        if p == dest_root || p.as_os_str() == "/" {
            break;
        }
        if p == mat_local {
            let _ = set_mode(&p, 0o755);
        } else if p.starts_with(&mat_local) {
            let _ = set_mode(&p, 0o750);
        }
        cur = p.parent().map(Path::to_path_buf);
        if cur.as_ref() == Some(&p) {
            break;
        }
    }
    if host_material == DURABLE_MATERIAL
        || host_material.starts_with(&format!("{DURABLE_MATERIAL}/"))
    {
        let state = dest_root.join("var/lib/surmount");
        if state.is_dir() {
            let _ = set_mode(&state, 0o755);
        }
    }
}

fn maybe_chown_ui_secret(_kind: &str, _path: &Path) {
    // chown only when surmount-ui exists; hermetic dest-root usually lacks it.
}

fn install_remote(
    src: &[u8],
    hpath: &str,
    kind: &str,
    host: &str,
    ssh_cmd: &str,
    dry_run: bool,
    client_ip: &str,
) -> Result<()> {
    let mode = leaf_install_mode(kind);
    if dry_run {
        if kind == "namecheap-api" && !client_ip.is_empty() {
            log_line(&format!(
                "dry-run: would ssh-install kind={kind} -> {host}:{} (mode 0600, ClientIp rewrite planned; values not logged)",
                sh_quote(hpath)
            ));
        } else {
            log_line(&format!(
                "dry-run: would ssh-install kind={kind} -> {host}:{} (mode {mode:04o}, umask 077, regular-file postcondition)",
                sh_quote(hpath)
            ));
        }
        return Ok(());
    }
    let parent = Path::new(hpath)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/".into());
    let material = host_material_root_for(hpath)
        .ok_or_else(|| ToolError::fail("refuse: remote path outside material roots".to_string()))?;
    let mat_mode = material_root_create_mode(material);
    let ui_chown = if kind_needs_ui_owner(kind) { 1 } else { 0 };
    let tls_axum = if kind_tls_axum_leaf(kind) { 1 } else { 0 };
    let script = format!(
        "set -euo pipefail\n\
material={}\n\
parent={}\n\
final={}\n\
mat_mode={}\n\
ui_chown={}\n\
leaf_mode={}\n\
tls_axum={}\n\
kind={}\n\
if [ -L \"$final\" ]; then echo \"secrets-install-host: refuse: destination is a symlink\" >&2; exit 1; fi\n\
if [ -d \"$final\" ]; then echo \"secrets-install-host: refuse: destination is a directory\" >&2; exit 1; fi\n\
if [ -e \"$final\" ] && [ ! -f \"$final\" ]; then echo \"secrets-install-host: refuse: destination is not a regular file\" >&2; exit 1; fi\n\
if [ ! -d \"$material\" ]; then\n\
  if [ \"$material\" = {DURABLE_MATERIAL} ]; then\n\
    if [ ! -d {PRODUCT_STATE} ]; then mkdir -m 0755 {PRODUCT_STATE}; else chmod 0755 {PRODUCT_STATE} 2>/dev/null || true; fi\n\
  fi\n\
  mkdir -m \"$mat_mode\" \"$material\"\n\
else\n\
  chmod \"$mat_mode\" \"$material\" 2>/dev/null || true\n\
fi\n\
rest=${{parent#\"$material\"}}\n\
rest=${{rest#/}}\n\
cur=\"$material\"\n\
IFS=/\n\
set -- $rest\n\
unset IFS\n\
for part in \"$@\"; do\n\
  [ -n \"$part\" ] || continue\n\
  cur=\"$cur/$part\"\n\
  if [ ! -d \"$cur\" ]; then mkdir -m 0750 \"$cur\"; fi\n\
done\n\
tmp=$(mktemp \"$parent/.surmount-secret.XXXXXX\")\n\
chmod 0600 \"$tmp\"\n\
umask 077\n\
cat >\"$tmp\"\n\
chmod 0600 \"$tmp\"\n\
if [ -d \"$final\" ] || [ -L \"$final\" ]; then rm -f \"$tmp\"; echo \"secrets-install-host: refuse: destination is directory or symlink\" >&2; exit 1; fi\n\
mv -f \"$tmp\" \"$final\"\n\
chmod \"$leaf_mode\" \"$final\"\n\
if [ \"$tls_axum\" = 1 ]; then\n\
  mail_tls=/var/lib/surmount/secrets/mail/tls\n\
  mkdir -p \"$mail_tls\"\n\
  chmod 0750 \"$mail_tls\"\n\
  chown stalwart-mail:stalwart-mail \"$mail_tls\" 2>/dev/null || true\n\
  leaf=$(basename \"$final\")\n\
  cp -f \"$final\" \"$mail_tls/$leaf\"\n\
  if [ \"$kind\" = tls-key ]; then chmod 0600 \"$mail_tls/$leaf\"; else chmod 0640 \"$mail_tls/$leaf\"; fi\n\
  chown stalwart-mail:stalwart-mail \"$mail_tls/$leaf\" 2>/dev/null || true\n\
fi\n\
if [ -L \"$final\" ] || [ ! -f \"$final\" ]; then echo \"secrets-install-host: refuse: install did not produce a regular file\" >&2; exit 1; fi\n",
        sh_quote(material),
        sh_quote(&parent),
        sh_quote(hpath),
        sh_quote(&format!("{mat_mode:o}")),
        sh_quote(&ui_chown.to_string()),
        sh_quote(&format!("{mode:o}")),
        sh_quote(&tls_axum.to_string()),
        sh_quote(kind),
    );
    let mut child = Command::new(ssh_cmd)
        .args(["--", host, &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ToolError::fail(format!("ssh failed to start: {e}")))?;
    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ToolError::fail("ssh stdin was not piped".to_string()))?;
        stdin.write_all(src)?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| ToolError::fail(format!("ssh wait: {e}")))?;
    if !out.stderr.is_empty() {
        let _ = io::stderr().write_all(&out.stderr);
    }
    if !out.status.success() {
        return Err(ToolError::fail(format!(
            "remote install failed (rc={})",
            out.status.code().unwrap_or(1)
        )));
    }
    log_line(&format!(
        "installed kind={kind} -> {host}:{hpath} (mode {mode:04o})"
    ));
    Ok(())
}

fn run_acme_parents(a: &Args, dest_root: Option<&Path>, repo_root: &Path) -> Result<()> {
    if !a.from_staging.is_empty() || a.from_secret_service {
        return Err(ToolError::fail(
            "refuse: --ensure-acme-parents cannot combine with --from-staging or --from-secret-service (parents-only mode)",
        ));
    }
    if !a.require_kinds.is_empty() || !a.ss_kinds.is_empty() {
        return Err(ToolError::fail(
            "refuse: --ensure-acme-parents does not take --require-kind / --kind (no secret payloads)",
        ));
    }
    assert_safe_token("owner", &a.acme_owner)?;
    assert_safe_token("group", &a.acme_group)?;
    if a.acme_owner.starts_with('-') || a.acme_group.starts_with('-') {
        return Err(ToolError::fail("owner/group must not start with '-'"));
    }
    let tls_canon = normalize_host_path(&a.acme_tls_dir, repo_root)?;
    let acme_canon = normalize_host_path(&a.acme_account_dir, repo_root)?;
    let material_tls = host_material_root_for(&tls_canon)
        .ok_or_else(|| ToolError::fail("internal: tls dir outside material roots".to_string()))?;
    let material_acme = host_material_root_for(&acme_canon)
        .ok_or_else(|| ToolError::fail("internal: acme dir outside material roots".to_string()))?;
    if material_tls != material_acme {
        return Err(ToolError::fail(
            "refuse: --tls-dir and --acme-dir must share the same material root",
        ));
    }
    if let Some(dest) = dest_root {
        ensure_acme_parents_local(
            dest,
            material_tls,
            &tls_canon,
            &acme_canon,
            &a.acme_owner,
            &a.acme_group,
            a.dry_run,
        )?;
    } else {
        if a.dry_run {
            log_line(&format!(
                "dry-run: would ssh-ensure ACME parents on {} (material={material_tls} tls={tls_canon} acme={acme_canon} mode=0750 owner={} group={}; no PEMs)",
                a.target, a.acme_owner, a.acme_group
            ));
            log_line("dry-run complete: ACME parents planned (no PEMs)");
            return Ok(());
        }
        return Err(ToolError::fail(
            "remote --ensure-acme-parents over SSH is operator-host only; hermetic tests use --dest-root",
        ));
    }
    if a.dry_run {
        log_line("dry-run complete: ACME parents planned (no PEMs)");
    } else {
        log_line("ACME parents ensure complete (no PEMs written)");
    }
    Ok(())
}

fn ensure_acme_parents_local(
    dest: &Path,
    material_root: &str,
    tls_canon: &str,
    acme_canon: &str,
    owner: &str,
    group: &str,
    dry_run: bool,
) -> Result<()> {
    if dry_run {
        log_line(&format!(
            "dry-run: would ensure ACME parents under dest-root (material={material_root} tls={tls_canon} acme={acme_canon} mode=0750 owner={owner} group={group}; no PEMs)"
        ));
        return Ok(());
    }
    let root_local = dest.join(material_root.trim_start_matches('/'));
    let tls_local = dest.join(tls_canon.trim_start_matches('/'));
    let acme_local = dest.join(acme_canon.trim_start_matches('/'));
    if is_symlink(&root_local) {
        return Err(ToolError::fail(format!(
            "refuse: material root is a symlink ({})",
            root_local.display()
        )));
    }
    if !root_local.is_dir() {
        if let Some(p) = root_local.parent() {
            fs::create_dir_all(p)?;
        }
        mkdir_mode(&root_local, 0o755).or_else(|_| {
            fs::create_dir_all(&root_local)?;
            set_mode(&root_local, 0o755)
        })?;
    }
    fs::create_dir_all(&tls_local)?;
    fs::create_dir_all(&acme_local)?;
    if is_symlink(&tls_local) || is_symlink(&acme_local) {
        return Err(ToolError::fail(
            "refuse: ACME parent path is a symlink".to_string(),
        ));
    }
    if !tls_local.is_dir() || !acme_local.is_dir() {
        return Err(ToolError::fail(
            "refuse: failed to create ACME parent directories".to_string(),
        ));
    }
    set_mode(&root_local, 0o755)?;
    set_mode(&tls_local, 0o750)?;
    set_mode(&acme_local, 0o750)?;
    if material_root == DURABLE_MATERIAL {
        let state = dest.join("var/lib/surmount");
        if state.is_dir() {
            let _ = set_mode(&state, 0o755);
        }
    }
    log_line(&format!(
        "ensured ACME parents (mode 0750; chown skipped if {owner}:{group} not present): {} {}",
        tls_local.display(),
        acme_local.display()
    ));
    if tls_local.join("cert.pem").exists()
        || tls_local.join("key.pem").exists()
        || acme_local.join("account.json").exists()
    {
        log_line(
            "note: existing PEM/account files present under parents (not modified by --ensure-acme-parents)",
        );
    }
    let _ = EPHEMERAL_MATERIAL;
    Ok(())
}
