//! CLI contracts matching the former bash scanner harness.
//! Synthetic fixtures only. Never print secret payloads in asserts beyond class names.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-private-data")
}

fn crate_testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

fn script_testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../script/testdata/private-data")
}

fn run_paths(paths: &[&Path]) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--paths");
    for p in paths {
        cmd.arg(p);
    }
    cmd.output().expect("run surmount-private-data")
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "surmount-private-data-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn cli_help_exits_0() {
    let out = Command::new(bin()).arg("--help").output().expect("help");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_unknown_arg_exits_1() {
    let out = Command::new(bin())
        .arg("--not-a-flag")
        .output()
        .expect("unknown");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn cli_paths_good_host_sample_exits_0() {
    let p = crate_testdata().join("good-host-sample.txt");
    let out = run_paths(&[&p]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_paths_bad_pem_exits_1_without_printing_key_material() {
    let p = crate_testdata().join("bad-pem.txt");
    let out = run_paths(&[&p]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(err.contains("pem-or-openssh-private-key"), "{err}");
    assert!(!err.contains("FAKEPRIVATEKEYMATERIALNOTREAL"), "{err}");
    assert!(
        !stdout.contains("FAKEPRIVATEKEYMATERIALNOTREAL"),
        "{stdout}"
    );
}

#[test]
fn cli_paths_each_bad_fixture_exits_nonzero() {
    let dir = crate_testdata();
    let mut found = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("bad-") && name.ends_with(".txt") {
            found += 1;
            let out = run_paths(&[&entry.path()]);
            assert_eq!(
                out.status.code(),
                Some(1),
                "{name} should fail: stderr={}",
                String::from_utf8_lossy(&out.stderr)
            );
            let err = String::from_utf8_lossy(&out.stderr);
            assert!(
                err.contains("private-data:"),
                "{name} should print class+path: {err}"
            );
        }
    }
    assert!(
        found >= 6,
        "expected several bad-*.txt fixtures, got {found}"
    );
}

#[test]
fn cli_multi_bad_paths_exits_nonzero() {
    let dir = crate_testdata();
    let pem = dir.join("bad-pem.txt");
    let age = dir.join("bad-age.txt");
    let out = run_paths(&[&pem, &age]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn cli_paths_missing_file_exits_1() {
    let out = Command::new(bin())
        .args([
            "--paths",
            "/tmp/surmount-private-data-does-not-exist-xyz.txt",
        ])
        .output()
        .expect("missing");
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn cli_basename_id_ed25519_exits_nonzero() {
    let dir = temp_dir("basename");
    let key = dir.join("id_ed25519");
    fs::write(&key, b"").unwrap();
    let out = run_paths(&[&key]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("secret-basename:ssh-private-name"), "{err}");
}

/// When the git-tree copy of script fixtures exists, it must stay in sync
/// with crate testdata (names). Skipped in a crates-only crane src.
#[test]
fn script_testdata_names_match_crate_when_present() {
    let script = script_testdata();
    if !script.is_dir() {
        return;
    }
    let mut crate_bad: Vec<String> = fs::read_dir(crate_testdata())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("bad-") && n.ends_with(".txt"))
        .collect();
    let mut script_bad: Vec<String> = fs::read_dir(&script)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("bad-") && n.ends_with(".txt"))
        .collect();
    crate_bad.sort();
    script_bad.sort();
    assert_eq!(crate_bad, script_bad);
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git_in(dir: &Path) -> Command {
    let mut c = Command::new("git");
    c.current_dir(dir);
    c.env("GIT_CONFIG_NOSYSTEM", "1");
    c.env("GIT_CONFIG_GLOBAL", dir.join(".gitconfig-empty"));
    c.env("GNUPGHOME", dir.join(".gnupg-empty"));
    c.env("HOME", dir);
    c.env("GIT_AUTHOR_NAME", "Detector");
    c.env("GIT_AUTHOR_EMAIL", "detector@example.test");
    c.env("GIT_COMMITTER_NAME", "Detector");
    c.env("GIT_COMMITTER_EMAIL", "detector@example.test");
    fs::create_dir_all(dir.join(".gnupg-empty")).ok();
    fs::write(
        dir.join(".gitconfig-empty"),
        "[user]\n\tname = Detector\n\temail = detector@example.test\n[commit]\n\tgpgsign = false\n[core]\n\thooksPath = /var/empty-surmount-private-data-hooks\n",
    )
    .ok();
    c
}

fn init_repo(dir: &Path) {
    assert!(
        git_in(dir)
            .args(["init", "-b", "main"])
            .status()
            .unwrap()
            .success()
    );
    fs::write(dir.join("README.md"), "lab\n").unwrap();
    assert!(
        git_in(dir)
            .args(["add", "README.md"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        git_in(dir)
            .args(["-c", "commit.gpgsign=false", "commit", "-m", "init"])
            .status()
            .unwrap()
            .success(),
        "commit init"
    );
}

#[test]
fn tree_mode_excludes_detector_fixtures() {
    if !git_available() {
        panic!("git is required for --tree contract tests");
    }
    let dir = temp_dir("tree-fix");
    init_repo(&dir);
    let fix = dir.join("script/testdata/private-data");
    fs::create_dir_all(&fix).unwrap();
    fs::write(
        fix.join("bad-pem.txt"),
        fs::read(crate_testdata().join("bad-pem.txt")).unwrap(),
    )
    .unwrap();
    assert!(
        git_in(&dir)
            .args(["add", "script/testdata/private-data/bad-pem.txt"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        git_in(&dir)
            .args(["-c", "commit.gpgsign=false", "commit", "-m", "fixture"])
            .status()
            .unwrap()
            .success()
    );
    let out = Command::new(bin())
        .arg("--tree")
        .current_dir(&dir)
        .output()
        .expect("tree");
    assert!(
        out.status.success(),
        "fixtures must be excluded: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn tree_mode_fails_on_tracked_pem_outside_fixtures() {
    if !git_available() {
        panic!("git is required for --tree contract tests");
    }
    let dir = temp_dir("tree-pem");
    init_repo(&dir);
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(
        dir.join("docs/oops.txt"),
        fs::read(crate_testdata().join("bad-pem.txt")).unwrap(),
    )
    .unwrap();
    assert!(
        git_in(&dir)
            .args(["add", "docs/oops.txt"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        git_in(&dir)
            .args(["-c", "commit.gpgsign=false", "commit", "-m", "oops"])
            .status()
            .unwrap()
            .success()
    );
    let out = Command::new(bin())
        .arg("--tree")
        .current_dir(&dir)
        .output()
        .expect("tree");
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("pem-or-openssh-private-key"), "{err}");
    assert!(!err.contains("FAKEPRIVATEKEYMATERIALNOTREAL"), "{err}");
}

#[test]
fn staged_mode_scans_index_blob_not_workdir() {
    if !git_available() {
        panic!("git is required for --staged contract tests");
    }
    let dir = temp_dir("staged");
    init_repo(&dir);
    let path = dir.join("note.txt");
    fs::write(&path, b"clean\n").unwrap();
    assert!(
        git_in(&dir)
            .args(["add", "note.txt"])
            .status()
            .unwrap()
            .success()
    );
    // Workdir is dirty with a fake token; index stays clean.
    fs::write(
        &path,
        fs::read(crate_testdata().join("bad-token-ghp.txt")).unwrap(),
    )
    .unwrap();
    let out = Command::new(bin())
        .arg("--staged")
        .current_dir(&dir)
        .output()
        .expect("staged");
    assert!(
        out.status.success(),
        "index blob is clean; workdir must not be scanned: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Now stage the token-shaped file: must fail.
    assert!(
        git_in(&dir)
            .args(["add", "note.txt"])
            .status()
            .unwrap()
            .success()
    );
    let out = Command::new(bin())
        .arg("--staged")
        .current_dir(&dir)
        .output()
        .expect("staged dirty");
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("api-token-shape"), "{err}");
    assert!(!err.contains("ghp_Fake"), "{err}");
}
