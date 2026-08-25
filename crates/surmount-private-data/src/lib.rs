//! Private-data pattern scanner for Surmount Server git admission.
//!
//! Patterns only. Prints class + path on hit, never the matching line.
//! Synthetic fixtures live under `testdata/` and `script/testdata/private-data/`.

mod args;
mod git;
mod scan;

use std::path::{Path, PathBuf};

pub use args::{Action, Mode, ParseError, parse_args};
pub use scan::{Hit, scan_named};

/// Footer printed on stderr after one or more hits.
pub const REFUSAL_FOOTER: &str = "\n\
surmount-private-data: refused. Private-data pattern class matched.\n\
  Remove the material from the index / tree. Keep keys, tokens, PEMs, age\n\
  identities, and real host public IPs on the host only (never in this public\n\
  git tree). Use TEST-NET (203.0.113.0/24, 198.51.100.0/24, 192.0.2.0/24),\n\
  loopback, and short placeholders in committed samples.\n\
  See docs/hygiene.md (pre-commit private-data scan) and docs/SECRETS.md.\n";

#[derive(Debug)]
pub enum ScanError {
    Usage(String),
    Tool(String),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Usage(s) | ScanError::Tool(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for ScanError {}

pub fn usage() -> &'static str {
    "Usage:\n  \
     surmount-private-data [--staged]\n  \
     surmount-private-data --tree\n  \
     surmount-private-data --paths PATH [PATH ...]\n\n\
     Scans for private-data pattern classes (PEM, age secret, tokens, secret\n\
     basenames, path-gated public IPv4 under hosts/, long SSH public keys).\n\
     Patterns only: no real secrets are stored in this tool.\n\n  \
     --staged   Staged files only (git diff --cached). Default.\n  \
     --tree     All tracked files (excludes detector fixtures and this crate).\n  \
     --paths    Explicit paths (fixtures; does not exclude testdata).\n"
}

/// Format a hit the way the CLI prints it (class + path, not the secret).
pub fn format_hit(hit: &Hit) -> String {
    format!("private-data: {}  {}", hit.class, hit.path)
}

/// Resolve git toplevel, or the current directory when git is absent.
pub fn repo_root() -> Result<PathBuf, ScanError> {
    if let Some(root) = git::toplevel() {
        return Ok(root);
    }
    std::env::current_dir().map_err(|e| ScanError::Tool(format!("current directory: {e}")))
}

pub fn is_fixture_path(rel: &str) -> bool {
    let p = rel.trim_start_matches("./");
    p == "script/testdata/private-data"
        || p.starts_with("script/testdata/private-data/")
        || p == "crates/surmount-private-data/testdata"
        || p.starts_with("crates/surmount-private-data/testdata/")
}

pub fn is_scanner_tool_path(rel: &str) -> bool {
    let p = rel.trim_start_matches("./");
    p == "script/check-private-data.sh"
        || p == "script/test-check-private-data.sh"
        || p == "nix/packages/surmount-private-data.nix"
        || p == "crates/surmount-private-data"
        || p.starts_with("crates/surmount-private-data/")
}

pub fn to_repo_rel(path: &Path, root: &Path) -> String {
    if path.is_absolute() {
        if let Ok(stripped) = path.strip_prefix(root) {
            return stripped.to_string_lossy().replace('\\', "/");
        }
        return path.to_string_lossy().replace('\\', "/");
    }
    let s = path.to_string_lossy();
    s.strip_prefix("./").unwrap_or(&s).replace('\\', "/")
}

/// Scan according to CLI mode. Returns hits (empty means clean).
pub fn run(mode: &Mode, root: &Path) -> Result<Vec<Hit>, ScanError> {
    let files = collect_files(mode, root)?;
    let mut hits = Vec::new();
    for path in files {
        hits.extend(scan_one(mode, root, &path)?);
    }
    Ok(hits)
}

fn collect_files(mode: &Mode, root: &Path) -> Result<Vec<PathBuf>, ScanError> {
    match mode {
        Mode::Staged => {
            if !git::in_work_tree() {
                return Err(ScanError::Tool(
                    "--staged requires a git work tree".to_string(),
                ));
            }
            let names = git::staged_names().map_err(ScanError::Tool)?;
            Ok(names
                .into_iter()
                .filter(|n| !is_fixture_path(n) && !is_scanner_tool_path(n))
                .map(PathBuf::from)
                .collect())
        }
        Mode::Tree => {
            if !git::in_work_tree() {
                return Err(ScanError::Tool(
                    "--tree requires a git work tree".to_string(),
                ));
            }
            let names = git::ls_files().map_err(ScanError::Tool)?;
            Ok(names
                .into_iter()
                .filter(|n| !is_fixture_path(n) && !is_scanner_tool_path(n))
                .map(PathBuf::from)
                .collect())
        }
        Mode::Paths(given) => {
            if given.is_empty() {
                return Err(ScanError::Usage(
                    "--paths requires at least one path".to_string(),
                ));
            }
            let mut files = Vec::new();
            for p in given {
                if p.exists() {
                    files.push(p.clone());
                    continue;
                }
                let under_root = root.join(p);
                if under_root.exists() {
                    files.push(under_root);
                    continue;
                }
                return Err(ScanError::Tool(format!("path not found: {}", p.display())));
            }
            if files.is_empty() {
                return Err(ScanError::Usage("--paths matched no files".to_string()));
            }
            Ok(files)
        }
    }
}

fn scan_one(mode: &Mode, root: &Path, path: &Path) -> Result<Vec<Hit>, ScanError> {
    let rel = to_repo_rel(path, root);
    let bytes = match mode {
        Mode::Staged => git::show_index(&rel).unwrap_or_default(),
        Mode::Tree | Mode::Paths(_) => read_worktree_file(path, root),
    };
    Ok(scan_named(&rel, &bytes, mode.gate()))
}

fn read_worktree_file(path: &Path, root: &Path) -> Vec<u8> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else if path.exists() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    std::fs::read(&abs).unwrap_or_default()
}

/// Run parsed CLI action. Exit code 0 clean / help, 1 on hit or error.
pub fn dispatch(action: Action) -> i32 {
    match action {
        Action::Help => {
            eprint!("{}", usage());
            0
        }
        Action::Run(mode) => match repo_root().and_then(|root| run(&mode, &root)) {
            Ok(hits) => {
                if hits.is_empty() {
                    0
                } else {
                    for h in &hits {
                        eprintln!("{}", format_hit(h));
                    }
                    eprint!("{REFUSAL_FOOTER}");
                    1
                }
            }
            Err(e) => {
                eprintln!("surmount-private-data: {e}");
                if matches!(e, ScanError::Usage(_)) {
                    eprint!("{}", usage());
                }
                1
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn testdata() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata")
    }

    fn load(name: &str) -> (String, Vec<u8>) {
        let p = testdata().join(name);
        let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
        (name.to_string(), bytes)
    }

    fn classes(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.class).collect()
    }

    #[test]
    fn bad_pem_fixture_is_hit() {
        let (name, bytes) = load("bad-pem.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(
            classes(&hits).contains(&"pem-or-openssh-private-key"),
            "{hits:?}"
        );
    }

    #[test]
    fn bad_age_fixture_is_hit() {
        let (name, bytes) = load("bad-age.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(classes(&hits).contains(&"age-secret-key"), "{hits:?}");
    }

    #[test]
    fn bad_token_ghp_fixture_is_hit() {
        let (name, bytes) = load("bad-token-ghp.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(classes(&hits).contains(&"api-token-shape"), "{hits:?}");
    }

    #[test]
    fn bad_public_ipv4_fixture_is_hit() {
        let (name, bytes) = load("bad-public-ipv4.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(classes(&hits).contains(&"hosts-public-ipv4"), "{hits:?}");
    }

    #[test]
    fn bad_ssh_pub_fixture_is_hit() {
        let (name, bytes) = load("bad-ssh-pub.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(classes(&hits).contains(&"long-ssh-public-key"), "{hits:?}");
    }

    #[test]
    fn bad_secret_assign_fixture_is_hit() {
        let (name, bytes) = load("bad-secret-assign.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(classes(&hits).contains(&"secret-assignment"), "{hits:?}");
    }

    #[test]
    fn bad_nsec_fixture_is_hit() {
        let (name, bytes) = load("bad-nsec.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(classes(&hits).contains(&"nostr-nsec"), "{hits:?}");
    }

    #[test]
    fn good_host_sample_passes() {
        let (name, bytes) = load("good-host-sample.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn basename_id_ed25519_is_hit() {
        let hits = scan_named("id_ed25519", b"", scan::Gate::Paths);
        assert!(
            classes(&hits).contains(&"secret-basename:ssh-private-name"),
            "{hits:?}"
        );
    }

    #[test]
    fn secrets_readme_basename_is_allowed() {
        let hits = scan_named("secrets/README.md", b"# layout only\n", scan::Gate::Product);
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn allowed_testnet_and_rfc1918_ipv4_are_not_hits() {
        let body = b"203.0.113.10 198.51.100.1 192.0.2.1 10.0.0.1 192.168.1.1 172.16.0.1 127.0.0.1 0.0.0.0 169.254.1.1\n";
        let hits = scan_named("hosts/mail-vps/configuration.nix", body, scan::Gate::Product);
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn public_certificate_pem_is_not_private_key_class() {
        let body = b"-----BEGIN CERTIFICATE-----\nMIIBfake\n-----END CERTIFICATE-----\n";
        let hits = scan_named("docs/sample.txt", body, scan::Gate::Product);
        assert!(
            !classes(&hits).contains(&"pem-or-openssh-private-key"),
            "{hits:?}"
        );
    }

    #[test]
    fn tree_mode_does_not_apply_hosts_ipv4_outside_hosts() {
        let body = b"address = \"1.2.3.4\";\n";
        let hits = scan_named("docs/example.md", body, scan::Gate::Product);
        assert!(!classes(&hits).contains(&"hosts-public-ipv4"), "{hits:?}");
    }

    #[test]
    fn format_hit_does_not_include_secret_bytes() {
        let (name, bytes) = load("bad-token-ghp.txt");
        let hits = scan_named(&name, &bytes, scan::Gate::Paths);
        let line = format_hit(&hits[0]);
        assert!(!line.contains("ghp_"), "{line}");
        assert!(line.contains("api-token-shape"), "{line}");
        assert!(!line.contains("FakeNotARealGitHubToken"), "{line}");
    }

    #[test]
    fn fixture_and_crate_paths_are_excluded_from_product_modes() {
        assert!(is_fixture_path("script/testdata/private-data/bad-pem.txt"));
        assert!(is_fixture_path(
            "crates/surmount-private-data/testdata/bad-pem.txt"
        ));
        assert!(is_scanner_tool_path(
            "crates/surmount-private-data/src/scan.rs"
        ));
        assert!(!is_fixture_path("hosts/mail-vps/configuration.nix"));
    }
}
