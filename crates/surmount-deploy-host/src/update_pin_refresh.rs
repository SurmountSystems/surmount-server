//! Hash refresh for nix/packages pins. Called from update-software.
//! fetchurl lines are updated in update_maintenance. This file covers
//! fetchCrate tarballs and fetchFromGitLab source hashes.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::ToolError;

/// Prefetch a crates.io crate tarball and rewrite the fetchCrate hash.
/// No fetchCrate block: return the text unchanged.
pub fn refresh_crate_tarball(text: &str, version: &str) -> Result<String, ToolError> {
    if !text.contains("fetchCrate") {
        return Ok(text.to_string());
    }
    let pname = pname_of(text)
        .ok_or_else(|| ToolError::fail("update-software: fetchCrate has no pname"))?;
    let url = format!("https://static.crates.io/crates/{pname}/{pname}-{version}.crate");
    let sri = prefetch_sri(&url)?;
    replace_fetchcrate_hash(text, &sri).ok_or_else(|| {
        ToolError::fail(format!(
            "update-software: fetchCrate has no hash line for {pname} {version}"
        ))
    })
}

/// Rebuild a flake package attr and paste the fixed-output `got:` hash.
/// `cargo` false updates the fetchFromGitLab `hash`. `cargo` true updates
/// `cargoHash`. A clean build means the hash already matches.
pub fn refresh_named_hash(
    root: &Path,
    rel: &str,
    installable_attr: Option<&str>,
    cargo: bool,
) -> Result<(), ToolError> {
    let attr = installable_attr.ok_or_else(|| {
        ToolError::fail(format!(
            "update-software: {rel} has no flake attr for hash refresh"
        ))
    })?;
    let installable = format!(".#{attr}");
    eprintln!("update-software: refreshing hash via nix build {installable}");
    let out = Command::new("nix")
        .args(["build", "--no-link", &installable])
        .current_dir(root)
        .output()?;
    if out.status.success() {
        eprintln!("update-software: {rel} hash already matches");
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let Some(got) = sri_got(&err) else {
        return Err(ToolError::fail(format!(
            "update-software: nix build {installable} failed and stderr had no got: hash"
        )));
    };
    let path = root.join(rel);
    let text = fs::read_to_string(&path)?;
    let updated = if cargo {
        replace_keyed(&text, "cargoHash = \"", &got)
    } else {
        replace_gitlab_src_hash(&text, &got)
    };
    let Some(updated) = updated else {
        return Err(ToolError::fail(format!(
            "update-software: {rel} has no hash line to set to {got}"
        )));
    };
    fs::write(&path, updated)?;
    eprintln!("update-software: {rel} hash set from nix build got: line");
    Ok(())
}

fn pname_of(text: &str) -> Option<String> {
    quoted_value_after(text, "pname = ")
}

fn quoted_value_after(text: &str, key: &str) -> Option<String> {
    let idx = text.find(key)?;
    let rest = text[idx + key.len()..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
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

fn json_string_field(body: &str, field: &str) -> Option<String> {
    let key = format!("\"{field}\"");
    let idx = body.find(&key)?;
    let after = &body[idx + key.len()..];
    let colon = after.find(':')?;
    let rest = after[colon + 1..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn replace_fetchcrate_hash(text: &str, sri: &str) -> Option<String> {
    let start = text.find("fetchCrate")?;
    let open = start + text[start..].find('{')?;
    let close = open + text[open..].find('}')?;
    let key = "hash = \"";
    let rel = text[open..=close].find(key)?;
    let val = open + rel + key.len();
    if val > close {
        return None;
    }
    let end = val + text[val..].find('"')?;
    if end > close {
        return None;
    }
    replace_span(text, val, end, sri)
}

fn replace_gitlab_src_hash(text: &str, sri: &str) -> Option<String> {
    let start = text.find("fetchFromGitLab")?;
    let open = start + text[start..].find('{')?;
    let close = open + text[open..].find('}')?;
    let key = "hash = \"";
    let rel = text[open..=close].find(key)?;
    let val = open + rel + key.len();
    let end = val + text[val..].find('"')?;
    if end > close {
        return None;
    }
    replace_span(text, val, end, sri)
}

fn replace_keyed(text: &str, key: &str, sri: &str) -> Option<String> {
    let start = text.find(key)?;
    let val = start + key.len();
    let end = val + text[val..].find('"')?;
    replace_span(text, val, end, sri)
}

fn replace_span(text: &str, val: usize, end: usize, sri: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + sri.len());
    out.push_str(&text[..val]);
    out.push_str(sri);
    out.push_str(&text[end..]);
    Some(out)
}

fn sri_got(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("got:") else {
            continue;
        };
        let hash = rest.trim();
        if hash.starts_with("sha256-") {
            return Some(hash.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetchcrate_hash_rewrite_leaves_cargohash() {
        let text = r#"
let
  cargoHash = "sha256-oldcargo";
in
buildWasmBindgenCli rec {
  src = fetchCrate {
    pname = "wasm-bindgen-cli";
    version = "0.2.128";
    hash = "sha256-oldsrc";
  };
}
"#;
        let updated = replace_fetchcrate_hash(text, "sha256-newsrc").unwrap();
        assert!(updated.contains("hash = \"sha256-newsrc\""));
        assert!(updated.contains("cargoHash = \"sha256-oldcargo\""));
        assert_eq!(pname_of(text).as_deref(), Some("wasm-bindgen-cli"));
    }

    #[test]
    fn gitlab_src_hash_is_inside_fetch_block() {
        let text = r#"
  src = fetchFromGitLab {
    repo = "arti";
    hash = "sha256-oldsrc";
  };
  cargoHash = "sha256-oldcargo";
"#;
        let updated = replace_gitlab_src_hash(text, "sha256-newsrc").unwrap();
        assert!(updated.contains("hash = \"sha256-newsrc\""));
        assert!(updated.contains("cargoHash = \"sha256-oldcargo\""));
    }
}
