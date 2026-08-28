//! Hermetic CLI contracts. Synthetic df only. No live SSH. Never embeds host SKUs.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-niced-builder")
}

fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!(
        "surmount-niced-builder-{label}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn fake_df(dir: &std::path::Path, pct: u32) -> PathBuf {
    let path = dir.join(format!("fake-df-{pct}"));
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' 'Filesystem     1024-blocks      Used Available Capacity Mounted on'\nprintf '%s\\n' '/dev/dummy        1000000     000000     00000      {pct}% /'\n"
    );
    fs::write(&path, body).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn fake_ionice(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("ionice");
    fs::write(&path, "#!/bin/sh\nwhile [ \"$1\" = \"-c3\" ] || [ \"$1\" = \"-c\" ]; do\n  if [ \"$1\" = \"-c\" ]; then shift; fi\n  shift\ndone\nexec \"$@\"\n").unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

fn run_with_df(df: &std::path::Path, args: &[&str]) -> std::process::Output {
    let bindir = df.parent().unwrap().join("bin");
    fs::create_dir_all(&bindir).unwrap();
    let _ = fake_ionice(&bindir);
    let true_bin = bindir.join("true");
    fs::write(&true_bin, "#!/bin/sh\nexit 0\n").unwrap();
    let mut tp = fs::metadata(&true_bin).unwrap().permissions();
    tp.set_mode(0o755);
    fs::set_permissions(&true_bin, tp).unwrap();
    let path = format!(
        "{}:{}",
        bindir.display(),
        std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into())
    );
    Command::new(bin())
        .args(args)
        .env("PATH", path)
        .env("SURMOUNT_BUILDER_USE_SYSTEMD_RUN", "0")
        .env("SURMOUNT_BUILDER_DF", df)
        .env("SURMOUNT_BUILDER_DISK_GUARD_PERCENT", "95")
        .env("SURMOUNT_BUILDER_DISK_GUARD_PATH", "/")
        .output()
        .expect("run niced-builder")
}

#[test]
fn help_exits_0_and_names_memory_nice_disk() {
    let out = Command::new(bin()).arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("MemoryMax"));
    assert!(text.to_ascii_lowercase().contains("nice"));
    assert!(
        text.to_ascii_lowercase().contains("disk")
            || text.contains("95")
            || text.to_ascii_lowercase().contains("storage")
    );
    assert!(!text.to_ascii_lowercase().contains("lake"));
}

#[test]
fn missing_command_fails_loud() {
    let dir = temp_dir("missing");
    let df = fake_df(&dir, 10);
    let out = run_with_df(&df, &[]);
    assert!(!out.status.success());
    let err = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    )
    .to_ascii_lowercase();
    assert!(
        err.contains("command") || err.contains("usage") || err.contains("required"),
        "{err}"
    );
}

#[test]
fn disk_at_95_refuses() {
    let dir = temp_dir("95");
    let df = fake_df(&dir, 95);
    let out = run_with_df(&df, &["--", "true"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr).to_ascii_lowercase();
    assert!(err.contains("95"));
    assert!(err.contains("refuse") || err.contains("full") || err.contains("guard"));
}

#[test]
fn disk_at_96_refuses() {
    let dir = temp_dir("96");
    let df = fake_df(&dir, 96);
    let out = run_with_df(&df, &["--", "/bin/true"]);
    assert!(!out.status.success());
}

#[test]
fn disk_at_94_runs_true() {
    let dir = temp_dir("94");
    let df = fake_df(&dir, 94);
    let out = run_with_df(&df, &["--", "true"]);
    assert!(
        out.status.success(),
        "stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}
