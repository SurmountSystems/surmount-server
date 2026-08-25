use std::path::PathBuf;
use std::process::Command;

fn git() -> Command {
    Command::new("git")
}

pub fn in_work_tree() -> bool {
    git()
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn toplevel() -> Option<PathBuf> {
    let out = git().args(["rev-parse", "--show-toplevel"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(PathBuf::from(s))
    }
}

pub fn staged_names() -> Result<Vec<String>, String> {
    let out = git()
        .args(["diff", "--cached", "--name-only", "--diff-filter=ACM"])
        .output()
        .map_err(|e| format!("git diff --cached: {e}"))?;
    if !out.status.success() {
        return Err("git diff --cached failed".to_string());
    }
    Ok(lines(&out.stdout))
}

pub fn ls_files() -> Result<Vec<String>, String> {
    let out = git()
        .args(["ls-files"])
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !out.status.success() {
        return Err("git ls-files failed".to_string());
    }
    Ok(lines(&out.stdout))
}

pub fn show_index(rel: &str) -> Option<Vec<u8>> {
    let spec = format!(":{rel}");
    let out = git().args(["show", &spec]).output().ok()?;
    if out.status.success() {
        Some(out.stdout)
    } else {
        None
    }
}

fn lines(stdout: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}
