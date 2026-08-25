//! Refuse product mkdir/write of leftover agent homes under the repo.
//!
//! Leftover homes: `.agents/{reports,plans,joins}` and `.grok/{reports,plans,joins}`.
//! Host `~/.agents/reports` and `mktemp` / `$TMPDIR` are allowed.
//! In-tree refuse fixtures live under `crates/surmount-leftover-homes/testdata/`.

mod git;

use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
Usage:
  surmount-leftover-homes --tree
  surmount-leftover-homes --paths PATH [PATH ...]

Scans for mkdir/write paths that recreate leftover agent homes under the
repo (.agents/reports, .agents/plans, .agents/joins, .grok/reports,
.grok/plans, .grok/joins). Host ~/.agents/reports is allowed.

  --tree    All tracked files (excludes crate testdata fixtures).
  --paths   Explicit paths (fixtures; does not exclude testdata).
";

pub const REFUSAL_FOOTER: &str = "\
surmount-leftover-homes: leftover home mkdir/write path(s) found
  Use mktemp / $TMPDIR, or host ~/.agents/reports/ for agent notes.
  In-tree refuse fixtures: crates/surmount-leftover-homes/testdata/, never repo .agents/ or .grok/ homes.
";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Help,
    Tree,
    Paths(Vec<PathBuf>),
}

#[derive(Debug)]
pub struct ScanError {
    pub message: String,
    pub exit_code: i32,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ScanError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub class: &'static str,
    pub path: String,
}

pub fn parse_args<I, S>(args: I) -> Result<Mode, ScanError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    let _argv0 = iter.next();
    let rest: Vec<String> = iter.map(|s| s.as_ref().to_string()).collect();
    let mut i = 0;
    let mut mode: Option<Mode> = None;
    while i < rest.len() {
        match rest[i].as_str() {
            "-h" | "--help" => return Ok(Mode::Help),
            "--tree" => {
                mode = Some(Mode::Tree);
                i += 1;
            }
            "--paths" => {
                i += 1;
                let mut paths = Vec::new();
                while i < rest.len() {
                    if rest[i].starts_with("--") {
                        break;
                    }
                    paths.push(PathBuf::from(&rest[i]));
                    i += 1;
                }
                if paths.is_empty() {
                    return Err(ScanError {
                        message: "--paths requires at least one path".into(),
                        exit_code: 1,
                    });
                }
                mode = Some(Mode::Paths(paths));
            }
            other => {
                return Err(ScanError {
                    message: format!("unknown argument: {other}"),
                    exit_code: 1,
                });
            }
        }
    }
    mode.ok_or_else(|| ScanError {
        message: "--tree or --paths is required".into(),
        exit_code: 1,
    })
}

pub fn repo_root() -> PathBuf {
    git::toplevel().unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn is_fixture_path(rel: &str) -> bool {
    let p = rel.trim_start_matches("./");
    p == "script/testdata/leftover-agent-homes"
        || p.starts_with("script/testdata/leftover-agent-homes/")
        || p == "crates/surmount-leftover-homes/testdata"
        || p.starts_with("crates/surmount-leftover-homes/testdata/")
}

pub fn is_scanner_tool_path(rel: &str) -> bool {
    let p = rel.trim_start_matches("./");
    p == "script/check-leftover-agent-homes.sh"
        || p == "script/test-check-leftover-agent-homes.sh"
        || p == "nix/packages/surmount-leftover-homes.nix"
        || p == "crates/surmount-leftover-homes"
        || p.starts_with("crates/surmount-leftover-homes/")
}

fn to_repo_rel(path: &Path, root: &Path) -> String {
    if let Ok(abs) = path.canonicalize() {
        if let Ok(stripped) = abs.strip_prefix(root) {
            return stripped.to_string_lossy().replace('\\', "/");
        }
    }
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(root.to_string_lossy().as_ref()) {
        return stripped.trim_start_matches('/').replace('\\', "/");
    }
    s.trim_start_matches("./").replace('\\', "/")
}

const ROOT_VARS: &[&str] = &[
    "${ROOT}",
    "${root}",
    "${REPO_ROOT}",
    "${repo_root}",
    "${PWD}",
    "${pwd}",
    "$ROOT",
    "$root",
    "$REPO_ROOT",
    "$repo_root",
    "$PWD",
    "$(pwd)",
];

pub fn has_root_leftover(text: &str) -> bool {
    for v in ROOT_VARS {
        let agents = format!("{v}/.agents");
        let grok = format!("{v}/.grok");
        if text.contains(&agents) || text.contains(&grok) {
            return true;
        }
    }
    false
}

pub fn has_rel_mkdir_leftover(text: &str) -> bool {
    for line in text.lines() {
        let t = line.trim();
        if !t.contains("mkdir") {
            continue;
        }
        // Relative leftover homes from repo root, not ~/.agents.
        if t.contains("~/.agents") || t.contains("~/.grok") {
            continue;
        }
        if t.contains("mkdir")
            && (contains_rel_home(t, ".agents") || contains_rel_home(t, ".grok"))
        {
            return true;
        }
    }
    false
}

fn contains_rel_home(line: &str, home: &str) -> bool {
    // mkdir ... .agents or ".agents or '.agents after mkdir, not ${X}/.agents
    let Some(idx) = line.find("mkdir") else {
        return false;
    };
    let after = &line[idx..];
    for needle in [
        format!(" {home}"),
        format!("/{home}"),
        format!("\"{home}"),
        format!("'{home}"),
        format!(" {home}/"),
    ] {
        if after.contains(&needle) {
            // Avoid ${ROOT}/.agents (root-relative class) matching as relative mkdir.
            if after.contains(&format!("}}/{home}")) || after.contains(&format!(")/{home}")) {
                continue;
            }
            return true;
        }
    }
    false
}

pub fn scan_text(rel: &str, text: &str) -> Vec<Hit> {
    let mut hits = Vec::new();
    if has_root_leftover(text) {
        hits.push(Hit {
            class: "root-relative-leftover-home",
            path: rel.to_string(),
        });
    }
    if has_rel_mkdir_leftover(text) {
        hits.push(Hit {
            class: "relative-mkdir-leftover-home",
            path: rel.to_string(),
        });
    }
    hits
}

pub fn scan_named(rel: &str, data: &[u8]) -> Vec<Hit> {
    if data.contains(&0) {
        return Vec::new();
    }
    scan_text(rel, &String::from_utf8_lossy(data))
}

pub fn dispatch(mode: Mode) -> i32 {
    match mode {
        Mode::Help => {
            print!("{USAGE}");
            0
        }
        Mode::Paths(paths) => scan_paths(&paths, true),
        Mode::Tree => scan_tree(),
    }
}

fn scan_paths(paths: &[PathBuf], include_fixtures: bool) -> i32 {
    let root = repo_root();
    let mut hits = Vec::new();
    for p in paths {
        let rel = to_repo_rel(p, &root);
        if !include_fixtures && (is_fixture_path(&rel) || is_scanner_tool_path(&rel)) {
            continue;
        }
        let abs = if p.is_absolute() {
            p.clone()
        } else {
            root.join(&rel)
        };
        if !abs.is_file() {
            continue;
        }
        if let Ok(data) = std::fs::read(&abs) {
            hits.extend(scan_named(&rel, &data));
        }
    }
    report(&hits)
}

fn scan_tree() -> i32 {
    let root = repo_root();
    if !git::in_work_tree() {
        eprintln!("surmount-leftover-homes: --tree requires a git work tree");
        return 1;
    }
    let files = match git::ls_files() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("surmount-leftover-homes: {e}");
            return 1;
        }
    };
    let mut hits = Vec::new();
    for rel in files {
        if is_fixture_path(&rel) || is_scanner_tool_path(&rel) {
            continue;
        }
        let abs = root.join(&rel);
        if !abs.is_file() {
            continue;
        }
        if let Ok(data) = std::fs::read(&abs) {
            hits.extend(scan_named(&rel, &data));
        }
    }
    report(&hits)
}

fn report(hits: &[Hit]) -> i32 {
    if hits.is_empty() {
        return 0;
    }
    for h in hits {
        eprintln!("leftover-agent-home: {}  {}", h.class, h.path);
    }
    eprint!("{REFUSAL_FOOTER}");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_mktemp_is_clean() {
        let t = include_str!("../testdata/good-mktemp.sh");
        assert!(scan_text("good-mktemp.sh", t).is_empty());
    }

    #[test]
    fn bad_root_hits() {
        let t = include_str!("../testdata/bad-mkdir-root.sh");
        let hits = scan_text("bad-mkdir-root.sh", t);
        assert!(
            hits.iter()
                .any(|h| h.class == "root-relative-leftover-home"),
            "{hits:?}"
        );
    }

    #[test]
    fn bad_rel_mkdir_hits() {
        let t = include_str!("../testdata/bad-mkdir-relative.sh");
        let hits = scan_text("bad-mkdir-relative.sh", t);
        assert!(
            hits.iter()
                .any(|h| h.class == "relative-mkdir-leftover-home"),
            "{hits:?}"
        );
    }

    #[test]
    fn parse_requires_mode() {
        assert!(parse_args(["surmount-leftover-homes"]).is_err());
    }
}
