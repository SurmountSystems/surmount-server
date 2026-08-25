//! CLI contracts matching the former bash leftover-home scanner harness.

use std::path::PathBuf;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-leftover-homes")
}

fn testdata() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata")
}

fn run_paths(paths: &[&std::path::Path]) -> std::process::Output {
    let mut cmd = Command::new(bin());
    cmd.arg("--paths");
    for p in paths {
        cmd.arg(p);
    }
    cmd.output().expect("run leftover-homes")
}

#[test]
fn help_exits_0() {
    let out = Command::new(bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
}

#[test]
fn good_mktemp_exits_0() {
    let p = testdata().join("good-mktemp.sh");
    let out = run_paths(&[&p]);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bad_mkdir_root_exits_1() {
    let p = testdata().join("bad-mkdir-root.sh");
    let out = run_paths(&[&p]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("root-relative-leftover-home"), "{err}");
}

#[test]
fn bad_mkdir_relative_exits_1() {
    let p = testdata().join("bad-mkdir-relative.sh");
    let out = run_paths(&[&p]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("relative-mkdir-leftover-home"), "{err}");
}

#[test]
fn tree_exits_0_in_this_repo() {
    let out = Command::new(bin())
        .arg("--tree")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
