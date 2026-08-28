//! Hermetic Vandelay import contracts. Fake vandelay + stalwart-cli. No live host.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_surmount-mail-import-maildir")
}

fn temp_dir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "surmount-mail-import-{}-{}-{}",
        label,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_exec(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, body).unwrap();
    let mut perm = fs::metadata(&path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&path, perm).unwrap();
    path
}

const PLANT: &str = "SURMOUNT-TEST-STALWART-TOKEN-import-9f3c2a1b-do-not-log";

fn setup() -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let work = temp_dir("imp");
    let bin_dir = work.join("bin");
    let maildir = work.join("Maildir");
    fs::create_dir_all(bin_dir.join("empty")).unwrap();
    fs::create_dir_all(maildir.join("cur")).unwrap();
    fs::create_dir_all(maildir.join("new")).unwrap();
    fs::create_dir_all(maildir.join("tmp")).unwrap();
    let token = work.join("token");
    fs::write(&token, format!("{PLANT}\n")).unwrap();
    let cli_log = work.join("cli.log");
    let vlog = work.join("vandelay.log");
    let elog = work.join("env.log");
    write_exec(
        &bin_dir,
        "stalwart-cli",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>'{cli}'\n\
case \"$*\" in\n\
  *' import '*) echo \"error: unrecognized subcommand 'import'\" >&2; exit 2 ;;\n\
  *'query Account'*) printf '%s\\n' '{{\"name\":\"hunter\",\"id\":\"c\"}}'; exit 0 ;;\n\
esac\necho unexpected >&2; exit 1\n",
            cli = cli_log.display()
        ),
    );
    write_exec(
        &bin_dir,
        "vandelay",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >>'{vlog}'\n\
if [ -n \"$VANDELAY_TOKEN\" ]; then echo VANDELAY_TOKEN=set >>'{elog}'; else echo VANDELAY_TOKEN=missing >>'{elog}'; fi\n\
case \"$*\" in\n\
  *'{plant}'*) echo plant >&2; exit 1 ;;\n\
esac\nexit 0\n",
            vlog = vlog.display(),
            elog = elog.display(),
            plant = PLANT
        ),
    );
    (work, bin_dir, maildir, token, vlog)
}

fn run_driver(bin_dir: &Path, token: &Path, extra: &[&str]) -> std::process::Output {
    let mut c = Command::new(bin());
    c.env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
        .env("STALWART_URL", "http://127.0.0.1:8080")
        .env("STALWART_TOKEN_FILE", token)
        .env_remove("STALWART_TOKEN")
        .args(extra);
    c.output().unwrap()
}

#[test]
fn usage_without_args_exits_2() {
    let (work, bin_dir, _, token, _) = setup();
    let out = run_driver(&bin_dir, &token, &[]);
    assert_eq!(out.status.code(), Some(2));
    let s = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(s.contains("Usage:"));
    let _ = work;
}

#[test]
fn missing_maildir_exits_1() {
    let (work, bin_dir, _, token, _) = setup();
    let out = run_driver(
        &bin_dir,
        &token,
        &[
            "hunter@surmount.systems",
            &work.join("no-such").display().to_string(),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn missing_token_exits_2() {
    let (work, bin_dir, maildir, _, _) = setup();
    let missing = work.join("missing-token");
    let mut c = Command::new(bin());
    c.env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
        .env("STALWART_URL", "http://127.0.0.1:8080")
        .env("STALWART_TOKEN_FILE", &missing)
        .env_remove("STALWART_TOKEN")
        .args(["hunter@surmount.systems"])
        .arg(&maildir);
    let out = c.output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(s.to_lowercase().contains("token"));
}

#[test]
fn missing_vandelay_exits_2() {
    let (work, _, maildir, token, _) = setup();
    let empty = work.join("empty-bin");
    fs::create_dir_all(&empty).unwrap();
    let mut c = Command::new(bin());
    c.env("PATH", format!("{}:/usr/bin:/bin", empty.display()))
        .env("STALWART_URL", "http://127.0.0.1:8080")
        .env("STALWART_TOKEN_FILE", &token)
        .env_remove("STALWART_TOKEN")
        .args(["hunter@surmount.systems"])
        .arg(&maildir);
    let out = c.output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(s.to_lowercase().contains("vandelay"));
}

#[test]
fn happy_path_vandelay_no_cli_import() {
    let (work, bin_dir, maildir, token, vlog) = setup();
    let archive = work.join("archives/hunter.sqlite");
    let mut c = Command::new(bin());
    c.env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
        .env("STALWART_URL", "http://127.0.0.1:8080")
        .env("STALWART_TOKEN_FILE", &token)
        .env("SURMOUNT_MAIL_IMPORT_ARCHIVE", &archive)
        .env_remove("STALWART_TOKEN")
        .args(["hunter@surmount.systems"])
        .arg(&maildir);
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v = fs::read_to_string(&vlog).unwrap();
    assert!(v.contains("import maildir"));
    assert!(v.contains("--account-id c"));
    assert!(v.contains("--auth-bearer"));
    assert!(v.contains("All Mail"));
    assert!(!v.contains("import messages"));
    assert!(!v.contains(PLANT));
}

#[test]
fn dry_run_forwarded() {
    let (work, bin_dir, maildir, token, vlog) = setup();
    let archive = work.join("a.sqlite");
    let mut c = Command::new(bin());
    c.env("PATH", format!("{}:/usr/bin:/bin", bin_dir.display()))
        .env("STALWART_URL", "http://127.0.0.1:8080")
        .env("STALWART_TOKEN_FILE", &token)
        .env("SURMOUNT_MAIL_IMPORT_ARCHIVE", &archive)
        .env_remove("STALWART_TOKEN")
        .args(["--dry-run", "hunter@surmount.systems"])
        .arg(&maildir);
    let out = c.output().unwrap();
    assert!(out.status.success());
    let v = fs::read_to_string(&vlog).unwrap();
    assert!(v.contains("--dry-run"));
}
