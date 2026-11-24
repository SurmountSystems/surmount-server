//! Flake input updater and nix/packages pin updater.
//!
//! `update-flakes` and `update-software` are the two subcommands.
//! Other just recipe names dispatch to the same underlying Nix or cargo
//! command the recipe used to spell, without a shell script in the justfile.
//! Invoked as argv0 (flake app symlink) or as `surmount-deploy-host <name>`.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::error::ToolError;
use crate::paths::discover_repo_root;

/// Menhera cooldown sparse index. Laptop cargo uses this name
/// (`menhera-cooldown`, 10 day window). Not the crates.io index.
pub const MENHERA_COOLDOWN_REGISTRY: &str = "sparse+https://index.crates.menhera.org/10d/";

const DEPLOY_TARGET: &str = "root@surmount-1";
const DEPLOY_HOST_LOCAL: &str = "/home/hunter/.local/share/surmount/host-local";

/// What `maybe_dispatch` will run. Tests use this so they do not spawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatch {
    NotOurs,
    UpdateFlakes,
    UpdateSoftware,
    Recipe(String),
}

pub fn maybe_dispatch(args: &[String]) -> Option<Result<(), ToolError>> {
    let (dispatch, rest) = classify_invocation(args);
    match dispatch {
        Dispatch::NotOurs => None,
        Dispatch::UpdateFlakes => Some(run_update_flakes()),
        Dispatch::UpdateSoftware => Some(run_update_software()),
        Dispatch::Recipe(name) => Some(run_recipe(&name, &rest)),
    }
}

pub fn classify_invocation(args: &[String]) -> (Dispatch, Vec<String>) {
    let argv0 = args
        .first()
        .map(|s| {
            Path::new(s)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(s)
                .to_string()
        })
        .unwrap_or_default();
    let rest_from = if args.is_empty() { 0 } else { 1 };
    let rest: Vec<String> = args[rest_from..].to_vec();
    if is_dispatch_name(&argv0) && argv0 != "surmount-deploy-host" {
        return (dispatch_for(&argv0), strip_leading_ddash(rest));
    }
    if let Some(first) = rest.first() {
        if is_dispatch_name(first) {
            let name = first.clone();
            return (dispatch_for(&name), strip_leading_ddash(rest[1..].to_vec()));
        }
    }
    (Dispatch::NotOurs, rest)
}

fn dispatch_for(name: &str) -> Dispatch {
    match name {
        "update-flakes" => Dispatch::UpdateFlakes,
        "update-software" => Dispatch::UpdateSoftware,
        other if recipe_kind(other).is_some() => Dispatch::Recipe(other.to_string()),
        _ => Dispatch::NotOurs,
    }
}

fn is_dispatch_name(name: &str) -> bool {
    !matches!(dispatch_for(name), Dispatch::NotOurs)
}

fn strip_leading_ddash(mut args: Vec<String>) -> Vec<String> {
    if args.first().map(String::as_str) == Some("--") {
        args.remove(0);
    }
    args
}

pub fn menhera_cargo_update_args() -> Vec<String> {
    vec![
        "update".to_string(),
        "--config".to_string(),
        "source.crates-io.replace-with=\"menhera-cooldown\"".to_string(),
        "--config".to_string(),
        format!("source.menhera-cooldown.registry=\"{MENHERA_COOLDOWN_REGISTRY}\""),
    ]
}

pub fn cargo_audit_nix_args(system: &str) -> Vec<String> {
    vec![
        "build".to_string(),
        "--option".to_string(),
        "max-jobs".to_string(),
        "0".to_string(),
        "--option".to_string(),
        "warn-dirty".to_string(),
        "false".to_string(),
        "--print-build-logs".to_string(),
        "--print-out-paths".to_string(),
        "--no-link".to_string(),
        format!(".#checks.{system}.cargo-audit"),
    ]
}

fn run_update_flakes() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let flakes = find_flake_files(&root)?;
    if flakes.is_empty() {
        return Err(ToolError::fail(
            "update-flakes: no flake.nix under the repo",
        ));
    }
    for flake in flakes {
        update_one_flake(&flake)?;
    }
    Ok(())
}

fn update_one_flake(flake: &Path) -> Result<(), ToolError> {
    let text = fs::read_to_string(flake)?;
    let inputs = parse_flake_inputs(&text);
    if inputs.is_empty() {
        return Err(ToolError::fail(format!(
            "update-flakes: no inputs parsed in {}",
            flake.display()
        )));
    }
    let mut text = text;
    for input in &inputs {
        if let Some(old_rev) = &input.commit_pin {
            let (owner, repo) = github_owner_repo(&input.url).ok_or_else(|| {
                ToolError::fail(format!(
                    "update-flakes: commit pin {} is not a github: URL ({})",
                    input.name, input.url
                ))
            })?;
            let tip = github_head(&owner, &repo)?;
            if tip != *old_rev {
                if !text.contains(old_rev) {
                    return Err(ToolError::fail(format!(
                        "update-flakes: rev {old_rev} for {} not found in {}",
                        input.name,
                        flake.display()
                    )));
                }
                text = text.replace(old_rev, &tip);
                eprintln!("update-flakes: {} {} -> {}", input.name, old_rev, tip);
            } else {
                eprintln!("update-flakes: {} already at {tip}", input.name);
            }
        }
    }
    fs::write(flake, &text)?;
    let dir = flake.parent().unwrap_or_else(|| Path::new("."));
    for input in &inputs {
        eprintln!(
            "update-flakes: nix flake lock --update-input {}",
            input.name
        );
        let status = Command::new("nix")
            .args(["flake", "lock", "--update-input", &input.name])
            .current_dir(dir)
            .status()?;
        if !status.success() {
            return Err(ToolError::fail(format!(
                "update-flakes: nix flake lock --update-input {} failed",
                input.name
            )));
        }
    }
    Ok(())
}

fn run_update_software() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let pins = discover_hand_pins(&root)?;
    for pin in pins {
        update_one_pin(&root, &pin)?;
    }
    eprintln!("update-software: cargo update via menhera-cooldown ({MENHERA_COOLDOWN_REGISTRY})");
    let cargo_args = menhera_cargo_update_args();
    let status = Command::new("cargo")
        .args(&cargo_args)
        .current_dir(&root)
        .status()?;
    if !status.success() {
        return Err(ToolError::fail(
            "update-software: cargo update against the menhera cooldown index failed",
        ));
    }
    if env_nonempty("BUILD_LOCAL") {
        return Err(ToolError {
            message: "update-software: refuse BUILD_LOCAL for the cargo-audit check (same gate as audit-remote)".into(),
            exit_code: 2,
        });
    }
    let system = flake_system()?;
    eprintln!(
        "update-software: hermetic cargo-audit via {}",
        cargo_audit_nix_args(&system).join(" ")
    );
    remote_nix_build(
        &root,
        &format!(".#checks.{system}.cargo-audit"),
        "audit-remote",
    )
}

fn update_one_pin(root: &Path, pin: &HandPin) -> Result<(), ToolError> {
    let path = root.join(&pin.rel);
    let text = fs::read_to_string(&path)?;
    let latest = match &pin.upstream {
        Upstream::GithubRelease { owner, repo } => github_latest_version(owner, repo)?,
        Upstream::GitlabTag {
            api_project,
            tag_prefix,
        } => gitlab_latest_version(api_project, tag_prefix)?,
    };
    if latest == pin.version {
        eprintln!("update-software: {} stays {}", pin.rel, pin.version);
        return Ok(());
    }
    eprintln!("update-software: {} {} -> {}", pin.rel, pin.version, latest);
    let (replaced, old) = replace_version_assignment(&text, &latest)?;
    if old != pin.version {
        return Err(ToolError::fail(format!(
            "update-software: {} version assignment is {old}, scan saw {}",
            pin.rel, pin.version
        )));
    }
    let hashed = refresh_fetch_hashes(&replaced, &latest)?;
    let hashed = crate::update_pin_refresh::refresh_crate_tarball(&hashed, &latest)?;
    fs::write(&path, &hashed)?;
    if hashed.contains("fetchFromGitLab") {
        crate::update_pin_refresh::refresh_named_hash(
            root,
            &pin.rel,
            pin.cargo_hash_attr.as_deref(),
            false,
        )?;
    }
    if pin.cargo_hash_attr.is_some() && hashed.contains("cargoHash") {
        refresh_cargo_hash(root, pin, &latest)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlakeInput {
    name: String,
    url: String,
    /// Set when the URL's last path segment is a 40 hex commit, not a branch.
    commit_pin: Option<String>,
}

#[derive(Debug, Clone)]
struct HandPin {
    rel: String,
    version: String,
    upstream: Upstream,
    /// Flake package attr to rebuild when `cargoHash` must move. None skips.
    cargo_hash_attr: Option<String>,
}

#[derive(Debug, Clone)]
enum Upstream {
    GithubRelease {
        owner: String,
        repo: String,
    },
    GitlabTag {
        api_project: String,
        tag_prefix: String,
    },
}

pub fn pin_version_is_upstream(version: &str) -> bool {
    !matches!(version, "0" | "1" | "0.1.0")
}

fn parse_flake_inputs(text: &str) -> Vec<FlakeInput> {
    let Some(block) = inputs_block(text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pending: Option<String> = None;
    for line in block.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        if let Some((name, url)) = split_dot_url(t) {
            out.push(flake_input(name, url));
            pending = None;
            continue;
        }
        if let Some(url) = quoted_after(t, "url = ") {
            if let Some(name) = pending.clone() {
                out.push(flake_input(name, url));
            }
            continue;
        }
        if let Some(name) = attr_open_name(t) {
            pending = Some(name);
        }
    }
    out
}

fn flake_input(name: String, url: String) -> FlakeInput {
    let commit_pin = commit_pin_of(&url);
    FlakeInput {
        name,
        url,
        commit_pin,
    }
}

pub fn commit_pin_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("github:")?;
    let segment = rest.split('/').nth(2)?;
    let segment = segment.split(['?', '#']).next().unwrap_or(segment);
    let segment = segment.trim_end_matches(".git");
    if is_git_commit(segment) {
        Some(segment.to_string())
    } else {
        None
    }
}

pub fn is_git_commit(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn github_owner_repo(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("github:")?;
    let mut parts = rest.split('/');
    let owner = parts.next()?.to_string();
    let repo = parts.next()?.split(['?', '#']).next().unwrap_or("");
    let repo = repo.trim_end_matches(".git").to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

fn inputs_block(text: &str) -> Option<String> {
    let start = text.find("inputs = {")?;
    let bytes = &text.as_bytes()[start..];
    let mut depth = 0i32;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'{' {
            depth += 1;
        } else if *b == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(text[start..start + i + 1].to_string());
            }
        }
    }
    None
}

fn split_dot_url(line: &str) -> Option<(String, String)> {
    let (left, right) = line.split_once(".url = ")?;
    let name = left.split_whitespace().last()?.trim().to_string();
    if name.is_empty() || name.contains('=') {
        return None;
    }
    let url = quoted_value(right.trim())?;
    Some((name, url))
}

fn attr_open_name(line: &str) -> Option<String> {
    let t = line.trim().trim_end_matches('{').trim();
    let t = t.strip_suffix('=')?.trim();
    if t.is_empty() || t.contains(' ') || t.contains('.') || t.starts_with('#') {
        return None;
    }
    if !line.contains('{') {
        return None;
    }
    Some(t.to_string())
}

fn quoted_after(line: &str, prefix: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(prefix)?;
    quoted_value(rest.trim_start())
}

fn quoted_value(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches(';').trim();
    let s = s.strip_prefix('"')?;
    let end = s.find('"')?;
    Some(s[..end].to_string())
}

fn find_flake_files(root: &Path) -> Result<Vec<PathBuf>, ToolError> {
    let mut out = Vec::new();
    walk_flakes(root, &mut out, 0)?;
    out.sort();
    Ok(out)
}

fn walk_flakes(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) -> Result<(), ToolError> {
    if depth > 6 {
        return Ok(());
    }
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    for ent in rd {
        let ent = ent?;
        let name = ent.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "vendor" || name == "result" {
            continue;
        }
        let path = ent.path();
        if path.is_dir() {
            walk_flakes(&path, out, depth + 1)?;
        } else if name == "flake.nix" {
            out.push(path);
        }
    }
    Ok(())
}

fn github_head(owner: &str, repo: &str) -> Result<String, ToolError> {
    let url = format!("https://github.com/{owner}/{repo}.git");
    let out = Command::new("git")
        .args(["ls-remote", &url, "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(ToolError::fail(format!(
            "update-flakes: git ls-remote {url} failed"
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let sha = text.split_whitespace().next().unwrap_or("");
    if !is_git_commit(sha) {
        return Err(ToolError::fail(format!(
            "update-flakes: git ls-remote {url} did not return a commit"
        )));
    }
    Ok(sha.to_string())
}

fn github_latest_version(owner: &str, repo: &str) -> Result<String, ToolError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");
    let body = curl_get(&url)?;
    let tag = json_string_field(&body, "tag_name").ok_or_else(|| {
        ToolError::fail(format!(
            "update-software: no tag_name in GitHub latest for {owner}/{repo}"
        ))
    })?;
    release_version(&tag, "")
}

fn gitlab_latest_version(api_project: &str, tag_prefix: &str) -> Result<String, ToolError> {
    let url =
        format!("https://gitlab.torproject.org/api/v4/projects/{api_project}/releases?per_page=20");
    let body = curl_get(&url)?;
    let mut rest = body.as_str();
    while let Some(tag) = json_string_field(rest, "tag_name") {
        if let Some(idx) = rest.find("\"tag_name\"") {
            rest = &rest[idx + 1..];
        } else {
            break;
        }
        if tag.contains("rc") || tag.contains("alpha") || tag.contains("beta") {
            continue;
        }
        if let Ok(ver) = release_version(&tag, tag_prefix) {
            return Ok(ver);
        }
    }
    Err(ToolError::fail(format!(
        "update-software: no GitLab release tag with prefix {tag_prefix}"
    )))
}

fn release_version(tag: &str, prefix: &str) -> Result<String, ToolError> {
    let rest = if !prefix.is_empty() {
        tag.strip_prefix(prefix).ok_or_else(|| {
            ToolError::fail(format!(
                "update-software: tag {tag} missing prefix {prefix}"
            ))
        })?
    } else if let Some(stripped) = tag.strip_prefix('v') {
        stripped
    } else {
        tag
    };
    if rest.is_empty() || !rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(ToolError::fail(format!(
            "update-software: tag {tag} is not a version"
        )));
    }
    Ok(rest.to_string())
}

pub fn json_string_field(body: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let idx = body.find(&key)?;
    let after = &body[idx + key.len()..];
    let after = after.trim_start().strip_prefix(':')?.trim_start();
    quoted_value(after)
}

pub fn replace_version_assignment(
    text: &str,
    new_version: &str,
) -> Result<(String, String), ToolError> {
    let key = "version = \"";
    let mut search = 0usize;
    while let Some(rel) = text[search..].find(key) {
        let start = search + rel;
        let line_start = text[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_end = text[start..]
            .find('\n')
            .map(|p| start + p)
            .unwrap_or(text.len());
        let line = &text[line_start..line_end];
        if line.trim_start().starts_with('#') {
            search = line_end;
            continue;
        }
        let val_start = start + key.len();
        let val_end = text[val_start..]
            .find('"')
            .map(|p| val_start + p)
            .ok_or_else(|| ToolError::fail("update-software: unterminated version string"))?;
        let old = text[val_start..val_end].to_string();
        let mut out = String::with_capacity(text.len() + new_version.len());
        out.push_str(&text[..val_start]);
        out.push_str(new_version);
        out.push_str(&text[val_end..]);
        return Ok((out, old));
    }
    Err(ToolError::fail(
        "update-software: no version = \"...\" assignment",
    ))
}

fn refresh_fetch_hashes(text: &str, version: &str) -> Result<String, ToolError> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if let Some(url_raw) = quoted_after(trimmed, "url = ") {
            let concrete = url_raw
                .replace("${version}", version)
                .replace("${finalAttrs.version}", version);
            if concrete.starts_with("https://") && concrete != url_raw {
                let sri = prefetch_sri(&concrete)?;
                out.push_str(line);
                i += 1;
                let mut replaced = false;
                while i < lines.len() {
                    if lines[i].contains("hash = \"") {
                        out.push_str(&replace_hash_line(lines[i], &sri));
                        if !lines[i].ends_with('\n') {
                            out.push('\n');
                        }
                        i += 1;
                        replaced = true;
                        break;
                    }
                    out.push_str(lines[i]);
                    i += 1;
                }
                if !replaced {
                    return Err(ToolError::fail(format!(
                        "update-software: no hash line after {url_raw}"
                    )));
                }
                continue;
            }
        }
        out.push_str(line);
        i += 1;
    }
    Ok(out)
}

fn replace_hash_line(line: &str, sri: &str) -> String {
    let Some(start) = line.find("hash = \"") else {
        return line.to_string();
    };
    let val = start + "hash = \"".len();
    let Some(end_rel) = line[val..].find('"') else {
        return line.to_string();
    };
    let mut out = String::new();
    out.push_str(&line[..val]);
    out.push_str(sri);
    out.push_str(&line[val + end_rel..]);
    out
}

fn prefetch_sri(url: &str) -> Result<String, ToolError> {
    let out = Command::new("nix")
        .args(["store", "prefetch-file", "--json", url])
        .output()?;
    if !out.status.success() {
        return Err(ToolError::fail(format!(
            "update-software: nix store prefetch-file failed for {url}"
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    json_string_field(&text, "hash").ok_or_else(|| {
        ToolError::fail(format!(
            "update-software: prefetch-file JSON has no hash for {url}"
        ))
    })
}

fn refresh_cargo_hash(root: &Path, pin: &HandPin, _version: &str) -> Result<(), ToolError> {
    let Some(attr) = &pin.cargo_hash_attr else {
        return Ok(());
    };
    let path = root.join(&pin.rel);
    let installable = format!(".#{attr}");
    eprintln!("update-software: refreshing cargoHash via nix build {installable}");
    let out = Command::new("nix")
        .args(["build", "--no-link", &installable])
        .current_dir(root)
        .output()?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let Some(got) = sri_got(&err) else {
        let _ = fs::read_to_string(&path);
        return Err(ToolError::fail(format!(
            "update-software: nix build {installable} failed and stderr had no got: hash"
        )));
    };
    let text = fs::read_to_string(&path)?;
    let Some(updated) = replace_cargo_hash(&text, &got) else {
        return Err(ToolError::fail(format!(
            "update-software: {} has no cargoHash line to set to {got}",
            pin.rel
        )));
    };
    fs::write(&path, updated)?;
    eprintln!(
        "update-software: {} cargoHash set from nix build got: line",
        pin.rel
    );
    Ok(())
}

fn sri_got(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("got:") {
            let hash = rest.trim();
            if hash.starts_with("sha256-") {
                return Some(hash.to_string());
            }
        }
    }
    None
}

fn replace_cargo_hash(text: &str, sri: &str) -> Option<String> {
    replace_cargo_hash_key(text, "cargoHash = \"", sri)
}

fn replace_cargo_hash_key(text: &str, key: &str, sri: &str) -> Option<String> {
    let start = text.find(key)?;
    let val = start + key.len();
    let end = val + text[val..].find('"')?;
    let mut out = String::new();
    out.push_str(&text[..val]);
    out.push_str(sri);
    out.push_str(&text[end..]);
    Some(out)
}

fn discover_hand_pins(root: &Path) -> Result<Vec<HandPin>, ToolError> {
    let dir = root.join("nix/packages");
    let rd = fs::read_dir(&dir)?;
    let mut pins = Vec::new();
    let mut unknown = Vec::new();
    for ent in rd {
        let ent = ent?;
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("nix") {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let fetched = text.contains("fetchurl")
            || text.contains("fetchFromGitLab")
            || text.contains("fetchCrate");
        if !fetched {
            continue;
        }
        let Some((_, version)) = first_version_assignment(&text) else {
            continue;
        };
        if !pin_version_is_upstream(&version) {
            continue;
        }
        let fname = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let rel = format!("nix/packages/{fname}");
        match upstream_for(&fname) {
            Some((upstream, cargo_hash_attr)) => pins.push(HandPin {
                rel,
                version,
                upstream,
                cargo_hash_attr,
            }),
            None => unknown.push(fname),
        }
    }
    if !unknown.is_empty() {
        unknown.sort();
        return Err(ToolError::fail(format!(
            "update-software: nix/packages pin has no upstream mapping: {}",
            unknown.join(", ")
        )));
    }
    pins.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(pins)
}

fn first_version_assignment(text: &str) -> Option<(String, String)> {
    replace_version_assignment(text, "0").ok()
}

fn upstream_for(file: &str) -> Option<(Upstream, Option<String>)> {
    let gh = |owner: &str, repo: &str| Upstream::GithubRelease {
        owner: owner.to_string(),
        repo: repo.to_string(),
    };
    match file {
        "stalwart-mail.nix" => Some((gh("stalwartlabs", "stalwart"), None)),
        "stalwart-cli.nix" => Some((gh("stalwartlabs", "cli"), None)),
        "stalwart-webui.nix" => Some((gh("stalwartlabs", "webui"), None)),
        "stalwart-spam-filter.nix" => Some((gh("stalwartlabs", "spam-filter"), None)),
        "vandelay.nix" => Some((gh("stalwartlabs", "vandelay"), None)),
        "lean4.nix" => Some((gh("leanprover", "lean4"), None)),
        "wasm-bindgen-cli.nix" => Some((gh("rustwasm", "wasm-bindgen"), None)),
        "arti-onion-service.nix" => Some((
            Upstream::GitlabTag {
                api_project: "tpo%2Fcore%2Farti".to_string(),
                tag_prefix: "arti-v".to_string(),
            },
            Some("arti-onion-service".to_string()),
        )),
        _ => None,
    }
}

fn curl_get(url: &str) -> Result<String, ToolError> {
    let out = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--user-agent",
            "surmount-update-software",
            url,
        ])
        .output()?;
    if !out.status.success() {
        return Err(ToolError::fail(format!(
            "update-software: curl failed for {url}"
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn run_recipe(name: &str, args: &[String]) -> Result<(), ToolError> {
    let kind = recipe_kind(name).ok_or_else(|| {
        ToolError::fail(format!("surmount-deploy-host: unknown recipe app {name}"))
    })?;
    match kind {
        RecipeKind::Deploy { dry_run } => {
            let mut prefix = Vec::new();
            if dry_run {
                prefix.push("--dry-run".to_string());
            }
            prefix.push("--target".to_string());
            prefix.push(DEPLOY_TARGET.to_string());
            prefix.push("--host-local".to_string());
            prefix.push(DEPLOY_HOST_LOCAL.to_string());
            prefix.extend(args.iter().cloned());
            exec_canonical(&prefix)
        }
        RecipeKind::DeployHost => exec_canonical(args),
        RecipeKind::NixBuild {
            suffix,
            remote,
            label,
            warn_dirty,
        } => {
            let root = discover_repo_root();
            let system = flake_system()?;
            let installable = format!(".#checks.{system}.{suffix}");
            if remote {
                remote_nix_build(&root, &installable, label)
            } else {
                local_nix_build(&root, &installable, warn_dirty)
            }
        }
        RecipeKind::NixRun { app, prefix } => {
            let root = discover_repo_root();
            let mut cmd_args = vec!["run".to_string(), format!(".#{app}"), "--".to_string()];
            cmd_args.extend(prefix.iter().map(|s| (*s).to_string()));
            cmd_args.extend(args.iter().cloned());
            run_in(&root, "nix", &cmd_args, &[])
        }
        RecipeKind::CargoFmt { write } => cargo_fmt(write),
        RecipeKind::CargoTest => {
            let root = discover_repo_root();
            run_in(&root, "cargo", &["test".to_string()], &[])
        }
        RecipeKind::CargoClippy => {
            let root = discover_repo_root();
            run_in(
                &root,
                "cargo",
                &[
                    "clippy".to_string(),
                    "--all-targets".to_string(),
                    "--".to_string(),
                    "-D".to_string(),
                    "warnings".to_string(),
                ],
                &[],
            )
        }
        RecipeKind::Dev => run_dev(),
        RecipeKind::Publish => run_publish(args),
        RecipeKind::Btop => run_btop(args),
        RecipeKind::HostStatus => run_host_status(args),
        RecipeKind::Scram => run_scram(args),
        RecipeKind::EvalFile { file } => {
            let root = discover_repo_root();
            run_in(
                &root,
                "nix",
                &[
                    "eval".to_string(),
                    "--impure".to_string(),
                    "--json".to_string(),
                    "--file".to_string(),
                    file.to_string(),
                ],
                &[],
            )
        }
        RecipeKind::RemoteNixBuild => {
            if args.len() < 2 {
                return Err(ToolError {
                    message: "remote-nix-build: need installable and label".into(),
                    exit_code: 2,
                });
            }
            let root = discover_repo_root();
            remote_nix_build(&root, &args[0], &args[1])
        }
        RecipeKind::CutoverStep => {
            let step = args.first().cloned().ok_or_else(|| ToolError {
                message: "cutover-step: need a step argument".into(),
                exit_code: 2,
            })?;
            let mut prefixed = vec!["--step".to_string(), step];
            prefixed.extend(args.iter().cloned());
            let root = discover_repo_root();
            let mut cmd_args = vec![
                "run".to_string(),
                ".#surmount-host-cutover".to_string(),
                "--".to_string(),
            ];
            cmd_args.extend(prefixed);
            run_in(&root, "nix", &cmd_args, &[])
        }
        RecipeKind::DefaultList => {
            let root = discover_repo_root();
            let justfile = root.join("justfile");
            run_in(
                &root,
                "just",
                &[
                    "--list".to_string(),
                    "--justfile".to_string(),
                    justfile.display().to_string(),
                    "--working-directory".to_string(),
                    root.display().to_string(),
                ],
                &[],
            )
        }
        RecipeKind::Vendor => crate::vendor_recipe::run(),
    }
}

#[derive(Clone, Copy)]
enum RecipeKind {
    Deploy {
        dry_run: bool,
    },
    DeployHost,
    NixBuild {
        suffix: &'static str,
        remote: bool,
        label: &'static str,
        warn_dirty: bool,
    },
    NixRun {
        app: &'static str,
        prefix: &'static [&'static str],
    },
    CargoFmt {
        write: bool,
    },
    CargoTest,
    CargoClippy,
    Dev,
    Publish,
    Btop,
    HostStatus,
    Scram,
    EvalFile {
        file: &'static str,
    },
    RemoteNixBuild,
    CutoverStep,
    DefaultList,
    Vendor,
}

fn recipe_kind(name: &str) -> Option<RecipeKind> {
    Some(match name {
        "deploy" => RecipeKind::Deploy { dry_run: false },
        "deploy-dry-run" => RecipeKind::Deploy { dry_run: true },
        "deploy-host" => RecipeKind::DeployHost,
        "ci" => RecipeKind::NixBuild {
            suffix: "ci",
            remote: false,
            label: "ci",
            warn_dirty: false,
        },
        "check-remote" => RecipeKind::NixBuild {
            suffix: "ci",
            remote: true,
            label: "check-remote",
            warn_dirty: true,
        },
        "audit" => RecipeKind::NixBuild {
            suffix: "cargo-audit",
            remote: false,
            label: "audit",
            warn_dirty: true,
        },
        "audit-remote" => RecipeKind::NixBuild {
            suffix: "cargo-audit",
            remote: true,
            label: "audit-remote",
            warn_dirty: true,
        },
        "deny" => RecipeKind::NixBuild {
            suffix: "cargo-deny-bans",
            remote: false,
            label: "deny",
            warn_dirty: false,
        },
        "check-heavy" => RecipeKind::NixBuild {
            suffix: "mail-vm-test",
            remote: false,
            label: "check-heavy",
            warn_dirty: false,
        },
        "fmt" => RecipeKind::CargoFmt { write: false },
        "fmt-write" => RecipeKind::CargoFmt { write: true },
        "test" => RecipeKind::CargoTest,
        "clippy" => RecipeKind::CargoClippy,
        "dev" => RecipeKind::Dev,
        "publish" => RecipeKind::Publish,
        "btop" => RecipeKind::Btop,
        "host-status" => RecipeKind::HostStatus,
        "scram" => RecipeKind::Scram,
        "remote-nix-build" => RecipeKind::RemoteNixBuild,
        "cutover-step" => RecipeKind::CutoverStep,
        "default" => RecipeKind::DefaultList,
        "vendor" => RecipeKind::Vendor,
        "e2e" => RecipeKind::NixRun {
            app: "e2e",
            prefix: &[],
        },
        "e2e-host" => RecipeKind::NixRun {
            app: "e2e-host",
            prefix: &[],
        },
        "add-stalwart-token" => RecipeKind::NixRun {
            app: "add-stalwart-token",
            prefix: &[],
        },
        "bootstrap-stalwart-token" => RecipeKind::NixRun {
            app: "bootstrap-stalwart-api-token",
            prefix: &[],
        },
        "check-leftover-agent-homes" => RecipeKind::NixRun {
            app: "surmount-leftover-homes",
            prefix: &[],
        },
        "check-private-data" => RecipeKind::NixRun {
            app: "surmount-private-data",
            prefix: &[],
        },
        "check-tls-hybrid" => RecipeKind::NixRun {
            app: "surmount-tls-hybrid",
            prefix: &[],
        },
        "diskstation-afp-mount" => RecipeKind::NixRun {
            app: "surmount-diskstation-afp-mount",
            prefix: &[],
        },
        "diskstation-discover" => RecipeKind::NixRun {
            app: "surmount-diskstation-discover",
            prefix: &[],
        },
        "dns-zone-namecheap" => RecipeKind::NixRun {
            app: "surmount-dns-zone",
            prefix: &[],
        },
        "domain-audit" => RecipeKind::NixRun {
            app: "surmount-domain-audit",
            prefix: &[],
        },
        "et" => RecipeKind::NixRun {
            app: "surmount-et",
            prefix: &[],
        },
        "fix-public-dashboard" => RecipeKind::NixRun {
            app: "surmount-fix-public-dashboard",
            prefix: &[],
        },
        "free-stalwart-public-443" => RecipeKind::NixRun {
            app: "free-stalwart-public-443",
            prefix: &[],
        },
        "grok-oss" => RecipeKind::NixRun {
            app: "surmount-grok-oss",
            prefix: &[],
        },
        "grok-oss-running" => RecipeKind::NixRun {
            app: "surmount-grok-oss",
            prefix: &["--running"],
        },
        "host-cutover" => RecipeKind::NixRun {
            app: "surmount-host-cutover",
            prefix: &[],
        },
        "host-inxi" => RecipeKind::NixRun {
            app: "surmount-inxi-host",
            prefix: &[],
        },
        "host-logs" => RecipeKind::NixRun {
            app: "surmount-host-logs",
            prefix: &[],
        },
        "laptop-renew-cert" => RecipeKind::NixRun {
            app: "surmount-laptop-renew-cert",
            prefix: &[],
        },
        "mailplus-copy-uid" => RecipeKind::NixRun {
            app: "surmount-copy-mailplus-uid",
            prefix: &[],
        },
        "point-stalwart-mail-tls" => RecipeKind::NixRun {
            app: "point-stalwart-mail-tls",
            prefix: &[],
        },
        "rdns-shc" => RecipeKind::NixRun {
            app: "surmount-shc",
            prefix: &[],
        },
        "register-dkim" => RecipeKind::NixRun {
            app: "register-dkim",
            prefix: &[],
        },
        "rekey" => RecipeKind::NixRun {
            app: "surmount-rekey",
            prefix: &[],
        },
        "render-host-profile-acme" => RecipeKind::NixRun {
            app: "surmount-render-host-profile-acme",
            prefix: &[],
        },
        "secrets-export-bw-to-staging" => RecipeKind::NixRun {
            app: "secrets-export-bw-to-staging",
            prefix: &[],
        },
        "secrets-install-host" => RecipeKind::NixRun {
            app: "secrets-install-host",
            prefix: &[],
        },
        "secrets-prompt" => RecipeKind::NixRun {
            app: "surmount-secrets-prompt",
            prefix: &[],
        },
        "stalwart-recovery-unlock" => RecipeKind::NixRun {
            app: "stalwart-recovery-unlock",
            prefix: &[],
        },
        "sync-static-sites-from-ds3018xs" => RecipeKind::NixRun {
            app: "surmount-sync-static-sites",
            prefix: &[],
        },
        "test-btop-eval" => RecipeKind::EvalFile {
            file: "tests/btop-contract.nix",
        },
        "test-eternal-terminal-eval" => RecipeKind::EvalFile {
            file: "tests/eternal-terminal-contract.nix",
        },
        "test-inxi-eval" => RecipeKind::EvalFile {
            file: "tests/inxi-contract.nix",
        },
        "test-logging-eval" => RecipeKind::EvalFile {
            file: "tests/logging-contract.nix",
        },
        "test-perf-tools-eval" => RecipeKind::EvalFile {
            file: "tests/perf-tools-contract.nix",
        },
        "test-remote-builder-eval" => RecipeKind::EvalFile {
            file: "tests/remote-builder-contract.nix",
        },
        "test-sshd-hostkeys-eval" => RecipeKind::EvalFile {
            file: "tests/sshd-hostkeys-contract.nix",
        },
        _ => return None,
    })
}

fn exec_canonical(args: &[String]) -> Result<(), ToolError> {
    let exe = env::current_exe()?;
    let canon = fs::canonicalize(&exe).unwrap_or(exe);
    let status = Command::new(canon).args(args).status()?;
    status_result(status)
}

fn cargo_fmt(write: bool) -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut fmt_args = vec!["fmt".to_string()];
    if !write {
        fmt_args.push("--check".to_string());
    }
    run_in(&root, "cargo", &fmt_args, &[])?;
    let system = flake_system()?;
    let check_flag: &[&str] = if write { &[] } else { &["--check"] };
    let dirs = ["modules", "tests", "hosts", "nix"];
    let mut files = Vec::new();
    for dir in dirs {
        collect_nix(&root.join(dir), &mut files);
    }
    files.sort();
    nixfmt_files(&root, &system, &files, check_flag)?;
    let flake = root.join("flake.nix");
    nixfmt_files(&root, &system, &[flake], check_flag)
}

fn collect_nix(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.is_dir() {
            collect_nix(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("nix") {
            out.push(path);
        }
    }
}

fn nixfmt_files(
    root: &Path,
    system: &str,
    files: &[PathBuf],
    flags: &[&str],
) -> Result<(), ToolError> {
    if files.is_empty() {
        return Ok(());
    }
    const CHUNK: usize = 80;
    for chunk in files.chunks(CHUNK) {
        let mut args = vec![
            "run".to_string(),
            format!(".#formatter.{system}"),
            "--".to_string(),
        ];
        args.extend(flags.iter().map(|s| (*s).to_string()));
        args.extend(chunk.iter().map(|p| p.display().to_string()));
        run_in(root, "nix", &args, &[])?;
    }
    Ok(())
}

fn run_dev() -> Result<(), ToolError> {
    let root = discover_repo_root();
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-p",
        "surmount-management-ui",
        "--bin",
        "surmount-management-ui",
    ])
    .current_dir(&root);
    set_default_env(&mut cmd, "SURMOUNT_LISTEN", "127.0.0.1:8080");
    set_default_env(&mut cmd, "SURMOUNT_PRIMARY_DOMAIN", "demo.local");
    set_default_env(&mut cmd, "SURMOUNT_MAIL_HOSTNAME", "mail.demo.local");
    set_default_env(
        &mut cmd,
        "SURMOUNT_SERVICES_HOSTNAME",
        "services.demo.local",
    );
    status_result(cmd.status()?)
}

fn set_default_env(cmd: &mut Command, key: &str, value: &str) {
    if !env_nonempty(key) {
        cmd.env(key, value);
    }
}

fn run_publish(args: &[String]) -> Result<(), ToolError> {
    let root = discover_repo_root();
    run_in(
        &root,
        "nice",
        &[
            "-n".to_string(),
            "19".to_string(),
            "nix".to_string(),
            "flake".to_string(),
            "update".to_string(),
            "surmount-site".to_string(),
        ],
        &[],
    )?;
    let out = Command::new("nice")
        .args([
            "-n",
            "19",
            "nix",
            "build",
            "--print-build-logs",
            "--no-link",
            "--print-out-paths",
            ".#surmount-public-site",
        ])
        .current_dir(&root)
        .output()?;
    if !out.status.success() {
        let _ = std::io::stderr().write_all(&out.stderr);
        return Err(ToolError::fail(
            "publish: nix build .#surmount-public-site failed",
        ));
    }
    let root_path = String::from_utf8_lossy(&out.stdout);
    let root_path = root_path.lines().last().unwrap_or("").trim();
    if root_path.is_empty() {
        return Err(ToolError::fail(
            "publish: public site build printed no store path",
        ));
    }
    let mut cmd_args = vec![
        "run".to_string(),
        ".#surmount-deploy-static-sites".to_string(),
        "--".to_string(),
    ];
    cmd_args.extend(args.iter().cloned());
    run_in(
        &root,
        "nix",
        &cmd_args,
        &[("SURMOUNT_PUBLIC_SITE_ROOT", root_path)],
    )
}

fn run_btop(args: &[String]) -> Result<(), ToolError> {
    if Path::new("/etc/surmount/root-justfile").is_file() {
        return run_in(&discover_repo_root(), "btop", args, &[]);
    }
    let home = env::var("HOME").unwrap_or_default();
    let identity = env::var("SURMOUNT_DEPLOY_SSH_IDENTITY")
        .unwrap_or_else(|_| format!("{home}/.ssh/id_ed25519"));
    let et = format!("{home}/.nix-profile/bin/et");
    let mut cmd = vec![
        "-k".to_string(),
        "5".to_string(),
        "-p".to_string(),
        "2022".to_string(),
        "--ssh-option".to_string(),
        format!("IdentityFile={identity}"),
        "-e".to_string(),
        "-c".to_string(),
        "btop".to_string(),
        DEPLOY_TARGET.to_string(),
    ];
    cmd.extend(args.iter().cloned());
    run_in(&discover_repo_root(), &et, &cmd, &[])
}

fn run_host_status(args: &[String]) -> Result<(), ToolError> {
    let mut target = env::var("SURMOUNT_DEPLOY_TARGET").unwrap_or_default();
    if target.is_empty() {
        if let Some(from_file) = target_from_agent_env() {
            target = from_file;
        }
    }
    let mut rest = args.to_vec();
    if rest.first().map(String::as_str) == Some("--target") {
        target = rest.get(1).cloned().unwrap_or_default();
        rest.drain(0..2.min(rest.len()));
    }
    if target.is_empty() {
        return Err(ToolError {
            message: "host-status: set SURMOUNT_DEPLOY_TARGET or --target USER@HOST".into(),
            exit_code: 2,
        });
    }
    let remote = crate::recipe_batch_03::host_status_remote_script();
    let mut cmd = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "--".to_string(),
        target,
        remote.to_string(),
    ];
    cmd.extend(rest);
    run_in(&discover_repo_root(), "ssh", &cmd, &[])
}

fn run_scram(args: &[String]) -> Result<(), ToolError> {
    let mut target = env::var("SURMOUNT_DEPLOY_TARGET").unwrap_or_default();
    if target.is_empty() {
        if let Some(from_file) = target_from_agent_env() {
            target = from_file;
        }
    }
    let mut rest = args.to_vec();
    if rest.first().map(String::as_str) == Some("--target") {
        target = rest.get(1).cloned().unwrap_or_default();
        if rest.len() >= 2 {
            rest.drain(0..2);
        } else {
            rest.clear();
        }
    }
    if target.is_empty() {
        target = DEPLOY_TARGET.to_string();
    }
    eprintln!("scram: {target} surmount-scram --now");
    let mut cmd = vec![
        "-o".to_string(),
        "BatchMode=yes".to_string(),
        "-o".to_string(),
        "ConnectTimeout=8".to_string(),
        "--".to_string(),
        target,
        "--".to_string(),
        "surmount-scram".to_string(),
        "--now".to_string(),
    ];
    cmd.extend(rest);
    run_in(&discover_repo_root(), "ssh", &cmd, &[])
}

fn target_from_agent_env() -> Option<String> {
    let home = env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".local/share/surmount/agent-target.env");
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        if let Some(val) = line.strip_prefix("SURMOUNT_DEPLOY_TARGET=") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn local_nix_build(root: &Path, installable: &str, warn_dirty: bool) -> Result<(), ToolError> {
    let mut args = vec!["build".to_string(), "--print-build-logs".to_string()];
    if warn_dirty {
        args.push("--option".to_string());
        args.push("warn-dirty".to_string());
        args.push("false".to_string());
    }
    args.push(installable.to_string());
    run_in(root, "nix", &args, &[])
}

fn remote_nix_build(root: &Path, installable: &str, label: &str) -> Result<(), ToolError> {
    if env_nonempty("BUILD_LOCAL") {
        return Err(ToolError {
            message: format!("just {label}: refuse BUILD_LOCAL (that is laptop rustc)"),
            exit_code: 2,
        });
    }
    eprintln!("==> just {label}: nix build {installable} (max-jobs 0)");
    eprintln!("==> just {label}: eval can take a few minutes; then remote logs or a store path");
    let max = 3u32;
    for attempt in 1..=max {
        let out = Command::new("nix")
            .args([
                "build",
                "--option",
                "max-jobs",
                "0",
                "--option",
                "warn-dirty",
                "false",
                "--print-build-logs",
                "--print-out-paths",
                "--no-link",
                installable,
            ])
            .current_dir(root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut child = out;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        if let Some(mut r) = child.stdout.take() {
            r.read_to_end(&mut stdout)?;
        }
        if let Some(mut r) = child.stderr.take() {
            r.read_to_end(&mut stderr)?;
        }
        let _ = std::io::stderr().write_all(&stderr);
        let status = child.wait()?;
        if status.success() {
            eprintln!("==> just {label}: ok");
            let _ = std::io::stdout().write_all(&stdout);
            return Ok(());
        }
        let log = String::from_utf8_lossy(&stderr);
        if is_connection_drop(&log) && attempt < max {
            eprintln!(
                "==> just {label}: connection failed (attempt {attempt} of {max}); retrying the nix build"
            );
            thread::sleep(Duration::from_secs(2));
            continue;
        }
        if is_connection_drop(&log) {
            eprintln!("==> just {label}: connection failed after {max} attempts");
            return Err(ToolError::fail(format!("just {label}: connection failed")));
        }
        eprintln!("==> just {label}: build failed (not a connection drop); not retrying");
        return Err(ToolError::fail(format!("just {label}: build failed")));
    }
    Err(ToolError::fail(format!(
        "just {label}: connection failed after {max} attempts"
    )))
}

fn is_connection_drop(log: &str) -> bool {
    if log.contains("builder failed with exit code") || log.contains("Reason: builder failed") {
        return false;
    }
    const NEEDLES: &[&str] = &[
        "failed to start SSH connection",
        "failed to start SSH master connection",
        "unable to open connection to remote store",
        "unexpected end-of-file",
        "unexpected EOF",
        "kex_exchange_identification",
        "Connection reset by peer",
        "Connection timed out",
        "Connection refused",
        "Broken pipe",
        "closed by remote host",
    ];
    if NEEDLES.iter().any(|n| log.contains(n)) {
        return true;
    }
    log.contains("Timeout, server ") && log.contains(" not responding")
}

fn flake_system() -> Result<String, ToolError> {
    if let Ok(sys) = env::var("CI_SYSTEM") {
        if !sys.is_empty() {
            return Ok(sys);
        }
    }
    let out = Command::new("nix")
        .args([
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ])
        .output()?;
    if !out.status.success() {
        return Err(ToolError::fail("nix eval builtins.currentSystem failed"));
    }
    let sys = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sys.is_empty() {
        return Err(ToolError::fail("nix eval builtins.currentSystem was empty"));
    }
    Ok(sys)
}

fn env_nonempty(key: &str) -> bool {
    env::var(key).ok().is_some_and(|v| !v.is_empty())
}

fn run_in(
    root: &Path,
    bin: &str,
    args: &[String],
    extra: &[(&str, &str)],
) -> Result<(), ToolError> {
    let mut cmd = Command::new(bin);
    cmd.args(args).current_dir(root);
    for (k, v) in extra {
        cmd.env(k, v);
    }
    status_result(cmd.status()?)
}

fn status_result(status: std::process::ExitStatus) -> Result<(), ToolError> {
    if status.success() {
        return Ok(());
    }
    let code = status.code().unwrap_or(1);
    Err(ToolError {
        message: format!("command exited {code}"),
        exit_code: code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_url_is_not_a_commit_pin() {
        assert_eq!(commit_pin_of("github:NixOS/nixpkgs/nixos-26.05"), None);
        assert_eq!(commit_pin_of("github:rustsec/advisory-db"), None);
        assert_eq!(commit_pin_of("github:SurmountSystems/site"), None);
    }

    #[test]
    fn literal_rev_is_a_commit_pin() {
        let rev = "fd400cc205c104a499657a82680b6a3eaa8b7d8e";
        assert_eq!(
            commit_pin_of(&format!("github:NixOS/nixpkgs/{rev}")),
            Some(rev.to_string())
        );
        assert!(is_git_commit(rev));
        assert!(!is_git_commit("nixos-26.05"));
        assert!(!is_git_commit("main"));
    }

    #[test]
    fn parse_sample_inputs_splits_branch_and_commit() {
        let text = r#"
        inputs = {
          nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
          nixpkgs-rust.url = "github:NixOS/nixpkgs/fd400cc205c104a499657a82680b6a3eaa8b7d8e";
          advisory-db = {
            url = "github:rustsec/advisory-db";
            flake = false;
          };
          splora.url = "github:SurmountSystems/splora/28eb3ea077eee02bd8a9c0cba63ee9a611b10e54";
        };
        "#;
        let inputs = parse_flake_inputs(text);
        let nixpkgs = inputs.iter().find(|i| i.name == "nixpkgs").unwrap();
        assert!(nixpkgs.commit_pin.is_none());
        let rust = inputs.iter().find(|i| i.name == "nixpkgs-rust").unwrap();
        assert_eq!(
            rust.commit_pin.as_deref(),
            Some("fd400cc205c104a499657a82680b6a3eaa8b7d8e")
        );
        let adv = inputs.iter().find(|i| i.name == "advisory-db").unwrap();
        assert!(adv.commit_pin.is_none());
        let splora = inputs.iter().find(|i| i.name == "splora").unwrap();
        assert!(splora.commit_pin.is_some());
    }

    #[test]
    fn menhera_args_do_not_point_at_crates_io_index() {
        let args = menhera_cargo_update_args().join(" ");
        assert!(args.contains("index.crates.menhera.org/10d/"));
        assert!(args.contains("menhera-cooldown"));
        assert!(!args.contains("index.crates.io"));
        assert!(!args.contains("crates.io-index"));
    }

    #[test]
    fn audit_args_add_no_ignore() {
        let args = cargo_audit_nix_args("x86_64-linux");
        assert!(
            args.iter()
                .any(|a| a == ".#checks.x86_64-linux.cargo-audit")
        );
        assert!(!args.iter().any(|a| a == "--ignore"));
    }

    #[test]
    fn version_assignment_skips_comments() {
        let text = "# version = \"9.9.9\"\n  version = \"1.2.3\";\n";
        let (new_text, old) = replace_version_assignment(text, "1.2.4").unwrap();
        assert_eq!(old, "1.2.3");
        assert!(new_text.contains("version = \"1.2.4\""));
        assert!(new_text.contains("# version = \"9.9.9\""));
    }

    #[test]
    fn crate_versions_are_not_upstream_pins() {
        assert!(!pin_version_is_upstream("0.1.0"));
        assert!(!pin_version_is_upstream("0"));
        assert!(!pin_version_is_upstream("1"));
        assert!(pin_version_is_upstream("1.1.0"));
        assert!(pin_version_is_upstream("2.8.0"));
    }

    #[test]
    fn argv0_selects_update_and_deploy_binary_does_not() {
        let flakes = classify_invocation(&["/nix/store/x/bin/update-flakes".into()]);
        assert_eq!(flakes.0, Dispatch::UpdateFlakes);
        let soft = classify_invocation(&["surmount-deploy-host".into(), "update-software".into()]);
        assert_eq!(soft.0, Dispatch::UpdateSoftware);
        let deploy = classify_invocation(&[
            "surmount-deploy-host".into(),
            "--target".into(),
            "root@example".into(),
        ]);
        assert_eq!(deploy.0, Dispatch::NotOurs);
        let recipe = classify_invocation(&["/nix/store/x/bin/fmt".into()]);
        assert_eq!(recipe.0, Dispatch::Recipe("fmt".into()));
    }

    #[test]
    fn release_tag_strips_v_and_arti_prefix() {
        assert_eq!(release_version("v1.1.0", "").unwrap(), "1.1.0");
        assert_eq!(release_version("arti-v2.8.0", "arti-v").unwrap(), "2.8.0");
    }

    #[test]
    fn known_package_files_have_upstreams() {
        for name in [
            "stalwart-mail.nix",
            "stalwart-cli.nix",
            "stalwart-webui.nix",
            "stalwart-spam-filter.nix",
            "vandelay.nix",
            "arti-onion-service.nix",
            "lean4.nix",
            "wasm-bindgen-cli.nix",
        ] {
            assert!(upstream_for(name).is_some(), "{name}");
        }
    }
}
